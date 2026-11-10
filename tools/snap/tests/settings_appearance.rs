//! Settings › Appearance: the Tab's place in the Sidebar and the UI size choice (saved, and the size
//! new Notes open at).

use snap::{tab, Harness};
use std::time::Duration;
use washi::sidebar_panel;
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
    let first = h.note_screen_size(0);
    assert_eq!(first, new_note_window(1.0));
    for (option, scale) in [(SMALL, 0.75), (MEDIUM, 0.80), (LARGE, 1.0)] {
        choose_ui_size(&mut h, option);
        let n = new_note(&mut h);
        assert_eq!(h.note_screen_size(n), new_note_window(scale), "UI size {option}");
        assert_eq!(h.note_screen_size(0), first, "an open note keeps its size");
    }
}

#[test]
fn a_new_note_ignores_the_size_of_the_note_it_is_opened_from() {
    let mut h = started();
    h.resize_note(0, 600, 560);
    choose_ui_size(&mut h, SMALL);
    let n = new_note(&mut h);
    assert_eq!(h.note_screen_size(n), new_note_window(0.75));
}

#[test]
fn the_fresh_note_after_a_restart_with_nothing_open_uses_the_ui_size() {
    let mut h = started();
    choose_ui_size(&mut h, SMALL);
    h.press_ctrl(0, 'w');
    assert!(h.has_quit());
    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.note_screen_size(0), new_note_window(0.75));
}

// ---- UI size rescales what Washi draws (ticket 03) ----

/// Where the Paper starts on the middle row of a rendered note: the first bright pixel from the
/// left (the margin around the Paper is transparent, rendered dark).
fn paper_left_edge(h: &Harness, note: usize) -> u32 {
    let (w, hh, px) = h.render(note);
    let row = hh / 2;
    (0..w).find(|&x| {
        let p = px[(row * w + x) as usize];
        (p.r as u32 + p.g as u32 + p.b as u32) / 3 > 150
    })
    .expect("no Paper on the middle row")
}

fn close_to(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance
}

/// How far a panel or Paper edge measured on a render may be from where it should be (px).
const EDGE: f32 = 2.0;

#[test]
fn small_and_medium_draw_an_open_note_smaller_inside_the_same_window() {
    let mut h = started();
    h.type_text(0, "hello");
    let window = h.note_screen_size(0);
    let edge = paper_left_edge(&h, 0) as f32;
    let words = h.text_runs(0);
    assert_eq!(h.note_drawing_scale(0), 1.0, "Large is today's size");
    for (option, scale) in [(SMALL, 0.75), (MEDIUM, 0.80)] {
        choose_ui_size(&mut h, option);
        assert_eq!(h.note_screen_size(0), window, "UI size {option}: the window keeps its size");
        assert_eq!(h.note_drawing_scale(0), scale, "UI size {option}: drawn at {scale}");
        let (lw, lh) = h.note_size(0);
        assert!(
            close_to(lw as f32 * scale, window.0 as f32, 1.0) && close_to(lh as f32 * scale, window.1 as f32, 1.0),
            "UI size {option}: the content reflows into the bigger logical window, got {lw}x{lh}"
        );
        let small_edge = paper_left_edge(&h, 0) as f32;
        assert!(close_to(small_edge, edge * scale, 2.0), "UI size {option}: margin {small_edge}, want {}", edge * scale);
        let now = h.text_runs(0);
        assert_eq!(now.first().map(|r| r.0.clone()), words.first().map(|r| r.0.clone()));
        // the per-note text size keeps its logical size: the scale comes on top of it
        assert_eq!(now.first().map(|r| r.5), words.first().map(|r| r.5), "UI size {option}: word width");
    }
}

#[test]
fn going_back_to_large_draws_the_note_exactly_as_before() {
    let mut h = started();
    h.type_text(0, "hello world");
    // the Settings round trip itself, at Large
    choose_ui_size(&mut h, LARGE);
    // (an inactive window draws no blinking caret)
    h.set_window_active(0, false);
    let before = h.render(0);
    choose_ui_size(&mut h, SMALL);
    choose_ui_size(&mut h, LARGE);
    assert_eq!(h.note_drawing_scale(0), 1.0);
    h.set_window_active(0, false);
    let after = h.render(0);
    assert_eq!((after.0, after.1), (before.0, before.1));
    let diff: Vec<usize> = (0..after.2.len()).filter(|&i| after.2[i] != before.2[i]).collect();
    let xy = |i: &usize| (*i as u32 % after.0, *i as u32 / after.0);
    assert!(diff.is_empty(), "Large is pixel-identical to before: {} px differ, first {:?} last {:?}", diff.len(), diff.first().map(xy), diff.last().map(xy));
}

