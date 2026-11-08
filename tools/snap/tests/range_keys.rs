//! Ticket 05 of cross-paragraph selection: the keys over a selection across paragraphs (typing,
//! Enter, Backspace, Delete, cut, copy, paste, the formats and the list button, the arrows), and
//! Ctrl+A as the range from the start of the note to its end.

use slint::platform::Key;
use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// title, first paragraph, second, third
const TEXT: &str = "# Title\nfirst paragraph here\nsecond one\nthird paragraph words";

fn note(text: &str) -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, text);
    let _ = h.render(0);
    h.advance(ms(5));
    h
}

/// The window point at the left edge (`end` false) or right edge of the `nth` resting word `word`.
fn word(h: &Harness, word: &str, nth: usize, end: bool) -> (f32, f32) {
    let runs = h.text_runs(0);
    let letters: Vec<(char, usize, usize)> =
        runs.iter().enumerate().flat_map(|(i, r)| r.0.chars().enumerate().map(move |(k, c)| (c, i, k))).filter(|l| !l.0.is_whitespace()).collect();
    let want: Vec<char> = word.chars().collect();
    let at = (0..letters.len())
        .filter(|&s| s + want.len() <= letters.len() && letters[s..s + want.len()].iter().map(|l| l.0).eq(want.iter().copied()))
        .nth(nth)
        .unwrap_or_else(|| panic!("no word {word}"));
    let (_, i, k) = letters[if end { at + want.len() - 1 } else { at }];
    let r = &runs[i];
    let n = r.0.chars().count() as f32;
    let edge = r.1 + r.5 * (if end { k as f32 + 1.0 } else { k as f32 }) / n;
    (42.0 + edge + if end { -1.0 } else { 1.0 }, 42.0 + r.2 + 8.0)
}

fn drag(h: &mut Harness, from: (f32, f32), to: (f32, f32)) {
    h.pointer_move(0, from.0, from.1);
    h.press_button(0, from.0, from.1, false);
    h.pointer_move(0, to.0, to.1);
    h.release_button(0, to.0, to.1, false);
    h.advance(ms(400));
}

/// "paragraph here / second one / third " selected: from block 1 offset 6 into block 3 offset 6.
fn across(h: &mut Harness) {
    let (from, to) = (word(h, "paragraph", 0, false), word(h, "paragraph", 1, false));
    drag(h, from, to);
    assert_eq!(h.selection(0), ((1, 6), (3, 6)));
}

#[test]
fn typing_over_a_range_across_paragraphs_leaves_one_merged_paragraph_and_undo_restores_them() {
    let mut h = note(TEXT);
    across(&mut h);
    h.type_text(0, "x");
    assert_eq!(h.note_text(0), "# Title\nfirst xparagraph words");
    assert_eq!(h.selection(0), ((1, 7), (1, 7)));
    assert!(h.sel_rects(0).is_empty());
    assert!(h.wash(0));
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), TEXT);
    // typing goes on at the caret, the selection is gone
    h.type_text(0, "y");
    assert!(h.note_text(0).contains('y'));
}

#[test]
fn ctrl_c_copies_the_selected_text_with_line_breaks_between_paragraphs() {
    let mut h = note(TEXT);
    across(&mut h);
    h.press_ctrl(0, 'c');
    assert_eq!(h.clipboard_text().as_deref(), Some("paragraph here\nsecond one\nthird "));
    assert_eq!(h.note_text(0), TEXT, "copy changes nothing");
    assert_eq!(h.selection(0), ((1, 6), (3, 6)), "and keeps the selection");
}

#[test]
fn ctrl_x_cuts_the_range() {
    let mut h = note(TEXT);
    across(&mut h);
    h.press_ctrl(0, 'x');
    assert_eq!(h.clipboard_text().as_deref(), Some("paragraph here\nsecond one\nthird "));
    assert_eq!(h.note_text(0), "# Title\nfirst paragraph words");
}

#[test]
fn pasting_over_a_range_replaces_it() {
    let mut h = note(TEXT);
    across(&mut h);
    h.paste(0, "A\nB");
    assert_eq!(h.note_text(0), "# Title\nfirst A\nBparagraph words");
    assert_eq!(h.selection(0), ((2, 1), (2, 1)));
}

