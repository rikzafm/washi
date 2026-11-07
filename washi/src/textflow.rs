//! Line layout of the note text, in the paper's frame, for the "stepped" resting display.
//!
//! The mockup text is tilted with the paper, but the software renderer cannot rotate text. At rest
//! the note therefore draws every word as its own upright `Text`, moved to where the tilted word
//! would be (the paper's tilt about the paper's center). While editing, the stock level `TextInput`
//! shows instead. Both must break lines in the same places, so this module wraps with the advances
//! of the same embedded fonts (they have no kerning table, so the advances are exact).

use crate::blocks::{Block, Kind};
use ttf_parser::Face;

const REGULAR: &[u8] = include_bytes!("../../fonts/ShadowsIntoLight.ttf");
const BOLD: &[u8] = include_bytes!("../../fonts/ShadowsIntoLight-Bold.ttf");

/// The look of one kind of block: font size and line pitch, in logical px.
#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub size: f64,
    pub pitch: f64,
}

/// Fixed logical values from `ui/theme.slint`.
#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    pub inset_left: f64,
    pub inset_right: f64,
    pub inset_top: f64,
    pub heading_gap: f64,
    pub body_gap: f64,
    pub title: Style,
    pub heading: Style,
    pub body: Style,
    /// Tilt of the text with the paper, degrees clockwise.
    pub tilt_deg: f64,
}

impl Tokens {
    fn style(&self, kind: Kind) -> Style {
        match kind {
            Kind::Title => self.title,
            Kind::Heading => self.heading,
            Kind::Body => self.body,
        }
    }
}

/// One word, upright, with the offset that puts it on the tilted baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub block: usize,
    pub kind: Kind,
    /// Line within the block (0 first).
    pub line: usize,
    pub text: String,
    /// Top-left of the line box in the paper's frame, level.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub size: f64,
    pub pitch: f64,
    /// Offset of the resting (tilted) position from the level one.
    pub dx: f64,
    pub dy: f64,
}

struct Measure<'a> {
    face: Face<'a>,
    scale: f64,
}

impl<'a> Measure<'a> {
    fn new(kind: Kind, size: f64) -> Measure<'a> {
        let data = if kind == Kind::Heading { BOLD } else { REGULAR };
        let face = Face::parse(data, 0).expect("embedded font parses");
        let scale = size / face.units_per_em() as f64;
        Measure { face, scale }
    }

    fn advance(&self, c: char) -> f64 {
        let g = self.face.glyph_index(c).unwrap_or(ttf_parser::GlyphId(0));
        self.face.glyph_hor_advance(g).unwrap_or(0) as f64 * self.scale
    }

    fn width(&self, s: &str) -> f64 {
        s.chars().map(|c| self.advance(c)).sum()
    }
}

/// Where the point `(x, y)` goes when the paper (size `paper`) is turned `tilt_deg` clockwise about
/// its center: the offset from `(x, y)`.
pub fn tilt_offset(x: f64, y: f64, paper: (f64, f64), tilt_deg: f64) -> (f64, f64) {
    let (s, c) = tilt_deg.to_radians().sin_cos();
    let (rx, ry) = (x - paper.0 / 2.0, y - paper.1 / 2.0);
    (rx * (c - 1.0) - ry * s, rx * s + ry * (c - 1.0))
}

/// Break one block's text into lines of words: `(text of the word, x within the line)` per word.
fn wrap(text: &str, avail: f64, m: &Measure) -> Vec<Vec<(String, f64)>> {
    let space = m.advance(' ');
    let mut lines: Vec<Vec<(String, f64)>> = vec![vec![]];
    let mut x = 0.0;
    let mut pending = 0.0; // spaces seen since the last word
    let mut at_line_start = true;
    for (i, word) in text.split(' ').enumerate() {
        if i > 0 {
            pending += space;
        }
        if word.is_empty() {
            continue;
        }
        let w = m.width(word);
        if !at_line_start && x + pending + w > avail + 1e-6 {
            lines.push(vec![]);
            x = 0.0;
            pending = 0.0;
            at_line_start = true;
        }
        if at_line_start && w > avail {
            // a word wider than the line breaks anywhere
            let mut chunk = String::new();
            let mut cw = 0.0;
            for c in word.chars() {
                let a = m.advance(c);
                if cw + a > avail && !chunk.is_empty() {
                    lines.last_mut().unwrap().push((std::mem::take(&mut chunk), 0.0));
                    lines.push(vec![]);
                    cw = 0.0;
                }
                chunk.push(c);
                cw += a;
            }
            lines.last_mut().unwrap().push((chunk, 0.0));
            x = cw;
            pending = 0.0;
            at_line_start = false;
            continue;
        }
        let at = x + pending;
        lines.last_mut().unwrap().push((word.to_string(), at));
        x = at + w;
        pending = 0.0;
        at_line_start = false;
    }
    lines
}

