//! Settings › Appearance › Settings height: the Paper grows with its Tab up to a cap (the tallest
//! built-in Tab, or the screen), and past the cap the Tab's content scrolls inside the Paper.

use snap::{tab, Harness};
use std::time::Duration;
use washi::platform::ScreenRect;
use washi::storage::Geometry;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// The tallest built-in Tab's Paper (General, since it gained its rows: about 798), in whole px as the
/// window has it.
const TALLEST: u32 = 798;
/// Shortcuts' Paper, no longer the tallest, in whole px as the window has it.
const SHORTCUTS_H: u32 = 768;

// Appearance control ids, as `hit-x` in appearance-tab.slint defines them.
const SETTINGS_HEIGHT_BUTTON: i32 = 21;
const TALLEST_TAB: i32 = 22;
const SCREEN: i32 = 23;
/// The Settings height list's number in open-list.
const SETTINGS_HEIGHT_LIST: i32 = 5;
/// The Notes sidebar's Theme dropdown (while the Theme is split), its list, and its Paper option.
const NOTES_THEME_BUTTON: i32 = 46;
const NOTES_THEME_LIST: i32 = 11;
const NOTES_THEME_PAPER: i32 = 49;

fn started() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    h
}

fn open_on(h: &mut Harness, t: i32) {
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    if t != tab::GENERAL {
        h.settings_click_tab(t);
        h.advance_frames(ms(600));
    }
}

fn switch_to(h: &mut Harness, t: i32) {
    h.settings_click_tab(t);
    h.advance_frames(ms(600));
}

fn choose_height(h: &mut Harness, option: i32) {
    h.settings_appearance_click(SETTINGS_HEIGHT_BUTTON);
    assert_eq!(h.settings_appearance_open_list(), SETTINGS_HEIGHT_LIST, "the Settings height list is open");
    h.settings_appearance_click(option);
    h.advance_frames(ms(600));
}

#[test]
fn settings_height_is_tallest_tab_until_chosen_and_says_the_screen_limits_it() {
    let mut h = started();
    open_on(&mut h, tab::APPEARANCE);
    assert_eq!(h.settings_height_mode(), 0);
    let (label, hint) = h.settings_height_label();
    assert_eq!(label, "Tallest tab");
    assert!(hint.to_lowercase().contains("screen"), "the hint says the screen limits it: {hint:?}");
}

#[test]
fn with_tallest_tab_no_tabs_paper_is_taller_than_the_tallest_built_in_tab() {
    let mut h = started();
    open_on(&mut h, tab::SHORTCUTS);
    assert_eq!(h.settings_paper_height(), SHORTCUTS_H, "Shortcuts at its own height");
    switch_to(&mut h, tab::APPEARANCE);
    // each Sidebar its own rows: Appearance is the one Tab that grows past the others
    h.settings_split_appearance();
    h.advance_frames(ms(600));
    let natural = h.settings_paper_height_for(tab::APPEARANCE);
    assert!(natural > TALLEST as f32 + 50.0, "{natural}");
    assert_eq!(h.settings_paper_height(), TALLEST, "a taller Tab stops at the tallest built-in one");
    let (_, range) = h.settings_scroll();
    // (scrolled to the end, an open list must also end inside the Paper, so the range may be more)
    assert!(range >= natural - TALLEST as f32 - 1.5, "the rest scrolls: {range} of {natural}");
    for t in tab::ALL {
        switch_to(&mut h, t);
        assert!(h.settings_paper_height() <= TALLEST, "tab {t}: {}", h.settings_paper_height());
        assert!(h.settings_paper_height() >= 600, "tab {t}");
    }
}

#[test]
fn a_tab_that_fits_does_not_scroll() {
    let mut h = started();
    open_on(&mut h, tab::SHORTCUTS);
    assert_eq!(h.settings_scroll(), (0.0, 0.0));
    h.settings_wheel(-200.0);
    assert_eq!(h.settings_scroll(), (0.0, 0.0));
}

