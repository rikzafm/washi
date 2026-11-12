//! Tests for the themed delete note dialog.

use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn delete_dialog_opens_centered_over_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    let id = h.note_id(0);

    assert!(!h.is_delete_dialog_open());

    // Press Ctrl+D to delete note
    h.press_ctrl(0, 'd');

    assert!(h.is_delete_dialog_open());
    assert_eq!(h.delete_dialog_note_id(), Some(id));
}

#[test]
fn delete_dialog_escape_cancels_and_keeps_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    let id = h.note_id(0);
    h.type_text(0, "save this text");

    h.press_ctrl(0, 'd');
    assert!(h.is_delete_dialog_open());

    // Press Escape on delete dialog
    h.press_delete_dialog_escape();

    assert!(!h.is_delete_dialog_open());
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.note_text(0), "# save this text");
    assert!(!h.is_deleted(id));
}

#[test]
fn delete_dialog_keep_it_button_cancels_and_keeps_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    let id = h.note_id(0);
    h.type_text(0, "important thought");

    h.delete_note(0);
    assert!(h.is_delete_dialog_open());

    // Invoke "Keep it"
    h.invoke_keep_note();

    assert!(!h.is_delete_dialog_open());
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.note_text(0), "# important thought");
    assert!(!h.is_deleted(id));
}

#[test]
fn delete_dialog_confirm_button_permanently_deletes_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.new_note_from(0);
    assert_eq!(h.open_note_count(), 2);

    let id1 = h.note_id(1);
    h.type_text(1, "delete this one");
    h.advance(ms(500));

    h.press_ctrl(1, 'd');
    assert!(h.is_delete_dialog_open());
    assert_eq!(h.delete_dialog_note_id(), Some(id1));

    // Confirm deletion
    h.invoke_confirm_delete();

    assert!(!h.is_delete_dialog_open());
    assert_eq!(h.open_note_count(), 1);
    assert!(h.is_deleted(id1));
}

#[test]
fn closing_note_dismisses_its_delete_dialog() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.new_note_from(0);
    assert_eq!(h.open_note_count(), 2);

    let id1 = h.note_id(1);
    h.type_text(1, "note to close");

    // Open delete dialog on note 1
    h.press_ctrl(1, 'd');
    assert!(h.is_delete_dialog_open());
    assert_eq!(h.delete_dialog_note_id(), Some(id1));

    // Close note 1
    h.close_note(1);

    // Dialog should be dismissed
    assert!(!h.is_delete_dialog_open());
}

#[test]
fn triggering_delete_again_does_not_open_duplicate_dialog() {
    let mut h = Harness::new();
    h.start().unwrap();
    let id = h.note_id(0);

    h.press_ctrl(0, 'd');
    assert!(h.is_delete_dialog_open());

    // Trigger delete again while open
    h.press_ctrl(0, 'd');
    assert!(h.is_delete_dialog_open());
    assert_eq!(h.delete_dialog_note_id(), Some(id));

    // Single dismissal closes it
    h.invoke_keep_note();
    assert!(!h.is_delete_dialog_open());
}

#[test]
fn delete_dialog_centers_over_note_on_secondary_monitor() {
    use washi::platform::ScreenRect;
    let mut h = Harness::new();
    h.set_work_areas(vec![
        ScreenRect::new(0, 0, 1920, 1080),       // Top-left (primary)
        ScreenRect::new(1920, 0, 1920, 1080),    // Top-right
        ScreenRect::new(0, 1080, 1920, 1080),    // Bottom-left
        ScreenRect::new(1920, 1080, 1920, 1080), // Bottom-right
    ]);
    h.start().unwrap();

    // Place the note on the bottom-right monitor (drag by 2400, 1400 from pink tape)
    h.drag(0, (80.0, 48.0), (2400.0, 1400.0));

    h.press_ctrl(0, 'd');
    assert!(h.is_delete_dialog_open());

    let (dx, dy) = h.delete_dialog_position().expect("delete dialog position");
    // Dialog should be centered over the note on the bottom-right monitor (around 2400 + (447 - 392)/2 = 2427)
    // NOT clamped to the primary monitor (which would be <= 1920 - 392 = 1528)
    assert!(dx >= 1920.0, "dialog x ({dx}) must be on the secondary monitor (>= 1920), not clamped to primary monitor");
    assert!(dy >= 1080.0, "dialog y ({dy}) must be on the bottom monitor (>= 1080), not clamped to primary monitor");
    assert!((dx - (2400.0 + (447.0 - 392.0) / 2.0)).abs() <= 2.0, "dx ({dx}) centered over note x (2400)");
}



