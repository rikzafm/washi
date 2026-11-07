//! Tiny scene rasterizer for synthetic test images. Expected values in tests come from the
//! scene parameters (geometry we chose), never from the measurement code.
#![allow(dead_code)]

use uidiff::Image;

pub type Rgb = [u8; 3];

pub const BG: Rgb = [217, 221, 225];
pub const PAPER: Rgb = [255, 255, 253];
pub const INK: Rgb = [43, 47, 54];

/// A layer returns Some(color) where it covers the point (continuous coords, y down).
pub type Layer<'a> = Box<dyn Fn(f64, f64) -> Option<Rgb> + 'a>;

/// Render layers (later layers on top) with 4x4 supersampling.
pub fn render(w: usize, h: usize, bg: Rgb, layers: &[Layer]) -> Image {
    let mut img = Image::new(w, h, bg);
    const N: usize = 4;
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f64; 3];
            for sy in 0..N {
                for sx in 0..N {
                    let fx = x as f64 + (sx as f64 + 0.5) / N as f64;
                    let fy = y as f64 + (sy as f64 + 0.5) / N as f64;
                    let mut c = bg;
                    for l in layers {
                        if let Some(v) = l(fx, fy) {
                            c = v;
                        }
                    }
                    for k in 0..3 {
                        acc[k] += c[k] as f64;
                    }
                }
            }
            let n = (N * N) as f64;
            img.set(x, y, [(acc[0] / n).round() as u8, (acc[1] / n).round() as u8, (acc[2] / n).round() as u8]);
        }
    }
    img
}

/// Rounded rectangle of size w x h, centered at (cx, cy), rotated `deg` clockwise (y down).
pub fn rrect<'a>(cx: f64, cy: f64, w: f64, h: f64, r: f64, deg: f64, color: Rgb) -> Layer<'a> {
    let (s, c) = deg.to_radians().sin_cos();
    Box::new(move |x, y| {
        let (dx, dy) = (x - cx, y - cy);
        // into the rect's own frame (rotate by -deg)
        let u = dx * c + dy * s;
        let v = -dx * s + dy * c;
        let (qx, qy) = (u.abs() - (w / 2.0 - r), v.abs() - (h / 2.0 - r));
        let inside = if qx > 0.0 && qy > 0.0 { qx * qx + qy * qy <= r * r } else { u.abs() <= w / 2.0 && v.abs() <= h / 2.0 };
        inside.then_some(color)
    })
}

/// Axis aligned rectangle [x0,x1) x [y0,y1) (continuous coords).
pub fn rect<'a>(x0: f64, y0: f64, x1: f64, y1: f64, color: Rgb) -> Layer<'a> {
    Box::new(move |x, y| (x >= x0 && x < x1 && y >= y0 && y < y1).then_some(color))
}
