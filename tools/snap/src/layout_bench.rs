//! What one input costs in layout work: the scenarios behind `bin/layout_bench` (wall time) and
//! `tests/layout_wraps.rs` (pinned `textflow::wrap` counts).
//!
//! Each scenario opens a note headless, drives one kind of input `iterations` times, and records for
//! every input the wall time and the number of `wrap` calls it caused.

use crate::Harness;
use slint::platform::Key;
use std::time::{Duration, Instant};
use washi::images::Rgba;
use washi::textflow;
use washi::trace::counts::{self, Count};

/// One kind of input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// One character typed in the middle of a paragraph.
    Keystroke,
    /// The Down arrow, from the start of the note.
    Down,
    /// The pointer moving over a selected picture.
    PointerOverImage,
    /// The paper getting 10 px wider (and back, on every other step).
    ResizeStep,
}

impl Op {
    pub const ALL: [Op; 4] = [Op::Keystroke, Op::Down, Op::PointerOverImage, Op::ResizeStep];

    pub fn name(self) -> &'static str {
        match self {
            Op::Keystroke => "typed character",
            Op::Down => "Down arrow",
            Op::PointerOverImage => "pointer over image",
            Op::ResizeStep => "10 px resize",
        }
    }
}

/// A note to measure on.
pub struct Fixture {
    pub name: &'static str,
    pub text: String,
}

/// `docs/design/fixtures/mockup.md`, `long.md` and a generated note of 50 paragraphs.
pub fn fixtures() -> Vec<Fixture> {
    let read = |s: &str| s.replace("\r\n", "\n").trim_end_matches('\n').to_string();
    vec![
        Fixture { name: "mockup.md", text: read(include_str!("../../../docs/design/fixtures/mockup.md")) },
        Fixture { name: "long.md", text: read(include_str!("../../../docs/design/fixtures/long.md")) },
        Fixture { name: "50 paragraphs", text: generated_note() },
    ]
}

/// A title and 49 body paragraphs of about 250 characters (3 to 4 lines each), deterministic.
pub fn generated_note() -> String {
    const WORDS: [&str; 12] = ["paper", "tape", "marker", "folded", "corner", "ink", "never", "rest", "white", "night", "nova", "cutter"];
    let mut text = String::from("# Fifty paragraphs");
    for p in 1..50 {
        text.push('\n');
        for w in 0..40 {
            if w > 0 {
                text.push(' ');
            }
            text.push_str(WORDS[(p * 7 + w * 5 + w / 3) % WORDS.len()]);
        }
    }
    text
}

/// What the inputs of one scenario cost, one entry per input.
pub struct Sample {
    pub times: Vec<Duration>,
    pub wraps: Vec<usize>,
    /// `Layout::build` calls per input.
    pub builds: Vec<usize>,
}

impl Sample {
    pub fn median_time(&self) -> Duration {
        percentile(&self.times, 50)
    }

    pub fn p95_time(&self) -> Duration {
        percentile(&self.times, 95)
    }

    /// The wrap count of the median input.
    pub fn median_wraps(&self) -> usize {
        percentile(&self.wraps, 50)
    }

    /// The most `Layout::build` calls any input caused.
    pub fn max_builds(&self) -> usize {
        self.builds.iter().copied().max().unwrap_or(0)
    }

    pub fn min_builds(&self) -> usize {
        self.builds.iter().copied().min().unwrap_or(0)
    }

    pub fn max_wraps(&self) -> usize {
        self.wraps.iter().copied().max().unwrap_or(0)
    }
}

fn percentile<T: Ord + Copy + Default>(v: &[T], p: usize) -> T {
    let mut v = v.to_vec();
    v.sort();
    v.get(((v.len() * p).div_ceil(100)).saturating_sub(1)).copied().unwrap_or_default()
}

/// A flat picture, for the pointer scenario.
fn picture() -> Rgba {
    let (w, h) = (300u32, 150u32);
    Rgba { width: w, height: h, data: (0..w * h).flat_map(|_| [200, 30, 60, 255]).collect() }
}

pub fn open(text: &str) -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, text);
    let _ = h.render(0); // drawing creates the text fields
    h.advance(Duration::from_millis(5));
    h.settle();
    h
}

