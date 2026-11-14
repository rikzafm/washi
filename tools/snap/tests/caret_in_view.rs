//! Spec caret-focus-fixes, ticket 03: the caret stays in view after a Check item toggle (the
//! shortcut, the check button, or typing `[] `) and after a resize, scrolling only when its line
//! would otherwise be outside the text view, and never while a selection is being dragged.

use snap::shot::WINDOW_MARGIN;
use snap::{ms, Harness};

const CENTER: (f32, f32) = (220.0, 200.0);

/// A paragraph long enough to wrap onto several lines at the smallest window.
const LONG: &str = "This paragraph is long enough to wrap onto several lines of the note at the smallest window, \
and the check box indent narrows it so the words fall onto one more line than they took before it.";

/// A title and `n` long paragraphs.
fn long_text(n: usize) -> String {
    let mut t = String::from("# Caret in view");
    for i in 1..=n {
        t.push_str(&format!("\n{i}. {LONG}"));
    }
    t
}

/// The caret's line box, top and bottom, in the text view's frame (0 is the view's top edge).
fn caret_line(h: &Harness) -> (f32, f32) {
    let (_, (block, offset)) = h.selection(0);
    let (_, top, bottom) = h.laid_out_caret(0, block, offset);
    (top - WINDOW_MARGIN as f32, bottom - WINDOW_MARGIN as f32)
}

fn assert_caret_in_view(h: &Harness, what: &str) {
    let (top, bottom) = caret_line(h);
    let view = h.text_view_height(0);
    assert!(top >= -0.5 && bottom <= view + 0.5, "{what}: caret line {top}..{bottom} is outside the view 0..{view} (offset {})", h.scroll_offset(0));
}

/// The caret at the end of the last paragraph, its last line at the bottom of the text view.
fn caret_at_the_bottom(h: &mut Harness) -> usize {
    let blocks = h.blocks(0);
    let last = blocks.len() - 1;
    h.place_caret_at(0, last, blocks[last].1.len());
    h.advance(ms(5));
    let (_, bottom) = caret_line(h);
    let view = h.text_view_height(0);
    assert!(h.scroll_offset(0) > 0.0, "the note overflows");
    assert!((bottom - view).abs() < 1.0, "the caret's line sits at the bottom of the view: {bottom} vs {view}");
    last
}

/// Click the check button with the real pointer (it shows the toolbar first, as a user would).
fn click_check_button(h: &mut Harness) {
    let (x, y) = h.toolbar_button(0, 5);
    h.pointer_move(0, x, y);
    h.advance(ms(300));
    h.click(0, x, y);
    h.advance(ms(5));
}

#[test]
fn toggling_a_check_item_with_the_shortcut_keeps_the_caret_in_view() {
    let mut h = Harness::sized_note(300, 300, &long_text(6));
    let last = caret_at_the_bottom(&mut h);
    h.press_ctrl_shift(0, 'c');
    h.advance(ms(5));
    assert_eq!(h.blocks(0)[last].0, "check", "the paragraph became a check item");
    assert_caret_in_view(&h, "into a check item");
    h.press_ctrl_shift(0, 'c');
    h.advance(ms(5));
    assert_ne!(h.blocks(0)[last].0, "check", "and back");
    assert_caret_in_view(&h, "back into a paragraph");
}

#[test]
fn toggling_a_check_item_with_the_button_keeps_the_caret_in_view() {
    let mut h = Harness::sized_note(300, 300, &long_text(6));
    let last = caret_at_the_bottom(&mut h);
    click_check_button(&mut h);
    assert_eq!(h.blocks(0)[last].0, "check", "the paragraph became a check item");
    assert_caret_in_view(&h, "into a check item");
    click_check_button(&mut h);
    assert_ne!(h.blocks(0)[last].0, "check", "and back");
    assert_caret_in_view(&h, "back into a paragraph");
}

