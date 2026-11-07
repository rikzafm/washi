//! End-to-end checks on the real reference mockups and the committed design spec.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::Command;
use uidiff::{measure_mockup, Spec};

fn design_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/design")
}

fn load_spec() -> (Spec, Value) {
    let text = std::fs::read_to_string(design_dir().join("spec.json")).unwrap();
    let raw: Value = serde_json::from_str(&text).unwrap();
    (serde_json::from_value(raw.clone()).unwrap(), raw)
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

#[test]
fn reference_mockups_match_their_recorded_hashes() {
    let readme = std::fs::read_to_string(design_dir().join("reference/README.md")).unwrap();
    for file in ["mockup-hover.png", "mockup-idle.png"] {
        let bytes = std::fs::read(design_dir().join("reference").join(file)).unwrap();
        let hash: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        let row = readme.lines().find(|l| l.contains(file)).unwrap_or_else(|| panic!("{file} missing from README"));
        assert!(row.contains(&hash), "{file}: README hash does not match the file (actual {hash})");
    }
}

#[test]
fn spec_expected_values_are_what_the_tool_measures_today() {
    let (spec, raw) = load_spec();
    for name in spec.mockups.keys() {
        let fresh = measure_mockup(&spec, name, &design_dir()).unwrap();
        assert_eq!(fresh, raw["expected"][name], "expected values for '{name}' are stale: run `uidiff --update-expected`");
    }
}

#[test]
fn measuring_twice_gives_identical_output() {
    let bin = env!("CARGO_BIN_EXE_uidiff");
    let spec = design_dir().join("spec.json");
    for file in ["mockup-hover.png", "mockup-idle.png"] {
        let png = design_dir().join("reference").join(file);
        let run = || Command::new(bin).arg("--measure").arg(&png).arg("--spec").arg(&spec).output().unwrap();
        let (a, b) = (run(), run());
        assert!(a.status.success(), "{}", String::from_utf8_lossy(&a.stderr));
        assert!(!a.stdout.is_empty());
        assert_eq!(a.stdout, b.stdout);
    }
}

#[test]
fn hover_checks_every_region_and_idle_checks_paper_tapes_and_no_toolbar() {
    let (spec, _) = load_spec();
    let hover = &spec.mockups["hover"];
    let mut all: Vec<&String> = spec.regions.keys().filter(|k| *k != "no_toolbar").collect();
    all.sort();
    let mut hv: Vec<&String> = hover.check.iter().collect();
    hv.sort();
    assert!(hover.hover && hv == all, "hover must check every region except no_toolbar (the toolbar is shown there)");

    let idle = &spec.mockups["idle"];
    let mut iv = idle.check.clone();
    iv.sort();
    assert_eq!(iv, ["no_toolbar", "paper", "tape_blue", "tape_pink"]);
    assert!(!idle.hover);
    assert_eq!(idle.mask.len(), 1, "the caption line must be masked");
}

#[test]
fn measured_mockups_show_the_expected_design() {
    let (_, raw) = load_spec();
    let (h, i) = (&raw["expected"]["hover"], &raw["expected"]["idle"]);

    // tilt ~1.5 deg clockwise on both
    for m in [h, i] {
        assert!((f(&m["paper"]["tilt_deg"]) - 1.5).abs() <= 0.1);
        assert!((f(&m["paper"]["width"]) - 544.4).abs() <= 1.0);
    }
    // idle has no toolbar
    assert_eq!(i["no_toolbar"]["non_paper_px"], 0);
    // hover has six toolbar icons, the first one active, and 4 body lines + 2 heading lines + 1 title line
    assert_eq!(h["toolbar"]["cell_count"], 6);
    assert_eq!(h["toolbar"]["cells"][0]["active"], true);
    assert_eq!(h["title"]["line_count"], 1);
    assert_eq!(h["heading"]["line_count"], 2);
    assert_eq!(h["body"]["line_count"], 4);
    // both tapes are the same size on both mockups
    for t in ["tape_pink", "tape_blue"] {
        assert!((f(&h[t]["length"]) - f(&i[t]["length"])).abs() <= 1.0);
        assert!((f(&h[t]["stripe_pitch"]) - 24.0).abs() <= 0.5);
    }
}

#[test]
fn recorded_scale_tilt_and_default_size_agree_with_the_measurements() {
    let (_, raw) = load_spec();
    let scale = f(&raw["scale"]["value"]);
    let h = &raw["expected"]["hover"];
    let i = &raw["expected"]["idle"];

    // default logical paper size is the idle paper size divided by the scale
    let w = f(&h["paper"]["width"]) / scale;
    let hgt = f(&i["paper"]["height"]) / scale;
    let def = &raw["default_paper_logical"];
    assert!((f(&def["width"]) - w).abs() <= 1.0, "{w}");
    assert!((f(&def["height"]) - hgt).abs() <= 1.0, "{hgt}");

    // paper tilt and text tilt are both recorded and match what was measured
    assert!((f(&raw["paper_tilt_deg"]["value"]) - f(&h["paper"]["tilt_deg"])).abs() <= 0.1);
    let text_tilt = f(&h["title"]["baseline_slope_deg"]);
    assert!((f(&raw["text_tilt_deg"]["value"]) - text_tilt).abs() <= 0.1);
    // the measured text is tilted like the paper, and the spec says so
    assert!((text_tilt - 1.5).abs() <= 0.2);
    assert_eq!(raw["text_tilt_deg"]["text_is_tilted"], true);

    // at the recorded scale the toolbar button and tape pitch are whole logical px
    let button = {
        let r = &h["toolbar"]["active_fill"]["rect"];
        (f(&r[2]) - f(&r[0])) / scale
    };
    assert!((button - button.round()).abs() <= 0.1, "{button}");
    let pitch = f(&h["tape_pink"]["stripe_pitch"]) / scale;
    assert!((pitch - pitch.round()).abs() <= 0.1, "{pitch}");
}

#[test]
fn layout_rules_from_the_decisions_are_recorded() {
    let (_, raw) = load_spec();
    let l = &raw["layout"];
    assert_eq!(l["paper"]["min"], serde_json::json!([360, 360]));
    assert_eq!(l["paper"]["minimum_applies_to"], "paper, not window");
    assert!(f(&l["window_margin"]["value"]) > 0.0);
    assert!(f(&l["toolbar"]["band_height"]) > 0.0);
    assert!(l["toolbar"]["rule"].as_str().unwrap().contains("text area ends at the band"));
    assert!(l["tapes"]["rule"].as_str().unwrap().contains("fixed size"));
    assert_eq!(l["text"]["fixed_insets_not_proportions"], true);
    // the band is tall enough for the 40 px button plus its bottom inset
    assert!(f(&l["toolbar"]["band_height"]) >= f(&l["toolbar"]["button_size"][1]) + f(&l["toolbar"]["button_bottom_inset"]) - 1.0);
}
