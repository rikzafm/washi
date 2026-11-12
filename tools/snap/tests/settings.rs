//! The Settings page: its window lifecycle (open from a dropdown, close back to the note).

use snap::{tab, Harness};
use std::time::Duration;
use washi::platform::ScreenRect;
use washi::storage::Geometry;

/// Sidebar strip, Paper width and the Paper's margin, as the spec fixes them.
const STRIP: i32 = 282;
const PAPER_W: u32 = 520;
const MARGIN: i32 = 42;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Press Settings and let the transition play out.
fn open_settings_from(h: &mut Harness, note: usize) {
    h.toggle_dropdown(note);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
}

/// Press the X and let the transition play out.
fn close_settings(h: &mut Harness) {
    h.settings_close();
    h.advance_frames(ms(600));
}

#[test]
fn settings_opens_at_the_note_hides_only_the_host_and_closes_back() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.new_note_from(0);
    assert_eq!(h.open_note_count(), 2);
    h.set_note_text(0, "# Host\nsome text");
    // well inside the screen, so nothing needs to shift
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    let before = h.note_geometry(0);
    let text = h.note_text(0);

    open_settings_from(&mut h, 0);

    assert!(h.is_settings_open());
    assert!(!h.note_visible(0), "the host note is hidden");
    assert!(h.note_visible(1), "other notes stay");
    let (x, y, w, _) = h.settings_window_geometry().unwrap();
    // The Paper's top-left stays where the note's Paper was: the window grows left by the Sidebar.
    assert_eq!(x + STRIP, before.x);
    assert_eq!(y, before.y);
    assert_eq!(w, STRIP as u32 + PAPER_W + 2 * MARGIN as u32);

    close_settings(&mut h);

    assert!(!h.is_settings_open());
    assert!(h.note_visible(0));
    assert_eq!(h.note_geometry(0), before);
    assert_eq!(h.note_text(0), text);
}

#[test]
fn the_host_stays_up_until_settings_has_drawn_and_comes_back_before_settings_goes() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(16));
    assert!(h.settings_shown());
    assert!(h.note_visible(0), "the note is not hidden in the frame Settings first shows");
    h.advance_frames(ms(60));
    assert!(!h.note_visible(0), "then it hides");
    h.advance_frames(ms(600));

    h.settings_close();
    let mut both = false;
    for _ in 0..40 {
        h.advance_frames(ms(8));
        if h.note_visible(0) && h.settings_shown() {
            both = true;
        }
        if !h.is_settings_open() {
            break;
        }
    }
    assert!(both, "the note shows before Settings hides");
    h.advance_frames(ms(300));
    assert!(h.note_visible(0));
}

#[test]
fn either_tape_drags_the_settings_window() {
    let mut h = Harness::new();
    open_at_rest(&mut h);
    let (x, y, w, hh) = window_at(&h);
    let (wx, wy, _, _) = h.settings_native_window_geometry().unwrap();
    let (px, py) = ((x - wx) as f32, (y - wy) as f32);
    let paper_left = px + STRIP as f32 + MARGIN as f32;
    let moves = |h: &Harness| h.control.log.borrow().iter().filter(|c| matches!(c, snap::WindowCommand::Move(_))).count();
    let before = moves(&h);
    // the top-left tape (paper 36.5, 5.5) and the bottom-right one
    h.settings_press_at(paper_left + 36.5, py + MARGIN as f32 + 5.5);
    assert_eq!(moves(&h), before + 1, "top tape");
    h.settings_press_at(px + w as f32 - MARGIN as f32 - 36.5, py + hh as f32 - MARGIN as f32 - 8.5);
    assert_eq!(moves(&h), before + 2, "bottom tape");
}

/// The Sidebar's panel is the All notes one (270 wide, centred at x = 143, so its left edge is at 8):
/// a Tab and the X are pressed where they sit on it, at spots the old, further-right panel missed.
#[test]
fn the_sidebars_tabs_and_close_are_clicked_where_the_panel_puts_them() {
    let mut h = Harness::new();
    open_at_rest(&mut h);
    // The panel's top-left corner in the window: by default (Match note) the panel is as tall as
    // the Paper, so its top is the Paper's, a margin down the page.
    let panel = |h: &Harness| {
        let (x, y, _, _) = window_at(h);
        let (wx, wy, _, _) = h.settings_native_window_geometry().unwrap();
        ((x - wx) as f32, (y - wy) as f32 + MARGIN as f32)
    };
    // The third Tab is 189..237 down the panel, 12..258 across.
    let (px, top) = panel(&h);
    h.settings_press_at(px + 28.0, top + 213.0);
    assert_eq!(h.settings_selected_tab(), tab::STYLE, "the Font & Style Tab at its left end");
    h.advance_frames(ms(600));
    // The X: 44px around (238, 32) on the panel (which moved with the shorter page).
    let (px, top) = panel(&h);
    h.settings_press_at(px + 232.0, top + 32.0);
    h.advance_frames(ms(600));
    assert!(!h.is_settings_open(), "the X closed Settings");
}

