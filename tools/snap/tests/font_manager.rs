//! Settings › Font & Style › the Font row and the Font manager: a taped slip beside the Settings
//! Paper that lists the installed Fonts. The Font row names the Font in use and holds the manage
//! button; the button opens and closes the slip, and so do Esc, a Tab switch and closing Settings. The
//! Settings window grows a strip on its right while the slip is open, and the list scrolls with a bar
//! that behaves like a Note's.

use slint::ComponentHandle;
use snap::{tab, Harness};
use std::time::Duration;
use washi::platform::ScreenRect;
use washi::settings::anim::MANAGER_STRIP;
use washi::storage::Geometry;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// The manage button, as `hit-x` in style-tab.slint defines the id.
const MANAGE_BUTTON: i32 = 0;

fn started() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    h
}

/// Settings open on the Font & Style tab, with the transition and tab switch finished.
fn open_style_tab(h: &mut Harness) {
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    h.settings_click_tab(tab::STYLE);
    h.advance_frames(ms(600));
}

fn open_manager(h: &mut Harness) {
    h.settings_style_click(MANAGE_BUTTON);
    h.advance_frames(ms(600));
}

// ---- the Font row ----

#[test]
fn the_font_row_names_the_font_in_use_and_its_manage_button_is_called_manage_fonts() {
    let mut h = started();
    open_style_tab(&mut h);
    assert_eq!(h.settings_font_row(), ("Shadows Into Light".to_string(), "One style".to_string()));
    assert_eq!(h.settings_manage_button_label(), "Manage fonts");
    assert!(!h.settings_manager_open(), "the slip is closed until asked for");
}

#[test]
fn the_old_list_picker_and_its_six_fake_fonts_are_gone() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(MANAGE_BUTTON);
    assert_eq!(h.settings_style_state().2, -1, "the button opens the slip, not a list");
}

// ---- opening and closing ----

#[test]
fn the_manage_button_opens_the_slip_and_pressing_it_again_closes_it() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(MANAGE_BUTTON);
    assert!(h.settings_manager_open());
    h.advance_frames(ms(600));
    assert_eq!(h.settings_manager_progress(), 1.0);
    h.settings_style_click(MANAGE_BUTTON);
    assert!(!h.settings_manager_open());
    h.advance_frames(ms(600));
    assert_eq!(h.settings_manager_progress(), 0.0);
    assert!(h.is_settings_open(), "only the slip closed");
}

#[test]
fn opening_and_closing_are_tweens_on_the_settings_clock_and_the_frame_timer_stops_at_rest() {
    let mut h = started();
    open_style_tab(&mut h);
    assert!(!h.settings_ticking());
    h.settings_style_click(MANAGE_BUTTON);
    assert!(h.settings_ticking());
    h.advance_frames(ms(64));
    let mid = h.settings_manager_progress();
    assert!(mid > 0.0 && mid < 1.0, "part way after 64 ms: {mid}");
    h.advance_frames(ms(400));
    assert_eq!(h.settings_manager_progress(), 1.0);
    assert!(!h.settings_ticking(), "at rest again");

    h.settings_style_click(MANAGE_BUTTON);
    h.advance_frames(ms(64));
    let mid = h.settings_manager_progress();
    assert!(mid > 0.0 && mid < 1.0, "closing is a tween too: {mid}");
    h.advance_frames(ms(400));
    assert_eq!(h.settings_manager_progress(), 0.0);
    assert!(!h.settings_ticking());
}

#[test]
fn the_manage_button_is_filled_dark_while_the_slip_is_open() {
    let mut h = started();
    open_style_tab(&mut h);
    assert!(!h.settings_manage_button_filled());
    open_manager(&mut h);
    assert!(h.settings_manage_button_filled());
}

#[test]
fn esc_closes_the_slip_before_it_closes_settings() {
    let mut h = started();
    open_style_tab(&mut h);
    open_manager(&mut h);
    h.settings_press_escape();
    h.advance_frames(ms(600));
    assert!(h.is_settings_open(), "the first Esc only closed the slip");
    assert!(!h.settings_manager_open());
    assert_eq!(h.settings_manager_progress(), 0.0);
    h.settings_press_escape();
    h.advance_frames(ms(600));
    assert!(!h.is_settings_open(), "the second Esc closed Settings");
}

