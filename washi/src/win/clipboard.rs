//! The clipboard's bitmap (`CF_DIB`) and rich HTML (`CF_HTML`), for images and formatted text.

use crate::images::Rgba;
use crate::platform::{ClipboardPayload, ImageClipboard};
use windows_sys::Win32::Foundation::GlobalFree;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};

const CF_UNICODETEXT: u32 = 13;
const CF_DIB: u32 = 8;

/// Pictures larger than this are not read from the clipboard (memory).
const MAX_DIB_BYTES: usize = 160 * 1024 * 1024;

pub struct Win32Clipboard;

/// Holds the open clipboard; closes it on every way out.
struct OpenClipboardGuard;

impl OpenClipboardGuard {
    fn open() -> Option<Self> {
        // SAFETY: a null owner window opens the clipboard for the current task.
        (unsafe { OpenClipboard(std::ptr::null_mut()) } != 0).then_some(OpenClipboardGuard)
    }
}

impl Drop for OpenClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: this guard opened the clipboard.
        unsafe { CloseClipboard() };
    }
}

pub(crate) fn cf_html() -> u32 {
    static CF: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let id = CF.load(std::sync::atomic::Ordering::Relaxed);
    if id != 0 {
        return id;
    }
    let name: Vec<u16> = "HTML Format\0".encode_utf16().collect();
    let registered = unsafe { RegisterClipboardFormatW(name.as_ptr()) };
    CF.store(registered, std::sync::atomic::Ordering::Relaxed);
    registered
}