#[test]
fn backspace_delete_and_enter_act_on_the_range() {
    for (key, want) in [
        (Key::Backspace, "# Title\nfirst paragraph words"),
        (Key::Delete, "# Title\nfirst paragraph words"),
        (Key::Return, "# Title\nfirst \nparagraph words"),
    ] {
        let mut h = note(TEXT);
        across(&mut h);
        h.press(0, key);
        assert_eq!(h.note_text(0), want);
        assert!(h.sel_rects(0).is_empty());
    }
}

#[test]
fn bold_over_a_range_bolds_exactly_that_text_and_the_toolbar_shows_it() {
    let mut h = note(TEXT);
    let (from, to) = (word(&h, "paragraph", 0, false), word(&h, "third", 0, true));
    drag(&mut h, from, to);
    assert_eq!(h.selection(0), ((1, 6), (3, 5)));
    h.press_ctrl(0, 'b');
    assert_eq!(h.note_text(0), "# Title\nfirst **paragraph here**\n**second one**\n**third** paragraph words");
    assert!(h.toolbar_active(0)[0]);
    assert_eq!(h.selection(0), ((1, 6), (3, 5)), "the selection stays");
    assert!(!h.sel_rects(0).is_empty());
    h.press_ctrl(0, 'b');
    assert_eq!(h.note_text(0), TEXT);
}

#[test]
fn the_list_button_toggles_every_paragraph_in_the_range() {
    let mut h = note(TEXT);
    across(&mut h);
    h.press_ctrl_shift(0, 'l');
    assert_eq!(h.note_text(0), "# Title\n- first paragraph here\n- second one\n- third paragraph words");
    assert!(h.toolbar_active(0)[4]);
    h.press_ctrl_shift(0, 'l');
    assert_eq!(h.note_text(0), TEXT);
}

#[test]
fn an_arrow_without_shift_collapses_the_range() {
    let mut h = note(TEXT);
    across(&mut h);
    h.press(0, Key::LeftArrow);
    assert_eq!(h.selection(0), ((1, 6), (1, 6)));
    assert!(h.sel_rects(0).is_empty());
    assert!(h.wash(0));
    across(&mut h);
    h.press(0, Key::RightArrow);
    assert_eq!(h.selection(0), ((3, 6), (3, 6)));
    assert!(h.sel_rects(0).is_empty());
    assert_eq!(h.note_text(0), TEXT);
}

#[test]
fn ctrl_a_is_the_range_from_the_start_of_the_note_to_its_end() {
    let mut h = note(TEXT);
    h.press_ctrl(0, 'a');
    assert_eq!(h.selection(0), ((0, 0), (3, "third paragraph words".len())));
    h.press_ctrl(0, 'c');
    assert_eq!(h.clipboard_text().as_deref(), Some("Title\nfirst paragraph here\nsecond one\nthird paragraph words"));
    h.type_text(0, "x");
    assert_eq!(h.note_text(0), "# x");
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), TEXT);
}

#[test]
fn a_click_on_an_image_selects_the_image_block_and_backspace_removes_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("red.png");
    let px = washi::images::Rgba { width: 300, height: 150, data: (0..300 * 150).flat_map(|_| [200, 30, 60, 255]).collect() };
    std::fs::write(&path, washi::images::encode_png(&px).unwrap()).unwrap();
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "Trip");
    h.choose_picture(&path);
    h.click_image_button(0);
    h.wait_for_images();
    h.advance(ms(600));
    assert_eq!(h.block_kinds(0), ["title", "image", "body"]);
    // both halves of the picture, found by its colour in the rendering
    let (w, _, px) = h.render(0);
    let red: Vec<usize> = px.iter().enumerate().filter(|(_, p)| (p.r as i32 - 200).abs() <= 12 && (p.g as i32 - 30).abs() <= 12 && (p.b as i32 - 60).abs() <= 12).map(|(i, _)| i).collect();
    let (top, bottom) = (red.first().unwrap() / w as usize, red.last().unwrap() / w as usize);
    let x = (red[0] % w as usize) as f32 + 40.0;
    for y in [top + (bottom - top) / 4, top + (bottom - top) * 3 / 4] {
        h.click(0, 5.0, 5.0);
        h.advance(ms(400));
        h.click(0, x, y as f32);
        h.advance(ms(400));
        assert_eq!(h.selection(0), ((1, 0), (1, 0)), "the image block holds the selection (click at y {y})");
        assert_eq!(h.block_kinds(0), ["title", "image", "body"]);
    }
    h.press(0, Key::Backspace);
    assert_eq!(h.block_kinds(0), ["title", "body"]);
}

