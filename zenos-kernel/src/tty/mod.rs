use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt::Write;
use hashbrown::HashMap;
use spin::{Lazy, Mutex};

/// Tracks current cursor position for text rendering in character coordinates.
struct Cursor {
    x: usize,
    y: usize,
}

impl Cursor {
    fn new() -> Self {
        Cursor { x: 0, y: 0 }
    }
}

pub struct Style {
    pub bold: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub ital: bool,
    pub fg_color: Option<(u8, u8, u8)>,
    pub bg_color: Option<(u8, u8, u8)>,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            bold: false,
            underline: false,
            strikethrough: false,
            ital: false,
            fg_color: None,
            bg_color: None,
        }
    }
}

impl Clone for Style {
    fn clone(&self) -> Self {
        Self {
            bold: self.bold,
            underline: self.underline,
            strikethrough: self.strikethrough,
            ital: self.ital,
            fg_color: self.fg_color,
            bg_color: self.bg_color,
        }
    }
}

/// ANSI escape sequence parser state.
#[derive(Clone, Copy, PartialEq)]
enum AnsiState {
    Normal,
    Escape,   // Saw ESC (0x1B)
    Csi,      // Saw ESC [
    CsiParam, // Collecting CSI parameters
}

pub trait TtyDisplayBackend: Send + Sync {
    fn draw_char(&mut self, c: u8, style: &Style, x: usize, y: usize) -> Result<(), ()>;
    fn clear_screen(&mut self) -> Result<(), ()>;
    fn scroll_up(&mut self, lines: usize) -> Result<(), ()>;
    fn cols(&self) -> usize;
    fn rows(&self) -> usize;
    /// Draw or erase the cursor at the given position.
    fn draw_cursor(&mut self, x: usize, y: usize, visible: bool) -> Result<(), ()>;
}

pub trait TtyInputBackend: Send + Sync {
    fn read_byte(&mut self) -> Option<u8>;
}

/// A TTY device that manages text output with cursor tracking and scrolling.
pub struct TTYDevice {
    display: Box<dyn TtyDisplayBackend>,
    input_backend: Box<dyn TtyInputBackend>,
    cursor: Cursor,
    cursor_visible: bool,
    style: Style,
    ansi_state: AnsiState,
    ansi_params: Vec<u16>,
    ansi_current_param: u16,
}

impl TTYDevice {
    pub fn new(
        display: Box<dyn TtyDisplayBackend>,
        input_backend: Box<dyn TtyInputBackend>,
    ) -> Self {
        Self {
            display,
            cursor: Cursor::new(),
            cursor_visible: true,
            input_backend,
            style: Style::default(),
            ansi_state: AnsiState::Normal,
            ansi_params: Vec::new(),
            ansi_current_param: 0,
        }
    }

    fn update_cursor(&mut self) -> Result<(), ()> {
        let vis = self.cursor_visible;
        self.cursor_visible = !self.cursor_visible;
        self.display.draw_cursor(self.cursor.x, self.cursor.y, vis)
    }

    /// Redraw the cursor at current position.
    fn show_cursor(&mut self) {
        if self.cursor_visible {
            self.display
                .draw_cursor(self.cursor.x, self.cursor.y, true)
                .ok();
        }
    }

    /// Hide the cursor (erase it from current position).
    fn hide_cursor(&mut self) {
        self.display
            .draw_cursor(self.cursor.x, self.cursor.y, false)
            .ok();
    }

    fn  newline(&mut self) {
        self.hide_cursor();
        self.cursor.x = 0;
        self.cursor.y += 1;
        let max_rows = self.display.rows();
        if self.cursor.y >= max_rows {
            self.display.scroll_up(1).ok();
            self.cursor.y = max_rows.saturating_sub(1);
        }
        self.show_cursor();
    }

    fn reset_ansi_state(&mut self) {
        self.ansi_state = AnsiState::Normal;
        self.ansi_params.clear();
        self.ansi_current_param = 0;
    }

