//! Tilted editing (Settings › General, on by default): while a note is edited its text stays on the
//! paper's tilted baseline, and so do the caret and the selection's wash; the pointer is mapped back
//! through the tilt. Off, the text settles flat for editing.

use snap::Harness;
use std::time::Duration;
use washi::storage::Geometry;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// A title and a paragraph of eight words that wraps over three lines.
const TEXT: &str = "# Title\nalpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike november oscar papa quebec romeo";

fn note() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, TEXT);
    let _ = h.render(0); // drawing creates the text fields
    h.advance(ms(5));
    h
}

/// Let a focus change settle (the settle animation runs on drawn frames).
fn settle_text(h: &mut Harness) {
    h.advance(ms(1));
    let _ = h.text_tilt_scale(0);
    h.advance(ms(500));
    let _ = h.text_tilt_scale(0);
}

/// The window point at the left edge of the first letter of the `nth` resting `word`, where the
/// tilted baseline draws it (the level place plus the run's offset onto the baseline).
fn drawn_word_start(h: &Harness, word: &str) -> (f32, f32) {
    let runs = h.text_runs(0);
    let letters: Vec<(char, usize, usize)> =
        runs.iter().enumerate().flat_map(|(i, r)| r.0.chars().enumerate().map(move |(k, c)| (c, i, k))).filter(|l| !l.0.is_whitespace()).collect();
    let want: Vec<char> = word.chars().collect();
    let at = (0..letters.len())
        .find(|&s| s + want.len() <= letters.len() && letters[s..s + want.len()].iter().map(|l| l.0).eq(want.iter().copied()))
        .unwrap_or_else(|| panic!("no word {word}"));
    let (_, i, k) = letters[at];
    let r = &runs[i];
    let edge = r.1 + r.5 * k as f32 / r.0.chars().count() as f32;
    // the paper's frame starts 42 px in from the window's edge (PaperTheme.margin); a line is 29 px tall
    (42.0 + edge + r.3 + 1.5, 42.0 + r.2 + r.4 + 14.0)
}

#[test]
fn focusing_a_note_keeps_its_text_on_the_tilted_baseline() {
    let mut h = note();
    h.set_window_active(0, false);
    settle_text(&mut h);
    assert_eq!(h.text_tilt_scale(0), 1.0);
    let rest = h.text_runs(0);

    h.set_window_active(0, true);
    settle_text(&mut h);
    assert!(h.text_focused_block(0) >= 0, "a field has the focus");
    assert_eq!(h.text_tilt_scale(0), 1.0, "tilted editing: nothing settles flat");
    assert_eq!(h.text_runs(0), rest, "the words did not move");
}

#[test]
fn with_tilted_editing_off_focusing_a_note_settles_it_flat() {
    let mut h = note();
    h.set_tilted_editing(false);
    h.set_window_active(0, false);
    settle_text(&mut h);
    h.set_window_active(0, true);
    settle_text(&mut h);
    assert_eq!(h.text_tilt_scale(0), 0.0);
}

#[test]
fn switching_the_option_changes_a_note_that_is_being_edited_at_once() {
    let mut h = note();
    h.set_window_active(0, true);
    settle_text(&mut h);
    assert_eq!(h.text_tilt_scale(0), 1.0);
    h.set_tilted_editing(false);
    settle_text(&mut h);
    assert_eq!(h.text_tilt_scale(0), 0.0, "flat from now on");
    h.set_tilted_editing(true);
    settle_text(&mut h);
    assert_eq!(h.text_tilt_scale(0), 1.0, "tilted again");
}

#[test]
fn a_press_on_a_tilted_word_puts_the_caret_in_that_word_wherever_the_word_is_on_the_paper() {
    let mut h = note();
    h.set_window_active(0, true);
    settle_text(&mut h);
    let body = TEXT.split('\n').nth(1).unwrap();
    // a press exactly on the tilted place of a caret position puts the caret there: positions along
    // all three lines (the tilt moves a point the more the farther it is from the paper's center)
    let mut moved = 0;
    for offset in (0..=body.len()).step_by(7) {
        let (x, top, bottom) = h.tilted_caret(0, 1, offset);
        h.click(0, x, (top + bottom) / 2.0);
        assert_eq!(h.selection(0), ((1, offset), (1, offset)), "a press on the tilted caret position {offset} at x {x}");
        assert_eq!(h.text_tilt_scale(0), 1.0, "still tilted");
        // the same press at the level place of that position (where it would be without the tilt)
        let (level_x, level_top, level_bottom) = h.laid_out_caret(0, 1, offset);
        h.click(0, level_x, (level_top + level_bottom) / 2.0);
        moved += usize::from(h.selection(0) != ((1, offset), (1, offset)));
    }
    assert!(moved >= 1, "the test tells the tilted place from the level one: {moved}");
}

#[test]
fn a_press_on_a_tilted_word_lands_on_that_word() {
    let mut h = note();
    h.set_window_active(0, true);
    settle_text(&mut h);
    let body = TEXT.split('\n').nth(1).unwrap();
    // by where each word is drawn (its run's offset onto the baseline), within a letter
    for word in ["alpha", "foxtrot", "november", "romeo"] {
        let (x, y) = drawn_word_start(&h, word);
        h.click(0, x, y);
        let (_, (block, at)) = h.selection(0);
        let want = body.find(word).unwrap();
        assert!(block == 1 && at.abs_diff(want) <= 2, "a press on {word} at ({x}, {y}) is at offset {at}, the word starts at {want}");
    }
}

