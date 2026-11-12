//! Note lifecycle and the right-click menu, tested from the outside through the app harness.

use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Two notes: note 0 is the first, note 1 was made with the menu.
fn two_notes() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.new_note_from(0);
    assert_eq!(h.open_note_count(), 2);
    h
}

#[test]
fn right_click_anywhere_opens_the_menu() {
    let mut h = Harness::new();
    h.start().unwrap();
    // Over the margin and over the text area.
    for (x, y) in [(5.0, 5.0), (200.0, 200.0), (420.0, 440.0)] {
        h.click_right(0, x, y);
    }
    assert_eq!(h.menus_shown(), 3);
    let menu = h.last_menu();
    assert!(!menu.can_paste, "no clipboard content yet");
    assert!(!menu.can_paste_image, "no image on clipboard");
    assert!(!menu.has_selection, "nothing selected");
    assert!(!menu.can_undo && !menu.can_redo, "nothing typed yet");
    assert!(!menu.pinned);
    assert_eq!(menu.tape_style, 0);
    assert!(menu.closed.is_empty(), "no closed notes yet, so nothing to reopen");
}

#[test]
fn left_click_does_not_open_the_menu() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.right_click(0, None);
    assert_eq!(h.menus_shown(), 1);
    assert_eq!(h.open_note_count(), 1, "dismissing the menu changes nothing");
}

#[test]
fn new_note_from_the_menu_is_empty_and_the_old_note_is_kept() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "first");
    h.new_note_from(0);
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.note_text(0), "# first");
    assert_eq!(h.note_text(1), "");
    assert_eq!(h.note_size(1), h.note_size(0), "default size");
}

#[test]
fn ctrl_n_opens_a_new_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.press_ctrl(0, 'n');
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.note_text(0), "", "the shortcut must not type a letter");
}

#[test]
fn close_hides_the_note_without_asking() {
    let mut h = two_notes();
    h.dialogs.confirm_answer.set(false); // would veto if it were asked
    h.type_text(1, "scribble");
    h.close_note(1);
    assert!(!h.has_quit());
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.window_count(), 1, "the closed note's window is released");
}

#[test]
fn ctrl_w_closes_the_note() {
    let mut h = two_notes();
    h.press_ctrl(1, 'w');
    assert_eq!(h.open_note_count(), 1);
    assert!(!h.has_quit());
}

#[test]
fn closed_notes_are_listed_by_title_newest_first() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "oldest note");
    h.advance(ms(600));
    h.new_note_from(0);
    h.type_text(1, "the middle one");
    h.press_enter(1);
    h.type_text(1, "with a second line");    h.advance(ms(600));
    h.new_note_from(1);
    let long = "x".repeat(60);
    h.type_text(2, &long);
    h.advance(ms(600));
    h.new_note_from(2);
    h.advance(ms(600));
    // Close the first three; the fourth (empty) stays open.
    h.press_ctrl(0, 'w');
    h.press_ctrl(0, 'w');
    h.press_ctrl(0, 'w');
    assert_eq!(h.open_note_count(), 1);

    h.right_click(0, None);
    let titles = h.last_menu().closed;
    assert_eq!(titles.len(), 3);
    assert_eq!(titles[0], format!("{}…", "x".repeat(40)), "truncated to 40 characters");
    assert_eq!(titles[1], "the middle one", "first line only");
    assert_eq!(titles[2], "oldest note");
}

#[test]
fn choosing_a_closed_note_reopens_it_at_its_saved_size() {
    let mut h = two_notes();
    h.type_text(1, "come back to me");
    h.resize_note(1, 500, 470);
    h.close_note(1);
    assert_eq!(h.open_note_count(), 1);

    h.right_click(0, Some(&["Recently closed", "come back to me"]));
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.note_text(1), "# come back to me");
    assert_eq!(h.note_size(1), (500, 470));

    // It is open again, so it is no longer listed.
    h.right_click(0, None);
    assert!(h.last_menu().closed.is_empty());
}

#[test]
fn delete_asks_and_keeps_the_note_when_declined() {
    let mut h = two_notes();
    h.type_text(1, "keep me");
    h.delete_note(1);
    assert!(h.is_delete_dialog_open());
    h.invoke_keep_note();
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.note_text(1), "# keep me");
    h.press_ctrl(1, 'd');
    assert!(h.is_delete_dialog_open());
    h.press_delete_dialog_escape();
    assert_eq!(h.open_note_count(), 2, "Ctrl+D asks too; Escape keeps it");
}

