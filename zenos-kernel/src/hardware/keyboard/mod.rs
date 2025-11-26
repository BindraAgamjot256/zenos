use crate::kprint;
use heapless::Deque;
use log::trace;
use pc_keyboard::{DecodedKey, HandleControl, KeyCode, Keyboard, ScancodeSet1, layouts};
use spin::{Lazy, Mutex};

static KEYBOARD: Lazy<Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>>> = Lazy::new(|| {
    Mutex::new(Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    ))
});

static KEYBUF: Lazy<Mutex<Deque<u8, 256>>> = Lazy::new(|| Mutex::new(Deque::new()));

pub fn joint_keyboard_handler(scancode: u8) {
    let mut keyboard = KEYBOARD.lock();
    if let Ok(Some(key_event)) = keyboard.add_byte(scancode)
        && let Some(key) = keyboard.process_keyevent(key_event)
    {
        match key {
            DecodedKey::Unicode(character) => {
                // kprint!("{}", character);
                let mut buf = KEYBUF.lock();
                let _ = buf.push_back(character as u8);
            }
            DecodedKey::RawKey(key) => raw_key_handler(key),
        }
    }
}

/// Reads up to `buf.len()` bytes from the keyboard buffer into `buf`.
/// Returns the number of bytes read.
pub fn read_into(buf: &mut [u8]) -> usize {
    let mut kb = KEYBUF.lock();
    let mut count = 0;
    while kb.len() == 0 {
        core::hint::spin_loop() /* fixme: preempt here...*/
    } // actually I should preempt every call of spin_loop.
    while count < buf.len() {
        if let Some(b) = kb.pop_front() {
            buf[count] = b;
            count += 1;
            if b == b'\n' {
                break;
            }
        } else {
            break;
        }
    }
    count
}

fn raw_key_handler(key: KeyCode) {
    // we use separate handlers for arrow keys and other keys, since arrow keys are
    match key {
        KeyCode::LShift | KeyCode::RShift => {}
        KeyCode::ArrowDown | KeyCode::ArrowUp => {
            trace!("Arrow Up/Down pressed... ignoring")
        }
        KeyCode::ArrowLeft | KeyCode::ArrowRight => {
            trace!("Arrow Left/Right pressed... ignoring again...")
        }
        _ => kprint!("{:#?}", key),
    }
}
