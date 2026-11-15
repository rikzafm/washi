//! Numbered lists (.scratch/numbered-list): typing `1. ` or `1, ` starts one, Enter continues the
//! count and Backspace removes the number; the list button shares its slot with the bulleted list
//! through a corner notch, a right-click or a hold that open a flyout. Real keys and clicks through
//! the app harness, checked against the stored note text.

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

/// Show the toolbar (the pointer over the paper), then click at `(x, y)`.
fn click_shown(h: &mut Harness, (x, y): (f32, f32)) {
    h.pointer_move(0, x, y);
    h.advance(ms(300));
    h.click(0, x, y);
}

const LIST: usize = 5;

fn click_list(h: &mut Harness) {
    let at = h.toolbar_button(0, LIST);
    click_shown(h, at);
}

/// Open the flyout with the notch and pick choice `k` (0 bulleted, 1 numbered).
fn pick(h: &mut Harness, k: usize) {
    let at = h.list_notch(0);
    click_shown(h, at);
    assert!(h.list_menu_open(0), "the notch opens the flyout");
    let at = h.list_choice(0, k);
    h.click(0, at.0, at.1);
    assert!(!h.list_menu_open(0), "picking closes the flyout");
}

#[test]
fn typing_one_dot_space_starts_a_numbered_list_that_enter_continues_and_ends() {
    let mut h = note("t\n");
    h.press(0, Key::DownArrow);
    h.type_text(0, "1. milk");
    assert_eq!(h.note_text(0), "# t\n1. milk");
    h.press_enter(0);
    h.type_text(0, "eggs");
    h.press_enter(0);
    h.type_text(0, "bread");
    assert_eq!(h.note_text(0), "# t\n1. milk\n2. eggs\n3. bread");
    assert_eq!(h.list_shown(0).0, 2, "the button shows the paragraph is numbered");
    h.press_enter(0);
    h.press_enter(0); // on the empty item: the list ends
    h.type_text(0, "done");
    assert_eq!(h.note_text(0), "# t\n1. milk\n2. eggs\n3. bread\ndone");
    assert_eq!(h.list_shown(0).0, 0);
    h.advance(ms(500));
    h.restart().unwrap();
    assert_eq!(h.note_text(0), "# t\n1. milk\n2. eggs\n3. bread\ndone");
}

#[test]
fn one_comma_space_starts_a_list_too_but_other_numbers_stay_text() {
    let mut h = note("t\n");
    h.press(0, Key::DownArrow);
    h.type_text(0, "1, one");
    assert_eq!(h.note_text(0), "# t\n1. one", "shown and stored as 1.");
    h.press_enter(0);
    h.press_enter(0);
    h.type_text(0, "3. apples");
    assert_eq!(h.note_text(0), "# t\n1. one\n\\3. apples", "plain text, its digit escaped");
    assert_eq!(h.list_shown(0).0, 0);
    h.advance(ms(500));
    h.restart().unwrap();
    assert_eq!(h.note_text(0), "# t\n1. one\n\\3. apples");
}

#[test]
fn backspace_at_the_start_removes_the_number_and_the_next_items_count_again() {
    let mut h = note("t\n1. a\n2. b\n3. c");
    h.press_n(0, Key::DownArrow, 2);
    h.press(0, Key::Home);
    h.press(0, Key::Backspace);
    assert_eq!(h.note_text(0), "# t\n1. a\nb\n1. c", "b is body; c starts a new run");
    h.press(0, Key::Backspace);
    assert_eq!(h.note_text(0), "# t\n1. ab\n2. c", "a second Backspace merges it up");
}