/// Lay the blocks out in a paper of size `paper` (logical px).
pub fn layout(blocks: &[Block], paper: (f64, f64), t: &Tokens) -> Vec<Run> {
    let avail = paper.0 - t.inset_left - t.inset_right;
    let mut runs = vec![];
    let mut top = t.inset_top;
    for (bi, b) in blocks.iter().enumerate() {
        let st = t.style(b.kind);
        if bi > 0 && blocks[bi - 1].kind != b.kind {
            top += if b.kind == Kind::Heading { t.heading_gap } else { t.body_gap };
        }
        let m = Measure::new(b.kind, st.size);
        for (li, line) in wrap(&b.text, avail, &m).into_iter().enumerate() {
            let y = top + li as f64 * st.pitch;
            for (word, at) in line {
                let x = t.inset_left + at;
                let width = m.width(&word);
                let (dx, dy) = tilt_offset(x + width / 2.0, y + st.pitch / 2.0, paper, t.tilt_deg);
                runs.push(Run { block: bi, kind: b.kind, line: li, text: word, x, y, width, size: st.size, pitch: st.pitch, dx, dy });
            }
        }
        // every block takes at least one line, even when empty
        let n = wrap(&b.text, avail, &m).len();
        top += n as f64 * st.pitch;
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::parse;

    pub const TOKENS: Tokens = Tokens {
        inset_left: 32.0,
        inset_right: 40.0,
        inset_top: 38.2,
        heading_gap: 10.0,
        body_gap: 8.1,
        title: Style { size: 37.3, pitch: 60.0 },
        heading: Style { size: 24.0, pitch: 29.3 },
        body: Style { size: 22.65, pitch: 29.1 },
        tilt_deg: 1.45,
    };

    const MOCKUP: &str = "# Endfield Note\n## **Thermite Cutter : POG & Akekuri**\nCurrently POG no Weapon : Ideal for him are Never Rest or White Night Nova. The last can be get in a box";

    fn line_texts(runs: &[Run], block: usize) -> Vec<String> {
        let n = runs.iter().filter(|r| r.block == block).map(|r| r.line).max().map_or(0, |l| l + 1);
        (0..n)
            .map(|l| runs.iter().filter(|r| r.block == block && r.line == l).map(|r| r.text.as_str()).collect::<Vec<_>>().join(" "))
            .collect()
    }

    #[test]
    fn the_mockup_breaks_where_the_mockup_does() {
        let runs = layout(&parse(MOCKUP), (363.0, 433.0), &TOKENS);
        assert_eq!(line_texts(&runs, 0), ["Endfield Note"]);
        assert_eq!(line_texts(&runs, 1), ["Thermite Cutter : POG &", "Akekuri"]);
        // the body is one block per stored line, but the mockup's body is one long line
        assert_eq!(line_texts(&runs, 2), ["Currently POG no Weapon : Ideal", "for him are Never Rest or White", "Night Nova. The last can be get", "in a box"]);
    }

    #[test]
    fn words_on_one_line_follow_the_papers_tilt() {
        let runs = layout(&parse(MOCKUP), (363.0, 433.0), &TOKENS);
        let line: Vec<&Run> = runs.iter().filter(|r| r.block == 2 && r.line == 1).collect();
        assert!(line.len() >= 5);
        let slope = TOKENS.tilt_deg.to_radians().tan();
        for w in line.windows(2) {
            let (a, b) = (w[0].x + w[0].width / 2.0, w[1].x + w[1].width / 2.0);
            let got = ((w[1].y + w[1].dy) - (w[0].y + w[0].dy)) / ((b + w[1].dx) - (a + w[0].dx));
            assert!((got - slope).abs() < 0.0005, "slope {got} vs {slope}");
        }
    }

    #[test]
    fn the_paper_centre_does_not_move() {
        let (dx, dy) = tilt_offset(181.5, 216.5, (363.0, 433.0), 1.45);
        assert!(dx.abs() < 1e-9 && dy.abs() < 1e-9);
    }

    #[test]
    fn a_bigger_paper_re_wraps_but_keeps_insets() {
        let narrow = layout(&parse(MOCKUP), (363.0, 433.0), &TOKENS);
        let wide = layout(&parse(MOCKUP), (600.0, 433.0), &TOKENS);
        assert_eq!(narrow[0].x, wide[0].x);
        assert_eq!(narrow[0].y, wide[0].y);
        assert_eq!(line_texts(&wide, 1), ["Thermite Cutter : POG & Akekuri"]);
    }

    #[test]
    fn an_empty_title_still_takes_a_line_and_a_long_word_breaks_anywhere() {
        let runs = layout(&parse("\nx"), (363.0, 433.0), &TOKENS);
        assert_eq!(runs.len(), 1);
        assert!((runs[0].y - (38.2 + 60.0 + 8.1)).abs() < 1e-9);
        let long = "w".repeat(80);
        let runs = layout(&parse(&format!("t\n{long}")), (363.0, 433.0), &TOKENS);
        assert!(runs.iter().filter(|r| r.block == 1).map(|r| r.line).max().unwrap() >= 1);
    }
}
