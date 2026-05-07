//! BMP image parser and framebuffer renderer.
//!
//! Parses BMP files (8-bit indexed, 24-bit RGB, 32-bit RGBA) without compression
//! and renders them to the framebuffer centered on the screen.
//!
//! On invalid BMP data, the framebuffer is cleared to black.

#![allow(unused)]
// Certain structs are unused in this module but may be useful for future extensions (e.g., supporting more BMP features).

use crate::RawFrameBufferInfo;
use bootloader_api::info::PixelFormat;
use core::ptr;

/// Error type for BMP parsing and rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BmpError {
    /// Invalid BMP signature (not "BM").
    InvalidSignature,
    /// File size is too small for a valid BMP.
    FileTooSmall,
    /// Unsupported or invalid DIB header size.
    InvalidDibHeader,
    /// Compression is not supported (only uncompressed BMPs allowed).
    CompressionNotSupported,
    /// Invalid pixel data offset.
    InvalidPixelDataOffset,
    /// Unsupported bits-per-pixel value.
    UnsupportedBitDepth,
    /// Invalid color palette.
    InvalidPalette,
    /// Framebuffer access error.
    FramebufferError,
}

/// BMP file header (14 bytes).
#[repr(C)]
struct BmpFileHeader {
    /// Magic number: 0x4D42 ("BM").
    signature: u16,
    /// File size in bytes.
    file_size: u32,
    /// Reserved field (usually 0).
    _reserved: u32,
    /// Offset to pixel data from start of file.
    pixel_data_offset: u32,
}

/// DIB header structure (BITMAPINFOHEADER, 40 bytes).
#[repr(C)]
struct BmpDibHeader {
    /// Size of this header (typically 40).
    header_size: u32,
    /// Width in pixels (can be negative for horizontal flip).
    width: i32,
    /// Height in pixels (negative = top-down, positive = bottom-up).
    height: i32,
    /// Number of planes (always 1).
    planes: u16,
    /// Bits per pixel (1, 4, 8, 16, 24, 32).
    bits_per_pixel: u16,
    /// Compression method (0 = none, others not supported).
    compression: u32,
    /// Image size (can be 0 for uncompressed).
    image_size: u32,
    /// Horizontal resolution in pixels per meter.
    _x_pixels_per_meter: i32,
    /// Vertical resolution in pixels per meter.
    _y_pixels_per_meter: i32,
    /// Number of colors in palette (0 = default for bit depth).
    colors_used: u32,
    /// Number of important colors (0 = all).
    _colors_important: u32,
}

/// Parsed BMP metadata.
struct BmpMetadata {
    /// Image width in pixels.
    width: usize,
    /// Image height in pixels (always positive, top-down assumed if originally bottom-up).
    height: usize,
    /// Bits per pixel (8, 24, or 32).
    bits_per_pixel: u16,
    /// Offset to pixel data from start of file.
    pixel_data_offset: usize,
    /// Offset to color palette from start of file (for 8-bit images).
    palette_offset: usize,
    /// Number of palette entries.
    palette_entries: usize,
    /// Whether the BMP is stored bottom-up (original height > 0).
    is_bottom_up: bool,
}

