mod common;
use common::*;
use serde_json::Value;
use uidiff::{measure_region, Region};

const FILL: Rgb = [240, 240, 238];
const GRAY: Rgb = [139, 146, 158];

fn bbox(v: &Value) -> Vec<i64> {
    v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect()
}

#[test]
fn toolbar_reports_per_cell_icon_boxes_colors_and_the_active_button() {
    let layers: Vec<Layer> = vec![
        rrect(48.0, 40.0, 58.0, 58.0, 14.0, 0.0, FILL), // active button [19,11,77,69]
        rect(36.0, 28.0, 60.0, 52.0, INK),              // B (dark)
        rect(110.0, 28.0, 120.0, 52.0, GRAY),           // I
        rect(160.0, 30.0, 184.0, 52.0, GRAY),           // U (+ a thin underline gap below)
        rect(160.0, 56.0, 184.0, 58.0, GRAY),
        rect(220.0, 30.0, 250.0, 50.0, GRAY),           // strike
        rect(290.0, 28.0, 294.0, 32.0, GRAY),           // list: dot, then lines after a small gap
        rect(299.0, 28.0, 316.0, 31.0, GRAY),
        rect(290.0, 48.0, 294.0, 52.0, GRAY),
        rect(299.0, 48.0, 316.0, 51.0, GRAY),
        rect(350.0, 28.0, 376.0, 52.0, GRAY),           // image
    ];
    let img = render(420, 80, PAPER, &layers);
    let r: Region = serde_json::from_str(
        r##"{"kind":"icons","rect":[0,0,420,80],"cells":6,"paper":"#FFFFFD","fill":{"color":"#F0F0EE"}}"##,
    )
    .unwrap();
    let m = measure_region(&img, &r);

    let cells = m["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 6, "{m}");
    assert_eq!(bbox(&cells[0]["bbox"]), vec![36, 28, 60, 52]);
    assert_eq!(cells[0]["color"], "#2B2F36");
    assert_eq!(cells[0]["active"], true);
    assert_eq!(bbox(&cells[1]["bbox"]), vec![110, 28, 120, 52]);
    assert_eq!(cells[1]["color"], "#8B929E");
    assert_eq!(cells[1]["active"], false);
    assert_eq!(bbox(&cells[2]["bbox"]), vec![160, 30, 184, 58]);
    assert_eq!(bbox(&cells[4]["bbox"]), vec![290, 28, 316, 52]);
    assert_eq!(bbox(&cells[5]["bbox"]), vec![350, 28, 376, 52]);

    let f = &m["active_fill"];
    let fb: Vec<f64> = f["rect"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    for (got, want) in fb.iter().zip([19.0, 11.0, 77.0, 69.0]) {
        assert!((got - want).abs() < 0.6, "{f}");
    }
    assert_eq!(f["color"], "#F0F0EE");
    assert!((f["radius"].as_f64().unwrap() - 14.0).abs() < 1.5, "{f}");
}

#[test]
fn toolbar_without_a_fill_reports_no_active_button() {
    let layers: Vec<Layer> = vec![rect(110.0, 28.0, 120.0, 52.0, GRAY), rect(210.0, 28.0, 220.0, 52.0, GRAY)];
    let img = render(300, 80, PAPER, &layers);
    let r: Region =
        serde_json::from_str(r##"{"kind":"icons","rect":[0,0,300,80],"cells":2,"paper":"#FFFFFD","fill":{"color":"#F0F0EE"}}"##).unwrap();
    let m = measure_region(&img, &r);
    assert!(m["active_fill"].is_null(), "{m}");
    assert_eq!(m["cells"].as_array().unwrap().len(), 2);
}
