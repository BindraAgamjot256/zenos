use crate::{arch::CpuContext, mm::GlobalAllocator};
#[cfg(debug_assertions)]
use alloc::{boxed::Box, format};
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use kprimitives::alloc::{CreatableKernelObject, KernelObject, boxed::KBox, sync::KArc};
use scheduler::{Thread as SchedulerThread, ThreadState, Tid};

pub type ThreadRef = KArc<Thread, GlobalAllocator>;

#[derive(Debug)]
pub struct Thread {
    tid: Tid,
    context: core::cell::UnsafeCell<CpuContext>,
    state: AtomicU8,
    refs: AtomicUsize,
    #[cfg(debug_assertions)]
    owner: Option<&'static str>,
}

impl Thread {
    pub fn new_current(
        tid: Tid,
    ) -> Result<KArc<Self, GlobalAllocator>, kprimitives::alloc::AllocationError> {
        Ok(KBox::new(Self {
            tid,
            context: core::cell::UnsafeCell::new(CpuContext::default()),
            state: AtomicU8::new(ThreadState::Running as u8),
            refs: AtomicUsize::new(1),
            #[cfg(debug_assertions)]
            owner: Some(kernel_thread_owner(tid)),
        })?
        .into_arc())
    }

    pub fn new_kernel(
        tid: Tid,
        entry: fn(),
        stack_ptr: *mut u8,
    ) -> Result<KArc<Self, GlobalAllocator>, kprimitives::alloc::AllocationError> {
        Ok(KBox::new(Self {
            tid,
            context: core::cell::UnsafeCell::new(CpuContext::new_from_fn_kernel(entry, stack_ptr)),
            state: AtomicU8::new(ThreadState::Ready as u8),
            refs: AtomicUsize::new(1),
            #[cfg(debug_assertions)]
            owner: Some(kernel_thread_owner(tid)),
        })?
        .into_arc())
    }

    pub fn new_user(
        tid: Tid,
        context: CpuContext,
    ) -> Result<KArc<Self, GlobalAllocator>, kprimitives::alloc::AllocationError> {
        Ok(KBox::new(Self {
            tid,
            context: core::cell::UnsafeCell::new(context),
            state: AtomicU8::new(ThreadState::Ready as u8),
            refs: AtomicUsize::new(1),
            #[cfg(debug_assertions)]
            owner: None,
        })?
        .into_arc())
    }

    /// # Safety
    ///
    /// Only called by the scheduler, in interrupt context.
    pub(crate) unsafe fn update_context(&self, context: &CpuContext) {
        unsafe {
            *self.context.get() = context.clone();
        }
    }

    pub fn context(&self) -> &CpuContext {
        unsafe { &*self.context.get() }
    }
}

// The context is only accessed while scheduler/interrupt access is serialized.
unsafe impl Sync for Thread {}

#[cfg(debug_assertions)]
fn kernel_thread_owner(tid: Tid) -> &'static str {
    Box::leak(format!("Kernel Thread #{}", tid.get()).into_boxed_str())
}

impl KernelObject for Thread {}

impl CreatableKernelObject for Thread {
    type Allocator = GlobalAllocator;
}

unsafe impl kprimitives::alloc::RefCounted for Thread {
    fn ref_count(&self) -> usize {
        self.refs.load(Ordering::Relaxed)
    }

    fn inc_refcount(&self, ordering: Ordering) -> usize {
        self.refs.fetch_add(1, ordering) + 1
    }

    fn dec_refcount(&self, ordering: Ordering) -> usize {
        self.refs.fetch_sub(1, ordering) - 1
    }
}

impl SchedulerThread for Thread {
    fn tid(&self) -> Tid {
        self.tid
    }

    fn state(&self) -> ThreadState {
        match self.state.load(Ordering::Acquire) {
            value if value == ThreadState::Running as u8 => ThreadState::Running,
            value if value == ThreadState::Ready as u8 => ThreadState::Ready,
            value if value == ThreadState::Sleeping as u8 => ThreadState::Sleeping,
            _ => unreachable!("invalid thread state"),
        }
    }

    fn set_state(&self, state: ThreadState) {
        self.state.store(state as u8, Ordering::Release);
    }

    #[cfg(debug_assertions)]
    fn owner(&self) -> Option<&'static str> {
        self.owner
    }
}
