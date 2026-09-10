use crate::{
    firmware::{Polarity, Trigger},
    mm::GlobalAllocator,
};
use core::time::Duration;
use kprimitives::alloc::{KernelObject, boxed::KBox};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IrqError {
    OutOfRange,
}

pub trait InterruptController: KernelObject {
    /// Maps a raw hardware line to an system vector and unmasks it.
    fn enable(
        &self,
        number: u32,
        trigger: Trigger,
        polarity: Polarity,
        irq: u32,
    ) -> Result<(), IrqError>;
    /// Disables the interrupt line and returns the previous IRQ value.
    fn disable(&self, number: u32) -> u32;
    /// Masks the interrupt line, preventing it from triggering. It does not disable the line.
    fn mask(&self, number: u32);
    /// Unmasks the interrupt line, allowing it to trigger.
    fn unmask(&self, number: u32);
    /// Sends an End of Interrupt signal to the hardware.
    fn send_eoi(&self);
    /// Returns a timer object that can be used to schedule interrupts. (Timer is programmed for 1-shot mode only.)
    fn timer(&self) -> KBox<dyn Clockevent, GlobalAllocator>;
}

pub trait Clockevent: KernelObject {
    fn next_tick(&self, then: Duration);
    fn toggle(&self);
    fn set_vector(&self, vector: u8);
}