#[test]
fn the_list_button_applies_the_last_type_and_takes_any_list_off() {
    let mut h = note("t\nbody");
    h.press(0, Key::DownArrow);
    assert_eq!(h.list_shown(0), (0, 1), "never picked: bulleted");
    click_list(&mut h);
    assert_eq!(h.note_text(0), "# t\n- body");
    click_list(&mut h);
    assert_eq!(h.note_text(0), "# t\nbody");
    pick(&mut h, 1);
    assert_eq!(h.note_text(0), "# t\n1. body");
    assert_eq!(h.list_shown(0), (2, 2));
    click_list(&mut h);
    assert_eq!(h.note_text(0), "# t\nbody", "on a list paragraph the button takes it off");
    assert_eq!(h.list_shown(0), (0, 2), "and the last type stays numbered");
    click_list(&mut h);
    assert_eq!(h.note_text(0), "# t\n1. body");
    // the last type is the app's, kept over a restart
    h.advance(ms(500));
    h.restart().unwrap();
    h.press(0, Key::DownArrow);
    assert_eq!(h.list_shown(0), (2, 2));
}

#[test]
fn the_list_button_takes_a_bulleted_paragraph_off_even_when_numbered_was_last() {
    let mut h = note("t\n- milk\nbody");
    h.press_n(0, Key::DownArrow, 2);
    pick(&mut h, 1);
    assert_eq!(h.note_text(0), "# t\n- milk\n1. body");
    h.press(0, Key::UpArrow);
    assert_eq!(h.list_shown(0), (1, 2));
    click_list(&mut h);
    assert_eq!(h.note_text(0), "# t\nmilk\n1. body");
}

#[test]
fn the_flyout_switches_a_paragraphs_type_and_picking_its_own_type_takes_it_off() {
    let mut h = note("t\n- milk");
    h.press(0, Key::DownArrow);
    pick(&mut h, 1);
    assert_eq!(h.note_text(0), "# t\n1. milk");
    pick(&mut h, 0);
    assert_eq!(h.note_text(0), "# t\n- milk");
    pick(&mut h, 0);
    assert_eq!(h.note_text(0), "# t\nmilk");
}

#[test]
fn only_the_selected_paragraphs_change_type() {
    let mut h = note("t\n- a\n- b\n- c\n- d");
    h.press_n(0, Key::DownArrow, 2);
    h.press(0, Key::Home);
    h.press_shift(0, Key::DownArrow, 1); // b and c
    pick(&mut h, 1);
    assert_eq!(h.note_text(0), "# t\n- a\n1. b\n2. c\n- d");
}

#[test]
fn a_right_click_or_a_hold_opens_the_flyout_and_changes_nothing() {
    let mut h = note("t\nbody");
    h.press(0, Key::DownArrow);
    let at = h.toolbar_button(0, LIST);
    h.pointer_move(0, at.0, at.1);
    h.advance(ms(300));
    h.click_right(0, at.0, at.1);
    h.settle();
    assert!(h.list_menu_open(0), "a right-click opens the flyout");
    assert_eq!(h.note_text(0), "# t\nbody");
    h.press(0, Key::Escape);
    assert!(!h.list_menu_open(0), "Escape closes it");

    h.hold(0, at.0, at.1, ms(600));
    assert!(h.list_menu_open(0), "a hold opens the flyout");
    assert_eq!(h.note_text(0), "# t\nbody", "and the release is no click");
    let c = h.list_choice(0, 1);
    h.click(0, c.0, c.1);
    assert_eq!(h.note_text(0), "# t\n1. body");
}

#[test]
fn the_keyboard_picks_in_the_open_flyout() {
    let mut h = note("t\nbody");
    h.press(0, Key::DownArrow);
    let at = h.list_notch(0);
    click_shown(&mut h, at);
    assert!(h.list_menu_open(0));
    h.press(0, Key::RightArrow);
    h.press(0, Key::Return);
    assert!(!h.list_menu_open(0));
    assert_eq!(h.note_text(0), "# t\n1. body");
}

#[test]
fn a_list_change_is_one_undo_step() {
    let mut h = note("t\nbody");
    h.press(0, Key::DownArrow);
    pick(&mut h, 1);
    assert_eq!(h.note_text(0), "# t\n1. body");
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), "# t\nbody");
    h.press_ctrl(0, 'y');
    assert_eq!(h.note_text(0), "# t\n1. body");
}
