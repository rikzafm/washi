mod common;
use common::*;
use serde_json::Value;
use uidiff::{measure_region, Region};

const BASE: Rgb = [247, 191, 205];
const STRIPE: Rgb = [247, 217, 225];

fn num(v: &Value, k: &str) -> f64 {
    v[k].as_f64().unwrap_or_else(|| panic!("missing {k} in {v}"))
}

/// A tape of `len` x `wid` rotated `deg`, with stripes whose lines run at `stripe_deg`
/// (image frame, clockwise) and whose period (perpendicular to the lines) is `pitch`.
fn tape<'a>(cx: f64, cy: f64, len: f64, wid: f64, deg: f64, stripe_deg: f64, pitch: f64) -> Layer<'a> {
    let (ts, tc) = deg.to_radians().sin_cos();
    let (ss, sc) = stripe_deg.to_radians().sin_cos();
    Box::new(move |x, y| {
        let (dx, dy) = (x - cx, y - cy);
        let u = dx * tc + dy * ts;
        let v = -dx * ts + dy * tc;
        if u.abs() > len / 2.0 || v.abs() > wid / 2.0 {
            return None;
        }
        // distance perpendicular to the stripe lines
        let d = -dx * ss + dy * sc;
        Some(if (d / pitch).rem_euclid(1.0) < 0.5 { BASE } else { STRIPE })
    })
}

#[test]
fn tape_reports_bbox_angle_colors_and_stripe_pitch() {
    let img = render(300, 260, BG, &[tape(150.0, 130.0, 170.0, 38.0, -32.0, 18.0, 14.0)]);
    let r: Region = serde_json::from_str(r##"{"kind":"tape","rect":[0,0,300,260],"mask":"r-g>=12"}"##).unwrap();
    let m = measure_region(&img, &r);

    assert!((num(&m, "angle_deg") - -32.0).abs() < 0.5, "{m}");
    assert!((num(&m, "length") - 170.0).abs() < 2.0, "{m}");
    assert!((num(&m, "width") - 38.0).abs() < 2.0, "{m}");
    assert_eq!(m["base"], "#F7BFCD", "{m}");
    assert_eq!(m["stripe"], "#F7D9E1", "{m}");
    assert!((num(&m, "stripe_angle_deg") - 18.0).abs() < 1.0, "{m}");
    assert!((num(&m, "stripe_pitch") - 14.0).abs() < 0.6, "{m}");
    let bbox: Vec<i64> = m["bbox"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect();
    // half extents of a rotated 170x38 rectangle
    let (s, c) = 32f64.to_radians().sin_cos();
    let hw = (170.0 * c + 38.0 * s) / 2.0;
    assert!((bbox[0] as f64 - (150.0 - hw)).abs() <= 2.0, "{m}");
    assert!((bbox[2] as f64 - (150.0 + hw)).abs() <= 2.0, "{m}");
}

#[test]
fn tape_mask_ignores_other_colored_things_outside_the_largest_blob() {
    let speck: Layer = rect(10.0, 10.0, 14.0, 14.0, BASE);
    let img = render(300, 260, BG, &[speck, tape(150.0, 130.0, 170.0, 38.0, -32.0, 18.0, 14.0)]);
    let r: Region = serde_json::from_str(r##"{"kind":"tape","mask":"r-g>=12"}"##).unwrap();
    let m = measure_region(&img, &r);
    assert!((num(&m, "length") - 170.0).abs() < 2.0, "{m}");
}
