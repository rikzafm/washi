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

// ---- Sidebar: Height and Anchor ----

/// Whether two window-px positions agree to within the anti-aliased edge of a tilted panel.
fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 2.0
}

#[test]
fn match_note_makes_the_settings_sidebar_as_tall_as_its_paper() {
    let mut h = started();
    // Shortcuts is taller than the Paper's 600 minimum.
    open_on(&mut h, tab::SHORTCUTS);
    let (top, bottom) = h.settings_sidebar_span();
    let (paper_top, paper_bottom) = h.settings_paper_span();
    assert!(paper_bottom - paper_top > 700.0, "the Paper is {paper_top}..{paper_bottom}");
    assert!(near(top, paper_top) && near(bottom, paper_bottom), "panel {top}..{bottom}, Paper {paper_top}..{paper_bottom}");
}

// Sidebar group control ids, as `hit-x` in appearance-tab.slint defines them.
const SAME_SWITCH: i32 = 4;
/// The Height button and its options, then the Anchor button and its options, of the Settings
/// sidebar (or both while shared) and of the Notes sidebar.
const HEIGHT: [i32; 2] = [5, 12];
const ANCHOR: [i32; 2] = [8, 15];
const MATCH_NOTE: i32 = 1;
const FIT_CONTENT: i32 = 2;
const TOP: i32 = 1;
const MIDDLE: i32 = 2;
const BOTTOM: i32 = 3;

/// Pick option `n` (1-based) of the Height or Anchor dropdown `button` on the open Appearance Tab.
fn pick(h: &mut Harness, button: i32, n: i32) {
    h.settings_appearance_click(button);
    h.settings_appearance_click(button + n);
}

/// The Settings sidebar's Fit-content panel: the header and five Tabs (the last one ends 337 down)
/// and the Paper's 44px bottom padding.
const SETTINGS_FIT_H: f32 = 381.0;
/// The All notes Fit-content panel with `rows` notes listed: the 104 header, the two 30px labels and
/// 14 between the groups, 54 a row, then the button (14 under the last row, 44 tall, 16 of padding).
fn notes_fit_h(rows: usize) -> f32 {
    104.0 + 30.0 + 14.0 + 30.0 + 54.0 * rows as f32 + 14.0 + 44.0 + 16.0
}

/// All notes open from note 0 and Settings from note 1 (a second note), both settled, Settings on
/// the Appearance Tab.
fn both_sidebars() -> Harness {
    let mut h = started();
    // A tall note, so a Fit-content All notes panel leaves room above and below it.
    h.resize_note(0, 447, 700);
    new_note(&mut h);
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    h.advance_frames(ms(600));
    assert!(h.is_all_notes_open());
    h.toggle_dropdown(1);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    h.settings_click_tab(tab::APPEARANCE);
    h.advance_frames(ms(600));
    assert!(h.is_settings_open());
    h
}

/// Where a panel sits against its Paper: its height and its top's and bottom's offsets from the
/// Paper's top and bottom.
fn against((top, bottom): (f32, f32), (paper_top, paper_bottom): (f32, f32)) -> (f32, f32, f32) {
    (bottom - top, top - paper_top, paper_bottom - bottom)
}

fn settings_place(h: &Harness) -> (f32, f32, f32) {
    against(h.settings_sidebar_span(), h.settings_paper_span())
}

fn notes_place(h: &Harness) -> (f32, f32, f32) {
    against(h.all_notes_sidebar_span(), h.all_notes_paper_span())
}

#[test]
fn match_note_is_the_default_and_makes_both_sidebars_as_tall_as_their_paper() {
    let h = both_sidebars();
    for (name, (_, above, below)) in [("Settings", settings_place(&h)), ("Notes", notes_place(&h))] {
        assert!(near(above, 0.0) && near(below, 0.0), "{name}: {above} above and {below} below the panel");
    }
}

#[test]
fn fit_content_makes_the_settings_sidebar_header_and_tabs_tall_and_centres_it() {
    let mut h = both_sidebars();
    pick(&mut h, HEIGHT[0], FIT_CONTENT);
    let (height, above, below) = settings_place(&h);
    assert!(near(height, SETTINGS_FIT_H), "panel {height} tall");
    assert!(near(above, below), "Middle: {above} above, {below} below");
}

#[test]
fn fit_content_makes_the_all_notes_sidebar_grow_with_its_rows_up_to_its_paper_then_scroll() {
    let mut h = both_sidebars();
    pick(&mut h, HEIGHT[0], FIT_CONTENT);
    // Two notes are listed.
    let (height, above, below) = notes_place(&h);
    assert!(near(height, notes_fit_h(2)), "panel {height} tall, wanted {}", notes_fit_h(2));
    assert!(near(above, below), "Middle: {above} above, {below} below");

    // More rows than the Paper holds: the panel stops at the Paper and the list scrolls.
    for _ in 0..8 {
        h.invoke_all_notes_new_note();
    }
    let (height, above, below) = notes_place(&h);
    assert!(near(above, 0.0) && near(below, 0.0), "capped at the Paper: {height} tall, {above} above, {below} below");
    let before = h.render_all_notes().unwrap().2;
    h.scroll_all_notes(100.0, 250.0, -60.0);
    assert!(h.render_all_notes().unwrap().2 != before, "the list scrolled");
}

