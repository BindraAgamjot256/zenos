//! Framebuffer module for the zenos kernel.
//!
//! This module provides functionality for interacting with the system's framebuffer,
//! allowing the kernel to render text and graphics. It includes ANSI sequence support
//! for formatting text output.

pub(crate) mod helpers;
pub(crate) mod macros;
pub(crate) mod pixel_converter;

use crate::{
    framebuffer::pixel_converter::PixelConverter,
    framebuffer::pixel_converter::{Bgr888Converter, Gray8Converter, Rgb888Converter},
};
use bootloader_api::info::{FrameBufferInfo, PixelFormat};
use core::{convert::Infallible, fmt};
use embedded_graphics::{
    Drawable, Pixel,
    geometry::Size,
    mono_font::iso_8859_1::FONT_10X20,
    mono_font::{MonoFont, MonoTextStyle},
    pixelcolor::{Rgb888, RgbColor},
    prelude::{DrawTarget, OriginDimensions, Point, Primitive},
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};
use heapless::Vec;
use spin::Mutex;

/// Left padding for text output in pixels.
const L_PADDING: usize = 5;

/// Top padding for text output in pixels.
const T_PADDING: usize = 15;

/// Tracks current and previous cursor position for text rendering.
///
/// The cursor maintains both current position and previous position
/// to support cursor movement operations and optimization.
struct Cursor {
    /// Current horizontal position in pixels.
    x: usize,
    /// Current vertical position in pixels.
    y: usize,
    /// Previous horizontal position in pixels.
    prev_x: usize,
    /// Previous vertical position in pixels.
    prev_y: usize,
}

impl Cursor {
    /// Returns the current horizontal cursor position.
    ///
    /// # Returns
    ///
    /// The current x-coordinate in pixels.
    pub(crate) fn get_x(&self) -> usize {
        self.x
    }

    /// Returns the current vertical cursor position.
    ///
    /// # Returns
    ///
    /// The current y-coordinate in pixels.
    pub(crate) fn get_y(&self) -> usize {
        self.y
    }

    /// Creates a new cursor at the default starting position.
    ///
    /// The cursor is initialized to the top-left position with padding.
    ///
    /// # Returns
    ///
    /// A new `Cursor` instance positioned at (L_PADDING, T_PADDING).
    fn new() -> Self {
        Cursor {
            x: L_PADDING,
            y: T_PADDING,
            prev_x: 0,
            prev_y: 0,
        }
    }

    /// Moves the cursor to a new position, saving the previous position.
    ///
    /// # Arguments
    ///
    /// * `x` - The new horizontal position in pixels
    /// * `y` - The new vertical position in pixels
    fn move_to(&mut self, x: usize, y: usize) {
        self.prev_x = self.x;
        self.prev_y = self.y;
        self.x = x;
        self.y = y;
    }
}

/// Represents the state of ANSI escape sequence parsing.
///
/// This enum tracks the current state when processing ANSI escape sequences
/// for terminal-like text formatting and control.
enum AnsiState {
    /// Normal text processing state.
    Ground,
    /// Escape character (\x1B) has been received, waiting for next character.
    Escape,
    /// Control Sequence Introducer (CSI) mode, collecting parameters.
    ///
    /// The `params` vector stores numeric parameters for the CSI sequence,
    /// limited to 8 parameters maximum.
    Csi { params: Vec<u8, 8> },
}

/// A framebuffer writer that provides text rendering and graphics capabilities.
///
/// This struct manages a framebuffer for rendering text with ANSI escape sequence
/// support, cursor management, and various text formatting options including
/// bold, underline, italic, and strikethrough styles.
///
/// # Features
///
/// - Text rendering with monospace fonts
/// - ANSI escape sequence processing
/// - Multiple pixel formats (RGB, BGR, U8 grayscale)
/// - Cursor management and blinking
/// - Screen scrolling
/// - Text formatting (bold, underline, strikethrough)
/// - Color support (foreground and background)
pub(crate) struct FrameBufferWriter<'fb> {
    /// Raw framebuffer memory.
    framebuffer: &'fb mut [u8],
    /// Framebuffer configuration information.
    info: FrameBufferInfo,
    /// Current background color for text and drawing operations.
    bg_color: Rgb888,
    /// Current foreground color for text and drawing operations.
    fg_color: Rgb888,
    /// Font used for text rendering.
    font: MonoFont<'static>,
    /// Current cursor position tracker.
    cursor: Cursor,
    /// Current state of ANSI escape sequence parsing.
    ansi_state: AnsiState,
    /// Whether bold text formatting is enabled.
    ansi_bold: bool,
    /// Whether underline text formatting is enabled.
    ansi_underline: bool,
    /// Whether italic text formatting is enabled.
    ansi_italic: bool,
    /// Whether strikethrough text formatting is enabled.
    ansi_strikethrough: bool,
    /// Whether the cursor is currently visible.
    cursor_visible: bool,
}

