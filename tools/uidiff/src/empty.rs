//! Empty region: any pixel that is not paper.

use crate::image::*;
use crate::spec::Rect;
use serde_json::{json, Value};

pub fn clamp_rect(img: &Image, r: &Rect) -> (usize, usize, usize, usize) {
    let c = |v: i64, max: usize| v.clamp(0, max as i64) as usize;
    (c(r[0], img.w), c(r[1], img.h), c(r[2], img.w), c(r[3], img.h))
}

pub fn measure(img: &Image, rect: &Rect, color: &str, tol: u8) -> Value {
    let paper = parse_hex(color).expect("paper color");
    let (x0, y0, x1, y1) = clamp_rect(img, rect);
    let (mut bx0, mut by0, mut bx1, mut by1, mut n) = (usize::MAX, usize::MAX, 0, 0, 0usize);
    for y in y0..y1 {
        for x in x0..x1 {
            if max_diff(img.get(x, y), paper) > tol {
                bx0 = bx0.min(x);
                by0 = by0.min(y);
                bx1 = bx1.max(x + 1);
                by1 = by1.max(y + 1);
                n += 1;
            }
        }
    }
    let bbox = if n == 0 { Value::Null } else { json!([bx0, by0, bx1, by1]) };
    json!({ "rect": rect, "non_paper_px": n, "bbox": bbox })
}