#[test]
fn the_settings_page_shrinks_at_once_and_stays_clickable() {
    let mut h = started();
    open_on(&mut h, tab::APPEARANCE);
    let (w, hh) = h.settings_screen_size().unwrap();
    h.settings_appearance_click(UI_SIZE_BUTTON);
    h.settings_appearance_click(SMALL);
    h.advance_frames(ms(600));
    let (sw, sh) = h.settings_screen_size().unwrap();
    assert!(close_to(sw as f32, w as f32 * 0.75, 2.0) && close_to(sh as f32, hh as f32 * 0.75, 2.0),
        "Settings at Small: {sw}x{sh}, want 75% of {w}x{hh}");

    // the scaled page still answers where it draws its controls
    h.settings_appearance_click(UI_SIZE_BUTTON);
    assert_eq!(h.settings_appearance_open_list(), 0);
    h.settings_appearance_click(LARGE);
    h.advance_frames(ms(600));
    assert_eq!(h.settings_ui_size(), 2);
    assert_eq!(h.settings_screen_size().unwrap(), (w, hh), "back at Large: today's size");
    h.settings_click_tab(tab::GENERAL);
    h.advance_frames(ms(600));
    assert_eq!(h.settings_shown_tab(), tab::GENERAL);
}

#[test]
fn closing_settings_puts_the_note_back_at_its_size_after_a_scale_change() {
    let mut h = started();
    let (window, at) = (h.note_screen_size(0), h.window_position(0));
    choose_ui_size(&mut h, SMALL);
    assert_eq!(h.note_screen_size(0), window);
    assert_eq!(h.window_position(0), at);
}

#[test]
fn the_dropdown_menu_scales() {
    let mut h = started();
    h.toggle_dropdown(0);
    let (w, hh) = h.dropdown_screen_size().unwrap();
    h.toggle_dropdown(0);
    choose_ui_size(&mut h, SMALL);
    h.toggle_dropdown(0);
    let (sw, sh) = h.dropdown_screen_size().unwrap();
    assert!(close_to(sw as f32, w as f32 * 0.75, 1.0) && close_to(sh as f32, hh as f32 * 0.75, 1.0),
        "dropdown at Small: {sw}x{sh}, want 75% of {w}x{hh}");
}

#[test]
fn the_dropdown_menu_position_is_aligned_when_scaled() {
    let mut h = started();
    choose_ui_size(&mut h, MEDIUM);
    h.drag(0, (80.0, 48.0), (300.0, 200.0));
    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());

    let (win_x, win_y) = h.dropdown_position().expect("dropdown position");
    let (bx, by, _bw, bh) = h.menu_button_rect(0);
    let note_geom = h.note_geometry(0);
    let btn_screen_x = note_geom.x as f32 + bx;
    let btn_screen_y = note_geom.y as f32 + by;

    let metrics = washi::dropdown::DropdownMetrics::default();
    let card_x = win_x + metrics.shadow_margin;
    let card_y = win_y + metrics.shadow_margin;

    assert!((card_x - (btn_screen_x - 14.0)).abs() <= 2.0, "card_x ({card_x}) must align with button ({btn_screen_x} - 14.0)");
    assert!((card_y - (btn_screen_y + bh + metrics.gap)).abs() <= 2.0, "card_y ({card_y}) must align below button");
}

#[test]
fn the_dropdown_menu_position_is_aligned_on_secondary_monitor_when_scaled() {
    use washi::platform::ScreenRect;
    let mut h = Harness::new();
    h.set_work_areas(vec![
        ScreenRect::new(0, 0, 1920, 1080),       // Top-left (primary)
        ScreenRect::new(1920, 0, 1920, 1080),    // Top-right
        ScreenRect::new(0, 1080, 1920, 1080),    // Bottom-left
        ScreenRect::new(1920, 1080, 1920, 1080), // Bottom-right
    ]);
    h.start().unwrap();

    choose_ui_size(&mut h, MEDIUM);
    // Move note to bottom-right monitor
    h.drag(0, (80.0, 48.0), (2400.0, 1400.0));
    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());

    let (win_x, win_y) = h.dropdown_position().expect("dropdown position");
    let (bx, by, _bw, bh) = h.menu_button_rect(0);
    let note_geom = h.note_geometry(0);
    let btn_screen_x = note_geom.x as f32 + bx;
    let btn_screen_y = note_geom.y as f32 + by;

    let metrics = washi::dropdown::DropdownMetrics::default();
    let menu_x = win_x + metrics.shadow_margin;
    let menu_y = win_y + metrics.shadow_margin;

    assert!(menu_x >= 1920.0, "menu_x ({menu_x}) must be on the secondary monitor (>= 1920)");
    assert!(menu_y >= 1080.0, "menu_y ({menu_y}) must be on the bottom monitor (>= 1080)");
    assert!((menu_x - (btn_screen_x - 14.0)).abs() <= 2.0, "menu_x ({menu_x}) must align with button ({btn_screen_x} - 14.0)");
    assert!((menu_y - (btn_screen_y + bh + metrics.gap)).abs() <= 2.0, "menu_y ({menu_y}) must align below button");
}

