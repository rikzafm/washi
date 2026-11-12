//! Empirical Stress Test Suite for Milestone 3: All Notes Seam Encapsulation
//! Stress tests cascade parking and bi-directional note synchronization.

use snap::Harness;
use std::time::Duration;
use washi::all_notes::{park_beside, Side, PARK_CASCADE, PARK_GAP, SIDEBAR_WIDTH};
use washi::platform::ScreenRect;
use washi::storage::Geometry;

fn placed_behind(h: &Harness, id: i64) -> bool {
    h.control.log.borrow().contains(&snap::WindowCommand::PlaceBehind(id))
}

/// Helper: create harness with `count` open notes, positioned so they overlap with All Notes.
fn setup_harness_with_open_notes(count: usize) -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();

    // Note 0 is created at start
    h.set_note_text(0, "# Note 0\nInitial body 0");
    let g0 = Geometry { x: 318, y: 100, w: 447, h: 468 };
    h.place_note(0, g0);
    h.advance(Duration::from_millis(50));

    for i in 1..count {
        h.new_note_from(0);
        let idx = h.open_note_count() - 1;
        h.set_note_text(idx, &format!("# Note {i}\nInitial body {i}"));
        // Position them so they overlap with the All Notes area (editor at 318 - 282 = 36..765)
        // All start at y: 100 so cascade offset in y is purely PARK_CASCADE
        h.place_note(idx, Geometry {
            x: 350 + (i as i32 * 20),
            y: 100,
            w: 447,
            h: 468,
        });
        h.advance(Duration::from_millis(50));
    }
    assert_eq!(h.open_note_count(), count);
    h
}

// =========================================================================
// PART 1: CASCADE PARKING STRESS TESTS
// =========================================================================

#[test]
fn test_cascade_parking_multiple_notes_5_plus() {
    // 1. Create 7 open notes on desktop.
    let mut h = setup_harness_with_open_notes(7);
    let note_ids: Vec<i64> = (0..7).map(|i| h.note_id(i)).collect();
    let initial_geoms: Vec<Geometry> = (0..7).map(|i| h.note_geometry(i)).collect();

    // Open All Notes from Note 0
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());
    assert_eq!(h.all_notes_selected_id(), Some(note_ids[0]));
    h.advance(Duration::from_millis(100));

    // Initially, Note 0 is hosted in All Notes editor
    let (ax, _, aw, _) = h.all_notes_window_geometry().unwrap();

    // Sequentially switch to Note 1, Note 2, Note 3, Note 4, Note 5, Note 6
    for i in 1..7 {
        h.invoke_all_notes_select_note(note_ids[i]);
        assert_eq!(h.all_notes_selected_id(), Some(note_ids[i]));
        h.advance(Duration::from_millis(50));

        let prev_idx = (0..h.open_note_count()).find(|&k| h.note_id(k) == note_ids[i - 1]).unwrap();
        // Switched-away note must be visible and placed behind All Notes
        assert!(h.note_visible(prev_idx), "Switched-away note must be visible on desktop");
        assert!(placed_behind(&h, note_ids[i - 1]), "Switched-away note must be placed behind");
    }

    // Now Notes 0, 1, 2, 3, 4, 5 (6 notes total, which is > 5) are all parked!
    // Check their geometries on desktop:
    let mut parked_rights: Vec<(i64, Geometry)> = Vec::new();
    let all_notes_right_edge = ax + aw as i32 - 42;

    for i in 0..6 {
        let idx = (0..h.open_note_count()).find(|&k| h.note_id(k) == note_ids[i]).unwrap();
        let current_geom = h.note_geometry(idx);
        // If parked on right, its paper x + 42 must be to the right of All Notes right edge
        if current_geom.x + 42 > all_notes_right_edge {
            parked_rights.push((note_ids[i], current_geom));
        }
    }

    // Verify right cascade offset (PARK_CASCADE = 30px in x and y) between consecutive right-parked notes
    assert!(parked_rights.len() >= 2, "Expected multiple notes parked on right side");
    for window in parked_rights.windows(2) {
        let (prev_id, prev_geom) = window[0];
        let (next_id, next_geom) = window[1];
        assert_eq!(
            next_geom.x - prev_geom.x,
            PARK_CASCADE,
            "Right cascade x-step mismatch between note {prev_id} and {next_id}"
        );
        assert_eq!(
            next_geom.y - prev_geom.y,
            PARK_CASCADE,
            "Right cascade y-step mismatch between note {prev_id} and {next_id}"
        );
    }

    // Dismiss All Notes and verify notes return
    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open());

    // Verify non-host parked notes restored to initial geometries
    for i in 1..6 {
        let idx = (0..h.open_note_count()).find(|&k| h.note_id(k) == note_ids[i]).unwrap();
        assert_eq!(
            h.note_geometry(idx),
            initial_geoms[i],
            "Note {i} geometry was not restored after unparking"
        );
    }
}

