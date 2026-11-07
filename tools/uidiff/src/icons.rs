//! Icons region (toolbar): per-cell icon ink box and color, plus the filled active button.

use crate::empty::clamp_rect;
use crate::image::*;
use crate::spec::{Fill, Rect};
use serde_json::{json, Value};

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// Filled rounded rectangle: sub-pixel rect (l, t, r, b), color and corner radius.
fn measure_fill(img: &Image, area: (usize, usize, usize, usize), paper: Rgb, fill: Rgb, ink_lum_max: f64) -> Option<(Value, [f64; 4])> {
    let (x0, y0, x1, y1) = area;
    let (plum, flum) = (lum(paper), lum(fill));
    let (mut bx0, mut by0, mut bx1, mut by1, mut n) = (usize::MAX, usize::MAX, 0, 0, 0usize);
    let (aw, ah) = (x1 - x0, y1 - y0);
    let member: Vec<bool> = (0..aw * ah).map(|i| max_diff(img.get(x0 + i % aw, y0 + i / aw), fill) <= 2).collect();
    // anti-aliased icon edges pass through the fill color too, so keep only the biggest blob
    let comp = crate::cc::largest_component(&member, aw, ah)?;
    for y in 0..ah {
        for x in 0..aw {
            if comp[y * aw + x] {
                bx0 = bx0.min(x0 + x);
                by0 = by0.min(y0 + y);
                bx1 = bx1.max(x0 + x + 1);
                by1 = by1.max(y0 + y + 1);
                n += 1;
            }
        }
    }
    if n < 100 {
        return None;
    }
    // soft "fillness": 0 on paper, 1 on the fill (icon ink counts as filled)
    let f = |x: i64, y: i64| -> f64 {
        if x < 0 || y < 0 || x as usize >= img.w || y as usize >= img.h {
            return 0.0;
        }
        let l = img.lum(x as usize, y as usize);
        if l < ink_lum_max {
            1.0
        } else {
            ((plum - l) / (plum - flum)).clamp(0.0, 1.0)
        }
    };
    let (bx0, by0, bx1, by1) = (bx0 as i64, by0 as i64, bx1 as i64, by1 as i64);
    let mid_rows: Vec<i64> = (by0 + (by1 - by0) / 4..by1 - (by1 - by0) / 4).collect();
    let mid_cols: Vec<i64> = (bx0 + (bx1 - bx0) / 4..bx1 - (bx1 - bx0) / 4).collect();
    let left = median(mid_rows.iter().map(|&y| (bx0 + 4) as f64 - (bx0 - 4..bx0 + 4).map(|x| f(x, y)).sum::<f64>()).collect());
    let right = median(mid_rows.iter().map(|&y| (bx1 - 4) as f64 + (bx1 - 4..bx1 + 4).map(|x| f(x, y)).sum::<f64>()).collect());
    let top = median(mid_cols.iter().map(|&x| (by0 + 4) as f64 - (by0 - 4..by0 + 4).map(|y| f(x, y)).sum::<f64>()).collect());
    let bottom = median(mid_cols.iter().map(|&x| (by1 - 4) as f64 + (by1 - 4..by1 + 4).map(|y| f(x, y)).sum::<f64>()).collect());

    // corner radius from the area missing in each corner square: (1 - pi/4) r^2
    let overlap = |px: i64, py: i64| {
        let ox = ((px + 1) as f64).min(right) - (px as f64).max(left);
        let oy = ((py + 1) as f64).min(bottom) - (py as f64).max(top);
        ox.max(0.0) * oy.max(0.0)
    };
    let s = 24i64;
    let squares = [
        (left.floor() as i64 - 1, top.floor() as i64 - 1),
        (right.ceil() as i64 + 1 - s, top.floor() as i64 - 1),
        (left.floor() as i64 - 1, bottom.ceil() as i64 + 1 - s),
        (right.ceil() as i64 + 1 - s, bottom.ceil() as i64 + 1 - s),
    ];
    let mut radii = vec![];
    for (sx, sy) in squares {
        let mut missing = 0.0;
        for y in sy..sy + s {
            for x in sx..sx + s {
                missing += overlap(x, y) - f(x, y);
            }
        }
        radii.push((missing.max(0.0) / (1.0 - std::f64::consts::FRAC_PI_4)).sqrt());
    }
    let radius = radii.iter().sum::<f64>() / 4.0;
    Some((
        json!({
            "rect": [r3(left), r3(top), r3(right), r3(bottom)],
            "color": to_hex(fill),
            "radius": r3(radius),
        }),
        [left, top, right, bottom],
    ))
}

