//! Serial port driver for x86_64 architecture.
//!
//! This module provides a driver for 16550-compatible UART serial ports, which are
//! commonly used for early-stage debugging output and kernel console communication.
//!
//! # Hardware Details
//! The driver communicates with serial ports using I/O port access. It initializes
//! the UART with standard settings (38400 baud, 8 bits, no parity) and provides
//! `core::fmt::Write` support for convenient debug output.

use crate::arch::x86_64::ports::{ReadOnlyPort, WriteOnlyPort};
use core::fmt;

/// A 16550-compatible UART serial port driver.
///
/// This struct represents a single serial port and provides methods for
/// initialization and output. The port operates at 38400 baud with 8-bit
/// characters, no parity, and one stop bit.
///
/// # Example
/// ```no_run
/// use arch::serial::SerialPort;
///
/// let serial = SerialPort::new(0x3f8); // COM1 port
/// unsafe {
///     serial.init();
///     serial.write_byte(b'A');
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct SerialPort {
    port: u16,
}

impl SerialPort {
    /// Creates a new `SerialPort` for the specified I/O port.
    ///
    /// This only creates the serial port structure; you must call `init()`
    /// to actually initialize the UART hardware.
    ///
    /// # Arguments
    /// * `port` - The base I/O port number (e.g., 0x3f8 for COM1, 0x2f8 for COM2)
    ///
    /// # Example
    /// ```
    /// let serial = SerialPort::new(0x3f8);
    /// ```
    pub const fn new(port: u16) -> Self {
        Self { port }
    }

    /// Initializes the UART hardware.
    ///
    /// This method configures the UART with the following settings:
    /// - Baud rate: 38400 (divisor = 3)
    /// - Character size: 8 bits
    /// - Parity: None
    /// - Stop bits: 1
    /// - FIFO: Enabled with 14-byte threshold
    /// - RTS/DSR: Enabled
    ///
    /// # Safety
    /// The caller must ensure that the port number is correct and safe to access.
    /// This method performs multiple I/O port writes that can affect hardware state.
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

    /// Checks if the transmitter is ready to accept another byte.
    ///
    /// Reads the line status register to determine if the transmit holding register is empty.
    ///
    /// # Returns
    /// `true` if the transmitter is empty and ready for more data, `false` otherwise
    fn is_transmit_empty(&self) -> bool {
        unsafe {
            let line_status = ReadOnlyPort::<u8>::new(self.port + 5);
            (line_status.read() & 0x20) != 0
        }
    }

    #[cfg(feature = "__test_timer")]
    fn is_receive_ready(&self) -> bool {
        unsafe {
            let line_status = ReadOnlyPort::<u8>::new(self.port + 5);
            (line_status.read() & 0x01) != 0
        }
    }

    /// Reads a single byte from the serial port.
    ///
    /// This method blocks until a byte is available.
    #[cfg(feature = "__test_timer")]
    pub fn read_byte(&self) -> u8 {
        unsafe {
            let data = ReadOnlyPort::<u8>::new(self.port);

            while !self.is_receive_ready() {}

            data.read()
        }
    }

    /// Writes a single byte to the serial port.
    ///
    /// This method blocks until the transmitter is empty before writing the byte.
    /// It should only be called after `init()` has been called.
    ///
    /// # Arguments
    /// * `byte` - The byte value to transmit
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
