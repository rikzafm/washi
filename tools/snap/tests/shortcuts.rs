//! Ticket 05: the Shortcuts tab lists the bindings the app really has. Each row of the table is
//! pressed on a fresh note, so rebinding a key without updating the table fails here.

use slint::platform::Key;
use snap::Harness;
use std::time::Duration;
use washi::settings::shortcuts::SHORTCUTS;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// A note holding a title and a body, the caret in the title.
fn note() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, "title\nbody");
    let _ = h.render(0);
    h.advance(ms(5));
    h
}

/// The caret in the body with "body" selected.
fn select_body(h: &mut Harness) {
    h.press(0, Key::DownArrow);
    h.press(0, Key::Home);
    h.press_shift(0, Key::End, 1);
}

/// Press the keys of a row as the table gives them.
fn press(h: &mut Harness, keys: &[&str]) {
    let letter = keys.last().unwrap().to_ascii_lowercase().chars().next().unwrap();
    match keys {
        ["Ctrl", _] => h.press_ctrl(0, letter),
        ["Ctrl", "Shift", _] => h.press_ctrl_shift(0, letter),
        other => panic!("unknown key shape {other:?}"),
    }
}

fn keys_of(label: &str) -> &'static [&'static str] {
    SHORTCUTS.iter().find(|s| s.label == label).unwrap_or_else(|| panic!("no row {label}")).keys
}

#[test]
fn the_table_has_the_eleven_rows_the_tab_is_sized_for() {
    assert_eq!(SHORTCUTS.len(), 11);
    let labels: Vec<_> = SHORTCUTS.iter().map(|s| s.label).collect();
    assert_eq!(labels, ["New note", "Close note", "Delete note", "Bold", "Italic", "Underline", "Strikethrough", "List", "Select all", "Undo", "Redo"]);
}

#[test]
fn new_note_opens_a_note() {
    let mut h = note();
    press(&mut h, keys_of("New note"));
    assert_eq!(h.open_note_count(), 2);
}

#[test]
fn close_note_closes_the_note() {
    let mut h = note();
    h.new_note_from(0);
    assert_eq!(h.open_note_count(), 2);
    press(&mut h, keys_of("Close note"));
    assert_eq!(h.open_note_count(), 1);
}

#[test]
fn delete_note_asks_to_delete() {
    let mut h = note();
    press(&mut h, keys_of("Delete note"));
    assert!(h.is_delete_dialog_open());
}

#[test]
fn the_format_rows_format_the_selection() {
    for (label, stored) in [
        ("Bold", "# title\n**body**"),
        ("Italic", "# title\n*body*"),
        ("Underline", "# title\n<u>body</u>"),
        ("Strikethrough", "# title\n~~body~~"),
        ("List", "# title\n- body"),
    ] {
        let mut h = note();
        select_body(&mut h);
        press(&mut h, keys_of(label));
        assert_eq!(h.note_text(0), stored, "{label}");
    }
}

#[test]
fn select_all_undo_and_redo_work() {
    let mut h = note();
    press(&mut h, keys_of("Select all"));
    press(&mut h, keys_of("Bold"));
    assert_eq!(h.note_text(0), "# **title**\n**body**", "select all selected both lines");
    press(&mut h, keys_of("Undo"));
    assert_eq!(h.note_text(0), "# title\nbody");
    press(&mut h, keys_of("Redo"));
    assert_eq!(h.note_text(0), "# **title**\n**body**");
}
