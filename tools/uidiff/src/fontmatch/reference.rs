//! Reference side: take a region of the (tilted) mockup, level it, and turn it into an ink mask.

use super::score::Bitmap;
use crate::image::{lum, Image, Rgb};
use crate::spec::Rect;

/// Copy `rect` out of `img` with the content rotated back by `deg` (the content is tilted clockwise
/// by `deg`, y down) about the rect's top-left corner. Bilinear; the result has the rect's size.
pub fn derotate(img: &Image, rect: Rect, deg: f64) -> Image {
    let (x0, y0, x1, y1) = (rect[0] as f64, rect[1] as f64, rect[2] as f64, rect[3] as f64);
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let (s, c) = deg.to_radians().sin_cos();
    let mut out = Image::new(w, h, [255, 255, 255]);
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f64 + 0.5, y as f64 + 0.5);
            let (sx, sy) = (x0 + dx * c - dy * s, y0 + dx * s + dy * c);
            let v = img.sample_lum(sx, sy).round().clamp(0.0, 255.0) as u8;
            out.set(x, y, [v, v, v]);
        }
    }
    out
}

/// Ink mask of `img`: coverage = (paper - lum) / (paper - ink), ink where it reaches 0.5.
pub fn ink_bitmap(img: &Image, paper: Rgb, ink: Rgb) -> Bitmap {
    let (pl, il) = (lum(paper), lum(ink));
    Bitmap::from_fn(img.w, img.h, |x, y| (pl - img.lum(x, y)) / (pl - il) >= 0.5)
}