#[test]
fn with_screen_limit_a_taller_tab_grows_up_to_the_work_area() {
    let mut h = started();
    // a tall desk: Appearance with both groups split fits on it
    let desk = 1200;
    h.set_work_areas(vec![ScreenRect::new(0, 0, 1920, desk)]);
    open_on(&mut h, tab::APPEARANCE);
    choose_height(&mut h, SCREEN);
    assert_eq!(h.settings_height_mode(), 1);
    assert_eq!(h.settings_height_label().0, "Screen limit");

    // past the tallest Tab, inside the screen
    h.settings_split_appearance();
    h.advance_frames(ms(600));
    let natural = h.settings_paper_height_for(tab::APPEARANCE).round() as u32;
    assert!(natural > TALLEST && natural < desk as u32 - 84, "{natural}");
    assert_eq!(h.settings_paper_height(), natural, "grows past the tallest Tab, nothing to scroll");
    assert_eq!(h.settings_scroll().1, 0.0);

    // a screen too short for it: a limit between the tallest Tab and the Tab's own height
    let limit = TALLEST + (natural - TALLEST) / 2;
    h.set_work_areas(vec![ScreenRect::new(0, 0, 1920, (limit + 84) as i32)]);
    switch_to(&mut h, tab::GENERAL);
    switch_to(&mut h, tab::APPEARANCE);
    assert_eq!(h.settings_paper_height(), limit, "stops at the work area");
    assert!(h.settings_scroll().1 > 0.0);

    choose_height(&mut h, TALLEST_TAB);
    assert_eq!(h.settings_paper_height(), TALLEST, "the change applies at once");
}

#[test]
fn a_small_work_area_stops_both_options_and_the_content_scrolls() {
    let mut h = started();
    h.set_work_areas(vec![ScreenRect::new(0, 0, 1920, 720)]);
    open_on(&mut h, tab::SHORTCUTS);
    assert_eq!(h.settings_paper_height(), 720 - 84, "Shortcuts stops at the work area");
    assert!(h.settings_scroll().1 > 100.0, "{:?}", h.settings_scroll());

    switch_to(&mut h, tab::APPEARANCE);
    choose_height(&mut h, SCREEN);
    switch_to(&mut h, tab::SHORTCUTS);
    assert_eq!(h.settings_paper_height(), 720 - 84, "Screen limit stops there too");

    h.set_work_areas(vec![ScreenRect::new(0, 0, 1920, 500)]);
    switch_to(&mut h, tab::GENERAL);
    assert_eq!(h.settings_paper_height(), 600, "never below the 600 minimum");
}

#[test]
fn the_wheel_scrolls_between_both_ends_and_a_tab_switch_goes_back_to_the_top() {
    let mut h = started();
    h.set_work_areas(vec![ScreenRect::new(0, 0, 1920, 720)]);
    open_on(&mut h, tab::SHORTCUTS);
    let (_, range) = h.settings_scroll();
    h.settings_wheel(-60.0);
    assert_eq!(h.settings_scroll().0, 60.0);
    h.settings_wheel(20.0);
    assert_eq!(h.settings_scroll().0, 40.0);
    h.settings_wheel(1000.0);
    assert_eq!(h.settings_scroll().0, 0.0, "never above the top");
    h.settings_wheel(-100000.0);
    assert_eq!(h.settings_scroll().0, range, "never past the end");

    switch_to(&mut h, tab::GENERAL);
    assert_eq!(h.settings_scroll().0, 0.0, "a Tab switch starts at the top");
    h.settings_wheel(-50.0);
    switch_to(&mut h, tab::SHORTCUTS);
    assert_eq!(h.settings_scroll().0, 0.0);
}