#[test]
fn the_delete_dialog_position_is_centered_when_scaled() {
    for size in [MEDIUM, SMALL] {
        let mut h = started();
        choose_ui_size(&mut h, size);
        h.drag(0, (80.0, 48.0), (300.0, 200.0));
        h.press_ctrl(0, 'd');
        assert!(h.is_delete_dialog_open());

        let (dx, dy) = h.delete_dialog_position().expect("delete dialog position");
        let geom = h.note_geometry(0);
        let expected_center_x = geom.x as f32 + (geom.w as f32 - washi::delete_dialog::DIALOG_WIDTH) / 2.0;
        let expected_center_y = geom.y as f32 + (geom.h as f32 - washi::delete_dialog::DIALOG_HEIGHT) / 2.0;

        assert!((dx - expected_center_x).abs() <= 2.0, "dialog dx ({dx}) must center over note ({expected_center_x}) at size {size:?}");
        assert!((dy - expected_center_y).abs() <= 2.0, "dialog dy ({dy}) must center over note ({expected_center_y}) at size {size:?}");
    }
}


#[test]
fn the_all_notes_strip_scales() {
    let mut h = started();
    choose_ui_size(&mut h, SMALL);
    let note = h.note_screen_size(0);
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    h.advance_frames(ms(600));
    let (_, _, w, hh) = h.all_notes_window_geometry().unwrap();
    assert_eq!(hh, note.1);
    assert!(close_to((w - note.0) as f32, sidebar_panel::STRIP * 0.75, 1.0), "strip {} on screen", w - note.0);
}

#[test]
fn closing_all_notes_after_ui_scale_change_keeps_host_note_position_and_size() {
    let mut h = started();
    let initial_pos = h.window_position(0);
    let initial_size = h.note_screen_size(0);

    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    h.advance_frames(ms(600));
    assert!(h.is_all_notes_open());

    choose_ui_size(&mut h, SMALL);

    h.press_all_notes_escape();
    h.advance_frames(ms(600));
    assert!(!h.is_all_notes_open());

    let final_pos = h.window_position(0);
    let final_size = h.note_screen_size(0);

    assert!(
        (final_pos.0 - initial_pos.0).abs() <= 1 && (final_pos.1 - initial_pos.1).abs() <= 1,
        "host note must return to same screen position after scale change and all-notes close (got {final_pos:?}, want {initial_pos:?})"
    );
    assert!(
        (final_size.0 as i32 - initial_size.0 as i32).abs() <= 1 && (final_size.1 as i32 - initial_size.1 as i32).abs() <= 1,
        "host note must retain its on-screen size after scale change and all-notes close (got {final_size:?}, want {initial_size:?})"
    );
}


#[test]
fn the_screen_still_caps_the_settings_page_at_small_and_more_of_a_tab_fits() {
    let mut h = started();
    h.set_work_areas(vec![washi::platform::ScreenRect::new(0, 0, 1920, 700)]);
    open_on(&mut h, tab::SHORTCUTS);
    let (_, large_on_screen) = h.settings_screen_size().unwrap();
    let large_paper = h.settings_paper_height();
    assert!(large_on_screen <= 700, "Large: {large_on_screen} px tall on a 700 px screen");
    assert!(h.settings_scroll().1 > 0.0, "Large: Shortcuts is capped by the screen and scrolls");

    h.settings_click_tab(tab::APPEARANCE);
    h.advance_frames(ms(600));
    h.settings_appearance_click(UI_SIZE_BUTTON);
    h.settings_appearance_click(SMALL);
    h.settings_click_tab(tab::SHORTCUTS);
    h.advance_frames(ms(600));
    let (_, small_on_screen) = h.settings_screen_size().unwrap();
    assert!(small_on_screen <= 700, "Small: {small_on_screen} px tall on a 700 px screen");
    assert!(h.settings_paper_height() > large_paper, "Small: the same screen holds more of the Tab");
    assert!(small_on_screen < large_on_screen, "Small: the page is shorter on screen");
}

#[test]
fn the_saved_ui_size_is_applied_at_startup() {
    let mut h = started();
    choose_ui_size(&mut h, MEDIUM);
    let (window, at) = (h.note_screen_size(0), h.window_position(0));
    h.restart().unwrap();
    assert_eq!(h.note_drawing_scale(0), 0.80);
    assert_eq!(h.note_screen_size(0), window, "the note comes back at the size it had on screen");
    assert_eq!(h.window_position(0), at);
}

#[test]
fn the_rust_copies_of_the_sidebar_panel_match_the_slint_one() {
    let h = started();
    assert_eq!(h.slint_sidebar_panel(), (sidebar_panel::STRIP, sidebar_panel::PANEL_W, sidebar_panel::PANEL_CX));
}

// ---- UI size on a monitor with another DPI: the monitor's scale times the UI size ----

