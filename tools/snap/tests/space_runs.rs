//! A run of spaces wider than a line wraps the way the edit fields wrap it, so the paragraphs
//! below it are where they look, and a click on any of them lets you type there, at the smallest
//! window and at a large one (.scratch/caret-focus-fixes, bug 1).

use snap::{ms, Harness};

/// Notes with a long run of spaces in the title, in a body paragraph and in a check item, leading,
/// in the middle and trailing, and the fixes Note whose report found the bug. Each with the index
/// of the block that holds the run (the paragraphs below it are the ones that broke).
fn fixtures() -> Vec<(&'static str, String, usize)> {
    let sp = |n: usize| " ".repeat(n);
    let probe = include_str!("washi_fixes_probe.md").replace("\r\n", "\n");
    vec![
        (
            "title",
            format!("#{}Washi Fixes\n\n**OTHER**\n- [ ] an item long enough to wrap onto a second line at the smallest size\n- [ ] second item\nA body paragraph after the items.", sp(65)),
            0,
        ),
        ("body", format!("# Title\nBefore the run\nleft{}right\n- [ ] item one\n- [ ] item two\nAfter one\nAfter two", sp(150)), 2),
        ("check item", format!("# Title\n- [ ] left{}right\n- [ ] item\nbody\nbody two", sp(150)), 1),
        ("leading", format!("# Title\n{}lead\n- [ ] item\nafter", sp(150)), 1),
        ("trailing", format!("# Title\ntrail{}\n- [ ] item\nafter", sp(150)), 1),
        ("mixed", format!("# T\n  two  leading, double  spaces{}word{}\nafter\n- [ ] item", sp(70), sp(70)), 1),
        ("fixes probe", probe.trim_end().to_string(), 0),
    ]
}

/// The bottom of the edit fields and of the app's layout for the note made of the first `n`
/// paragraphs of `text`.
fn bottoms(h: &mut Harness, text: &str, n: usize) -> (f32, f32) {
    let prefix: Vec<&str> = text.split('\n').take(n).collect();
    h.set_note_text(0, &prefix.join("\n"));
    h.advance(ms(5));
    h.fields_and_layout_bottom(0)
}

#[test]
fn every_paragraph_takes_as_many_lines_as_its_edit_field_at_the_smallest_and_a_large_window() {
    for size in [300, 1000] {
        for (name, text, _) in fixtures() {
            let mut h = Harness::sized_note(size, size, &text);
            let blocks = h.blocks(0).len();
            assert_eq!(blocks, text.split('\n').count(), "{name}: one block per stored line");
            // the notes of the first n paragraphs: their fields and their layout end at the same
            // place for every n, so every paragraph has as many lines in both
            for n in 1..=blocks {
                let (fields, layout) = bottoms(&mut h, &text, n);
                assert!((fields - layout).abs() < 0.5, "{name} at {size} px, paragraphs 0..{n}: the fields end at {fields}, the layout at {layout}");
            }
        }
    }
}

#[test]
fn the_spaces_are_kept() {
    for (name, text, _) in fixtures() {
        let h = Harness::sized_note(300, 300, &text);
        assert_eq!(h.note_text(0).replace("\r\n", "\n").trim_end_matches('\n'), text.trim_end_matches('\n'), "{name}");
    }
}

/// The lines of `block` as the layout has them: for each, the window y of its middle and the window
/// x of its first and last caret position (the same as the drawn words, at rest or flat).
fn lines_of(h: &Harness, block: usize) -> Vec<(f32, f32, f32)> {
    let text = h.blocks(0)[block].1.clone();
    let k = h.text_tilt_scale(0);
    let mut lines: Vec<(f32, f32, f32, f32)> = vec![];
    for o in (0..=text.len()).filter(|&o| text.is_char_boundary(o)) {
        let (lx, top, bottom) = h.laid_out_caret(0, block, o);
        let (tx, ttop, tbottom) = h.tilted_caret(0, block, o);
        let x = lx + (tx - lx) * k;
        let y = (top + bottom) / 2.0 + ((ttop + tbottom) / 2.0 - (top + bottom) / 2.0) * k;
        match lines.last_mut() {
            Some(l) if (l.0 - top).abs() < 0.5 => l.3 = x,
            _ => lines.push((top, y, x, x)),
        }
    }
    lines.into_iter().map(|(_, y, a, b)| (y, a, b)).collect()
}

/// Scroll the note back to its top (a new text does not reset the scroll).
fn to_top(h: &mut Harness) {
    h.wheel(0, 200.0, 200.0, 10_000.0);
    h.advance(ms(5));
}

/// The UI scales (monitor DPI / 96) the clicks are made at: user story 28.
const SCALES: [f32; 4] = [1.0, 1.25, 1.5, 2.0];

#[test]
fn a_click_on_any_line_below_a_run_of_spaces_lets_you_type_there_at_the_smallest_window() {
    clicks_below_a_run_of_spaces_type_there(SCALES[0]);
}

#[test]
fn a_click_below_a_run_of_spaces_types_there_at_125_percent() {
    clicks_below_a_run_of_spaces_type_there(SCALES[1]);
}

#[test]
fn a_click_below_a_run_of_spaces_types_there_at_150_percent() {
    clicks_below_a_run_of_spaces_type_there(SCALES[2]);
}

#[test]
fn a_click_below_a_run_of_spaces_types_there_at_200_percent() {
    clicks_below_a_run_of_spaces_type_there(SCALES[3]);
}