#[test]
fn a_drag_over_the_tilted_words_selects_exactly_them_and_the_note_washes_it_tilted() {
    let mut h = note();
    h.set_window_active(0, true);
    settle_text(&mut h);
    let body = TEXT.split('\n').nth(1).unwrap();
    let from = drawn_word_start(&h, "bravo");
    let to = drawn_word_start(&h, "lima");
    h.pointer_move(0, from.0, from.1);
    h.press_button(0, from.0, from.1, false);
    h.pointer_move(0, to.0, to.1);
    h.release_button(0, to.0, to.1, false);
    h.advance(ms(400));
    assert_eq!(h.selection(0), ((1, body.find("bravo").unwrap()), (1, body.find("lima").unwrap())));
    // inside one block, and still washed by the note: one box per line the selection touches
    let rects = h.sel_rects(0);
    assert!(rects.len() >= 2, "a wash box for every line of the selection: {rects:?}");
    assert!(h.wash(0), "the selection stays inside one block");
    assert_eq!(h.text_tilt_scale(0), 1.0);
}

#[test]
fn a_selection_inside_one_block_is_washed_by_its_field_when_the_text_settles_flat() {
    let mut h = note();
    h.set_tilted_editing(false);
    h.set_window_active(0, true);
    settle_text(&mut h);
    h.place_caret_at(0, 1, 0);
    h.press_shift(0, slint::platform::Key::RightArrow, 5);
    assert!(h.sel_rects(0).is_empty(), "flat editing: the field washes a selection inside a block");
    // turning the option on makes the note wash it
    h.set_tilted_editing(true);
    assert_eq!(h.sel_rects(0).len(), 1, "{:?}", h.sel_rects(0));
}

#[test]
fn shift_arrows_inside_a_block_are_washed_by_the_note_and_a_collapsed_selection_is_not() {
    let mut h = note();
    h.set_window_active(0, true);
    settle_text(&mut h);
    h.place_caret_at(0, 1, 0);
    assert!(h.sel_rects(0).is_empty(), "a caret has no wash");
    h.press_shift(0, slint::platform::Key::RightArrow, 5);
    assert_eq!(h.selection(0), ((1, 0), (1, 5)));
    assert_eq!(h.sel_rects(0).len(), 1, "{:?}", h.sel_rects(0));
    h.press(0, slint::platform::Key::LeftArrow);
    assert!(h.sel_rects(0).is_empty(), "the selection collapsed");
}

#[test]
fn the_caret_is_drawn_on_the_tilted_baseline_like_the_words() {
    let mut h = note();
    h.set_window_active(0, true);
    settle_text(&mut h);
    let body = TEXT.split('\n').nth(1).unwrap();
    let mut checked = 0;
    // the start, the middle and the end of the paragraph, which wraps over three lines
    for offset in [0, body.find("foxtrot").unwrap(), body.find("lima").unwrap(), body.find("papa").unwrap(), body.len()] {
        h.place_caret_at(0, 1, offset);
        let (left, top, right, bottom) = h.drawn_caret(0).unwrap_or_else(|| panic!("no caret is drawn at offset {offset}"));
        let (x, line_top, line_bottom) = h.tilted_caret(0, 1, offset);
        let at = format!("offset {offset}: caret x {left}..{right} y {top}..{bottom}, tilted line x {x} y {line_top}..{line_bottom}");
        assert!(right - left <= 3 && (left as f32 - x).abs() <= 1.5, "{at}");
        assert!((top as f32 - line_top).abs() <= 1.5 && (bottom as f32 - line_bottom).abs() <= 1.5, "{at}");
        // and it is not where a level caret would be: the far corners of the paper move several px
        let (level_x, level_top, _) = h.laid_out_caret(0, 1, offset);
        checked += usize::from((x - level_x).abs() > 1.5 || (line_top - level_top).abs() > 1.5);
    }
    assert!(checked >= 3, "the tilt moved the caret away from its level place {checked} times");
}

#[test]
fn the_option_is_in_general_defaults_to_on_persists_and_reaches_open_notes() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    assert!(h.is_settings_open());
    assert!(h.settings_tilted_editing(), "on by default");

    h.settings_press_tilted_editing();
    assert!(!h.settings_tilted_editing());
    h.settings_close();
    h.advance_frames(ms(600));
    h.set_window_active(0, true);
    settle_text(&mut h);
    assert_eq!(h.text_tilt_scale(0), 0.0, "the open note settles flat");

    h.restart().unwrap();
    h.set_window_active(0, true);
    settle_text(&mut h);
    assert_eq!(h.text_tilt_scale(0), 0.0, "the choice survives a restart");
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    assert!(!h.settings_tilted_editing());
    h.settings_press_tilted_editing();
    assert!(h.settings_tilted_editing());
    h.settings_close();
    h.advance_frames(ms(600));
    settle_text(&mut h);
    assert_eq!(h.text_tilt_scale(0), 1.0, "tilted again, live");
}
