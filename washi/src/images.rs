//! Pictures in notes: decode, shrink, re-encode (plan 6.3).
//!
//! An image the user adds (a file or the clipboard) is turned into the small encoded form that is
//! stored in the encrypted database. This is the only heavy work Washi does, so [`spawn_prepare`]
//! runs it on a short-lived thread: typing never stalls. Decoded pixels are made again from the
//! stored form only while a note is shown (`decode`), and dropped when it closes or Washi suspends.

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::mpsc;

/// The default text-area width in logical px (the paper's default width minus the text insets).
pub const DEFAULT_TEXT_WIDTH: u32 = 363 - 32 - 40;

/// Stored images fit in a box this many px on a side: twice the default text-area width, which
/// keeps them sharp at 200% display scaling and a decoded image near 1 MB.
pub const MAX_STORED: u32 = 2 * DEFAULT_TEXT_WIDTH;

/// Refuse pictures above this many pixels rather than try to decode them (memory).
const MAX_PIXELS: u64 = 120_000_000;

const JPEG_QUALITY: u8 = 85;

/// Decoded pixels, 8-bit RGBA, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// An image ready to be stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageError(pub String);

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ImageError {}

/// Where a new image comes from.
pub enum Source {
    File(PathBuf),
    Pixels(Rgba),
}

pub const MIME_PNG: &str = "image/png";
pub const MIME_JPEG: &str = "image/jpeg";

/// Decode a PNG or JPEG (found by its first bytes, not its name).
pub fn decode(data: &[u8]) -> Result<Rgba, ImageError> {
    if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        decode_png(data)
    } else if data.starts_with(&[0xFF, 0xD8]) {
        decode_jpeg(data)
    } else {
        Err(ImageError("Only PNG and JPEG pictures can be added.".into()))
    }
}

fn check_size(w: u64, h: u64) -> Result<(), ImageError> {
    if w == 0 || h == 0 || w * h > MAX_PIXELS {
        return Err(ImageError("This picture is too large to add.".into()));
    }
    Ok(())
}

fn decode_png(data: &[u8]) -> Result<Rgba, ImageError> {
    let bad = |e: png::DecodingError| ImageError(format!("This PNG could not be read: {e}"));
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(bad)?;
    let (w, h) = (reader.info().width, reader.info().height);
    check_size(w as u64, h as u64)?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).map_err(bad)?;
    buf.truncate(frame.buffer_size());
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    match frame.color_type {
        png::ColorType::Rgba => out = buf,
        png::ColorType::Rgb => buf.chunks_exact(3).for_each(|p| out.extend_from_slice(&[p[0], p[1], p[2], 255])),
        png::ColorType::Grayscale => buf.iter().for_each(|&g| out.extend_from_slice(&[g, g, g, 255])),
        png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).for_each(|p| out.extend_from_slice(&[p[0], p[0], p[0], p[1]])),
        png::ColorType::Indexed => return Err(ImageError("This PNG could not be read.".into())),
    }
    Ok(Rgba { width: w, height: h, data: out })
}

fn decode_jpeg(data: &[u8]) -> Result<Rgba, ImageError> {
    use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(data), options);
    let bad = |e: zune_jpeg::errors::DecodeErrors| ImageError(format!("This JPEG could not be read: {e:?}"));
    decoder.decode_headers().map_err(bad)?;
    let (w, h) = decoder.dimensions().ok_or_else(|| ImageError("This JPEG could not be read.".into()))?;
    check_size(w as u64, h as u64)?;
    let rgb = decoder.decode().map_err(bad)?;
    if rgb.len() != w * h * 3 {
        return Err(ImageError("This JPEG could not be read.".into()));
    }
    let mut out = Vec::with_capacity(w * h * 4);
    rgb.chunks_exact(3).for_each(|p| out.extend_from_slice(&[p[0], p[1], p[2], 255]));
    Ok(Rgba { width: w as u32, height: h as u32, data: out })
}