/// At the smallest window on a monitor at `scale`: a click on every line of every paragraph
/// below a run of spaces focuses that paragraph's field, and a typed character lands in it.
fn clicks_below_a_run_of_spaces_type_there(scale: f32) {
    for (name, text, run) in fixtures() {
        let mut h = Harness::sized_note(300, 300, &text);
        if scale != 1.0 {
            h.move_to_monitor(0, scale);
        }
        let name = format!("{name} at {scale}x");
        let (_, height) = h.note_size(0);
        let blocks = h.blocks(0);
        let mut clicked = 0;
        for block in run + 1..blocks.len() {
            if blocks[block].0 == "image" {
                continue;
            }
            for line in 0..lines_of(&h, block).len() {
                h.set_note_text(0, &text);
                h.advance(ms(5));
                to_top(&mut h);
                let before = h.blocks(0);
                // scroll the line to near the top of the text view
                let (y, _, _) = lines_of(&h, block)[line];
                h.wheel(0, 200.0, 200.0, 120.0 - y);
                h.advance(ms(5));
                let (y, x0, x1) = lines_of(&h, block)[line];
                let at = format!("{name}: block {block} line {line} at y {y}");
                assert!(y > 50.0 && y < height as f32 - 90.0, "{at}: the line is in view");
                h.click(0, (x0 + x1) / 2.0 + 1.0, y);
                h.advance(ms(400));
                assert_eq!(h.text_focused_block(0), block as i32, "{at}: the clicked paragraph's field has the focus");
                h.type_text(0, "Z");
                h.advance(ms(5));
                let after = h.blocks(0);
                for (i, (b, a)) in before.iter().zip(&after).enumerate() {
                    if i == block {
                        assert_eq!(a.1.replacen('Z', "", 1), b.1, "{at}: the Z went into the clicked paragraph");
                        assert!(a.1.contains('Z'), "{at}: the Z went into the clicked paragraph");
                    } else {
                        assert_eq!(a.1, b.1, "{at}: paragraph {i} is unchanged");
                    }
                }
                clicked += 1;
            }
        }
        assert!(clicked >= 2, "{name}: {clicked} lines clicked");
    }
}

#[test]
fn a_caret_in_or_after_a_run_of_spaces_is_drawn_on_its_line_and_types_where_it_is() {
    let sp = " ".repeat(150);
    let text = format!("# Title\nleft{sp}right\nbelow");
    let mut h = Harness::sized_note(300, 300, &text);
    h.set_tilted_editing(false);
    let block = 1;
    let run_end = "left".len() + sp.len();
    let lines = lines_of(&h, block);
    assert!(lines.len() >= 3, "the run takes lines of its own: {lines:?}");
    // a click on each line, then Home, End, Up and Down from there
    let moves: [Option<slint::platform::Key>; 5] = [None, Some(slint::platform::Key::Home), Some(slint::platform::Key::End), Some(slint::platform::Key::UpArrow), Some(slint::platform::Key::DownArrow)];
    let mut checked = 0;
    for line in 0..lines.len() {
        for key in &moves {
            h.set_note_text(0, &text);
            h.advance(ms(5));
            to_top(&mut h);
            let (y, x0, x1) = lines_of(&h, block)[line];
            h.wheel(0, 200.0, 200.0, 140.0 - y);
            h.advance(ms(5));
            let (y, x0b, x1b) = lines_of(&h, block)[line];
            let _ = (x0, x1);
            h.click(0, (x0b + x1b) / 2.0, y);
            h.advance(ms(400));
            if let Some(k) = key {
                h.press(0, k.clone());
                h.advance(ms(5));
            }
            let at = format!("line {line} after {key:?}");
            let (_, (b, offset)) = h.selection(0);
            assert_eq!(h.text_focused_block(0), b as i32, "{at}");
            // the scrollbar the wheel showed fades out first
            h.advance_frames(ms(3000));
            let (drawn_l, drawn_t, _, drawn_b) = h.drawn_caret(0).unwrap_or_else(|| panic!("{at}: a caret is drawn"));
            let (x, top, bottom) = h.laid_out_caret(0, b, offset);
            assert!((drawn_l as f32 - x).abs() <= 1.0, "{at}: drawn at x {drawn_l}, laid out at {x}");
            assert!((drawn_t as f32 - top).abs() <= 1.0 && (drawn_b as f32 - bottom).abs() <= 1.0, "{at}: drawn {drawn_t}..{drawn_b}, line {top}..{bottom}");
            let (w, _) = h.note_size(0);
            assert!(x < w as f32, "{at}: the caret is on the paper");
            // what is typed goes where the caret is
            let before = h.blocks(0)[b].1.clone();
            h.type_text(0, "Z");
            h.advance(ms(5));
            let mut want = before.clone();
            want.insert(offset, 'Z');
            assert_eq!(h.blocks(0)[b].1, want, "{at}: typed at offset {offset}");
            checked += 1;
        }
    }
    // and right after the run, from the keyboard
    h.set_note_text(0, &text);
    h.advance(ms(5));
    h.place_caret_at(0, block, run_end);
    let (_, t, _, b) = h.drawn_caret(0).expect("a caret right after the run");
    let (_, top, bottom) = h.laid_out_caret(0, block, run_end);
    assert!((t as f32 - top).abs() <= 1.0 && (b as f32 - bottom).abs() <= 1.0);
    assert!(checked >= 15);
}
