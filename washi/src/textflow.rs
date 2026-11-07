//! Line layout of the note text, in the paper's frame, for the "stepped" resting display.
//!
//! The mockup text is tilted with the paper, but the software renderer cannot rotate text. At rest
//! the note therefore draws every word as its own upright `Text`, moved to where the tilted word
//! would be (the paper's tilt about the paper's center). While editing, the stock level `TextInput`
//! shows instead. Both must break lines in the same places, so this module wraps with the advances
//! of the same embedded fonts (they have no kerning table, so the advances are exact).

use crate::document::{Block, Kind, Style as Flags};
use ttf_parser::Face;

const REGULAR: &[u8] = include_bytes!("../../fonts/ShadowsIntoLight.ttf");
const BOLD: &[u8] = include_bytes!("../../fonts/ShadowsIntoLight-Bold.ttf");
const ITALIC: &[u8] = include_bytes!("../../fonts/ShadowsIntoLight-Italic.ttf");
const BOLD_ITALIC: &[u8] = include_bytes!("../../fonts/ShadowsIntoLight-BoldItalic.ttf");

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
    /// Words wider than this are drawn in pieces, so that each piece is within a fraction of a
    /// pixel of the tilted baseline (a whole word would be off by half its width times tan).
    pub max_run: f64,
}

impl Tokens {
    fn style(&self, kind: Kind) -> Style {
        match kind {
            Kind::Title => self.title,
            Kind::Heading => self.heading,
            Kind::Body | Kind::ListItem | Kind::Image => self.body,
        }
    }
}

/// One word, upright, with the offset that puts it on the tilted baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub block: usize,
    pub kind: Kind,
    pub bold: bool,
    pub italic: bool,
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
    fn new(bold: bool, italic: bool, size: f64) -> Measure<'a> {
        let data = match (bold, italic) {
            (false, false) => REGULAR,
            (true, false) => BOLD,
            (false, true) => ITALIC,
            (true, true) => BOLD_ITALIC,
        };
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

/// One word (or piece of a word too long for the line) on a line.
struct Word {
    text: String,
    /// x within the line.
    at: f64,
    /// Byte offset of the word in the block's text.
    off: usize,
}

/// Add a word to the last line (`wrap` always starts with one line, and only ever adds more).
fn push_word(lines: &mut [Vec<Word>], word: Word) {
    if let Some(line) = lines.last_mut() {
        line.push(word);
    }
}

/// Break one block's text into lines of words.
fn wrap(text: &str, avail: f64, m: &Measure) -> Vec<Vec<Word>> {
    let space = m.advance(' ');
    let mut lines: Vec<Vec<Word>> = vec![vec![]];
    let mut x = 0.0;
    let mut pending = 0.0; // spaces seen since the last word
    let mut at_line_start = true;
    let mut pos = 0; // byte offset of the current word
    for (i, word) in text.split(' ').enumerate() {
        if i > 0 {
            pending += space;
            pos += 1;
        }
        let word_pos = pos;
        pos += word.len();
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
            let mut chunk_off = word_pos;
            let mut cw = 0.0;
            for (ci, c) in word.char_indices() {
                let a = m.advance(c);
                if cw + a > avail && !chunk.is_empty() {
                    push_word(&mut lines, Word { text: std::mem::take(&mut chunk), at: 0.0, off: chunk_off });
                    lines.push(vec![]);
                    chunk_off = word_pos + ci;
                    cw = 0.0;
                }
                chunk.push(c);
                cw += a;
            }
            push_word(&mut lines, Word { text: chunk, at: 0.0, off: chunk_off });
            x = cw;
            pending = 0.0;
            at_line_start = false;
            continue;
        }
        let at = x + pending;
        push_word(&mut lines, Word { text: word.to_string(), at, off: word_pos });
        x = at + w;
        pending = 0.0;
        at_line_start = false;
    }
    lines
}

