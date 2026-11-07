//! `uidiff --font-match` on the committed fonts and the real hover mockup: which font wins each
//! role, and how well. The floors are the recorded scores in docs/design/fonts.md minus a margin.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn committed_fonts_win_every_role_on_the_hover_mockup() {
    let out = Command::new(env!("CARGO_BIN_EXE_uidiff"))
        .arg("--font-match")
        .arg("--fonts")
        .arg(repo().join("fonts"))
        .arg("--spec")
        .arg(repo().join("docs/design/spec.json"))
        .output()
        .unwrap();
    // exit code 1 is allowed: it means a role is under the 0.85 gate (the body is, see fonts.md)
    let report: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("no JSON report ({e}): {}", String::from_utf8_lossy(&out.stderr)));

    // role -> (winning font, floor)
    let expected = [
        ("title", "ShadowsIntoLight", 0.85),
        ("heading", "ShadowsIntoLight-Bold", 0.85),
        ("body", "ShadowsIntoLight", 0.80),
        ("glyph_B", "Gelasio-Bold", 0.85),
        ("glyph_I", "NotoSerif-Italic", 0.85),
        ("glyph_U", "NotoSerif-Regular", 0.85),
        ("glyph_ab", "Gelasio-Regular", 0.85),
    ];
    for (role, font, floor) in expected {
        let best = &report["roles"][role]["best"];
        assert_eq!(best["font"], font, "role {role}");
        let iou = best["iou"].as_f64().unwrap();
        assert!(iou >= floor, "role {role}: IoU {iou} is under {floor}");
    }

    // the winner is not a close call: the runner-up trails by a wide margin for the handwriting roles
    for role in ["title", "body"] {
        let ranking = report["roles"][role]["ranking"].as_array().unwrap();
        let (first, second) = (ranking[0]["iou"].as_f64().unwrap(), ranking[1]["iou"].as_f64().unwrap());
        assert!(first - second > 0.2, "role {role}: {first} vs {second}");
    }
}

#[test]
fn every_committed_font_ships_with_an_ofl_license() {
    let dir = repo().join("fonts");
    let files: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    let licenses: Vec<&String> = files.iter().filter(|f| f.starts_with("OFL-") && f.ends_with(".txt")).collect();
    for font in files.iter().filter(|f| f.ends_with(".ttf")) {
        let family = font.split(['-', '.']).next().unwrap().to_lowercase();
        assert!(licenses.iter().any(|l| l.to_lowercase().contains(&family)), "{font} has no OFL license file ({licenses:?})");
        let size = std::fs::metadata(dir.join(font)).unwrap().len();
        assert!(size < 100_000, "{font} is {size} bytes: subset it");
    }
}