impl<'fb> FrameBufferWriter<'fb> {
    /// Width of the cursor in pixels.
    const CURSOR_WIDTH: u32 = 20;
    /// Height of the cursor in pixels.
    const CURSOR_HEIGHT: u32 = 2;

    /// Creates a new framebuffer writer.
    ///
    /// Initializes the writer with default colors (white text on black background),
    /// a monospace font, and default cursor position.
    ///
    /// # Arguments
    ///
    /// * `framebuffer` - Mutable reference to the raw framebuffer memory
    /// * `info` - Framebuffer configuration information
    ///
    /// # Returns
    ///
    /// A new `FrameBufferWriter` instance ready for text and graphics output.
    pub fn new(framebuffer: &'fb mut [u8], info: FrameBufferInfo) -> Self {
        FrameBufferWriter {
            framebuffer,
            info,
            bg_color: Rgb888::new(0, 0, 0),       // Black background
            fg_color: Rgb888::new(255, 255, 255), // White foreground
            font: FONT_10X20,
            cursor: Cursor::new(),
            ansi_state: AnsiState::Ground,
            ansi_bold: false,
            ansi_underline: false,
            ansi_italic: false,
            ansi_strikethrough: false,
            cursor_visible: false,
        }
    }

    /// Scrolls the framebuffer content up by one line height.
    ///
    /// This method efficiently moves the entire framebuffer content upward
    /// and clears the bottom area with the background color. The cursor
    /// position is adjusted accordingly.
    ///
    /// # Performance
    ///
    /// Uses bulk memory operations for efficiency rather than pixel-by-pixel copying.
    fn scroll_up(&mut self) {
        let char_height = self.font.character_size.height as usize + 3; // Include line spacing
        let width = self.info.width;
        let bpp = self.info.bytes_per_pixel;
        let bytes_per_row = width * bpp;
        let scroll_bytes = char_height * bytes_per_row;

        // Use memmove-like operation for bulk copying
        // This is much faster than pixel-by-pixel copying
        let total_bytes = self.framebuffer.len();
        let remaining_bytes = total_bytes.saturating_sub(scroll_bytes);

        // Copy the entire framebuffer content up by scroll_bytes
        if scroll_bytes < total_bytes {
            // Use copy_within for efficient memory copying
            self.framebuffer.copy_within(scroll_bytes..total_bytes, 0);
        }

        // Clear the bottom area in one go
        let clear_start = remaining_bytes;
        if clear_start < total_bytes {
            // Get background color bytes based on pixel format
            let bg_bytes = self.get_background_color_bytes();

            // Fill the bottom area with background color
            for chunk in self.framebuffer[clear_start..].chunks_exact_mut(bpp) {
                chunk.copy_from_slice(&bg_bytes[..bpp]);
            }
        }

        // Move cursor up by one line
        let new_y = self.cursor.get_y().saturating_sub(char_height);
        self.cursor.move_to(self.cursor.get_x(), new_y);
    }

    /// Converts the background color to raw bytes based on the pixel format.
    ///
    /// # Returns
    ///
    /// An array of 4 bytes representing the background color in the
    /// framebuffer's native pixel format.
    fn get_background_color_bytes(&self) -> [u8; 4] {
        match self.info.pixel_format {
            PixelFormat::Rgb => [self.bg_color.r(), self.bg_color.g(), self.bg_color.b(), 0],
            PixelFormat::Bgr => [self.bg_color.b(), self.bg_color.g(), self.bg_color.r(), 0],
            PixelFormat::U8 => {
                let gray = ((self.bg_color.r() as u16
                    + self.bg_color.g() as u16
                    + self.bg_color.b() as u16)
                    / 3) as u8;
                [gray, 0, 0, 0]
            }
            _ => [self.bg_color.r(), self.bg_color.g(), self.bg_color.b(), 0],
        }
    }

