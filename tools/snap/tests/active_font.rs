//! The active font (`washi::fonts`) is what every note lays its text out in and draws it with:
//! setting another one changes where lines break and the face the words are drawn in, in every
//! open note, and putting the Default font back restores both exactly.

use slint::ComponentHandle;
use snap::Harness;
use std::borrow::Cow;
use std::time::Duration;
use washi::fonts::{ActiveFont, ThemeMetrics};
use washi::note_window::Theme;

const TEXT: &str = "# Title\nthe quick brown fox jumps over the lazy dog and keeps running far away from here";

fn another_font() -> ActiveFont {
    let regular = include_bytes!("../../../fonts/Gelasio-Regular.ttf").as_slice();
    let bold = include_bytes!("../../../fonts/Gelasio-Bold.ttf").as_slice();
    ActiveFont::new("Gelasio", "Washi Caret", [regular, bold, regular, bold].map(Cow::Borrowed)).unwrap()
}

#[test]
fn a_baked_face_set_becomes_an_active_font_whose_metrics_are_the_bakes() {
    use washi::fonts::{bake, read_package, AxisValues};
    let zip = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fonts/Test_Serif.zip")).unwrap();
    let set = bake(&read_package(&zip).unwrap(), AxisValues::default()).unwrap();
    let font = ActiveFont::from_face_set(&set).unwrap();
    assert_eq!((font.text_family.as_str(), font.caret_family.as_str()), (set.text_family.as_str(), set.caret_family.as_str()));
    assert_eq!(font.metrics.space_em, set.metrics.space_em);
    assert_eq!(font.metrics.line_em, set.metrics.line_em);
    // the room values only ever round the measured reach up
    assert!(font.metrics.lean_room_em >= set.metrics.lean_em);
    assert!(font.metrics.descender_room_em > set.metrics.descent_em);
    // and the layout's faces are the bake's own: a space is as wide as the bake says
    let face = font.faces.face(false, false);
    let adv = face.glyph_hor_advance(face.glyph_index(' ').unwrap()).unwrap() as f32;
    assert!((adv / face.units_per_em() as f32 - set.metrics.space_em).abs() < 1e-4);
}

fn theme<R>(h: &Harness, note: usize, f: impl FnOnce(&Theme) -> R) -> R {
    h.app().with_note(h.id(note), |n| f(&n.ui().global::<Theme>())).unwrap()
}

#[test]
fn another_active_font_changes_the_breaks_and_the_face_in_every_open_note_and_the_default_restores_them() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.new_note_from(0);
    h.advance(Duration::from_millis(20));
    for note in 0..2 {
        h.set_note_text(note, TEXT);
    }
    let before: Vec<_> = (0..2).map(|n| h.text_runs(n)).collect();
    let level = |runs: &[(String, f32, f32, f32, f32, f32)]| runs.iter().map(|r| (r.0.clone(), r.1, r.2, r.5)).collect::<Vec<_>>();
    assert_eq!(theme(&h, 0, |t| t.get_handwriting().to_string()), "Shadows Into Light");

    let font = another_font();
    let metrics = font.metrics;
    h.app().set_active_font(font);
    h.advance(Duration::from_millis(20));
    for note in 0..2 {
        assert_eq!(theme(&h, note, |t| t.get_handwriting().to_string()), "Gelasio", "the words are drawn in the new face");
        assert_eq!(theme(&h, note, |t| t.get_lead_em()), metrics.space_em);
        assert_eq!(theme(&h, note, |t| t.get_handwriting_line_em()), metrics.line_em);
        assert_ne!(level(&h.text_runs(note)), level(&before[note]), "note {note}: the lines break elsewhere");
    }

    h.app().set_active_font(ActiveFont::default_font());
    h.advance(Duration::from_millis(20));
    let d = ThemeMetrics::DEFAULT;
    for note in 0..2 {
        assert_eq!(theme(&h, note, |t| t.get_handwriting().to_string()), "Shadows Into Light");
        assert_eq!(theme(&h, note, |t| (t.get_lead_em(), t.get_lean_room(), t.get_descender_room(), t.get_italic_overhang())), (d.space_em, d.lean_room_em, d.descender_room_em, d.italic_overhang_em));
        assert_eq!(level(&h.text_runs(note)), level(&before[note]), "note {note}: back where it was");
    }
}