#[test]
fn escape_closes_settings() {
    let mut h = Harness::new();
    h.start().unwrap();
    let before = h.note_geometry(0);
    open_settings_from(&mut h, 0);
    assert!(h.is_settings_open());

    h.settings_press_escape();
    h.advance_frames(ms(600));

    assert!(!h.is_settings_open());
    assert!(h.note_visible(0));
    assert_eq!(h.note_geometry(0), before);
}

#[test]
fn a_second_press_brings_the_existing_page_forward() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.new_note_from(0);
    let windows_before = h.window_count();

    open_settings_from(&mut h, 0);
    let windows_with_settings = h.window_count();
    assert_eq!(windows_with_settings, windows_before + 1);
    let geometry = h.settings_window_geometry();

    // another note's dropdown
    open_settings_from(&mut h, 1);

    assert_eq!(h.window_count(), windows_with_settings, "no second Settings page");
    assert_eq!(h.settings_window_geometry(), geometry);
    assert!(!h.note_visible(0), "still hosted by the first note");
    assert!(h.note_visible(1));
}

#[test]
fn the_compact_dropdown_has_no_settings() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.toggle_dropdown(0);
    assert_eq!(h.dropdown_settings_visible(), Some(true));

    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());
    h.invoke_editor_menu();

    assert_eq!(h.dropdown_settings_visible(), Some(false));
}

#[test]
fn all_notes_never_shows_the_settings_host() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.new_note_from(0);
    let (id0, id1) = (h.note_id(0), h.note_id(1));
    open_settings_from(&mut h, 0);
    assert!(!h.note_visible(0));

    h.toggle_dropdown(1);
    h.invoke_dropdown_all_notes();
    h.invoke_all_notes_select_note(id0);
    h.invoke_all_notes_select_note(id1);

    assert!(!h.note_visible(0), "Settings still stands in for note 0");
}

#[test]
fn settings_stays_on_screen_near_the_left_edge() {
    let mut h = Harness::new();
    h.set_work_areas(vec![ScreenRect::new(0, 0, 1920, 1040)]);
    h.start().unwrap();
    let near_edge = Geometry { x: 10, y: 20, ..h.note_geometry(0) };
    h.place_note(0, near_edge);

    open_settings_from(&mut h, 0);

    let (x, y, _, _) = h.settings_window_geometry().unwrap();
    assert_eq!(x, 0, "the Sidebar strip starts at the screen's left edge");
    assert!(y >= 0);

    close_settings(&mut h);
    assert_eq!(h.note_geometry(0), near_edge, "the shift made to stay on screen is undone");
}

// ---- the open and close transition ----

/// Width of the Settings window at rest.
const FULL_W: u32 = STRIP as u32 + PAPER_W + 2 * MARGIN as u32;

/// Each tab's Paper height: the laid-out content plus the Paper's padding, at least 600 (the mockup's
/// 658, 603, 767 and 600; see settings_fidelity.rs, plus the rows General has gained since), in whole
/// logical px as the window has them.
/// Font & Style (since Fade text edge and UI size left it) is shorter than the minimum. Appearance is
/// not in the mockup: see `APPEARANCE_PAPER_H` in settings_fidelity.rs.
const GENERAL_H: u32 = 798;
const APPEARANCE_H: u32 = 699;
const STYLE_H: u32 = 600;
const SHORTCUTS_H: u32 = 768;
const PRIVACY_H: u32 = 600;

fn window_at(h: &Harness) -> (i32, i32, u32, u32) {
    h.settings_window_geometry().unwrap()
}

