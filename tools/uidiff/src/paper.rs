//! Paper: mask bbox, four fitted edge lines (-> tilt, size), corner radius.

use crate::image::*;
use serde_json::{json, Value};

/// Line v = a + b*u.
#[derive(Clone, Copy)]
struct Line {
    a: f64,
    b: f64,
}

fn fit(points: &[(f64, f64)]) -> Option<Line> {
    let mut pts = points.to_vec();
    for _ in 0..3 {
        if pts.len() < 8 {
            return None;
        }
        let n = pts.len() as f64;
        let (su, sv): (f64, f64) = pts.iter().fold((0.0, 0.0), |s, p| (s.0 + p.0, s.1 + p.1));
        let (mu, mv) = (su / n, sv / n);
        let sxx: f64 = pts.iter().map(|p| (p.0 - mu).powi(2)).sum();
        let sxy: f64 = pts.iter().map(|p| (p.0 - mu) * (p.1 - mv)).sum();
        let b = sxy / sxx;
        let a = mv - b * mu;
        let res: Vec<f64> = pts.iter().map(|p| (p.1 - (a + b * p.0)).abs()).collect();
        let sigma = (res.iter().map(|r| r * r).sum::<f64>() / n).sqrt();
        let cut = (2.0 * sigma).max(0.4);
        let kept: Vec<(f64, f64)> = pts.iter().zip(&res).filter(|(_, r)| **r <= cut).map(|(p, _)| *p).collect();
        if kept.len() == pts.len() {
            return Some(Line { a, b });
        }
        pts = kept;
    }
    let n = pts.len() as f64;
    let (su, sv): (f64, f64) = pts.iter().fold((0.0, 0.0), |s, p| (s.0 + p.0, s.1 + p.1));
    let (mu, mv) = (su / n, sv / n);
    let sxx: f64 = pts.iter().map(|p| (p.0 - mu).powi(2)).sum();
    let sxy: f64 = pts.iter().map(|p| (p.0 - mu) * (p.1 - mv)).sum();
    let b = sxy / sxx;
    Some(Line { a: mv - b * mu, b })
}

/// Intersection of a "horizontal" line y = a + b x with a "vertical" line x = c + d y.
fn intersect(h: Line, v: Line) -> (f64, f64) {
    let x = (v.a + v.b * h.a) / (1.0 - v.b * h.b);
    (x, h.a + h.b * x)
}