#[test]
fn switching_tab_closes_the_slip() {
    let mut h = started();
    open_style_tab(&mut h);
    open_manager(&mut h);
    h.settings_click_tab(tab::GENERAL);
    h.advance_frames(ms(600));
    assert!(!h.settings_manager_open());
    assert_eq!(h.settings_manager_progress(), 0.0);
    h.settings_click_tab(tab::STYLE);
    h.advance_frames(ms(600));
    assert!(!h.settings_manager_open(), "and it stays closed on coming back");
}

#[test]
fn closing_settings_closes_the_slip_and_gives_the_note_back() {
    let mut h = started();
    let before = h.note_geometry(0);
    open_style_tab(&mut h);
    open_manager(&mut h);
    h.settings_close();
    h.advance_frames(ms(600));
    assert!(!h.is_settings_open());
    assert_eq!(h.note_geometry(0), before, "the note is back where it was");

    // and the next time Settings opens, the slip is closed
    open_style_tab(&mut h);
    assert!(!h.settings_manager_open());
}

#[test]
fn closing_settings_from_the_os_while_the_slip_is_opening_is_clean() {
    let mut h = started();
    open_style_tab(&mut h);
    h.settings_style_click(MANAGE_BUTTON);
    h.advance_frames(ms(48));
    h.settings_request_close();
    h.advance_frames(ms(800));
    assert!(!h.is_settings_open());
}

// ---- the window's right-hand strip ----

#[test]
fn the_page_grows_a_strip_on_its_right_while_the_slip_is_open() {
    let mut h = started();
    open_style_tab(&mut h);
    let (x, y, w, hh) = h.settings_window_geometry().unwrap();
    open_manager(&mut h);
    let (x2, y2, w2, h2) = h.settings_window_geometry().unwrap();
    assert_eq!((x2, y2, h2), (x, y, hh), "only the right edge moved");
    assert_eq!(w2, w + MANAGER_STRIP as u32);
    assert_eq!(h.settings_native_window_geometry().map(|g| (g.2, g.3)), Some((w2, h2)), "the window is trimmed to the page at rest");

    h.settings_style_click(MANAGE_BUTTON);
    h.advance_frames(ms(600));
    assert_eq!(h.settings_window_geometry().unwrap(), (x, y, w, hh), "and it goes again");
}

#[test]
fn the_strip_is_the_width_the_slip_needs() {
    let mut h = started();
    open_style_tab(&mut h);
    let ui = h.app().settings().unwrap().ui().clone_strong();
    assert_eq!(ui.global::<washi::note_window::ManagerMetrics>().get_strip(), MANAGER_STRIP);
}

#[test]
fn the_page_shifts_left_when_the_slip_has_no_room_at_the_right_screen_edge() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_work_areas(vec![ScreenRect::new(0, 0, 1920, 1040)]);
    h.place_note(0, Geometry { x: 1500, y: 100, ..h.note_geometry(0) });
    open_style_tab(&mut h);
    let (x, ..) = h.settings_window_geometry().unwrap();
    open_manager(&mut h);
    let (x2, _, w2, _) = h.settings_window_geometry().unwrap();
    assert!(x2 < x, "the page moved left: {x} to {x2}");
    assert!(x2 + w2 as i32 <= 1920, "the whole page, slip included, is on screen: right edge at {}", x2 + w2 as i32);
    assert_eq!(x2 + w2 as i32, 1920, "and it moved no further than that");
}

#[test]
fn the_page_stays_put_when_there_is_room() {
    let mut h = started();
    open_style_tab(&mut h);
    let (x, y, ..) = h.settings_window_geometry().unwrap();
    open_manager(&mut h);
    let (x2, y2, ..) = h.settings_window_geometry().unwrap();
    assert_eq!((x2, y2), (x, y));
}

#[test]
fn the_window_is_not_resized_every_frame_while_the_slip_opens() {
    let mut h = started();
    open_style_tab(&mut h);
    let before = h.settings_window_resizes();
    open_manager(&mut h);
    let used = h.settings_window_resizes() - before;
    assert!(used <= 2, "one move to the canvas and one to trim it: {used}");
}