#[test]
fn a_note_keeps_its_ui_size_after_moving_to_a_monitor_with_another_dpi() {
    let mut h = started();
    choose_ui_size(&mut h, SMALL);
    let (w, hh) = h.note_screen_size(0);
    h.move_note_to_monitor(0, 1.5);
    assert!(close_to(h.note_drawing_scale(0), 1.5 * 0.75, 1e-4), "drawn at {}", h.note_drawing_scale(0));
    assert!(close_to(h.note_pitch_scale(0), 1.5 * 0.75, 1e-4), "pitch scale at {}", h.note_pitch_scale(0));
    let (mw, mh) = h.note_screen_size(0);
    assert!(close_to(mw as f32, w as f32 * 1.5, 2.0) && close_to(mh as f32, hh as f32 * 1.5, 2.0), "{mw}x{mh}");
}

#[test]
fn a_note_at_large_updates_pitch_scale_after_moving_to_a_monitor_with_another_dpi() {
    let mut h = started();
    assert_eq!(h.note_drawing_scale(0), 1.0);
    assert_eq!(h.note_pitch_scale(0), 1.0);
    h.move_note_to_monitor(0, 1.5);
    assert!(close_to(h.note_drawing_scale(0), 1.5, 1e-4), "drawn at {}", h.note_drawing_scale(0));
    assert!(close_to(h.note_pitch_scale(0), 1.5, 1e-4), "pitch scale at {}", h.note_pitch_scale(0));
}

#[test]
fn settings_keeps_its_ui_size_after_moving_to_a_monitor_with_another_dpi() {
    let mut h = started();
    choose_ui_size(&mut h, SMALL);
    open_on(&mut h, tab::APPEARANCE);
    assert_eq!(h.settings_drawing_scale(), Some(0.75));
    let (w, hh) = h.settings_screen_size().unwrap();
    h.move_settings_to_monitor(1.5);
    let scale = h.settings_drawing_scale().unwrap();
    assert!(close_to(scale, 1.5 * 0.75, 1e-4), "drawn at {scale}, want the monitor's 1.5 times Small");
    let (mw, mh) = h.settings_screen_size().unwrap();
    assert!(close_to(mw as f32, w as f32 * 1.5, 2.0) && close_to(mh as f32, hh as f32 * 1.5, 2.0),
        "the same page at the new DPI: {mw}x{mh}, want 1.5 times {w}x{hh}");

    // the page still answers where it draws its controls, and stays at the new scale
    h.settings_appearance_click(UI_SIZE_BUTTON);
    assert_eq!(h.settings_appearance_open_list(), 0);
    h.settings_press_escape();
    h.settings_click_tab(tab::GENERAL);
    h.advance_frames(ms(600));
    assert_eq!(h.settings_shown_tab(), tab::GENERAL);
    let after = h.settings_drawing_scale().unwrap();
    assert!(close_to(after, 1.5 * 0.75, 1e-4), "after a Tab switch: {after}");
}

#[test]
fn the_delete_dialog_keeps_its_ui_size_after_moving_to_a_monitor_with_another_dpi() {
    let mut h = started();
    choose_ui_size(&mut h, SMALL);
    h.press_ctrl(0, 'd');
    let ((w, hh), scale) = h.delete_dialog_screen().unwrap();
    assert_eq!(scale, 0.75);
    h.move_delete_dialog_to_monitor(1.5);
    let ((mw, mh), scale) = h.delete_dialog_screen().unwrap();
    assert!(close_to(scale, 1.5 * 0.75, 1e-4), "drawn at {scale}");
    assert!(close_to(mw as f32, w as f32 * 1.5, 2.0) && close_to(mh as f32, hh as f32 * 1.5, 2.0), "{mw}x{mh}");
}

#[test]
fn the_picture_viewer_opens_at_the_ui_size() {
    let red = [200u8, 30, 60];
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("red.png");
    let picture = washi::images::Rgba { width: 300, height: 150, data: (0..300 * 150).flat_map(|_| [red[0], red[1], red[2], 255]).collect() };
    std::fs::write(&path, washi::images::encode_png(&picture).unwrap()).unwrap();
    let mut h = started();
    choose_ui_size(&mut h, SMALL);
    h.choose_picture(&path);
    h.click_image_button(0);
    h.wait_for_images();

    // double click the middle of the picture (found on the render, screen px)
    let (w, _, px) = h.render(0);
    let spots: Vec<(u32, u32)> = px.iter().enumerate()
        .filter(|(_, p)| (p.r as i32 - 200).abs() <= 12 && (p.g as i32 - 30).abs() <= 12 && (p.b as i32 - 60).abs() <= 12)
        .map(|(i, _)| (i as u32 % w, i as u32 / w))
        .collect();
    assert!(!spots.is_empty(), "the picture is drawn");
    let n = spots.len() as f32;
    let scale = h.note_drawing_scale(0);
    let x = spots.iter().map(|s| s.0 as f32).sum::<f32>() / n / scale;
    let y = spots.iter().map(|s| s.1 as f32).sum::<f32>() / n / scale;
    h.click(0, x, y);
    h.click(0, x, y);
    assert_eq!(h.viewer_drawing_scales(), [0.75], "the viewer is drawn at Small too");
}