    fn process_csi_command(&mut self, cmd: u8) {
        // Push the last parameter if any
        if self.ansi_current_param > 0 || !self.ansi_params.is_empty() {
            self.ansi_params.push(self.ansi_current_param);
        }

        match cmd {
            b'm' => self.process_sgr(), // Select Graphic Rendition
            b'H' | b'f' => self.process_cursor_position(),
            b'J' => self.process_erase_display(),
            b'K' => self.process_erase_line(),
            b'A' => self.cursor_up(),
            b'B' => self.cursor_down(),
            b'C' => self.cursor_forward(),
            b'D' => self.cursor_back(),
            _ => {} // Unknown command, ignore
        }
        self.reset_ansi_state();
    }

    fn process_sgr(&mut self) {
        if self.ansi_params.is_empty() {
            self.ansi_params.push(0); // Default to reset
        }

        let mut i = 0;
        while i < self.ansi_params.len() {
            match self.ansi_params[i] {
                0 => self.style = Style::default(), // Reset
                1 => self.style.bold = true,
                3 => self.style.ital = true,
                4 => self.style.underline = true,
                9 => self.style.strikethrough = true,
                22 => self.style.bold = false,
                23 => self.style.ital = false,
                24 => self.style.underline = false,
                29 => self.style.strikethrough = false,
                // Foreground colors (30-37)
                30 => self.style.fg_color = Some((0, 0, 0)), // Black
                31 => self.style.fg_color = Some((205, 49, 49)), // Red
                32 => self.style.fg_color = Some((13, 188, 121)), // Green
                33 => self.style.fg_color = Some((229, 229, 16)), // Yellow
                34 => self.style.fg_color = Some((36, 114, 200)), // Blue
                35 => self.style.fg_color = Some((188, 63, 188)), // Magenta
                36 => self.style.fg_color = Some((17, 168, 205)), // Cyan
                37 => self.style.fg_color = Some((229, 229, 229)), // White
                39 => self.style.fg_color = None,            // Default
                // Background colors (40-47)
                40 => self.style.bg_color = Some((0, 0, 0)), // Black
                41 => self.style.bg_color = Some((205, 49, 49)), // Red
                42 => self.style.bg_color = Some((13, 188, 121)), // Green
                43 => self.style.bg_color = Some((229, 229, 16)), // Yellow
                44 => self.style.bg_color = Some((36, 114, 200)), // Blue
                45 => self.style.bg_color = Some((188, 63, 188)), // Magenta
                46 => self.style.bg_color = Some((17, 168, 205)), // Cyan
                47 => self.style.bg_color = Some((229, 229, 229)), // White
                49 => self.style.bg_color = None,            // Default
                // 256 color mode: 38;5;n or 48;5;n
                38 => {
                    if i + 2 < self.ansi_params.len() && self.ansi_params[i + 1] == 5 {
                        self.style.fg_color = Some(color_256(self.ansi_params[i + 2] as u8));
                        i += 2;
                    }
                }
                48 => {
                    if i + 2 < self.ansi_params.len() && self.ansi_params[i + 1] == 5 {
                        self.style.bg_color = Some(color_256(self.ansi_params[i + 2] as u8));
                        i += 2;
                    }
                }
                // Bright foreground colors (90-97)
                90 => self.style.fg_color = Some((128, 128, 128)),
                91 => self.style.fg_color = Some((255, 0, 0)),
                92 => self.style.fg_color = Some((0, 255, 0)),
                93 => self.style.fg_color = Some((255, 255, 0)),
                94 => self.style.fg_color = Some((0, 0, 255)),
                95 => self.style.fg_color = Some((255, 0, 255)),
                96 => self.style.fg_color = Some((0, 255, 255)),
                97 => self.style.fg_color = Some((255, 255, 255)),
                // Bright background colors (100-107)
                100 => self.style.bg_color = Some((128, 128, 128)),
                101 => self.style.bg_color = Some((255, 0, 0)),
                102 => self.style.bg_color = Some((0, 255, 0)),
                103 => self.style.bg_color = Some((255, 255, 0)),
                104 => self.style.bg_color = Some((0, 0, 255)),
                105 => self.style.bg_color = Some((255, 0, 255)),
                106 => self.style.bg_color = Some((0, 255, 255)),
                107 => self.style.bg_color = Some((255, 255, 255)),
                _ => {}
            }
            i += 1;
        }
    }

