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
    h.right_click(0, Some(&["New note"]));
    assert_eq!(h.open_note_count(), 2);
    h
}

fn labels(items: &[washi::platform::MenuItem]) -> Vec<String> {
    items.iter().map(|i| i.label.clone()).collect()
}

#[test]
fn right_click_anywhere_opens_the_menu() {
    let mut h = Harness::new();
    h.start().unwrap();
    // Over the margin and over the text area.
    for (x, y) in [(5.0, 5.0), (200.0, 200.0), (420.0, 440.0)] {
        h.click_right(0, x, y);
    }
    assert_eq!(h.menu.shown.borrow().len(), 3);
    let items = h.last_menu();
    assert_eq!(labels(&items), ["New note", "Open note", "Close", "Delete", "Always on top", "Start with Windows", "Export as Markdown\u{2026}"]);
    assert!(!items[1].enabled, "no closed notes yet, so nothing to open");
}

#[test]
fn left_click_does_not_open_the_menu() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.right_click(0, None);
    assert_eq!(h.menu.shown.borrow().len(), 1);
    assert_eq!(h.open_note_count(), 1, "dismissing the menu changes nothing");
}

#[test]
fn new_note_from_the_menu_is_empty_and_the_old_note_is_kept() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "first");
    h.right_click(0, Some(&["New note"]));
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
    h.right_click(1, Some(&["Close"]));
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
    h.right_click(0, Some(&["New note"]));
    h.type_text(1, "the middle one");
    h.press_enter(1);
    h.type_text(1, "with a second line");    h.advance(ms(600));
    h.right_click(1, Some(&["New note"]));
    let long = "x".repeat(60);
    h.type_text(2, &long);
    h.advance(ms(600));
    h.right_click(2, Some(&["New note"]));
    h.advance(ms(600));
    // Close the first three; the fourth (empty) stays open.
    h.press_ctrl(0, 'w');
    h.press_ctrl(0, 'w');
    h.press_ctrl(0, 'w');
    assert_eq!(h.open_note_count(), 1);

    h.right_click(0, None);
    let items = h.last_menu();
    let open = &items[1];
    assert!(open.enabled);
    let titles = labels(&open.children);
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
    h.right_click(1, Some(&["Close"]));
    assert_eq!(h.open_note_count(), 1);

    h.right_click(0, Some(&["Open note", "come back to me"]));
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.note_text(1), "# come back to me");
    assert_eq!(h.note_size(1), (500, 470));

    // It is open again, so it is no longer listed.
    h.right_click(0, None);
    assert!(!h.last_menu()[1].enabled);
}

#[test]
fn delete_asks_and_keeps_the_note_when_declined() {
    let mut h = two_notes();
    h.type_text(1, "keep me");
    h.dialogs.confirm_answer.set(false);
    h.right_click(1, Some(&["Delete"]));
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.note_text(1), "# keep me");
    h.press_ctrl(1, 'd');
    assert_eq!(h.open_note_count(), 2, "Ctrl+D asks too");
}

#[test]
fn delete_removes_the_note_for_good_when_confirmed() {
    let mut h = two_notes();
    let id = h.note_id(1);
    h.type_text(1, "WASHI_CANARY_delete_me");
    h.advance(ms(500));
    h.dialogs.confirm_answer.set(true);
    h.right_click(1, Some(&["Delete"]));
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.window_count(), 1);
    assert!(h.is_deleted(id));

    // Not listed as a closed note either.
    h.right_click(0, None);
    assert!(!h.last_menu()[1].enabled);

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
    h.dialogs.confirm_answer.set(true);
    h.press_ctrl(1, 'd');
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
    h.dialogs.confirm_answer.set(true);
    h.press_ctrl(0, 'd');
    assert!(h.has_quit());
}

#[test]
fn restart_reopens_exactly_the_notes_that_were_open() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "alpha");
    h.right_click(0, Some(&["New note"]));
    h.type_text(1, "beta");
    h.right_click(1, Some(&["New note"]));
    h.type_text(2, "gamma");
    h.advance(ms(500));
    h.press_ctrl(1, 'w'); // close beta

    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.note_text(0), "# alpha");
    assert_eq!(h.note_text(1), "# gamma");
    h.right_click(0, None);
    assert_eq!(labels(&h.last_menu()[1].children), ["beta"]);
}

#[test]
fn start_after_everything_was_closed_gives_one_empty_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "closed for the night");
    h.press_ctrl(0, 'w');
    assert!(h.has_quit());

    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.note_text(0), "", "a fresh empty note, not the closed one");
    h.right_click(0, None);
    assert_eq!(labels(&h.last_menu()[1].children), ["closed for the night"]);
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn consecutive_note_closes_within_30s_accumulate_in_exit_candidates() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "note 0");
    h.right_click(0, Some(&["New note"]));
    h.type_text(1, "note 1");
    h.right_click(1, Some(&["New note"]));
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
    h.right_click(0, Some(&["New note"]));
    h.type_text(1, "note 1");
    h.right_click(1, Some(&["New note"]));
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
    h.right_click(0, Some(&["New note"]));
    h.type_text(1, "note 1");
    assert_eq!(h.open_note_count(), 2);

    let id1 = h.note_id(1);

    // Close note 1
    h.close_note(1);
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.exit_candidates().len(), 1);
    assert_eq!(h.exit_candidates()[0].id, id1);

    // Reopen note 1 via "Open note" menu
    h.right_click(0, Some(&["Open note", "note 1"]));
    assert_eq!(h.open_note_count(), 2);
    assert!(h.exit_candidates().is_empty());
}

#[test]
fn closing_empty_note_records_it_as_empty_candidate() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "not empty");
    h.right_click(0, Some(&["New note"]));
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
