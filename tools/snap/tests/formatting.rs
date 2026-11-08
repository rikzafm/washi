//! Ticket 11: the toolbar buttons and the shortcuts format the selected text (the word at the
//! caret, or what is typed next, with nothing selected; the whole note after a select-all) and the
//! list button the paragraph holding the caret; the buttons show the selection's formats; `# ` and
//! `- ` make a heading or list item. Real clicks and
//! key events through the app harness; the look is read from snapshots.

use slint::platform::Key;
use snap::shot::{snapshot, Options, Snapshot, WINDOW_MARGIN};
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

/// The center of toolbar button `k` (0 B, 1 I, 2 U, 3 ab, 4 A, 5 list, 6 image) in window coordinates: the
/// button row sits in the paper's frame, which is turned 1.5 degrees about the paper's center.
fn button(h: &Harness, k: usize) -> (f32, f32) {
    let (w, hh) = h.note_size(0);
    let (pw, ph) = (w as f64 - 84.0, hh as f64 - 84.0);
    let du = 23.2 + 20.0 + k as f64 * 42.667 - pw / 2.0;
    let dv = ph / 2.0 - 55.8 + 20.0;
    let (s, c) = 1.5_f64.to_radians().sin_cos();
    ((w as f64 / 2.0 + du * c - dv * s) as f32, (hh as f64 / 2.0 + du * s + dv * c) as f32)
}

/// Click button `k` with the real pointer (it shows the toolbar first, as a user would).
fn click_button(h: &mut Harness, k: usize) {
    let (x, y) = button(h, k);
    h.pointer_move(0, x, y);
    h.advance(ms(300));
    h.click(0, x, y);
}

#[test]
fn clicking_b_bolds_the_selected_text_and_b_shows_active() {
    let mut h = note("title\nbody text");
    h.press(0, Key::DownArrow); // the caret goes to the body
    assert_eq!(h.toolbar_active(0), [false; 5]);
    h.press_shift(0, Key::RightArrow, 4); // "body"
    click_button(&mut h, 0);
    assert_eq!(h.note_text(0), "# title\n**body** text");
    assert_eq!(h.toolbar_active(0), [true, false, false, false, false]);
    // the rest of the paragraph too: part of the selection is bold, so all of it becomes bold
    h.press(0, Key::Home);
    h.press_shift(0, Key::End, 1);
    assert!(!h.toolbar_active(0)[0], "not all of the selection is bold");
    click_button(&mut h, 0);
    assert_eq!(h.note_text(0), "# title\n**body text**");
    // and the caret is still in the body: typing goes there, in the style before it
    h.press(0, Key::End);
    h.type_text(0, "!");
    assert_eq!(h.note_text(0), "# title\n**body text!**");
}

#[test]
fn b_follows_the_caret_to_text_that_is_not_bold() {
    let mut h = note("title\nbody text");
    h.press(0, Key::DownArrow);
    h.press_shift(0, Key::End, 1);
    click_button(&mut h, 0);
    assert!(h.toolbar_active(0)[0]);
    h.press(0, Key::Home);
    h.press(0, Key::UpArrow);
    assert!(!h.toolbar_active(0)[0], "the title is not bold");
    h.press(0, Key::DownArrow);
    assert!(h.toolbar_active(0)[0], "back in the bold body");
}

#[test]
fn a_heading_shows_b_active() {
    let mut h = note("title\n## **Heading**\nbody");
    h.press(0, Key::DownArrow);
    assert!(h.toolbar_active(0)[0]);
    h.press(0, Key::DownArrow);
    assert!(!h.toolbar_active(0)[0]);
}

#[test]
fn each_button_toggles_its_format_and_a_second_click_takes_it_off() {
    let cases = [(1, "*body*"), (2, "<u>body</u>"), (3, "~~body~~"), (5, "- body")];
    for (k, stored) in cases {
        let mut h = note("title\nbody");
        h.press(0, Key::DownArrow);
        h.press_shift(0, Key::End, 1);
        click_button(&mut h, k);
        assert_eq!(h.note_text(0), format!("# title\n{stored}"), "button {k}");
        let mut active = [false; 5];
        active[if k == 5 { 4 } else { k }] = true;
        assert_eq!(h.toolbar_active(0), active, "button {k}");
        click_button(&mut h, k);
        assert_eq!(h.note_text(0), "# title\nbody", "button {k} again");
        assert_eq!(h.toolbar_active(0), [false; 5]);
    }
}