// ---- what the slip shows ----

#[test]
fn the_slip_has_its_title_the_default_font_and_an_install_area() {
    let mut h = started();
    open_style_tab(&mut h);
    open_manager(&mut h);
    assert_eq!(h.settings_manager_titles(), ("Installed fonts".to_string(), "Install a font".to_string()));
    let rows = h.settings_manager_rows();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!((r.name.as_str(), r.subtitle.as_str()), ("Shadows Into Light", "One style"));
    assert!(r.active, "the Default font is the Font in use, with its radio dot filled");
    assert!(r.locked, "the Default font cannot be removed");
    assert_eq!(r.tag, "Default");
}

#[test]
fn a_short_list_makes_a_short_slip_that_never_scrolls() {
    let mut h = started();
    open_style_tab(&mut h);
    open_manager(&mut h);
    let (_, max) = h.settings_manager_scroll();
    assert_eq!(max, 0.0);
    let (x, y) = h.settings_manager_list_middle();
    h.settings_wheel_at(x, y, -300.0);
    assert_eq!(h.settings_manager_scroll().0, 0.0);
    h.advance(ms(300));
    assert_eq!(h.settings_manager_bar().0, 0.0);
    assert!(h.settings_manager_height() <= h.settings_paper_height() as f32, "never taller than the Paper");
}

// ---- the list scrolls, with a bar like a Note's ----

fn with_many_rows() -> Harness {
    let mut h = started();
    open_style_tab(&mut h);
    open_manager(&mut h);
    h.settings_manager_test_rows(14);
    h.advance_frames(ms(100));
    h
}

#[test]
fn with_more_fonts_than_fit_the_slip_stops_at_the_paper_height_and_the_list_scrolls() {
    let mut h = with_many_rows();
    assert!(h.settings_manager_height() <= h.settings_paper_height() as f32);
    let (offset, max) = h.settings_manager_scroll();
    assert_eq!(offset, 0.0);
    assert!(max > 100.0, "there is something to scroll to: {max}");

    let (x, y) = h.settings_manager_list_middle();
    h.settings_wheel_at(x, y, -60.0);
    let (a, _) = h.settings_manager_scroll();
    assert!(a > 0.0, "the wheel scrolled: {a}");
    h.settings_wheel_at(x, y, -100000.0);
    assert_eq!(h.settings_manager_scroll().0, max, "and it stops at the end");
    h.settings_wheel_at(x, y, 100000.0);
    assert_eq!(h.settings_manager_scroll().0, 0.0, "and at the top");
}

#[test]
fn the_slips_bar_shows_only_when_scrolling_hovered_or_dragged_and_fades_like_a_notes() {
    let mut h = with_many_rows();
    h.advance(ms(1000));
    assert_eq!(h.settings_manager_bar().0, 0.0, "hidden until used");

    let (x, y) = h.settings_manager_list_middle();
    h.settings_pointer_move(x, y);
    h.advance(ms(300));
    assert_eq!(h.settings_manager_bar().0, 0.0, "hovering the list alone does not show it");

    h.settings_wheel_at(x, y, -60.0);
    h.advance(ms(300));
    assert_eq!(h.settings_manager_bar().0, 1.0, "scrolling shows it");
    h.advance(ms(500));
    assert_eq!(h.settings_manager_bar().0, 0.0, "and it hides again once scrolling stops");

    let (_, tx, ty) = h.settings_manager_bar();
    h.settings_pointer_move(tx, ty);
    h.advance(ms(300));
    assert_eq!(h.settings_manager_bar().0, 1.0, "hovering the thumb shows it");
    h.advance(ms(1000));
    assert_eq!(h.settings_manager_bar().0, 1.0, "and keeps it");
    h.settings_pointer_move(x, y);
    h.advance(ms(300));
    assert_eq!(h.settings_manager_bar().0, 0.0, "leaving it hides it");
}

