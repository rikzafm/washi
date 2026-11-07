//! Tape: color mask -> bbox, PCA angle, base/stripe colors, stripe angle and pitch.

use crate::empty::clamp_rect;
use crate::image::*;
use crate::spec::Rect;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Parse "r-g>=12" into (channel a, channel b, threshold).
fn parse_mask(s: &str) -> (usize, usize, i32) {
    let ch = |c: char| match c {
        'r' => 0,
        'g' => 1,
        'b' => 2,
        _ => panic!("bad channel in mask {s}"),
    };
    let cs: Vec<char> = s.chars().collect();
    assert!(cs.len() > 5 && cs[1] == '-' && s[3..].starts_with(">="), "mask must look like r-g>=12, got {s}");
    (ch(cs[0]), ch(cs[2]), s[5..].trim().parse().expect("mask threshold"))
}

fn normalize_half_turn(mut deg: f64) -> f64 {
    while deg > 90.0 {
        deg -= 180.0;
    }
    while deg <= -90.0 {
        deg += 180.0;
    }
    deg
}

pub fn measure(img: &Image, rect: Option<&Rect>, mask: &str) -> Value {
    let (ca, cb, thr) = parse_mask(mask);
    let (x0, y0, x1, y1) = match rect {
        Some(r) => clamp_rect(img, r),
        None => (0, 0, img.w, img.h),
    };
    let (w, h) = (x1 - x0, y1 - y0);
    let on = |x: usize, y: usize| {
        let c = img.get(x, y);
        c[ca] as i32 - c[cb] as i32 >= thr
    };
    let mut member = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            member[y * w + x] = on(x0 + x, y0 + y);
        }
    }

    let Some(comp_mask) = crate::cc::largest_component(&member, w, h) else {
        return json!({ "found": false });
    };
    let comp = |x: usize, y: usize| comp_mask[y * w + x];

    // bbox, centroid, PCA
    let (mut bx0, mut by0, mut bx1, mut by1) = (usize::MAX, usize::MAX, 0, 0);
    let (mut n, mut sx, mut sy) = (0.0, 0.0, 0.0);
    for y in 0..h {
        for x in 0..w {
            if comp(x, y) {
                bx0 = bx0.min(x);
                by0 = by0.min(y);
                bx1 = bx1.max(x + 1);
                by1 = by1.max(y + 1);
                n += 1.0;
                sx += x as f64 + 0.5;
                sy += y as f64 + 0.5;
            }
        }
    }
    let (mx, my) = (sx / n, sy / n);
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for y in 0..h {
        for x in 0..w {
            if comp(x, y) {
                let (dx, dy) = (x as f64 + 0.5 - mx, y as f64 + 0.5 - my);
                sxx += dx * dx;
                syy += dy * dy;
                sxy += dx * dy;
            }
        }
    }
    let axis = 0.5 * (2.0 * sxy).atan2(sxx - syy); // radians, long axis, y down => clockwise positive
    let (asn, acs) = axis.sin_cos();
    let (mut umin, mut umax, mut vmin, mut vmax) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for y in 0..h {
        for x in 0..w {
            if comp(x, y) {
                let (dx, dy) = (x as f64 + 0.5 - mx, y as f64 + 0.5 - my);
                let u = dx * acs + dy * asn;
                let v = -dx * asn + dy * acs;
                umin = umin.min(u);
                umax = umax.max(u);
                vmin = vmin.min(v);
                vmax = vmax.max(v);
            }
        }
    }
    let angle_deg = normalize_half_turn(axis.to_degrees());

    // interior pixels (eroded twice) for color and stripe analysis
    let interior = |x: usize, y: usize, r: i64| {
        (-r..=r).all(|dy| {
            (-r..=r).all(|dx| {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                nx >= 0 && ny >= 0 && nx < w as i64 && ny < h as i64 && comp(nx as usize, ny as usize)
            })
        })
    };
    let mut inner: Vec<(f64, f64, Rgb)> = vec![];
    for y in 0..h {
        for x in 0..w {
            if comp(x, y) && interior(x, y, 2) {
                inner.push((x as f64 + 0.5, y as f64 + 0.5, img.get(x0 + x, y0 + y)));
            }
        }
    }
    let base_json = |b: Value| b;
    if inner.len() < 50 {
        return base_json(json!({ "found": true, "bbox": [x0 + bx0, y0 + by0, x0 + bx1, y0 + by1], "interior": false }));
    }

    // two-means on luminance
    let lums: Vec<f64> = inner.iter().map(|p| lum(p.2)).collect();
    let (mut lo, mut hi) = (lums.iter().cloned().fold(f64::MAX, f64::min), lums.iter().cloned().fold(f64::MIN, f64::max));
    for _ in 0..30 {
        let mid = (lo + hi) / 2.0;
        let (mut a, mut an, mut b, mut bn) = (0.0, 0.0, 0.0, 0.0);
        for l in &lums {
            if *l < mid {
                a += l;
                an += 1.0;
            } else {
                b += l;
                bn += 1.0;
            }
        }
        if an == 0.0 || bn == 0.0 {
            break;
        }
        lo = a / an;
        hi = b / bn;
    }
    let mid = (lo + hi) / 2.0;
    let modal = |light: bool| -> Rgb {
        let mut counts: BTreeMap<Rgb, usize> = BTreeMap::new();
        for (p, l) in inner.iter().zip(&lums) {
            if (*l >= mid) == light {
                *counts.entry(p.2).or_default() += 1;
            }
        }
        let mut best = ([0u8; 3], 0usize);
        for (c, n) in counts {
            if n > best.1 {
                best = (c, n);
            }
        }
        best.0
    };
    let (base, stripe) = (modal(false), modal(true));

    // stripe direction: the projection direction whose 1-D profile of (stripe=+1, base=-1) varies most
    let profile = |nu_deg: f64| -> (Vec<f64>, f64) {
        let (s, c) = nu_deg.to_radians().sin_cos();
        let mut acc: BTreeMap<i64, (f64, f64)> = BTreeMap::new();
        for (p, l) in inner.iter().zip(&lums) {
            let t = (p.0 * c + p.1 * s) * 2.0;
            let e = acc.entry(t.floor() as i64).or_default();
            e.0 += if *l >= mid { 1.0 } else { -1.0 };
            e.1 += 1.0;
        }
        let (lo_k, hi_k) = (*acc.keys().next().unwrap(), *acc.keys().last().unwrap());
        let mut prof = vec![f64::NAN; (hi_k - lo_k + 1) as usize];
        let (mut score, mut total) = (0.0, 0.0);
        for (k, (sum, cnt)) in &acc {
            if *cnt >= 3.0 {
                let m = sum / cnt;
                prof[(k - lo_k) as usize] = m;
                score += m * m * cnt;
                total += cnt;
            }
        }
        (prof, if total > 0.0 { score / total } else { 0.0 })
    };
    let mut best_nu = (f64::MIN, 0.0);
    let mut nu = 0.0;
    while nu < 180.0 {
        let s = profile(nu).1;
        if s > best_nu.0 {
            best_nu = (s, nu);
        }
        nu += 0.5;
    }
    let center = best_nu.1;
    let mut best_fine = (f64::MIN, center);
    let mut nu = center - 1.0;
    while nu <= center + 1.0 {
        let s = profile(nu).1;
        if s > best_fine.0 + 1e-12 {
            best_fine = (s, nu);
        }
        nu += 0.05;
    }
    let nu = best_fine.1;
    let stripe_angle = normalize_half_turn(nu - 90.0);

    // pitch: smallest lag whose autocorrelation is within 85% of the best
    let (prof, _) = profile(nu);
    let mean = {
        let v: Vec<f64> = prof.iter().cloned().filter(|x| !x.is_nan()).collect();
        v.iter().sum::<f64>() / v.len() as f64
    };
    let ac = |lag: usize| -> f64 {
        let (mut s, mut c) = (0.0, 0.0);
        for i in 0..prof.len().saturating_sub(lag) {
            let (a, b) = (prof[i], prof[i + lag]);
            if !a.is_nan() && !b.is_nan() {
                s += (a - mean) * (b - mean);
                c += 1.0;
            }
        }
        if c < 10.0 {
            f64::MIN
        } else {
            s / c
        }
    };
    let max_lag = prof.len() / 2;
    let acs_v: Vec<f64> = (0..=max_lag + 1).map(ac).collect();
    let peak = (6..=max_lag).map(|l| acs_v[l]).fold(f64::MIN, f64::max);
    let mut pitch = 0.0;
    for l in 6..=max_lag {
        if acs_v[l] >= 0.85 * peak && acs_v[l] >= acs_v[l - 1] && acs_v[l] >= acs_v[l + 1] {
            let (a, b, c) = (acs_v[l - 1], acs_v[l], acs_v[l + 1]);
            let off = if (a - 2.0 * b + c).abs() > 1e-12 { 0.5 * (a - c) / (a - 2.0 * b + c) } else { 0.0 };
            pitch = (l as f64 + off) / 2.0; // bins are 0.5 px
            break;
        }
    }
    // angle between the profile normal and the tape's long axis converts pitch to "along the tape"
    let cos_between = (nu.to_radians() - axis).cos().abs();
    let pitch_along = if cos_between > 1e-6 { pitch / cos_between } else { f64::INFINITY };

    json!({
        "found": true,
        "bbox": [x0 + bx0, y0 + by0, x0 + bx1, y0 + by1],
        "area_px": n as usize,
        "centroid": { "x": r3(x0 as f64 + mx), "y": r3(y0 as f64 + my) },
        "angle_deg": r3(angle_deg),
        "length": r3(umax - umin),
        "width": r3(vmax - vmin),
        "base": to_hex(base),
        "stripe": to_hex(stripe),
        "stripe_angle_deg": r3(stripe_angle),
        "stripe_pitch": r3(pitch),
        "stripe_pitch_along_axis": r3(pitch_along),
    })
}