    /// Checks if the cursor is beyond screen bounds and scrolls if necessary.
    ///
    /// This method is called after each character write to ensure the cursor
    /// remains within visible screen area. If the cursor would extend beyond
    /// the bottom of the screen, it triggers a scroll operation.
    fn check_and_scroll(&mut self) {
        let char_height = self.font.character_size.height as usize + 3; // Include line spacing
        let screen_height = self.info.height;

        // Check if cursor is beyond screen bounds
        if self.cursor.get_y() + char_height > screen_height {
            self.scroll_up();
        }
    }

    /// Handles a single character, processing ANSI escape sequences.
    ///
    /// This method implements a state machine for parsing ANSI escape sequences
    /// while also handling regular character output.
    ///
    /// # Arguments
    ///
    /// * `c` - The character to process
    ///
    /// # ANSI Support
    ///
    /// Supports the following ANSI escape sequences:
    /// - ESC[m - SGR (Select Graphic Rendition) for text formatting
    /// - ESC[J - Erase in Display
    /// - ESC[H - Cursor Position
    fn handle_char(&mut self, c: char) {
        match &mut self.ansi_state {
            AnsiState::Ground => {
                if c == '\x1B' {
                    self.ansi_state = AnsiState::Escape;
                } else {
                    self.write_character(c);
                }
            }
            AnsiState::Escape => {
                if c == '[' {
                    self.ansi_state = AnsiState::Csi { params: Vec::new() };
                } else {
                    self.ansi_state = AnsiState::Ground;
                    self.write_character('\x1B');
                    self.write_character(c);
                }
            }
            AnsiState::Csi { params } => {
                if c.is_ascii_digit() {
                    if let Some(last) = params.last_mut() {
                        *last = *last * 10 + (c as u8 - b'0');
                    } else {
                        let _ = params.push(c as u8 - b'0');
                    }
                } else if c == ';' {
                    let _ = params.push(0);
                } else {
                    let completed = core::mem::take(params);
                    self.ansi_state = AnsiState::Ground;
                    match c {
                        'm' => self.process_ansi_parameters(&completed),
                        'J' => self.process_erase_display(&completed),
                        'H' => self.process_cursor_position(&completed),
                        _ => {} // Ignore unknown CSI sequences
                    }
                }
            }
        }
    }

