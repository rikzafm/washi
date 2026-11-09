//! The Settings page: its window lifecycle (open from a dropdown, close back to the note).

use snap::Harness;
use washi::platform::ScreenRect;
use washi::storage::Geometry;

/// Sidebar strip, Paper width and the Paper's margin, as the spec fixes them.
const STRIP: i32 = 282;
const PAPER_W: u32 = 520;
const MARGIN: i32 = 42;

fn open_settings_from(h: &mut Harness, note: usize) {
    h.toggle_dropdown(note);
    h.invoke_dropdown_settings();
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

    h.settings_close();

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

    h.settings_close();
    assert_eq!(h.note_geometry(0), near_edge, "the shift made to stay on screen is undone");
}