unsafe fn set_clipboard_bytes(format: u32, bytes: &[u8]) -> bool {
    let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
    if handle.is_null() {
        return false;
    }
    let ptr = GlobalLock(handle) as *mut u8;
    if ptr.is_null() {
        GlobalFree(handle);
        return false;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    GlobalUnlock(handle);
    if SetClipboardData(format, handle).is_null() {
        GlobalFree(handle);
        return false;
    }
    true
}

unsafe fn set_clipboard_text_utf16(text: &str) -> bool {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
    set_clipboard_bytes(CF_UNICODETEXT, bytes)
}

impl ImageClipboard for Win32Clipboard {
    fn image(&self) -> Option<Rgba> {
        // SAFETY: the clipboard is opened and closed around the reads; the global memory block is
        // locked while it is copied and the copy is what is parsed.
        unsafe {
            if IsClipboardFormatAvailable(CF_UNICODETEXT) != 0 || IsClipboardFormatAvailable(CF_DIB) == 0 {
                return None;
            }
            let _open = OpenClipboardGuard::open()?;
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
            dib_to_rgba(&bytes?)
        }
    }

    fn set_image(&self, img: &Rgba) -> Result<(), String> {
        let dib = rgba_to_dib(img);
        let png = crate::images::encode_png(img).map_err(|e| e.to_string())?;
        use base64::prelude::*;
        let b64 = BASE64_STANDARD.encode(&png);
        let html_fragment = format!(r#"<img src="data:image/png;base64,{b64}" width="{}" height="{}" />"#, img.width, img.height);
        let cf_html_data = wrap_cf_html(&html_fragment);

        unsafe {
            let _open = OpenClipboardGuard::open().ok_or_else(|| "Could not open clipboard".to_string())?;
            EmptyClipboard();
            set_clipboard_bytes(CF_DIB, &dib);
            let format_html = cf_html();
            if format_html != 0 {
                set_clipboard_bytes(format_html, &cf_html_data);
            }
        }
        Ok(())
    }

    fn set_rich(&self, payload: ClipboardPayload) -> Result<(), String> {
        let _open = OpenClipboardGuard::open().ok_or_else(|| "Could not open clipboard".to_string())?;
        unsafe {
            EmptyClipboard();
            if let Some(ref img) = payload.image {
                let dib = rgba_to_dib(img);
                set_clipboard_bytes(CF_DIB, &dib);
            }
            if let Some(ref html) = payload.html {
                let cf_html_data = wrap_cf_html(html);
                let format_html = cf_html();
                if format_html != 0 {
                    set_clipboard_bytes(format_html, &cf_html_data);
                }
            }
            if let Some(ref plain) = payload.plain {
                set_clipboard_text_utf16(plain);
            }
        }
        Ok(())
    }

    fn html(&self) -> Option<String> {
        let format = cf_html();
        if format == 0 {
            return None;
        }
        unsafe {
            if IsClipboardFormatAvailable(format) == 0 {
                return None;
            }
            let _open = OpenClipboardGuard::open()?;
            let handle = GetClipboardData(format);
            if handle.is_null() {
                return None;
            }
            let size = GlobalSize(handle);
            let ptr = GlobalLock(handle) as *const u8;
            if ptr.is_null() {
                return None;
            }
            let bytes = if size > 0 && size <= MAX_DIB_BYTES {
                Some(std::slice::from_raw_parts(ptr, size).to_vec())
            } else {
                None
            };
            GlobalUnlock(handle);
            unwrap_cf_html(&bytes?)
        }
    }

    fn text(&self) -> Option<String> {
        unsafe {
            if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
                return None;
            }
            let _open = OpenClipboardGuard::open()?;
            let handle = GetClipboardData(CF_UNICODETEXT);
            if handle.is_null() {
                return None;
            }
            let size = GlobalSize(handle);
            let ptr = GlobalLock(handle) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let len = size / 2;
            let slice = std::slice::from_raw_parts(ptr, len);
            let end = slice.iter().position(|&c| c == 0).unwrap_or(len);
            let text = String::from_utf16(&slice[..end]).ok();
            GlobalUnlock(handle);
            text
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

/// A packed device-independent bitmap (a `BITMAPINFOHEADER` followed by 32-bit BGRA pixels),
/// formatted bottom-up for Windows clipboard (`CF_DIB`).
pub(crate) fn rgba_to_dib(img: &Rgba) -> Vec<u8> {
    let (w, h) = (img.width as usize, img.height as usize);
    let stride = w * 4;
    let image_size = stride.checked_mul(h).unwrap_or(0);
    let header_size = 40usize;
    let mut out = Vec::with_capacity(header_size + image_size);

    out.extend_from_slice(&(header_size as u32).to_le_bytes()); // biSize
    out.extend_from_slice(&(img.width as i32).to_le_bytes());  // biWidth
    out.extend_from_slice(&(img.height as i32).to_le_bytes()); // biHeight (positive = bottom-up)
    out.extend_from_slice(&1u16.to_le_bytes());                // biPlanes
    out.extend_from_slice(&32u16.to_le_bytes());               // biBitCount
    out.extend_from_slice(&0u32.to_le_bytes());                // biCompression = BI_RGB
    out.extend_from_slice(&(image_size as u32).to_le_bytes()); // biSizeImage
    out.extend_from_slice(&0i32.to_le_bytes());                // biXPelsPerMeter
    out.extend_from_slice(&0i32.to_le_bytes());                // biYPelsPerMeter
    out.extend_from_slice(&0u32.to_le_bytes());                // biClrUsed
    out.extend_from_slice(&0u32.to_le_bytes());                // biClrImportant

    // In a bottom-up DIB, row 0 in the file corresponds to the bottom row of the image (h - 1).
    for y_dib in 0..h {
        let y_img = h - 1 - y_dib;
        let start = y_img * stride;
        if let Some(row_slice) = img.data.get(start..start + stride) {
            for px in row_slice.chunks_exact(4) {
                // RGBA -> BGRA
                out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
            }
        }
    }
    out
}

/// Wrap an HTML fragment into Windows CF_HTML clipboard format.
pub fn wrap_cf_html(fragment: &str) -> Vec<u8> {
    const START_HTML: usize = 105;
    const START_FRAGMENT: usize = 139;
    let end_fragment = START_FRAGMENT + fragment.len();
    let end_html = end_fragment + 34; // "<!--EndFragment-->\r\n</body></html>".len()

    let header = format!(
        "Version:0.9\r\nStartHTML:{:010}\r\nEndHTML:{:010}\r\nStartFragment:{:010}\r\nEndFragment:{:010}\r\n<html><body>\r\n<!--StartFragment-->",
        START_HTML, end_html, START_FRAGMENT, end_fragment
    );

    let mut out = Vec::with_capacity(header.len() + fragment.len() + 35);
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(fragment.as_bytes());
    out.extend_from_slice(b"<!--EndFragment-->\r\n</body></html>\0");
    out
}

/// Extract HTML fragment from Windows CF_HTML buffer.
pub fn unwrap_cf_html(cf_html: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(cf_html).ok()?;
    let mut start_fragment = None;
    let mut end_fragment = None;
    for line in s.lines() {
        if let Some(v) = line.strip_prefix("StartFragment:") {
            start_fragment = v.trim().parse::<usize>().ok();
        } else if let Some(v) = line.strip_prefix("EndFragment:") {
            end_fragment = v.trim().parse::<usize>().ok();
        }
    }
    if let (Some(start), Some(end)) = (start_fragment, end_fragment) {
        if start <= end && end <= cf_html.len() {
            return String::from_utf8(cf_html[start..end].to_vec()).ok();
        }
    }
    // Fallback: look for markers directly
    let start_tag = "<!--StartFragment-->";
    let end_tag = "<!--EndFragment-->";
    let start = s.find(start_tag)? + start_tag.len();
    let end = s[start..].find(end_tag)? + start;
    Some(s[start..end].to_string())
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

    #[test]
    fn rgba_to_dib_roundtrips_cleanly_through_dib_to_rgba() {
        let original = Rgba {
            width: 2,
            height: 2,
            data: vec![
                255, 0, 0, 255,   0, 255, 0, 255, // top row: red, green
                0, 0, 255, 255,   128, 64, 32, 255, // bottom row: blue, brown
            ],
        };
        let dib = rgba_to_dib(&original);
        let decoded = dib_to_rgba(&dib).expect("decoded dib");
        assert_eq!((decoded.width, decoded.height), (original.width, original.height));
        assert_eq!(decoded.data, original.data);
    }

    #[test]
    fn wrap_cf_html_roundtrips_cleanly_through_unwrap_cf_html() {
        let fragment = "<b>Hello</b> <i>World</i>";
        let wrapped = wrap_cf_html(fragment);
        let unwrapped = unwrap_cf_html(&wrapped).expect("unwrapped html");
        assert_eq!(unwrapped, fragment);
    }
}