#[test]
fn an_open_picture_viewer_follows_the_ui_size_live() {
    let red = [200u8, 30, 60];
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("red.png");
    let picture = washi::images::Rgba { width: 300, height: 150, data: (0..300 * 150).flat_map(|_| [red[0], red[1], red[2], 255]).collect() };
    std::fs::write(&path, washi::images::encode_png(&picture).unwrap()).unwrap();
    let mut h = started();
    h.choose_picture(&path);
    h.click_image_button(0);
    h.wait_for_images();

    let (w, _, px) = h.render(0);
    let spots: Vec<(u32, u32)> = px.iter().enumerate()
        .filter(|(_, p)| (p.r as i32 - 200).abs() <= 12 && (p.g as i32 - 30).abs() <= 12 && (p.b as i32 - 60).abs() <= 12)
        .map(|(i, _)| (i as u32 % w, i as u32 / w))
        .collect();
    assert!(!spots.is_empty(), "the picture is drawn");
    let n = spots.len() as f32;
    let scale = h.note_drawing_scale(0);
    let x = spots.iter().map(|s| s.0 as f32).sum::<f32>() / n / scale;
    let y = spots.iter().map(|s| s.1 as f32).sum::<f32>() / n / scale;
    h.click(0, x, y);
    h.click(0, x, y);
    assert_eq!(h.viewer_drawing_scales(), [1.0], "the viewer opens at Large");

    choose_ui_size(&mut h, SMALL);
    assert_eq!(h.viewer_drawing_scales(), [0.75], "the viewer updates to Small live");
}

// ---- Sidebar: Height and Anchor ----

/// Whether two window-px positions agree to within the anti-aliased edge of a tilted panel.
#[test]
fn match_note_makes_the_settings_sidebar_as_tall_as_its_paper() {
    let mut h = started();
    // Shortcuts is taller than the Paper's 600 minimum.
    open_on(&mut h, tab::SHORTCUTS);
    let (top, bottom) = h.settings_sidebar_span();
    let (paper_top, paper_bottom) = h.settings_paper_span();
    assert!(paper_bottom - paper_top > 700.0, "the Paper is {paper_top}..{paper_bottom}");
    assert!(close_to(top, paper_top, EDGE) && close_to(bottom, paper_bottom, EDGE), "panel {top}..{bottom}, Paper {paper_top}..{paper_bottom}");
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
        assert!(close_to(above, 0.0, EDGE) && close_to(below, 0.0, EDGE), "{name}: {above} above and {below} below the panel");
    }
}

#[test]
fn fit_content_makes_the_settings_sidebar_header_and_tabs_tall_and_centres_it() {
    let mut h = both_sidebars();
    pick(&mut h, HEIGHT[0], FIT_CONTENT);
    let (height, above, below) = settings_place(&h);
    assert!(close_to(height, SETTINGS_FIT_H, EDGE), "panel {height} tall");
    assert!(close_to(above, below, EDGE), "Middle: {above} above, {below} below");
}

#[test]
fn fit_content_makes_the_all_notes_sidebar_grow_with_its_rows_up_to_its_paper_then_scroll() {
    let mut h = both_sidebars();
    pick(&mut h, HEIGHT[0], FIT_CONTENT);
    // Two notes are listed.
    let (height, above, below) = notes_place(&h);
    assert!(close_to(height, notes_fit_h(2), EDGE), "panel {height} tall, wanted {}", notes_fit_h(2));
    assert!(close_to(above, below, EDGE), "Middle: {above} above, {below} below");

    // More rows than the Paper holds: the panel stops at the Paper and the list scrolls.
    for _ in 0..8 {
        h.invoke_all_notes_new_note();
    }
    let (height, above, below) = notes_place(&h);
    assert!(close_to(above, 0.0, EDGE) && close_to(below, 0.0, EDGE), "capped at the Paper: {height} tall, {above} above, {below} below");
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
                TOP => close_to(above, 0.0, EDGE) && below > 100.0,
                BOTTOM => close_to(below, 0.0, EDGE) && above > 100.0,
                _ => close_to(above, below, EDGE) && above > 50.0,
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
    assert!(close_to(s_above, 0.0, EDGE) && close_to(n_above, 0.0, EDGE) && n_below > 50.0, "both still Fit content at the Top");

    // The Notes sidebar's own controls change only it.
    pick(&mut h, ANCHOR[1], BOTTOM);
    let (_, n_above, n_below) = notes_place(&h);
    assert!(close_to(n_below, 0.0, EDGE) && n_above > 50.0, "Notes at the Bottom: {n_above} above, {n_below} below");
    assert!(close_to(settings_place(&h).1, 0.0, EDGE), "Settings still at the Top");
    pick(&mut h, HEIGHT[1], MATCH_NOTE);
    let (_, n_above, n_below) = notes_place(&h);
    assert!(close_to(n_above, 0.0, EDGE) && close_to(n_below, 0.0, EDGE), "Notes as tall as its Paper");
    let (s_h, ..) = settings_place(&h);
    assert!(close_to(s_h, SETTINGS_FIT_H, EDGE), "Settings still Fit content: {s_h}");

    // The Settings sidebar's controls change only it.
    pick(&mut h, ANCHOR[0], BOTTOM);
    assert!(close_to(settings_place(&h).2, 0.0, EDGE), "Settings at the Bottom");
    let (_, n_above, n_below) = notes_place(&h);
    assert!(close_to(n_above, 0.0, EDGE) && close_to(n_below, 0.0, EDGE), "Notes still Match note");
}

