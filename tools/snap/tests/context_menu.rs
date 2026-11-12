//! The right-click menu (the Claude Design "Right Click Menu"): edit buttons, Pin on top, Tape color
//! per note, and Recently closed. Real right clicks open the menu window; its buttons are pressed
//! through the window's callbacks.

use snap::{Harness, TAPE_COLORS};
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn started() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h
}

#[test]
fn a_right_click_opens_one_menu_and_a_second_one_replaces_it() {
    let mut h = started();
    h.right_click(0, None);
    assert!(h.menu_open());
    assert_eq!(h.window_count(), 2, "the note and the menu");
    h.right_click(0, None);
    assert!(h.menu_open());
    assert_eq!(h.window_count(), 2, "still one menu");
    assert_eq!(h.menus_shown(), 2);
}

#[test]
fn escape_or_an_outside_click_dismisses_the_menu() {
    let mut h = started();
    h.right_click(0, None);
    h.dismiss_menu();
    assert!(!h.menu_open());
    assert_eq!(h.window_count(), 1);
}

#[test]
fn the_menu_goes_away_when_the_note_is_clicked() {
    let mut h = started();
    h.right_click(0, None);
    h.click(0, 100.0, 200.0);
    assert!(!h.menu_open());
}

#[test]
fn the_menu_grows_when_a_list_opens_and_stays_on_the_card() {
    let mut h = started();
    h.right_click(0, None);
    let closed = h.last_menu().size;
    h.set_menu_lists(true, false);
    let tapes = h.last_menu().size;
    assert!(tapes.1 > closed.1, "the Tape color list adds its four options");
    h.set_menu_lists(false, true);
    let none_closed = h.last_menu().size;
    assert!(none_closed.1 > closed.1, "Recently closed shows 'No recently closed notes'");
    assert!(none_closed.1 < tapes.1);
}

#[test]
fn undo_and_redo_are_offered_when_there_is_something_to_undo_or_redo() {
    let mut h = started();
    h.type_text(0, "first");
    h.advance(ms(600));
    h.type_text(0, " second");
    h.advance(ms(600));
    h.right_click(0, None);
    assert!(h.last_menu().can_undo);
    assert!(!h.last_menu().can_redo);

    let before = h.note_text(0);
    h.pick(&["Undo"]);
    assert_ne!(h.note_text(0), before, "the last typing was undone");

    h.right_click(0, None);
    assert!(h.last_menu().can_redo);
    h.pick(&["Redo"]);
    assert_eq!(h.note_text(0), before);
}

#[test]
fn cut_and_copy_need_a_selection_and_take_it() {
    let mut h = started();
    h.type_text(0, "hello");
    h.advance(ms(600));
    h.right_click(0, None);
    assert!(!h.last_menu().has_selection, "a caret is not a selection");

    // (the menu took the keyboard focus: start again to select with the keys)
    let mut h = started();
    h.type_text(0, "hello");
    h.advance(ms(600));
    h.press_ctrl(0, 'a');
    h.right_click(0, None);
    assert!(h.last_menu().has_selection);
    h.pick(&["Copy"]);
    assert_eq!(h.clipboard_text().as_deref(), Some("hello"));
    assert_eq!(h.note_text(0), "# hello", "copy leaves the note alone");

    h.right_click(0, Some(&["Cut"]));
    assert_eq!(h.note_text(0), "");
    assert_eq!(h.clipboard_text().as_deref(), Some("hello"));
}

#[test]
fn tape_color_is_chosen_per_note_and_kept_over_a_restart() {
    let mut h = started();
    h.new_note_from(0);
    assert_eq!((h.tape_style(0), h.tape_style(1)), (0, 0));

    h.right_click(0, None);
    assert_eq!(h.last_menu().tape_style, 0);
    h.pick(&["Tape color", "Lavender + peach"]);
    assert_eq!(h.tape_style(0), 2);
    assert_eq!(h.tape_style(1), 0, "the other note keeps its tape");
    assert_eq!(h.last_menu().tape_style, 2, "the open menu shows the choice at once");

    h.right_click(1, None);
    assert_eq!(h.last_menu().tape_style, 0);

    h.restart().unwrap();
    assert_eq!(h.tape_style(0), 2);
    assert_eq!(h.tape_style(1), 0);
    h.right_click(0, None);
    assert_eq!(h.last_menu().tape_style, 2);
}

#[test]
fn every_tape_color_can_be_picked_and_drawn() {
    let mut h = started();
    for (i, name) in TAPE_COLORS.iter().enumerate() {
        h.right_click(0, Some(&["Tape color", name]));
        assert_eq!(h.tape_style(0), i as i32);
        let _ = h.render(0); // the recolored tapes draw without trouble
    }
}

#[test]
fn a_recolored_tape_looks_different_from_the_default() {
    let mut h = started();
    let default = h.render(0);
    h.right_click(0, Some(&["Tape color", "Slate + rose"]));
    let slate = h.render(0);
    assert_ne!(default, slate, "the tapes changed color");
    h.right_click(0, Some(&["Tape color", "Yellow + green"]));
    assert_ne!(h.render(0), slate, "each colorway has its own look");
}

#[test]
fn recently_closed_lists_closed_notes_and_reopens_the_one_picked() {
    let mut h = started();
    h.type_text(0, "keep me");
    h.new_note_from(0);
    h.type_text(1, "groceries");
    h.advance(ms(600));
    h.close_note(1);
    assert_eq!(h.open_note_count(), 1);

    h.right_click(0, None);
    assert_eq!(h.last_menu().closed, ["groceries"]);
    h.pick(&["Recently closed", "groceries"]);
    assert_eq!(h.open_note_count(), 2);
    assert!(!h.menu_open(), "choosing a note takes the menu away");
}

#[test]
fn pin_on_top_keeps_the_menu_open_and_shows_the_state() {
    let mut h = started();
    h.right_click(0, None);
    h.pick(&["Pin on top"]);
    assert!(h.menu_open());
    assert!(h.always_on_top(0));
    assert!(h.last_menu().pinned);
    h.pick(&["Pin on top"]);
    assert!(!h.always_on_top(0));
    assert!(!h.last_menu().pinned);
}