#[test]
fn opening_grows_left_at_once_with_no_wait_for_a_text_fade_and_closing_comes_back_exactly() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    let note = h.note_geometry(0);
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();

    // Settings is on screen from the first instant, exactly on the note's Paper (no sequential fade first)
    assert!(h.settings_shown());
    assert_eq!(window_at(&h), (note.x, note.y, note.w, note.h), "starts exactly on the note's Paper");
    h.advance_frames(ms(16)); // swap: the host stays up until Settings has drawn
    assert!(h.note_visible(0));
    h.advance_frames(ms(48)); // into the morph, the host now hidden
    assert!(!h.note_visible(0));
    let (x, y, w, _) = window_at(&h);
    assert!(w > note.w && w < FULL_W, "wider than the note by part of the Sidebar: {w}");
    assert!(x < note.x && x > note.x - STRIP, "the strip is partly out: {x}");
    assert_eq!(y, note.y, "top-left anchored");

    h.advance_frames(ms(200)); // done
    let (x, y, w, hh) = window_at(&h);
    assert_eq!((x + STRIP, y, w), (note.x, note.y, FULL_W));
    assert_eq!(hh, GENERAL_H + 2 * MARGIN as u32, "opens at the General tab's height, not at 600");

    close_settings(&mut h);
    assert!(!h.is_settings_open());
    assert!(h.note_visible(0));
    assert_eq!(h.note_geometry(0), note);
    assert_eq!(h.note_content_opacity(0), 1.0);
}

#[test]
fn opening_is_snappy_over_in_about_200ms_with_ease_out() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    let note = h.note_geometry(0);
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(16 + 64)); // swap frame, then a third of the morph
    let (_, _, w1, _) = window_at(&h);
    h.advance_frames(ms(64));
    let (_, _, w2, _) = window_at(&h);
    let grown = |w: u32| w as f32 - note.w as f32;
    let total = (FULL_W - note.w) as f32;
    assert!(grown(w1) > total / 3.0, "ease-out: well past a third after a third of the time ({} of {total})", grown(w1));
    assert!(w2 > w1 && w2 < FULL_W);
    h.advance_frames(ms(100)); // 244 ms after the press: at rest
    assert_eq!(window_at(&h).2, FULL_W);
    assert!(!h.settings_ticking());
}

#[test]
fn open_takes_about_200ms_and_close_about_250ms() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    let note = h.note_geometry(0);
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(230));
    assert!(!h.settings_ticking(), "open is at rest by 230 ms");
    assert_eq!(window_at(&h).2, FULL_W);

    h.settings_close();
    h.advance_frames(ms(260)); // (harness frames are 16 ms; real ones are 8 ms)
    assert!(!h.is_settings_open(), "close is over by 260 ms");
    assert_eq!(h.note_geometry(0), note, "lands exactly on the host's original geometry");
    assert!(h.note_visible(0));
    assert_eq!(h.note_content_opacity(0), 1.0);
}

#[test]
fn close_mid_open_reverses_from_the_current_state() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    let note = h.note_geometry(0);
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(16 + 64));
    let at_close = window_at(&h).2;
    assert!(at_close > note.w && at_close < FULL_W);

    h.settings_close();
    let mut last = at_close;
    for _ in 0..30 {
        h.advance_frames(ms(16));
        if !h.is_settings_open() || !h.settings_shown() {
            break;
        }
        let w = window_at(&h).2;
        assert!(w <= last, "shrinks monotonically ({w} after {last})");
        last = w;
    }
    h.advance_frames(ms(600));
    assert!(!h.is_settings_open());
    assert_eq!(h.note_geometry(0), note);
    assert!(h.note_visible(0));
}

#[test]
fn frames_hold_the_fine_timer_resolution_only_while_they_run() {
    // Windows timers fire on a ~15.6 ms grid unless asked: that, not the drawing, made it choppy
    let mut h = Harness::new();
    h.start().unwrap();
    assert_eq!(h.control.fine_timer_holds.get(), 0);
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    assert_eq!(h.control.fine_timer_holds.get(), 1, "held while opening");
    h.advance_frames(ms(300));
    assert_eq!(h.control.fine_timer_holds.get(), 0, "given back at rest");
    h.settings_click_tab(tab::STYLE);
    assert_eq!(h.control.fine_timer_holds.get(), 1, "held for a tab switch");
    h.advance_frames(ms(300));
    h.settings_close();
    h.advance_frames(ms(600));
    assert_eq!(h.control.fine_timer_holds.get(), 0, "balanced after close");
}

#[test]
fn close_right_after_pressing_settings_lands_on_the_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    let note = h.note_geometry(0);
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(8));
    h.settings_close();
    h.advance_frames(ms(600));

    assert!(!h.settings_shown());
    assert!(!h.is_settings_open());
    assert!(h.note_visible(0));
    assert_eq!(h.note_content_opacity(0), 1.0);
    assert_eq!(h.note_geometry(0), note);
}