/// Shrink to fit `max` x `max`, keeping the aspect ratio; never enlarges. Box filter: every new
/// pixel is the average of the old pixels it covers.
pub fn fit(src: Rgba, max: u32) -> Rgba {
    if src.width <= max && src.height <= max {
        return src;
    }
    let scale = max as f64 / src.width.max(src.height) as f64;
    let dw = ((src.width as f64 * scale).round() as u32).max(1);
    let dh = ((src.height as f64 * scale).round() as u32).max(1);
    let (sw, sh, dw_, dh_) = (src.width as usize, src.height as usize, dw as usize, dh as usize);
    let span = |i: usize, from: usize, to: usize| {
        let a = i * from / to;
        (a, ((i + 1) * from / to).max(a + 1))
    };
    let xs: Vec<(usize, usize)> = (0..dw_).map(|x| span(x, sw, dw_)).collect();
    let mut out = vec![0u8; dw_ * dh_ * 4];
    let mut acc = vec![0u32; dw_ * 4];
    for dy in 0..dh_ {
        let (y0, y1) = span(dy, sh, dh_);
        acc.fill(0);
        for sy in y0..y1 {
            let row = &src.data[sy * sw * 4..(sy + 1) * sw * 4];
            for (dx, &(x0, x1)) in xs.iter().enumerate() {
                let a = &mut acc[dx * 4..dx * 4 + 4];
                for p in row[x0 * 4..x1 * 4].chunks_exact(4) {
                    a[0] += p[0] as u32;
                    a[1] += p[1] as u32;
                    a[2] += p[2] as u32;
                    a[3] += p[3] as u32;
                }
            }
        }
        for (dx, &(x0, x1)) in xs.iter().enumerate() {
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            for c in 0..4 {
                out[(dy * dw_ + dx) * 4 + c] = ((acc[dx * 4 + c] + n / 2) / n) as u8;
            }
        }
    }
    Rgba { width: dw, height: dh, data: out }
}

fn is_opaque(img: &Rgba) -> bool {
    img.data.chunks_exact(4).all(|p| p[3] == 255)
}

/// PNG bytes of the pixels.
pub fn encode_png(img: &Rgba) -> Result<Vec<u8>, ImageError> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, img.width, img.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Default);
    let bad = |e: png::EncodingError| ImageError(format!("The picture could not be encoded: {e}"));
    enc.write_header().map_err(bad)?.write_image_data(&img.data).map_err(bad)?;
    Ok(out)
}

fn encode_jpeg(img: &Rgba) -> Result<Vec<u8>, ImageError> {
    let rgb: Vec<u8> = img.data.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, JPEG_QUALITY)
        .encode(&rgb, img.width as u16, img.height as u16, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| ImageError(format!("The picture could not be encoded: {e}")))?;
    Ok(out)
}

/// The stored form of already decoded pixels: shrunk, then whichever of PNG and JPEG (opaque
/// pictures only) is smaller. Screenshots come out as PNG, photos as JPEG.
pub fn store(img: Rgba) -> Result<Stored, ImageError> {
    let img = fit(img, MAX_STORED);
    let png = encode_png(&img)?;
    if is_opaque(&img) {
        let jpeg = encode_jpeg(&img)?;
        if jpeg.len() < png.len() {
            return Ok(Stored { mime: MIME_JPEG, width: img.width, height: img.height, data: jpeg });
        }
    }
    Ok(Stored { mime: MIME_PNG, width: img.width, height: img.height, data: png })
}

/// Everything between "the user chose a picture" and "bytes to store".
pub fn prepare(source: Source) -> Result<Stored, ImageError> {
    let pixels = match source {
        Source::Pixels(p) => p,
        Source::File(path) => {
            let data = std::fs::read(&path).map_err(|e| ImageError(format!("The picture could not be opened: {e}")))?;
            decode(&data)?
        }
    };
    check_size(pixels.width as u64, pixels.height as u64)?;
    store(pixels)
}

