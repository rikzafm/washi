//! Ticket 04 of cross-paragraph selection: the note owns the pointer. Click, double and triple
//! click, Shift+click and drag make a note-level selection (`Doc::selection`).

use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// title, first paragraph, second, third
const TEXT: &str = "# Title\nfirst paragraph here\nsecond one\nthird paragraph words";

fn note(text: &str) -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, text);
    let _ = h.render(0);
    h.advance(ms(5));
    h
}

/// The window point at the left edge (`end` false) or right edge of the `nth` resting word `word`.
fn word(h: &Harness, word: &str, nth: usize, end: bool) -> (f32, f32) {
    // The resting text is a handful of letters per run, without the spaces: find the word in the
    // letters of all runs, in order, and take its first or last letter's place.
    let runs = h.text_runs(0);
    let letters: Vec<(char, usize, usize)> =
        runs.iter().enumerate().flat_map(|(i, r)| r.0.chars().enumerate().map(move |(k, c)| (c, i, k))).filter(|l| !l.0.is_whitespace()).collect();
    let want: Vec<char> = word.chars().collect();
    let at = (0..letters.len())
        .filter(|&s| s + want.len() <= letters.len() && letters[s..s + want.len()].iter().map(|l| l.0).eq(want.iter().copied()))
        .nth(nth)
        .unwrap_or_else(|| panic!("no word {word}"));
    let (_, i, k) = letters[if end { at + want.len() - 1 } else { at }];
    let r = &runs[i];
    let n = r.0.chars().count() as f32;
    let edge = r.1 + r.5 * (if end { k as f32 + 1.0 } else { k as f32 }) / n;
    // the paper's frame starts 42 px in from the window's edge (PaperTheme.margin)
    (42.0 + edge + if end { -1.0 } else { 1.0 }, 42.0 + r.2 + 8.0)
}

fn drag(h: &mut Harness, from: (f32, f32), to: (f32, f32)) {
    h.pointer_move(0, from.0, from.1);
    h.press_button(0, from.0, from.1, false);
    h.pointer_move(0, to.0, to.1);
    h.release_button(0, to.0, to.1, false);
    h.advance(ms(400));
}

#[test]
fn dragging_from_paragraph_1_into_paragraph_3_selects_across_all_three() {
    let mut h = note(TEXT);
    let from = word(&h, "paragraph", 0, false);
    let to = word(&h, "words", 0, true);
    drag(&mut h, from, to);
    // anchor: before "paragraph" in block 1; focus: the end of block 3
    assert_eq!(h.selection(0), ((1, 6), (3, "third paragraph words".len())));
}

#[test]
fn a_press_while_at_rest_settles_the_note_to_level() {
    let mut h = note(TEXT);
    // the window loses the system focus: no field has the keyboard focus, the note lies at rest
    h.set_window_active(0, false);
    h.advance(ms(1));
    let _ = h.text_tilt_scale(0);
    h.advance(ms(500));
    assert_eq!(h.text_tilt_scale(0), 1.0);
    let p = word(&h, "second", 0, false);
    h.pointer_move(0, p.0, p.1);
    h.press_button(0, p.0, p.1, false);
    h.release_button(0, p.0, p.1, false);
    h.advance(ms(1));
    let _ = h.text_tilt_scale(0);
    h.advance(ms(500));
    assert_eq!(h.text_tilt_scale(0), 0.0);
    assert_eq!(h.text_focused_block(0), 2);
    assert_eq!(h.selection(0), ((2, 0), (2, 0)));
}

