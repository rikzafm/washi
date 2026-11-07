mod common;
use common::*;
use serde_json::Value;
use uidiff::{measure_region, Region};

fn region(json: &str) -> Region {
    serde_json::from_str(json).unwrap()
}

fn nums(v: &Value) -> Vec<i64> {
    v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect()
}

#[test]
fn empty_region_reports_stray_pixels_and_their_bbox() {
    let stray: Layer = rect(40.0, 50.0, 44.0, 53.0, INK);
    let img = render(200, 120, PAPER, &[stray]);
    let r = region(r##"{"kind":"empty","rect":[10,10,190,110],"color":"#FFFFFD","tol":4}"##);
    let m = measure_region(&img, &r);
    assert_eq!(m["non_paper_px"], 12);
    assert_eq!(nums(&m["bbox"]), vec![40, 50, 44, 53]);

    let clean = render(200, 120, PAPER, &[]);
    let m = measure_region(&clean, &r);
    assert_eq!(m["non_paper_px"], 0);
    assert!(m["bbox"].is_null());
}

#[test]
fn text_region_lists_lines_with_ink_box_baseline_height_color_and_coverage() {
    // two "lines": a block with a descender, and a second block below
    let layers: Vec<Layer> = vec![
        rect(30.0, 20.0, 150.0, 40.0, INK), // line 1: rows 20..40
        rect(60.0, 40.0, 66.0, 46.0, INK),  // its descender: rows 40..46
        rect(30.0, 70.0, 110.0, 90.0, INK), // line 2: rows 70..90
    ];
    let img = render(200, 120, PAPER, &layers);
    let r = region(r##"{"kind":"text","rect":[10,10,190,110],"paper":"#FFFFFD"}"##);
    let m = measure_region(&img, &r);

    let lines = m["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 2, "{m}");
    assert_eq!(nums(&lines[0]["bbox"]), vec![30, 20, 150, 46]);
    assert_eq!(nums(&lines[1]["bbox"]), vec![30, 70, 110, 90]);
    assert_eq!(lines[0]["ink_height"].as_f64().unwrap(), 26.0);
    // baseline: bottom of the dense part, not the descender
    assert_eq!(lines[0]["baseline"].as_f64().unwrap(), 40.0);
    assert_eq!(lines[1]["baseline"].as_f64().unwrap(), 90.0);
    assert_eq!(lines[1]["ink_color"], "#2B2F36");
    // coverage = ink area / bbox area; line 2 is a solid 80x20 block
    let cov = lines[1]["coverage"].as_f64().unwrap();
    assert!((cov - 1.0).abs() < 0.01, "{cov}");
    let cov0 = lines[0]["coverage"].as_f64().unwrap();
    let expect0 = (120.0 * 20.0 + 36.0) / (120.0 * 26.0);
    assert!((cov0 - expect0).abs() < 0.01, "{cov0} vs {expect0}");
}

#[test]
fn text_baseline_slope_is_zero_for_level_text_and_positive_for_clockwise_tilt() {
    // many thin glyph-like ticks along a baseline
    let ticks = |slope_deg: f64| -> Vec<Layer<'static>> {
        let k = slope_deg.to_radians().tan();
        (0..30)
            .map(|i| {
                let x = 20.0 + i as f64 * 9.0;
                let base = 60.0 + (x - 20.0) * k;
                rect(x, base - 14.0, x + 4.0, base, INK)
            })
            .collect()
    };
    let r = region(r##"{"kind":"text","rect":[10,10,290,110],"paper":"#FFFFFD"}"##);

    let level = measure_region(&render(300, 120, PAPER, &ticks(0.0)), &r);
    assert!(level["baseline_slope_deg"].as_f64().unwrap().abs() < 0.15, "{level}");

    let tilted = measure_region(&render(300, 120, PAPER, &ticks(1.5)), &r);
    let s = tilted["baseline_slope_deg"].as_f64().unwrap();
    assert!((s - 1.5).abs() < 0.25, "{tilted}");
}

#[test]
fn tilted_lines_that_overlap_in_raw_rows_are_still_separate_lines() {
    let k = 1.5f64.to_radians().tan();
    let mut layers: Vec<Layer> = vec![];
    for (line, base0) in [(0, 60.0), (1, 80.0)] {
        let _ = line;
        for i in 0..30 {
            let x = 20.0 + i as f64 * 9.0;
            let base = base0 + (x - 20.0) * k;
            layers.push(rect(x, base - 14.0, x + 4.0, base, INK));
        }
    }
    let img = render(300, 140, PAPER, &layers);
    let r = region(r##"{"kind":"text","rect":[10,10,290,130],"paper":"#FFFFFD"}"##);
    let m = measure_region(&img, &r);
    let lines = m["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 2, "{m}");
    // baseline is reported at the left of the region (x = 10): base0 minus the tilt up to x=20
    for (l, base0) in lines.iter().zip([60.0, 80.0]) {
        let b = l["baseline"].as_f64().unwrap();
        assert!((b - (base0 - 10.0 * k)).abs() < 1.0, "{l}");
        assert!((l["ink_height"].as_f64().unwrap() - 14.0).abs() < 1.5, "{l}");
    }
}
