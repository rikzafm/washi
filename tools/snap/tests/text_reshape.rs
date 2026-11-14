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