#[test]
fn test_cascade_parking_right_overflow_to_left_overflow_to_fallback() {
    let screen = [ScreenRect::new(0, 0, 1920, 1080)];
    let note = Geometry { x: 800, y: 100, w: 447, h: 468 };
    // Editor placed near the right edge: paper from 1200 to 1647
    let editor = Geometry { x: 1200 - SIDEBAR_WIDTH, y: 100, w: 447 + SIDEBAR_WIDTH as u32, h: 468 };

    // 1. Right side cannot fit note (1200 - 282 + 729 - 42 + 12 = 1617 + 447 - 84 = 1980 > 1920)
    // So park_beside must attempt Left!
    let (left_pos_0, side_0) = park_beside(note, editor, [0, 0], &screen).expect("parks on left");
    assert_eq!(side_0, Side::Left, "Expected Left side parking when right overflows");

    // Left position must sit to the left of editor
    assert!(
        left_pos_0.x + left_pos_0.w as i32 - 42 <= editor.x - PARK_GAP,
        "Left parked paper must not overlap editor"
    );

    // 2. Cascade on Left until Left also overflows!
    // Each cascade step on left moves left by PARK_CASCADE (30) and down by 30
    let mut step = 0;
    let mut last_left = left_pos_0;
    loop {
        step += 1;
        match park_beside(note, editor, [0, step], &screen) {
            Some((pos, side)) => {
                assert_eq!(side, Side::Left);
                assert_eq!(pos.x, last_left.x - PARK_CASCADE);
                last_left = pos;
            }
            None => {
                // Left overflowed screen boundary (x < 0), successfully fell back to None (behind)!
                break;
            }
        }
        assert!(step < 100, "Left cascade infinite loop protection");
    }
    assert!(step > 0, "Left overflowed and returned None as expected");

    // 3. Narrow work area fallback:
    // Screen is 800x600, editor is 729 wide -> neither right nor left can fit 447px note
    let narrow_screen = [ScreenRect::new(0, 0, 800, 600)];
    let narrow_editor = Geometry { x: 20, y: 50, w: 729, h: 468 };
    let narrow_note = Geometry { x: 100, y: 50, w: 447, h: 468 };
    assert_eq!(
        park_beside(narrow_note, narrow_editor, [0, 0], &narrow_screen),
        None,
        "Narrow screen with no room on either side must return None"
    );
}

#[test]
fn test_cascade_parking_overlapping_vs_non_overlapping() {
    let screen = [ScreenRect::new(0, 0, 1920, 1080)];
    let editor = Geometry { x: 318, y: 100, w: 729, h: 468 };

    // Case A: Completely non-overlapping note (far away to the right)
    let non_overlapping = Geometry { x: 1400, y: 100, w: 447, h: 468 };
    assert_eq!(
        park_beside(non_overlapping, editor, [0, 0], &screen),
        None,
        "Non-overlapping note must not be moved beside"
    );

    // Case B: Non-overlapping note (far below)
    let below_non_overlapping = Geometry { x: 400, y: 700, w: 447, h: 468 };
    assert_eq!(
        park_beside(below_non_overlapping, editor, [0, 0], &screen),
        None,
        "Note below All Notes without paper overlap must not be moved beside"
    );

    // Case C: Overlapping note (shares paper box area)
    let overlapping = Geometry { x: 500, y: 200, w: 447, h: 468 };
    assert!(
        park_beside(overlapping, editor, [0, 0], &screen).is_some(),
        "Overlapping note must be parked beside"
    );
}