#[test]
fn delete_removes_the_note_for_good_when_confirmed() {
    let mut h = two_notes();
    let id = h.note_id(1);
    h.type_text(1, "WASHI_CANARY_delete_me");
    h.advance(ms(500));
    h.delete_note(1);
    assert!(h.is_delete_dialog_open());
    h.invoke_confirm_delete();
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.window_count(), 1);
    assert!(h.is_deleted(id));

    // Not listed as a closed note either.
    h.right_click(0, None);
    assert!(h.last_menu().closed.is_empty());

    // The write-ahead log was emptied, so no trace of the note lingers in it.
    let (db, wal) = h.raw_db_bytes();
    assert!(wal.is_empty(), "checkpoint truncates the WAL");
    assert!(!contains(&db, b"WASHI_CANARY_delete_me"));

    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 1);
    assert!(h.is_deleted(id));
}

#[test]
fn ctrl_d_deletes_after_confirmation() {
    let mut h = two_notes();
    let id = h.note_id(1);
    h.press_ctrl(1, 'd');
    assert!(h.is_delete_dialog_open());
    h.invoke_confirm_delete();
    assert_eq!(h.open_note_count(), 1);
    assert!(h.is_deleted(id));
}

#[test]
fn closing_the_last_open_note_quits() {
    let mut h = two_notes();
    h.press_ctrl(1, 'w');
    assert!(!h.has_quit());
    h.press_ctrl(0, 'w');
    assert!(h.has_quit());
    assert_eq!(h.open_note_count(), 0);
}

#[test]
fn deleting_the_last_open_note_quits() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.press_ctrl(0, 'd');
    assert!(h.is_delete_dialog_open());
    h.invoke_confirm_delete();
    assert!(h.has_quit());
}

#[test]
fn restart_reopens_exactly_the_notes_that_were_open() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "alpha");
    h.new_note_from(0);
    h.type_text(1, "beta");
    h.new_note_from(1);
    h.type_text(2, "gamma");
    h.advance(ms(500));
    h.press_ctrl(1, 'w'); // close beta

    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.note_text(0), "# alpha");
    assert_eq!(h.note_text(1), "# gamma");
    h.right_click(0, None);
    assert_eq!(h.last_menu().closed, ["beta"]);
}

#[test]
fn start_after_everything_was_closed_gives_one_empty_note() {
    // When an empty note is closed causing quit, restarting opens a fresh blank note
    let mut h = Harness::new();
    h.start().unwrap();
    h.press_ctrl(0, 'w');
    assert!(h.has_quit());

    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.note_text(0), "", "restarting opens a fresh blank note");

    // When a non-empty note is closed and Washi quits, restarting should now restore that non-empty note
    h.type_text(0, "closed for the night");
    h.press_ctrl(0, 'w');
    assert!(h.has_quit());

    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.note_text(0), "# closed for the night", "restores the non-empty note");
}

#[test]
fn closing_multiple_notes_in_quick_succession_reopens_all_on_restart() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "first note");
    h.new_note_from(0);
    h.type_text(1, "second note");
    assert_eq!(h.open_note_count(), 2);

    let id0 = h.note_id(0);
    let id1 = h.note_id(1);

    h.close_note(1);
    h.advance(Duration::from_secs(5));
    h.close_note(0);
    assert!(h.has_quit());

    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 2);
    let ids: Vec<_> = (0..2).map(|i| h.note_id(i)).collect();
    assert!(ids.contains(&id0));
    assert!(ids.contains(&id1));
}

#[test]
fn closing_a_note_then_waiting_over_30s_keeps_it_closed_on_restart() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "note A");
    h.new_note_from(0);
    h.type_text(1, "note B");
    assert_eq!(h.open_note_count(), 2);

    let id_b = h.note_id(1);

    h.close_note(0);
    h.advance(Duration::from_secs(35));
    h.close_note(0); // note B is now at index 0
    assert!(h.has_quit());

    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.note_id(0), id_b);
    assert_eq!(h.note_text(0), "# note B");
    h.right_click(0, None);
    assert_eq!(h.last_menu().closed, ["note A"]);
}

#[test]
fn closing_a_note_then_deleting_the_last_note_restores_the_closed_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "note A");
    h.new_note_from(0);
    h.type_text(1, "note B");
    assert_eq!(h.open_note_count(), 2);

    let id_a = h.note_id(0);

    h.close_note(0);
    h.advance(Duration::from_secs(5));
    // Note B is now at index 0. Delete it.
    h.delete_note(0);
    assert!(h.is_delete_dialog_open());
    h.invoke_confirm_delete();
    assert!(h.has_quit());

    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.note_id(0), id_a);
    assert_eq!(h.note_text(0), "# note A");
}

#[test]
fn closing_empty_and_non_empty_notes_reopens_only_non_empty_notes_on_restart() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "not empty note");
    h.new_note_from(0);
    // Note 1 is empty
    assert_eq!(h.open_note_count(), 2);
    let id0 = h.note_id(0);

    h.close_note(1); // empty note closed
    h.advance(Duration::from_secs(5));
    h.close_note(0); // non-empty note closed, app quits
    assert!(h.has_quit());

    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.note_id(0), id0);
    assert_eq!(h.note_text(0), "# not empty note");
}

