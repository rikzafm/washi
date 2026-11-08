//! Ticket 03 of text-color-and-highlight: the "A" button and the popover shell. Opening the
//! popover must leave the text field focused and the note's range intact; it closes on Escape,
//! a click elsewhere, a resize, losing the focus, and typing. Picking does nothing yet.

use slint::platform::Key;
use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// A note holding "title" and "body text", the word "body" selected in the body.
fn note_with_selected_word() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, "title\nbody text");
    let _ = h.render(0);
    h.advance(ms(5));
    h.press(0, Key::DownArrow);
    h.press_shift(0, Key::RightArrow, 4);
    h
}

/// Click toolbar button `k` (4 is the "A"), showing the toolbar first as a user would.
fn click_button(h: &mut Harness, k: usize) {
    let (x, y) = h.toolbar_button(0, k);
    h.pointer_move(0, x, y);
    h.advance(ms(300));
    h.click(0, x, y);
}

#[test]
fn clicking_a_opens_the_popover_and_leaves_the_field_focused_and_the_range_intact() {
    let mut h = note_with_selected_word();
    let (focused, range) = (h.text_focused_block(0), h.selection(0));
    assert_eq!(range, ((1, 0), (1, 4)));
    assert!(!h.picker_open(0));
    click_button(&mut h, 4);
    assert!(h.picker_open(0));
    assert_eq!(h.text_focused_block(0), focused, "the field kept the focus");
    assert_eq!(h.selection(0), range, "the range survived");
    assert_eq!(h.note_text(0), "# title\nbody text", "nothing was changed");
}

#[test]
fn moving_the_pointer_over_the_popover_changes_nothing() {
    let mut h = note_with_selected_word();
    click_button(&mut h, 4);
    let (focused, range) = (h.text_focused_block(0), h.selection(0));
    let (x, y) = h.picker_point(0);
    h.pointer_move(0, x, y);
    h.advance(ms(50));
    assert!(h.picker_open(0));
    assert_eq!(h.text_focused_block(0), focused);
    assert_eq!(h.selection(0), range);
}

#[test]
fn clicking_inside_the_popover_keeps_it_open_focused_and_unchanged() {
    let mut h = note_with_selected_word();
    click_button(&mut h, 4);
    let (focused, range) = (h.text_focused_block(0), h.selection(0));
    let (x, y) = h.picker_point(0);
    h.click(0, x, y);
    assert!(h.picker_open(0));
    assert_eq!(h.text_focused_block(0), focused);
    assert_eq!(h.selection(0), range);
    assert_eq!(h.note_text(0), "# title\nbody text");
}

#[test]
fn a_second_click_on_a_closes_it() {
    let mut h = note_with_selected_word();
    click_button(&mut h, 4);
    click_button(&mut h, 4);
    assert!(!h.picker_open(0));
}

#[test]
fn escape_closes_it_and_keeps_the_range() {
    let mut h = note_with_selected_word();
    let range = h.selection(0);
    click_button(&mut h, 4);
    h.press(0, Key::Escape);
    assert!(!h.picker_open(0));
    assert_eq!(h.selection(0), range);
}

#[test]
fn a_click_elsewhere_closes_it() {
    let mut h = note_with_selected_word();
    click_button(&mut h, 4);
    h.click(0, 120.0, 90.0); // in the text
    assert!(!h.picker_open(0));
}

#[test]
fn resizing_the_note_closes_it() {
    let mut h = note_with_selected_word();
    click_button(&mut h, 4);
    let (w, hh) = h.note_size(0);
    h.resize_note(0, w + 40, hh + 20);
    h.advance(ms(5));
    assert!(!h.picker_open(0));
}

#[test]
fn the_note_losing_the_focus_closes_it() {
    let mut h = note_with_selected_word();
    click_button(&mut h, 4);
    h.set_window_active(0, false);
    h.advance(ms(5));
    assert!(!h.picker_open(0));
}

#[test]
fn minimising_closes_it() {
    let mut h = note_with_selected_word();
    click_button(&mut h, 4);
    h.minimize(0);
    assert!(!h.picker_open(0));
}

#[test]
fn typing_over_the_selection_closes_it() {
    let mut h = note_with_selected_word();
    click_button(&mut h, 4);
    h.type_text(0, "x");
    assert!(!h.picker_open(0));
}

#[test]
fn other_toolbar_buttons_close_it_and_still_work() {
    let mut h = note_with_selected_word();
    click_button(&mut h, 4);
    click_button(&mut h, 0);
    assert!(!h.picker_open(0));
    assert_eq!(h.note_text(0), "# title\n**body** text");
}