#[test]
fn test_cascade_parking_geometry_restoration_on_all_notes_close() {
    let mut h = setup_harness_with_open_notes(3);
    let _id0 = h.note_id(0);
    let id1 = h.note_id(1);
    let id2 = h.note_id(2);

    let home0 = h.note_geometry(0);
    let home1 = h.note_geometry(1);
    let home2 = h.note_geometry(2);

    // Open All Notes from Note 0
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());

    // Switch to Note 1: Note 0 parks
    h.invoke_all_notes_select_note(id1);
    h.advance(Duration::from_millis(50));

    // Switch to Note 2: Note 1 parks
    h.invoke_all_notes_select_note(id2);
    h.advance(Duration::from_millis(50));

    // Close All Notes
    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open());

    // Verify non-host parked note (Note 1) returned to exact home geometry
    let restored1 = h.note_geometry(1);
    assert_eq!(restored1, home1, "Parked note 1 must return to home geometry");

    let restored2 = h.note_geometry(2);
    assert_eq!(restored2, home2, "Note 2 must return to home geometry");

    // Verify host note 0 returns to paper geometry (where All Notes paper was)
    let restored0 = h.note_geometry(0);
    assert_eq!(restored0, home0, "Host note 0 must retain paper geometry");
}

#[test]
fn test_cascade_parking_manual_drag_preserves_new_position() {
    let mut h = setup_harness_with_open_notes(3);
    let id0 = h.note_id(0);
    let _id1 = h.note_id(1);

    // Open All Notes from Note 0
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();

    // Switch to Note 0: initial
    h.invoke_all_notes_select_note(id0);
    h.advance(Duration::from_millis(50));

    // Switch to Note 1 then back to Note 0: Note 1 (non-host note) is parked beside
    h.invoke_all_notes_select_note(h.note_id(1));
    h.advance(Duration::from_millis(50));
    h.invoke_all_notes_select_note(id0);
    h.advance(Duration::from_millis(50));

    let parked_geom1 = h.note_geometry(1);

    // Simulate user manually dragging the parked Note 1 while parked
    let dragged_pos1 = Geometry {
        x: parked_geom1.x + 80,
        y: parked_geom1.y + 60,
        ..parked_geom1
    };
    h.place_note(1, dragged_pos1);

    // Dismiss All Notes
    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open());

    // When unparked, user-dragged note 1 must NOT revert to home; it must stay at dragged position
    let final_pos1 = h.note_geometry(1);
    assert_eq!(
        (final_pos1.x, final_pos1.y),
        (dragged_pos1.x, dragged_pos1.y),
        "User drag of parked note must be preserved upon unparking"
    );
}

#[test]
fn test_multi_monitor_negative_coordinates_cascade() {
    // Secondary monitor to the left: [-1920, 0, 1920, 1080], Primary monitor: [0, 0, 1920, 1080]
    let screens = [
        ScreenRect::new(-1920, 0, 1920, 1080),
        ScreenRect::new(0, 0, 1920, 1080),
    ];

    // Case 1: Editor well inside left monitor at -1500 (plenty of room on right of it: -1500 + 729 = -771 < 0)
    let note1 = Geometry { x: -1400, y: 100, w: 447, h: 468 };
    let editor1 = Geometry { x: -1500, y: 100, w: 729, h: 468 };
    let (right, side1) = park_beside(note1, editor1, [0, 0], &screens)
        .expect("parks on right of left monitor");
    assert_eq!(side1, Side::Right);
    assert_eq!(right.x + 42, editor1.x + editor1.w as i32 - 42 + PARK_GAP);
    assert!(right.x >= -1920 && right.x < 0, "Right parked note remains within left monitor");

    // Case 2: Editor near boundary at -1000: right side would spill over into primary monitor (> 0),
    // so park_beside correctly falls back to Left!
    let note2 = Geometry { x: -800, y: 100, w: 447, h: 468 };
    let editor2 = Geometry { x: -1000, y: 100, w: 729, h: 468 };
    let (left, side2) = park_beside(note2, editor2, [0, 0], &screens)
        .expect("falls back to left on negative monitor when right crosses boundary");
    assert_eq!(side2, Side::Left);
    assert!(left.x >= -1920);
}

