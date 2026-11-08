//! Ticket 05 of text-color-and-highlight: the popover can be driven from the keyboard while the
//! text field keeps the focus and the range. Arrows move over the swatches, Enter or Space picks,
//! Escape closes.

use slint::platform::Key;
use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// A note with "title" and "body text", "body" selected, and the popover open.
fn open_picker_over_a_word() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, "title\nbody text");
    let _ = h.render(0);
    h.advance(ms(5));
    h.press(0, Key::DownArrow);
    h.press_shift(0, Key::RightArrow, 4);
    let (x, y) = h.toolbar_button(0, 4);
    h.pointer_move(0, x, y);
    h.advance(ms(300));
    h.click(0, x, y);
    assert!(h.picker_open(0));
    h
}

#[test]
fn arrows_walk_the_swatches_and_do_not_move_the_caret_or_the_range() {
    let mut h = open_picker_over_a_word();
    let (focused, range) = (h.text_focused_block(0), h.selection(0));
    assert_eq!(h.picker_focus(0), None);
    h.press(0, Key::RightArrow);
    assert_eq!(h.picker_focus(0), Some((0, 1)));
    h.press(0, Key::RightArrow);
    assert_eq!(h.picker_focus(0), Some((0, 2)));
    h.press(0, Key::LeftArrow);
    assert_eq!(h.picker_focus(0), Some((0, 1)));
    h.press(0, Key::DownArrow);
    assert_eq!(h.picker_focus(0), Some((1, 1)), "Down switches to the marker row, same column");
    h.press(0, Key::UpArrow);
    assert_eq!(h.picker_focus(0), Some((0, 1)));
    assert!(h.picker_open(0));
    assert_eq!(h.text_focused_block(0), focused, "the field kept the focus");
    assert_eq!(h.selection(0), range, "the range survived");
    assert_eq!(h.note_text(0), "# title\nbody text");
}

#[test]
fn the_ends_of_a_row_stop_the_walk() {
    let mut h = open_picker_over_a_word();
    h.press(0, Key::LeftArrow);
    h.press(0, Key::LeftArrow);
    assert_eq!(h.picker_focus(0), Some((0, 0)));
    h.press(0, Key::RightArrow);
    h.press(0, Key::DownArrow);
    h.press(0, Key::DownArrow);
    assert_eq!(h.picker_focus(0), Some((1, 1)));
    for _ in 0..12 {
        h.press(0, Key::RightArrow);
    }
    assert_eq!(h.picker_focus(0), Some((1, 9)));
}

#[test]
fn enter_picks_ink_and_space_picks_a_marker_and_the_popover_stays_open() {
    let mut h = open_picker_over_a_word();
    let range = h.selection(0);
    for _ in 0..6 {
        h.press(0, Key::RightArrow);
    }
    h.press(0, Key::Return);
    assert_eq!(h.note_text(0), "# title\n<c=blue>body</c> text");
    assert_eq!(h.shown_colors(0), (6, 0));
    h.press(0, Key::DownArrow);
    for _ in 0..2 {
        h.press(0, Key::LeftArrow);
    }
    assert_eq!(h.picker_focus(0), Some((1, 4)));
    h.type_text(0, " ");
    assert_eq!(h.note_text(0), "# title\n<c=blue><m=yellow>body</c></m> text");
    assert!(h.picker_open(0));
    assert_eq!(h.selection(0), range);
}

#[test]
fn escape_after_walking_closes_it_and_returns_to_the_text() {
    let mut h = open_picker_over_a_word();
    let range = h.selection(0);
    h.press(0, Key::RightArrow);
    h.press(0, Key::Escape);
    assert!(!h.picker_open(0));
    assert_eq!(h.selection(0), range);
    assert_eq!(h.picker_focus(0), None, "the next opening starts fresh");
    // the arrows move the caret again
    h.press(0, Key::RightArrow);
    assert_ne!(h.selection(0), range);
}

#[test]
fn space_before_any_arrow_is_text_not_a_pick() {
    let mut h = open_picker_over_a_word();
    h.type_text(0, " ");
    assert!(!h.picker_open(0), "typing closes the popover");
    assert_eq!(h.note_text(0), "# title\n  text");
}
