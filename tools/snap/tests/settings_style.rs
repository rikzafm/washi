//! The Settings page's Font & Style tab: every control works and moves the preview, and nothing leaves
//! the page but Letter spacing, which is saved and lays out every note's text again.

use snap::{tab, Harness};
use std::time::Duration;
use washi::storage::Geometry;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

// Control ids, as `hit-x` in style-tab.slint defines them (0, the manage button, is in font_manager.rs).
/// The Font size slider where the value `v` is (the box and the saving are in font_size.rs).
const fn size_slider(v: i32) -> i32 {
    100 + v
}
const TAPE_BUTTON: i32 = 10;
const fn tape_item(i: i32) -> i32 {
    11 + i
}
const BLANK_PAPER: i32 = 20;
const fn spacing_seg(i: i32) -> i32 {
    21 + i
}

/// Settings open on the Font & Style tab, with the transition and tab switch finished.
fn open_style_tab(h: &mut Harness) {
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    h.settings_click_tab(tab::STYLE);
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
    assert_eq!(h.settings_style_state(), (22, 0, -1));
    assert_eq!(h.settings_style_preview_font_size(), 24.0);
}

#[test]
fn the_font_size_slider_resizes_the_preview() {
    let mut h = started();
    open_style_tab(&mut h);
    for (size, px) in [(12, 24.0 * 12.0 / 22.0), (40, 24.0 * 40.0 / 22.0), (22, 24.0)] {
        h.settings_style_click(size_slider(size));
        assert_eq!(h.settings_style_state().0, size);
        assert!((h.settings_style_preview_font_size() - px).abs() < 1e-3, "size {size}: {}", h.settings_style_preview_font_size());
    }
}

#[test]
fn the_tape_picker_opens_chooses_and_closes() {
    let mut h = started();
    open_style_tab(&mut h);

    h.settings_style_click(TAPE_BUTTON);
    assert_eq!(h.settings_style_state().2, 1, "tape list open");
    h.settings_style_click(tape_item(3));
    assert_eq!(h.settings_style_state().1, 3, "the fourth colorway (Slate + rose) can be chosen");
    assert_eq!(h.settings_style_state().2, -1, "choosing closes the list");
}

#[test]
fn an_outside_click_closes_the_open_list_and_changes_nothing() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(TAPE_BUTTON);
    assert_eq!(h.settings_style_state().2, 1);

    h.settings_style_click(BLANK_PAPER);
    assert_eq!(h.settings_style_state(), (22, 0, -1), "outside click closes and changes nothing");
}

#[test]
fn escape_closes_an_open_list_before_it_closes_settings() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(TAPE_BUTTON);
    h.settings_press_escape();
    assert!(h.is_settings_open());
    assert_eq!(h.settings_style_state().2, -1);
    h.settings_press_escape();
    h.advance_frames(ms(600));
    assert!(!h.is_settings_open());
}

#[test]
fn choices_survive_a_tab_switch_but_not_reopening_settings() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(TAPE_BUTTON);
    h.settings_style_click(tape_item(2));
    h.settings_click_tab(tab::GENERAL);
    h.advance_frames(ms(600));
    h.settings_click_tab(tab::STYLE);
    h.advance_frames(ms(600));
    assert_eq!(h.settings_style_state(), (22, 2, -1));

    h.settings_close();
    h.advance_frames(ms(600));
    open_style_tab(&mut h);
    assert_eq!(h.settings_style_state(), (22, 0, -1), "defaults again");
}

#[test]
fn letter_spacing_is_saved_and_widens_every_note() {
    let mut h = started();
    h.set_note_text(0, "# Host\njpg jpg jpg");
    let before = h.text_runs(0);
    let end = |runs: &[(String, f32, f32, f32, f32, f32)]| runs.iter().map(|r| r.1 + r.5).fold(0.0, f32::max);
    open_style_tab(&mut h);
    assert_eq!(h.settings_style_spacing(), 1, "Normal until chosen");
    h.settings_style_click(spacing_seg(2));
    assert_eq!(h.settings_style_spacing(), 2);
    h.settings_close();
    h.advance_frames(ms(600));
    let loose = h.text_runs(0);
    assert!(end(&loose) > end(&before) + 10.0, "the words moved apart: {} against {}", end(&loose), end(&before));

    // Saved: Settings shows it again, and Normal brings the words back where they were.
    open_style_tab(&mut h);
    assert_eq!(h.settings_style_spacing(), 2);
    h.settings_style_click(spacing_seg(1));
    h.settings_close();
    h.advance_frames(ms(600));
    // (the level layout: the offsets onto the tilted baseline follow the note's size, which Settings
    // opening and closing leaves a fraction of a pixel off)
    let level = |runs: Vec<(String, f32, f32, f32, f32, f32)>| runs.into_iter().map(|r| (r.0, r.1, r.2, r.5)).collect::<Vec<_>>();
    assert_eq!(level(h.text_runs(0)), level(before), "Normal is the font's own spacing");
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
            h.settings_style_click(TAPE_BUTTON);
            h.settings_style_click(tape_item(1));
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

/// Where the mockup's Font & Style content ends (its 603.2 Paper less the 44px bottom padding), less
/// the two rows taken out of it since: Fade text edge (80px, its hint on two lines, and its 2px rule)
/// and UI size (the 56px minimum, the last row, so no rule; the Tape color row above it loses its rule
/// instead), plus the Letter spacing row added under Tape color since (58px with the rule Tape color
/// has again).
const STYLE_CONTENT_BOTTOM: f32 = 603.2 - 44.0 - 82.0 - 56.0 - 2.0 + 58.0 + 2.0;

#[test]
fn the_tab_has_no_fade_or_ui_size_row_and_its_rows_close_up() {
    let mut h = started();
    open_style_tab(&mut h);
    let bottom = h.settings_content_bottoms()[tab::STYLE as usize];
    assert!((bottom - STYLE_CONTENT_BOTTOM).abs() <= 3.0, "content ends at {bottom}, want about {STYLE_CONTENT_BOTTOM}");
    let (_, _, _, total) = h.settings_window_geometry().unwrap();
    // Shorter than the 600 minimum now, so the Paper is 600 (plus the 42px margins).
    assert_eq!(total, 600 + 2 * 42);
}
