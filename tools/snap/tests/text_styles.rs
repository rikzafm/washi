//! Ticket 08: the mockup text (title, bold heading, body) is laid out at the mockup's positions,
//! sizes, line heights and line breaks, from fixed logical insets, with the embedded fonts.
//!
//! Expected numbers are the mockup's text lines mapped into the paper's own frame
//! (docs/design/spec.json `expected.hover`, leveled by the 1.5 deg paper tilt, in logical px).
//! These snaps are of an editing note (level text fields); the resting, tilted look is checked by
//! the fidelity loop (tools/fidelity.ps1) against the mockups.
use snap::shot::{snapshot, Options, Snapshot, WINDOW_MARGIN};

const MOCKUP: &str = "# Endfield Note\n## **Thermite Cutter : POG & Akekuri**\nCurrently POG no Weapon : Ideal for him are Never Rest or White Night Nova. The last can be get in a box";

struct Line {
    baseline: f64,
    left: f64,
    right: f64,
}

/// Rows above the toolbar band (the toolbar glyphs are ink too, but not text).
fn text_rows(s: &Snapshot, paper_h: f64, scale: f64) -> u32 {
    let win_h = ((paper_h + 2.0 * WINDOW_MARGIN) * scale).round();
    let top = (s.height as f64 - win_h) / 2.0 + WINDOW_MARGIN * scale;
    (top + (paper_h - 56.0) * scale) as u32
}

/// Ink lines of the snapshot in the paper's frame (logical px from its top-left corner).
fn lines(s: &Snapshot, paper: (f64, f64), scale: f64) -> Vec<Line> {
    let (win_w, win_h) = (((paper.0 + 2.0 * WINDOW_MARGIN) * scale).round(), ((paper.1 + 2.0 * WINDOW_MARGIN) * scale).round());
    let ox = (s.width as f64 - win_w) / 2.0 + WINDOW_MARGIN * scale;
    let oy = (s.height as f64 - win_h) / 2.0 + WINDOW_MARGIN * scale;
    let ink = |x: u32, y: u32| s.pixel(x, y).iter().map(|&c| c as u32).sum::<u32>() < 3 * 140;
    let rows: Vec<u32> = (0..text_rows(s, paper.1, scale)).map(|y| (0..s.width).filter(|&x| ink(x, y)).count() as u32).collect();
    let mut out = vec![];
    let mut start = None;
    for y in 0..=rows.len() {
        let has = y < rows.len() && rows[y] > 0;
        match (has, start) {
            (true, None) => start = Some(y),
            (false, Some(a)) => {
                start = None;
                // the baseline is where the ink density drops the most
                let base = (a..y - 1).max_by_key(|&r| rows[r] as i64 - rows[r + 1] as i64).unwrap() + 1;
                let cols: Vec<u32> = (0..s.width).filter(|&x| (a..y).any(|r| ink(x, r as u32))).collect();
                out.push(Line {
                    baseline: (base as f64 - oy) / scale,
                    left: (*cols.first().unwrap() as f64 - ox) / scale,
                    right: (*cols.last().unwrap() as f64 + 1.0 - ox) / scale,
                });
            }
            _ => {}
        }
    }
    out
}

fn mockup(paper: (f64, f64)) -> Vec<Line> {
    let o = Options { text: MOCKUP.into(), paper, hover: true, editing: true, ..Options::default() };
    lines(&snapshot(&o).unwrap(), paper, o.scale)
}

// (baseline v, left u, right u) of the mockup lines in the paper frame.
const EXPECTED: [(f64, f64, f64); 7] = [
    (81.0, 33.8, 205.2),  // title
    (132.4, 32.5, 257.2), // heading: "Thermite Cutter : POG &"
    (161.7, 31.9, 94.6),  // heading: "Akekuri"
    (198.4, 32.9, 311.6), // body: "Currently POG no Weapon : Ideal"
    (226.4, 32.9, 321.7), // body: "for him are Never Rest or White"
    (256.4, 33.1, 308.5), // body: "Night Nova. The last can be get"
    (285.7, 33.2, 100.5), // body: "in a box"
];

#[test]
fn title_heading_and_body_break_lines_exactly_like_the_mockup_and_sit_at_its_positions() {
    let l = mockup((363.0, 433.0));
    assert_eq!(l.len(), 7, "title, heading (2 lines: Akekuri wraps), body (4 lines)");
    for (i, (line, (v, u0, u1))) in l.iter().zip(EXPECTED).enumerate() {
        // body line 2's baseline estimate is noisy (descenders); 2.5 px covers it, line breaks are exact
        assert!((line.baseline - v).abs() <= 2.5, "line {i}: baseline {:.1} vs {v}", line.baseline);
        // the title field has the focus: its caret sits left of the first letter
        if i > 0 {
            assert!((line.left - u0).abs() <= 1.5, "line {i}: left {:.1} vs {u0}", line.left);
        }
        assert!((line.right - u1).abs() <= 1.5, "line {i}: right {:.1} vs {u1}", line.right);
    }
}

#[test]
fn insets_and_sizes_are_fixed_values_not_proportions_of_the_paper() {
    let (a, b) = (mockup((363.0, 433.0)), mockup((420.0, 480.0)));
    // same top and left in the paper frame (title, first heading line); the bigger paper only changes the wrapping
    for (i, (x, y)) in a.iter().zip(&b).take(2).enumerate() {
        assert!((x.baseline - y.baseline).abs() <= 0.7, "line {i}: baseline {} vs {}", x.baseline, y.baseline);
        assert!((x.left - y.left).abs() <= 0.7, "line {i}: left {} vs {}", x.left, y.left);
    }
    assert!((a[0].right - b[0].right).abs() <= 0.7, "title width changed: font size is not fixed");
}

#[test]
fn ink_uses_the_mockup_color() {
    let o = Options { text: MOCKUP.into(), paper: (363.0, 433.0), hover: true, editing: true, ..Options::default() };
    let s = snapshot(&o).unwrap();
    let darkest = (0..text_rows(&s, 433.0, 1.5)).flat_map(|y| (0..s.width).map(move |x| (x, y))).map(|(x, y)| s.pixel(x, y)).min_by_key(|p| p.iter().map(|&c| c as u32).sum::<u32>()).unwrap();
    // the renderer's premultiplied blending may be off by one level
    for (got, want) in darkest.iter().zip([0x2B_u8, 0x2F, 0x36]) {
        assert!(got.abs_diff(want) <= 2, "{darkest:?}");
    }
}
