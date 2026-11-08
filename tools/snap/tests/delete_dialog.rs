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

    h.right_click(0, Some(&["Delete"]));
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
    h.right_click(0, Some(&["New note"]));
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
    h.right_click(0, Some(&["New note"]));
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