#[test]
fn the_shift_to_stay_on_screen_grows_in_and_is_undone_on_close() {
    let mut h = Harness::new();
    h.set_work_areas(vec![ScreenRect::new(0, 0, 1920, 1040)]);
    h.start().unwrap();
    let near_edge = Geometry { x: 10, y: 20, ..h.note_geometry(0) };
    h.place_note(0, near_edge);
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(16 + 100));
    let (x, _, _, _) = window_at(&h);
    assert!(x >= 0, "never off the left edge mid-way: {x}");
    h.advance_frames(ms(300));
    assert_eq!(window_at(&h).0, 0);
    close_settings(&mut h);
    assert_eq!(h.note_geometry(0), near_edge);
}

#[test]
fn a_drag_of_the_page_is_kept_on_close() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    let note = h.note_geometry(0);
    open_settings_from(&mut h, 0);
    let (x, y, _, _) = window_at(&h);
    h.move_settings_window(x + 100, y + 50);

    close_settings(&mut h);

    assert_eq!(h.note_geometry(0), Geometry { x: note.x + 100, y: note.y + 50, ..note });
}

#[test]
fn the_frame_timer_stops_at_rest() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    assert!(h.settings_ticking());
    h.advance_frames(ms(600));
    assert!(h.is_settings_open());
    assert!(!h.settings_ticking());
}

// ---- tabs and the height tween ----

fn open_at_rest(h: &mut Harness) {
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    open_settings_from(h, 0);
}

/// The Sidebar is as tall as the window, and so is the Paper plus margin: both come from one height.
fn paper_h(h: &Harness) -> u32 {
    window_at(h).3 - 2 * MARGIN as u32
}

#[test]
fn each_tab_ends_at_its_height_and_the_width_stays() {
    let mut h = Harness::new();
    open_at_rest(&mut h);
    let (x0, y0, _, _) = window_at(&h);
    for (tab, paper) in [(tab::STYLE, STYLE_H), (tab::GENERAL, GENERAL_H), (tab::APPEARANCE, APPEARANCE_H), (tab::SHORTCUTS, SHORTCUTS_H), (tab::PRIVACY, PRIVACY_H)] {
        h.settings_click_tab(tab);
        h.advance_frames(ms(250));
        let (x, y, w, _) = window_at(&h);
        assert_eq!(paper_h(&h), paper, "tab {tab}");
        assert_eq!(w, FULL_W, "tab {tab}");
        assert_eq!((x, y), (x0, y0), "top-left stays anchored (tab {tab})");
        assert_eq!(h.settings_selected_tab(), tab);
        assert_eq!(h.settings_shown_tab(), tab);
        assert_eq!(h.settings_content_opacity(), 1.0);
        assert!(!h.settings_ticking());
    }
}

#[test]
fn the_height_tweens_over_about_130ms() {
    let mut h = Harness::new();
    open_at_rest(&mut h);
    h.settings_click_tab(tab::STYLE);
    h.settings_click_tab(tab::GENERAL);
    h.advance_frames(ms(250));
    assert_eq!(paper_h(&h), GENERAL_H);
    h.settings_click_tab(tab::STYLE);
    h.advance_frames(ms(32));
    let mid = paper_h(&h);
    assert!(mid > STYLE_H && mid < GENERAL_H, "mid-tween: {mid}");
    assert!(GENERAL_H - mid > (GENERAL_H - STYLE_H) / 2, "ease-out: past half the way a quarter in: {mid}");
    h.advance_frames(ms(64));
    assert!(paper_h(&h) < STYLE_H + 10, "nearly there at 3/4 of the time");
    h.advance_frames(ms(48));
    assert_eq!(paper_h(&h), STYLE_H, "done within 140 ms");
    assert!(!h.settings_ticking());
}

#[test]
fn switching_mid_tween_retargets_smoothly() {
    let mut h = Harness::new();
    open_at_rest(&mut h);
    h.settings_click_tab(tab::SHORTCUTS); // Shortcuts
    h.advance_frames(ms(250));
    h.settings_click_tab(tab::GENERAL); // towards General
    h.advance_frames(ms(64));
    h.settings_click_tab(tab::STYLE); // retarget to Font & Style
    let mut last_h = paper_h(&h) as i32;
    let mut last_o = h.settings_content_opacity();
    assert_eq!(h.settings_shown_tab(), tab::STYLE, "the new tab shows at once");
    for _ in 0..20 {
        h.advance_frames(ms(16));
        let now_h = paper_h(&h) as i32;
        assert!(now_h <= last_h, "monotonic: {now_h} after {last_h}");
        last_h = now_h;
        let o = h.settings_content_opacity();
        assert!(o >= 0.45 - 1e-6, "the content never goes blank: {o}");
        assert!(o >= last_o - 1e-6, "it only settles in: {last_o} -> {o}");
        last_o = o;
    }
    assert_eq!(last_h, STYLE_H as i32);
    assert_eq!(h.settings_shown_tab(), tab::STYLE);
    assert_eq!(h.settings_content_opacity(), 1.0);
}