#[test]
fn the_scrollbar_shows_only_while_scrolling_or_hovered_and_fades_out() {
    let mut h = started();
    h.set_work_areas(vec![ScreenRect::new(0, 0, 1920, 720)]);
    open_on(&mut h, tab::SHORTCUTS);
    h.advance(ms(300));
    assert_eq!(h.settings_scrollbar_opacity(), 0.0, "hidden at rest");
    let (mx, my) = h.settings_paper_middle();
    h.settings_pointer_to(mx, my);
    h.advance(ms(300));
    assert_eq!(h.settings_scrollbar_opacity(), 0.0, "hovering the Paper alone does not show it");

    h.settings_wheel(-60.0);
    h.advance(ms(300));
    assert_eq!(h.settings_scrollbar_opacity(), 1.0, "shown while scrolling");
    h.advance(ms(500));
    assert_eq!(h.settings_scrollbar_opacity(), 0.0, "fades once scrolling stops");

    let (tx, ty) = h.settings_scrollbar_thumb();
    h.settings_pointer_to(tx, ty);
    h.advance(ms(1000));
    assert_eq!(h.settings_scrollbar_opacity(), 1.0, "shown while hovered");
    h.settings_pointer_to(mx, my);
    h.advance(ms(300));
    assert_eq!(h.settings_scrollbar_opacity(), 0.0);

    // a Tab that fits never shows it
    let mut fits = started();
    open_on(&mut fits, tab::SHORTCUTS);
    let (tx, ty) = fits.settings_scrollbar_thumb();
    fits.settings_pointer_to(tx, ty);
    fits.advance(ms(300));
    assert_eq!(fits.settings_scrollbar_opacity(), 0.0);
}

#[test]
fn dragging_the_thumb_scrolls() {
    let mut h = started();
    h.set_work_areas(vec![ScreenRect::new(0, 0, 1920, 720)]);
    open_on(&mut h, tab::SHORTCUTS);
    let (_, range) = h.settings_scroll();
    let (tx, ty) = h.settings_scrollbar_thumb();
    h.settings_drag((tx, ty), (tx, ty + 40.0));
    let (mid, _) = h.settings_scroll();
    assert!(mid > 0.0 && mid < range, "part way: {mid} of {range}");
    let (tx, ty) = h.settings_scrollbar_thumb();
    h.settings_drag((tx, ty), (tx, ty + 2000.0));
    assert_eq!(h.settings_scroll().0, range, "to the end");
}

#[test]
fn clicking_a_row_while_scrolled_hits_that_rows_control() {
    let mut h = started();
    h.set_work_areas(vec![ScreenRect::new(0, 0, 1920, 720)]);
    open_on(&mut h, tab::APPEARANCE);
    h.settings_split_appearance();
    h.advance_frames(ms(600));
    // from the top
    h.settings_wheel(100000.0);
    h.settings_wheel(-80.0);
    assert_eq!(h.settings_scroll().0, 80.0);

    // 80px up: the Notes sidebar's Theme row moved up by 80, more than half its button's height,
    // so an unscrolled click would miss it
    h.settings_appearance_click(NOTES_THEME_BUTTON);
    assert_eq!(h.settings_appearance_open_list(), NOTES_THEME_LIST, "the Notes sidebar's Theme list opened");
    assert_eq!(h.settings_scroll().0, 80.0, "the button was in view: the click did not scroll");
    h.settings_appearance_click(NOTES_THEME_PAPER);
    assert_eq!(h.settings_theme_rows()[1].1, "Paper");
}

#[test]
fn the_sidebar_does_not_scroll() {
    let mut h = started();
    h.set_work_areas(vec![ScreenRect::new(0, 0, 1920, 720)]);
    open_on(&mut h, tab::SHORTCUTS);
    // the wheel over the Sidebar changes nothing, and its Tabs stay where they were
    let position_before = h.settings_scroll();
    h.settings_pointer_to(100.0, 300.0);
    h.settings_wheel_at(100.0, 300.0, -200.0);
    assert_eq!(h.settings_scroll(), position_before);
    h.settings_wheel(-200.0);
    switch_to(&mut h, tab::PRIVACY);
    assert_eq!(h.settings_shown_tab(), tab::PRIVACY, "the Sidebar's Tabs are still where they were");
}

#[test]
fn the_choice_survives_a_restart() {
    let mut h = started();
    open_on(&mut h, tab::APPEARANCE);
    choose_height(&mut h, SCREEN);
    h.settings_close();
    h.advance_frames(ms(600));
    h.restart().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    open_on(&mut h, tab::APPEARANCE);
    assert_eq!(h.settings_height_mode(), 1);
    assert_eq!(h.settings_height_label().0, "Screen limit");
}