/// Split a word into pieces of at most `max` width (at least one character each).
fn pieces(word: &str, max: f64, m: &Measure) -> Vec<String> {
    let mut out = vec![];
    let (mut cur, mut w) = (String::new(), 0.0);
    for c in word.chars() {
        let a = m.advance(c);
        if !cur.is_empty() && w + a > max {
            out.push(std::mem::take(&mut cur));
            w = 0.0;
        }
        cur.push(c);
        w += a;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Blocks of one group share a rhythm (line pitch); a change of group opens a gap.
fn group(kind: Kind) -> Kind {
    if kind == Kind::ListItem { Kind::Body } else { kind }
}

/// Where the caret is within a block's wrapped lines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretLine {
    /// Visual line of the caret (0 first) and how many lines the block has.
    pub line: usize,
    pub lines: usize,
    /// Distance from the line's left edge, logical px.
    pub x: f64,
}

fn line_starts(lines: &[Vec<Word>]) -> Vec<usize> {
    lines.iter().enumerate().map(|(i, l)| if i == 0 { 0 } else { l.first().map_or(0, |w| w.off) }).collect()
}

/// The visual line and x of the caret at byte `offset` of block `b`, in a paper `paper_w` wide.
pub fn caret_line(b: &Block, offset: usize, paper_w: f64, t: &Tokens) -> CaretLine {
    let avail = paper_w - t.inset_left - t.inset_right;
    let m = Measure::new(b.is_bold(), b.style.contains(Flags::ITALIC), t.style(b.kind).size);
    let lines = wrap(&b.text, avail, &m);
    let starts = line_starts(&lines);
    let mut offset = offset.min(b.text.len());
    while !b.text.is_char_boundary(offset) {
        offset -= 1;
    }
    let line = starts.iter().rposition(|&s| s <= offset).unwrap_or(0);
    CaretLine { line, lines: lines.len(), x: m.width(&b.text[starts[line]..offset]) }
}

/// The byte offset on visual `line` (clamped to the block's last) of block `b` nearest to `x`.
pub fn offset_at(b: &Block, line: usize, x: f64, paper_w: f64, t: &Tokens) -> usize {
    let avail = paper_w - t.inset_left - t.inset_right;
    let m = Measure::new(b.is_bold(), b.style.contains(Flags::ITALIC), t.style(b.kind).size);
    let lines = wrap(&b.text, avail, &m);
    let starts = line_starts(&lines);
    let line = line.min(lines.len() - 1);
    let (start, end) = (starts[line], starts.get(line + 1).copied().unwrap_or(b.text.len() + 1));
    let mut best = (start, f64::MAX);
    let mut at = 0.0;
    for (i, c) in b.text[start..].char_indices().map(|(i, c)| (start + i, c)).chain(std::iter::once((b.text.len(), ' '))) {
        if i >= end {
            break;
        }
        let d = (at - x).abs();
        if d < best.1 {
            best = (i, d);
        }
        at += m.advance(c);
    }
    best.0
}

/// A thin filled bar in the paper's frame: an underline, a strike-through or a list bullet.
/// Level box (`x`, `y`, `width`, `height`) plus the offset (`dx`, `dy`) onto the tilted baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub block: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub dx: f64,
    pub dy: f64,
}

/// Where the baseline of a line is below the top of its line box: the font's ascent plus half of
/// the leading the line pitch adds to the font's natural line height.
fn baseline(m: &Measure, pitch: f64) -> f64 {
    let (asc, desc) = (m.face.ascender() as f64 * m.scale, m.face.descender() as f64 * m.scale);
    asc + (pitch - (asc - desc)) / 2.0
}

/// Lay the blocks out in a paper of size `paper` (logical px).
pub fn layout(blocks: &[Block], paper: (f64, f64), t: &Tokens) -> Vec<Run> {
    layout_all(blocks, paper, t).0
}

/// [`layout`], and the bars that go with it: underlines and strike-throughs (a bar per line, in
/// pieces like the words) and a bullet for each list item.
pub fn layout_all(blocks: &[Block], paper: (f64, f64), t: &Tokens) -> (Vec<Run>, Vec<Rule>) {
    layout_all_with_images(blocks, paper, t, &|_| None)
}

/// The height of an image at the width it is shown: the text-area width times its aspect ratio.
pub fn image_height(width: f64, pixels: (u32, u32)) -> f64 {
    if pixels.0 == 0 {
        0.0
    } else {
        width * pixels.1 as f64 / pixels.0 as f64
    }
}