/// Give the first field of `block` the caret at byte `offset`.
pub fn focus(h: &Harness, block: usize, offset: usize) {
    let id = h.id(0);
    h.app().with_note(id, |n| {
        let ui = n.ui();
        ui.set_focus_block(block as i32);
        ui.set_focus_anchor(-1);
        ui.set_focus_offset(offset as i32);
        ui.set_focus_serial(ui.get_focus_serial() + 1);
    });
    h.advance_quiet();
}

impl Harness {
    fn advance_quiet(&self) {
        crate::platform::advance(Duration::ZERO);
    }
}

/// Run `op` `iterations` times on a note holding `text`.
pub fn run(text: &str, op: Op, iterations: usize) -> Sample {
    let mut h = open(text);
    let mut sample = Sample { times: vec![], wraps: vec![], builds: vec![] };
    let timed =|sample: &mut Sample, f: &mut dyn FnMut()| {
        counts::reset();
        let start = Instant::now();
        f();
        sample.times.push(start.elapsed());
        sample.wraps.push(counts::get(Count::Wrap));
        sample.builds.push(counts::get(Count::LayoutBuild));
    };
    match op {
        Op::Keystroke => {
            let blocks = h.blocks(0);
            // a body paragraph in the middle of the note, at a char boundary in the middle of it
            let block = (blocks.len() / 2).max(1).min(blocks.len() - 1);
            let mut mid = blocks[block].1.len() / 2;
            while !blocks[block].1.is_char_boundary(mid) {
                mid += 1;
            }
            focus(&h, block, mid);
            for _ in 0..iterations {
                let before = h.note_text(0);
                timed(&mut sample, &mut || h.type_text(0, "x"));
                assert_eq!(h.note_text(0).len(), before.len() + 1, "the typed character reached the note");
                h.press(0, Key::Backspace);
            }
        }
        Op::Down => {
            for _ in 0..iterations {
                focus(&h, 0, 0);
                let before = h.selection(0);
                timed(&mut sample, &mut || h.press(0, Key::DownArrow));
                assert_ne!(h.selection(0), before, "the caret moved down");
            }
        }
        Op::PointerOverImage => {
            let last = h.blocks(0).len() - 1;
            focus(&h, last, 0);
            h.copy_picture(picture());
            h.press_ctrl(0, 'v');
            h.wait_for_images();
            let image = h.block_kinds(0).iter().position(|k| k == "image").expect("the picture was pasted");
            h.place_caret_at(0, image, 0);
            h.advance(Duration::from_millis(600));
            let (cx, cy, hw, hh) = h
                .app()
                .with_note(h.id(0), |n| {
                    let (ui, editor) = (n.ui(), n.editor());
                    let r = editor.layout(ui).image_rect(image).unwrap();
                    let (margin, scroll) = (ui.get_window_margin() as f64, ui.get_scroll_y() as f64);
                    ((margin + r.x + r.width / 2.0) as f32, (margin + r.y - scroll + r.height / 2.0) as f32, (r.width / 4.0) as f32, (r.height / 4.0) as f32)
                })
                .unwrap();
            for i in 0..iterations {
                let (x, y) = if i % 2 == 0 { (cx - hw, cy - hh) } else { (cx + hw, cy + hh) };
                timed(&mut sample, &mut || h.pointer_move(0, x, y));
            }
        }
        Op::ResizeStep => {
            let (w, hgt) = h.note_size(0);
            for i in 0..iterations {
                let width = if i % 2 == 0 { w + 10 } else { w };
                timed(&mut sample, &mut || {
                    h.resize_note(0, width, hgt);
                    h.advance_quiet();
                });
            }
        }
    }
    sample
}

/// The median time of one `Layout::build` of the note `text` (as the editor builds it: its blocks,
/// paper, tokens and picture sizes), over `iterations` builds. What a keystroke spends in the layout
/// at least, until a wrap cache: compare it with the typed character's median time.
pub fn build_time(text: &str, iterations: usize) -> Duration {
    let h = open(text);
    let mut times = vec![];
    h.app().with_note(h.id(0), |n| {
        let editor = n.editor();
        let layout = editor.layout(n.ui());
        let (paper, tokens) = (layout.paper(), *layout.tokens());
        for _ in 0..iterations {
            let start = Instant::now();
            let built = textflow::Layout::build(editor.doc().blocks(), paper, &tokens, &|b| editor.images.size(b));
            times.push(start.elapsed());
            std::hint::black_box(built);
        }
    });
    percentile(&times, 50)
}