#[test]
fn test_cascade_math_high_iteration_bounds() {
    let screen = [ScreenRect::new(0, 0, 3840, 2160)];
    let note = Geometry { x: 500, y: 100, w: 447, h: 468 };
    let editor = Geometry { x: 300, y: 100, w: 729, h: 468 };

    // Test k up to 50 without arithmetic overflows or crashes
    for k in 0..50 {
        let res_right = park_beside(note, editor, [k, 0], &screen);
        if let Some((pos, side)) = res_right {
            assert_eq!(side, Side::Right);
            assert!(pos.x + pos.w as i32 <= 3840);
        }
    }
}

// =========================================================================
// PART 2: BI-DIRECTIONAL SYNCHRONIZATION STRESS TESTS
// =========================================================================

#[test]
fn test_bidirectional_sync_concurrent_rapid_edits() {
    let mut h = setup_harness_with_open_notes(2);
    let _id0 = h.note_id(0);
    let id1 = h.note_id(1);

    // Open All Notes from Note 0
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());

    // Select Note 1 in All Notes
    h.invoke_all_notes_select_note(id1);
    h.advance(Duration::from_millis(50));

    // Rapid alternating edits between All Notes editor and desktop note window:
    let edits = [
        ("editor", "# Edit 1 from All Notes\nAlpha"),
        ("desktop", "# Edit 2 from Desktop\nBeta"),
        ("editor", "# Edit 3 from All Notes\nGamma"),
        ("desktop", "# Edit 4 from Desktop\nDelta"),
        ("editor", "# Edit 5 from All Notes\nEpsilon"),
    ];

    for (source, content) in edits {
        if source == "editor" {
            h.set_all_notes_editor_text(content);
            // Verify desktop note receives update immediately
            let idx = (0..h.open_note_count()).find(|&i| h.note_id(i) == id1).unwrap();
            assert_eq!(h.note_text(idx), content);
            assert_eq!(h.all_notes_editor_text().unwrap(), content);
        } else {
            let idx = (0..h.open_note_count()).find(|&i| h.note_id(i) == id1).unwrap();
            h.set_note_text(idx, content);
            // Verify All Notes editor receives update immediately
            assert_eq!(h.all_notes_editor_text().unwrap(), content);
        }

        // Check sidebar row title updates
        let open_items = h.all_notes_open_items();
        let item = open_items.iter().find(|i| i.id == id1 as i32).expect("item in list");
        let first_line = content.lines().next().unwrap().trim_start_matches("# ");
        assert_eq!(item.title.as_str(), first_line);
    }

    // Advance clock past autosave debounce (500ms)
    h.advance(Duration::from_millis(600));

    // Verify All Notes editor has the latest text
    assert_eq!(h.all_notes_editor_text().unwrap(), "# Edit 5 from All Notes\nEpsilon");
}

#[test]
fn test_bidirectional_sync_delete_desktop_vs_sidebar() {
    let mut h = setup_harness_with_open_notes(3);
    let id0 = h.note_id(0);
    let id1 = h.note_id(1);
    let id2 = h.note_id(2);

    // Open All Notes
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());

    // Currently viewing Note 0. Now select Note 1 in All Notes
    h.invoke_all_notes_select_note(id1);
    assert_eq!(h.all_notes_selected_id(), Some(id1));

    // --- CASE 1: Delete note from Desktop while All Notes is open ---
    // Note 2 is deleted from desktop
    let idx2 = (0..h.open_note_count()).find(|&i| h.note_id(i) == id2).unwrap();
    h.delete_note(idx2);
    h.advance(Duration::from_millis(100));
    assert!(h.is_delete_dialog_open());
    h.invoke_confirm_delete();
    h.advance(Duration::from_millis(100));

    // Verify Note 2 is gone from open notes
    assert_eq!(h.open_note_count(), 2);
    // Verify All Notes list updated and Note 2 is no longer in open items
    let open_items = h.all_notes_open_items();
    assert!(!open_items.iter().any(|i| i.id == id2 as i32));
    // Selection should remain on Note 1
    assert_eq!(h.all_notes_selected_id(), Some(id1));

    // --- CASE 2: Delete active note via All Notes sidebar ---
    // Note 1 is selected in All Notes. Request delete
    h.invoke_all_notes_request_delete();
    assert_eq!(h.all_notes_confirming_delete(), Some(true));

    // Cancel delete first
    h.invoke_all_notes_cancel_delete();
    assert_eq!(h.all_notes_confirming_delete(), Some(false));
    assert_eq!(h.all_notes_selected_id(), Some(id1));

    // Request delete again and confirm
    h.invoke_all_notes_request_delete();
    assert_eq!(h.all_notes_confirming_delete(), Some(true));
    h.invoke_all_notes_confirm_delete();
    h.advance(Duration::from_millis(100));

    // Note 1 must be deleted
    assert_eq!(h.open_note_count(), 1);
    let open_items_after = h.all_notes_open_items();
    assert_eq!(open_items_after.len(), 1);
    assert_eq!(open_items_after[0].id, id0 as i32);

    // All Notes should automatically switch selection to the remaining note (Note 0)
    assert_eq!(h.all_notes_selected_id(), Some(id0));
    assert_eq!(h.all_notes_editor_note_id(), Some(id0));
    assert!(h.all_notes_editor_text().unwrap().contains("Note 0"));
}

