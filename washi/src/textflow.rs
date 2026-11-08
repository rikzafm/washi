//! Line layout of the note text, in the paper's frame, for the "stepped" resting display.
//!
//! The mockup text is tilted with the paper, but the software renderer cannot rotate text. At rest
//! the note therefore draws every word as its own upright `Text`, moved to where the tilted word
//! would be (the paper's tilt about the paper's center). While editing, the stock level `TextInput`
//! shows instead. Both must break lines in the same places, so this module wraps with the advances
//! of the same embedded fonts (they have no kerning table, so the advances are exact).
//!
//! The four faces (regular, bold, italic, bold italic) have the same advances, so a line breaks in
//! the same places whatever its characters' styles. The edit fields lay their text out in a blank
//! face with those advances too (`fonts/WashiCaret.ttf`), and the words are drawn from this layout
//! while editing as well, each piece in its own face.

use crate::document::{Block, Caret, Kind, Range, Style as Flags};
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
///
/// Lines break at the Unicode line-break opportunities (UAX #14), as Slint's own text layout does:
/// after spaces, but also after a hyphen, a slash and the like. Breaking at spaces only would put
/// fewer lines here than the edit fields show for text such as paths, and the note would then
/// scroll short of its end and move the caret to the wrong lines.
fn wrap(text: &str, avail: f64, m: &Measure) -> Vec<Vec<Word>> {
    let mut lines: Vec<Vec<Word>> = vec![vec![]];
    let mut x = 0.0;
    let mut pending = 0.0; // trailing spaces since the last word
    let mut at_line_start = true;
    let mut start = 0; // byte offset of the current fragment
    for (end, _) in unicode_linebreak::linebreaks(text) {
        if end <= start {
            continue;
        }
        let fragment = &text[start..end];
        let word_pos = start;
        start = end;
        let word = fragment.trim_end_matches(' ');
        let trailing = m.width(&fragment[word.len()..]);
        if word.is_empty() {
            pending += trailing;
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
            pending = trailing;
            at_line_start = false;
            continue;
        }
        let at = x + pending;
        push_word(&mut lines, Word { text: word.to_string(), at, off: word_pos });
        x = at + w;
        pending = trailing;
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

/// The stretches of `start..end` of a block drawn in one face (bold and italic the same).
fn face_runs(b: &Block, start: usize, end: usize) -> Vec<(usize, usize)> {
    let face = |i: usize| b.shown_at(i).bits() & (Flags::BOLD.bits() | Flags::ITALIC.bits());
    let mut out = vec![];
    let mut s = start;
    for i in start + 1..end {
        if b.text.is_char_boundary(i) && face(i) != face(s) {
            out.push((s, i));
            s = i;
        }
    }
    if s < end {
        out.push((s, end));
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
    let m = Measure::new(false, false, t.style(b.kind).size);
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
    let m = Measure::new(false, false, t.style(b.kind).size);
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
        let m = Measure::new(false, false, st.size);
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
            for Word { text: word, at, off } in line {
                let mut x = t.inset_left + at;
                for (s, e) in face_runs(b, *off, *off + word.len()) {
                    let shown = b.shown_at(s);
                    let (bold, italic) = (shown.contains(Flags::BOLD), shown.contains(Flags::ITALIC));
                    for piece in pieces(&b.text[s..e], t.max_run, &m) {
                        let width = m.width(&piece);
                        let (dx, dy) = tilt_offset(x + width / 2.0, y + st.pitch / 2.0, paper, t.tilt_deg);
                        runs.push(Run { block: bi, kind: b.kind, bold, italic, line: li, text: piece, x, y, width, size: st.size, pitch: st.pitch, dx, dy });
                        x += width;
                    }
                }
            }
            if let (Some(first), Some(last)) = (line.first(), line.last()) {
                // from the line's first word to the end of its last (the spaces between words
                // included, those after the last left out)
                let (from, to) = (first.off, last.off + last.text.len());
                let x_of = |i: usize| t.inset_left + first.at + m.width(&b.text[from..i]);
                for (flag, lift) in [(Flags::UNDERLINE, -0.1), (Flags::STRIKE, 0.3)] {
                    let mut i = from;
                    while i < to {
                        // every byte of a character has its mark: a change is at a character boundary
                        if !b.marks[i].contains(flag) {
                            i += 1;
                            continue;
                        }
                        let j = (i..to).find(|&k| !b.marks[k].contains(flag)).unwrap_or(to);
                        bar(&mut rules, x_of(i), y + base - lift * st.size, x_of(j) - x_of(i), thick);
                        i = j;
                    }
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
        let m = Measure::new(false, false, st.size);
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

/// The caret under `point` (level paper frame, x from the paper's left edge): the block whose top is
/// the last one above the point, the visual line within it, then the nearest character boundary.
/// Above the first block is the start of the note, below the last its end. On an image the upper
/// half is the image's start and the lower half the start of the block after it (an image caret
/// has offset 0; the last image has no block after it).
pub fn hit(blocks: &[Block], point: (f64, f64), paper_w: f64, t: &Tokens, image_size: &dyn Fn(&Block) -> Option<(u32, u32)>) -> Caret {
    let (tops, bottom) = extent_with_images(blocks, paper_w, t, image_size);
    let Some(last) = blocks.len().checked_sub(1) else { return Caret { block: 0, offset: 0 } };
    if point.1 < tops[0] {
        return Caret { block: 0, offset: 0 };
    }
    if point.1 >= bottom {
        return Caret { block: last, offset: if blocks[last].kind == Kind::Image { 0 } else { blocks[last].text.len() } };
    }
    let block = tops.iter().rposition(|&top| top <= point.1).unwrap_or(0);
    let b = &blocks[block];
    if b.kind == Kind::Image {
        let end = tops.get(block + 1).copied().unwrap_or(bottom);
        let lower = point.1 >= tops[block] + (end - tops[block]) / 2.0;
        return Caret { block: if lower && block < last { block + 1 } else { block }, offset: 0 };
    }
    let pitch = t.style(b.kind).pitch;
    let line = ((point.1 - tops[block]) / pitch).floor().max(0.0) as usize;
    Caret { block, offset: offset_at(b, line, point.0 - t.inset_left, paper_w, t) }
}

/// A level box in the paper's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The wash of a selection: a level rect for each visual line the range touches, from the caret x of
/// where the range starts on the line to where it ends. A line that ends inside the range (a wrap,
/// or a paragraph break) gets a trailing pad of 0.3 em, so an empty paragraph shows a sliver. An
/// image in the range is washed over its whole size.
pub fn selection_rects(blocks: &[Block], range: Range, paper_w: f64, t: &Tokens, image_size: &dyn Fn(&Block) -> Option<(u32, u32)>) -> Vec<Rect> {
    let (start, end) = range.ordered();
    if start == end {
        return vec![];
    }
    let avail = paper_w - t.inset_left - t.inset_right;
    let (tops, _) = extent_with_images(blocks, paper_w, t, image_size);
    let mut out = vec![];
    for bi in start.block..=end.block.min(blocks.len().saturating_sub(1)) {
        let b = &blocks[bi];
        let st = t.style(b.kind);
        if b.kind == Kind::Image {
            let height = image_size(b).map_or(st.pitch, |px| image_height(avail, px));
            out.push(Rect { x: t.inset_left, y: tops[bi], width: avail, height, });
            continue;
        }
        let from = if bi == start.block { start.offset.min(b.text.len()) } else { 0 };
        let to = if bi == end.block { end.offset.min(b.text.len()) } else { b.text.len() };
        let m = Measure::new(false, false, st.size);
        let lines = wrap(&b.text, avail, &m);
        let starts = line_starts(&lines);
        for (li, &ls) in starts.iter().enumerate() {
            let last = li + 1 == starts.len();
            let le = if last { b.text.len() } else { starts[li + 1] };
            if (from >= le && !last) || to < ls {
                continue;
            }
            let (a, e) = (from.clamp(ls, le), to.clamp(ls, le));
            let pad = if last { bi < end.block } else { to > le };
            if a >= e && !pad {
                continue;
            }
            // the space a wrap breaks at is not shown washed: the pad stands for it
            let shown_end = if last { e } else { e.min(ls + b.text[ls..le].trim_end().len()).max(a) };
            let x0 = m.width(&b.text[ls..a]);
            let x1 = m.width(&b.text[ls..shown_end]);
            let tail = if pad { st.size * 0.3 } else { 0.0 };
            out.push(Rect { x: t.inset_left + x0, y: tops[bi] + li as f64 * st.pitch, width: x1 - x0 + tail, height: st.pitch });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Block, Caret, Document, Range};

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
    fn lines_break_after_hyphens_like_the_edit_fields_do() {
        // no spaces at all: only the hyphens let it break between words
        let text = "t\nlibopenssl-sys-bfe5c79-libcrypto-lib-ct-b64";
        let runs = layout(Document::parse(text).blocks(), (363.0, 433.0), &TOKENS);
        let lines = line_texts(&runs, 1);
        assert!(lines.len() >= 2, "{lines:?}");
        assert!(lines.iter().take(lines.len() - 1).all(|l| l.ends_with('-')), "{lines:?}");
        // the text is all there, in order
        assert_eq!(lines.concat().replace(' ', ""), "libopenssl-sys-bfe5c79-libcrypto-lib-ct-b64");
    }

    #[test]
    fn styled_words_are_drawn_in_their_own_face_and_break_like_plain_ones() {
        let plain = Document::parse(MOCKUP);
        let mut styled = plain.clone();
        // "POG" bold, half of "Weapon" italic, "no" underlined
        styled.toggle_style(2, 10, 13, Flags::BOLD);
        styled.toggle_style(2, 20, 23, Flags::ITALIC);
        styled.toggle_style(2, 14, 16, Flags::UNDERLINE);
        let (runs, rules) = layout_all(styled.blocks(), (363.0, 433.0), &TOKENS);
        let plain_runs = layout(plain.blocks(), (363.0, 433.0), &TOKENS);
        assert_eq!(line_texts(&runs, 2).len(), line_texts(&plain_runs, 2).len(), "same line breaks");
        let pog = runs.iter().find(|r| r.block == 2 && r.text == "POG").unwrap();
        assert!(pog.bold && !pog.italic);
        let wea = runs.iter().find(|r| r.block == 2 && r.text == "Wea").unwrap();
        let pon = runs.iter().find(|r| r.block == 2 && r.text == "pon").unwrap();
        assert!(!wea.italic && pon.italic, "a word is split where its style changes");
        assert!((wea.x + wea.width - pon.x).abs() < 1e-9);
        // the heading is bold throughout
        assert!(runs.iter().filter(|r| r.block == 1).all(|r| r.bold));
        // one underline, under "no" only
        let no = runs.iter().find(|r| r.block == 2 && r.text == "no").unwrap();
        let bars: Vec<&Rule> = rules.iter().filter(|r| r.block == 2).collect();
        assert_eq!(bars.len(), 1);
        assert!((bars[0].x - no.x).abs() < 1e-9 && (bars[0].width - no.width).abs() < 1e-9);
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

    /// The centre of the caret's line box for `c`, in the paper's frame (level).
    fn point_of(d: &Document, c: Caret) -> (f64, f64) {
        let (tops, _) = extent(d.blocks(), 363.0, &TOKENS);
        let b = &d.blocks()[c.block];
        let cl = caret_line(b, c.offset, 363.0, &TOKENS);
        let pitch = TOKENS.style(b.kind).pitch;
        (TOKENS.inset_left + cl.x, tops[c.block] + (cl.line as f64 + 0.5) * pitch)
    }

    #[test]
    fn hit_is_the_inverse_of_the_caret_position_for_every_offset() {
        let d = Document::parse(MOCKUP);
        for (block, b) in d.blocks().iter().enumerate() {
            for offset in (0..=b.text.len()).filter(|&o| b.text.is_char_boundary(o)) {
                let c = Caret { block, offset };
                let got = hit(d.blocks(), point_of(&d, c), 363.0, &TOKENS, &|_| None);
                assert_eq!(got.block, block);
                // a caret after a wrapping space sits where the next line starts: compare positions
                let (a, e) = (&d.blocks()[got.block], &d.blocks()[block]);
                assert_eq!(caret_line(a, got.offset, 363.0, &TOKENS), caret_line(e, offset, 363.0, &TOKENS), "block {block} offset {offset}");
            }
        }
    }

    #[test]
    fn hit_snaps_above_the_first_block_below_the_last_and_to_the_sides() {
        let d = Document::parse(MOCKUP);
        let none = |_: &Block| None;
        assert_eq!(hit(d.blocks(), (100.0, 2.0), 363.0, &TOKENS, &none), Caret { block: 0, offset: 0 });
        let last = d.blocks().len() - 1;
        let len = d.blocks()[last].text.len();
        assert_eq!(hit(d.blocks(), (100.0, 5000.0), 363.0, &TOKENS, &none), Caret { block: last, offset: len });
        // left of the text area is the start of the line, right of it the end
        let (_, y) = point_of(&d, Caret { block: 0, offset: 0 });
        assert_eq!(hit(d.blocks(), (-50.0, y), 363.0, &TOKENS, &none).offset, 0);
        assert_eq!(hit(d.blocks(), (5000.0, y), 363.0, &TOKENS, &none).offset, d.blocks()[0].text.len());
        // the gap between two blocks belongs to the block above it
        let (tops, _) = extent(d.blocks(), 363.0, &TOKENS);
        assert_eq!(hit(d.blocks(), (5000.0, tops[1] - 1.0), 363.0, &TOKENS, &none).block, 0);
    }

    #[test]
    fn hit_on_an_image_snaps_to_its_start_or_to_the_start_of_what_follows() {
        let d = Document::from_blocks(vec![Block::new(Kind::Title, "t"), Block::image(1), Block::new(Kind::Body, "after")]);
        let size = |b: &Block| b.image_id.map(|_| (200, 100));
        let (tops, _) = extent_with_images(d.blocks(), 363.0, &TOKENS, &size);
        let h = image_height(363.0 - TOKENS.inset_left - TOKENS.inset_right, (200, 100));
        assert_eq!(hit(d.blocks(), (100.0, tops[1] + h * 0.25), 363.0, &TOKENS, &size), Caret { block: 1, offset: 0 });
        assert_eq!(hit(d.blocks(), (100.0, tops[1] + h * 0.75), 363.0, &TOKENS, &size), Caret { block: 2, offset: 0 });
    }

    fn rects(d: &Document, range: Range) -> Vec<Rect> {
        selection_rects(d.blocks(), range, 363.0, &TOKENS, &|_| None)
    }

    #[test]
    fn selection_rects_cover_exactly_the_lines_in_the_range() {
        let d = Document::parse(MOCKUP);
        // the whole body: four lines, one level rect each, stacked at the body's pitch
        let body = Range { anchor: Caret { block: 2, offset: 0 }, focus: Caret { block: 2, offset: d.blocks()[2].text.len() } };
        let r = rects(&d, body);
        let (tops, _) = extent(d.blocks(), 363.0, &TOKENS);
        assert_eq!(r.len(), 4);
        for (i, s) in r.iter().enumerate() {
            assert!((s.y - (tops[2] + i as f64 * TOKENS.body.pitch)).abs() < 1e-9);
            assert!((s.height - TOKENS.body.pitch).abs() < 1e-9);
            assert!((s.x - TOKENS.inset_left).abs() < 1e-9);
        }
        // a collapsed range shows nothing; a reversed one is the same as the ordered one
        assert!(rects(&d, Range { anchor: body.anchor, focus: body.anchor }).is_empty());
        let rev = Range { anchor: body.focus, focus: body.anchor };
        assert_eq!(rects(&d, rev), r);
        // inside one line: from the caret x of the start to the caret x of the end (no pad: the line goes on)
        let b = &d.blocks()[2];
        let part = Range { anchor: Caret { block: 2, offset: 2 }, focus: Caret { block: 2, offset: 6 } };
        let p = rects(&d, part);
        assert_eq!(p.len(), 1);
        let (x0, x1) = (caret_line(b, 2, 363.0, &TOKENS).x, caret_line(b, 6, 363.0, &TOKENS).x);
        assert!((p[0].x - (TOKENS.inset_left + x0)).abs() < 1e-9 && (p[0].width - (x1 - x0)).abs() < 1e-9);
    }

    #[test]
    fn lines_that_end_inside_the_range_get_a_pad_and_an_empty_paragraph_a_sliver() {
        let d = Document::from_blocks(vec![Block::new(Kind::Title, "ab"), Block::new(Kind::Body, ""), Block::new(Kind::Body, "cd")]);
        let range = Range { anchor: Caret { block: 0, offset: 1 }, focus: Caret { block: 2, offset: 1 } };
        let r = rects(&d, range);
        assert_eq!(r.len(), 3, "{r:?}");
        let pad = TOKENS.title.size * 0.3;
        let b = &d.blocks()[0];
        let w = caret_line(b, 2, 363.0, &TOKENS).x - caret_line(b, 1, 363.0, &TOKENS).x;
        assert!((r[0].width - (w + pad)).abs() < 1e-9);
        // the empty paragraph: nothing but the pad
        assert!((r[1].width - TOKENS.body.size * 0.3).abs() < 1e-9 && r[1].width > 1.0);
        // the last line ends in the range's end: no pad
        let c = caret_line(&d.blocks()[2], 1, 363.0, &TOKENS).x;
        assert!((r[2].width - c).abs() < 1e-9);
    }

    #[test]
    fn an_image_inside_the_range_is_washed_over_its_whole_size() {
        let d = Document::from_blocks(vec![Block::new(Kind::Title, "t"), Block::image(1), Block::new(Kind::Body, "after")]);
        let size = |b: &Block| b.image_id.map(|_| (200, 100));
        let range = Range { anchor: Caret { block: 0, offset: 0 }, focus: Caret { block: 2, offset: 3 } };
        let r = selection_rects(d.blocks(), range, 363.0, &TOKENS, &size);
        let (tops, _) = extent_with_images(d.blocks(), 363.0, &TOKENS, &size);
        let avail = 363.0 - TOKENS.inset_left - TOKENS.inset_right;
        let img = r.iter().find(|s| (s.y - tops[1]).abs() < 1e-9).expect("a rect at the image's top");
        assert!((img.x - TOKENS.inset_left).abs() < 1e-9 && (img.width - avail).abs() < 1e-9);
        assert!((img.height - image_height(avail, (200, 100))).abs() < 1e-9);
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