#[test]
fn turning_same_for_both_back_on_gives_both_sidebars_the_settings_sidebars_values() {
    let mut h = both_sidebars();
    h.settings_appearance_click(SAME_SWITCH);
    pick(&mut h, HEIGHT[0], FIT_CONTENT);
    pick(&mut h, ANCHOR[0], BOTTOM);
    // The Notes sidebar stays Match note.
    let (_, n_above, _) = notes_place(&h);
    assert!(close_to(n_above, 0.0, EDGE));

    h.settings_appearance_click(SAME_SWITCH);
    let (n_h, n_above, n_below) = notes_place(&h);
    assert!(close_to(n_h, notes_fit_h(2), EDGE) && close_to(n_below, 0.0, EDGE) && n_above > 50.0, "Notes took Fit content, Bottom");

    // One value drives both again.
    pick(&mut h, ANCHOR[0], TOP);
    assert!(close_to(settings_place(&h).1, 0.0, EDGE) && close_to(notes_place(&h).1, 0.0, EDGE), "both at the Top");
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
    assert!(close_to(n_above, 0.0, EDGE) && n_below > 50.0 && n_h < 400.0, "Notes: Fit content at the Top ({n_h}, {n_above}, {n_below})");

    h.toggle_dropdown(1);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    let (s_h, s_above, s_below) = settings_place(&h);
    assert!(close_to(s_h, SETTINGS_FIT_H, EDGE) && close_to(s_below, 0.0, EDGE) && s_above > 50.0, "Settings: Fit content at the Bottom");
}

// ---- Sidebar themes ----

// Theme control ids, as `hit-x` in appearance-tab.slint defines them.
const THEME_SWITCH: i32 = 40;
/// The first Theme dropdown (both Sidebars while shared, the Settings sidebar's while split) and the
/// Notes sidebar's; each one's options follow its id (Default first).
const THEME_A: i32 = 41;
const THEME_B: i32 = 46;
const DEFAULT: i32 = 0;
const TRANSPARENT: i32 = 1;
const PAPER: i32 = 2;
const GLASS: i32 = 3;
const THEME_NAMES: [&str; 4] = ["Default", "Transparent", "Paper", "Liquid glass"];

const SLATE: [u8; 3] = [0xD6, 0xDB, 0xE2];
const PAPER_FILL: [u8; 3] = [0xFF, 0xFF, 0xFD];
const DESK: [u8; 3] = snap::DESK;
/// The panel's centre column in the Sidebar's strip.
const PANEL_CX: u32 = sidebar_panel::PANEL_CX as u32;

fn near_rgb(p: [u8; 3], want: [u8; 3]) -> bool {
    p.iter().zip(want).all(|(a, b)| (*a as i32 - b as i32).abs() <= 1)
}

/// Pick `theme` from the Theme dropdown `dropdown` (THEME_A or THEME_B) on the open Appearance Tab.
fn pick_theme(h: &mut Harness, dropdown: i32, theme: i32) {
    h.settings_appearance_click(dropdown);
    h.settings_appearance_click(dropdown + 1 + theme);
}

/// A blank spot of the Settings Sidebar's panel (under its Tabs), as rendered.
fn settings_panel_spot(h: &Harness) -> [u8; 3] {
    let shot = h.render_settings();
    shot.pixel(PANEL_CX, shot.height / 2 + 200)
}

/// A blank spot of the All notes Sidebar's panel (right of the New-note button, under the rows).
fn all_notes_panel_spot(h: &Harness) -> [u8; 3] {
    let shot = h.render_all_notes_on_desk().expect("All notes is open");
    shot.pixel(220, shot.height / 2 + 150)
}

fn open_all_notes(h: &mut Harness, note: usize) {
    h.toggle_dropdown(note);
    h.invoke_dropdown_all_notes();
    h.advance_frames(ms(600));
    assert!(h.is_all_notes_open());
}

