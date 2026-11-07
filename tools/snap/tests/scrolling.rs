//! Ticket 13: text longer than the note scrolls with the mouse wheel; the scrollbar shows only on
//! hover and only when the text overflows; the caret is kept in view; text ends above the toolbar.

use slint::platform::Key;
use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

const CENTER: (f32, f32) = (220.0, 200.0);

fn long_text() -> String {
    let mut t = String::from("# Long note");
    for i in 1..=30 {
        t.push_str(&format!("\nLine {i} of a long note that keeps going so the text has to scroll: a few more words."));
    }
    t
}

fn note(text: &str) -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, text);
    let _ = h.render(0);
    h.advance(ms(5));
    h
}

/// The bottom of the last resting line, in the paper's frame.
fn last_line_bottom(h: &Harness) -> f32 {
    h.text_runs(0).iter().map(|r| r.2).fold(0.0, f32::max) + 29.1
}

#[test]
fn the_wheel_scrolls_overflowing_text_and_stops_at_both_ends() {
    let mut h = note(&long_text());
    h.pointer_move(0, CENTER.0, CENTER.1);
    assert_eq!(h.scroll_offset(0), 0.0);
    h.wheel(0, CENTER.0, CENTER.1, -60.0);
    assert_eq!(h.scroll_offset(0), 60.0);
    h.wheel(0, CENTER.0, CENTER.1, 20.0);
    assert_eq!(h.scroll_offset(0), 40.0);
    h.wheel(0, CENTER.0, CENTER.1, 1000.0);
    assert_eq!(h.scroll_offset(0), 0.0, "never above the top");
    h.wheel(0, CENTER.0, CENTER.1, -100000.0);
    let end = h.scroll_offset(0);
    // the last line's bottom sits at the bottom of the text area, not beyond
    assert!((last_line_bottom(&h) - end - h.text_view_height(0)).abs() < 1.0, "end {end}");
}

#[test]
fn the_wheel_changes_which_lines_are_visible() {
    let mut h = note(&long_text());
    h.pointer_move(0, CENTER.0, CENTER.1);
    let view = h.text_view_height(0);
    let visible = |h: &Harness| -> Vec<String> {
        let off = h.scroll_offset(0);
        h.text_runs(0).into_iter().filter(|r| r.2 - off >= 0.0 && r.2 - off + 20.0 <= view).map(|r| r.0).collect()
    };
    let before = visible(&h);
    assert!(before.join("").starts_with("Longnote"), "{before:?}");
    h.wheel(0, CENTER.0, CENTER.1, -400.0);
    let after = visible(&h);
    assert!(!after.join("").contains("Longnote"), "the title scrolled out");
    assert_ne!(before, after);
}


#[test]
fn a_note_that_fits_does_not_scroll() {
    let mut h = note("# Short\nhello");
    h.pointer_move(0, CENTER.0, CENTER.1);
    h.wheel(0, CENTER.0, CENTER.1, -200.0);
    assert_eq!(h.scroll_offset(0), 0.0);
}

#[test]
fn the_scrollbar_shows_only_when_hovered_and_overflowing() {
    let mut h = note(&long_text());
    h.advance(ms(300));
    assert_eq!(h.scrollbar_opacity(0), 0.0, "not hovered");
    h.pointer_move(0, CENTER.0, CENTER.1);
    h.advance(ms(300));
    assert_eq!(h.scrollbar_opacity(0), 1.0, "hovered and overflowing");
    h.pointer_exit(0);
    h.advance(ms(300));
    assert_eq!(h.scrollbar_opacity(0), 0.0, "pointer left");

    let mut short = note("# Short\nhello");
    short.pointer_move(0, CENTER.0, CENTER.1);
    short.advance(ms(300));
    assert_eq!(short.scrollbar_opacity(0), 0.0, "hovered but nothing to scroll");
}

#[test]
fn typing_at_the_end_of_a_long_note_keeps_the_caret_in_view() {
    let mut h = note(&long_text());
    // the caret goes to the end: Down through every block, then End
    for _ in 0..90 {
        h.press(0, Key::DownArrow);
    }
    h.press(0, Key::End);
    h.type_text(0, " and then some more typing that wraps onto further lines of the last paragraph so it grows");
    h.advance(ms(5));
    let off = h.scroll_offset(0);
    let view = h.text_view_height(0);
    let bottom = last_line_bottom(&h);
    assert!(off > 0.0, "scrolled");
    assert!(bottom - off <= view + 0.5, "caret line bottom {} is below the area {view}", bottom - off);
    assert!(bottom - off >= view - 40.0, "and not scrolled far past it");
}

#[test]
fn moving_the_caret_back_up_scrolls_back_to_it() {
    let mut h = note(&long_text());
    for _ in 0..90 {
        h.press(0, Key::DownArrow);
    }
    h.advance(ms(5));
    assert!(h.scroll_offset(0) > 0.0);
    for _ in 0..90 {
        h.press(0, Key::UpArrow);
    }
    h.advance(ms(5));
    assert_eq!(h.scroll_offset(0), 0.0);
}

#[test]
fn text_never_reaches_the_toolbar_band() {
    let h = note(&long_text());
    // the text area is shorter than the paper by the toolbar band (56), the gap (16) and the tilt allowance
    let (_, window_h) = h.note_size(0);
    let paper_h = window_h as f32 - 84.0;
    assert!(h.text_view_height(0) <= paper_h - 56.0 - 16.0 + 0.5, "view {}", h.text_view_height(0));
}