#[test]
fn a_range_edit_right_after_unsnapshotted_typing_undoes_alone_then_the_typing() {
    let mut h = note(TEXT);
    h.place_caret(0, 1);
    h.advance(ms(5));
    h.type_text(0, "ZZ");
    let typed = h.note_text(0);
    assert_eq!(typed, "# Title\nZZfirst paragraph here\nsecond one\nthird paragraph words");
    // select across paragraphs before the typing pauses long enough to be snapshotted
    let (from, to) = (word(&h, "paragraph", 0, false), word(&h, "paragraph", 1, false));
    h.pointer_move(0, from.0, from.1);
    h.press_button(0, from.0, from.1, false);
    h.pointer_move(0, to.0, to.1);
    h.release_button(0, to.0, to.1, false);
    h.advance(ms(100));
    assert_eq!(h.selection(0).0.0, 1);
    assert_eq!(h.selection(0).1.0, 3);
    h.type_text(0, "x");
    assert_ne!(h.note_text(0), typed);
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), typed, "the range edit alone is undone");
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), TEXT, "then the typing");
}

#[test]
fn ctrl_c_copies_rich_html_and_pasting_preserves_formatting() {
    let mut h = note(TEXT);
    let (from, to) = (word(&h, "paragraph", 0, false), word(&h, "third", 0, true));
    drag(&mut h, from, to);
    assert_eq!(h.selection(0), ((1, 6), (3, 5)));
    h.press_ctrl(0, 'b');
    assert_eq!(h.note_text(0), "# Title\nfirst **paragraph here**\n**second one**\n**third** paragraph words");

    // Copy range with bold formatting
    h.press_ctrl(0, 'c');
    let html = h.clipboard_html().expect("HTML payload in clipboard");
    assert!(html.contains("<b>paragraph here</b>"), "html should contain bold tags: {html}");
    assert!(html.contains("<b>second one</b>"), "html should contain bold tags: {html}");
    assert_eq!(h.clipboard_text().as_deref(), Some("paragraph here\nsecond one\nthird"));

    // Select the entire note and paste rich content over it
    h.press_ctrl(0, 'a');
    h.press_ctrl(0, 'v');
    assert_eq!(h.note_text(0), "# **paragraph here**\n**second one**\n**third**");

    // Undo restores the original note
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), "# Title\nfirst **paragraph here**\n**second one**\n**third** paragraph words");
}

#[test]
fn ctrl_c_and_ctrl_x_on_image_selection_and_undo() {
    let px = washi::images::Rgba { width: 80, height: 80, data: vec![200, 30, 60, 255].repeat(6400) };
    let path = std::env::temp_dir().join("snap_test_image_range_cut.png");
    std::fs::write(&path, washi::images::encode_png(&px).unwrap()).unwrap();
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "Trip");
    h.choose_picture(&path);
    h.click_image_button(0);
    h.wait_for_images();
    h.advance(ms(600));
    assert_eq!(h.block_kinds(0), ["title", "image", "body"]);

    let (w, _, px_rendered) = h.render(0);
    let red: Vec<usize> = px_rendered.iter().enumerate().filter(|(_, p)| (p.r as i32 - 200).abs() <= 12 && (p.g as i32 - 30).abs() <= 12 && (p.b as i32 - 60).abs() <= 12).map(|(i, _)| i).collect();
    let (top, bottom) = (red.first().unwrap() / w as usize, red.last().unwrap() / w as usize);
    let x = (red[0] % w as usize) as f32 + 40.0;
    let y = (top + (bottom - top) / 2) as f32;

    h.click(0, 5.0, 5.0);
    h.advance(ms(400));
    h.click(0, x, y);
    h.advance(ms(400));
    assert_eq!(h.selection(0), ((1, 0), (1, 0)));

    // Ctrl+C copies the image
    h.press_ctrl(0, 'c');
    let img = h.clipboard_image().expect("image on clipboard");
    assert_eq!((img.width, img.height), (80, 80));

    // Ctrl+X cuts the image
    h.press_ctrl(0, 'x');
    assert_eq!(h.block_kinds(0), ["title", "body"]);

    // Ctrl+Z undoes the cut and restores the image
    h.press_ctrl(0, 'z');
    assert_eq!(h.block_kinds(0), ["title", "image", "body"]);
}
