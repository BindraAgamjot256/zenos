use crate::arch::ports::{ReadOnlyPort, WriteOnlyPort};
use core::fmt;

#[derive(Debug, Clone, Copy)]
pub struct SerialPort {
    port: u16,
}

impl SerialPort {
    pub const fn new(port: u16) -> Self {
        Self { port }
    }

    pub unsafe fn init(&self) {
        unsafe {
            let interrupt = WriteOnlyPort::<u8>::new(self.port + 1);
            let data = WriteOnlyPort::<u8>::new(self.port);
            let fifo = WriteOnlyPort::<u8>::new(self.port + 2);
            let line_ctrl = WriteOnlyPort::<u8>::new(self.port + 3);
            let modem = WriteOnlyPort::<u8>::new(self.port + 4);

            // Disable interrupts
            interrupt.write(0x00);

            // Enable DLAB (set baud rate divisor)
            line_ctrl.write(0x80);

            // Set divisor to 3 (lo byte) 38400 baud
            data.write(0x03);
            // Set divisor to 0 (hi byte)
            interrupt.write(0x00);

            // 8 bits, no parity, one stop bit
            line_ctrl.write(0x03);

            // Enable FIFO, clear them, 14-byte threshold
            fifo.write(0xC7);

            // IRQs enabled, RTS/DSR set
            modem.write(0x0B);
        }
    }

    fn is_transmit_empty(&self) -> bool {
        unsafe {
            let line_status = ReadOnlyPort::<u8>::new(self.port + 5);
            (line_status.read() & 0x20) != 0
        }
    }

    pub fn write_byte(&self, byte: u8) {
        unsafe {
            let data = WriteOnlyPort::<u8>::new(self.port);

            // Wait until transmitter is ready
            while !self.is_transmit_empty() {}

            data.write(byte);
        }
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.write_byte(byte);
        }
        Ok(())
    }
}