/// Parse BMP file header and DIB header.
fn parse_bmp_headers(bmp_slice: &[u8]) -> Result<BmpMetadata, BmpError> {
    // Check minimum file size for headers.
    if bmp_slice.len() < 54 {
        return Err(BmpError::FileTooSmall);
    }

    // Parse BMP file header (14 bytes).
    let signature = u16::from_le_bytes([bmp_slice[0], bmp_slice[1]]);
    if signature != 0x4D42 {
        // Not "BM"
        return Err(BmpError::InvalidSignature);
    }

    let _file_size = u32::from_le_bytes([bmp_slice[2], bmp_slice[3], bmp_slice[4], bmp_slice[5]]);
    let pixel_data_offset =
        u32::from_le_bytes([bmp_slice[10], bmp_slice[11], bmp_slice[12], bmp_slice[13]]) as usize;

    if pixel_data_offset > bmp_slice.len() {
        return Err(BmpError::InvalidPixelDataOffset);
    }

    // Parse DIB header (starting at offset 14).
    let dib_header_size =
        u32::from_le_bytes([bmp_slice[14], bmp_slice[15], bmp_slice[16], bmp_slice[17]]);

    if dib_header_size < 40 {
        return Err(BmpError::InvalidDibHeader);
    }

    let width = i32::from_le_bytes([bmp_slice[18], bmp_slice[19], bmp_slice[20], bmp_slice[21]]);
    let height = i32::from_le_bytes([bmp_slice[22], bmp_slice[23], bmp_slice[24], bmp_slice[25]]);

    if width <= 0 || height == 0 {
        return Err(BmpError::InvalidDibHeader);
    }

    let planes = u16::from_le_bytes([bmp_slice[26], bmp_slice[27]]);
    if planes != 1 {
        return Err(BmpError::InvalidDibHeader);
    }

    let bits_per_pixel = u16::from_le_bytes([bmp_slice[28], bmp_slice[29]]);
    let compression =
        u32::from_le_bytes([bmp_slice[30], bmp_slice[31], bmp_slice[32], bmp_slice[33]]);

    if compression != 0 {
        return Err(BmpError::CompressionNotSupported);
    }

    match bits_per_pixel {
        8 | 24 | 32 => {}
        _ => return Err(BmpError::UnsupportedBitDepth),
    }

    let colors_used =
        u32::from_le_bytes([bmp_slice[46], bmp_slice[47], bmp_slice[48], bmp_slice[49]]);

    let is_bottom_up = height > 0;
    let height_abs = height.abs() as usize;
    let width_abs = width as usize;

    // Calculate palette offset and size.
    let dib_header_size_usize = dib_header_size as usize;
    let palette_offset = 14_usize
        .checked_add(dib_header_size_usize)
        .ok_or(BmpError::InvalidDibHeader)?;
    if palette_offset > bmp_slice.len() {
        return Err(BmpError::InvalidDibHeader);
    }
    let palette_entries = if bits_per_pixel == 8 {
        if colors_used > 0 {
            colors_used as usize
        } else {
            256 // Default for 8-bit.
        }
    } else {
        0
    };

    Ok(BmpMetadata {
        width: width_abs,
        height: height_abs,
        bits_per_pixel,
        pixel_data_offset,
        palette_offset,
        palette_entries,
        is_bottom_up,
    })
}

/// Read a 32-bit BGRA color from palette at index.
fn read_palette_entry(bmp_slice: &[u8], palette_offset: usize, index: usize) -> (u8, u8, u8) {
    let offset = match index
        .checked_mul(4)
        .and_then(|o| palette_offset.checked_add(o))
    {
        Some(o) => o,
        None => return (0, 0, 0), // Overflow or out of bounds, return black.
    };
    if offset
        .checked_add(4)
        .map_or(true, |end| end > bmp_slice.len())
    {
        return (0, 0, 0); // Out of bounds, return black.
    }
    let b = bmp_slice[offset];
    let g = bmp_slice[offset + 1];
    let r = bmp_slice[offset + 2];
    // Skip alpha channel (offset + 3).
    (r, g, b)
}

/// Convert RGB color to framebuffer format and write to buffer.
fn write_pixel(
    fb_buffer: &mut [u8],
    offset: usize,
    r: u8,
    g: u8,
    b: u8,
    pixel_format: PixelFormat,
    bytes_per_pixel: usize,
) {
    if offset + bytes_per_pixel > fb_buffer.len() {
        return; // Out of bounds, skip.
    }

    match pixel_format {
        PixelFormat::Rgb => {
            fb_buffer[offset] = r;
            fb_buffer[offset + 1] = g;
            fb_buffer[offset + 2] = b;
            if bytes_per_pixel == 4 {
                fb_buffer[offset + 3] = 0; // Alpha or padding.
            }
        }
        PixelFormat::Bgr => {
            fb_buffer[offset] = b;
            fb_buffer[offset + 1] = g;
            fb_buffer[offset + 2] = r;
            if bytes_per_pixel == 4 {
                fb_buffer[offset + 3] = 0; // Alpha or padding.
            }
        }
        PixelFormat::U8 => {
            // Convert RGB to grayscale using standard luminance formula.
            let gray = ((r as u16 * 299 + g as u16 * 587 + b as u16 * 114) / 1000) as u8;
            fb_buffer[offset] = gray;
        }
        _ => {
            // Fallback to RGB for unknown formats.
            fb_buffer[offset] = r;
            if bytes_per_pixel > 1 {
                fb_buffer[offset + 1] = g;
            }
            if bytes_per_pixel > 2 {
                fb_buffer[offset + 2] = b;
            }
        }
    }
}