    /// Writes a single character to the framebuffer with current formatting.
    ///
    /// This method handles special characters (newline, carriage return) and
    /// renders normal characters with the current text formatting attributes.
    ///
    /// # Arguments
    ///
    /// * `c` - The character to write
    ///
    /// # Special Characters
    ///
    /// - `\n` - Moves cursor to next line
    /// - `\r` - Moves cursor to beginning of current line
    /// - `\x08` - Moves cursor back one character (backspace)
    ///
    /// # Text Formatting
    ///
    /// Applies current formatting attributes:
    /// - Bold (simulated by drawing text twice with offset)
    /// - Underline (drawn as a line below the character)
    /// - Strikethrough (drawn as a line through the middle)
    /// - Italic (simulated by drawing character with horizontal shear)
    fn write_character(&mut self, c: char) {
        let binding = [c as u8];
        let str: &str = str::from_utf8(&binding).unwrap();

        if c == '\n' {
            // Move cursor to next line
            self.cursor.move_to(
                L_PADDING,
                self.cursor.get_y() + self.font.character_size.height as usize + 3,
            );
            self.check_and_scroll();
            return;
        } else if c == '\r' {
            // Move cursor to the beginning of the line
            self.cursor.move_to(L_PADDING, self.cursor.get_y());
            return;
        } else if c == '\x08' {
            // Backspace
            if self.cursor.get_x() > L_PADDING {
                // Move cursor left one character
                let new_x = self
                    .cursor
                    .get_x()
                    .saturating_sub(self.font.character_size.width as usize);
                self.cursor.move_to(new_x, self.cursor.get_y());

                // Draw a background rect to "erase" the previous char
                let bg_rect = Rectangle::new(
                    Point::new(
                        new_x as i32,
                        self.cursor.get_y() as i32 - self.font.baseline as i32,
                    ),
                    Size::new(
                        self.font.character_size.width,
                        self.font.character_size.height + 3,
                    ),
                );
                bg_rect
                    .into_styled(PrimitiveStyle::with_fill(self.bg_color))
                    .draw(self)
                    .ok();
            }
            return;
        } else if c == '\t' {
            // Tab character - move cursor to next tab stop (every 4 characters)
            let char_width = self.font.character_size.width as usize;
            let current_col = (self.cursor.get_x() - L_PADDING) / char_width;
            let next_tab_col = (current_col / 4 + 1) * 4;
            let new_x = L_PADDING + next_tab_col * char_width;
            self.cursor.move_to(new_x, self.cursor.get_y());
            return;
        } else if c == '\0' {
            // Ignore null characters
            return;
        }

        let font = self.font;
        let style = MonoTextStyle::new(&font, self.fg_color);

        // Draw background rectangle
        let bg_rect = Rectangle::new(
            Point::new(
                self.cursor.get_x() as i32,
                self.cursor.get_y() as i32 - self.font.baseline as i32,
            ),
            Size::new(
                self.font.character_size.width,
                self.font.character_size.height + 3,
            ),
        );
        bg_rect
            .into_styled(PrimitiveStyle::with_fill(self.bg_color))
            .draw(self)
            .ok();

        // Determine the position offset for italic effect
        let italic_offset = if self.ansi_italic { 2 } else { 0 };

        // Draw the character (with italic offset if enabled)
        Text::new(
            str,
            Point::new(
                self.cursor.get_x() as i32 + italic_offset,
                self.cursor.get_y() as i32,
            ),
            style,
        )
        .draw(self)
        .ok();

        // Apply text formatting effects
        if self.ansi_bold {
            // Draw bold text by drawing it again with a slight offset
            Text::new(
                str,
                Point::new(
                    self.cursor.get_x() as i32 + 1 + italic_offset,
                    self.cursor.get_y() as i32,
                ),
                style,
            )
            .draw(self)
            .ok();
        }

        if self.ansi_underline {
            // Draw underline
            let underline_y = self.cursor.get_y() as i32 + self.font.character_size.height as i32
                - self.font.baseline as i32;
            Rectangle::new(
                Point::new(self.cursor.get_x() as i32, underline_y),
                Size::new(self.font.character_size.width, 1),
            )
            .into_styled(PrimitiveStyle::with_fill(self.fg_color))
            .draw(self)
            .ok();
        }

        if self.ansi_strikethrough {
            // Draw strikethrough
            let strike_y = self.cursor.get_y() as i32 + self.font.character_size.height as i32 / 2
                - self.font.baseline as i32;
            Rectangle::new(
                Point::new(self.cursor.get_x() as i32, strike_y),
                Size::new(self.font.character_size.width, 1),
            )
            .into_styled(PrimitiveStyle::with_fill(self.fg_color))
            .draw(self)
            .ok();
        }

        if self.ansi_italic {
            // todo(probably never gonna be fixed(at least by me) but... sure)
            //      Italic effect is supposed to be simulated by drawing the character with a horizontal shear.
            //      How we would do that is beyond me right now.
        }

        self.cursor.move_to(
            self.cursor.get_x() + self.font.character_size.width as usize,
            self.cursor.get_y(),
        );

        // move text to the next line if char is out of bounds.
        if self.cursor.get_x() >= self.info.width - L_PADDING {
            // subtract L_PADDING to get a uniform padding on both sides.
            self.cursor.move_to(
                L_PADDING,
                self.cursor.get_y() + self.font.character_size.height as usize + 3,
            )
        }

        self.check_and_scroll();
    }

