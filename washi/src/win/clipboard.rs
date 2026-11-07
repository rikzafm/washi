//! The clipboard's bitmap (`CF_DIB`), for pasting a screenshot into a note.

use crate::images::Rgba;
use crate::platform::ImageClipboard;
use windows_sys::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

const CF_UNICODETEXT: u32 = 13;
const CF_DIB: u32 = 8;

/// Pictures larger than this are not read from the clipboard (memory).
const MAX_DIB_BYTES: usize = 160 * 1024 * 1024;

pub struct Win32Clipboard;

impl ImageClipboard for Win32Clipboard {
    fn image(&self) -> Option<Rgba> {
        // SAFETY: the clipboard is opened and closed around the reads; the global memory block is
        // locked while it is copied and the copy is what is parsed.
        unsafe {
            if IsClipboardFormatAvailable(CF_UNICODETEXT) != 0 || IsClipboardFormatAvailable(CF_DIB) == 0 {
                return None;
            }
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return None;
            }
            let mut bytes = None;
            let handle = GetClipboardData(CF_DIB);
            if !handle.is_null() {
                let size = GlobalSize(handle);
                let ptr = GlobalLock(handle) as *const u8;
                if !ptr.is_null() {
                    if size > 0 && size <= MAX_DIB_BYTES {
                        bytes = Some(std::slice::from_raw_parts(ptr, size).to_vec());
                    }
                    GlobalUnlock(handle);
                }
            }
            CloseClipboard();
            dib_to_rgba(&bytes?)
        }
    }
}

fn le16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// A packed device-independent bitmap (a `BITMAPINFOHEADER` followed by the pixels), 24 or 32 bits
/// per pixel. The alpha byte of 32-bit clipboard bitmaps is unreliable, so pictures are opaque.
pub(crate) fn dib_to_rgba(dib: &[u8]) -> Option<Rgba> {
    let header = le32(dib, 0)? as usize;
    if header < 40 {
        return None;
    }
    let width = le32(dib, 4)? as i32;
    let height = le32(dib, 8)? as i32;
    let bits = le16(dib, 14)? as usize;
    let compression = le32(dib, 16)?;
    if width <= 0 || height == 0 || !(bits == 24 || bits == 32) || !(compression == 0 || compression == 3) {
        return None;
    }
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    if w.checked_mul(h)? > 100_000_000 {
        return None;
    }
    // BI_BITFIELDS in a 40-byte header keeps three colour masks after it
    let masks = if compression == 3 && header == 40 { 12 } else { 0 };
    let start = header + masks + le32(dib, 32)? as usize * if bits <= 8 { 4 } else { 0 };
    let stride = (w * bits).div_ceil(32) * 4;
    let pixels = dib.get(start..start.checked_add(stride.checked_mul(h)?)?)?;
    let bytes = bits / 8;
    let mut data = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        // a positive height means the rows run bottom to top
        let row = if height > 0 { h - 1 - y } else { y };
        for px in pixels[row * stride..row * stride + w * bytes].chunks_exact(bytes) {
            data.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    Some(Rgba { width: w as u32, height: h as u32, data })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dib(width: i32, height: i32, bits: u16, pixels: &[u8]) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&40u32.to_le_bytes());
        d.extend_from_slice(&width.to_le_bytes());
        d.extend_from_slice(&height.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&bits.to_le_bytes());
        d.extend_from_slice(&[0; 24]); // compression, sizes, resolution, palette
        d.extend_from_slice(pixels);
        d
    }

    #[test]
    fn a_bottom_up_24_bit_bitmap_is_read_top_down_with_rows_padded_to_4_bytes() {
        // 1 x 2, bottom row first: blue (BGR 255,0,0) then red; each row padded from 3 to 4 bytes
        let d = dib(1, 2, 24, &[255, 0, 0, 0, 0, 0, 255, 0]);
        let img = dib_to_rgba(&d).unwrap();
        assert_eq!((img.width, img.height), (1, 2));
        assert_eq!(img.data, [255, 0, 0, 255, 0, 0, 255, 255]);
    }

    #[test]
    fn a_top_down_32_bit_bitmap_is_read_as_is() {
        let d = dib(2, -1, 32, &[0, 255, 0, 0, 10, 20, 30, 0]);
        let img = dib_to_rgba(&d).unwrap();
        assert_eq!(img.data, [0, 255, 0, 255, 30, 20, 10, 255]);
    }

    #[test]
    fn unsupported_or_truncated_bitmaps_are_refused() {
        assert!(dib_to_rgba(&[]).is_none());
        assert!(dib_to_rgba(&dib(2, 2, 24, &[0; 4])).is_none(), "truncated");
        assert!(dib_to_rgba(&dib(2, 2, 8, &[0; 16])).is_none(), "palette bitmaps are not supported");
    }
}
