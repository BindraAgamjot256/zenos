use embedded_graphics::pixelcolor::{Bgr888, Gray8, GrayColor, Rgb888, RgbColor};

/// Trait for converting between different pixel formats
pub(crate) trait PixelConverter {
    type Color;

    fn write_to_buffer(&self, color: Self::Color, buffer: &mut [u8], offset: usize);
    fn create_color(r: u8, g: u8, b: u8) -> Self::Color;
}

pub(crate) struct Rgb888Converter;
pub(crate) struct Bgr888Converter;
pub(crate) struct Gray8Converter;

impl PixelConverter for Rgb888Converter {
    type Color = Rgb888;

    fn write_to_buffer(&self, color: Self::Color, buffer: &mut [u8], offset: usize) {
        buffer[offset] = color.r();
        buffer[offset + 1] = color.g();
        buffer[offset + 2] = color.b();
    }

    fn create_color(r: u8, g: u8, b: u8) -> Self::Color {
        Rgb888::new(r, g, b)
    }
}

impl PixelConverter for Bgr888Converter {
    type Color = Bgr888;

    fn write_to_buffer(&self, color: Self::Color, buffer: &mut [u8], offset: usize) {
        buffer[offset] = color.b();
        buffer[offset + 1] = color.g();
        buffer[offset + 2] = color.r();
    }

    fn create_color(r: u8, g: u8, b: u8) -> Self::Color {
        Bgr888::new(r, g, b)
    }
}

impl PixelConverter for Gray8Converter {
    type Color = Gray8;

    fn write_to_buffer(&self, color: Self::Color, buffer: &mut [u8], offset: usize) {
        // Convert to grayscale using the standard luminance formula
        let gray =
            ((color.luma() as u16 * 299 + color.luma() as u16 * 587 + color.luma() as u16 * 114)
                / 1000) as u8;
        buffer[offset] = gray;
    }

    fn create_color(r: u8, g: u8, b: u8) -> Self::Color {
        // Convert RGB to grayscale
        let gray = ((r as u16 * 299 + g as u16 * 587 + b as u16 * 114) / 1000) as u8;
        Gray8::new(gray)
    }
}
