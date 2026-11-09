//! Settings › Appearance: the Tab's place in the Sidebar and the UI size choice (saved, and the size
//! new Notes open at).

use snap::{tab, Harness};
use std::time::Duration;
use washi::storage::Geometry;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn started() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    h
}

/// Settings open on `t`, the transition and the tab switch finished.
fn open_on(h: &mut Harness, t: i32) {
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    if t != tab::GENERAL {
        h.settings_click_tab(t);
        h.advance_frames(ms(600));
    }
}

#[test]
fn the_sidebar_lists_five_tabs_with_appearance_second() {
    let mut h = started();
    open_on(&mut h, tab::GENERAL);
    assert_eq!(h.settings_tab_names(), ["General", "Appearance", "Font & Style", "Shortcuts", "Privacy"]);

    h.settings_click_tab(tab::APPEARANCE);
    h.advance_frames(ms(600));
    assert_eq!(h.settings_shown_tab(), tab::APPEARANCE);
    assert_eq!(h.settings_title(), "Appearance");
}

// Control ids, as `hit-x` in appearance-tab.slint defines them.
const UI_SIZE_BUTTON: i32 = 0;
const SMALL: i32 = 1;
const MEDIUM: i32 = 2;
const LARGE: i32 = 3;
const BLANK_PAPER: i32 = 20;

/// A new Note's window: the 363 by 384 Paper and its 42px margins, all times the UI size's scale.
fn new_note_window(scale: f32) -> (u32, u32) {
    (((363.0 + 2.0 * 42.0) * scale).round() as u32, ((384.0 + 2.0 * 42.0) * scale).round() as u32)
}

fn close_settings(h: &mut Harness) {
    h.settings_close();
    h.advance_frames(ms(600));
    assert!(!h.is_settings_open());
}

fn choose_ui_size(h: &mut Harness, option: i32) {
    open_on(h, tab::APPEARANCE);
    h.settings_appearance_click(UI_SIZE_BUTTON);
    h.settings_appearance_click(option);
    close_settings(h);
}

fn new_note(h: &mut Harness) -> usize {
    h.right_click(0, Some(&["New note"]));
    h.open_note_count() - 1
}

#[test]
fn ui_size_is_large_until_chosen() {
    let mut h = started();
    open_on(&mut h, tab::APPEARANCE);
    assert_eq!(h.settings_ui_size(), 2);
    assert_eq!(h.settings_ui_size_label(), "Large 100%");
    assert_eq!(h.settings_appearance_open_list(), -1);
}

#[test]
fn the_ui_size_dropdown_opens_chooses_and_closes() {
    let mut h = started();
    open_on(&mut h, tab::APPEARANCE);
    h.settings_appearance_click(UI_SIZE_BUTTON);
    assert_eq!(h.settings_appearance_open_list(), 0, "the list is open");
    h.settings_appearance_click(SMALL);
    assert_eq!(h.settings_appearance_open_list(), -1, "choosing closes the list");
    assert_eq!((h.settings_ui_size(), h.settings_ui_size_label().as_str()), (0, "Small 75%"));

    h.settings_appearance_click(UI_SIZE_BUTTON);
    h.settings_appearance_click(MEDIUM);
    assert_eq!((h.settings_ui_size(), h.settings_ui_size_label().as_str()), (1, "Medium 80%"));

    h.settings_appearance_click(UI_SIZE_BUTTON);
    h.settings_appearance_click(LARGE);
    assert_eq!((h.settings_ui_size(), h.settings_ui_size_label().as_str()), (2, "Large 100%"));
}

#[test]
fn an_outside_click_or_escape_closes_the_list_and_changes_nothing() {
    let mut h = started();
    open_on(&mut h, tab::APPEARANCE);
    h.settings_appearance_click(UI_SIZE_BUTTON);
    h.settings_appearance_click(BLANK_PAPER);
    assert_eq!(h.settings_appearance_open_list(), -1);
    assert_eq!(h.settings_ui_size(), 2);

    h.settings_appearance_click(UI_SIZE_BUTTON);
    h.settings_press_escape();
    assert!(h.is_settings_open(), "Escape closes the list first");
    assert_eq!(h.settings_appearance_open_list(), -1);
    assert_eq!(h.settings_ui_size(), 2);
}

#[test]
fn the_chosen_ui_size_is_kept_when_settings_reopens_and_after_a_restart() {
    let mut h = started();
    choose_ui_size(&mut h, MEDIUM);
    open_on(&mut h, tab::APPEARANCE);
    assert_eq!(h.settings_ui_size(), 1, "Settings opened again");
    close_settings(&mut h);

    h.restart().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    open_on(&mut h, tab::APPEARANCE);
    assert_eq!(h.settings_ui_size(), 1, "Washi started again");
}

#[test]
fn a_new_note_opens_at_the_default_size_times_the_ui_size() {
    let mut h = started();
    let first = h.note_size(0);
    assert_eq!(first, new_note_window(1.0));
    for (option, scale) in [(SMALL, 0.75), (MEDIUM, 0.80), (LARGE, 1.0)] {
        choose_ui_size(&mut h, option);
        let n = new_note(&mut h);
        assert_eq!(h.note_size(n), new_note_window(scale), "UI size {option}");
        assert_eq!(h.note_size(0), first, "an open note keeps its size");
    }
}

#[test]
fn a_new_note_ignores_the_size_of_the_note_it_is_opened_from() {
    let mut h = started();
    h.resize_note(0, 600, 560);
    choose_ui_size(&mut h, SMALL);
    let n = new_note(&mut h);
    assert_eq!(h.note_size(n), new_note_window(0.75));
}

#[test]
fn the_fresh_note_after_a_restart_with_nothing_open_uses_the_ui_size() {
    let mut h = started();
    choose_ui_size(&mut h, SMALL);
    h.press_ctrl(0, 'w');
    assert!(h.has_quit());
    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.note_size(0), new_note_window(0.75));
}
