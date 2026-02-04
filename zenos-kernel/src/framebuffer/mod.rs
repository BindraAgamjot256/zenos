//! Framebuffer module for the zenos kernel.
//!
//! This module provides functionality for interacting with the system's framebuffer,
//! allowing the kernel to render text and graphics. It includes ANSI sequence support
//! for formatting text output.

pub(crate) mod helpers;
pub(crate) mod macros;
pub(crate) mod pixel_converter;

use crate::tty::{Style, TtyDisplayBackend};
use crate::{
    framebuffer::pixel_converter::PixelConverter,
    framebuffer::pixel_converter::{Bgr888Converter, Gray8Converter, Rgb888Converter},
};
use bootloader_api::info::{FrameBufferInfo, PixelFormat};
use core::convert::Infallible;
use core::fmt::Write;
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
    fn write_character(&mut self, c: char, state: &Style, pos: Cursor) {
        self.cursor.move_to(pos.x, pos.y);
        let binding = [c as u8];
        let str = str::from_utf8(&binding);
        if str.is_err() {
            return self.write_character('?', state, pos);
        }
        let str = str.unwrap();
        let font = self.font;
        let fg = state
            .fg_color
            .map(|c| Rgb888::new(c.0, c.1, c.2))
            .unwrap_or(self.fg_color);
        let style = MonoTextStyle::new(&font, fg);

        let bg = state
            .bg_color
            .map(|c| Rgb888::new(c.0, c.1, c.2))
            .unwrap_or(self.bg_color);

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
            .into_styled(PrimitiveStyle::with_fill(bg))
            .draw(self)
            .ok();

        // Determine the position offset for italic effect
        let italic_offset = if state.ital { 2 } else { 0 };

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
        if state.bold {
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

        if state.underline {
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

        if state.strikethrough {
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

        if state.ital {
            // todo(probably never gonna be fixed(at least by me) but... sure)
            //      Italic effect is supposed to be simulated by drawing the character with a horizontal shear.
            //      How we would do that is beyond me right now.
        }
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

impl Write for FrameBufferWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.chars() {
            self.draw_char(c as u8, &Style::default(), 0, 0)
                .map_err(|_| core::fmt::Error)?;
        }
        Ok(())
    }
}

impl TtyDisplayBackend for FrameBufferWriter<'_> {
    fn draw_char(&mut self, c: u8, style: &Style, x: usize, y: usize) -> Result<(), ()> {
        let mut cur = Cursor::new();
        cur.x = L_PADDING + x * self.font.character_size.width as usize;
        cur.y = T_PADDING
            + y * (self.font.character_size.height as usize + 3)
            + self.font.baseline as usize;
        self.write_character(c as char, style, cur);
        Ok(())
    }

    fn clear_screen(&mut self) -> Result<(), ()> {
        self.clear(Rgb888::BLACK).ok();
        Ok(())
    }

    fn scroll_up(&mut self, lines: usize) -> Result<(), ()> {
        for _ in 0..lines {
            FrameBufferWriter::scroll_up(self);
        }
        Ok(())
    }

    fn cols(&self) -> usize {
        (self.info.width.saturating_sub(L_PADDING * 2)) / self.font.character_size.width as usize
    }

    fn rows(&self) -> usize {
        (self.info.height.saturating_sub(T_PADDING * 2))
            / (self.font.character_size.height as usize + 3)
    }

    fn draw_cursor(&mut self, x: usize, y: usize, visible: bool) -> Result<(), ()> {
        let pixel_x = L_PADDING + x * self.font.character_size.width as usize;
        let pixel_y = T_PADDING + y * (self.font.character_size.height as usize + 3) + 2;

        let color = if visible {
            self.fg_color
        } else {
            self.bg_color
        };

        // Draw an underline-style cursor
        let cursor_rect = Rectangle::new(
            Point::new(pixel_x as i32, pixel_y as i32),
            Size::new(Self::CURSOR_HEIGHT, Self::CURSOR_WIDTH),
        );
        cursor_rect
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(self)
            .ok();
        Ok(())
    }
}
