use crate::kprint;
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

pub fn joint_keyboard_handler(scancode: u8) {
    let mut keyboard = KEYBOARD.lock();
    if let Ok(Some(key_event)) = keyboard.add_byte(scancode)
        && let Some(key) = keyboard.process_keyevent(key_event)
    {
        match key {
            DecodedKey::Unicode(character) => kprint!("{}", character),
            DecodedKey::RawKey(key) => raw_key_handler(key),
        }
    }
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
        _ => kprint!("{:#?}", key),
    }
}
