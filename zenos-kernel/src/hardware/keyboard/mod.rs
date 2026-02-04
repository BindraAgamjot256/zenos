use crate::kprint;
use crate::primitives::RingBuf;
use crate::process::block_current_process;
use crate::process::file_handles::STDIN_BLOCKED;
use crate::tty::TtyInputBackend;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering::SeqCst;
use log::trace;
use pc_keyboard::{DecodedKey, HandleControl, KeyCode, Keyboard, ScancodeSet1, layouts};
use spin::{Lazy, Mutex};

const KEYBUF_SIZE: usize = 256;

/// Keyboard input handler with a ring buffer for storing scancodes.
pub struct KeyboardInput {
    keyboard: Keyboard<layouts::Us104Key, ScancodeSet1>,
    buffer: RingBuf<u8, KEYBUF_SIZE>,
}

impl KeyboardInput {
    pub const fn new() -> Self {
        Self {
            keyboard: Keyboard::new(
                ScancodeSet1::new(),
                layouts::Us104Key,
                HandleControl::Ignore,
            ),
            buffer: RingBuf::new(),
        }
    }

    /// Handles a scancode from the keyboard interrupt.
    pub fn handle_scancode(&mut self, scancode: u8) {
        if let Ok(Some(key_event)) = self.keyboard.add_byte(scancode)
            && let Some(key) = self.keyboard.process_keyevent(key_event)
        {
            match key {
                DecodedKey::Unicode(character) => {
                    kprint!("{}", character);
                    self.buffer.push_back(character as u8);
                    // Wake any process blocked on stdin
                    STDIN_BLOCKED.store(false, SeqCst);
                }
                DecodedKey::RawKey(key) => Self::raw_key_handler(key),
            }
        }
    }

    /// Pops the next byte from the buffer.
    pub fn pop(&mut self) -> Option<u8> {
        self.buffer.pop_front()
    }

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
        self.pop().map(|b| b)
    }
}

pub(crate) static KEYBOARD_INPUT: Lazy<Mutex<KeyboardInput>> =
    Lazy::new(|| Mutex::new(KeyboardInput::new()));

pub fn joint_keyboard_handler(scancode: u8) {
    KEYBOARD_INPUT.lock().handle_scancode(scancode);
}

/// Reads bytes from the keyboard buffer into `buf` until newline or buffer full.
/// Returns the number of bytes read.
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