pub fn measure(img: &Image, color: &str, tol: u8) -> Value {
    let paper = parse_hex(color).expect("paper color");
    let plum = lum(paper);
    let is_paper = |x: usize, y: usize| max_diff(img.get(x, y), paper) <= tol;

    // mask bbox
    let (mut x0, mut y0, mut x1, mut y1, mut count) = (usize::MAX, usize::MAX, 0, 0, 0usize);
    for y in 0..img.h {
        for x in 0..img.w {
            if is_paper(x, y) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
                count += 1;
            }
        }
    }
    if count == 0 {
        return json!({ "found": false });
    }
    let (bw, bh) = ((x1 - x0 + 1) as f64, (y1 - y0 + 1) as f64);

    // Is the edge sample next to a tape (saturated pixel) rather than the plain background?
    let tape_near = |x: i64, y: i64, dx: i64, dy: i64| {
        (1..=3).any(|k| {
            let (px, py) = (x + dx * k, y + dy * k);
            px >= 0 && py >= 0 && (px as usize) < img.w && (py as usize) < img.h && saturation(img.get(px as usize, py as usize)) >= 20
        })
    };
    let side_trim_x = (x0 as f64 + 0.1 * bw) as usize..(x1 as f64 - 0.1 * bw) as usize;
    let side_trim_y = (y0 as f64 + 0.1 * bh) as usize..(y1 as f64 - 0.1 * bh) as usize;

    let cov_at = |nx: usize, ny: usize, ox: i64, oy: i64| {
        let out = img.lum(ox.max(0) as usize, oy.max(0) as usize);
        ((img.lum(nx, ny) - out) / (plum - out)).clamp(0.0, 1.0)
    };

    // top and bottom edges: samples (x, y)
    let mut top = vec![];
    let mut bottom = vec![];
    for x in side_trim_x.clone() {
        if let Some(y) = (0..img.h).find(|&y| is_paper(x, y)) {
            if y >= 4 && !tape_near(x as i64, y as i64, 0, -1) {
                let cov = cov_at(x, y - 1, x as i64, y as i64 - 3);
                top.push((x as f64 + 0.5, y as f64 - cov));
            }
        }
        if let Some(y) = (0..img.h).rev().find(|&y| is_paper(x, y)) {
            if y + 4 < img.h && !tape_near(x as i64, y as i64, 0, 1) {
                let cov = cov_at(x, y + 1, x as i64, y as i64 + 3);
                bottom.push((x as f64 + 0.5, y as f64 + 1.0 + cov));
            }
        }
    }
    // left and right edges: samples (y, x)
    let mut left = vec![];
    let mut right = vec![];
    for y in side_trim_y {
        if let Some(x) = (0..img.w).find(|&x| is_paper(x, y)) {
            if x >= 4 && !tape_near(x as i64, y as i64, -1, 0) {
                let cov = cov_at(x - 1, y, x as i64 - 3, y as i64);
                left.push((y as f64 + 0.5, x as f64 - cov));
            }
        }
        if let Some(x) = (0..img.w).rev().find(|&x| is_paper(x, y)) {
            if x + 4 < img.w && !tape_near(x as i64, y as i64, 1, 0) {
                let cov = cov_at(x + 1, y, x as i64 + 3, y as i64);
                right.push((y as f64 + 0.5, x as f64 + 1.0 + cov));
            }
        }
    }
    let (Some(t), Some(b), Some(l), Some(r)) = (fit(&top), fit(&bottom), fit(&left), fit(&right)) else {
        return json!({ "found": true, "fit": false, "bbox": [x0, y0, x1 + 1, y1 + 1] });
    };

    let tl = intersect(t, l);
    let tr = intersect(t, r);
    let bl = intersect(b, l);
    let br = intersect(b, r);
    let dist = |p: (f64, f64), q: (f64, f64)| ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
    let width = (dist(tl, tr) + dist(bl, br)) / 2.0;
    let height = (dist(tl, bl) + dist(tr, br)) / 2.0;
    let tilt_top = t.b.atan().to_degrees();
    let tilt_bottom = b.b.atan().to_degrees();
    let tilt_left = (-l.b).atan().to_degrees();
    let tilt_right = (-r.b).atan().to_degrees();
    let tilt = (tilt_top + tilt_bottom + tilt_left + tilt_right) / 4.0;
    let center = ((tl.0 + tr.0 + bl.0 + br.0) / 4.0, (tl.1 + tr.1 + bl.1 + br.1) / 4.0);

    // corner radius from the diagonal depth of each corner that no tape covers
    let corners = [("tl", tl, tr, bl), ("tr", tr, tl, br), ("bl", bl, br, tl), ("br", br, bl, tr)];
    let mut radii = serde_json::Map::new();
    let mut vals = vec![];
    for (name, v, e1, e2) in corners {
        let unit = |p: (f64, f64)| {
            let d = dist(v, p);
            ((p.0 - v.0) / d, (p.1 - v.1) / d)
        };
        let (u1, u2) = (unit(e1), unit(e2));
        let bis = (u1.0 + u2.0, u1.1 + u2.1);
        let bl_ = (bis.0 * bis.0 + bis.1 * bis.1).sqrt();
        let bis = (bis.0 / bl_, bis.1 / bl_);
        // skip corners with tape nearby
        let covered = (0..60).any(|k| {
            let (px, py) = (v.0 + bis.0 * k as f64 * 0.7 - 8.0 * bis.0, v.1 + bis.1 * k as f64 * 0.7 - 8.0 * bis.1);
            px >= 0.0 && py >= 0.0 && (px as usize) < img.w && (py as usize) < img.h && saturation(img.get(px as usize, py as usize)) >= 20
        });
        if covered {
            radii.insert(name.into(), Value::Null);
            continue;
        }
        let out = img.sample_lum(v.0 - 6.0 * bis.0, v.1 - 6.0 * bis.1);
        let mid = (out + plum) / 2.0;
        let mut prev = out;
        let mut crossing = None;
        let mut t_ = -6.0;
        while t_ < 40.0 {
            t_ += 0.05;
            let cur = img.sample_lum(v.0 + t_ * bis.0, v.1 + t_ * bis.1);
            if t_ > 0.0 - 6.0 && cur >= mid && prev < mid {
                crossing = Some(t_ - 0.05 * (cur - mid) / (cur - prev));
                break;
            }
            prev = cur;
        }
        if let Some(d) = crossing {
            let rad = d / (2f64.sqrt() - 1.0);
            radii.insert(name.into(), json!(r3(rad)));
            vals.push(rad);
        } else {
            radii.insert(name.into(), Value::Null);
        }
    }
    let corner_radius = if vals.is_empty() { Value::Null } else { json!(r3(vals.iter().sum::<f64>() / vals.len() as f64)) };

    json!({
        "found": true,
        "bbox": [x0, y0, x1 + 1, y1 + 1],
        "area_px": count,
        "tilt_deg": r3(tilt),
        "tilt_edges_deg": { "top": r3(tilt_top), "bottom": r3(tilt_bottom), "left": r3(tilt_left), "right": r3(tilt_right) },
        "width": r3(width),
        "height": r3(height),
        "center": { "x": r3(center.0), "y": r3(center.1) },
        "corners": {
            "tl": [r3(tl.0), r3(tl.1)], "tr": [r3(tr.0), r3(tr.1)],
            "bl": [r3(bl.0), r3(bl.1)], "br": [r3(br.0), r3(br.1)]
        },
        "corner_radius": corner_radius,
        "corner_radii": Value::Object(radii),
    })
}
