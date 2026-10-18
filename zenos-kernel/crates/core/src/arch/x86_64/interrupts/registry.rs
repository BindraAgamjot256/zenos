//! Runtime registry for interrupt and exception handlers.
//!
//! The registry is the bridge between the assembly interrupt stubs and the
//! Rust callbacks that process them. It stores the currently installed handler
//! for each vector, provides a default fallback for unconfigured entries, and
//! offers a small RAII guard for temporary overrides during debugging or
//! bring-up work.

use crate::arch::{
    SpinLock,
    common::interrupts::{InterruptHandler, InterruptHandlerRet},
    x86_64::interrupts::ctx::CpuContext as InterruptContext,
};
use core::{ops::Deref, sync::atomic::Ordering};
use kprimitives::{
    alloc::{CreatableKernelObject, GlobalAllocator, KernelObject, boxed::KBox},
    rwlock::RwLock,
};

type HandlerType = KBox<dyn InterruptHandler, GlobalAllocator>;

enum Handlers {
    Unset,
    Locked(HandlerType),
    Shared([Option<HandlerType>; 10]), // arbitrarily chose 10, seems good enough.
}

pub(super) struct InterruptRegistry {
    handlers: SpinLock<[Handlers; 256]>,
}

impl InterruptRegistry {
    pub const fn new() -> Self {
        Self {
            handlers: SpinLock::new([const { Handlers::Unset }; 256]),
        }
    }

    pub fn register(
        &self,
        handler: KBox<dyn InterruptHandler, GlobalAllocator>,
        number: u8,
    ) -> Result<u8, &'static str> {
        let mut lock = self.handlers.lock();
        let Some(block) = lock.get_mut(number as usize) else {
            return Err("out of bounds");
        };

        match block {
            Handlers::Shared(inner) => {
                let mut written = false;
                let mut ret = u8::MAX;
                for (index, block) in inner.iter_mut().enumerate() {
                    if block.is_some() {
                        continue;
                    }

                    written = true;
                    block.replace(handler);
                    ret = index as u8;
                    break;
                }

                if written {
                    Ok(ret)
                } else {
                    Err("Out of space")
                }
            }
            Handlers::Unset => {
                let mut arr = [const { None }; 10];
                arr[0] = Some(handler);
                *block = Handlers::Shared(arr);
                Ok(0)
            }
            _ => Err("Invalid input"),
        }
    }

    fn locked(
        &self,
        handler: KBox<dyn InterruptHandler, GlobalAllocator>,
        number: u8,
    ) -> Result<(), &'static str> {
        let mut lock = self.handlers.lock();
        let Some(block) = lock.get_mut(number as usize) else {
            return Err("Fuck you");
        };

        match block {
            Handlers::Unset => *block = Handlers::Locked(handler),
            _ => return Err("locked"),
        }

        Ok(())
    }

    fn clear(&self, number: u8, id: u8) -> Result<(), &'static str> {
        let mut lock = self.handlers.lock();
        let Some(block) = lock.get_mut(number as usize) else {
            return Err("Out of bounds you idiot");
        };

        match block {
            Handlers::Locked(_) => {
                *block = Handlers::Unset;
            }
            Handlers::Shared(t) => {
                let Some(id) = t.get_mut(id as usize) else {
                    return Err("Out of bounds you idiot #2");
                };

                if id.is_none() {
                    return Err("why? just why?");
                };

                drop(id.take().unwrap());

                if !t.iter().any(|t| t.is_some()) {
                    *block = Handlers::Unset
                }
            }
            Handlers::Unset => {
                return Err("Fuck you");
            }
        }

        Ok(())
    }

    pub fn with_handler(
        &self,
        number: u8,
        mut f: impl FnMut(&dyn InterruptHandler) -> InterruptHandlerRet,
    ) -> Option<InterruptHandlerRet> {
        let lock = self.handlers.lock();
        let block = lock.get(number as usize)?;

        match block {
            Handlers::Locked(handler) => Some(f(handler.as_ref())),
            Handlers::Shared(list) => {
                for handler in list.iter().flatten() {
                    let ret = f(handler.as_ref());
                    if ret == InterruptHandlerRet::Errored {
                        return Some(InterruptHandlerRet::Errored);
                    } else if ret == InterruptHandlerRet::Handled {
                        return Some(InterruptHandlerRet::Handled);
                    }
                }
                Some(InterruptHandlerRet::NotMine)
            }
            Handlers::Unset => None,
        }
    }
}

pub(super) static INTERRUPT_REGISTRY: InterruptRegistry = InterruptRegistry::new();

struct FnHandler {
    ptr: fn(&mut InterruptContext) -> InterruptHandlerRet,
}
impl KernelObject for FnHandler {}
impl CreatableKernelObject for FnHandler {
    type Allocator = GlobalAllocator;
}
impl InterruptHandler for FnHandler {
    fn handle(&self, ctx: &mut InterruptContext) -> InterruptHandlerRet {
        let ptr = self.ptr;
        ptr(ctx)
    }
}

/// Registers a handler and returns a guard that restores the default handler on drop.
pub fn register_interrupt_handler(
    interrupt_number: u8,
    handler: fn(&mut InterruptContext) -> InterruptHandlerRet,
) -> InterruptGuard {
    log::info!("registering handler");
    let no = INTERRUPT_REGISTRY
        .register(
            KBox::new(FnHandler { ptr: handler }).unwrap(),
            interrupt_number,
        )
        .expect("Register sucess.");
    InterruptGuard(interrupt_number, no)
}

/// Clears a handler by restoring the default panic handler for the given vector.
pub fn deregister_interrupt_handler(interrupt_number: u8, id: u8) {
    INTERRUPT_REGISTRY
        .clear(interrupt_number, id)
        .expect("clear sucess");
}

/// Registers a handler without returning a guard object.
pub fn register_guardless(
    interrupt_number: u8,
    handler: fn(&mut InterruptContext) -> InterruptHandlerRet,
) {
    INTERRUPT_REGISTRY
        .locked(
            KBox::new(FnHandler { ptr: handler }).unwrap(),
            interrupt_number,
        )
        .expect("Register success.")
}

/// A small RAII guard that restores the default handler when it goes out of scope.
pub struct InterruptGuard(u8, u8);

impl Drop for InterruptGuard {
    fn drop(&mut self) {
        deregister_interrupt_handler(self.0, self.1);
    }
}