/// Run [`prepare`] on its own thread; the result arrives on the returned channel. The thread
/// ends when it is done. Fails if the system will not start a thread.
pub fn spawn_prepare(source: Source) -> Result<mpsc::Receiver<Result<Stored, ImageError>>, ImageError> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("washi-image".into())
        .spawn(move || {
            let _ = tx.send(prepare(source));
        })
        .map_err(|e| ImageError(format!("The picture could not be prepared: {e}")))?;
    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A photograph-like picture: smooth colour fields, soft shapes, fine sensor noise.
    pub fn photo(w: u32, h: u32) -> Rgba {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        let mut seed = 12345u32;
        for y in 0..h {
            for x in 0..w {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let noise = ((seed >> 24) % 9) as i32 - 4;
                let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
                let r = 90.0 + 120.0 * fx + 30.0 * (fy * 9.0).sin();
                let g = 160.0 - 70.0 * fy + 25.0 * (fx * 7.0).cos();
                let b = 120.0 + 60.0 * (fx * fy) + 20.0 * ((fx + fy) * 5.0).sin();
                let c = |v: f32| (v as i32 + noise).clamp(0, 255) as u8;
                data.extend_from_slice(&[c(r), c(g), c(b), 255]);
            }
        }
        Rgba { width: w, height: h, data }
    }

    pub fn jpeg_of(img: &Rgba, quality: u8) -> Vec<u8> {
        let rgb: Vec<u8> = img.data.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        let mut out = Vec::new();
        jpeg_encoder::Encoder::new(&mut out, quality).encode(&rgb, img.width as u16, img.height as u16, jpeg_encoder::ColorType::Rgb).unwrap();
        out
    }

    #[test]
    fn a_12_megapixel_jpeg_is_stored_in_about_100_kb() {
        let big = photo(4000, 3000);
        let jpeg = jpeg_of(&big, 92);
        assert!(jpeg.len() > 1_000_000, "the source is a real photo-sized file");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.jpg");
        std::fs::write(&path, &jpeg).unwrap();

        let stored = prepare(Source::File(path)).unwrap();
        assert!(stored.width <= MAX_STORED && stored.height <= MAX_STORED);
        assert_eq!((stored.width, stored.height), (MAX_STORED, MAX_STORED * 3 / 4));
        assert!(stored.data.len() <= 100 * 1024, "stored {} bytes", stored.data.len());
        // and it is still the picture
        let back = decode(&stored.data).unwrap();
        assert_eq!((back.width, back.height), (stored.width, stored.height));
    }

    #[test]
    fn small_pictures_are_not_enlarged_and_screenshots_stay_png() {
        // flat colour blocks: PNG is far smaller than JPEG
        let (w, h) = (200u32, 100u32);
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let c = if (x / 20 + y / 20) % 2 == 0 { [250, 250, 250, 255] } else { [20, 90, 200, 255] };
                data.extend_from_slice(&c);
            }
        }
        let shot = Rgba { width: w, height: h, data };
        let stored = store(shot.clone()).unwrap();
        assert_eq!((stored.width, stored.height, stored.mime), (w, h, MIME_PNG));
        assert_eq!(decode(&stored.data).unwrap(), shot, "PNG storage is lossless");
    }

    #[test]
    fn transparency_is_kept() {
        let img = Rgba { width: 2, height: 1, data: vec![255, 0, 0, 255, 0, 0, 255, 0] };
        let stored = store(img.clone()).unwrap();
        assert_eq!(stored.mime, MIME_PNG);
        assert_eq!(decode(&stored.data).unwrap(), img);
    }

    #[test]
    fn the_box_filter_averages() {
        let img = Rgba { width: 4, height: 2, data: [[0u8, 0, 0, 255], [100, 100, 100, 255], [200, 0, 0, 255], [200, 0, 0, 255]].iter().cycle().take(8).flatten().copied().collect() };
        let small = fit(img, 2);
        assert_eq!((small.width, small.height), (2, 1));
        // left pixel covers (0,0,0) and (100,100,100) twice each
        assert_eq!(&small.data[0..4], &[50, 50, 50, 255]);
        assert_eq!(&small.data[4..8], &[200, 0, 0, 255]);
    }

    #[test]
    fn what_is_not_a_png_or_jpeg_is_refused_politely() {
        assert!(decode(b"GIF89a....").is_err());
        assert!(decode(&[]).is_err());
        assert!(decode(&[0xFF, 0xD8, 1, 2, 3]).is_err());
        assert!(decode(&[0x89, b'P', b'N', b'G', 1, 2, 3]).is_err());
    }

    #[test]
    fn the_worker_thread_delivers_the_result() {
        let rx = spawn_prepare(Source::Pixels(photo(64, 48))).unwrap();
        let stored = rx.recv().unwrap().unwrap();
        assert_eq!((stored.width, stored.height), (64, 48));
    }
}
