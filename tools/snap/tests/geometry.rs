//! Moving, resizing, and keeping geometry: what the user grabs, what the platform is asked to do,
//! the minimum size, the geometry debounce, restoring after a restart, and off-screen recovery.
//!
//! A note opens at the default paper size (363 x 384) plus the 42 px margin on every side:
//! a 447 x 468 window whose paper spans x 42..405 and y 42..426.

use snap::{Harness, WindowCommand};
use std::time::Duration;
use washi::platform::{ResizeEdge, ScreenRect};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn note() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h
}

const W: u32 = 447;
const H: u32 = 468;
const MIN: u32 = 360 + 84;
/// Paper edges in window coordinates.
const L: f32 = 42.0;
const T: f32 = 42.0;
const R: f32 = 405.0;
const B: f32 = 426.0;
const CX: f32 = 225.0;
const CY: f32 = 235.0;

/// On the pink tape (its center is about 37 px right of and 6 px below the paper's top-left).
const TAPE: (f32, f32) = (80.0, 48.0);
/// In the top margin, right of the tape.
const TOP_MARGIN: (f32, f32) = (250.0, 56.0);
/// In the middle of the text area.
const TEXT: (f32, f32) = (150.0, 180.0);
/// On the first toolbar button.
const BUTTON: (f32, f32) = (L + 23.2 + 20.0, B - 56.0 + 20.0);

#[test]
fn new_notes_open_at_the_default_paper_size() {
    let h = note();
    assert_eq!(h.note_size(0), (W, H));
    assert_eq!(h.window_position(0), (0, 0));
}

#[test]
fn dragging_the_pink_tape_or_top_margin_moves_the_window() {
    for from in [TAPE, TOP_MARGIN] {
        let mut h = note();
        let grabbed = h.drag(0, from, (30.0, 20.0));
        assert_eq!(grabbed, Some(WindowCommand::Move(h.note_id(0))), "from {from:?}");
        assert_eq!(h.window_position(0), (30, 20), "from {from:?}");
        assert_eq!(h.note_size(0), (W, H), "a move never changes the size");
    }
}

#[test]
fn dragging_text_or_the_toolbar_does_not_move_the_window() {
    for from in [TEXT, BUTTON] {
        let mut h = note();
        h.pointer_move(0, 5.0, 5.0); // the toolbar is shown while the pointer is over the note
        h.advance(ms(300));
        assert_eq!(h.drag(0, from, (30.0, 20.0)), None, "from {from:?}");
        assert_eq!(h.window_position(0), (0, 0), "from {from:?}");
    }
}

#[test]
fn the_eight_zones_resize_in_their_direction() {
    use ResizeEdge::*;
    let zones = [
        ((250.0, T), North),
        ((R, T), NorthEast),
        ((R, CY), East),
        ((R, B), SouthEast),
        ((CX, B), South),
        ((L, B), SouthWest),
        ((L, CY), West),
        ((L, T), NorthWest),
    ];
    for ((x, y), edge) in zones {
        let mut h = note();
        let id = h.note_id(0);
        assert_eq!(h.drag(0, (x, y), (0.0, 0.0)), Some(WindowCommand::Resize(id, edge)), "at ({x}, {y})");
    }
}

#[test]
fn zones_straddle_the_edge_by_4_px() {
    let mut h = note();
    let id = h.note_id(0);
    // the east edge is at x = 405: 4 px inside and 4 px outside still resize, 6 px away does not
    for x in [R - 3.5, R + 3.5] {
        assert_eq!(h.drag(0, (x, CY), (0.0, 0.0)), Some(WindowCommand::Resize(id, ResizeEdge::East)), "x = {x}");
    }
    assert_eq!(h.drag(0, (R + 6.0, CY), (0.0, 0.0)), None);
    assert_eq!(h.drag(0, (R - 6.0, CY), (0.0, 0.0)), None);
}