#[test]
fn the_shortcuts_format_the_selection_with_the_toolbar_hidden() {
    let mut h = note("title\nbody");
    h.press(0, Key::DownArrow);
    h.press_shift(0, Key::End, 1);
    assert_eq!(h.toolbar_opacity(0), 0.0);
    h.press_ctrl(0, 'b');
    assert_eq!(h.note_text(0), "# title\n**body**");
    h.press_ctrl(0, 'i');
    assert_eq!(h.note_text(0), "# title\n***body***");
    h.press_ctrl(0, 'u');
    assert_eq!(h.note_text(0), "# title\n<u>***body***</u>");
    h.press_ctrl_shift(0, 'x');
    assert_eq!(h.note_text(0), "# title\n<u>~~***body***~~</u>");
    assert_eq!(h.toolbar_active(0), [true, true, true, true, false]);
    h.press_ctrl_shift(0, 'l');
    assert_eq!(h.note_text(0), "# title\n- <u>~~***body***~~</u>");
    assert_eq!(h.toolbar_active(0), [true, true, true, true, true]);
    assert_eq!(h.toolbar_opacity(0), 0.0, "the toolbar never showed");
    // all off again
    h.press_ctrl(0, 'b');
    h.press_ctrl(0, 'i');
    h.press_ctrl(0, 'u');
    h.press_ctrl_shift(0, 'x');
    h.press_ctrl_shift(0, 'l');
    assert_eq!(h.note_text(0), "# title\nbody");
}

#[test]
fn with_nothing_selected_a_format_takes_the_word_at_the_caret() {
    let mut h = note("title\nbody text");
    h.press(0, Key::DownArrow);
    h.press_n(0, Key::RightArrow, 2); // inside "body"
    h.press_ctrl(0, 'b');
    assert_eq!(h.note_text(0), "# title\n**body** text");
    assert!(h.toolbar_active(0)[0]);
    h.press_ctrl(0, 'b');
    assert_eq!(h.note_text(0), "# title\nbody text");
}

#[test]
fn outside_a_word_a_format_applies_to_what_is_typed_next() {
    let mut h = note("title\nbody");
    h.press(0, Key::DownArrow);
    h.press(0, Key::End);
    h.press_ctrl(0, 'b');
    assert_eq!(h.note_text(0), "# title\nbody", "nothing to style yet");
    assert!(h.toolbar_active(0)[0], "B shows what typing will do");
    h.type_text(0, " bold");
    assert_eq!(h.note_text(0), "# title\nbody** bold**");
    // off again, and Enter carries the style on
    h.press_ctrl(0, 'b');
    h.type_text(0, "!");
    assert_eq!(h.note_text(0), "# title\nbody** bold**!");
    h.press_ctrl(0, 'i');
    h.press_enter(0);
    assert!(h.toolbar_active(0)[1], "the new paragraph goes on in italic");
    h.type_text(0, "x");
    assert_eq!(h.note_text(0), "# title\nbody** bold**!\n*x*");
    // moving the caret forgets a style asked for
    h.press(0, Key::End);
    h.press_ctrl(0, 'u');
    h.press(0, Key::Home);
    h.press(0, Key::End);
    h.type_text(0, "y");
    assert_eq!(h.note_text(0), "# title\nbody** bold**!\n*xy*");
}

#[test]
fn after_a_select_all_a_format_styles_the_whole_note() {
    let mut h = note("title\nbody");
    h.press_ctrl(0, 'a');
    h.press_ctrl(0, 'b');
    assert_eq!(h.note_text(0), "# **title**\n**body**");
    assert!(h.toolbar_active(0)[0]);
    h.press_ctrl(0, 'b');
    assert_eq!(h.note_text(0), "# title\nbody");
}

#[test]
fn a_shortcut_does_not_type_its_letter_or_cut() {
    let mut h = note("title\nbody");
    h.press(0, Key::DownArrow);
    h.press_shift(0, Key::End, 1);
    h.press_ctrl(0, 'b');
    h.press_ctrl_shift(0, 'x');
    assert!(h.note_text(0).contains("body"), "{:?}", h.note_text(0));
}

#[test]
fn headings_and_titles_take_the_styles_but_only_body_becomes_a_list_item() {
    let mut h = note("title\nbody");
    h.press_shift(0, Key::End, 1); // in the title
    h.press_ctrl(0, 'i');
    assert_eq!(h.note_text(0), "# *title*\nbody");
    h.press_ctrl_shift(0, 'l');
    assert_eq!(h.note_text(0), "# *title*\nbody", "a title is not a list item");
}

#[test]
fn typing_hash_space_or_dash_space_at_the_start_of_a_paragraph_makes_a_heading_or_list_item() {
    let mut h = note("title\nplan");
    h.press(0, Key::DownArrow);
    h.press(0, Key::Home);
    h.type_text(0, "# ");
    assert_eq!(h.note_text(0), "# title\n## plan");
    h.type_text(0, "X"); // the caret is at the start of the heading
    assert_eq!(h.note_text(0), "# title\n## Xplan");

    let mut h = note("title");
    h.press(0, Key::End);
    h.press_enter(0);
    h.type_text(0, "- milk");
    assert_eq!(h.note_text(0), "# title\n- milk");
    // `# ` typed later in a paragraph stays text
    h.press_enter(0);
    h.press_enter(0); // an empty item ends the list
    h.type_text(0, "a # b");
    assert_eq!(h.note_text(0), "# title\n- milk\na # b");
    // and the title does not turn into a heading
    let mut h = note("");
    h.type_text(0, "# x");
    assert_eq!(h.note_text(0), "# \\# x"); // still the title; its text starts with a (stored escaped) hash
}

