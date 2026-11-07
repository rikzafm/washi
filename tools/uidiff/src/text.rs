//! Text region: line list (ink bbox, baseline, ink height, coverage, ink color) and baseline slope.

use crate::empty::clamp_rect;
use crate::image::*;
use crate::spec::Rect;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Sum of squares of the row histogram of `pts` after shearing by `k` (tan of the tilt).
/// Weights are split linearly between the two nearest rows so anti-aliased edges carry the
/// sub-pixel information.
fn sharpness(pts: &[(f64, f64, f64)], k: f64) -> f64 {
    let mut bins: BTreeMap<i64, f64> = BTreeMap::new();
    for (x, y, w) in pts {
        let v = y - x * k - 0.5;
        let f = v.floor();
        let t = v - f;
        *bins.entry(f as i64).or_default() += w * (1.0 - t);
        *bins.entry(f as i64 + 1).or_default() += w * t;
    }
    bins.values().map(|v| v * v).sum()
}

/// Baseline slope in degrees (positive = right side lower) that makes the ink's row histogram
/// sharpest. `pts` are (x, y, weight) with pixel-center coordinates.
fn baseline_slope(pts: &[(f64, f64, f64)]) -> f64 {
    if pts.len() < 20 {
        return 0.0;
    }
    let mut best: (f64, f64) = (f64::MIN, 0.0);
    for i in -60..=60 {
        let deg = i as f64 * 0.05;
        let s = sharpness(pts, deg.to_radians().tan());
        if s > best.0 + 1e-9 || ((s - best.0).abs() <= 1e-9 && deg.abs() < best.1.abs()) {
            best = (s, deg);
        }
    }
    // only trust a tilt that is clearly better than level text
    if best.0 < sharpness(pts, 0.0) * 1.003 {
        0.0
    } else {
        best.1
    }
}