#[test]
fn typing_the_check_marker_keeps_the_caret_in_view() {
    let mut h = Harness::sized_note(300, 300, &long_text(6));
    caret_at_the_bottom(&mut h);
    h.press_enter(0);
    h.type_text(0, "[] ");
    h.advance(ms(5));
    let last = h.blocks(0).len() - 1;
    assert_eq!(h.blocks(0)[last].0, "check", "`[] ` made a check item");
    assert_caret_in_view(&h, "after `[] `");
    h.type_text(0, "x");
    assert!(h.note_text(0).ends_with("\n- [ ] x"), "typing goes to the check item: {:?}", h.note_text(0));
}

#[test]
fn shrinking_the_note_keeps_the_caret_in_view() {
    // at a large size the whole note fits, so the caret in its last paragraph is in view unscrolled
    let mut h = Harness::sized_note(900, 900, &long_text(4));
    let blocks = h.blocks(0);
    let last = blocks.len() - 1;
    h.place_caret_at(0, last, blocks[last].1.len());
    h.advance(ms(5));
    assert_eq!(h.scroll_offset(0), 0.0, "the note fits at the large size");
    assert_caret_in_view(&h, "before the resize");
    h.resize_note(0, 300, 300);
    let _ = h.render(0);
    h.advance(ms(5));
    assert!(h.note_size(0).1 < 600, "the note shrank: {:?}", h.note_size(0));
    assert_caret_in_view(&h, "after shrinking to the smallest");
}

#[test]
fn a_toggle_or_resize_with_the_caret_in_view_does_not_scroll() {
    let mut h = Harness::sized_note(300, 300, &long_text(10));
    h.pointer_move(0, CENTER.0, CENTER.1);
    h.wheel(0, CENTER.0, CENTER.1, -300.0);
    h.advance(ms(5));
    h.click(0, CENTER.0, CENTER.1);
    h.advance(ms(5));
    let scrolled = h.scroll_offset(0);
    assert!(scrolled > 0.0, "scrolled into the middle of the note");
    assert_caret_in_view(&h, "the click");
    let block = h.selection(0).1 .0;

    h.press_ctrl_shift(0, 'c');
    h.advance(ms(5));
    assert_eq!(h.blocks(0)[block].0, "check");
    assert_eq!(h.scroll_offset(0), scrolled, "into a check item");
    h.press_ctrl_shift(0, 'c');
    h.advance(ms(5));
    assert_eq!(h.scroll_offset(0), scrolled, "back into a paragraph");

    let (w, height) = h.note_size(0);
    h.resize_note(0, w, height + 60);
    let _ = h.render(0);
    h.advance(ms(5));
    assert_eq!(h.note_size(0), (w, height + 60), "the note grew taller");
    assert_eq!(h.scroll_offset(0), scrolled, "heightened");
    assert_caret_in_view(&h, "heightened");
}

#[test]
fn a_resize_while_a_selection_is_dragged_does_not_scroll() {
    let mut h = Harness::sized_note(900, 900, &long_text(4));
    // press low in the note and drag a little, the button still held
    let blocks = h.blocks(0);
    let last = blocks.len() - 1;
    let (x, top, bottom) = h.laid_out_caret(0, last, blocks[last].1.len());
    let y = (top + bottom) / 2.0;
    h.pointer_move(0, x - 60.0, y);
    h.press_button(0, x - 60.0, y, false);
    h.pointer_move(0, x - 20.0, y);
    assert_eq!(h.selection(0).1 .0, last, "the drag is in the last paragraph");
    assert_eq!(h.scroll_offset(0), 0.0);
    h.resize_note(0, 300, 300);
    let _ = h.render(0);
    assert!(h.note_size(0).1 < 600, "the note shrank");
    assert_eq!(h.scroll_offset(0), 0.0, "the resize leaves a dragged selection to the drag");
    h.release_button(0, x - 20.0, y, false);
}
