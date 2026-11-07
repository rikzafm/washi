mod common;
use common::*;
use serde_json::Value;
use uidiff::{measure_region, Region};

fn paper_region() -> Region {
    serde_json::from_str(r##"{"kind":"paper","color":"#FFFFFD","tol":4}"##).unwrap()
}

fn num(v: &Value, path: &[&str]) -> f64 {
    let mut cur = v;
    for p in path {
        cur = &cur[*p];
    }
    cur.as_f64().unwrap_or_else(|| panic!("missing number at {path:?} in {v}"))
}

#[test]
fn tilted_paper_reports_tilt_size_and_corner_radius() {
    // 300 x 200 paper, radius 12, tilted 1.5 degrees clockwise, with a soft-ish drop shadow below
    let shadow: Layer = rrect(302.0, 206.0, 300.0, 200.0, 12.0, 1.5, [200, 204, 208]);
    let paper: Layer = rrect(300.0, 200.0, 300.0, 200.0, 12.0, 1.5, PAPER);
    let img = render(600, 400, BG, &[shadow, paper]);

    let m = measure_region(&img, &paper_region());

    assert!((num(&m, &["tilt_deg"]) - 1.5).abs() < 0.1, "{m}");
    assert!((num(&m, &["width"]) - 300.0).abs() < 1.0, "{m}");
    assert!((num(&m, &["height"]) - 200.0).abs() < 1.0, "{m}");
    assert!((num(&m, &["center", "x"]) - 300.0).abs() < 1.0, "{m}");
    assert!((num(&m, &["corner_radius"]) - 12.0).abs() < 1.5, "{m}");
}

#[test]
fn tape_over_a_corner_does_not_disturb_the_paper_fit() {
    let paper: Layer = rrect(300.0, 200.0, 300.0, 200.0, 12.0, 1.5, PAPER);
    // a saturated pink strip across the top-left corner
    let tape: Layer = rrect(160.0, 110.0, 120.0, 34.0, 0.0, -35.0, [247, 191, 205]);
    let img = render(600, 400, BG, &[paper, tape]);

    let m = measure_region(&img, &paper_region());

    assert!((num(&m, &["tilt_deg"]) - 1.5).abs() < 0.1, "{m}");
    assert!((num(&m, &["width"]) - 300.0).abs() < 1.0, "{m}");
    assert!((num(&m, &["height"]) - 200.0).abs() < 1.0, "{m}");
}
