//! Ticket 02 of text-color-and-highlight: ink colors the words, a marker is a rounded rect behind
//! them. Both are drawn at rest (tilted, in pieces) and while editing (level), under the selection
//! wash, with no seams in a marker.
use slint::platform::Key;
use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// A note with a title and two colored body lines: blue ink on "blue" and, on the next line, a
/// yellow marker on "highlighted stretch" and both a red ink and a green marker on "both".
const TEXT: &str = "title\nplain <c=blue>blue</c> end\n<m=yellow>highlighted stretch</m> <c=red><m=green>both</c></m>";

fn note(text: &str, editing: bool) -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, text);
    let _ = h.render(0);
    h.advance(ms(5));
    h.set_window_active(0, editing);
    h.advance(ms(1));
    let _ = h.text_tilt_scale(0);
    h.advance(ms(500));
    let _ = h.text_tilt_scale(0);
    h
}

/// One drawn frame. (A frame is only drawn when something changed: take one per state.)
struct Shot {
    w: u32,
    h: u32,
    buf: Vec<slint::Rgb8Pixel>,
}

impl Shot {
    fn of(h: &Harness) -> Shot {
        let (w, hh, buf) = h.render(0);
        Shot { w, h: hh, buf }
    }

    /// Pixels within `tol` of `rgb`: `(x, y)` of each.
    fn pixels_of(&self, rgb: u32, tol: i32) -> Vec<(u32, u32)> {
        let want = [(rgb >> 16) as i32, (rgb >> 8 & 255) as i32, (rgb & 255) as i32];
        let mut out = vec![];
        for y in 0..self.h {
            for x in 0..self.w {
                let p = self.buf[(y * self.w + x) as usize];
                if [p.r, p.g, p.b].iter().zip(want).all(|(&a, b)| (a as i32 - b).abs() <= tol) {
                    out.push((x, y));
                }
            }
        }
        out
    }
}

#[test]
fn the_run_model_carries_each_runs_ink_and_the_markers_their_color() {
    let h = note(TEXT, false);
    let runs = h.text_runs(0);
    let inks = h.run_inks(0);
    assert_eq!(runs.len(), inks.len());
    // words are drawn in pieces, so look a letter up: the run that holds it, in the order of the text
    let letters: Vec<usize> = runs.iter().enumerate().flat_map(|(i, r)| r.0.chars().map(move |_| i)).collect();
    let flat: String = runs.iter().map(|r| r.0.as_str()).collect();
    let run_of = |s: &str, at: usize| letters[flat.find(s).unwrap_or_else(|| panic!("no {s:?} in {flat:?}")) + at];
    let ink_of = |s: &str| inks[run_of(s, 0)];
    assert_eq!(ink_of("plain"), 0x2B2F36);
    assert_eq!(ink_of("blue"), 0x3A6D96);
    assert_eq!(ink_of("highlighted"), 0x2B2F36, "a marker alone leaves the ink");
    assert_eq!(ink_of("both"), 0xB8453D);
    assert_eq!(ink_of("end"), 0x2B2F36);
    // a piece never mixes inks: every piece of "blue" is blue
    assert!((0..4).all(|k| inks[run_of("blue", k)] == 0x3A6D96));
    // one marker per colored stretch: yellow under the phrase, green under "both"
    let markers = h.markers(0);
    assert_eq!(markers.iter().map(|m| m.4).collect::<Vec<_>>(), [0xFDECC8, 0xDBEDDB]);
    // under the words: the phrase's marker spans from its first piece to its last (the space between
    // the two words is inside it) and is the line box (29 px at scale 1) minus 2 px
    let (first, last) = (&runs[run_of("highlightedstretch", 0)], &runs[run_of("highlightedstretch", 17)]);
    let m = markers[0];
    assert!((m.0 - first.1).abs() < 0.01 && (m.0 + m.2 - (last.1 + last.5)).abs() < 0.01, "{m:?}");
    assert!((m.1 - (first.2 + 1.0)).abs() < 0.01 && (m.3 - (29.0 - 2.0)).abs() < 0.01, "{m:?}");
}

