//! Tests for dropdown menu actions and state synchronization through the app harness.

use snap::Harness;

#[test]
fn dropdown_pin_toggle_reflects_and_updates_note_always_on_top() {
    let mut h = Harness::new();
    h.start().unwrap();

    // Note 0 starts unpinned
    assert!(!h.always_on_top(0));

    // Open dropdown for note 0
    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());
    assert_eq!(h.dropdown_pinned(), Some(false));

    // Toggle pin from dropdown
    h.invoke_dropdown_toggle_pin();
    assert!(h.always_on_top(0));
    assert_eq!(h.dropdown_pinned(), Some(true));

    // Toggle pin again
    h.invoke_dropdown_toggle_pin();
    assert!(!h.always_on_top(0));
    assert_eq!(h.dropdown_pinned(), Some(false));
}

#[test]
fn dropdown_delete_note_dismisses_and_deletes_current_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.right_click(0, Some(&["New note"]));
    assert_eq!(h.open_note_count(), 2);

    // Open dropdown on note 1
    h.toggle_dropdown(1);
    assert!(h.is_dropdown_open());
    assert_eq!(h.dropdown_note_id(), Some(h.note_id(1)));

    // Confirm deletion dialog
    h.dialogs.confirm_answer.set(true);

    // Delete note from dropdown
    h.invoke_dropdown_delete_note();

    // Dropdown should be dismissed and note 1 deleted
    assert!(!h.is_dropdown_open());
    assert_eq!(h.open_note_count(), 1);
}

#[test]
fn dropdown_new_note_dismisses_and_creates_new_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    assert_eq!(h.open_note_count(), 1);

    // Open dropdown on note 0
    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());

    // Click new note in dropdown
    h.invoke_dropdown_new_note();

    // Dropdown should be dismissed and a new note opened
    assert!(!h.is_dropdown_open());
    assert_eq!(h.open_note_count(), 2);
}