#[test]
fn formats_are_saved_and_come_back_after_a_restart() {
    let mut h = note("title\nbody\nmore");
    h.press(0, Key::DownArrow);
    h.press_shift(0, Key::End, 1);
    click_button(&mut h, 0);
    click_button(&mut h, 2);
    h.press(0, Key::End);
    h.press(0, Key::DownArrow);
    click_button(&mut h, 5);
    h.press(0, Key::End);
    h.press_shift(0, Key::Home, 1);
    h.press_ctrl(0, 'i');
    h.advance(ms(500));
    let want = "# title\n<u>**body**</u>\n- *more*";
    assert_eq!(h.note_text(0), want);
    assert_eq!(h.saved_text(0), want);
    h.restart().unwrap();
    assert_eq!(h.note_text(0), want);
}

// ---- how it looks ----

fn text_rows(paper_h: f64) -> (u32, u32) {
    let win_h = ((paper_h + 2.0 * WINDOW_MARGIN) * 1.5).round();
    let top = (840.0 - win_h) / 2.0 + WINDOW_MARGIN * 1.5;
    (top as u32, (top + (paper_h - 56.0) * 1.5) as u32)
}

fn ink(s: &Snapshot, x: u32, y: u32) -> bool {
    s.pixel(x, y).iter().map(|&c| c as u32).sum::<u32>() < 3 * 140
}

fn shot(text: &str, editing: bool) -> Snapshot {
    snapshot(&Options { text: text.into(), editing, ..Options::default() }).unwrap()
}

/// The longest horizontal run of ink in any one row of the text area.
fn longest_run(s: &Snapshot) -> u32 {
    let (top, bottom) = text_rows(384.0);
    let mut best = 0;
    for y in top..bottom {
        let mut run = 0;
        for x in 0..s.width {
            run = if ink(s, x, y) { run + 1 } else { 0 };
            best = best.max(run);
        }
    }
    best
}

fn ink_count(s: &Snapshot) -> usize {
    let (top, bottom) = text_rows(384.0);
    (top..bottom).map(|y| (0..s.width).filter(|&x| ink(s, x, y)).count()).sum()
}

#[test]
fn underline_and_strike_are_drawn_as_a_line_under_and_through_the_words() {
    // level (editing): one row of ink as long as the words, which no plain text has
    let plain = longest_run(&shot("t\nhello wide world", true));
    for stored in ["t\n<u>hello wide world</u>", "t\n~~hello wide world~~"] {
        let run = longest_run(&shot(stored, true));
        assert!(run >= 150 && run > plain + 60, "{stored}: a line of {run} px (plain text {plain})");
    }
    // at rest the words follow the tilted baseline and so does the line
    let plain = ink_count(&shot("t\nhello wide world", false));
    for stored in ["t\n<u>hello wide world</u>", "t\n~~hello wide world~~"] {
        let more = ink_count(&shot(stored, false));
        assert!(more > plain + 150, "{stored}: {more} ink pixels at rest, plain text {plain}");
    }
}

#[test]
fn a_list_item_has_a_bullet_left_of_its_text() {
    // paper frame x < 28: nothing is drawn there (the text starts at 32) except a bullet
    let left_ink = |s: &Snapshot| {
        let (top, bottom) = text_rows(384.0);
        let x0 = ((840.0f64 * 0.0) + (720.0 - (363.0 + 84.0) * 1.5) / 2.0 + WINDOW_MARGIN * 1.5) as u32;
        (top..bottom).map(|y| (x0..x0 + (28.0 * 1.5) as u32).filter(|&x| ink(s, x, y)).count()).sum::<usize>()
    };
    assert_eq!(left_ink(&shot("t\nitem", true)), 0);
    assert!(left_ink(&shot("t\n- item", true)) >= 8, "no bullet");
    assert!(left_ink(&shot("t\n- item", false)) >= 8, "no bullet at rest");
}

#[test]
fn a_bold_word_is_bold_while_editing_and_at_rest_and_the_rest_of_the_line_is_not() {
    for editing in [true, false] {
        let plain = shot("t\nplain words then", editing);
        let mixed = shot("t\nplain **words** then", editing);
        let all = shot("t\n**plain words then**", editing);
        assert!(ink_count(&mixed) > ink_count(&plain), "editing {editing}: the bold word adds ink");
        assert!(ink_count(&mixed) < ink_count(&all), "editing {editing}: the plain words stay thin");
    }
}

#[test]
fn italic_text_is_drawn_in_the_italic_face_and_bold_italic_in_its_own() {
    let regular = shot("t\nSlanted words here", true);
    let italic = shot("t\n*Slanted words here*", true);
    let both = shot("t\n***Slanted words here***", true);
    let bold = shot("t\n**Slanted words here**", true);
    assert!(regular.rgb() != italic.rgb(), "italic looks like regular");
    assert!(both.rgb() != bold.rgb(), "bold italic looks like bold");
    assert!(both.rgb() != italic.rgb(), "bold italic looks like italic");
    // at rest too
    assert!(shot("t\nSlanted words here", false).rgb() != shot("t\n*Slanted words here*", false).rgb());
}