#[test]
fn a_marker_is_drawn_behind_the_ink_at_rest_and_while_editing_with_no_seams() {
    for editing in [false, true] {
        let s = Shot::of(&note(TEXT, editing));
        let yellow = s.pixels_of(0xFDECC8, 2);
        assert!(yellow.len() > 2000, "editing {editing}: {} marker pixels", yellow.len());
        let (x0, x1) = (yellow.iter().map(|p| p.0).min().unwrap(), yellow.iter().map(|p| p.0).max().unwrap());
        // every column of the marker (but the rounded ends) has marker pixels: a seam would be a
        // column without
        for x in x0 + 3..=x1 - 3 {
            let n = yellow.iter().filter(|p| p.0 == x).count();
            assert!(n >= 2, "editing {editing}: column {x} has {n} marker pixels, box {x0}..{x1}");
        }
        // the text is drawn over the marker: ink pixels inside the marker's box
        let ink = s.pixels_of(0x2B2F36, 40);
        let (y0, y1) = (yellow.iter().map(|p| p.1).min().unwrap(), yellow.iter().map(|p| p.1).max().unwrap());
        assert!(ink.iter().filter(|p| (x0..=x1).contains(&p.0) && (y0..=y1).contains(&p.1)).count() > 100, "editing {editing}: no ink on the marker");
        // blue ink, the red ink of "both" and its green marker are drawn
        assert!(s.pixels_of(0x3A6D96, 50).len() > 30, "editing {editing}: no blue ink");
        assert!(s.pixels_of(0xB8453D, 50).len() > 30, "editing {editing}: no red ink");
        assert!(s.pixels_of(0xDBEDDB, 2).len() > 150, "editing {editing}: no green marker");
    }
}

/// `over(selection-over, marker)`: the see-through wash #6FA2F5 at 0x66 over a marker color.
fn tinted(marker: u32) -> u32 {
    let m = [(marker >> 16) & 255, (marker >> 8) & 255, marker & 255];
    let wash = [0x6F, 0xA2, 0xF5];
    let c: Vec<u32> = (0..3).map(|i| ((wash[i] * 0x66 + m[i] * (255 - 0x66)) as f32 / 255.0).round() as u32).collect();
    c[0] << 16 | c[1] << 8 | c[2]
}

#[test]
fn a_selected_highlighted_word_shows_its_marker_tinted_by_the_wash() {
    // inside one block: the field's own wash
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, "title\n<m=yellow>highlight</m> rest");
    let _ = h.render(0);
    h.advance(ms(5));
    h.press(0, Key::DownArrow);
    h.press_shift(0, Key::RightArrow, 9);
    h.advance(ms(400));
    let _ = h.render(0);
    h.advance(ms(400));
    let s = Shot::of(&h);
    let (before, after) = (s.pixels_of(0xFDECC8, 2).len(), s.pixels_of(tinted(0xFDECC8), 3).len());
    assert!(after > 300, "{after} tinted marker pixels");
    assert!(before < 150, "{before} untinted marker pixels left under the selection");
}

#[test]
fn a_selection_across_blocks_tints_the_markers_it_covers_and_washes_the_rest_as_before() {
    let mut h = note("title\n<m=yellow>highlight</m> rest\nnext line", true);
    h.press_ctrl(0, 'a');
    h.advance(ms(400));
    let _ = h.render(0);
    h.advance(ms(400));
    let s = Shot::of(&h);
    assert!(s.pixels_of(tinted(0xFDECC8), 3).len() > 300, "no tinted marker");
    assert!(s.pixels_of(0xFDECC8, 2).len() < 150, "marker shows untinted under the wash");
    // the plain parts of the selection keep the wash color
    assert!(s.pixels_of(0xBAD3FC, 3).len() > 1000);
}
