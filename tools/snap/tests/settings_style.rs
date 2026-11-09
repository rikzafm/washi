//! The Settings page's Font & Style tab: every control works and moves the preview, and nothing leaves
//! the page.

use snap::Harness;
use std::time::Duration;
use washi::storage::Geometry;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

// Control ids, as `hit-x` in style-tab.slint defines them.
const FONT_BUTTON: i32 = 0;
const fn font_item(i: i32) -> i32 {
    1 + i
}
const fn size_seg(i: i32) -> i32 {
    7 + i
}
const TAPE_BUTTON: i32 = 10;
const fn tape_item(i: i32) -> i32 {
    11 + i
}
const FADE: i32 = 15;
const fn ui_seg(i: i32) -> i32 {
    16 + i
}
const BLANK_PAPER: i32 = 20;

/// Settings open on the Font & Style tab, with the transition and tab switch finished.
fn open_style_tab(h: &mut Harness) {
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    h.settings_click_tab(1);
    h.advance_frames(ms(600));
}

fn started() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    h
}

#[test]
fn the_tab_opens_on_the_defaults() {
    let mut h = started();
    open_style_tab(&mut h);
    assert_eq!(h.settings_style_state(), (0, 1, 0, true, 0, -1));
    assert_eq!(h.settings_style_preview_font_size(), 24.0);
}

#[test]
fn text_size_buttons_resize_the_preview() {
    let mut h = started();
    open_style_tab(&mut h);
    for (seg, px) in [(0, 19.0), (2, 30.0), (1, 24.0)] {
        h.settings_style_click(size_seg(seg));
        assert_eq!(h.settings_style_state().1, seg);
        assert_eq!(h.settings_style_preview_font_size(), px);
    }
}

#[test]
fn font_and_tape_pickers_open_choose_and_close() {
    let mut h = started();
    open_style_tab(&mut h);

    h.settings_style_click(FONT_BUTTON);
    assert_eq!(h.settings_style_state().5, 0, "font list open");
    h.settings_style_click(font_item(2));
    assert_eq!(h.settings_style_state().0, 2);
    assert_eq!(h.settings_style_state().5, -1, "choosing closes the list");

    h.settings_style_click(TAPE_BUTTON);
    assert_eq!(h.settings_style_state().5, 1, "tape list open");
    h.settings_style_click(tape_item(3));
    assert_eq!(h.settings_style_state().2, 3);
    assert_eq!(h.settings_style_state().5, -1);
}

#[test]
fn the_font_list_has_the_mockups_six_fonts_and_the_tape_list_its_four_colorways() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(FONT_BUTTON);
    h.settings_style_click(font_item(5));
    assert_eq!(h.settings_style_state().0, 5, "the sixth font (Kalam) can be chosen");
    h.settings_style_click(TAPE_BUTTON);
    h.settings_style_click(tape_item(3));
    assert_eq!(h.settings_style_state().2, 3, "the fourth colorway (Slate + rose) can be chosen");
}

#[test]
fn opening_one_list_replaces_the_other_and_an_outside_click_closes_it() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(TAPE_BUTTON);
    h.settings_style_click(FONT_BUTTON);
    assert_eq!(h.settings_style_state().5, 0, "the tape list closed when the font list opened");

    h.settings_style_click(BLANK_PAPER);
    let s = h.settings_style_state();
    assert_eq!(s.5, -1, "outside click closes");
    assert_eq!((s.0, s.1, s.2), (0, 1, 0), "and changes nothing");
}

#[test]
fn escape_closes_an_open_list_before_it_closes_settings() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(FONT_BUTTON);
    h.settings_press_escape();
    assert!(h.is_settings_open());
    assert_eq!(h.settings_style_state().5, -1);
    h.settings_press_escape();
    h.advance_frames(ms(600));
    assert!(!h.is_settings_open());
}

#[test]
fn fade_toggle_and_ui_size_respond() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(FADE);
    assert!(!h.settings_style_state().3);
    h.settings_style_click(FADE);
    assert!(h.settings_style_state().3);
    h.settings_style_click(ui_seg(1));
    assert_eq!(h.settings_style_state().4, 1);
    h.settings_style_click(ui_seg(0));
    assert_eq!(h.settings_style_state().4, 0);
}

#[test]
fn choices_survive_a_tab_switch_but_not_reopening_settings() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(size_seg(2));
    h.settings_style_click(FADE);
    h.settings_click_tab(0);
    h.advance_frames(ms(600));
    h.settings_click_tab(1);
    h.advance_frames(ms(600));
    assert_eq!(h.settings_style_state(), (0, 2, 0, false, 0, -1));

    h.settings_close();
    h.advance_frames(ms(600));
    open_style_tab(&mut h);
    assert_eq!(h.settings_style_state(), (0, 1, 0, true, 0, -1), "defaults again");
}

#[test]
fn nothing_the_tab_does_reaches_any_note() {
    // Open the tab and close Settings again, with or without using the controls; the host note
    // must come back the same either way.
    let run = |use_controls: bool| {
        let mut h = started();
        h.set_note_text(0, "# Host\nsome text");
        let text = h.note_text(0);
        let startup = h.startup.enabled.get();
        open_style_tab(&mut h);
        if use_controls {
            h.settings_style_click(size_seg(2));
            h.settings_style_click(FONT_BUTTON);
            h.settings_style_click(font_item(3));
            h.settings_style_click(TAPE_BUTTON);
            h.settings_style_click(tape_item(1));
            h.settings_style_click(FADE);
            h.settings_style_click(ui_seg(1));
        }
        h.settings_close();
        h.advance_frames(ms(600));
        assert_eq!(h.note_text(0), text);
        assert_eq!(h.startup.enabled.get(), startup);
        h.render(0)
    };
    let (w, hh, untouched) = run(false);
    let (w2, h2, used) = run(true);
    assert_eq!((w2, h2), (w, hh));
    assert!(untouched == used, "the note looks exactly the same whatever was chosen");
}

#[test]
fn the_tab_is_as_tall_as_its_preview_and_rows_need() {
    let mut h = started();
    open_style_tab(&mut h);
    let (_, _, _, total) = h.settings_window_geometry().unwrap();
    // 603 in the mockup; at least the 600 minimum (see settings_fidelity.rs).
    assert!((684..=690).contains(&total), "{total}");
}
