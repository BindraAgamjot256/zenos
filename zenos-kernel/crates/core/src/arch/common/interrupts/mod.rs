pub mod controller;
pub use controller::IrqError;
use kprimitives::alloc::KernelObject;

use crate::arch::CpuContext;

//todo: doc this:

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InterruptHandlerRet {
    #[default]
    Handled,
    NotMine,
    Errored,
}

pub trait InterruptHandler: KernelObject + Send + Sync {
    fn handle(&self, ctx: &mut CpuContext) -> InterruptHandlerRet;
}