#[test]
fn test_bidirectional_sync_external_changes_while_browsing_other_notes() {
    let mut h = setup_harness_with_open_notes(3);
    let id0 = h.note_id(0);
    let id1 = h.note_id(1);
    let id2 = h.note_id(2);

    // Open All Notes viewing Note 0
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert_eq!(h.all_notes_selected_id(), Some(id0));

    // Modify Note 1 and Note 2 on desktop while All Notes is focused on Note 0
    let idx1 = (0..h.open_note_count()).find(|&i| h.note_id(i) == id1).unwrap();
    let idx2 = (0..h.open_note_count()).find(|&i| h.note_id(i) == id2).unwrap();

    h.set_note_text(idx1, "# Updated Note 1 Externally\nContent 1");
    h.set_note_text(idx2, "# Updated Note 2 Externally\nContent 2");
    h.advance(Duration::from_millis(50));

    // Verify Note 0 in editor is untouched
    assert!(h.all_notes_editor_text().unwrap().contains("Note 0"));

    // Verify sidebar item titles for Note 1 and 2 updated
    let items = h.all_notes_open_items();
    let item1 = items.iter().find(|i| i.id == id1 as i32).unwrap();
    let item2 = items.iter().find(|i| i.id == id2 as i32).unwrap();
    assert_eq!(item1.title.as_str(), "Updated Note 1 Externally");
    assert_eq!(item2.title.as_str(), "Updated Note 2 Externally");

    // Switch to Note 1: verify it displays the updated content
    h.invoke_all_notes_select_note(id1);
    h.advance(Duration::from_millis(50));
    assert_eq!(h.all_notes_editor_note_id(), Some(id1));
    assert_eq!(h.all_notes_editor_text().unwrap(), "# Updated Note 1 Externally\nContent 1");

    // Switch to Note 2: verify it displays the updated content
    h.invoke_all_notes_select_note(id2);
    h.advance(Duration::from_millis(50));
    assert_eq!(h.all_notes_editor_note_id(), Some(id2));
    assert_eq!(h.all_notes_editor_text().unwrap(), "# Updated Note 2 Externally\nContent 2");
}

#[test]
fn test_all_notes_note_close_while_parked_geometry_integrity() {
    let mut h = setup_harness_with_open_notes(3);
    let id0 = h.note_id(0);
    let id1 = h.note_id(1);

    // Open All Notes from Note 0
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();

    // Switch to Note 1: Note 0 is parked
    h.invoke_all_notes_select_note(id1);
    h.advance(Duration::from_millis(50));

    // Switch back to Note 0: Note 1 is parked
    h.invoke_all_notes_select_note(id0);
    h.advance(Duration::from_millis(50));

    // Now close Note 1 from All Notes sidebar (`invoke_all_notes_close_note`)
    // First select Note 1
    h.invoke_all_notes_select_note(id1);
    h.invoke_all_notes_close_note();
    h.advance(Duration::from_millis(50));

    // Note 1 is now closed
    let closed_items = h.all_notes_closed_items();
    assert!(closed_items.iter().any(|i| i.id == id1 as i32), "Note 1 should be in closed list");

    // Reopen Note 1 via double click on closed row (`invoke_open_note`)
    h.invoke_all_notes_open_or_focus();
    h.advance(Duration::from_millis(50));

    // Verify Note 1 is open again
    let open_items = h.all_notes_open_items();
    assert!(open_items.iter().any(|i| i.id == id1 as i32), "Note 1 reopened");

    // Close All Notes
    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open());

    // Verify Note 1 desktop window geometry is on screen and valid
    let idx1 = (0..h.open_note_count()).find(|&i| h.note_id(i) == id1).unwrap();
    let current_geom = h.note_geometry(idx1);
    assert!(current_geom.w >= 300 && current_geom.h >= 200, "Valid geometry dimensions");
}