/// Clear framebuffer to black.
fn clear_framebuffer_black(fb_info: &RawFrameBufferInfo) {
    unsafe {
        let fb_buffer =
            ptr::slice_from_raw_parts_mut(fb_info.addr.as_u64() as *mut u8, fb_info.info.byte_len);
        if !fb_buffer.is_null() {
            (*fb_buffer).fill(0);
        }
    }
}

/// Draw a BMP image onto the framebuffer starting at pixel (0, 0).
///
/// Parses a BMP file and renders it to the framebuffer. Supports 8-bit indexed,
/// 24-bit RGB, and 32-bit RGBA BMP images without compression.
///
/// On error, the framebuffer is cleared to black.
///
/// # Arguments
///
/// * `bmp_slice` - Byte slice containing the complete BMP file data.
/// * `fb_info` - Framebuffer information including address and layout.
///
/// # Safety
///
/// This function is unsafe because it dereferences the raw framebuffer pointer.
/// The caller must ensure that `fb_info.addr` points to valid, writable framebuffer memory.
pub unsafe fn draw_bmp(bmp_slice: &[u8], fb_info: &RawFrameBufferInfo) {
    // Parse BMP headers.
    let metadata = match parse_bmp_headers(bmp_slice) {
        Ok(m) => m,
        Err(_) => {
            clear_framebuffer_black(fb_info);
            return;
        }
    };

    // Validate metadata against framebuffer bounds.
    if metadata.width == 0 || metadata.height == 0 {
        clear_framebuffer_black(fb_info);
        return;
    }

    // Get mutable access to framebuffer.
    let fb_buffer = {
        let fb_ptr = fb_info.addr.as_u64() as *mut u8;
        if fb_ptr.is_null() {
            clear_framebuffer_black(fb_info);
            return;
        }
        unsafe { ptr::slice_from_raw_parts_mut(fb_ptr, fb_info.info.byte_len).as_mut() }
    };

    let fb_buffer = match fb_buffer {
        Some(buf) => buf,
        None => {
            clear_framebuffer_black(fb_info);
            return;
        }
    };

    let fb_width = fb_info.info.width;
    let fb_height = fb_info.info.height;
    let fb_stride = fb_info.info.stride;
    let fb_bpp = fb_info.info.bytes_per_pixel;
    let fb_format = fb_info.info.pixel_format;

    // Calculate offsets to center the BMP on the screen
    let offset_x = ((fb_width as isize - metadata.width as isize) / 2).max(0) as usize;
    let offset_y = ((fb_height as isize - metadata.height as isize) / 2).max(0) as usize;

    // Calculate starting positions in BMP for proper centering when BMP is larger than screen
    let bmp_start_x = if offset_x == 0 && metadata.width > fb_width {
        (metadata.width - fb_width) / 2
    } else {
        0
    };
    let bmp_start_y = if offset_y == 0 && metadata.height > fb_height {
        (metadata.height - fb_height) / 2
    } else {
        0
    };

    // Calculate BMP row stride (padded to 4-byte boundary).
    let bmp_bytes_per_pixel = metadata.bits_per_pixel / 8;
    let bmp_row_bytes = metadata
        .width
        .checked_mul(bmp_bytes_per_pixel as usize)
        .unwrap_or(usize::MAX);
    let bmp_row_stride = (bmp_row_bytes + 3) & !3; // Pad to 4-byte boundary.
    if bmp_row_stride == usize::MAX {
        clear_framebuffer_black(fb_info);
        return;
    }

    clear_framebuffer_black(fb_info);

    // Iterate over BMP rows.
    for y in bmp_start_y..metadata.height {
        let screen_y = if offset_y > 0 {
            offset_y + (y - bmp_start_y)
        } else {
            y - bmp_start_y
        };
        if screen_y >= fb_height {
            break; // Image exceeds framebuffer height.
        }

        // Calculate row offsets.
        // BMP is stored bottom-up if is_bottom_up is true.
        let bmp_row_idx = if metadata.is_bottom_up {
            metadata
                .height
                .checked_sub(1)
                .and_then(|v| v.checked_sub(y))
        } else {
            Some(y)
        };
        let bmp_row_idx = match bmp_row_idx {
            Some(idx) => idx,
            None => break, // Invalid row index, stop processing.
        };
        let bmp_row_offset = match bmp_row_idx
            .checked_mul(bmp_row_stride)
            .and_then(|o| metadata.pixel_data_offset.checked_add(o))
        {
            Some(o) => o,
            None => break, // Overflow in offset calculation, stop processing.
        };

        // Iterate over pixels in this row.
        for x in bmp_start_x..metadata.width {
            let screen_x = if offset_x > 0 {
                offset_x + (x - bmp_start_x)
            } else {
                x - bmp_start_x
            };
            if screen_x >= fb_width {
                break; // Image exceeds framebuffer width.
            }

            let (r, g, b) = match metadata.bits_per_pixel {
                8 => {
                    // Indexed color: read palette index, then lookup color.
                    let pixel_offset = match bmp_row_offset.checked_add(x) {
                        Some(o) => o,
                        None => break, // Overflow, stop processing row.
                    };
                    if pixel_offset >= bmp_slice.len() {
                        (0, 0, 0)
                    } else {
                        let index = bmp_slice[pixel_offset] as usize;
                        read_palette_entry(bmp_slice, metadata.palette_offset, index)
                    }
                }
                24 => {
                    // 24-bit BGR.
                    let pixel_offset =
                        match x.checked_mul(3).and_then(|o| bmp_row_offset.checked_add(o)) {
                            Some(o) => o,
                            None => break, // Overflow, stop processing row.
                        };
                    if pixel_offset
                        .checked_add(3)
                        .map_or(true, |end| end > bmp_slice.len())
                    {
                        (0, 0, 0)
                    } else {
                        let b = bmp_slice[pixel_offset];
                        let g = bmp_slice[pixel_offset + 1];
                        let r = bmp_slice[pixel_offset + 2];
                        (r, g, b)
                    }
                }
                32 => {
                    // 32-bit BGRA (skip alpha).
                    let pixel_offset =
                        match x.checked_mul(4).and_then(|o| bmp_row_offset.checked_add(o)) {
                            Some(o) => o,
                            None => break, // Overflow, stop processing row.
                        };
                    if pixel_offset
                        .checked_add(4)
                        .map_or(true, |end| end > bmp_slice.len())
                    {
                        (0, 0, 0)
                    } else {
                        let b = bmp_slice[pixel_offset];
                        let g = bmp_slice[pixel_offset + 1];
                        let r = bmp_slice[pixel_offset + 2];
                        // Skip alpha: bmp_slice[pixel_offset + 3]
                        (r, g, b)
                    }
                }
                _ => (0, 0, 0), // Shouldn't reach here due to validation.
            };

            // Write pixel to framebuffer.
            let fb_offset = match screen_y
                .checked_mul(fb_stride)
                .and_then(|row_offset| row_offset.checked_add(screen_x))
                .and_then(|pixel_pos| pixel_pos.checked_mul(fb_bpp))
            {
                Some(o) => o,
                None => continue, // Overflow in offset calculation, skip this pixel.
            };
            write_pixel(fb_buffer, fb_offset, r, g, b, fb_format, fb_bpp);
        }
    }

    let mut pause = 0;
    while pause != u16::MAX as u64 * 4 {
        pause += 1;
        core::hint::spin_loop();
    }
}