#[test]
fn closing_non_empty_then_empty_note_reopens_only_non_empty_notes_on_restart() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "not empty note");
    h.new_note_from(0);
    // Note 1 is empty
    assert_eq!(h.open_note_count(), 2);
    let id0 = h.note_id(0);

    h.close_note(0); // non-empty note closed
    h.advance(Duration::from_secs(5));
    h.close_note(0); // empty note closed (was index 1, now index 0), app quits
    assert!(h.has_quit());

    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.note_id(0), id0);
    assert_eq!(h.note_text(0), "# not empty note");
}

#[test]
fn exit_sequence_preserves_stacking_order_on_restart() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "bottom note");
    h.new_note_from(0);
    h.type_text(1, "middle note");
    h.new_note_from(0);
    h.type_text(2, "top note");
    let ids: Vec<_> = (0..3).map(|i| h.note_id(i)).collect();
    // stacking order bottom to top: ids[1], ids[0], ids[2]
    h.set_stacking(vec![ids[1], ids[0], ids[2]]);

    // Close in arbitrary order: top first, then middle, then bottom
    h.close_note(2);
    h.advance(Duration::from_secs(2));
    h.close_note(1);
    h.advance(Duration::from_secs(2));
    h.close_note(0);
    assert!(h.has_quit());

    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 3);
    let after: Vec<_> = (0..3).map(|i| h.note_id(i)).collect();
    assert_eq!(after, vec![ids[1], ids[0], ids[2]], "stacking order preserved on restart");
}

#[test]
fn session_end_with_pending_exit_candidates_restores_them_on_restart() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "note 0");
    h.new_note_from(0);
    h.type_text(1, "note 1");
    assert_eq!(h.open_note_count(), 2);

    let id0 = h.note_id(0);
    let id1 = h.note_id(1);

    // Close note 1
    h.close_note(1);
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.exit_candidates().len(), 1);

    // Windows session ends (e.g. shutdown)
    h.end_session();

    // Restart app
    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 2);
    let ids: Vec<_> = (0..2).map(|i| h.note_id(i)).collect();
    assert!(ids.contains(&id0));
    assert!(ids.contains(&id1));
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn consecutive_note_closes_within_30s_accumulate_in_exit_candidates() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "note 0");
    h.new_note_from(0);
    h.type_text(1, "note 1");
    h.new_note_from(1);
    h.type_text(2, "note 2");
    assert_eq!(h.open_note_count(), 3);

    let id2 = h.note_id(2);
    let id1 = h.note_id(1);

    // Close note 2
    h.close_note(2);
    assert_eq!(h.open_note_count(), 2);
    let candidates = h.exit_candidates();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, id2);
    assert!(!candidates[0].was_empty);
    assert!(candidates[0].geometry.is_some());

    // Advance 5 seconds (< 30s) and close note 1
    h.advance(Duration::from_secs(5));
    h.close_note(1);
    assert_eq!(h.open_note_count(), 1);
    let candidates = h.exit_candidates();
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].id, id2);
    assert_eq!(candidates[1].id, id1);
    assert!(!candidates[1].was_empty);
}

#[test]
fn exit_candidates_cleared_after_30s_inactivity() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "note 0");
    h.new_note_from(0);
    h.type_text(1, "note 1");
    h.new_note_from(1);
    h.type_text(2, "note 2");
    assert_eq!(h.open_note_count(), 3);

    let id2 = h.note_id(2);
    let id1 = h.note_id(1);

    // Close note 2
    h.close_note(2);
    assert_eq!(h.exit_candidates().len(), 1);
    assert_eq!(h.exit_candidates()[0].id, id2);

    // Advance 30 seconds: timer fires and clears candidates
    h.advance(Duration::from_secs(30));
    assert!(h.exit_candidates().is_empty());

    // Closing another note after the window starts a fresh sequence
    h.close_note(1);
    let candidates = h.exit_candidates();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, id1);
}

#[test]
fn reopening_note_removes_it_from_exit_candidates() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "note 0");
    h.new_note_from(0);
    h.type_text(1, "note 1");
    assert_eq!(h.open_note_count(), 2);

    let id1 = h.note_id(1);

    // Close note 1
    h.close_note(1);
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.exit_candidates().len(), 1);
    assert_eq!(h.exit_candidates()[0].id, id1);

    // Reopen note 1 via "Open note" menu
    h.right_click(0, Some(&["Recently closed", "note 1"]));
    assert_eq!(h.open_note_count(), 2);
    assert!(h.exit_candidates().is_empty());
}

#[test]
fn closing_empty_note_records_it_as_empty_candidate() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "not empty");
    h.new_note_from(0);
    assert_eq!(h.open_note_count(), 2);

    let id1 = h.note_id(1);

    // Note 1 is empty, closing it discards it from DB but records in exit_candidates
    h.close_note(1);
    assert_eq!(h.open_note_count(), 1);
    let candidates = h.exit_candidates();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, id1);
    assert!(candidates[0].was_empty);
}
