//! PS/2 Keyboard input handling.
//!
//! This module provides a keyboard input handler that processes scancodes
//! from the PS/2 keyboard controller and buffers them for userspace reads.
//!
//! # Overview
//!
//! The keyboard subsystem consists of:
//!
//! - **Interrupt Handler**: Called from the IDT on IRQ1 (vector 0x21)
//! - **Scancode Decoder**: Converts raw scancodes to characters using [`pc_keyboard`]
//! - **Input Buffer**: VecDeque for buffering typed characters
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────┐
//! │  PS/2 Controller │
//! │   (Port 0x60)    │
//! └────────┬─────────┘
//!          │ IRQ1
//!          ▼
//! ┌──────────────────┐
//! │  IDT Handler     │
//! │  (Vector 0x21)   │
//! └────────┬─────────┘
//!          │ scancode
//!          ▼
//! ┌──────────────────┐
//! │  KeyboardInput   │
//! │  - pc_keyboard   │──────► Character decoding (US 104-key layout)
//! │  - VecDeque buf  │──────► Buffered input for read syscalls
//! └────────┬─────────┘
//!          │
//!          ▼
//!     TTY / Process
//! ```
//!
//! # Scancode Sets
//!
//! This implementation uses Scancode Set 1 (the default for most PC keyboards).
//! Make codes indicate key press, break codes indicate key release.
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::hardware::keyboard::{joint_keyboard_handler, read_exact};
//!
//! // Called from interrupt handler:
//! joint_keyboard_handler(scancode);
//!
//! // Blocking read from userspace:
//! let mut buf = [0u8; 64];
//! let n = read_exact(&mut buf);  // Returns on newline
//! ```

use crate::kprint;
use crate::process::block_current_process;
use crate::process::file_handles::STDIN_BLOCKED;
use crate::tty::TtyInputBackend;
use alloc::collections::VecDeque;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering::SeqCst;
use log::trace;
use pc_keyboard::{DecodedKey, HandleControl, KeyCode, Keyboard, ScancodeSet1, layouts};
use spin::{Lazy, Mutex};
use x86_64::instructions::interrupts::without_interrupts;

/// PS/2 keyboard input handler with character buffering.
///
/// Processes raw scancodes from the keyboard controller, decodes them
/// into characters using a US 104-key layout, and buffers them for
/// reading by userspace processes.
///
/// # Key Features
///
/// - **US 104-Key Layout**: Standard QWERTY keyboard mapping
/// - **Scancode Set 1**: PC/AT compatible scancode interpretation
/// - **Buffered Input**: Characters queued until read by process
/// - **TTY Integration**: Implements [`TtyInputBackend`] for TTY subsystem
pub struct KeyboardInput {
    /// Scancode decoder with US layout.
    keyboard: Keyboard<layouts::Us104Key, ScancodeSet1>,
    /// Buffer of decoded characters waiting to be read.
    buffer: VecDeque<u8>,
}

impl KeyboardInput {
    /// Create a new keyboard input handler.
    ///
    /// Initializes the scancode decoder with:
    /// - Scancode Set 1 (PC/AT compatible)
    /// - US 104-key layout
    /// - Control key handling disabled (passed through as-is)
    pub fn new() -> Self {
        Self {
            keyboard: Keyboard::new(
                ScancodeSet1::new(),
                layouts::Us104Key,
                HandleControl::Ignore,
            ),
            buffer: VecDeque::new(),
        }
    }

    /// Process a scancode from the keyboard interrupt.
    ///
    /// Decodes the scancode, echoes printable characters to the TTY,
    /// and buffers them for later reading. Wakes any process blocked
    /// on stdin when new input is available.
    ///
    /// # Arguments
    ///
    /// * `scancode` - Raw scancode byte from I/O port 0x60
    pub fn handle_scancode(&mut self, scancode: u8) {
        if let Ok(Some(key_event)) = self.keyboard.add_byte(scancode)
            && let Some(key) = self.keyboard.process_keyevent(key_event)
        {
            without_interrupts(|| {
                match key {
                    DecodedKey::Unicode(character) => {
                        kprint!("{}", character);
                        self.buffer.push_back(character as u8);
                        // Wake any process blocked on stdin
                        STDIN_BLOCKED.store(false, SeqCst);
                    }
                    DecodedKey::RawKey(key) => Self::raw_key_handler(key),
                }
            })
        }
    }

    /// Pop the next character from the input buffer.
    ///
    /// Returns `None` if the buffer is empty.
    pub fn pop(&mut self) -> Option<u8> {
        self.buffer.pop_front()
    }

    /// Handle non-printable keys (arrows, modifiers, etc.).
    fn raw_key_handler(key: KeyCode) {
        match key {
            KeyCode::LShift | KeyCode::RShift => {}
            KeyCode::ArrowDown | KeyCode::ArrowUp => {
                trace!("Arrow Up/Down pressed... ignoring")
            }
            KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                trace!("Arrow Left/Right pressed... ignoring again...")
            }
            _ => {}
        }
    }
}

impl TtyInputBackend for KeyboardInput {
    fn read_byte(&mut self) -> Option<u8> {
        self.pop()
    }
}

/// Global keyboard input instance.
///
/// Protected by a spinlock for interrupt-safe access.
pub(crate) static KEYBOARD_INPUT: Lazy<Mutex<KeyboardInput>> =
    Lazy::new(|| Mutex::new(KeyboardInput::new()));

/// Keyboard interrupt handler entry point.
///
/// Called from the IDT handler for IRQ1 (vector 0x21).
/// Forwards the scancode to the global [`KeyboardInput`] instance.
///
/// # Arguments
///
/// * `scancode` - Raw scancode byte read from I/O port 0x60
pub fn joint_keyboard_handler(scancode: u8) {
    KEYBOARD_INPUT.lock().handle_scancode(scancode);
}

/// Blocking read from keyboard into buffer.
///
/// Reads characters from the keyboard buffer until:
/// - A newline (`\n`) is encountered (not included in output)
/// - The buffer is full
///
/// # Arguments
///
/// * `buf` - Destination buffer for input
///
/// # Returns
///
/// Number of bytes read (excluding the newline).
///
/// # Blocking Behavior
///
/// This function blocks the calling process when the keyboard buffer
/// is empty, yielding to the scheduler until new input arrives.
///
/// # Special Characters
///
/// - `\n` (newline): Terminates input, not stored in buffer
/// - `\r` (carriage return): Ignored
/// - `\x08` (backspace): Removes last character from buffer
pub fn read_exact(buf: &mut [u8]) -> usize {
    static PROCESSING: AtomicBool = AtomicBool::new(false);
    let mut count = 0;
    while count < buf.len() {
        PROCESSING.store(true, SeqCst);
        let byte = loop {
            {
                let mut kb = KEYBOARD_INPUT.lock();
                if let Some(b) = kb.pop() {
                    break Some(b);
                }
            }
            block_current_process(&PROCESSING);
            core::hint::spin_loop();
        };

        if let Some(b) = byte {
            if b == b'\n' {
                PROCESSING.store(false, SeqCst);
                break;
            } else if b == b'\r' {
                continue;
            } else if b == b'\x08' {
                if count > 0 {
                    count -= 1;
                }
            } else {
                buf[count] = b;
                count += 1;
            }
        }
    }
    count
}