    fn process_cursor_position(&mut self) {
        let row = self.ansi_params.first().copied().unwrap_or(1).max(1) as usize - 1;
        let col = self.ansi_params.get(1).copied().unwrap_or(1).max(1) as usize - 1;
        self.cursor.y = row.min(self.display.rows().saturating_sub(1));
        self.cursor.x = col.min(self.display.cols().saturating_sub(1));
    }

    fn process_erase_display(&mut self) {
        let mode = self.ansi_params.first().copied().unwrap_or(0);
        match mode {
            2 | 3 => {
                self.display.clear_screen().ok();
                self.cursor.x = 0;
                self.cursor.y = 0;
            }
            _ => {} // Other modes not implemented
        }
    }

    fn process_erase_line(&mut self) {
        // Clear from cursor to end of line by writing spaces
        let mode = self.ansi_params.first().copied().unwrap_or(0);
        let cols = self.display.cols();
        let (start, end) = match mode {
            0 => (self.cursor.x, cols),  // Cursor to end
            1 => (0, self.cursor.x + 1), // Start to cursor
            2 => (0, cols),              // Entire line
            _ => return,
        };
        let blank_style = Style::default();
        for x in start..end {
            self.display
                .draw_char(b' ', &blank_style, x, self.cursor.y)
                .ok();
        }
    }

    fn cursor_up(&mut self) {
        let n = self.ansi_params.first().copied().unwrap_or(1).max(1) as usize;
        self.cursor.y = self.cursor.y.saturating_sub(n);
    }

    fn cursor_down(&mut self) {
        let n = self.ansi_params.first().copied().unwrap_or(1).max(1) as usize;
        self.cursor.y = (self.cursor.y + n).min(self.display.rows().saturating_sub(1));
    }

    fn cursor_forward(&mut self) {
        let n = self.ansi_params.first().copied().unwrap_or(1).max(1) as usize;
        self.cursor.x = (self.cursor.x + n).min(self.display.cols().saturating_sub(1));
    }

    fn cursor_back(&mut self) {
        let n = self.ansi_params.first().copied().unwrap_or(1).max(1) as usize;
        self.cursor.x = self.cursor.x.saturating_sub(n);
    }

    fn write_char_internal(&mut self, c: u8) {
        match self.ansi_state {
            AnsiState::Normal => {
                if c == 0x1B {
                    self.ansi_state = AnsiState::Escape;
                } else {
                    self.write_visible_char(c);
                }
            }
            AnsiState::Escape => {
                if c == b'[' {
                    self.ansi_state = AnsiState::Csi;
                    self.ansi_params.clear();
                    self.ansi_current_param = 0;
                } else {
                    // Not a CSI sequence, output ESC and char
                    self.reset_ansi_state();
                    self.write_visible_char(c);
                }
            }
            AnsiState::Csi | AnsiState::CsiParam => {
                if c.is_ascii_digit() {
                    self.ansi_state = AnsiState::CsiParam;
                    self.ansi_current_param = self
                        .ansi_current_param
                        .saturating_mul(10)
                        .saturating_add((c - b'0') as u16);
                } else if c == b';' {
                    self.ansi_params.push(self.ansi_current_param);
                    self.ansi_current_param = 0;
                } else if c >= 0x40 && c <= 0x7E {
                    // Command character
                    self.process_csi_command(c);
                } else {
                    // Invalid sequence
                    self.reset_ansi_state();
                }
            }
        }
    }

    fn write_visible_char(&mut self, c: u8) {
        self.hide_cursor();
        match c {
            b'\n' => {
                self.newline();
            }
            b'\r' => self.cursor.x = 0,
            b'\x08' => {
                if self.cursor.x > 0 {
                    self.display
                        .draw_char(b' ', &self.style, self.cursor.x - 1, self.cursor.y)
                        .ok();
                    self.cursor.x -= 1;
                }
            }
            b'\0' => {} // ASCII NUL, ignore
            b'\t' => {
                let next_tab_stop = ((self.cursor.x / 8) + 1) * 8;
                self.cursor.x = next_tab_stop.min(self.display.cols().saturating_sub(1));
            }
            _ => {
                let max_cols = self.display.cols();
                let max_rows = self.display.rows();

                if self.cursor.x >= max_cols {
                    self.cursor.x = 0;
                    self.cursor.y += 1;
                    if self.cursor.y >= max_rows {
                        self.display.scroll_up(1).ok();
                        self.cursor.y = max_rows.saturating_sub(1);
                    }
                }

                self.display
                    .draw_char(c, &self.style, self.cursor.x, self.cursor.y)
                    .ok();
                self.cursor.x += 1;
            }
        }
        self.show_cursor();
    }

