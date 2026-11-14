//! `uidiff --invariants`: a default-size snap against a larger one. The real mockup, moved
//! around on a bigger canvas, stands in for a perfect pair; edits to a copy stand in for regressions.
use std::path::PathBuf;
use uidiff::invariants::{check, Options};
use uidiff::{Image, Spec};

fn design_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/design")
}

/// The spec, for snaps made from the hover mockup: these tests place the mockup image, whose toolbar
/// is the mockup's (`toolbar`), not the app's smaller one (`app_toolbar`), so the invariants measure that.
fn spec() -> Spec {
    let mut s: Spec = serde_json::from_str(&std::fs::read_to_string(design_dir().join("spec.json")).unwrap()).unwrap();
    s.regions.remove("app_toolbar");
    s
}

fn hover() -> Image {
    Image::load_png(&design_dir().join("reference/mockup-hover.png")).unwrap()
}

const DESK: [u8; 3] = [0xD9, 0xDD, 0xE1];
const PAPER: [u8; 3] = [0xFF, 0xFF, 0xFD];

/// `img` placed at (dx, dy) on a canvas of w x h desk.
fn placed(img: &Image, w: usize, h: usize, dx: usize, dy: usize) -> Image {
    let mut out = Image::new(w, h, DESK);
    for y in 0..img.h {
        for x in 0..img.w {
            out.set(x + dx, y + dy, img.get(x, y));
        }
    }
    out
}

fn move_rect(img: &Image, rect: [usize; 4], dx: i64, dy: i64, fill: [u8; 3]) -> Image {
    let mut out = img.clone();
    for y in rect[1]..rect[3] {
        for x in rect[0]..rect[2] {
            out.set(x, y, fill);
        }
    }
    for y in rect[1]..rect[3] {
        for x in rect[0]..rect[2] {
            out.set((x as i64 + dx) as usize, (y as i64 + dy) as usize, img.get(x, y));
        }
    }
    out
}

fn failing(rows: &[uidiff::compare::Row]) -> Vec<String> {
    rows.iter().filter(|r| !r.pass).map(|r| format!("{}/{}", r.group, r.item)).collect()
}

fn run(default: &Image, large: &Image) -> Vec<uidiff::compare::Row> {
    check(&spec(), &hover(), default, large, &Options::default()).unwrap()
}

const LW: usize = 900;
const LH: usize = 1000;

#[test]
fn identical_notes_at_different_places_satisfy_every_invariant() {
    let m = hover();
    let default = placed(&m, 720, 840, 0, 0);
    let large = placed(&m, LW, LH, 60, 90);
    let rows = run(&default, &large);
    assert!(failing(&rows).is_empty(), "failing: {:?}", failing(&rows));
    for g in ["tilt", "tapes", "insets", "fonts", "toolbar", "text area"] {
        assert!(rows.iter().any(|r| r.group == g), "group {g} is checked");
    }
}

#[test]
fn a_title_that_moved_three_pixels_breaks_the_insets() {
    let m = hover();
    let default = placed(&m, 720, 840, 0, 0);
    let moved = move_rect(&m, [130, 160, 440, 235], 0, 3, PAPER);
    let large = placed(&moved, LW, LH, 60, 90);
    let bad = failing(&run(&default, &large));
    assert!(bad.iter().any(|b| b.starts_with("insets/")), "failing: {bad:?}");
}

#[test]
fn a_tape_that_slid_breaks_the_tape_offset() {
    let m = hover();
    let default = placed(&m, 720, 840, 0, 0);
    // blue tape lives around [485,669,654,803]
    let moved = move_rect(&m, [470, 655, 670, 815], 6, 6, PAPER);
    let large = placed(&moved, LW, LH, 60, 90);
    let bad = failing(&run(&default, &large));
    assert!(bad.iter().any(|b| b.starts_with("tapes/")), "failing: {bad:?}");
}

#[test]
fn a_toolbar_that_moved_breaks_the_toolbar_geometry() {
    let m = hover();
    let default = placed(&m, 720, 840, 0, 0);
    let moved = move_rect(&m, [110, 645, 500, 725], 0, -4, PAPER);
    let large = placed(&moved, LW, LH, 60, 90);
    let bad = failing(&run(&default, &large));
    assert!(bad.iter().any(|b| b.starts_with("toolbar/")), "failing: {bad:?}");
}

#[test]
fn text_reaching_into_the_toolbar_band_is_caught() {
    let m = hover();
    let default = placed(&m, 720, 840, 0, 0);
    let mut spill = m.clone();
    // a dark line of "text" in the band, right of the last icon, outside the toolbar rect
    for x in 510..600 {
        for y in 700..705 {
            spill.set(x, y, [0x2B, 0x2F, 0x36]);
        }
    }
    let large = placed(&spill, LW, LH, 60, 90);
    let bad = failing(&run(&default, &large));
    assert!(bad.iter().any(|b| b.starts_with("text area/")), "failing: {bad:?}");
}

#[test]
fn a_snapshot_without_paper_fails_instead_of_passing_vacuously() {
    let m = hover();
    let default = placed(&m, 720, 840, 0, 0);
    let large = Image::new(LW, LH, DESK);
    let bad = failing(&run(&default, &large));
    assert!(bad.iter().any(|b| b.starts_with("tilt/")), "failing: {bad:?}");
}
