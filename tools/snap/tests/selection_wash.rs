//! Ticket 03 of cross-paragraph selection: the wash of a selection is laid out by Rust
//! (`Layout::selection_rects`) and drawn from the `sel-rects` model. Ctrl+A fills it.

use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

const TEXT: &str = "Endfield Note\n\nThermite Cutter : POG & Akekuri and a line that has to wrap around the paper\n\nlast words";

fn note(text: &str) -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, text);
    let _ = h.render(0);
    h.advance(ms(5));
    h
}

/// Pixels of the selection wash (#BAD3FC) in the rendered note, and their bounding box.
fn wash(h: &Harness) -> (usize, (u32, u32, u32, u32)) {
    let (w, hh, buf) = h.render(0);
    let (mut n, mut bb) = (0, (u32::MAX, u32::MAX, 0, 0));
    for y in 0..hh {
        for x in 0..w {
            let p = buf[(y * w + x) as usize];
            if [p.r, p.g, p.b].iter().zip([0xBA, 0xD3, 0xFC]).all(|(&a, b)| (a as i32 - b).abs() <= 3) {
                n += 1;
                bb = (bb.0.min(x), bb.1.min(y), bb.2.max(x + 1), bb.3.max(y + 1));
            }
        }
    }
    (n, bb)
}

#[test]
fn ctrl_a_washes_the_note_as_before() {
    let mut h = note(TEXT);
    assert_eq!(wash(&h).0, 0, "nothing selected, no wash");
    h.press_ctrl(0, 'a');
    h.advance(ms(400));
    let (n, bb) = wash(&h);
    // measured on the per-word wash this replaces: 25620 px inside (74, 80, 363, 322)
    // (the new wash is one box per line, so the gaps between words and the wrap pad are washed
    // too: about 9% more)
    assert!((n as f64 - 25620.0).abs() < 25620.0 * 0.12, "{n} wash pixels");
    for (got, want) in [bb.0, bb.1, bb.2, bb.3].into_iter().zip([74, 80, 363, 322]) {
        assert!((got as i64 - want).abs() <= 6, "wash box {bb:?}");
    }
}

#[test]
fn ctrl_a_fills_the_wash_model_one_box_per_line_and_a_sliver_for_each_empty_paragraph() {
    let mut h = note(TEXT);
    assert!(h.sel_rects(0).is_empty());
    h.press_ctrl(0, 'a');
    h.advance(ms(400));
    let r = h.sel_rects(0);
    // title, empty, body (several lines), empty, last
    assert!(r.len() >= 6, "{r:?}");
    let empty_lines: Vec<_> = r.iter().filter(|b| b.2 < 10.0 && b.2 > 1.0).collect();
    assert_eq!(empty_lines.len(), 2, "a sliver for each empty paragraph: {r:?}");
    // the last line stops at its last character: no pad
    let last = r.last().unwrap();
    assert!(last.2 > 20.0);
}

#[test]
fn the_wash_goes_when_the_select_all_ends() {
    let mut h = note(TEXT);
    h.press_ctrl(0, 'a');
    h.advance(ms(400));
    assert!(!h.sel_rects(0).is_empty());
    h.press(0, slint::platform::Key::RightArrow);
    h.advance(ms(400));
    assert!(h.sel_rects(0).is_empty());
    assert_eq!(wash(&h).0, 0);
}