    /// Processes ANSI SGR (Select Graphic Rendition) parameters for text formatting.
    ///
    /// This method handles ANSI escape sequences that control text appearance,
    /// including colors, bold, underline, and other formatting options.
    ///
    /// # Arguments
    ///
    /// * `params` - Vector of numeric parameters from the ANSI sequence
    ///
    /// # Supported Parameters
    ///
    /// - 0: Reset all attributes
    /// - 1: Bold on, 22: Bold off
    /// - 3: Italic on, 23: Italic off (italic not implemented)
    /// - 4: Underline on, 24: Underline off
    /// - 9: Strikethrough on, 29: Strikethrough off
    /// - 30-37: Foreground colors
    /// - 40-47: Background colors
    /// - 38;5;n: Extended foreground color
    /// - 48;5;n: Extended background color
    fn process_ansi_parameters(&mut self, params: &Vec<u8, 8>) {
        if params.is_empty() {
            self.reset_attributes();
            return;
        }

        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => self.reset_attributes(),
                1 => self.ansi_bold = true,
                3 => self.ansi_italic = true,
                4 => self.ansi_underline = true,
                9 => self.ansi_strikethrough = true,
                22 => self.ansi_bold = false,
                23 => self.ansi_italic = false,
                24 => self.ansi_underline = false,
                29 => self.ansi_strikethrough = false,
                30 => self.fg_color = Rgb888::new(0, 0, 0), // Black
                31 => self.fg_color = Rgb888::new(170, 0, 0), // Red
                32 => self.fg_color = Rgb888::new(0, 170, 0), // Green
                33 => self.fg_color = Rgb888::new(170, 85, 0), // Yellow
                34 => self.fg_color = Rgb888::new(0, 0, 170), // Blue
                35 => self.fg_color = Rgb888::new(170, 0, 170), // Magenta
                36 => self.fg_color = Rgb888::new(0, 170, 170), // Cyan
                37 => self.fg_color = Rgb888::new(170, 170, 170), // White
                40 => self.bg_color = Rgb888::new(0, 0, 0), // Black background
                41 => self.bg_color = Rgb888::new(85, 0, 0), // Red background
                42 => self.bg_color = Rgb888::new(0, 85, 0), // Green background
                43 => self.bg_color = Rgb888::new(85, 42, 0), // Yellow background
                44 => self.bg_color = Rgb888::new(0, 0, 85), // Blue background
                45 => self.bg_color = Rgb888::new(85, 0, 85), // Magenta background
                46 => self.bg_color = Rgb888::new(0, 85, 85), // Cyan background
                47 => self.bg_color = Rgb888::new(85, 85, 85), // White background
                38 => {
                    // Extended foreground color: ESC[38;5;n
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        self.set_fg_color_from_ansi(params[i + 2]);
                        i += 2;
                    }
                }
                48 => {
                    // Extended background color: ESC[48;5;n
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        self.set_bg_color_from_ansi(params[i + 2]);
                        i += 2;
                    }
                }
                _ => {} // Ignore unknown parameters
            }
            i += 1;
        }
    }

    /// Resets all text formatting attributes to default values.
    ///
    /// This includes:
    /// - Disabling bold, underline, italic, and strikethrough
    /// - Resetting colors to white foreground on black background
    fn reset_attributes(&mut self) {
        self.ansi_bold = false;
        self.ansi_underline = false;
        self.ansi_italic = false;
        self.ansi_strikethrough = false;
        self.fg_color = Rgb888::new(255, 255, 255); // White
        self.bg_color = Rgb888::new(0, 0, 0); // Black
    }

    /// Sets the foreground color based on an ANSI color index.
    ///
    /// This is a simplified implementation that only supports a few colors.
    ///
    /// # Arguments
    ///
    /// * `index` - ANSI color index (0-255, but only a few are implemented)
    fn set_fg_color_from_ansi(&mut self, index: u8) {
        self.fg_color = match index {
            1 => Rgb888::new(170, 0, 0),     // Red
            2 => Rgb888::new(0, 170, 0),     // Green
            4 => Rgb888::new(0, 0, 170),     // Blue
            _ => Rgb888::new(255, 255, 255), // Default white
        }
    }

    /// Sets the background color based on an ANSI color index.
    ///
    /// This is a simplified implementation that only supports a few colors.
    ///
    /// # Arguments
    ///
    /// * `index` - ANSI color index (0-255, but only a few are implemented)
    fn set_bg_color_from_ansi(&mut self, index: u8) {
        self.bg_color = match index {
            1 => Rgb888::new(85, 0, 0), // Dark red
            2 => Rgb888::new(0, 85, 0), // Dark green
            4 => Rgb888::new(0, 0, 85), // Dark blue
            _ => Rgb888::new(0, 0, 0),  // Default black
        }
    }

    /// Processes ANSI erase display sequences (ESC[J).
    ///
    /// # Arguments
    ///
    /// * `params` - Parameters for the erase operation
    ///
    /// # Supported Modes
    ///
    /// - 0: Clear from cursor to end of screen (not implemented)
    /// - 1: Clear from the beginning of screen to cursor (not implemented)
    /// - 2: Clear the entire screen
    fn process_erase_display(&mut self, params: &Vec<u8, 8>) {
        let mode = if params.is_empty() { 0 } else { params[0] };

        match mode {
            2 => {
                // Clear entire screen
                self.clear(self.bg_color)
                    .expect("Won't fail... infallible.");
            }
            0 => {
                // Clear from cursor to end of screen (not implemented)
                unimplemented!()
            }
            1 => {
                // Clear from the beginning of screen to cursor (not implemented)
                unimplemented!()
            }
            _ => {} // Ignore unknown modes
        }
    }

    /// Processes ANSI cursor position sequences (ESC[H).
    ///
    /// Moves the cursor to the specified row and column. Coordinates are
    /// 1-based (as per ANSI standard) and are converted to pixel positions.
    ///
    /// # Arguments
    ///
    /// * `params` - Row and column parameters (1-based)
    ///
    /// # Coordinate System
    ///
    /// - Row 1, Column 1 corresponds to the top-left text position
    /// - Coordinates are clamped to screen boundaries
    fn process_cursor_position(&mut self, params: &Vec<u8, 8>) {
        let row = if params.is_empty() {
            1
        } else {
            params[0].max(1)
        };
        let col = if params.len() < 2 {
            1
        } else {
            params[1].max(1)
        };

        let char_height = self.font.character_size.height as usize + 3;
        let char_width = 10; // Approximate character width

        let x = L_PADDING + (col as usize - 1) * char_width;
        let y = T_PADDING + (row as usize - 1) * char_height;

        // Ensure cursor stays within screen bounds
        let max_x = self.info.width - char_width;
        let max_y = self.info.height - char_height;

        let final_x = x.min(max_x);
        let final_y = y.min(max_y);

        self.cursor.move_to(final_x, final_y);
    }

    /// Draws the cursor at the current position.
    ///
    /// The cursor is rendered as a small rectangle using the current
    /// foreground color. This method also toggles the cursor visibility state.
    fn draw_cursor(&mut self) {
        let cursor_x = self.cursor.get_x() as i32;
        let cursor_y = self.cursor.get_y() as i32 - self.font.baseline as i32;

        Rectangle::new(
            Point::new(cursor_x, cursor_y),
            Size::new(Self::CURSOR_HEIGHT, Self::CURSOR_WIDTH),
        )
        .into_styled(PrimitiveStyle::with_fill(self.fg_color))
        .draw(self)
        .ok();
        self.cursor_visible = !self.cursor_visible;
    }

    /// Erases the cursor at the current position.
    ///
    /// The cursor area is filled with the background color to hide it.
    /// This method also toggles the cursor visibility state.
    fn erase_cursor(&mut self) {
        let cursor_x = self.cursor.get_x() as i32;
        let cursor_y = self.cursor.get_y() as i32 - self.font.baseline as i32;
        Rectangle::new(
            Point::new(cursor_x, cursor_y),
            Size::new(Self::CURSOR_HEIGHT, Self::CURSOR_WIDTH),
        )
        .into_styled(PrimitiveStyle::with_fill(self.bg_color))
        .draw(self)
        .ok();
        self.cursor_visible = !self.cursor_visible;
    }

    /// Updates the cursor visibility by toggling between visible and hidden states.
    ///
    /// This method should be called periodically to create a blinking cursor effect.
    /// If the cursor is currently visible, it will be erased. If hidden, it will be drawn.
    pub(crate) fn update_cursor(&mut self) {
        if self.cursor_visible {
            self.erase_cursor();
        } else {
            self.draw_cursor();
        }
    }
}

