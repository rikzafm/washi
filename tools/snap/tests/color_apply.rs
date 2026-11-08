//! Ticket 04 of text-color-and-highlight: the popover applies colors to the note's range (or to
//! text typed next at a collapsed caret), Clear resets both, and the button, rings and caption
//! show what the selection has.

use slint::platform::Key;
use snap::Harness;
use std::time::Duration;

const BLUE: usize = 6;
const YELLOW: usize = 4;
const RED: usize = 9;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// A note holding "title" and "body text", the caret at the start of the body.
fn note() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, "title\nbody text");
    let _ = h.render(0);
    h.advance(ms(5));
    h.press(0, Key::DownArrow);
    h
}

fn note_with_selected_word() -> Harness {
    let mut h = note();
    h.press_shift(0, Key::RightArrow, 4);
    h
}

fn open_popover(h: &mut Harness) {
    let (x, y) = h.toolbar_button(0, 4);
    h.pointer_move(0, x, y);
    h.advance(ms(300));
    h.click(0, x, y);
    assert!(h.picker_open(0));
}

fn pick(h: &mut Harness, row: usize, i: usize) {
    let (x, y) = h.swatch_point(0, row, i);
    h.click(0, x, y);
}

#[test]
fn picking_ink_then_marker_colors_the_word_and_undo_steps_back_through_both() {
    let mut h = note_with_selected_word();
    let range = h.selection(0);
    open_popover(&mut h);
    pick(&mut h, 0, BLUE);
    assert_eq!(h.note_text(0), "# title\n<c=blue>body</c> text");
    assert_eq!(h.shown_colors(0), (BLUE as i32, 0));
    pick(&mut h, 1, YELLOW);
    assert_eq!(h.note_text(0), "# title\n<c=blue><m=yellow>body</c></m> text");
    assert_eq!(h.shown_colors(0), (BLUE as i32, YELLOW as i32));
    assert!(h.picker_open(0), "the popover stays open after a pick");
    assert_eq!(h.selection(0), range, "the range survived");
    let (x, y) = h.picker_point(0);
    h.pointer_move(0, x, y);
    assert_eq!(h.picker_caption(0), "Blue ink · Yellow marker");

    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), "# title\n<c=blue>body</c> text");
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), "# title\nbody text");
}

#[test]
fn picking_what_the_range_already_has_adds_no_undo_step() {
    let mut h = note_with_selected_word();
    open_popover(&mut h);
    pick(&mut h, 0, BLUE);
    pick(&mut h, 0, BLUE);
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), "# title\nbody text");
}

#[test]
fn a_range_across_two_paragraphs_takes_the_color_in_both() {
    let mut h = note();
    h.press_ctrl(0, 'a');
    open_popover(&mut h);
    pick(&mut h, 0, RED);
    let text = h.note_text(0);
    assert!(text.contains("<c=red>title</c>") && text.contains("<c=red>body text</c>"), "{text}");
}

#[test]
fn a_mixed_range_shows_no_ring_and_the_default_a() {
    let mut h = note_with_selected_word();
    open_popover(&mut h);
    pick(&mut h, 0, BLUE);
    // extend the selection over the uncolored " text"
    h.press_shift(0, Key::RightArrow, 5);
    assert_eq!(h.shown_colors(0).0, -1);
}

#[test]
fn clear_removes_both_colors_and_the_caption_goes_back_to_default() {
    let mut h = note_with_selected_word();
    open_popover(&mut h);
    pick(&mut h, 0, BLUE);
    pick(&mut h, 1, YELLOW);
    let (x, y) = h.clear_point(0);
    h.click(0, x, y);
    assert_eq!(h.note_text(0), "# title\nbody text");
    assert_eq!(h.shown_colors(0), (0, 0));
    assert!(!h.picker_open(0), "Clear closes the popover");
    open_popover(&mut h);
    assert_eq!(h.picker_caption(0), "Default ink");
}

#[test]
fn hovering_a_swatch_names_it_and_leaving_restores_the_summary() {
    let mut h = note_with_selected_word();
    open_popover(&mut h);
    pick(&mut h, 0, BLUE);
    let (x, y) = h.swatch_point(0, 1, YELLOW);
    h.pointer_move(0, x, y);
    assert_eq!(h.picker_caption(0), "Yellow marker");
    let (x, y) = h.swatch_point(0, 1, 0);
    h.pointer_move(0, x, y);
    assert_eq!(h.picker_caption(0), "No marker");
    let (x, y) = h.picker_point(0);
    h.pointer_move(0, x, y);
    assert_eq!(h.picker_caption(0), "Blue ink");
}

#[test]
fn a_collapsed_caret_stores_the_color_for_the_text_typed_next() {
    let mut h = note();
    open_popover(&mut h);
    pick(&mut h, 0, RED);
    assert_eq!(h.shown_colors(0), (RED as i32, 0), "the button shows the pending ink");
    h.press(0, Key::Escape);
    h.type_text(0, "hi ");
    assert_eq!(h.note_text(0), "# title\n<c=red>hi </c>body text");
}

#[test]
fn colors_are_saved_and_load_back() {
    let mut h = note_with_selected_word();
    open_popover(&mut h);
    pick(&mut h, 0, BLUE);
    let text = h.note_text(0);
    assert_eq!(text, "# title\n<c=blue>body</c> text");
    h.advance(ms(5000));
    assert_eq!(h.saved_text(0), text, "the autosave wrote the colors");
    h.set_note_text(0, &h.saved_text(0));
    assert_eq!(h.note_text(0), text);
}
