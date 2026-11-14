//! Caret and focus fixes, ticket 02: the scrollbar thumb, invisible until hovered, no longer takes
//! clicks meant for the text beside it. A click on a line, right up to the text's right edge and
//! past it into the margin beside the thumb, places the caret there; the thumb still shows on hover
//! and can still be dragged.

use snap::{ms, Harness};

/// The UI scales (monitor DPI / 96) the clicks are made at: user story 28.
const SCALES: [f32; 4] = [1.0, 1.25, 1.5, 2.0];

/// A Note of `n` long paragraphs below a title.
fn paragraphs(n: usize) -> String {
    let mut t = String::from("# Long note");
    for i in 1..=n {
        t.push_str(&format!("\nLine {i} of a long note that keeps going so the text has to scroll: a few more words."));
    }
    t
}

/// The level tops of the resting lines, in the paper's frame (content px, before scrolling).
fn line_tops(h: &Harness) -> Vec<f32> {
    let mut ys: Vec<f32> = h.text_runs(0).iter().map(|r| r.2).collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    ys
}

/// Scroll to the top or the bottom, leaving the bar to fade out (or kept showing, `show`).
fn scroll_to(h: &mut Harness, bottom: bool, show: bool) {
    h.pointer_move(0, 200.0, 200.0);
    h.wheel(0, 200.0, 200.0, if bottom { -100000.0 } else { 100000.0 });
    h.advance(ms(if show { 5 } else { 1000 }));
}

/// The visible lines' middles, `v` px below the paper's top edge.
fn visible_line_middles(h: &Harness) -> Vec<f32> {
    let (_, ph) = h.paper_size(0);
    let off = h.scroll_offset(0);
    // the text area runs from about the text's top inset to above the toolbar band
    line_tops(h).into_iter().map(|y| y - off + 12.0).filter(|&v| v > 45.0 && v < ph - 60.0).collect()
}

/// Click at `(u, v)` on the paper (from its top-left corner) and return the caret it leaves and
/// the focused field.
fn click_paper(h: &mut Harness, u: f32, v: f32) -> ((usize, usize), i32) {
    let (x, y) = h.paper_point(0, u, v);
    h.click(0, x, y);
    h.advance(ms(400));
    let (a, f) = h.selection(0);
    assert_eq!(a, f, "a click leaves a caret, not a selection");
    (f, h.text_focused_block(0))
}

/// Whether `(u, v)` on the paper falls on the menu button (top right), which has a click of its own.
fn on_menu_button(h: &Harness, u: f32, v: f32) -> bool {
    let (x, y) = h.paper_point(0, u, v);
    let (bx, by, bw, bh) = h.menu_button_rect(0);
    x >= bx - 2.0 && x <= bx + bw + 2.0 && y >= by - 2.0 && y <= by + bh + 2.0
}

/// How many times `ch` occurs in paragraph `block` of the Note's text.
fn count_in(h: &Harness, block: usize, ch: char) -> usize {
    h.note_text(0).split('\n').nth(block).unwrap_or("").matches(ch).count()
}

#[test]
fn a_click_at_the_end_of_any_line_beside_the_hidden_thumb_places_the_caret_and_typing_lands_there() {
    clicks_beside_the_hidden_thumb_type_there(SCALES[0]);
}

#[test]
fn a_click_beside_the_hidden_thumb_types_there_at_125_percent() {
    clicks_beside_the_hidden_thumb_type_there(SCALES[1]);
}

#[test]
fn a_click_beside_the_hidden_thumb_types_there_at_150_percent() {
    clicks_beside_the_hidden_thumb_type_there(SCALES[2]);
}

#[test]
fn a_click_beside_the_hidden_thumb_types_there_at_200_percent() {
    clicks_beside_the_hidden_thumb_type_there(SCALES[3]);
}

