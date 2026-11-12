//! Ticket 10: a note is paragraphs, written with Enter, Backspace, Delete and the arrow keys, stored
//! as the note text format; an empty note shows the hint. Real key events through the app harness.

use slint::platform::Key;
use snap::shot::{snapshot, Options, WINDOW_MARGIN};
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

/// A note holding `text`, the caret in its title field (a reopened note focuses its title).
fn note(text: &str) -> Harness {
    let mut h = started();
    h.set_note_text(0, text);
    let _ = h.render(0); // drawing creates the text fields; the first one takes the focus
    h.advance(ms(5));
    h
}

#[test]
fn a_title_and_body_written_with_enter_come_back_after_restart() {
    let mut h = started();
    h.type_text(0, "Shopping");
    h.press_enter(0);
    h.type_text(0, "milk and eggs");
    h.press_enter(0);
    h.type_text(0, "bread");
    h.advance(ms(500));
    let want = "# Shopping\nmilk and eggs\nbread";
    assert_eq!(h.note_text(0), want);
    assert_eq!(h.saved_text(0), want);
    h.restart().unwrap();
    assert_eq!(h.note_text(0), want);
    // and the note can be written on after the restart: the title field has the focus again
    h.type_text(0, "?");
    assert_eq!(h.note_text(0), "# ?Shopping\nmilk and eggs\nbread");
}

#[test]
fn enter_splits_the_paragraph_at_the_caret_and_the_second_half_starts_a_body() {
    let mut h = started();
    h.type_text(0, "hello world");
    h.press_n(0, Key::LeftArrow, 6);
    h.press_enter(0);
    assert_eq!(h.note_text(0), "# hello\n world");
    h.type_text(0, "X"); // the caret is at the start of the new paragraph
    assert_eq!(h.note_text(0), "# hello\nX world");
}

#[test]
fn enter_in_a_list_continues_it_and_on_an_empty_item_ends_it() {
    let mut h = note("t\n- milk");
    h.press(0, Key::DownArrow);
    h.press(0, Key::End);
    h.press_enter(0);
    h.type_text(0, "eggs");
    assert_eq!(h.note_text(0), "# t\n- milk\n- eggs");
    h.press_enter(0);
    assert_eq!(h.note_text(0), "# t\n- milk\n- eggs\n- ");
    h.press_enter(0);
    assert_eq!(h.note_text(0), "# t\n- milk\n- eggs\n");
    h.type_text(0, "done");
    assert_eq!(h.note_text(0), "# t\n- milk\n- eggs\ndone");
}

#[test]
fn backspace_at_the_start_merges_into_the_previous_paragraph_and_the_title_stays() {
    let mut h = note("title\nab\ncd");
    h.press(0, Key::DownArrow);
    h.press(0, Key::DownArrow);
    h.press(0, Key::Home);
    h.press_backspace(0);
    assert_eq!(h.note_text(0), "# title\nabcd");
    h.type_text(0, "X"); // the caret is where the paragraphs met
    assert_eq!(h.note_text(0), "# title\nabXcd");
    h.press(0, Key::Home);
    h.press_backspace(0);
    assert_eq!(h.note_text(0), "# titleabXcd");
    // the title cannot be merged away
    h.press_n(0, Key::LeftArrow, 20);
    h.press_backspace(0);
    assert_eq!(h.note_text(0), "# titleabXcd");
}

#[test]
fn backspace_inside_a_paragraph_still_deletes_a_letter() {
    let mut h = started();
    h.type_text(0, "abc");
    h.press_backspace(0);
    assert_eq!(h.note_text(0), "# ab");
}

#[test]
fn delete_at_the_end_pulls_the_next_paragraph_in() {
    let mut h = note("ab\ncd\nef");
    h.press(0, Key::End);
    h.press(0, Key::Delete);
    assert_eq!(h.note_text(0), "# abcd\nef");
    h.type_text(0, "X");
    assert_eq!(h.note_text(0), "# abXcd\nef");
    h.press(0, Key::Delete); // inside the paragraph: deletes a letter
    assert_eq!(h.note_text(0), "# abXd\nef");
    // the end of the last paragraph: nothing to pull in
    h.press(0, Key::DownArrow);
    h.press(0, Key::End);
    h.press(0, Key::Delete);
    assert_eq!(h.note_text(0), "# abXd\nef");
}

#[test]
fn up_and_down_move_between_paragraphs_keeping_the_x_position() {
    let mut h = note("t\nabcdef\nabcdef");
    h.press(0, Key::DownArrow); // title -> first body paragraph
    h.press_n(0, Key::RightArrow, 3);
    h.press(0, Key::DownArrow); // -> second, same x
    h.type_text(0, "X");
    assert_eq!(h.note_text(0), "# t\nabcdef\nabcXdef");
    h.press(0, Key::UpArrow);
    h.type_text(0, "Y");
    assert_eq!(h.note_text(0).lines().nth(1).unwrap().len(), 7, "one letter added to the first paragraph");
    assert!(h.note_text(0).lines().nth(1).unwrap().contains('Y'));
    h.press(0, Key::UpArrow);
    h.type_text(0, "Z");
    assert!(h.note_text(0).starts_with("# t"), "back in the title");
    assert!(h.note_text(0).lines().next().unwrap().contains('Z'));
}

#[test]
fn the_ends_of_the_note_keep_their_arrow_keys_and_nothing_panics() {
    let mut h = note("a\nb");
    h.press(0, Key::UpArrow);
    h.press(0, Key::DownArrow);
    h.press(0, Key::DownArrow);
    h.press(0, Key::DownArrow);
    h.type_text(0, "X");
    assert_eq!(h.note_text(0), "# a\nXb");
}

