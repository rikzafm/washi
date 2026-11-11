//! The caret an edit field draws is the box of the line the app's own layout gives it, for a note
//! of mixed styles with a picture in the middle of the text, at every monitor scale.

use snap::Harness;
use washi::images::{encode_png, Rgba};

/// A note as exported (its stored text), without the line of its picture.
const NOTE: &str = include_str!("fixtures/mixed-note.md");

/// The note at the default size on a monitor at `scale`, a picture below its eleventh paragraph.
fn note_with_picture(scale: f32) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new();
    h.start().unwrap();
    // the caret against its level line box: the text settles flat (tilted_editing.rs has the tilted caret)
    h.set_tilted_editing(false);
    h.set_note_text(0, NOTE.replace("\r\n", "\n").trim_end_matches('\n'));
    let above = h.blocks(0).iter().position(|(_, text)| text.starts_with(".\\target")).expect("the paragraph above the picture");
    h.place_caret_at(0, above, 0);
    h.drawn_caret(0).expect("the caret is in the paragraph above the picture");
    let png = dir.path().join("picture.png");
    let picture = Rgba { width: 582, height: 486, data: (0..582 * 486).flat_map(|_| [200, 30, 60, 255]).collect() };
    std::fs::write(&png, encode_png(&picture).unwrap()).unwrap();
    h.choose_picture(&png);
    h.click_image_button(0);
    h.wait_for_images();
    assert_eq!(h.blocks(0)[above + 1].0, "image");
    if scale != 1.0 {
        h.move_to_monitor(0, scale);
    }
    h
}

/// Every `step`th caret position of the note (and the end of every paragraph): the caret as drawn
/// against the line box of the layout, both in screen px.
fn check_carets(scale: f32, step: usize) {
    let mut h = note_with_picture(scale);
    let mut checked = 0;
    for (block, (kind, text)) in h.blocks(0).iter().enumerate() {
        if kind == "image" {
            continue;
        }
        let ends: Vec<usize> = (0..=text.len()).filter(|&o| text.is_char_boundary(o)).collect();
        for offset in ends.iter().copied().enumerate().filter(|(i, o)| i % step == 0 || *o == text.len()).map(|(_, o)| o) {
            h.place_caret_at(0, block, offset);
            let (left, top, right, bottom) = h.drawn_caret(0).unwrap_or_else(|| panic!("no caret is drawn at block {block} offset {offset}"));
            let (x, line_top, line_bottom) = h.laid_out_caret(0, block, offset);
            let (x, line_top, line_bottom) = (x * scale, line_top * scale, line_bottom * scale);
            let at = format!("block {block} offset {offset} at scale {scale}: caret x {left}..{right} y {top}..{bottom}, line x {x} y {line_top}..{line_bottom}");
            assert!(right - left <= 3 && (left as f32 - x).abs() <= 1.0, "{at}");
            // the line box, to the pixel its edges round to
            assert!((top as f32 - line_top).abs() <= 1.0 && (bottom as f32 - line_bottom).abs() <= 1.0, "{at}");
            checked += 1;
        }
    }
    assert!(checked > 40, "{checked} positions");
}

#[test]
fn the_caret_is_the_box_of_its_line_on_every_line_of_a_note_with_a_picture() {
    check_carets(1.0, 16);
}

/// The lines of a text block as the app lays them out, each as (top, bottom, x of its last caret
/// position that is on no other line, the first byte offset on it), in the note's window.
fn lines_of(h: &Harness, block: usize, text: &str) -> Vec<(f32, f32, f32, usize)> {
    let mut lines: Vec<(f32, f32, f32, usize)> = vec![];
    for o in (0..=text.len()).filter(|&o| text.is_char_boundary(o)) {
        let (x, top, bottom) = h.laid_out_caret(0, block, o);
        match lines.last_mut() {
            Some(l) if (l.0 - top).abs() < 0.5 => l.2 = l.2.max(x),
            _ => lines.push((top, bottom, x, o)),
        }
    }
    lines
}