/// On a monitor at `scale`: a click at the end of every visible line, from the text's last 20 px
/// out to the hidden thumb's strip, puts the caret in that line's paragraph, and typing lands there.
fn clicks_beside_the_hidden_thumb_type_there(scale: f32) {
    for n in [3, 8] {
        let text = paragraphs(n);
        let mut h = Harness::sized_note(300, 300, &text);
        if scale != 1.0 {
            h.move_to_monitor(0, scale);
        }
        assert!(h.note_size(0).0 <= 444, "the smallest window: {:?}", h.note_size(0));
        let (pw, _) = h.paper_size(0);
        let inset_right = h.text_inset_right(0);
        for bottom in [false, true] {
            scroll_to(&mut h, bottom, false);
            assert!(!bottom || h.scroll_offset(0) > 0.0, "the Note overflows");
            for v in visible_line_middles(&h) {
                // the line's paragraph, read by a click in its middle (nowhere near the bar)
                scroll_to(&mut h, bottom, false);
                let (mid, _) = click_paper(&mut h, pw / 2.0, v);
                // from the text's last 20 px out across the margin, up to the thumb's own strip
                let mut u = pw - inset_right - 20.0;
                while u <= pw - 20.0 {
                    if on_menu_button(&h, u, v) {
                        u += 4.0;
                        continue;
                    }
                    h.set_note_text(0, &text);
                    let _ = h.render(0);
                    // the caret starts in another paragraph (placing it may scroll: scroll back)
                    let elsewhere = if mid.0 == 1 { 2 } else { 1 };
                    h.place_caret(0, elsewhere);
                    h.advance(ms(5));
                    scroll_to(&mut h, bottom, false);
                    assert_eq!(h.scrollbar_opacity(0), 0.0, "the bar is hidden");
                    let ((block, offset), focused) = click_paper(&mut h, u, v);
                    let at = format!("{scale}x n {n} bottom {bottom} line v {v:.1} u {u:.0}");
                    assert_eq!(block, mid.0, "the caret went to the clicked paragraph: {at}");
                    assert!(offset >= mid.1, "and to the end side of the line: {at}");
                    assert_eq!(focused, block as i32, "its field has the keyboard focus: {at}");
                    let before = count_in(&h, block, 'Z');
                    h.type_text(0, "Z");
                    h.advance(ms(5));
                    assert_eq!(count_in(&h, block, 'Z'), before + 1, "the character landed there: {at}");
                    u += 4.0;
                }
            }
        }
    }
}

#[test]
fn hovering_the_thumb_at_the_smallest_window_shows_the_bar_and_dragging_it_scrolls() {
    let mut h = Harness::sized_note(300, 300, &paragraphs(8));
    h.advance(ms(1000));
    assert_eq!(h.scrollbar_opacity(0), 0.0);
    let (tx, ty) = h.scrollbar_thumb(0);
    h.pointer_move(0, tx, ty);
    h.advance(ms(300));
    assert_eq!(h.scrollbar_opacity(0), 1.0, "hovering the thumb shows the bar");

    h.press_button(0, tx, ty, false);
    assert_eq!(h.scroll_offset(0), 0.0, "pressing the thumb does not jump");
    h.pointer_move(0, tx, ty + 60.0);
    assert!(h.scroll_offset(0) > 50.0, "dragging the thumb scrolls: {}", h.scroll_offset(0));
    h.release_button(0, tx, ty + 60.0, false);
}

#[test]
fn a_click_just_past_the_texts_right_edge_places_the_caret_even_while_the_bar_shows_at_every_size() {
    clicks_past_the_texts_right_edge_with_the_bar_showing(SCALES[0]);
}

#[test]
fn a_click_past_the_texts_right_edge_with_the_bar_showing_at_125_percent() {
    clicks_past_the_texts_right_edge_with_the_bar_showing(SCALES[1]);
}

#[test]
fn a_click_past_the_texts_right_edge_with_the_bar_showing_at_150_percent() {
    clicks_past_the_texts_right_edge_with_the_bar_showing(SCALES[2]);
}

#[test]
fn a_click_past_the_texts_right_edge_with_the_bar_showing_at_200_percent() {
    clicks_past_the_texts_right_edge_with_the_bar_showing(SCALES[3]);
}

/// On a monitor at `scale`, at every size: a click just past the text's right edge, with the bar
/// showing beside it, puts the caret in the clicked line's paragraph and focuses its field.
fn clicks_past_the_texts_right_edge_with_the_bar_showing(scale: f32) {
    // From the smallest window to a tall one; each Note barely overflows, so its thumb is long and
    // runs beside many lines (a long tilted thumb leans furthest toward the text).
    for (w, wh) in [(300, 300), (600, 700), (500, 1400)] {
        let mut h = Harness::sized_note(w, wh, "# Title");
        if scale != 1.0 {
            h.move_to_monitor(0, scale);
        }
        let mut n = 1;
        loop {
            h.set_note_text(0, &paragraphs(n));
            let _ = h.render(0);
            scroll_to(&mut h, true, true);
            if h.scroll_offset(0) > 0.0 {
                break;
            }
            n += 1;
        }
        let (pw, _) = h.paper_size(0);
        let inset_right = h.text_inset_right(0);
        for bottom in [false, true] {
            scroll_to(&mut h, bottom, true);
            for v in visible_line_middles(&h) {
                scroll_to(&mut h, bottom, true);
                let (mid, _) = click_paper(&mut h, pw / 2.0, v);
                // just past the text's right edge, with the bar showing (scrolling just now)
                let elsewhere = if mid.0 == 1 { 2 } else { 1 };
                h.place_caret(0, elsewhere);
                h.advance(ms(5));
                scroll_to(&mut h, bottom, true);
                assert_eq!(h.scrollbar_opacity(0), 1.0, "the bar shows");
                let ((block, _), focused) = click_paper(&mut h, pw - inset_right + 2.0, v);
                let at = format!("{scale}x {w}x{wh} bottom {bottom} line v {v:.1}");
                assert_eq!(block, mid.0, "the caret went to the clicked paragraph: {at}");
                assert_eq!(focused, block as i32, "its field has the keyboard focus: {at}");
            }
        }
    }
}

