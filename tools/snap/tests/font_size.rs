//! Settings › Font & Style › Font size: a slider (12 to 40, snapping at 14, 18, 22, 26, 30 and 36) and
//! a number box. The preview follows while the slider is dragged; on release or commit the size is
//! saved and every note lays its text out at it (size 22 is today's look, scale = size / 22).

use slint::platform::Key;
use slint::ComponentHandle;
use snap::{tab, Harness};
use std::time::Duration;
use washi::note_window::Theme;
use washi::storage::Geometry;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

const SIZE_BOX: i32 = 7;
const BLANK_PAPER: i32 = 20;
const fn slider_at(v: i32) -> i32 {
    100 + v
}

fn started() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    h
}

fn open_style_tab(h: &mut Harness) {
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    h.settings_click_tab(tab::STYLE);
    h.advance_frames(ms(600));
}

fn close_settings(h: &mut Harness) {
    h.settings_close();
    h.advance_frames(ms(600));
}

/// The body size the note is laid out at (logical px), as its `Theme` has it.
fn body_size(h: &Harness, note: usize) -> f32 {
    h.app().with_note(h.id(note), |n| n.ui().global::<Theme>().get_body_size()).unwrap()
}

fn expected_body(size: i32) -> f32 {
    (22.65 * size as f64 / 22.0) as f32
}

const LINES: &str = "# Title\nfirst line\nsecond line\nthird line\nfourth line\nfifth line\nsixth line\nseventh line\neighth line\nninth line\ntenth line\neleventh line";

#[test]
fn the_slider_and_number_box_open_at_the_saved_size_22() {
    let mut h = started();
    open_style_tab(&mut h);
    assert_eq!(h.settings_font_size(), 22);
    assert_eq!(h.settings_font_size_box(), ("22".to_string(), false));
    assert_eq!(h.settings_style_preview_font_size(), 24.0, "the preview is the 24px it always was");
    assert_eq!(body_size(&h, 0), 22.65);
}

#[test]
fn dragging_within_one_of_a_snap_point_lands_on_it_and_every_other_value_is_reachable() {
    let mut h = started();
    open_style_tab(&mut h);
    for (to, lands) in [(25, 26), (27, 26), (29, 30), (13, 14), (15, 14), (35, 36), (37, 36), (28, 28), (32, 32), (12, 12), (40, 40), (20, 20), (24, 24)] {
        h.settings_drag_font_size(22, to);
        assert_eq!(h.settings_font_size(), lands, "dragging to {to}");
        assert_eq!(h.settings_font_size_box().0, lands.to_string(), "the number box follows");
    }
}

#[test]
fn clicking_the_track_jumps_there() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(slider_at(33));
    assert_eq!(h.settings_font_size(), 33);
    h.settings_style_click(slider_at(17));
    assert_eq!(h.settings_font_size(), 18, "17 is within one of the 18 snap point");
}

#[test]
fn the_preview_follows_the_drag_and_the_notes_wait_for_the_release() {
    let mut h = started();
    h.set_note_text(0, LINES);
    open_style_tab(&mut h);
    h.settings_hold_font_size(22, 34);
    assert_eq!(h.settings_font_size(), 34);
    assert!((h.settings_style_preview_font_size() - 24.0 * 34.0 / 22.0).abs() < 1e-3, "the preview grew with the drag");
    assert_eq!(body_size(&h, 0), 22.65, "nothing is saved or applied while dragging");
    h.settings_release_at_font_size(34);
    assert!((body_size(&h, 0) - expected_body(34)).abs() < 1e-3, "released: every note follows");
}

#[test]
fn arrow_keys_home_and_end_move_the_slider_and_save() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(slider_at(22));
    h.settings_press_key(Key::RightArrow);
    assert_eq!(h.settings_font_size(), 23, "every whole value is reachable, snap points or not");
    assert!((body_size(&h, 0) - expected_body(23)).abs() < 1e-3);
    h.settings_press_key(Key::LeftArrow);
    h.settings_press_key(Key::LeftArrow);
    assert_eq!(h.settings_font_size(), 21);
    h.settings_press_key(Key::Home);
    assert_eq!(h.settings_font_size(), 12);
    h.settings_press_key(Key::LeftArrow);
    assert_eq!(h.settings_font_size(), 12, "stops at the end");
    h.settings_press_key(Key::End);
    assert_eq!(h.settings_font_size(), 40);
    h.settings_press_key(Key::RightArrow);
    assert_eq!(h.settings_font_size(), 40);
    assert!((body_size(&h, 0) - expected_body(40)).abs() < 1e-3);
}