#[test]
fn resizing_changes_the_size_in_the_dragged_direction() {
    let mut h = note();
    h.drag(0, (R, CY), (100.0, 0.0)); // east
    assert_eq!(h.note_size(0), (W + 100, H));
    h.drag(0, (250.0, B), (0.0, 60.0)); // south
    assert_eq!(h.note_size(0), (W + 100, H + 60));
    h.drag(0, (L, CY), (-50.0, 0.0)); // west: the left edge moves
    assert_eq!(h.note_size(0), (W + 150, H + 60));
    assert_eq!(h.window_position(0), (-50, 0));
    h.drag(0, (250.0, T), (0.0, -20.0)); // north
    assert_eq!(h.note_size(0), (W + 150, H + 80));
    assert_eq!(h.window_position(0), (-50, -20));
}

#[test]
fn resizing_below_the_minimum_stops_at_360_by_360_paper() {
    let mut h = note();
    // the window minimum is the paper minimum plus the margins on both sides
    h.drag(0, (R, B), (-500.0, -500.0)); // south-east, far past the minimum
    assert_eq!(h.note_size(0), (MIN, MIN));
    assert_eq!(h.window_position(0), (0, 0), "the top-left corner stays put");
}

#[test]
fn shrinking_from_the_west_keeps_the_right_edge() {
    let mut h = note();
    h.drag(0, (L, CY), (500.0, 0.0));
    assert_eq!(h.note_size(0).0, MIN);
    assert_eq!(h.window_position(0).0, W as i32 - MIN as i32, "the right edge stays where it was");
}

#[test]
fn there_is_no_maximum_size() {
    let mut h = note();
    h.drag(0, (R, B), (3000.0, 2000.0));
    assert_eq!(h.note_size(0), (W + 3000, H + 2000));
}

#[test]
fn right_click_on_the_tape_and_on_a_zone_still_opens_the_menu() {
    let mut h = note();
    h.click_right(0, TAPE.0, TAPE.1);
    assert_eq!(h.menus_shown(), 1);
    h.click_right(0, R, CY);
    assert_eq!(h.menus_shown(), 2);
}

#[test]
fn the_toolbar_stays_while_the_pointer_is_over_the_tape_or_a_zone() {
    let mut h = note();
    h.pointer_move(0, 250.0, 150.0);
    h.advance(ms(300));
    for p in [TAPE, (R, CY)] {
        h.pointer_move(0, p.0, p.1);
        h.advance(ms(300));
        assert_eq!(h.toolbar_opacity(0), 1.0, "at {p:?}");
    }
}

// ---- saving and restoring ----

#[test]
fn geometry_is_saved_after_its_own_quiet_period() {
    let mut h = note();
    let id = h.note_id(0);
    h.advance(ms(0));
    let before = h.saved_geometry(id);
    h.drag(0, TAPE, (40.0, 30.0));
    h.advance(ms(400));
    assert_eq!(h.saved_geometry(id), before, "400 ms after the move");
    h.advance(ms(100));
    let g = h.saved_geometry(id).unwrap();
    assert_eq!((g.x, g.y, g.w, g.h), (40, 30, W, H));
}

#[test]
fn every_move_restarts_the_geometry_debounce() {
    let mut h = note();
    let id = h.note_id(0);
    h.drag(0, TAPE, (10.0, 0.0));
    h.advance(ms(400));
    h.drag(0, TAPE, (10.0, 0.0));
    h.advance(ms(400));
    assert!(h.saved_geometry(id).is_none(), "not saved yet");
    h.advance(ms(100));
    assert_eq!(h.saved_geometry(id).unwrap().x, 20);
}

