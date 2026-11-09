//! The Settings page: its window lifecycle (open from a dropdown, close back to the note).

use snap::Harness;
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
    h.right_click(0, Some(&["New note"]));
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
    h.right_click(0, Some(&["New note"]));
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
    h.right_click(0, Some(&["New note"]));
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

fn window_at(h: &Harness) -> (i32, i32, u32, u32) {
    h.settings_window_geometry().unwrap()
}

#[test]
fn opening_fades_the_note_then_grows_left_and_closing_comes_back_exactly() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    let note = h.note_geometry(0);
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();

    // the host's own text fades out first, Settings is not on screen yet
    assert!(h.note_visible(0));
    assert!(!h.settings_shown());
    h.advance_frames(ms(40));
    let o = h.note_content_opacity(0);
    assert!(o > 0.0 && o < 1.0, "mid-fade: {o}");
    assert_eq!(window_at(&h), (note.x, note.y, note.w, note.h), "waits exactly on the note's Paper");

    h.advance_frames(ms(80)); // fade done (on the next frame tick): Settings shown
    assert!(h.settings_shown());
    h.advance_frames(ms(16)); // swap: host hidden
    assert!(!h.note_visible(0));

    h.advance_frames(ms(80)); // into the morph
    let (x, y, w, _) = window_at(&h);
    assert!(w > note.w && w < FULL_W, "wider than the note by part of the Sidebar: {w}");
    assert!(x < note.x && x > note.x - STRIP, "the strip is partly out: {x}");
    assert_eq!(y, note.y, "top-left anchored");

    h.advance_frames(ms(300)); // done
    let (x, y, w, hh) = window_at(&h);
    assert_eq!((x + STRIP, y, w), (note.x, note.y, FULL_W));
    assert_eq!(hh, 600 + 2 * MARGIN as u32);

    close_settings(&mut h);
    assert!(!h.is_settings_open());
    assert!(h.note_visible(0));
    assert_eq!(h.note_geometry(0), note);
    assert_eq!(h.note_content_opacity(0), 1.0);
}

#[test]
fn the_morph_takes_about_300ms_with_ease_out() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    let note = h.note_geometry(0);
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(106)); // fade (90) + swap (16): the morph starts here
    h.advance_frames(ms(80));
    let (_, _, w1, _) = window_at(&h);
    h.advance_frames(ms(80));
    let (_, _, w2, _) = window_at(&h);
    let grown = |w: u32| w as f32 - note.w as f32;
    let total = (FULL_W - note.w) as f32;
    assert!(grown(w1) > total / 3.0, "ease-out: well past a third after a third of the time ({} of {total})", grown(w1));
    assert!(w2 > w1 && w2 < FULL_W);
    h.advance_frames(ms(100)); // 260 ms into the morph
    assert_eq!(window_at(&h).2, FULL_W);
}

#[test]
fn close_mid_open_reverses_from_the_current_state() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    let note = h.note_geometry(0);
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(106 + 64));
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
fn close_during_the_note_fade_never_shows_settings() {
    let mut h = Harness::new();
    h.start().unwrap();
    let note = h.note_geometry(0);
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(40));
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
    h.advance_frames(ms(106 + 120));
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
