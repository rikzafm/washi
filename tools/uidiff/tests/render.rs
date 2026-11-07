//! The Slint software renderer seam: load a font file, render a string, get ink coverage.

use std::path::Path;
use uidiff::fontmatch::{load_fonts, TextRenderer};

fn fonts_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts")
}

#[test]
fn renders_ink_for_a_loaded_font_and_scales_with_size() {
    let faces = load_fonts(&fonts_dir()).unwrap();
    let face = faces.iter().find(|f| f.family == "ShadowsIntoLight").expect("Shadows Into Light is committed");
    let mut r = TextRenderer::new();

    let small = r.render(face, "Endfield Note", 30.0);
    let big = r.render(face, "Endfield Note", 60.0);
    let (_, sy0, _, sy1) = small.ink_bbox(0.5).expect("small render has ink");
    let (_, by0, _, by1) = big.ink_bbox(0.5).expect("big render has ink");
    let (sh, bh) = ((sy1 - sy0) as f64, (by1 - by0) as f64);
    // doubling the font size doubles the ink height (within a pixel or two of AA)
    assert!((bh / sh - 2.0).abs() < 0.1, "ink heights {sh} -> {bh}");

    // a different string gives different ink
    let other = r.render(face, "i", 30.0);
    let (ox0, _, ox1, _) = other.ink_bbox(0.5).unwrap();
    let (sx0, _, sx1, _) = small.ink_bbox(0.5).unwrap();
    assert!(ox1 - ox0 < (sx1 - sx0) / 4);
}

#[test]
fn fractional_offsets_and_letter_spacing_change_the_render() {
    let faces = load_fonts(&fonts_dir()).unwrap();
    let face = &faces[0];
    let mut r = TextRenderer::new();
    let base = r.render_at(face, "Endfield Note", 40.0, 0.0, 0.0, 0.0);
    let half = r.render_at(face, "Endfield Note", 40.0, 0.5, 0.5, 0.0);
    let spaced = r.render_at(face, "Endfield Note", 40.0, 0.0, 0.0, 2.0);
    assert_ne!(base.a, half.a, "a half-pixel offset must change the anti-aliasing");
    let w = |c: &uidiff::fontmatch::Coverage| {
        let (x0, _, x1, _) = c.ink_bbox(0.5).unwrap();
        x1 - x0
    };
    // 12 gaps inside the ink extent: 2 px more per gap
    assert!(w(&spaced) >= w(&base) + 20, "{} vs {}", w(&spaced), w(&base));
}
