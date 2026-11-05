pub mod controller;
use crate::alloc::KernelObject;
use crate::arch::CpuContext;
pub use controller::IrqError;

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