#[test]
fn a_range_across_blocks_is_washed_by_the_note_and_a_range_inside_one_block_by_its_field() {
    let mut h = note(TEXT);
    let (from, to) = (word(&h, "paragraph", 0, false), word(&h, "words", 0, true));
    drag(&mut h, from, to);
    assert!(h.sel_rects(0).len() >= 3, "{:?}", h.sel_rects(0));
    assert!(!h.wash(0), "the field must not wash it twice");
    // a drag inside the first paragraph: the field's own wash, nothing from the note
    let (from, to) = (word(&h, "paragraph", 0, false), word(&h, "here", 0, true));
    h.advance(ms(900));
    drag(&mut h, from, to);
    assert!(h.sel_rects(0).is_empty());
    assert!(h.wash(0));
    assert_eq!(h.selection(0), ((1, 6), (1, "first paragraph here".len())));
}

#[test]
fn dragging_upwards_keeps_the_anchor_where_the_press_was() {
    let mut h = note(TEXT);
    let (from, to) = (word(&h, "words", 0, true), word(&h, "first", 0, false));
    drag(&mut h, from, to);
    assert_eq!(h.selection(0), ((3, "third paragraph words".len()), (1, 0)));
}

#[test]
fn a_double_click_selects_the_word_and_dragging_after_it_extends_by_words() {
    let mut h = note(TEXT);
    let p = word(&h, "paragraph", 0, false);
    let p = (p.0 + 10.0, p.1);
    h.pointer_move(0, p.0, p.1);
    h.press_button(0, p.0, p.1, false);
    h.release_button(0, p.0, p.1, false);
    h.press_button(0, p.0, p.1, false);
    assert_eq!(h.selection(0), ((1, 6), (1, 15)));
    // the second press is still down: drag into the middle of "words" and the whole word joins
    let mid = word(&h, "words", 0, false);
    h.pointer_move(0, mid.0 + 6.0, mid.1);
    h.release_button(0, mid.0 + 6.0, mid.1, false);
    assert_eq!(h.selection(0), ((1, 6), (3, "third paragraph words".len())));
}

#[test]
fn a_triple_click_selects_the_paragraph() {
    let mut h = note(TEXT);
    let p = word(&h, "second", 0, false);
    h.pointer_move(0, p.0, p.1);
    for _ in 0..3 {
        h.press_button(0, p.0, p.1, false);
        h.release_button(0, p.0, p.1, false);
    }
    assert_eq!(h.selection(0), ((2, 0), (2, "second one".len())));
}

#[test]
fn shift_click_extends_from_the_anchor() {
    let mut h = note(TEXT);
    let p = word(&h, "first", 0, false);
    h.click(0, p.0, p.1);
    h.advance(ms(900));
    let q = word(&h, "words", 0, true);
    h.pointer_move(0, q.0, q.1);
    h.press_shift_button(0, q.0, q.1);
    h.release_button(0, q.0, q.1, false);
    assert_eq!(h.selection(0), ((1, 0), (3, "third paragraph words".len())));
}

#[test]
fn typing_after_a_drag_inside_one_paragraph_replaces_the_selected_text() {
    let mut h = note(TEXT);
    let (from, to) = (word(&h, "paragraph", 0, false), word(&h, "paragraph", 0, true));
    drag(&mut h, from, to);
    h.type_text(0, "Q");
    assert_eq!(h.note_text(0), "# Title\nfirst Q here\nsecond one\nthird paragraph words");
}

#[test]
fn a_press_in_another_paragraph_ends_the_selection_in_the_first() {
    let mut h = note(TEXT);
    let (from, to) = (word(&h, "paragraph", 0, false), word(&h, "here", 0, true));
    drag(&mut h, from, to);
    let p = word(&h, "second", 0, false);
    h.advance(ms(900));
    h.click(0, p.0, p.1);
    h.advance(ms(400));
    h.type_text(0, "Q");
    assert_eq!(h.note_text(0), "# Title\nfirst paragraph here\nQsecond one\nthird paragraph words");
}

#[test]
fn right_click_in_the_text_still_opens_the_note_menu() {
    let mut h = note(TEXT);
    let p = word(&h, "second", 0, false);
    let before = h.menus_shown();
    h.click_right(0, p.0, p.1);
    assert_eq!(h.menus_shown(), before + 1);
}
