//! Ticket 06 of cross-paragraph selection: Shift+arrows carry the selection over a paragraph's
//! edge, Shift+Ctrl+Home/End reach the note's ends, and a drag held outside the view scrolls it.

use slint::platform::Key;
use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

const TEXT: &str = "# Title\nfirst paragraph here\nsecond one\nthird paragraph words";

fn note(text: &str) -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, text);
    let _ = h.render(0);
    h.advance(ms(5));
    h
}

/// The window point at the left edge (`end` false) or right edge of the first word `word`.
fn word(h: &Harness, word: &str, end: bool) -> (f32, f32) {
    let runs = h.text_runs(0);
    let letters: Vec<(char, usize, usize)> =
        runs.iter().enumerate().flat_map(|(i, r)| r.0.chars().enumerate().map(move |(k, c)| (c, i, k))).filter(|l| !l.0.is_whitespace()).collect();
    let want: Vec<char> = word.chars().collect();
    let at = (0..letters.len())
        .find(|&s| s + want.len() <= letters.len() && letters[s..s + want.len()].iter().map(|l| l.0).eq(want.iter().copied()))
        .unwrap_or_else(|| panic!("no word {word}"));
    let (_, i, k) = letters[if end { at + want.len() - 1 } else { at }];
    let r = &runs[i];
    let n = r.0.chars().count() as f32;
    let edge = r.1 + r.5 * (if end { k as f32 + 1.0 } else { k as f32 }) / n;
    (42.0 + edge + if end { -1.0 } else { 1.0 }, 42.0 + r.2 + 8.0)
}

fn click_word(h: &mut Harness, w: &str, end: bool) {
    let p = word(h, w, end);
    h.click(0, p.0, p.1);
    h.advance(ms(900));
}

// Where each key lands is the caret motion module's to test (washi/src/caret_motion.rs); these only
// check that the keys reach it from the window and that its range is washed.

#[test]
fn shift_arrows_reach_the_note_and_carry_the_range_over_a_paragraph_s_edge() {
    let mut h = note(TEXT);
    click_word(&mut h, "paragraph", false);
    h.press_shift(0, Key::DownArrow, 1);
    let (anchor, focus) = h.selection(0);
    assert_eq!(anchor, (1, 6), "the anchor stays");
    assert_ne!(focus.0, 1, "the focus left the paragraph");
    assert!(h.sel_rects(0).len() >= 2);

    click_word(&mut h, "here", true);
    h.press_shift(0, Key::RightArrow, 1);
    assert_eq!(h.selection(0).1 .0, 2, "Shift+Right over the paragraph's end");
    click_word(&mut h, "second", false);
    h.press_shift(0, Key::LeftArrow, 1);
    assert_eq!(h.selection(0).1 .0, 1, "Shift+Left over the paragraph's start");
}

#[test]
fn shift_ctrl_end_and_home_reach_the_note_and_extend_to_its_ends() {
    let mut h = note(TEXT);
    click_word(&mut h, "second", false);
    h.press_ctrl_shift_key(0, Key::End);
    assert_eq!(h.selection(0).1 .0, 3);
    h.press_ctrl_shift_key(0, Key::Home);
    assert_eq!(h.selection(0).1, (0, 0));
}

#[test]
fn a_drag_held_below_the_view_scrolls_the_note_and_the_range_follows() {
    let long: String = (1..=40).map(|i| format!("line number {i}")).collect::<Vec<_>>().join("\n");
    let mut h = note(&format!("# Title\n{long}"));
    let from = word(&h, "line", false);
    h.pointer_move(0, from.0, from.1);
    h.press_button(0, from.0, from.1, false);
    // the pointer goes below the text area and stays there
    let below = (from.0 + 20.0, 42.0 + h.text_view_height(0) + 12.0);
    h.pointer_move(0, below.0, below.1);
    h.advance(ms(100));
    let first = h.scroll_offset(0);
    let first_focus = h.selection(0).1 .0;
    h.advance(ms(1000));
    assert!(h.scroll_offset(0) > first, "{} then {}", first, h.scroll_offset(0));
    assert!(h.selection(0).1 .0 > first_focus, "the range follows the scroll");
    // releasing the button stops it
    h.release_button(0, below.0, below.1, false);
    let stopped = h.scroll_offset(0);
    h.advance(ms(1000));
    assert_eq!(h.scroll_offset(0), stopped);
}

#[test]
fn a_drag_held_above_the_view_scrolls_back_up() {
    let long: String = (1..=40).map(|i| format!("line number {i}")).collect::<Vec<_>>().join("\n");
    let mut h = note(&format!("# Title\n{long}"));
    h.wheel(0, 100.0, 100.0, -10_000.0);
    h.advance(ms(5));
    let scrolled = h.scroll_offset(0);
    assert!(scrolled > 0.0);
    let from = word(&h, "line", false);
    let _ = from; // resting words sit in the tilted layout; press at the middle of the view
    let mid = (100.0, 42.0 + h.text_view_height(0) / 2.0);
    h.pointer_move(0, mid.0, mid.1);
    h.press_button(0, mid.0, mid.1, false);
    h.pointer_move(0, mid.0, 30.0);
    h.advance(ms(1500));
    assert!(h.scroll_offset(0) < scrolled);
    h.release_button(0, mid.0, 30.0, false);
}