#[test]
fn the_number_box_commits_on_enter_clamps_and_takes_only_digits() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(SIZE_BOX);
    assert!(h.settings_font_size_box().1, "clicking the box gives it the keyboard");
    h.settings_type("3a1");
    assert_eq!(h.settings_font_size_box().0, "31", "letters are refused");
    assert_eq!(h.settings_font_size(), 22, "nothing is committed before Enter");
    h.settings_press_key(Key::Return);
    assert_eq!(h.settings_font_size(), 31);
    assert!((body_size(&h, 0) - expected_body(31)).abs() < 1e-3);

    for (typed, clamped) in [("99", 40), ("5", 12), ("0", 12), ("13", 13)] {
        // (Enter keeps the box and its text selected, so the next number replaces it)
        h.settings_type(typed);
        h.settings_press_key(Key::Return);
        assert_eq!(h.settings_font_size(), clamped, "typed {typed}");
        assert_eq!(h.settings_font_size_box().0, clamped.to_string(), "the box shows what was kept");
    }
}

#[test]
fn up_and_down_step_the_number_box_and_escape_restores_it() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(SIZE_BOX);
    h.settings_press_key(Key::UpArrow);
    assert_eq!(h.settings_font_size(), 23);
    h.settings_press_key(Key::DownArrow);
    h.settings_press_key(Key::DownArrow);
    assert_eq!(h.settings_font_size(), 21);
    assert!((body_size(&h, 0) - expected_body(21)).abs() < 1e-3, "stepping commits");

    h.settings_type("37");
    assert_eq!(h.settings_font_size_box().0, "37");
    h.settings_press_key(Key::Escape);
    assert_eq!(h.settings_font_size_box().0, "21", "Esc puts the old number back");
    assert!(h.is_settings_open(), "and does not close Settings");
    assert_eq!(h.settings_font_size(), 21);
}

#[test]
fn leaving_the_number_box_commits_what_was_typed() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(SIZE_BOX);
    h.settings_type("27");
    h.settings_style_click(BLANK_PAPER);
    assert!(!h.settings_font_size_box().1, "the box let go of the keyboard");
    assert_eq!(h.settings_font_size(), 27);
    assert!((body_size(&h, 0) - expected_body(27)).abs() < 1e-3);

    h.settings_style_click(SIZE_BOX);
    h.settings_type("");
    h.settings_press_key(Key::Backspace);
    h.settings_press_key(Key::Backspace);
    h.settings_style_click(BLANK_PAPER);
    assert_eq!(h.settings_font_size(), 27, "an empty box leaves the size as it was");
    assert_eq!(h.settings_font_size_box().0, "27");
}

#[test]
fn the_size_is_saved_and_survives_reopening_settings_and_restarting() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_drag_font_size(22, 30);
    close_settings(&mut h);
    open_style_tab(&mut h);
    assert_eq!(h.settings_font_size(), 30, "Settings shows the saved size again");
    assert_eq!(h.settings_font_size_box().0, "30");
    close_settings(&mut h);

    h.restart().unwrap();
    assert!((body_size(&h, 0) - expected_body(30)).abs() < 1e-3, "a new run lays notes out at it");
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    h.settings_click_tab(tab::STYLE);
    h.advance_frames(ms(600));
    assert_eq!(h.settings_font_size(), 30);
}

#[test]
fn text_scales_with_the_size_and_size_22_is_exactly_today() {
    let mut h = started();
    h.set_note_text(0, "# Heading\nsome body text here\n[ ] a check item");
    let level = |h: &Harness| h.text_runs(0).into_iter().map(|r| (r.0, r.1, r.2, r.5)).collect::<Vec<_>>();
    let before = level(&h);
    let (w, hh, pixels) = h.render(0);
    let tops = |h: &Harness| h.blocks(0).iter().enumerate().map(|(i, _)| h.laid_out_caret(0, i, 0).1).collect::<Vec<_>>();
    let tops22 = tops(&h);

    h.app().set_font_size(40);
    h.advance(ms(20));
    let at40 = tops(&h);
    for (a, b) in tops22.iter().zip(&at40) {
        assert!(b >= a, "the blocks are further apart at 40");
    }
    assert!(at40[2] - at40[1] > (tops22[2] - tops22[1]) * 1.5, "the pitch grew with the size");
    assert_ne!(level(&h), before);

    h.app().set_font_size(22);
    h.advance(ms(20));
    assert_eq!(level(&h), before, "back at 22 every word is where it was");
    assert_eq!(tops(&h), tops22);
    let (w2, h2, again) = h.render(0);
    assert_eq!((w2, h2), (w, hh));
    assert!(again == pixels, "and the note is drawn pixel for pixel as before");
}

#[test]
fn the_paper_insets_do_not_scale() {
    let mut h = started();
    h.set_note_text(0, "# Title\nbody");
    let left = |h: &Harness| h.text_runs(0).iter().map(|r| r.1).fold(f32::MAX, f32::min);
    let at22 = left(&h);
    h.app().set_font_size(40);
    h.advance(ms(20));
    assert_eq!(left(&h), at22, "the text still starts at the same inset");
}