/// Implementation of the `fmt::Write` trait for string output.
///
/// This allows the framebuffer writer to be used with Rust's formatting
/// macros like `write!` and `writeln!`.
impl<'fb> fmt::Write for FrameBufferWriter<'fb> {
    /// Writes a string to the framebuffer.
    ///
    /// The cursor is temporarily hidden during writing and restored afterwards
    /// to prevent visual artifacts during text output.
    ///
    /// # Arguments
    ///
    /// * `s` - The string to write
    ///
    /// # Returns
    ///
    /// Always returns `Ok(())` as framebuffer writing cannot fail.
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.erase_cursor();
        for c in s.chars() {
            self.handle_char(c);
        }
        self.draw_cursor();
        Ok(())
    }
}

/// Implementation of the `DrawTarget` trait for embedded-graphics compatibility.
///
/// This allows the framebuffer writer to be used as a drawing target for
/// the embedded-graphics library, enabling efficient graphics rendering.
impl<'fb> DrawTarget for FrameBufferWriter<'fb> {
    type Color = Rgb888;
    type Error = Infallible;

    /// Draws an iterator of pixels to the framebuffer.
    ///
    /// This method efficiently writes pixels directly to the framebuffer memory,
    /// handling different pixel formats automatically.
    ///
    /// # Arguments
    ///
    /// * `iter` - Iterator of pixels to draw
    ///
    /// # Returns
    ///
    /// Always returns `Ok(())` as pixel drawing cannot fail.
    ///
    /// # Pixel Format Handling
    ///
    /// - RGB: Direct copying
    /// - BGR: Color channel swapping
    /// - U8: Grayscale conversion
    fn draw_iter<I>(&mut self, iter: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let width = self.info.width;
        let bpp = self.info.bytes_per_pixel;

        for Pixel(point, color) in iter {
            let x = point.x as usize;
            let y = point.y as usize;
            if x >= width || y >= self.info.height {
                continue;
            }
            let offset = (y * width + x) * bpp;

            match self.info.pixel_format {
                PixelFormat::Rgb => {
                    // Direct RGB -> RGB
                    Rgb888Converter.write_to_buffer(color, self.framebuffer, offset);
                }
                PixelFormat::Bgr => {
                    // RGB -> BGR
                    let c = Bgr888Converter::create_color(color.r(), color.g(), color.b());
                    Bgr888Converter.write_to_buffer(c, self.framebuffer, offset);
                }
                PixelFormat::U8 => {
                    // RGB -> Gray
                    let c = Gray8Converter::create_color(color.r(), color.g(), color.b());
                    Gray8Converter.write_to_buffer(c, self.framebuffer, offset);
                }
                _ => {
                    // Fallback to RGB
                    Rgb888Converter.write_to_buffer(color, self.framebuffer, offset);
                }
            }
        }
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let buf = self.framebuffer.chunks_exact_mut(self.info.bytes_per_pixel);
        for pixel in buf {
            match self.info.pixel_format {
                PixelFormat::Rgb => {
                    Rgb888Converter.write_to_buffer(color, pixel, 0);
                }
                PixelFormat::Bgr => {
                    let c = Bgr888Converter::create_color(color.r(), color.g(), color.b());
                    Bgr888Converter.write_to_buffer(c, pixel, 0);
                }
                PixelFormat::U8 => {
                    let c = Gray8Converter::create_color(color.r(), color.g(), color.b());
                    Gray8Converter.write_to_buffer(c, pixel, 0);
                }
                _ => {
                    Rgb888Converter.write_to_buffer(color, pixel, 0);
                }
            }
        }
        Ok(())
    }
}