/// What a theme looks like at a blank panel spot.
fn looks_like(p: [u8; 3], theme: i32) -> bool {
    match theme {
        DEFAULT => near_rgb(p, SLATE),
        TRANSPARENT => near_rgb(p, DESK),
        PAPER => near_rgb(p, PAPER_FILL),
        // A translucent white over the desk: clearly lighter than it, not paper-white.
        _ => p.iter().zip(DESK).all(|(a, d)| *a as i32 >= d as i32 + 6) && !near_rgb(p, PAPER_FILL),
    }
}

/// The width of the Paper-themed All notes panel on its middle row, as rendered (screen px): the run
/// of paper-white pixels that starts in the Sidebar's strip.
fn paper_panel_width(h: &Harness) -> u32 {
    let shot = h.render_all_notes_on_desk().expect("All notes is open");
    let row = shot.height / 2 + 150;
    let white = |x: u32| near_rgb(shot.pixel(x, row), PAPER_FILL);
    let start = (0..shot.width).find(|&x| white(x)).expect("the panel is drawn");
    let end = (start..shot.width).find(|&x| !white(x)).unwrap_or(shot.width);
    end - start
}

#[test]
fn a_themed_sidebar_scales_with_the_ui_size() {
    let mut widths = Vec::new();
    for size in [LARGE, SMALL] {
        let mut h = started();
        open_on(&mut h, tab::APPEARANCE);
        pick_theme(&mut h, THEME_A, PAPER);
        h.settings_appearance_click(UI_SIZE_BUTTON);
        h.settings_appearance_click(size);
        close_settings(&mut h);
        open_all_notes(&mut h, 0);
        widths.push(paper_panel_width(&h) as f32);
    }
    assert!(close_to(widths[0], sidebar_panel::PANEL_W, 3.0), "Large: the {} px panel, got {}", sidebar_panel::PANEL_W, widths[0]);
    assert!(close_to(widths[1], widths[0] * 0.75, 3.0), "Small: {} on screen, want 75% of {}", widths[1], widths[0]);
}

#[test]
fn the_theme_is_default_and_shared_until_chosen() {
    let mut h = started();
    open_on(&mut h, tab::APPEARANCE);
    assert!(h.settings_theme_shared());
    assert_eq!(h.settings_theme_rows(), [("Both sidebars".to_string(), "Default".to_string())]);
    assert!(near_rgb(settings_panel_spot(&h), SLATE), "{:?}", settings_panel_spot(&h));
}

#[test]
fn each_theme_draws_the_settings_sidebar_at_once() {
    let mut h = started();
    open_on(&mut h, tab::APPEARANCE);
    for theme in [TRANSPARENT, PAPER, GLASS, DEFAULT] {
        pick_theme(&mut h, THEME_A, theme);
        assert_eq!(h.settings_appearance_open_list(), -1, "choosing closes the list");
        assert_eq!(h.settings_theme_rows()[0].1, THEME_NAMES[theme as usize]);
        let p = settings_panel_spot(&h);
        assert!(looks_like(p, theme), "{}: panel spot {p:?}", THEME_NAMES[theme as usize]);
    }
}

#[test]
fn liquid_glass_is_lighter_at_the_top_than_at_the_bottom() {
    let mut h = started();
    open_on(&mut h, tab::APPEARANCE);
    pick_theme(&mut h, THEME_A, GLASS);
    let shot = h.render_settings();
    let (top, bottom) = (shot.pixel(PANEL_CX, shot.height / 2 - 280), shot.pixel(PANEL_CX, shot.height / 2 + 280));
    let luma = |p: [u8; 3]| p.iter().map(|&c| c as i32).sum::<i32>();
    assert!(luma(top) > luma(bottom) + 6, "a soft vertical gradient: top {top:?}, bottom {bottom:?}");
}

#[test]
fn a_shared_theme_draws_the_all_notes_sidebar_too() {
    let mut h = started();
    open_on(&mut h, tab::APPEARANCE);
    pick_theme(&mut h, THEME_A, PAPER);
    close_settings(&mut h);
    open_all_notes(&mut h, 0);
    let p = all_notes_panel_spot(&h);
    assert!(near_rgb(p, PAPER_FILL), "{p:?}");
}

#[test]
fn an_open_all_notes_sidebar_follows_the_theme_live() {
    let mut h = started();
    new_note(&mut h);
    open_all_notes(&mut h, 1);
    assert!(near_rgb(all_notes_panel_spot(&h), SLATE));
    open_on(&mut h, tab::APPEARANCE);
    pick_theme(&mut h, THEME_A, TRANSPARENT);
    let p = all_notes_panel_spot(&h);
    assert!(near_rgb(p, DESK), "{p:?}");
}

