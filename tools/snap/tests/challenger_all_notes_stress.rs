//! Empirical Challenger Stress Tests for Milestone 3:
//! All Notes Seam Encapsulation (Features AN-01 to AN-07).

use snap::Harness;
use std::time::Duration;
use washi::storage::Geometry;

fn two_open_notes() -> (Harness, i64, i64) {
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
    (h, host, other)
}

#[test]
fn test_rapid_selection_and_switch_stress() {
    let mut h = Harness::new();
    h.start().unwrap();
    let g = h.note_geometry(0);
    for i in 1..5 {
        h.new_note_from(0);
        h.set_note_text(i, &format!("# Note {i}\nBody {i}"));
        h.place_note(i, Geometry { x: 600 + (i as i32 * 50), y: 100, ..g });
    }
    h.advance(Duration::from_millis(600));

    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    h.advance(Duration::from_millis(300));
    assert!(h.is_all_notes_open());

    // Rapidly switch between notes
    for _ in 0..3 {
        for i in 0..5 {
            let id = h.note_id(i);
            h.invoke_all_notes_select_note(id);
            h.advance(Duration::from_millis(10));
            assert_eq!(h.all_notes_selected_id(), Some(id));
        }
    }

    h.press_all_notes_escape();
    h.advance(Duration::from_millis(100));
    assert!(!h.is_all_notes_open());
}

#[test]
fn test_bidirectional_text_sync_and_autosave() {
    let (mut h, host, _) = two_open_notes();

    assert_eq!(h.all_notes_selected_id(), Some(host));
    assert_eq!(h.all_notes_editor_text(), Some("# Host\nbody".to_string()));

    // Edit from desktop
    h.set_note_text(0, "# Host Edited on Desktop\nDesktop updated");
    h.advance(Duration::from_millis(50));

    assert_eq!(
        h.all_notes_editor_text(),
        Some("# Host Edited on Desktop\nDesktop updated".to_string())
    );

    // Edit from All Notes editor
    h.set_all_notes_editor_text("# Host Edited in Sidebar\nSidebar updated");

    assert_eq!(
        h.note_text(0),
        "# Host Edited in Sidebar\nSidebar updated"
    );

    h.press_all_notes_escape();
    h.advance(Duration::from_millis(100));
    assert!(!h.is_all_notes_open());

    assert_eq!(
        h.saved_text(0),
        "# Host Edited in Sidebar\nSidebar updated"
    );
}

#[test]
fn test_delete_active_note_in_all_notes() {
    let (mut h, host, other) = two_open_notes();
    assert_eq!(h.all_notes_selected_id(), Some(host));

    h.invoke_all_notes_request_delete();
    assert_eq!(h.all_notes_confirming_delete(), Some(true));
    h.invoke_all_notes_confirm_delete();

    assert!(h.is_deleted(host));
    assert_eq!(h.all_notes_selected_id(), Some(other));
    assert_eq!(h.all_notes_open_count(), Some(1));

    h.press_all_notes_escape();
    h.advance(Duration::from_millis(100));
    assert!(!h.is_all_notes_open());
}

#[test]
fn test_repeated_open_dismiss_lifecycle() {
    let mut h = Harness::new();
    h.start().unwrap();
    let id = h.note_id(0);

    for i in 0..5 {
        h.toggle_dropdown(0);
        h.invoke_dropdown_all_notes();
        h.advance(Duration::from_millis(100));
        assert!(h.is_all_notes_open(), "cycle {i}: must be open");
        assert_eq!(h.all_notes_selected_id(), Some(id));

        h.press_all_notes_escape();
        h.advance(Duration::from_millis(100));
        assert!(!h.is_all_notes_open(), "cycle {i}: must be dismissed");
    }
}

#[test]
fn test_host_note_saves_geometry_after_parked_and_dismissed() {
    let (mut h, host, other) = two_open_notes();

    // 1. Initial geometry is (600, 100)
    let g = h.note_geometry(0);
    assert_eq!(g.x, 600);

    // 2. Select `other` note -> `host` is parked
    h.invoke_all_notes_select_note(other);
    h.advance(Duration::from_millis(100));
    assert_eq!(h.all_notes_selected_id(), Some(other));

    // 3. Dismiss All Notes with Escape
    h.press_all_notes_escape();
    h.advance(Duration::from_millis(200));
    assert!(!h.is_all_notes_open());

    // 4. Now move `host` note on desktop to a new position (200, 200)
    h.place_note(0, Geometry { x: 200, y: 200, ..g });
    h.advance(Duration::from_millis(600));

    // 5. The new geometry MUST be saved to database!
    let saved = h.saved_geometry(host).expect("saved geometry after move");
    assert_eq!(
        saved.x, 200,
        "Host note geometry must persist after moving, but saves_geometry was left false!"
    );
}