#[test]
fn geometry_and_text_debounces_are_independent() {
    let mut h = note();
    let id = h.note_id(0);
    h.drag(0, TAPE, (10.0, 10.0));
    h.advance(ms(300));
    h.type_text(0, "a"); // restarts the text debounce only
    h.advance(ms(200));
    assert_eq!(h.saved_geometry(id).unwrap().x, 10, "geometry saved 500 ms after the move");
    assert_eq!(h.saved_text(0), "", "text still waiting: 200 ms after the edit");
    h.advance(ms(300));
    assert_eq!(h.saved_text(0), "a");
}

#[test]
fn a_moved_and_resized_note_restarts_at_the_same_geometry() {
    let mut h = note();
    h.drag(0, TAPE, (120.0, 80.0));
    h.drag(0, (R, B), (50.0, 70.0));
    let (pos, size) = (h.window_position(0), h.note_size(0));
    assert_eq!((pos, size), ((120, 80), (W + 50, H + 70)));
    h.restart().unwrap();
    assert_eq!(h.window_position(0), pos);
    assert_eq!(h.note_size(0), size);
}

#[test]
fn geometry_survives_a_restart_even_before_the_debounce_fires() {
    let mut h = note();
    h.drag(0, TAPE, (15.0, 25.0));
    h.restart().unwrap(); // quitting saves at once
    assert_eq!(h.window_position(0), (15, 25));
}

#[test]
fn closed_note_reopens_at_its_moved_geometry() {
    let mut h = note();
    h.drag(0, TAPE, (200.0, 100.0));
    h.right_click(0, Some(&["New note"]));
    assert_eq!(h.open_note_count(), 2);
    h.close_note(0);
    h.right_click(0, Some(&["Open note", "(empty note)"]));
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.window_position(1), (200, 100));
}

#[test]
fn a_new_note_opens_offset_from_the_moved_one() {
    let mut h = note();
    h.drag(0, TAPE, (200.0, 100.0));
    h.right_click(0, Some(&["New note"]));
    assert_eq!(h.window_position(1), (230, 130));
}

// ---- off-screen recovery ----

const PRIMARY: ScreenRect = ScreenRect { x: 0, y: 0, w: 1920, h: 1040 };
const SECOND: ScreenRect = ScreenRect { x: 1920, y: 0, w: 1920, h: 1040 };

#[test]
fn a_note_on_a_monitor_that_is_gone_comes_back_on_the_primary_work_area() {
    let mut h = Harness::new();
    h.set_work_areas(vec![PRIMARY, SECOND]);
    h.start().unwrap();
    h.drag(0, TAPE, (2400.0, 100.0)); // onto the second monitor
    h.restart().unwrap();
    assert_eq!(h.window_position(0), (2400, 100), "the second monitor is still there");

    h.set_work_areas(vec![PRIMARY]); // unplugged
    h.restart().unwrap();
    let (x, y) = h.window_position(0);
    let (w, hh) = h.note_size(0);
    let paper = ScreenRect::new(x + 42, y + 42, w as i32 - 84, hh as i32 - 84);
    assert_eq!(PRIMARY.intersect(&paper), Some(paper), "paper is on the primary work area: {paper:?}");
    assert_eq!((w, hh), (W, H), "size is kept");
}

#[test]
fn the_recovered_position_is_what_gets_saved_next() {
    let mut h = Harness::new();
    h.set_work_areas(vec![PRIMARY, SECOND]);
    h.start().unwrap();
    h.drag(0, TAPE, (2400.0, 100.0));
    h.restart().unwrap();
    h.set_work_areas(vec![PRIMARY]);
    h.restart().unwrap();
    let pos = h.window_position(0);
    h.restart().unwrap();
    assert_eq!(h.window_position(0), pos, "stable after recovery");
}

#[test]
fn a_note_that_is_on_screen_is_left_alone() {
    let mut h = Harness::new();
    h.set_work_areas(vec![PRIMARY]);
    h.start().unwrap();
    h.drag(0, TAPE, (1700.0, 900.0)); // mostly off the corner but the top strip is on screen
    h.restart().unwrap();
    assert_eq!(h.window_position(0), (1700, 900));
}