    #[allow(unused)]
    fn read_char(&mut self) -> Option<u8> {
        let ib = &mut self.input_backend;
        ib.read_byte();
        todo!("use this instead of the direct call done by tty_read_nonblocking.")
    }

}

/// Convert 256-color palette index to RGB.
fn color_256(n: u8) -> (u8, u8, u8) {
    match n {
        0 => (0, 0, 0),
        1 => (128, 0, 0),
        2 => (0, 128, 0),
        3 => (128, 128, 0),
        4 => (0, 0, 128),
        5 => (128, 0, 128),
        6 => (0, 128, 128),
        7 => (192, 192, 192),
        8 => (128, 128, 128),
        9 => (255, 0, 0),
        10 => (0, 255, 0),
        11 => (255, 255, 0),
        12 => (0, 0, 255),
        13 => (255, 0, 255),
        14 => (0, 255, 255),
        15 => (255, 255, 255),
        16..=231 => {
            // 6x6x6 color cube
            let n = n - 16;
            let r = (n / 36) % 6;
            let g = (n / 6) % 6;
            let b = n % 6;
            let to_val = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            (to_val(r), to_val(g), to_val(b))
        }
        232..=255 => {
            // Grayscale
            let gray = 8 + (n - 232) * 10;
            (gray, gray, gray)
        }
    }
}

impl Write for TTYDevice {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.bytes() {
            self.write_char_internal(c);
        }
        Ok(())
    }
}

/// Global TTY device instance.
pub static TTY: Mutex<Option<TTYDevice>> = Mutex::new(None);

pub static PROC_TTYS: Lazy<Mutex<HashMap<u64, TTYDevice>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Initialize the TTY with a display backend.
pub fn init(display: Box<dyn TtyDisplayBackend>, input_backend: Box<dyn TtyInputBackend>) {
    let mut tty = TTY.lock();
    *tty = Some(TTYDevice::new(display, input_backend));
}

/// Create a new TTY for a process. For now, processes share the global TTY.
pub fn new_proc_tty(pid: u64) {
    // Currently we don't create separate TTYs per process - they all share the global one.
    // This function is a placeholder for future per-process TTY support.
    let _ = pid;
}

/// Remove a process's TTY entry when the process exits.
pub fn remove_proc_tty(pid: u64) {
    let mut proc_ttys = PROC_TTYS.lock();
    proc_ttys.remove(&pid);
}

/// Write bytes to the TTY for a given process (currently uses global TTY).
pub fn tty_write(pid: u64, buf: &[u8]) -> Result<usize, ()> {
    let _ = pid; // Currently ignored, all processes share global TTY
    let mut tty = TTY.try_lock().unwrap_or_else(|| {
        unsafe { TTY.force_unlock() };
        TTY.lock()
    });
    if let Some(ref mut tty_device) = *tty {
        for &b in buf {
            tty_device.write_char_internal(b);
        }
        Ok(buf.len())
    } else {
        Err(())
    }
}

/// Read bytes from the TTY for a given process (currently uses global TTY's input).
/// This is non-blocking and returns 0 if no data is available.
pub fn tty_read_nonblocking(_pid: u64, buf: &mut [u8]) -> Result<usize, ()> {
    // Read directly from the global keyboard buffer
    let mut kb = crate::hardware::keyboard::KEYBOARD_INPUT.lock();
    let mut count = 0;
    for slot in buf.iter_mut() {
        if let Some(b) = kb.pop() {
            *slot = b;
            count += 1;
        } else {
            break;
        }
    }
    Ok(count)
}

pub fn update_cursor() {
    unsafe { TTY.force_unlock() }
    let mut tty = TTY.lock();
    if let Some(ref mut tty_device) = *tty {
        tty_device.update_cursor().ok();
    }
}