/// Like [`layout_all`], where `image_size` gives an image block's size in pixels. An image block is as
/// wide as the text area and as tall as its aspect ratio makes it (a missing size: one body line).
pub fn layout_all_with_images(blocks: &[Block], paper: (f64, f64), t: &Tokens, image_size: &dyn Fn(&Block) -> Option<(u32, u32)>) -> (Vec<Run>, Vec<Rule>) {
    let avail = paper.0 - t.inset_left - t.inset_right;
    let mut runs = vec![];
    let mut rules = vec![];
    let mut top = t.inset_top;
    for (bi, b) in blocks.iter().enumerate() {
        let st = t.style(b.kind);
        if bi > 0 && group(blocks[bi - 1].kind) != group(b.kind) {
            top += if b.kind == Kind::Heading { t.heading_gap } else { t.body_gap };
        }
        if b.kind == Kind::Image {
            top += image_size(b).map_or(st.pitch, |px| image_height(avail, px));
            continue;
        }
        let italic = b.style.contains(Flags::ITALIC);
        let m = Measure::new(b.is_bold(), italic, st.size);
        let lines = wrap(&b.text, avail, &m);
        let base = baseline(&m, st.pitch);
        let thick = (st.size / 20.0).max(1.0);
        let bar = |rules: &mut Vec<Rule>, x: f64, y: f64, width: f64, height: f64| {
            let mut at = 0.0;
            while at < width - 1e-9 {
                let w = (width - at).min(t.max_run);
                let (dx, dy) = tilt_offset(x + at + w / 2.0, y + height / 2.0, paper, t.tilt_deg);
                rules.push(Rule { block: bi, x: x + at, y, width: w, height, dx, dy });
                at += w;
            }
        };
        if b.kind == Kind::ListItem {
            let d = (st.size / 4.0).max(3.0);
            bar(&mut rules, t.inset_left - 2.5 * d, top + base - 0.3 * st.size - d / 2.0, d, d);
        }
        for (li, line) in lines.iter().enumerate() {
            let y = top + li as f64 * st.pitch;
            for Word { text: word, at, .. } in line {
                let mut x = t.inset_left + at;
                for piece in pieces(word, t.max_run, &m) {
                    let width = m.width(&piece);
                    let (dx, dy) = tilt_offset(x + width / 2.0, y + st.pitch / 2.0, paper, t.tilt_deg);
                    runs.push(Run { block: bi, kind: b.kind, bold: b.is_bold(), italic, line: li, text: piece, x, y, width, size: st.size, pitch: st.pitch, dx, dy });
                    x += width;
                }
            }
            if let (Some(first), Some(last)) = (line.first(), line.last()) {
                let (x0, x1) = (t.inset_left + first.at, t.inset_left + last.at + m.width(&last.text));
                if b.style.contains(Flags::UNDERLINE) {
                    bar(&mut rules, x0, y + base + 0.1 * st.size, x1 - x0, thick);
                }
                if b.style.contains(Flags::STRIKE) {
                    bar(&mut rules, x0, y + base - 0.3 * st.size, x1 - x0, thick);
                }
            }
        }
        // every block takes at least one line, even when empty
        top += lines.len() as f64 * st.pitch;
    }
    (runs, rules)
}

/// Vertical extent of the text in the paper's frame: the top of every block, and the bottom of the
/// last line (what the note scrolls through).
pub fn extent(blocks: &[Block], paper_w: f64, t: &Tokens) -> (Vec<f64>, f64) {
    extent_with_images(blocks, paper_w, t, &|_| None)
}

/// [`extent`] with the sizes of the images (see [`layout_all_with_images`]).
pub fn extent_with_images(blocks: &[Block], paper_w: f64, t: &Tokens, image_size: &dyn Fn(&Block) -> Option<(u32, u32)>) -> (Vec<f64>, f64) {
    let avail = paper_w - t.inset_left - t.inset_right;
    let mut tops = vec![];
    let mut top = t.inset_top;
    for (bi, b) in blocks.iter().enumerate() {
        let st = t.style(b.kind);
        if bi > 0 && group(blocks[bi - 1].kind) != group(b.kind) {
            top += if b.kind == Kind::Heading { t.heading_gap } else { t.body_gap };
        }
        tops.push(top);
        if b.kind == Kind::Image {
            top += image_size(b).map_or(st.pitch, |px| image_height(avail, px));
            continue;
        }
        let m = Measure::new(b.is_bold(), b.style.contains(Flags::ITALIC), st.size);
        top += wrap(&b.text, avail, &m).len() as f64 * st.pitch;
    }
    (tops, top)
}

/// The top and bottom of the line holding the caret (byte `offset` of block `block`), in the paper's frame.
pub fn caret_span(blocks: &[Block], block: usize, offset: usize, paper_w: f64, t: &Tokens) -> Option<(f64, f64)> {
    caret_span_with_images(blocks, block, offset, paper_w, t, &|_| None)
}