pub fn measure(img: &Image, rect: &Rect, cells: usize, paper: &str, fill: Option<&Fill>, ink_lum_max: f64) -> Value {
    let paper = parse_hex(paper).expect("paper color");
    let (x0, y0, x1, y1) = clamp_rect(img, rect);
    let is_ink = |x: usize, y: usize| img.lum(x, y) < ink_lum_max;

    let active = fill.and_then(|f| measure_fill(img, (x0, y0, x1, y1), paper, parse_hex(&f.color).expect("fill color"), ink_lum_max));

    // column runs of ink
    let col_ink: Vec<bool> = (x0..x1).map(|x| (y0..y1).any(|y| is_ink(x, y))).collect();
    let mut runs: Vec<(usize, usize)> = vec![];
    let mut start = None;
    for (i, &c) in col_ink.iter().enumerate() {
        match (c, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                runs.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        runs.push((s, col_ink.len()));
    }
    // cut at the (cells - 1) widest gaps
    let mut gaps: Vec<(usize, usize)> = (1..runs.len()).map(|i| (runs[i].0 - runs[i - 1].1, i)).collect();
    gaps.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut cuts: Vec<usize> = gaps.iter().take(cells.saturating_sub(1)).map(|g| g.1).collect();
    cuts.sort();
    let mut groups: Vec<(usize, usize)> = vec![];
    let mut first = 0;
    for c in cuts.iter().chain(std::iter::once(&runs.len())) {
        if *c > first {
            groups.push((runs[first].0, runs[*c - 1].1));
        }
        first = *c;
    }

    let mut out = vec![];
    for (gs, ge) in groups {
        let (cx0, cx1) = (x0 + gs, x0 + ge);
        let (mut bx0, mut by0, mut bx1, mut by1) = (usize::MAX, usize::MAX, 0, 0);
        let mut minlum = f64::MAX;
        for y in y0..y1 {
            for x in cx0..cx1 {
                if is_ink(x, y) {
                    bx0 = bx0.min(x);
                    by0 = by0.min(y);
                    bx1 = bx1.max(x + 1);
                    by1 = by1.max(y + 1);
                    minlum = minlum.min(img.lum(x, y));
                }
            }
        }
        let mut sum = [0.0f64; 3];
        let mut cnt = 0.0;
        for y in y0..y1 {
            for x in cx0..cx1 {
                if is_ink(x, y) && img.lum(x, y) <= minlum + 12.0 {
                    let c = img.get(x, y);
                    for k in 0..3 {
                        sum[k] += c[k] as f64;
                    }
                    cnt += 1.0;
                }
            }
        }
        let color = [(sum[0] / cnt).round() as u8, (sum[1] / cnt).round() as u8, (sum[2] / cnt).round() as u8];
        let (cx, cy) = ((bx0 + bx1) as f64 / 2.0, (by0 + by1) as f64 / 2.0);
        let is_active = active.as_ref().map(|(_, r)| cx >= r[0] && cx <= r[2] && cy >= r[1] && cy <= r[3]).unwrap_or(false);
        out.push(json!({ "bbox": [bx0, by0, bx1, by1], "color": to_hex(color), "active": is_active }));
    }
    json!({
        "rect": rect,
        "cell_count": out.len(),
        "cells": out,
        "active_fill": active.map(|a| a.0).unwrap_or(Value::Null),
    })
}