#[test]
fn dragging_the_thumb_scrolls_the_list_and_the_bar_stays_while_dragging() {
    let mut h = with_many_rows();
    let (_, max) = h.settings_manager_scroll();
    let (_, tx, ty) = h.settings_manager_bar();
    h.settings_pointer_move(tx, ty);
    h.settings_pointer_press(tx, ty);
    assert_eq!(h.settings_manager_scroll().0, 0.0, "pressing the thumb does not jump");
    h.settings_pointer_move(tx - 80.0, ty + 60.0);
    let mid = h.settings_manager_scroll().0;
    assert!(mid > 0.0 && mid < max, "dragged part of the way, even with drift sideways: {mid}");
    assert_eq!(h.settings_manager_bar().0, 1.0);
    h.settings_pointer_move(tx, ty + 5000.0);
    assert_eq!(h.settings_manager_scroll().0, max, "dragging past the end stops at the end");
    h.settings_pointer_move(tx, ty - 5000.0);
    assert_eq!(h.settings_manager_scroll().0, 0.0);
    h.settings_pointer_release(tx - 300.0, ty - 5000.0);
    h.advance(ms(1200));
    assert_eq!(h.settings_manager_bar().0, 0.0, "released away from the bar, it hides");
}

#[test]
fn only_the_thumb_takes_the_pointer_not_the_track() {
    let mut h = with_many_rows();
    h.advance(ms(1000));
    let (_, tx, ty) = h.settings_manager_bar();
    // a press on the track, well below the thumb, scrolls nothing while the bar is hidden
    h.settings_pointer_move(tx, ty + 150.0);
    h.settings_pointer_press(tx, ty + 150.0);
    h.settings_pointer_release(tx, ty + 150.0);
    assert_eq!(h.settings_manager_scroll().0, 0.0);
}

#[test]
fn a_press_in_the_list_beside_the_bar_does_not_move_it() {
    let mut h = with_many_rows();
    let (x, y) = h.settings_manager_list_middle();
    h.settings_pointer_move(x, y);
    h.settings_pointer_press(x, y);
    h.settings_pointer_release(x, y);
    assert_eq!(h.settings_manager_scroll().0, 0.0);
}

#[test]
fn the_slips_scroll_does_not_move_the_settings_paper() {
    let mut h = with_many_rows();
    let (x, y) = h.settings_manager_list_middle();
    let paper = h.settings_scroll().0;
    h.settings_wheel_at(x, y, -80.0);
    assert!(h.settings_manager_scroll().0 > 0.0);
    assert_eq!(h.settings_scroll().0, paper);
}

// ---- what is drawn ----

/// Whether the pixel at `(x, y)` of a rendered Settings window is the Paper's own off-white.
fn is_paper(shot: &snap::shot::Snapshot, x: u32, y: u32) -> bool {
    shot.pixel(x, y) == [0xFF, 0xFF, 0xFD]
}

#[test]
fn the_slip_is_drawn_on_paper_of_its_own_right_of_the_settings_paper_and_only_while_open() {
    let mut h = started();
    open_style_tab(&mut h);
    let strip = h.app().settings().unwrap().ui().get_strip() as u32;
    // A blank spot of the slip: 300 px along and 40 down from its top left corner, which is 20 px
    // right of the Paper (the Sidebar's strip, the margin and 520 px in), 34 below the Paper's top.
    let (sx, sy) = (strip + 42 + 520 + 20 + 300, 42 + 34 + 40);

    let closed = h.render_settings();
    assert!(!is_paper(&closed, sx.min(closed.width - 1), sy), "nothing there while the slip is closed");

    open_manager(&mut h);
    let open = h.render_settings();
    if let Some(dir) = std::env::var_os("WASHI_SNAP_OUT_DIR") {
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join("font-manager-open.png"), open.png()).unwrap();
    }
    assert!(is_paper(&open, sx, sy), "the slip's own paper is there");
    assert!(!is_paper(&open, strip + 42 + 520 + 10, sy), "and the gap between it and the Paper is clear");

    h.settings_style_click(MANAGE_BUTTON);
    h.advance_frames(ms(600));
    assert!(!is_paper(&h.render_settings(), sx.min(closed.width - 1), sy), "gone again once closed");
}
