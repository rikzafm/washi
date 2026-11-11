//! Empirical Challenger Stress Test for Milestone 3 (Iteration 2 Verification):
//! Multi-cycle open/switch/dismiss/drag geometry persistence & debounce verification.

use snap::Harness;
use std::time::Duration;
use washi::storage::Geometry;

fn setup_two_notes() -> (Harness, i64, i64) {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, "# Host A\nBody of host");
    let g = h.note_geometry(0);
    h.place_note(0, Geometry { x: 500, y: 100, ..g });
    h.advance(Duration::from_millis(600));

    h.right_click(0, Some(&["New note"]));
    h.set_note_text(1, "# Note B\nBody of note B");
    h.place_note(1, Geometry { x: 1200, y: 400, ..g });
    h.advance(Duration::from_millis(600));

    let (host, other) = (h.note_id(0), h.note_id(1));
    (h, host, other)
}

#[test]
fn test_multi_cycle_park_dismiss_drag_geometry_persistence() {
    let (mut h, host, other) = setup_two_notes();

    // Perform 10 consecutive cycles:
    // Open All Notes on host A -> switch to note B (parking A) -> dismiss All Notes -> drag note A -> assert h.saved_geometry(A) updates
    for cycle in 1..=10 {
        // 1. Open All Notes on host A
        h.toggle_dropdown(0);
        h.invoke_dropdown_all_notes();
        h.advance(Duration::from_millis(200));
        assert!(h.is_all_notes_open(), "Cycle {cycle}: All Notes must be open");
        assert_eq!(h.all_notes_selected_id(), Some(host), "Cycle {cycle}: Host A selected initially");

        // 2. Switch to Note B -> parks Note A
        h.invoke_all_notes_select_note(other);
        h.advance(Duration::from_millis(100));
        assert_eq!(h.all_notes_selected_id(), Some(other), "Cycle {cycle}: Note B selected");

        // 3. Dismiss All Notes
        h.press_all_notes_escape();
        h.advance(Duration::from_millis(200));
        assert!(!h.is_all_notes_open(), "Cycle {cycle}: All Notes must be dismissed");

        // 4. Drag note A to new position
        let new_x = 100 + (cycle as i32 * 30);
        let new_y = 150 + (cycle as i32 * 20);
        let g = h.note_geometry(0);
        h.place_note(0, Geometry { x: new_x, y: new_y, ..g });

        // Debounce: before 500ms, database may not yet have new geometry
        h.advance(Duration::from_millis(600));

        // 5. Assert saved_geometry(host) updates properly every time
        let saved = h.saved_geometry(host).expect("saved geometry after move");
        assert_eq!(
            saved.x, new_x,
            "Cycle {cycle}: Host note X geometry must update in SQLite to {new_x}, got {}",
            saved.x
        );
        assert_eq!(
            saved.y, new_y,
            "Cycle {cycle}: Host note Y geometry must update in SQLite to {new_y}, got {}",
            saved.y
        );
    }
}

#[test]
fn test_alternating_host_notes_multi_cycle_geometry_persistence() {
    let (mut h, host_a, host_b) = setup_two_notes();

    for cycle in 1..=6 {
        let (active_idx, _target_idx, active_id, target_id) = if cycle % 2 == 1 {
            (0, 1, host_a, host_b)
        } else {
            (1, 0, host_b, host_a)
        };

        // Open All Notes on active host
        h.toggle_dropdown(active_idx);
        h.invoke_dropdown_all_notes();
        h.advance(Duration::from_millis(200));
        assert!(h.is_all_notes_open(), "Cycle {cycle}: All Notes open");
        assert_eq!(h.all_notes_selected_id(), Some(active_id));

        // Switch to target -> active note gets parked
        h.invoke_all_notes_select_note(target_id);
        h.advance(Duration::from_millis(100));
        assert_eq!(h.all_notes_selected_id(), Some(target_id));

        // Dismiss All Notes
        h.press_all_notes_escape();
        h.advance(Duration::from_millis(200));
        assert!(!h.is_all_notes_open());

        // Drag active note
        let target_x = 200 + (cycle as i32 * 40);
        let target_y = 100 + (cycle as i32 * 25);
        let g = h.note_geometry(active_idx);
        h.place_note(active_idx, Geometry { x: target_x, y: target_y, ..g });
        h.advance(Duration::from_millis(600));

        let saved = h.saved_geometry(active_id).expect("saved geometry after move");
        assert_eq!(
            saved.x, target_x,
            "Cycle {cycle}: Active note {active_id} X geometry failed to update: expected {target_x}, got {}",
            saved.x
        );
        assert_eq!(
            saved.y, target_y,
            "Cycle {cycle}: Active note {active_id} Y geometry failed to update: expected {target_y}, got {}",
            saved.y
        );
    }
}

#[test]
fn test_real_pointer_drag_persistence_after_park_and_dismiss() {
    let (mut h, host, other) = setup_two_notes();

    // 1. Open All Notes on host
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    h.advance(Duration::from_millis(200));

    // 2. Switch to other note -> host is parked
    h.invoke_all_notes_select_note(other);
    h.advance(Duration::from_millis(100));

    // 3. Dismiss All Notes
    h.press_all_notes_escape();
    h.advance(Duration::from_millis(200));
    assert!(!h.is_all_notes_open());

    let initial_geom = h.note_geometry(0);

    // 4. Drag note using window title drag (pointer drag)
    // Grab at (50.0, 10.0), drag by (120.0, 80.0)
    let grabbed = h.drag(0, (50.0, 10.0), (120.0, 80.0));
    assert!(grabbed.is_some(), "Drag should have grabbed the window title");
    h.advance(Duration::from_millis(600));

    let saved = h.saved_geometry(host).expect("saved geometry after pointer drag");
    let current_geom = h.note_geometry(0);

    assert_eq!(
        saved.x, current_geom.x,
        "Saved X must match dragged window X: saved={}, current={}",
        saved.x, current_geom.x
    );
    assert_eq!(
        saved.y, current_geom.y,
        "Saved Y must match dragged window Y: saved={}, current={}",
        saved.y, current_geom.y
    );
    assert_ne!(
        saved.x, initial_geom.x,
        "Saved X must have changed from initial position"
    );
}

#[test]
fn test_programmatic_autosave_debounce_timing_oracle() {
    let mut h = Harness::new();
    h.start().unwrap();
    // At creation, note 0 has empty content in DB
    assert_eq!(h.saved_text(0), "");

    // Open All Notes
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    h.advance(Duration::from_millis(200));
    assert!(h.is_all_notes_open());

    // Edit programmatically via set_all_notes_editor_text
    h.set_all_notes_editor_text("# Edited Content\nAutosave pending");

    // Before 500ms debounce (e.g. at 200ms), DB should NOT be updated yet
    h.advance(Duration::from_millis(200));
    assert_eq!(
        h.saved_text(0),
        "",
        "At 200ms (< 500ms debounce), DB content must NOT yet be updated!"
    );

    // Advance beyond 500ms debounce (another 350ms, total 550ms)
    h.advance(Duration::from_millis(350));
    assert_eq!(
        h.saved_text(0),
        "# Edited Content\nAutosave pending",
        "At 550ms (> 500ms debounce), DB content MUST be updated by autosave timer!"
    );
}