/// [`caret_span`] with the sizes of the images.
pub fn caret_span_with_images(blocks: &[Block], block: usize, offset: usize, paper_w: f64, t: &Tokens, image_size: &dyn Fn(&Block) -> Option<(u32, u32)>) -> Option<(f64, f64)> {
    let b = blocks.get(block)?;
    let (tops, _) = extent_with_images(blocks, paper_w, t, image_size);
    let pitch = t.style(b.kind).pitch;
    let y = tops[block] + caret_line(b, offset, paper_w, t).line as f64 * pitch;
    Some((y, y + pitch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Block, Document};

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
        max_run: f64::MAX,
    };

    const MOCKUP: &str = "# Endfield Note\n## **Thermite Cutter : POG & Akekuri**\nCurrently POG no Weapon : Ideal for him are Never Rest or White Night Nova. The last can be get in a box";

    #[test]
    fn extent_gives_block_tops_and_the_bottom_of_the_last_line() {
        let doc = Document::parse(MOCKUP);
        let (tops, bottom) = extent(doc.blocks(), 363.0, &TOKENS);
        let runs = layout(doc.blocks(), (363.0, 433.0), &TOKENS);
        for (bi, top) in tops.iter().enumerate() {
            let first = runs.iter().find(|r| r.block == bi).unwrap();
            assert!((first.y - top).abs() < 1e-9);
        }
        let last = runs.last().unwrap();
        assert!((bottom - (last.y + last.pitch)).abs() < 1e-9);
    }

    #[test]
    fn caret_span_is_the_line_box_of_the_caret() {
        let doc = Document::parse(MOCKUP);
        let runs = layout(doc.blocks(), (363.0, 433.0), &TOKENS);
        let last_len = doc.blocks()[2].text.len();
        let (top, bottom) = caret_span(doc.blocks(), 2, last_len, 363.0, &TOKENS).unwrap();
        let last = runs.last().unwrap();
        assert_eq!((top, bottom), (last.y, last.y + last.pitch));
        assert_eq!(caret_span(doc.blocks(), 9, 0, 363.0, &TOKENS), None);
    }

    fn line_texts(runs: &[Run], block: usize) -> Vec<String> {
        let n = runs.iter().filter(|r| r.block == block).map(|r| r.line).max().map_or(0, |l| l + 1);
        (0..n)
            .map(|l| runs.iter().filter(|r| r.block == block && r.line == l).map(|r| r.text.as_str()).collect::<Vec<_>>().join(" "))
            .collect()
    }

    #[test]
    fn the_mockup_breaks_where_the_mockup_does() {
        let runs = layout(Document::parse(MOCKUP).blocks(), (363.0, 433.0), &TOKENS);
        assert_eq!(line_texts(&runs, 0), ["Endfield Note"]);
        assert_eq!(line_texts(&runs, 1), ["Thermite Cutter : POG &", "Akekuri"]);
        // the body is one block per stored line, but the mockup's body is one long line
        assert_eq!(line_texts(&runs, 2), ["Currently POG no Weapon : Ideal", "for him are Never Rest or White", "Night Nova. The last can be get", "in a box"]);
    }

    #[test]
    fn words_on_one_line_follow_the_papers_tilt() {
        let runs = layout(Document::parse(MOCKUP).blocks(), (363.0, 433.0), &TOKENS);
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
    fn long_words_are_drawn_in_pieces_that_stay_within_a_pixel_of_the_tilted_baseline() {
        let tokens = Tokens { max_run: 16.0, ..TOKENS };
        let runs = layout(Document::parse(MOCKUP).blocks(), (363.0, 433.0), &tokens);
        let whole = layout(Document::parse(MOCKUP).blocks(), (363.0, 433.0), &TOKENS);
        // same words, same line breaks, same text
        assert_eq!(runs.iter().map(|r| r.text.as_str()).collect::<String>(), whole.iter().map(|r| r.text.as_str()).collect::<String>());
        assert_eq!(runs.iter().map(|r| r.line + 10 * r.block).max(), whole.iter().map(|r| r.line + 10 * r.block).max());
        assert!(runs.len() > whole.len() * 2);
        // every piece sits within 0.3 px of the exact tilt of its own left and right end
        let s = tokens.tilt_deg.to_radians().sin();
        for r in &runs {
            for end in [r.x, r.x + r.width] {
                let exact = tilt_offset(end, r.y + r.pitch / 2.0, (363.0, 433.0), tokens.tilt_deg).1;
                let _ = s;
                assert!((exact - r.dy).abs() < 0.3, "{:?}: {} vs {}", r.text, exact, r.dy);
            }
        }
    }

    #[test]
    fn the_paper_centre_does_not_move() {
        let (dx, dy) = tilt_offset(181.5, 216.5, (363.0, 433.0), 1.45);
        assert!(dx.abs() < 1e-9 && dy.abs() < 1e-9);
    }

    #[test]
    fn a_bigger_paper_re_wraps_but_keeps_insets() {
        let narrow = layout(Document::parse(MOCKUP).blocks(), (363.0, 433.0), &TOKENS);
        let wide = layout(Document::parse(MOCKUP).blocks(), (600.0, 433.0), &TOKENS);
        assert_eq!(narrow[0].x, wide[0].x);
        assert_eq!(narrow[0].y, wide[0].y);
        assert_eq!(line_texts(&wide, 1), ["Thermite Cutter : POG & Akekuri"]);
    }

    #[test]
    fn an_empty_title_still_takes_a_line_and_a_long_word_breaks_anywhere() {
        let runs = layout(Document::parse("\nx").blocks(), (363.0, 433.0), &TOKENS);
        assert_eq!(runs.len(), 1);
        assert!((runs[0].y - (38.2 + 60.0 + 8.1)).abs() < 1e-9);
        let long = "w".repeat(80);
        let runs = layout(Document::parse(&format!("t
{long}")).blocks(), (363.0, 433.0), &TOKENS);
        assert!(runs.iter().filter(|r| r.block == 1).map(|r| r.line).max().unwrap() >= 1);
    }

    #[test]
    fn the_caret_knows_its_visual_line_and_x() {
        let d = Document::parse(MOCKUP);
        let body = &d.blocks()[2];
        let first = caret_line(body, 0, 363.0, &TOKENS);
        assert_eq!((first.line, first.lines, first.x), (0, 4, 0.0));
        // "for" starts the second line
        let second = caret_line(body, "Currently POG no Weapon : Ideal ".len(), 363.0, &TOKENS);
        assert_eq!((second.line, second.x), (1, 0.0));
        let last = caret_line(body, body.text.len(), 363.0, &TOKENS);
        assert_eq!(last.line, 3);
        assert!(last.x > 50.0);
        // an empty block is one line
        assert_eq!(caret_line(&Block::new(Kind::Body, ""), 0, 363.0, &TOKENS).lines, 1);
    }

    #[test]
    fn offset_at_is_the_inverse_of_caret_line() {
        let d = Document::parse(MOCKUP);
        for b in d.blocks() {
            for o in (0..=b.text.len()).filter(|&o| b.text.is_char_boundary(o)) {
                let c = caret_line(b, o, 363.0, &TOKENS);
                let back = offset_at(b, c.line, c.x, 363.0, &TOKENS);
                // a caret after a wrapping space sits where the next line starts, so compare the position
                assert_eq!(caret_line(b, back, 363.0, &TOKENS), c, "{:?} at {o}", b.text);
            }
        }
    }

    #[test]
    fn offset_at_picks_the_nearest_boundary_on_the_line_and_clamps() {
        let d = Document::parse(MOCKUP);
        let body = &d.blocks()[2];
        assert_eq!(offset_at(body, 0, 0.0, 363.0, &TOKENS), 0);
        assert_eq!(offset_at(body, 1, 0.0, 363.0, &TOKENS), "Currently POG no Weapon : Ideal ".len());
        assert_eq!(offset_at(body, 3, 1e6, 363.0, &TOKENS), body.text.len());
        assert_eq!(offset_at(body, 99, 0.0, 363.0, &TOKENS), "Currently POG no Weapon : Ideal for him are Never Rest or White Night Nova. The last can be get ".len());
        // far right of a wrapped line stays on that line
        let end0 = offset_at(body, 0, 1e6, 363.0, &TOKENS);
        assert_eq!(caret_line(body, end0, 363.0, &TOKENS).line, 0);
        let multi = Block::new(Kind::Body, "héllo wörld");
        assert!(multi.text.is_char_boundary(offset_at(&multi, 0, 20.0, 363.0, &TOKENS)));
    }
}
