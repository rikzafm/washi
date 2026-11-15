//! A scale change makes Slint shape a note's text again (`reshape_text_soon`, `Theme.text-epoch`).
//!
//! On Windows a note window briefly has the monitor's own scale at startup, before the UI size goes
//! back on top. Slint kept the text it shaped in that moment at the monitor's size, so the words
//! were drawn too big for their places (overlapping letters) until a resize. The stand-in platform
//! does not reproduce that quirk of the real backend, so these tests pin the wiring: every scale
//! change of a window showing a note bumps the epoch the words and edit fields read.

use slint::ComponentHandle;
use snap::Harness;
use std::time::Duration;
use washi::note_window::Theme;

fn epoch(h: &Harness, note: usize) -> i32 {
    h.app().with_note(h.id(note), |n| n.ui().global::<Theme>().get_text_epoch()).unwrap()
}

#[test]
fn moving_a_note_to_another_monitor_reshapes_its_text() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "some words");
    h.advance(Duration::from_millis(20));
    let before = epoch(&h, 0);
    h.move_note_to_monitor(0, 1.25);
    h.advance(Duration::from_millis(20));
    assert!(epoch(&h, 0) > before, "the scale change must make the text shape again");
}

#[test]
fn the_platform_putting_its_own_scale_back_reshapes_the_text_at_the_ui_size() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "some words");
    h.set_ui_scale(0.75);
    h.advance(Duration::from_millis(20));
    let before = epoch(&h, 0);
    // what winit does when it creates the native window: the monitor's own scale, without the UI size
    let id = h.id(0);
    h.app().with_note(id, |n| n.ui().window().dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor: 1.0 }));
    h.advance(Duration::from_millis(20));
    let scale = h.app().with_note(id, |n| n.ui().window().scale_factor()).unwrap();
    assert!((scale - 0.75).abs() < 1e-4, "the UI size goes back on top: {scale}");
    assert!(epoch(&h, 0) > before, "and the text is shaped again at that scale");
}

fn settings_epoch(h: &Harness) -> i32 {
    h.app().settings().expect("Settings is open").ui().global::<Theme>().get_text_epoch()
}

/// Settings comes up like a note does: its native window is made at the monitor's own scale. Its
/// labels kept that size (too big at a smaller UI size) until a Tab switch made new ones.
#[test]
fn settings_shapes_its_text_again_once_it_has_shown() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_ui_scale(0.75);
    h.advance(Duration::from_millis(20));
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance(Duration::from_millis(20));
    // 1 is the font set on the new window (`ActiveFont::apply`); the next is the shaping after it showed
    assert!(settings_epoch(&h) >= 2, "the turn after it shows, the text is shaped again");
}

#[test]
fn the_platform_putting_its_own_scale_back_reshapes_the_settings_text() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_ui_scale(0.75);
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(Duration::from_millis(600));
    let before = settings_epoch(&h);
    let ui = h.app().settings().unwrap().ui().clone_strong();
    ui.window().dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });
    h.advance(Duration::from_millis(20));
    h.advance(Duration::from_millis(20));
    assert!((ui.window().scale_factor() - 0.75).abs() < 1e-4, "the UI size goes back on top");
    assert!(settings_epoch(&h) > before, "and the text is shaped again at that scale");
}

/// Every UI label reads the epoch through `Theme.ui-fit`, so a bump reaches all of them.
#[test]
fn the_ui_fit_reads_the_text_epoch() {
    let src = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../washi/ui/theme.slint")).unwrap();
    assert!(src.contains("out property <float> ui-fit: self.ui-fit-value + 0 * self.text-epoch;"));
}