#[test]
fn the_new_tab_shows_at_once_and_settles_in_without_going_blank() {
    let mut h = Harness::new();
    open_at_rest(&mut h);
    h.settings_click_tab(tab::SHORTCUTS);
    assert_eq!(h.settings_selected_tab(), tab::SHORTCUTS, "the Sidebar lights up at once");
    assert_eq!(h.settings_shown_tab(), tab::SHORTCUTS, "and the content with it");
    let (o0, dy0) = (h.settings_content_opacity(), h.settings_content_dy());
    assert!(o0 >= 0.45 - 1e-6 && o0 < 0.6, "starts half visible: {o0}");
    assert!(dy0 > 4.0, "a little low: {dy0}");
    let mut last = (o0, dy0);
    for _ in 0..12 {
        h.advance_frames(ms(16));
        let now = (h.settings_content_opacity(), h.settings_content_dy());
        assert!(now.0 >= last.0 - 1e-6 && now.1 <= last.1 + 1e-6, "settles monotonically: {last:?} -> {now:?}");
        last = now;
    }
    assert_eq!((h.settings_content_opacity(), h.settings_content_dy()), (1.0, 0.0));
    assert!(!h.settings_ticking());
}

#[test]
fn a_tab_switch_does_not_resize_the_window_every_frame() {
    let mut h = Harness::new();
    open_at_rest(&mut h);
    for tab in [tab::STYLE, tab::GENERAL, tab::APPEARANCE, tab::SHORTCUTS, tab::PRIVACY, tab::GENERAL] {
        let before = h.settings_window_resizes();
        h.settings_click_tab(tab);
        h.advance_frames(ms(250));
        let made = h.settings_window_resizes() - before;
        assert!(made <= 2, "tab {tab}: at most grow once and trim once, got {made}");
        assert_eq!(h.settings_native_window_geometry(), h.settings_window_geometry(), "trimmed to the page at rest");
    }
}

#[test]
fn opening_and_closing_do_not_resize_the_window_every_frame() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    assert!(h.settings_window_resizes() <= 2, "open: {}", h.settings_window_resizes());
    let before = h.settings_window_resizes();
    h.settings_close();
    h.advance_frames(ms(100));
    assert!(h.settings_window_resizes() - before <= 1, "close: {}", h.settings_window_resizes() - before);
    h.advance_frames(ms(500));
    assert!(!h.is_settings_open());
}

#[test]
fn a_taller_tab_lifts_the_page_near_the_bottom_of_the_screen() {
    let mut h = Harness::new();
    h.set_work_areas(vec![ScreenRect::new(0, 0, 1920, 900)]);
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    open_settings_from(&mut h, 0);
    h.settings_click_tab(tab::STYLE);
    h.advance_frames(ms(250));
    let (_, y_short, _, hh) = window_at(&h);
    assert_eq!(hh, STYLE_H + 2 * MARGIN as u32);

    h.settings_click_tab(tab::SHORTCUTS);
    h.advance_frames(ms(250));
    let (_, y, _, hh) = window_at(&h);
    assert_eq!(hh, SHORTCUTS_H + 2 * MARGIN as u32);
    assert!(y + hh as i32 <= 900, "still on screen: {y} + {hh}");
    assert!(y <= y_short);
}

#[test]
fn closing_mid_tab_switch_still_puts_the_host_back() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    let note = h.note_geometry(0);
    open_settings_from(&mut h, 0);
    h.settings_click_tab(tab::SHORTCUTS);
    h.advance_frames(ms(64));
    close_settings(&mut h);
    assert!(!h.is_settings_open());
    assert!(h.note_visible(0));
    assert_eq!(h.note_geometry(0), note);
}

#[test]
fn os_close_request_closes_settings_and_allows_reopening() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    let note = h.note_geometry(0);

    open_settings_from(&mut h, 0);
    assert!(h.is_settings_open());
    assert!(!h.note_visible(0));

    // Simulate Alt+F4 or OS close request
    h.settings_request_close();
    h.advance_frames(ms(600));

    assert!(!h.is_settings_open(), "settings should be closed");
    assert!(h.note_visible(0), "host note should be restored");
    assert_eq!(h.note_geometry(0), note);

    // Opening settings again from the dropdown must succeed
    open_settings_from(&mut h, 0);
    assert!(h.is_settings_open(), "settings must open again after OS close");
    assert!(!h.note_visible(0));
}