/// Implementation of the `OriginDimensions` trait for embedded-graphics compatibility.
///
/// This provides the framebuffer dimensions to the embedded-graphics library.
impl<'fb> OriginDimensions for FrameBufferWriter<'fb> {
    /// Returns the size of the framebuffer.
    ///
    /// # Returns
    ///
    /// A `Size` struct containing the width and height of the framebuffer in pixels.
    fn size(&self) -> Size {
        Size::new(self.info.width as u32, self.info.height as u32)
    }
}

/// Global framebuffer writer instance.
///
/// This static variable holds the framebuffer writer in a mutex for thread-safe
/// access across the kernel. It is initialized as `None` and set up during
/// system initialization.
///
/// # Thread Safety
///
/// The `Mutex` ensures that only one thread can access the framebuffer at a time,
/// preventing race conditions during text output and graphics operations.
pub(crate) static FRAMEBUFFER: Mutex<Option<FrameBufferWriter>> = Mutex::new(None);

#[cfg(feature = "run-kunittest")]
mod tests {
    use super::*;
    use crate::{Test, test_assert, test_assert_eq};

    const WIDTH: usize = 100;
    const HEIGHT: usize = 50;
    const BPP: usize = 4; // bytes per pixel, assuming 32-bit color
    const FB_SIZE: usize = WIDTH * HEIGHT * BPP;

