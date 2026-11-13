//! What one typed character costs besides layout (ticket 04 of .scratch/layout-cache): no copy of the
//! whole document unless auto-format fires, one caret box, one `to_text`, one window row built, and
//! no picture reload unless the note's pictures changed. Counted with `washi::trace::counts`.

use snap::layout_bench::{focus, generated_note, open};
use snap::Harness;
use std::time::Duration;
use washi::images::Rgba;
use washi::trace::counts::{get, reset, Count};

/// A note of 50 paragraphs with the caret in the middle of one in the middle of it.
fn fifty() -> Harness {
    let h = open(&generated_note());
    let blocks = h.blocks(0);
    let block = blocks.len() / 2;
    let mut mid = blocks[block].1.len() / 2;
    while !blocks[block].1.is_char_boundary(mid) {
        mid += 1;
    }
    focus(&h, block, mid);
    h
}

#[test]
fn a_typed_character_does_not_copy_the_document() {
    let mut h = fifty();
    reset();
    h.type_text(0, "x");
    assert_eq!(get(Count::DocumentClone), 0);
}

#[test]
fn applying_a_change_computes_the_caret_box_once() {
    let h = fifty();
    let id = h.id(0);
    reset();
    h.app().with_note(id, |n| n.editor().notify_change(None));
    assert_eq!(get(Count::CaretBox), 1);
}

#[test]
fn a_typed_character_writes_the_note_out_as_text_once() {
    let mut h = fifty();
    reset();
    h.type_text(0, "x");
    assert_eq!(get(Count::ToText), 1);
}

#[test]
fn a_typed_character_builds_one_window_row_and_loads_no_pictures() {
    let mut h = fifty();
    reset();
    h.type_text(0, "x");
    assert_eq!(get(Count::BlockData), 1);
    assert_eq!(get(Count::ImagesLoad), 0);
}

fn picture() -> Rgba {
    let (w, h) = (30u32, 20u32);
    Rgba { width: w, height: h, data: (0..w * h).flat_map(|_| [200, 30, 60, 255]).collect() }
}

#[test]
fn pictures_load_when_the_set_of_pictures_changes_and_not_otherwise() {
    let mut h = fifty();
    reset();
    h.copy_picture(picture());
    h.press_ctrl(0, 'v');
    h.wait_for_images();
    assert!(get(Count::ImagesLoad) >= 1, "a pasted picture is learned");
    assert!(h.block_kinds(0).iter().any(|k| k == "image"));
    h.type_text(0, "y");
    reset();
    h.type_text(0, "x");
    assert_eq!(get(Count::ImagesLoad), 0);
    assert_eq!(get(Count::BlockData), 1);
}

#[test]
fn auto_format_records_the_text_before_the_space_as_its_undo_step() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "title");
    h.press_enter(0);
    h.type_text(0, "#");
    h.advance(Duration::from_millis(600));
    h.type_text(0, " ");
    assert_eq!(h.note_text(0), "# title\n## ");
    h.press_ctrl(0, 'z');
    assert_eq!(h.note_text(0), "# title\n\\#");
    h.press_ctrl(0, 'y');
    assert_eq!(h.note_text(0), "# title\n## ");
}