#[test]
fn down_inside_a_wrapped_paragraph_stays_in_it() {
    let long = "word ".repeat(30);
    let mut h = note(&format!("t\n{long}\nnext"));
    h.press(0, Key::DownArrow);
    h.press(0, Key::DownArrow); // second visual line of the same paragraph
    h.type_text(0, "X");
    let text = h.note_text(0);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[1].contains('X'), "{text}");
    assert_eq!(lines[2], "next");
}

#[test]
fn a_multi_line_paste_becomes_paragraphs() {
    let mut h = started();
    h.type_text(0, "ab");
    h.paste(0, "x\ny\r\nz");
    assert_eq!(h.note_text(0), "# abx\ny\nz");
    h.type_text(0, "!"); // the caret is after the pasted text
    assert_eq!(h.note_text(0), "# abx\ny\nz!");
    // into the middle of a paragraph: the text after the caret goes with the last line
    let mut h = note("t\nhead tail");
    h.press(0, Key::DownArrow);
    h.press_n(0, Key::RightArrow, 4);
    h.paste(0, "1\n2");
    assert_eq!(h.note_text(0), "# t\nhead1\n2 tail");
}

#[test]
fn older_plain_notes_open_as_title_and_body() {
    let mut h = started();
    h.set_note_text(0, "shopping list\nmilk\n\neggs");
    assert_eq!(h.window_title(0), "shopping list");
    assert_eq!(h.note_text(0), "# shopping list\nmilk\n\neggs");
}

// ---- the hint ----

#[test]
fn an_empty_note_shows_the_hint_until_the_first_keystroke() {
    let mut h = started();
    assert_eq!(h.hint_shown(0), "NoteSomething....");
    h.type_text(0, "a");
    assert_eq!(h.hint_shown(0), "");
    assert_eq!(h.note_text(0), "# a", "the typed text is the title");
    h.press_backspace(0);
    assert_eq!(h.hint_shown(0), "NoteSomething....", "an empty note again");
    assert_eq!(h.note_text(0), "");
}

#[test]
fn the_hint_is_never_saved_and_a_note_with_text_has_none() {
    let mut h = started();
    h.advance(ms(600));
    assert_eq!(h.saved_text(0), "");
    h.set_note_text(0, "something\nelse");
    assert_eq!(h.hint_shown(0), "");
    h.set_note_text(0, "");
    assert_eq!(h.hint_shown(0), "NoteSomething....");
    // an empty title with a body is not an empty note
    h.set_note_text(0, "\nbody");
    assert_eq!(h.hint_shown(0), "");
}

fn pixels_like(s: &snap::shot::Snapshot, paper: (f64, f64), color: [u8; 3], tol: i32) -> usize {
    // the text rows only: above the toolbar band
    let top = ((s.height as f64 - (paper.1 + 2.0 * WINDOW_MARGIN) * 1.5) / 2.0 + WINDOW_MARGIN * 1.5) as u32;
    let bottom = top + ((paper.1 - 80.0) * 1.5) as u32;
    (top..bottom)
        .flat_map(|y| (0..s.width).map(move |x| (x, y)))
        .filter(|&(x, y)| s.pixel(x, y).iter().zip(color).all(|(&a, b)| (a as i32 - b as i32).abs() <= tol))
        .count()
}

#[test]
fn the_hint_is_drawn_in_the_hint_color_at_rest_and_while_editing() {
    let paper = (363.0, 433.0);
    for editing in [false, true] {
        let o = Options { text: String::new(), paper, editing, ..Options::default() };
        let s = snapshot(&o).unwrap();
        let hint = pixels_like(&s, paper, [0xB4, 0xB9, 0xC0], 6);
        assert!(hint > 150, "editing {editing}: {hint} hint-colored pixels");
        let ink = pixels_like(&s, paper, [0x2B, 0x2F, 0x36], 30);
        // (the editing caret is note-colored)
        assert!(ink <= if editing { 300 } else { 0 }, "editing {editing}: {ink} note-colored pixels");
    }
}

#[test]
fn the_hint_sits_on_the_title_line() {
    let h = started();
    let runs = h.text_runs(0);
    // drawn in pieces of at most 16 px (stepped text); the words are Note and Something....
    assert_eq!(runs.iter().map(|r| r.0.as_str()).collect::<String>(), "NoteSomething....");
    assert!((runs[0].1 - 32.0).abs() < 0.01 && (runs[0].2 - 38.2).abs() < 0.01, "{:?}", runs[0]);
    assert!(runs.iter().all(|r| r.2 == runs[0].2), "one line");
}

/// Navigation reaches the note as a key plus typed modifiers (no string protocol), and the note,
/// not the field, decides what an edge means.
#[test]
fn the_note_takes_a_navigation_key_with_typed_modifiers() {
    let mut h = note("t
ab");
    h.press(0, Key::RightArrow); // the caret at the end of the title
    // Plain Left inside a paragraph is the field's own: the note declines it
    assert!(!h.block_key(0, 0, Key::LeftArrow, false, false, 1));
    // Shift+Right at the end of the title carries the selection into the next paragraph
    assert!(h.block_key(0, 0, Key::RightArrow, true, false, 1));
    assert_eq!(h.selection(0), ((0, 1), (1, 0)));
    // Ctrl+Shift+End extends to the end of the note
    assert!(h.block_key(0, 0, Key::End, true, true, 1));
    assert_eq!(h.selection(0), ((0, 1), (1, 2)));
}
