//! The toolbar: hover fade, hidden toolbars ignoring clicks, and the note never changing size.

use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn note() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    // the real app has drawn the note by the time a pointer arrives
    assert_eq!(h.toolbar_opacity(0), 0.0);
    h
}

/// A point on the first toolbar button, in window coordinates (an untilted 431 x 451 window:
/// margin 40, band 56 at the bottom of the paper, first button 23 px from the paper's left edge).
const BUTTON: (f32, f32) = (40.0 + 23.2 + 20.0, 451.0 - 40.0 - 56.0 + 20.0);

#[test]
fn the_toolbar_is_hidden_until_the_pointer_is_over_the_note() {
    let mut h = note();
    h.advance(ms(500));
    assert_eq!(h.toolbar_opacity(0), 0.0);
    assert!(!h.toolbar_visible(0));
}

#[test]
fn pointer_enters_the_toolbar_fades_in_over_120_ms() {
    let mut h = note();
    h.pointer_move(0, 5.0, 5.0);
    h.advance(ms(0));
    h.advance(ms(60));
    let half = h.toolbar_opacity(0);
    assert!(half > 0.0 && half < 1.0, "halfway through the fade: {half}");
    assert!(h.toolbar_visible(0));
    h.advance(ms(60));
    assert_eq!(h.toolbar_opacity(0), 1.0);
}

#[test]
fn pointer_leaves_the_toolbar_fades_out_and_stops_taking_part() {
    let mut h = note();
    h.pointer_move(0, 5.0, 5.0);
    h.advance(ms(300));
    assert_eq!(h.toolbar_opacity(0), 1.0);
    h.pointer_exit(0);
    h.advance(ms(0));
    h.advance(ms(60));
    let half = h.toolbar_opacity(0);
    assert!(half > 0.0 && half < 1.0, "halfway through the fade: {half}");
    h.advance(ms(60));
    assert_eq!(h.toolbar_opacity(0), 0.0);
    assert!(!h.toolbar_visible(0));
}

#[test]
fn hover_over_the_text_still_counts() {
    let mut h = note();
    h.set_note_text(0, "some text under the pointer");
    h.pointer_move(0, 60.0, 60.0); // inside the text box
    h.advance(ms(300));
    assert_eq!(h.toolbar_opacity(0), 1.0);
}

#[test]
fn keyboard_focus_alone_never_shows_the_toolbar() {
    let mut h = note();
    h.activate_window(0);
    h.type_text(0, "typing away");
    h.advance(ms(500));
    assert_eq!(h.toolbar_opacity(0), 0.0);
    assert!(!h.toolbar_visible(0));
}

#[test]
fn a_hidden_toolbar_ignores_clicks() {
    let mut h = note();
    h.advance(ms(300));
    let before = h.menus_shown();
    // no pointer move first: the toolbar is hidden, so the click reaches the paper behind it
    h.press_button(0, BUTTON.0, BUTTON.1, true);
    h.release_button(0, BUTTON.0, BUTTON.1, true);
    assert_eq!(h.menus_shown(), before + 1, "the right click went through to the paper");
}

#[test]
fn a_shown_toolbar_button_takes_the_click() {
    let mut h = note();
    h.pointer_move(0, BUTTON.0, BUTTON.1);
    h.advance(ms(300));
    assert_eq!(h.toolbar_opacity(0), 1.0);
    let before = h.menus_shown();
    h.press_button(0, BUTTON.0, BUTTON.1, true);
    h.release_button(0, BUTTON.0, BUTTON.1, true);
    assert_eq!(h.menus_shown(), before, "the button swallowed the click");
}

#[test]
fn the_toolbar_stays_while_a_button_is_held_down() {
    let mut h = note();
    h.pointer_move(0, BUTTON.0, BUTTON.1);
    h.advance(ms(300));
    h.press_button(0, BUTTON.0, BUTTON.1, false);
    // dragged out of the window while held (Windows keeps delivering moves to a pressed button)
    h.pointer_move(0, -50.0, -50.0);
    h.advance(ms(0));
    h.advance(ms(300));
    assert_eq!(h.toolbar_opacity(0), 1.0, "held button keeps the toolbar");
    h.release_button(0, -50.0, -50.0, false);
    h.advance(ms(0));
    h.advance(ms(300));
    assert_eq!(h.toolbar_opacity(0), 0.0);
}

#[test]
fn the_note_keeps_its_size_and_layout_when_the_toolbar_appears() {
    let mut h = note();
    h.set_note_text(0, "Endfield Note\nsome body text");
    let size = h.note_size(0);
    let (_, _, idle) = h.render(0);
    h.pointer_move(0, 5.0, 5.0);
    h.advance(ms(300));
    assert_eq!(h.note_size(0), size);
    let (w, _, hover) = h.render(0);
    // only pixels inside the band (and the transparent margin below it is untouched) may change
    let band_top = (451 - 40 - 56 - 10) as usize; // slack for the tilt (about 9 px across the paper)
    let changed: Vec<usize> = (0..idle.len()).filter(|&i| idle[i] != hover[i]).collect();
    assert!(!changed.is_empty(), "the toolbar drew nothing");
    assert!(changed.iter().all(|&i| i / w as usize >= band_top), "something outside the band changed");
}