#[test]
fn every_open_note_and_all_notes_reflow_when_the_size_is_applied() {
    let mut h = started();
    h.new_note_from(0);
    h.advance(ms(20));
    h.set_note_text(0, LINES);
    h.set_note_text(1, LINES);
    h.app().set_font_size(30);
    h.advance(ms(20));
    for note in 0..2 {
        assert!((body_size(&h, note) - expected_body(30)).abs() < 1e-3, "note {note}");
    }
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    h.advance_frames(ms(600));
    let all = h.app().all_notes().expect("All notes is open");
    assert!((all.ui().global::<Theme>().get_body_size() - expected_body(30)).abs() < 1e-3);
    h.app().set_font_size(16);
    h.advance(ms(20));
    let all = h.app().all_notes().unwrap();
    assert!((all.ui().global::<Theme>().get_body_size() - expected_body(16)).abs() < 1e-3, "All notes follows a later change too");
}

#[test]
fn a_note_scrolled_down_is_kept_inside_its_text_when_a_smaller_size_shortens_it() {
    let mut h = started();
    h.set_note_text(0, LINES);
    h.app().set_font_size(40);
    h.advance(ms(20));
    let view = h.text_view_height(0);
    h.wheel(0, 150.0, 200.0, -2000.0);
    h.advance(ms(300));
    assert!(h.scroll_offset(0) > 50.0, "scrolled down at 40: {}", h.scroll_offset(0));
    h.app().set_font_size(12);
    h.advance(ms(300));
    let content_at_12 = h.laid_out_caret(0, h.blocks(0).len() - 1, 0).2;
    assert!(content_at_12 < view, "at 12 the text fits ({content_at_12} in {view})");
    assert_eq!(h.scroll_offset(0), 0.0, "so the scroll is back at the top");
}

/// Every `step`th caret position of the note: the caret as drawn against the line box of the layout.
fn check_carets_at(size: u32, step: usize) {
    let mut h = started();
    h.set_tilted_editing(false);
    h.set_note_text(0, include_str!("fixtures/mixed-note.md").replace("\r\n", "\n").trim_end_matches('\n'));
    h.app().set_font_size(size);
    h.advance(ms(20));
    h.place_caret_at(0, 0, 0);
    let mut checked = 0;
    for (block, (kind, text)) in h.blocks(0).iter().enumerate() {
        if kind == "image" {
            continue;
        }
        let ends: Vec<usize> = (0..=text.len()).filter(|&o| text.is_char_boundary(o)).collect();
        for offset in ends.iter().copied().enumerate().filter(|(i, o)| i % step == 0 || *o == text.len()).map(|(_, o)| o) {
            h.place_caret_at(0, block, offset);
            let (left, top, right, bottom) = h.drawn_caret(0).unwrap_or_else(|| panic!("size {size}: no caret at block {block} offset {offset}"));
            let (x, line_top, line_bottom) = h.laid_out_caret(0, block, offset);
            let at = format!("size {size}, block {block} offset {offset}: caret x {left}..{right} y {top}..{bottom}, line x {x} y {line_top}..{line_bottom}");
            assert!(right - left <= 3 && (left as f32 - x).abs() <= 1.0, "{at}");
            assert!((top as f32 - line_top).abs() <= 1.0 && (bottom as f32 - line_bottom).abs() <= 1.0, "{at}");
            checked += 1;
        }
    }
    assert!(checked > 30, "{checked} positions");
}

#[test]
fn at_12_and_40_the_caret_is_on_its_line_in_every_block() {
    check_carets_at(12, 16);
    check_carets_at(40, 16);
}

#[test]
fn at_12_and_40_a_note_can_be_edited_and_keeps_its_text() {
    for size in [12, 40] {
        let mut h = started();
        h.app().set_font_size(size);
        h.advance(ms(20));
        h.set_note_text(0, "# Title\nbody text");
        h.place_caret_at(0, 1, 4);
        h.type_text(0, " more");
        assert_eq!(h.blocks(0)[1].1, "body more text", "size {size}");
        let (_, top, right_bottom) = h.laid_out_caret(0, 1, 9);
        assert!(top > 0.0 && right_bottom > top);
    }
}

#[test]
fn nothing_but_the_font_size_changes_when_only_other_controls_are_used() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(0);
    h.settings_style_click(3);
    h.settings_style_click(10);
    h.settings_style_click(12);
    assert_eq!(body_size(&h, 0), 22.65, "the Font and Tape color pickers still change only the preview");
}

#[test]
fn the_rows_and_the_tabs_height_stay_put_whatever_the_preview_size() {
    let mut h = started();
    open_style_tab(&mut h);
    let bottom = h.settings_content_bottoms()[tab::STYLE as usize];
    let total = h.settings_window_geometry().unwrap().3;
    for size in [12, 40, 22] {
        h.settings_style_click(slider_at(size));
        assert_eq!(h.settings_font_size(), size);
        assert_eq!(h.settings_content_bottoms()[tab::STYLE as usize], bottom, "the last row ends where it did (size {size})");
        assert_eq!(h.settings_window_geometry().unwrap().3, total, "and the Paper is as tall (size {size})");
    }
}
