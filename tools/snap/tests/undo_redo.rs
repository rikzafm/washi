//! Ticket 12: Ctrl+Z and Ctrl+Y undo and redo a note's text and structure, per note. Typing is
//! snapshotted after a 500 ms pause, structural changes at once. Real key events, fake clock.

use slint::platform::Key;
use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn started() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h
}

/// A note holding `text`, the caret in its title field.
fn note(text: &str) -> Harness {
    let mut h = started();
    h.set_note_text(0, text);
    let _ = h.render(0);
    h.advance(ms(5));
    h
}

#[test]
fn typing_merging_two_paragraphs_then_undo_splits_them_again() {
    let mut h = started();
    h.type_text(0, "title");
    h.press_enter(0);
    h.type_text(0, "ab");
    h.press_enter(0);
    h.type_text(0, "cd");
    h.press(0, Key::Home);
    h.press_backspace(0);
    assert_eq!(h.note_text(0), "# title\nabcd");
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), "# title\nab\ncd");
    h.press_ctrl(0, 'y');
    assert_eq!(h.note_text(0), "# title\nabcd");
}

#[test]
fn undo_is_in_steps_of_typing_bursts_separated_by_a_pause() {
    let mut h = started();
    h.type_text(0, "one");
    h.advance(ms(500));
    h.type_text(0, " two");
    h.advance(ms(499)); // not paused long enough yet: still the same burst
    h.type_text(0, "!");
    assert_eq!(h.note_text(0), "# one two!");
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), "# one");
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), "");
    h.press_ctrl(0, 'z'); // nothing left to undo
    assert_eq!(h.note_text(0), "");
    h.press_ctrl(0, 'y');
    h.press_ctrl(0, 'y');
    assert_eq!(h.note_text(0), "# one two!");
}

#[test]
fn a_new_edit_after_an_undo_clears_redo() {
    let mut h = started();
    h.type_text(0, "one");
    h.advance(ms(500));
    h.type_text(0, " two");
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), "# one");
    h.type_text(0, "X");
    h.press_ctrl(0, 'y');
    assert_eq!(h.note_text(0), "# oneX", "redo does nothing after a new edit");
}

#[test]
fn undone_and_redone_states_are_autosaved() {
    let mut h = started();
    h.type_text(0, "one");
    h.press_enter(0);
    h.type_text(0, "two");
    h.advance(ms(500));
    assert_eq!(h.saved_text(0), "# one\ntwo");
    h.press_ctrl(0, 'z');
    h.advance(ms(500));
    assert_eq!(h.saved_text(0), "# one\n");
    h.press_ctrl(0, 'y');
    assert_eq!(h.saved_text(0), "# one\n", "not yet: the save waits for the quiet period");
    h.advance(ms(500));
    assert_eq!(h.saved_text(0), "# one\ntwo");
}

#[test]
fn history_holds_fifty_steps_and_the_oldest_are_dropped() {
    let mut h = started();
    for _ in 0..60 {
        h.type_text(0, "x");
        h.advance(ms(500));
    }
    for _ in 0..80 {
        h.press_ctrl(0, 'z');
    }
    assert_eq!(h.note_text(0), format!("# {}", "x".repeat(10)));
}

#[test]
fn each_note_has_its_own_history() {
    let mut h = started();
    h.type_text(0, "first");
    h.advance(ms(500));
    h.press_ctrl(0, 'n');
    h.advance(ms(5));
    assert_eq!(h.window_count(), 2);
    h.press_ctrl(1, 'z'); // nothing to undo in the new note
    assert_eq!(h.note_text(0), "# first");
    h.type_text(1, "second");
    h.press_ctrl(1, 'z');
    assert_eq!(h.note_text(1), "");
    assert_eq!(h.note_text(0), "# first");
}

/// The smallest note, holding `text`, with the caret at byte `offset` of `block`.
fn smallest_note(text: &str, block: usize, offset: usize) -> Harness {
    let mut h = note(text);
    h.resize_note(0, 300, 300);
    h.place_caret_at(0, block, offset);
    h.advance(ms(5));
    h
}

#[test]
fn undo_and_redo_of_a_tick_leave_the_caret_where_it_was() {
    let mut h = smallest_note("# Jadwal\n- [ ] Selasa\nsome words to type in", 2, 4);
    let (at, scroll) = (h.selection(0), h.scroll_offset(0));
    assert_eq!(at, ((2, 4), (2, 4)));
    let (x, y) = h.check_box_center(0, 0);
    h.click(0, x, y);
    assert_eq!(h.note_text(0), "# Jadwal\n- [x] Selasa\nsome words to type in", "the click ticks the box");
    assert_eq!(h.selection(0), at, "a tick does not move the caret");
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), "# Jadwal\n- [ ] Selasa\nsome words to type in");
    assert_eq!(h.selection(0), at, "undo of a tick leaves the caret");
    assert_eq!(h.scroll_offset(0), scroll);
    h.press_ctrl(0, 'y');
    assert_eq!(h.note_text(0), "# Jadwal\n- [x] Selasa\nsome words to type in");
    assert_eq!(h.selection(0), at, "redo of a tick leaves the caret");
    assert_eq!(h.scroll_offset(0), scroll);
    h.type_text(0, "X");
    assert_eq!(h.note_text(0), "# Jadwal\n- [x] Selasa\nsomeX words to type in", "typing still lands at the caret");
}

#[test]
fn undo_and_redo_of_a_check_toggle_keep_the_caret_in_its_paragraph() {
    let mut h = smallest_note("# t\nbuy milk now", 1, 4);
    h.press_ctrl_shift(0, 'c');
    assert_eq!(h.note_text(0), "# t\n- [ ] buy milk now");
    assert_eq!(h.selection(0), ((1, 4), (1, 4)));
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), "# t\nbuy milk now");
    assert_eq!(h.selection(0), ((1, 4), (1, 4)), "undo of a toggle keeps the caret's place");
    h.press_ctrl(0, 'y');
    assert_eq!(h.note_text(0), "# t\n- [ ] buy milk now");
    assert_eq!(h.selection(0), ((1, 4), (1, 4)), "redo of a toggle keeps the caret's place");
    h.type_text(0, "X");
    assert_eq!(h.note_text(0), "# t\n- [ ] buy Xmilk now", "typing still lands at the caret");
}