pub fn measure(img: &Image, rect: &Rect, paper: &str, ink_lum_max: f64) -> Value {
    let paper = parse_hex(paper).expect("paper color");
    let plum = lum(paper);
    let (x0, y0, x1, y1) = clamp_rect(img, rect);
    let is_ink = |x: usize, y: usize| img.lum(x, y) < ink_lum_max;
    let soft_weight = |x: usize, y: usize, denom: f64| ((plum - img.lum(x, y)) / denom).clamp(0.0, 1.0);

    // Soft ink over the whole region, used for the tilt estimate.
    let mut all_pts: Vec<(f64, f64, f64)> = vec![];
    for y in y0..y1 {
        for x in x0..x1 {
            let w = soft_weight(x, y, plum - 43.0);
            if w > 0.02 {
                all_pts.push((x as f64 + 0.5, y as f64 + 0.5, w));
            }
        }
    }
    let slope = baseline_slope(&all_pts);
    let k = slope.to_radians().tan();

    // Shear the rows by the tilt so tilted lines stay separable, then group rows into lines.
    let shear = |x: usize, y: usize| ((y as f64 + 0.5) - ((x - x0) as f64 + 0.5) * k).floor() as i64;
    let mut hist: BTreeMap<i64, usize> = BTreeMap::new();
    for y in y0..y1 {
        for x in x0..x1 {
            if is_ink(x, y) {
                *hist.entry(shear(x, y)).or_default() += 1;
            }
        }
    }
    // Descenders of one line can touch the ascenders of the next, so lines are the dense runs of
    // rows, cut at the emptiest row between neighbours (closest to the middle on ties).
    let mut runs: Vec<(i64, i64)> = vec![];
    if let (Some(&lo), Some(&hi)) = (hist.keys().next(), hist.keys().last()) {
        let count = |b: i64| hist.get(&b).copied().unwrap_or(0);
        let thr = (0.06 * hist.values().copied().max().unwrap() as f64).max(1.0);
        let mut dense: Vec<(i64, i64)> = vec![];
        for b in lo..=hi {
            if count(b) as f64 >= thr {
                match dense.last_mut() {
                    Some(r) if r.1 == b => r.1 = b + 1,
                    _ => dense.push((b, b + 1)),
                }
            }
        }
        // a real line has a dense baseline row; a bump of stray descenders/ascenders does not
        let peak_needed = 0.12 * hist.values().copied().max().unwrap() as f64;
        dense.retain(|&(s, e)| (s..e).any(|b| count(b) as f64 >= peak_needed));
        let mut start = lo;
        for pair in dense.windows(2) {
            let (gap_lo, gap_hi) = (pair[0].1, pair[1].0);
            let mid = (gap_lo + gap_hi) as f64 / 2.0;
            let cut = (gap_lo..gap_hi)
                .min_by(|&a, &b| {
                    count(a)
                        .cmp(&count(b))
                        .then((a as f64 + 0.5 - mid).abs().partial_cmp(&(b as f64 + 0.5 - mid).abs()).unwrap())
                })
                .unwrap_or(gap_lo);
            runs.push((start, cut));
            start = cut;
        }
        runs.push((start, hi + 1));
        // trim each line to its inked rows
        runs = runs
            .into_iter()
            .filter_map(|(s, e)| {
                let first = (s..e).find(|&b| count(b) > 0)?;
                let last = (s..e).rev().find(|&b| count(b) > 0)?;
                Some((first, last + 1))
            })
            .collect();
    }

    let mut lines = vec![];
    for (rs, re) in runs {
        let (mut lx0, mut ly0, mut lx1, mut ly1) = (usize::MAX, usize::MAX, 0, 0);
        let mut color_counts: BTreeMap<Rgb, usize> = BTreeMap::new();
        let mut darkest = ([0u8; 3], f64::MAX);
        let mut members = vec![];
        for y in y0..y1 {
            for x in x0..x1 {
                let b = shear(x, y);
                if b < rs - 1 || b >= re + 1 {
                    continue;
                }
                members.push((x, y));
                if is_ink(x, y) && b >= rs && b < re {
                    lx0 = lx0.min(x);
                    lx1 = lx1.max(x + 1);
                    ly0 = ly0.min(y);
                    ly1 = ly1.max(y + 1);
                    let l = img.lum(x, y);
                    if l < 100.0 {
                        *color_counts.entry(img.get(x, y)).or_default() += 1;
                    }
                    if l < darkest.1 {
                        darkest = (img.get(x, y), l);
                    }
                }
            }
        }
        let ink_color = color_counts
            .iter()
            .fold(None::<(&Rgb, &usize)>, |best, (c, n)| match best {
                Some((_, bn)) if bn >= n => best,
                _ => Some((c, n)),
            })
            .map(|(c, _)| *c)
            .unwrap_or(darkest.0);
        let denom = (plum - lum(ink_color)).max(1.0);
        let mut area = 0.0;
        let mut pts: Vec<(f64, f64, f64)> = vec![];
        for &(x, y) in &members {
            let w = soft_weight(x, y, denom);
            area += w;
            if w > 0.02 {
                pts.push((x as f64 + 0.5, y as f64 + 0.5, w));
            }
        }
        let ink_height = (re - rs) as f64;
        let width = (lx1 - lx0) as f64;
        // baseline: bottom of the last sheared row at least 40% as inked as the densest row
        let counts: Vec<usize> = (rs..re).map(|b| hist.get(&b).copied().unwrap_or(0)).collect();
        let max_row = *counts.iter().max().unwrap() as f64;
        let last_dense = counts.iter().rposition(|&n| n as f64 >= 0.4 * max_row).unwrap();
        let baseline = (rs + last_dense as i64 + 1) as f64; // y at the region's left edge
        let slope_line = if lx1 - lx0 >= 150 { json!(r3(baseline_slope(&pts))) } else { Value::Null };
        lines.push(json!({
            "bbox": [lx0, ly0, lx1, ly1],
            "baseline": r3(baseline),
            "baseline_left": r3(baseline + (lx0 - x0) as f64 * k),
            "baseline_right": r3(baseline + (lx1 - x0) as f64 * k),
            "ink_height": r3(ink_height),
            "ink_area": r3(area),
            "coverage": r3(area / (width * ink_height)),
            "ink_color": to_hex(ink_color),
            "baseline_slope_deg": slope_line,
        }));
    }
    json!({
        "rect": rect,
        "line_count": lines.len(),
        "lines": lines,
        "baseline_slope_deg": r3(slope),
    })
}