#[test]
fn the_anchor_puts_a_fit_content_panel_at_the_papers_top_middle_or_bottom() {
    let mut h = both_sidebars();
    pick(&mut h, HEIGHT[0], FIT_CONTENT);
    for (anchor, name) in [(TOP, "Top"), (MIDDLE, "Middle"), (BOTTOM, "Bottom")] {
        pick(&mut h, ANCHOR[0], anchor);
        for (sidebar, (_, above, below)) in [("Settings", settings_place(&h)), ("Notes", notes_place(&h))] {
            let ok = match anchor {
                TOP => near(above, 0.0) && below > 100.0,
                BOTTOM => near(below, 0.0) && above > 100.0,
                _ => near(above, below) && above > 50.0,
            };
            assert!(ok, "{name}, {sidebar}: {above} above, {below} below the panel");
        }
    }
}

#[test]
fn the_anchor_is_disabled_while_the_height_is_match_note() {
    let mut h = both_sidebars();
    h.settings_appearance_click(ANCHOR[0]);
    assert_eq!(h.settings_appearance_open_list(), -1, "Match note: the Anchor list does not open");
    let before = settings_place(&h);

    pick(&mut h, HEIGHT[0], FIT_CONTENT);
    h.settings_appearance_click(ANCHOR[0]);
    assert_eq!(h.settings_appearance_open_list(), 2, "Fit content: it opens");
    h.settings_appearance_click(ANCHOR[0] + TOP);

    // Back to Match note: the Top anchor no longer moves the panel.
    pick(&mut h, HEIGHT[0], MATCH_NOTE);
    assert_eq!(settings_place(&h), before);
}

#[test]
fn turning_same_for_both_off_lets_each_sidebar_follow_its_own_values_from_the_shared_ones() {
    let mut h = both_sidebars();
    pick(&mut h, HEIGHT[0], FIT_CONTENT);
    pick(&mut h, ANCHOR[0], TOP);

    h.settings_appearance_click(SAME_SWITCH);
    // Nothing moved: both start from the shared values.
    let (_, s_above, _) = settings_place(&h);
    let (_, n_above, n_below) = notes_place(&h);
    assert!(near(s_above, 0.0) && near(n_above, 0.0) && n_below > 50.0, "both still Fit content at the Top");

    // The Notes sidebar's own controls change only it.
    pick(&mut h, ANCHOR[1], BOTTOM);
    let (_, n_above, n_below) = notes_place(&h);
    assert!(near(n_below, 0.0) && n_above > 50.0, "Notes at the Bottom: {n_above} above, {n_below} below");
    assert!(near(settings_place(&h).1, 0.0), "Settings still at the Top");
    pick(&mut h, HEIGHT[1], MATCH_NOTE);
    let (_, n_above, n_below) = notes_place(&h);
    assert!(near(n_above, 0.0) && near(n_below, 0.0), "Notes as tall as its Paper");
    let (s_h, ..) = settings_place(&h);
    assert!(near(s_h, SETTINGS_FIT_H), "Settings still Fit content: {s_h}");

    // The Settings sidebar's controls change only it.
    pick(&mut h, ANCHOR[0], BOTTOM);
    assert!(near(settings_place(&h).2, 0.0), "Settings at the Bottom");
    let (_, n_above, n_below) = notes_place(&h);
    assert!(near(n_above, 0.0) && near(n_below, 0.0), "Notes still Match note");
}

#[test]
fn turning_same_for_both_back_on_gives_both_sidebars_the_settings_sidebars_values() {
    let mut h = both_sidebars();
    h.settings_appearance_click(SAME_SWITCH);
    pick(&mut h, HEIGHT[0], FIT_CONTENT);
    pick(&mut h, ANCHOR[0], BOTTOM);
    // The Notes sidebar stays Match note.
    let (_, n_above, _) = notes_place(&h);
    assert!(near(n_above, 0.0));

    h.settings_appearance_click(SAME_SWITCH);
    let (n_h, n_above, n_below) = notes_place(&h);
    assert!(near(n_h, notes_fit_h(2)) && near(n_below, 0.0) && n_above > 50.0, "Notes took Fit content, Bottom");

    // One value drives both again.
    pick(&mut h, ANCHOR[0], TOP);
    assert!(near(settings_place(&h).1, 0.0) && near(notes_place(&h).1, 0.0), "both at the Top");
}

#[test]
fn the_sidebar_layout_survives_a_restart() {
    let mut h = both_sidebars();
    h.settings_appearance_click(SAME_SWITCH);
    pick(&mut h, HEIGHT[0], FIT_CONTENT);
    pick(&mut h, ANCHOR[0], BOTTOM);
    pick(&mut h, HEIGHT[1], FIT_CONTENT);
    pick(&mut h, ANCHOR[1], TOP);
    close_settings(&mut h);
    h.press_all_notes_escape();
    h.advance_frames(ms(600));
    assert!(!h.is_all_notes_open());

    h.restart().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    h.advance_frames(ms(600));
    let (n_h, n_above, n_below) = notes_place(&h);
    assert!(near(n_above, 0.0) && n_below > 50.0 && n_h < 400.0, "Notes: Fit content at the Top ({n_h}, {n_above}, {n_below})");

    h.toggle_dropdown(1);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    let (s_h, s_above, s_below) = settings_place(&h);
    assert!(near(s_h, SETTINGS_FIT_H) && near(s_below, 0.0) && s_above > 50.0, "Settings: Fit content at the Bottom");
}
