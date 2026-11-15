//! Lists that are moved in (.scratch/list-indent): Tab moves the whole list the caret is in one step
//! in, Shift+Tab (or Backspace at the very front of an item) moves it back, whatever its kinds. Real
//! keys through the app harness, checked against the stored note text.

use slint::platform::Key;
use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// A note holding `text`, the caret in its title field.
fn note(text: &str) -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, text);
    let _ = h.render(0); // drawing creates the text fields; the first one takes the focus
    h.advance(ms(5));
    h
}

#[test]
fn tab_moves_the_whole_list_in_and_shift_tab_moves_it_back() {
    let mut h = note("t\n- a\n1. b\n- [ ] c\nplain\n- d");
    h.place_caret(0, 2);
    h.press(0, Key::Tab);
    assert_eq!(h.note_text(0), "# t\n    - a\n    1. b\n    - [ ] c\nplain\n- d");
    h.press(0, Key::Tab);
    assert_eq!(h.note_text(0), "# t\n    - a\n    1. b\n    - [ ] c\nplain\n- d", "a second Tab does nothing");
    h.press_shift(0, Key::Tab, 1);
    assert_eq!(h.note_text(0), "# t\n- a\n1. b\n- [ ] c\nplain\n- d");
    h.press_shift(0, Key::Tab, 1);
    assert_eq!(h.note_text(0), "# t\n- a\n1. b\n- [ ] c\nplain\n- d", "a list that is not moved in stays");
}

#[test]
fn tab_in_a_plain_paragraph_changes_nothing() {
    let mut h = note("t\nplain\n- a");
    h.place_caret(0, 1);
    h.press(0, Key::Tab);
    assert_eq!(h.note_text(0), "# t\nplain\n- a");
}

#[test]
fn enter_continues_a_moved_in_list_and_backspace_at_the_front_moves_it_back_before_dropping_the_marker() {
    let mut h = note("t\n- a");
    h.place_caret(0, 1);
    h.press(0, Key::Tab);
    h.press(0, Key::End);
    h.press_enter(0);
    h.type_text(0, "b");
    assert_eq!(h.note_text(0), "# t\n    - a\n    - b");
    h.press(0, Key::Home);
    h.press_backspace(0);
    assert_eq!(h.note_text(0), "# t\n- a\n- b", "the list moved back");
    h.press_backspace(0);
    assert_eq!(h.note_text(0), "# t\n- a\nb", "then the marker went");
}

#[test]
fn a_tab_is_one_undo_step_and_survives_a_restart() {
    let mut h = note("t\n- a\n- b");
    h.place_caret(0, 1);
    h.press(0, Key::Tab);
    assert_eq!(h.note_text(0), "# t\n    - a\n    - b");
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), "# t\n- a\n- b");
    h.press_ctrl(0, 'y');
    assert_eq!(h.note_text(0), "# t\n    - a\n    - b");
    h.restart().unwrap();
    assert_eq!(h.note_text(0), "# t\n    - a\n    - b");
}
