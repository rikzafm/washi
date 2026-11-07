//! What is saved when: session end, focus loss, and the stacking order of the notes.

use snap::Harness;

#[test]
fn session_end_saves_pending_edits_of_every_note_at_once() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "first");
    h.right_click(0, Some(&["New note"]));
    h.type_text(1, "second");
    assert_eq!(h.saved_text(0), "", "still inside the autosave delay");
    h.end_session(); // WM_QUERYENDSESSION / WM_ENDSESSION: no time to wait for the debounce
    assert_eq!(h.saved_text(0), "# first");
    assert_eq!(h.saved_text(1), "# second");
}

#[test]
fn a_note_losing_focus_saves_its_pending_edits() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "scribble");
    assert_eq!(h.saved_text(0), "");
    h.switch_away();
    assert_eq!(h.saved_text(0), "# scribble");
}

#[test]
fn note_shortcuts_also_work_when_the_layout_sends_control_codes() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "word");
    h.press_ctrl_code(0, 'b');
    assert!(h.toolbar_active(0)[0], "Ctrl+B made the paragraph bold");
    h.press_ctrl_code(0, 'n');
    assert_eq!(h.open_note_count(), 2, "Ctrl+N opened a note");
    h.press_ctrl_code(1, 'w');
    assert_eq!(h.open_note_count(), 1, "Ctrl+W closed it");
}

#[test]
fn a_failing_random_generator_is_a_start_error_not_a_crash() {
    let mut h = Harness::with_protector(std::rc::Rc::new(snap::fakes::FakeProtector::with_broken_rng()));
    let err = h.start().expect_err("no key can be made");
    assert!(matches!(err, washi::StartError::Key(_)), "{err}");
    assert!(err.to_string().contains("encryption key"), "{err}");
}

#[test]
fn the_stacking_order_of_open_notes_survives_a_restart() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.right_click(0, Some(&["New note"]));
    h.right_click(0, Some(&["New note"]));
    let ids: Vec<_> = (0..3).map(|i| h.note_id(i)).collect();
    // the user brought the first note to the front, then the third: bottom to top is 1, 0, 2
    h.set_stacking(vec![ids[1], ids[0], ids[2]]);
    h.restart().unwrap();
    let after: Vec<_> = (0..3).map(|i| h.note_id(i)).collect();
    assert_eq!(after, [ids[1], ids[0], ids[2]], "windows are opened bottom first, so the top one ends on top");
}