    fn make_info() -> FrameBufferInfo {
        FrameBufferInfo {
            width: WIDTH,
            height: HEIGHT,
            bytes_per_pixel: BPP,
            pixel_format: PixelFormat::Rgb,
            byte_len: FB_SIZE,
            stride: 0,
        }
    }

    fn make_fb() -> [u8; FB_SIZE] {
        [0u8; FB_SIZE]
    }

    #[zenos_macros::test]
    pub fn test_write_character_basic() -> Option<()> {
        let mut fb = make_fb();
        let info = make_info();
        let mut writer = FrameBufferWriter::new(&mut fb, info);

        writer.write_character('A');
        // The cursor should move right by char width (approx 10)
        test_assert_eq!(writer.cursor.get_x(), L_PADDING + 10);
        test_assert_eq!(writer.cursor.get_y(), T_PADDING);

        // Write newline moves cursor down and resets x
        writer.write_character('\n');
        test_assert_eq!(writer.cursor.get_x(), L_PADDING);
        test_assert_eq!(
            writer.cursor.get_y() + 10, // for one char height
            writer.font.character_size.height as usize + 5
        );
        Some(())
    }
    #[zenos_macros::test]
    pub fn test_ansi_escape_bold_and_color() -> Option<()> {
        let mut fb = make_fb();
        let info = make_info();
        let mut writer = FrameBufferWriter::new(&mut fb, info);

        // Send ANSI escape sequence for bold on
        writer.handle_char('\x1B');
        writer.handle_char('[');
        writer.handle_char('1');
        writer.handle_char('m');
        test_assert!(writer.ansi_bold);

        // Send ANSI escape for red fg
        writer.handle_char('\x1B');
        writer.handle_char('[');
        writer.handle_char('3');
        writer.handle_char('1');
        writer.handle_char('m');
        test_assert_eq!(writer.fg_color, Rgb888::new(170, 0, 0));

        // Reset attributes
        writer.handle_char('\x1B');
        writer.handle_char('[');
        writer.handle_char('0');
        writer.handle_char('m');
        test_assert!(!writer.ansi_bold);
        test_assert_eq!(writer.fg_color, Rgb888::new(255, 255, 255));
        Some(())
    }
    #[zenos_macros::test]
    pub fn test_scroll_up() -> Option<()> {
        let mut fb = make_fb();
        let info = make_info();
        let mut writer = FrameBufferWriter::new(&mut fb, info);

        // Put cursor at bottom to force scroll
        writer.cursor.move_to(L_PADDING, HEIGHT - 1);

        // Write newline to check scroll triggers
        writer.write_character('\n');
        // Cursor should move up by char_height after scroll
        test_assert!(writer.cursor.get_y() < HEIGHT);

        Some(())
    }

    #[zenos_macros::test]
    pub fn test_reset_attributes() -> Option<()> {
        let mut fb = make_fb();
        let info = make_info();
        let mut writer = FrameBufferWriter::new(&mut fb, info);

        writer.ansi_bold = true;
        writer.ansi_underline = true;
        writer.ansi_italic = true;
        writer.ansi_strikethrough = true;
        writer.fg_color = Rgb888::new(123, 45, 67);
        writer.bg_color = Rgb888::new(10, 20, 30);

        writer.reset_attributes();

        test_assert!(!writer.ansi_bold);
        test_assert!(!writer.ansi_underline);
        test_assert!(!writer.ansi_italic);
        test_assert!(!writer.ansi_strikethrough);
        test_assert_eq!(writer.fg_color, Rgb888::new(255, 255, 255));
        test_assert_eq!(writer.bg_color, Rgb888::new(0, 0, 0));
        Some(())
    }
}