#[test]
fn test_rapid_alternating_note_switches_stress() {
    let mut h = setup_harness_with_open_notes(5);
    let note_ids: Vec<i64> = (0..5).map(|i| h.note_id(i)).collect();

    // Open All Notes from Note 0
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());

    // Switch rapidly 50 times across all notes
    for _cycle in 0..10 {
        for &id in &note_ids {
            h.invoke_all_notes_select_note(id);
            assert_eq!(h.all_notes_selected_id(), Some(id));
            assert_eq!(h.all_notes_editor_note_id(), Some(id));
        }
    }

    // After 50 switches, verify editor text is correct and window count is stable
    h.invoke_all_notes_select_note(note_ids[2]);
    assert_eq!(h.all_notes_selected_id(), Some(note_ids[2]));
    assert!(h.all_notes_editor_text().unwrap().contains("Note 2"));

    // Close All Notes
    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open());
}

// =========================================================================
// PART 3: REPRODUCED BUG TEST CASES
// =========================================================================

/// EMPIRICAL BUG 1:
/// When the host note of All Notes is parked (switched away from), `n.set_saves_geometry(false)`
/// is called. When All Notes is dismissed via `dismiss_all_notes`, `unpark_notes` restores all
/// other notes with `set_saves_geometry(true)` but explicitly skips `host_note_id` (`if id != self.host_note_id`).
/// `App::dismiss_all_notes` then calls `n.place(paper)` on host, but `place()` does NOT restore `saves_geometry(true)`.
/// As a result, the host note's `saves_geometry` flag remains `false` forever, preventing any subsequent window
/// moves or resizes from ever being saved to the database!
#[test]
fn test_reproduce_host_geometry_save_bug() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, "# Host\nbody");
    let g = h.note_geometry(0);
    h.place_note(0, Geometry { x: 600, y: 100, ..g });
    h.new_note_from(0);
    h.set_note_text(1, "# Other\nbody");
    h.place_note(1, Geometry { x: 1400, y: 500, ..g });
    let (host, other) = (h.note_id(0), h.note_id(1));
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    h.advance(Duration::from_millis(300));

    // Select other note -> host note is parked, setting saves_geometry(false)
    h.invoke_all_notes_select_note(other);
    h.advance(Duration::from_millis(100));
    assert_eq!(h.all_notes_selected_id(), Some(other));

    // Dismiss All Notes
    h.press_all_notes_escape();
    h.advance(Duration::from_millis(200));
    assert!(!h.is_all_notes_open());

    // Move host note to (200, 200)
    h.place_note(0, Geometry { x: 200, y: 200, ..g });
    h.advance(Duration::from_millis(600));

    let saved = h.saved_geometry(host).expect("saved geometry after move");
    assert_eq!(saved.x, 200, "Host note geometry must persist after moving");
}

/// EMPIRICAL BUG 2:
/// `AllNotesView::set_editor_text` sets `self.autosave_dirty.set(true)`, but never starts `self.autosave_timer`.
/// `delegate.set_desktop_note_text` similarly marks the desktop note dirty but does not start its autosave timer.
/// As a result, text edited via `set_editor_text` is never flushed to the database after the 500ms debounce delay
/// unless All Notes is explicitly dismissed or switched.
#[test]
fn test_empirical_bug_set_editor_text_autosave_timer_not_started() {
    let mut h = setup_harness_with_open_notes(2);

    // Open All Notes
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());

    // Call set_all_notes_editor_text
    h.set_all_notes_editor_text("# New Content Via Sidebar\nShould autosave to DB");
    h.advance(Duration::from_millis(600)); // debounce autosave (500ms)

    let db_saved = h.saved_text(0);
    assert_eq!(
        db_saved, "# New Content Via Sidebar\nShould autosave to DB",
        "BUG CONFIRMED: set_editor_text sets autosave_dirty but does not start autosave_timer, so text is not persisted to DB!"
    );
}