#[test]
fn split_themes_start_from_the_shared_value_and_rejoin_on_the_settings_one() {
    let mut h = started();
    open_on(&mut h, tab::APPEARANCE);
    pick_theme(&mut h, THEME_A, PAPER);

    h.settings_appearance_click(THEME_SWITCH);
    assert!(!h.settings_theme_shared());
    assert_eq!(
        h.settings_theme_rows(),
        [("Settings sidebar".to_string(), "Paper".to_string()), ("Notes sidebar".to_string(), "Paper".to_string())]
    );

    pick_theme(&mut h, THEME_B, TRANSPARENT);
    assert_eq!(h.settings_theme_rows()[1].1, "Transparent");
    assert!(near_rgb(settings_panel_spot(&h), PAPER_FILL), "the Settings sidebar keeps its own");
    close_settings(&mut h);
    open_all_notes(&mut h, 0);
    assert!(near_rgb(all_notes_panel_spot(&h), DESK), "the Notes sidebar follows its own");
    h.press_all_notes_escape();
    h.advance_frames(ms(600));
    assert!(!h.is_all_notes_open());

    open_on(&mut h, tab::APPEARANCE);
    pick_theme(&mut h, THEME_A, GLASS);
    h.settings_appearance_click(THEME_SWITCH);
    assert!(h.settings_theme_shared());
    assert_eq!(h.settings_theme_rows(), [("Both sidebars".to_string(), "Liquid glass".to_string())]);
    close_settings(&mut h);
    open_all_notes(&mut h, 0);
    let p = all_notes_panel_spot(&h);
    assert!(looks_like(p, GLASS), "both take the Settings sidebar's theme: {p:?}");
}

#[test]
fn the_themes_survive_a_restart() {
    let mut h = started();
    open_on(&mut h, tab::APPEARANCE);
    h.settings_appearance_click(THEME_SWITCH);
    pick_theme(&mut h, THEME_A, GLASS);
    pick_theme(&mut h, THEME_B, PAPER);
    close_settings(&mut h);

    h.restart().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    open_on(&mut h, tab::APPEARANCE);
    assert!(!h.settings_theme_shared());
    assert_eq!(
        h.settings_theme_rows(),
        [("Settings sidebar".to_string(), "Liquid glass".to_string()), ("Notes sidebar".to_string(), "Paper".to_string())]
    );
    assert!(looks_like(settings_panel_spot(&h), GLASS));
    close_settings(&mut h);
    open_all_notes(&mut h, 0);
    assert!(near_rgb(all_notes_panel_spot(&h), PAPER_FILL));
}

/// How many near-white pixels sit in the box (x0..x1, y0..y1) of a shot: a light text shadow (the
/// slate panel, the desk and the dark ink have none).
fn bright_pixels(shot: &snap::shot::Snapshot, x0: u32, x1: u32, y0: u32, y1: u32) -> usize {
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|&(x, y)| shot.pixel(x, y).iter().all(|&c| c >= 0xEC))
        .count()
}

#[test]
fn transparent_sidebar_labels_carry_a_light_text_shadow() {
    let mut h = started();
    new_note(&mut h);
    open_all_notes(&mut h, 1);
    open_on(&mut h, tab::APPEARANCE);

    // The "Settings" title and the "Notes" header, clear of the lit Tab and of the rows.
    let title = |h: &Harness| {
        let s = h.render_settings();
        bright_pixels(&s, 20, 220, s.height / 2 - 286, s.height / 2 - 220)
    };
    let header = |h: &Harness| {
        let s = h.render_all_notes_on_desk().unwrap();
        let top = s.height / 2 - (s.height - 84) / 2;
        bright_pixels(&s, 20, 140, top + 10, top + 60)
    };
    assert_eq!((title(&h), header(&h)), (0, 0), "Default: no shadow");
    pick_theme(&mut h, THEME_A, TRANSPARENT);
    assert!(title(&h) > 50, "the Settings title has a shadow: {}", title(&h));
    assert!(header(&h) > 50, "the Notes header has a shadow: {}", header(&h));
}

#[test]
fn the_theme_switch_and_the_sidebar_groups_switch_are_independent() {
    let mut h = both_sidebars();
    // The Theme switch off: Height still drives both Sidebars.
    h.settings_appearance_click(THEME_SWITCH);
    assert!(!h.settings_theme_shared());
    pick(&mut h, HEIGHT[0], FIT_CONTENT);
    let ((s_h, ..), (n_h, ..)) = (settings_place(&h), notes_place(&h));
    assert!(close_to(s_h, SETTINGS_FIT_H, EDGE), "Settings Fit content: {s_h}");
    assert!(n_h < notes_place_match(&h) - 50.0, "Notes Fit content too: {n_h}");

    // The Sidebar group's switch off: the Theme switch stays off, and back on it does not move it.
    h.settings_appearance_click(THEME_SWITCH);
    assert!(h.settings_theme_shared());
    h.settings_appearance_click(SAME_SWITCH);
    assert!(h.settings_theme_shared(), "the Sidebar group's switch leaves the Theme's alone");
    assert_eq!(h.settings_theme_rows().len(), 1);
}

/// The All notes Sidebar's Paper height (what Match note makes its panel).
fn notes_place_match(h: &Harness) -> f32 {
    let (top, bottom) = h.all_notes_paper_span();
    bottom - top
}