/// A click to the right of every line of the note: the caret is drawn at the end of that line. A
/// line that fills the field to its right edge has its end there, and a line that wraps without a
/// space ends at the offset its next line starts at.
#[test]
fn a_click_right_of_a_line_draws_the_caret_at_the_end_of_that_line() {
    let mut h = note_with_picture(1.0);
    let (mut checked, mut shared_ends) = (0, 0);
    for (block, (kind, text)) in h.blocks(0).iter().enumerate() {
        if kind == "image" {
            continue;
        }
        for line in 0..lines_of(&h, block, text).len() {
            // bring the line into view, then find it where it is now
            let first = lines_of(&h, block, text)[line].3;
            h.place_caret_at(0, block, first);
            h.drawn_caret(0).unwrap_or_else(|| panic!("no caret is drawn at block {block} offset {first}"));
            let lines = lines_of(&h, block, text);
            let (top, bottom, end_x, _) = lines[line];
            h.click(0, end_x + 12.0, (top + bottom) / 2.0);
            let (_, (at_block, at_offset)) = h.selection(0);
            let at = format!("block {block} line {line} ({:?}): the caret is at offset {at_offset}", &text[first..]);
            assert_eq!(at_block, block, "{at}");
            let (left, drawn_top, right, drawn_bottom) = h.drawn_caret(0).unwrap_or_else(|| panic!("{at}, and no caret is drawn"));
            let at = format!("{at}, drawn at x {left}..{right} y {drawn_top}..{drawn_bottom}; the line is y {top}..{bottom} and ends at x {end_x}");
            assert!((drawn_top as f32 - top).abs() <= 1.0 && (drawn_bottom as f32 - bottom).abs() <= 1.0, "{at}");
            assert!(right - left <= 3 && left as f32 >= end_x - 1.0 && (left as f32) < end_x + 12.0, "{at}");
            if lines.get(line + 1).is_some_and(|next| next.3 == at_offset) {
                shared_ends += 1;
            }
            checked += 1;
        }
    }
    assert!(checked > 30, "{checked} lines");
    assert!(shared_ends > 0, "the note has a line that wraps without a space");
}

/// Home and End stay on the caret's line, Up and Down keep its column, also where a line wraps
/// without a space. (The lines are told by their bottom: a line at the top of the view is cut
/// along the paper's tilted edge, and its caret with it.)
#[test]
fn home_end_and_the_arrows_move_along_the_lines_the_note_lays_out() {
    use slint::platform::Key;
    let mut h = note_with_picture(1.0);
    let blocks = h.blocks(0);
    let block = blocks.iter().position(|(_, text)| text.starts_with("Wrong folder.")).expect("the paragraph with the path");
    let text = &blocks[block].1;
    h.place_caret_at(0, block, 3);
    h.drawn_caret(0).expect("the caret is in the paragraph");
    let lines = lines_of(&h, block, text);
    assert_eq!(&text[..lines[1].3], "Wrong folder. The exe is at D:", "the first line wraps without a space");

    h.press(0, Key::End);
    assert_eq!(h.selection(0).1, (block, lines[1].3));
    let (left, _, _, bottom) = h.drawn_caret(0).expect("the caret at the end of the first line");
    assert!((bottom as f32 - lines[0].1).abs() <= 1.0 && left as f32 > lines[0].2, "End: x {left} bottom {bottom}, the line ends at y {} and past x {}", lines[0].1, lines[0].2);

    h.press(0, Key::Home);
    assert_eq!(h.selection(0).1, (block, 0));
    h.press(0, Key::End);
    h.press(0, Key::DownArrow);
    let (left, _, _, bottom) = h.drawn_caret(0).expect("the caret on the second line");
    // the second line is shorter: the caret is at its end, and comes back to its column going up
    assert!((bottom as f32 - lines[1].1).abs() <= 1.0 && (left as f32 - lines[1].2).abs() <= 1.0, "Down: x {left} bottom {bottom}, the line ends at x {}", lines[1].2);
    h.press(0, Key::UpArrow);
    let (_, _, _, bottom) = h.drawn_caret(0).expect("the caret back on the first line");
    assert!((bottom as f32 - lines[0].1).abs() <= 1.0, "Up: bottom {bottom}");
    assert_eq!(h.selection(0).1, (block, lines[1].3), "back at the end of the first line");

    // the same offset from the other side: the start of the second line
    h.press(0, Key::DownArrow);
    h.press(0, Key::Home);
    assert_eq!(h.selection(0).1, (block, lines[1].3));
    let (left, _, _, bottom) = h.drawn_caret(0).expect("the caret at the start of the second line");
    assert!((bottom as f32 - lines[1].1).abs() <= 1.0 && (left as f32 - h.laid_out_caret(0, block, 0).0).abs() <= 1.0, "Home: x {left} bottom {bottom}");
}

#[test]
fn the_caret_is_the_box_of_its_line_at_fractional_monitor_scales() {
    check_carets(1.25, 40);
    check_carets(1.5, 40);
}
