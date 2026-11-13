//! Where the note's text is, in the paper's frame: one geometry source, [`Layout`].
//!
//! A [`Layout`] is the note at one paper size, one set of [`Tokens`] and one set of picture sizes:
//! where every block starts, where every line of it wraps, and what to draw (the words as [`Run`]s,
//! the bars as [`Rule`]s, the highlights as [`Marker`]s). It is built once, wrapping each text block
//! once, and every question about where text is (the caret's box, the caret under a point, the wash
//! of a selection, a picture's rect, the height the note scrolls through) is answered from it
//! without wrapping again. There is no other way to ask: the editor keeps the current `Layout` and
//! rebuilds it when the document, the paper, the tokens or a picture's size changes.
//!
//! The rest of this doc says why the geometry is computed here and not read back from the UI.
//!
//! The mockup text is tilted with the paper, but the software renderer cannot rotate text. At rest
//! the note therefore draws every word as its own upright `Text`, moved to where the tilted word
//! would be (the paper's tilt about the paper's center). While editing, the stock level `TextInput`
//! only places the caret and the selection, and the words stay where they are (tilted editing,
//! Settings › General) or settle flat. Everything here is in the level layout either way: the note
//! maps what it draws and what the pointer touches through the tilt. Both must break lines in the same
//! places, so this module wraps with the advances of the same embedded fonts (they have no kerning
//! table, so the advances are exact).
//!
//! The four faces (regular, bold, italic, bold italic) have the same advances, so a line breaks in
//! the same places whatever its characters' styles. The edit fields lay their text out in a blank
//! face with those advances too (`fonts/WashiCaret.ttf`), and the words are drawn from this layout
//! while editing as well, each piece in its own face.

use crate::document::{Block, Caret, Kind, Paint, Range, Style as Flags};
use crate::trace::{count, counts::Count};
use std::rc::Rc;
use ttf_parser::Face;

/// Calls to `wrap` on this thread since the last [`reset_wrap_calls`] (counted only in tests and
/// with the `trace` feature, see `trace::counts`).
#[cfg(any(test, feature = "trace"))]
pub fn wrap_calls() -> usize {
    crate::trace::counts::get(Count::Wrap)
}

/// Start counting `wrap` calls from zero.
#[cfg(any(test, feature = "trace"))]
pub fn reset_wrap_calls() {
    crate::trace::counts::clear(Count::Wrap);
}

/// A marker is a line pitch tall less this much at its top and bottom, so marked lines do not touch.
const MARKER_INSET: f64 = 1.0;

const REGULAR: &[u8] = include_bytes!("../../fonts/ShadowsIntoLight.ttf");
const BOLD: &[u8] = include_bytes!("../../fonts/ShadowsIntoLight-Bold.ttf");
const ITALIC: &[u8] = include_bytes!("../../fonts/ShadowsIntoLight-Italic.ttf");
const BOLD_ITALIC: &[u8] = include_bytes!("../../fonts/ShadowsIntoLight-BoldItalic.ttf");

/// The look of one kind of block: font size and line pitch, in logical px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub size: f64,
    pub pitch: f64,
}

/// Fixed logical values from `ui/theme.slint`.
#[derive(Debug, Clone, Copy, PartialEq)]
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
    /// Extra space after every character, in em of the text's size (Settings › Font & Style ›
    /// Letter spacing; 0 is the font's own spacing). The note's `Text`s and edit fields add the
    /// same `letter-spacing` after every character, so the lines break in the same places.
    pub letter_spacing: f64,
}

impl Tokens {
    fn measure(&self, size: f64) -> Measure {
        Measure { spacing: self.letter_spacing * size, ..Measure::new(false, false, size) }
    }

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
    /// Palette indices of the run's ink and marker (0: default ink, no marker); one value per run.
    pub ink: u8,
    pub marker: u8,
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

/// The four faces, parsed on first use and kept: regular, bold, italic, bold italic.
fn faces() -> &'static [Face<'static>; 4] {
    static FACES: std::sync::OnceLock<[Face<'static>; 4]> = std::sync::OnceLock::new();
    FACES.get_or_init(|| [REGULAR, BOLD, ITALIC, BOLD_ITALIC].map(|data| Face::parse(data, 0).expect("embedded font parses")))
}

struct Measure {
    face: &'static Face<'static>,
    scale: f64,
    /// Added to every character's advance (letter spacing), logical px.
    spacing: f64,
}

impl Measure {
    fn new(bold: bool, italic: bool, size: f64) -> Measure {
        let face = match (bold, italic) {
            (false, false) => &faces()[0],
            (true, false) => &faces()[1],
            (false, true) => &faces()[2],
            (true, true) => &faces()[3],
        };
        let scale = size / face.units_per_em() as f64;
        Measure { face, scale, spacing: 0.0 }
    }

    fn advance(&self, c: char) -> f64 {
        let g = self.face.glyph_index(c).unwrap_or(ttf_parser::GlyphId(0));
        self.face.glyph_hor_advance(g).unwrap_or(0) as f64 * self.scale + self.spacing
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
    count(Count::Wrap);
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

/// One line of text (the regular face) at `size` px cut into pieces of at most `max` width, as
/// (text, start x, width): each piece is drawn level on the tilted baseline, like the note's words.
pub fn line_pieces(text: &str, size: f64, max: f64) -> Vec<(String, f64, f64)> {
    let m = Measure::new(false, false, size);
    let mut at = 0.0;
    pieces(text, max, &m)
        .into_iter()
        .map(|p| {
            let w = m.width(&p);
            let out = (p, at, w);
            at += w;
            out
        })
        .collect()
}

/// The width of one line of text (the regular face) at `size` px.
pub fn line_width(text: &str, size: f64) -> f64 {
    Measure::new(false, false, size).width(text)
}

/// The stretches of `start..end` of a block drawn alike: in one face (bold and italic the same)
/// and in one ink and marker.
fn look_runs(b: &Block, start: usize, end: usize) -> Vec<(usize, usize)> {
    let face = |i: usize| (b.shown_at(i).bits() & (Flags::BOLD.bits() | Flags::ITALIC.bits()), b.paints[i]);
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
    let mut starts = Vec::with_capacity(lines.len());
    let mut prev = 0;
    for (i, l) in lines.iter().enumerate() {
        if i == 0 {
            starts.push(0);
        } else if let Some(w) = l.first() {
            prev = w.off;
            starts.push(w.off);
        } else {
            starts.push(prev);
        }
    }
    starts
}

/// The visual line and x of the caret at byte `offset` of a block's text, wrapped to `starts` (its
/// [`line_starts`]). Where a line wraps without a space the same offset ends one line and starts the
/// next: this is the start of the next, or with `line_end` the end of the one before.
fn caret_line_in(text: &str, starts: &[usize], m: &Measure, offset: usize, line_end: bool) -> CaretLine {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let mut line = starts.iter().rposition(|&s| s <= offset).unwrap_or(0);
    if line_end && line > 0 && starts[line] == offset {
        line -= 1;
    }
    CaretLine { line, lines: starts.len(), x: m.width(&text[starts[line]..offset]) }
}

/// The byte offset on visual `line` (clamped to the block's last) of a block's wrapped text nearest to
/// `x`, and whether that offset is the end of `line` where the next line starts at the same offset (a
/// wrap without a space): the caret is then on this line, not at the start of the next.
fn offset_in(text: &str, lines: &[Vec<Word>], starts: &[usize], m: &Measure, line: usize, x: f64) -> (usize, bool) {
    let line = line.min(lines.len() - 1);
    let start = starts[line];
    let line_words = &lines[line];
    let line_text_end = if line == lines.len() - 1 {
        text.len()
    } else {
        line_words.last().map_or(text.len(), |w| w.off + w.text.len())
    };
    let mut best = (start, f64::MAX);
    let mut at = 0.0;
    for (i, c) in text[start..line_text_end].char_indices().map(|(i, c)| (start + i, c)) {
        let d = (at - x).abs();
        if d < best.1 {
            best = (i, d);
        }
        at += m.advance(c);
    }
    let d = (at - x).abs();
    if d < best.1 {
        best = (line_text_end, d);
    }
    (best.0, best.0 > start && starts.get(line + 1) == Some(&best.0))
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

/// A marker (highlight) behind one line's stretch of one color: a rounded rect, level, in the paper's
/// frame. Its height is the line box minus 2 px. It spans the words and the spaces between them
/// whole, whatever the pieces they are drawn in, so the tilted drawing has no seams.
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub block: usize,
    /// Palette index (1..).
    pub marker: u8,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Where a selection wash (level boxes) meets the markers: the boxes the wash is drawn again in,
/// see-through, above the markers, so that a selected highlighted word shows its marker tinted.
pub fn marker_wash(markers: &[Marker], wash: &[Rect]) -> Vec<Rect> {
    let mut out = vec![];
    for m in markers {
        for w in wash {
            let (x0, x1) = (m.x.max(w.x), (m.x + m.width).min(w.x + w.width));
            let (y0, y1) = (m.y.max(w.y), (m.y + m.height).min(w.y + w.height));
            if x1 > x0 && y1 > y0 {
                out.push(Rect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 });
            }
        }
    }
    out
}

/// Where the baseline of a line is below the top of its line box: the font's ascent plus half of
/// the leading the line pitch adds to the font's natural line height.
fn baseline(m: &Measure, pitch: f64) -> f64 {
    let (asc, desc) = (m.face.ascender() as f64 * m.scale, m.face.descender() as f64 * m.scale);
    asc + (pitch - (asc - desc)) / 2.0
}

/// The height of an image at the width it is shown: the text-area width times its aspect ratio.
pub fn image_height(width: f64, pixels: (u32, u32)) -> f64 {
    if pixels.0 == 0 {
        0.0
    } else {
        width * pixels.1 as f64 / pixels.0 as f64
    }
}

/// The words, bars and markers of one text block whose lines start at `top`, pushed onto the lists.
#[allow(clippy::too_many_arguments)]
fn draw_block(b: &Block, bi: usize, top: f64, lines: &[Vec<Word>], m: &Measure, paper: (f64, f64), t: &Tokens, runs: &mut Vec<Run>, rules: &mut Vec<Rule>, markers: &mut Vec<Marker>) {
    let st = t.style(b.kind);
    let base = baseline(m, st.pitch);
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
        bar(rules, t.inset_left - 2.5 * d, top + base - 0.3 * st.size - d / 2.0, d, d);
    }
    for (li, line) in lines.iter().enumerate() {
        let y = top + li as f64 * st.pitch;
        for Word { text: word, at, off } in line {
            let mut x = t.inset_left + at;
            for (s, e) in look_runs(b, *off, *off + word.len()) {
                let shown = b.shown_at(s);
                let (bold, italic) = (shown.contains(Flags::BOLD), shown.contains(Flags::ITALIC));
                let Paint { ink, marker } = b.paints[s];
                for piece in pieces(&b.text[s..e], t.max_run, m) {
                    let width = m.width(&piece);
                    let (dx, dy) = tilt_offset(x + width / 2.0, y + st.pitch / 2.0, paper, t.tilt_deg);
                    runs.push(Run { block: bi, kind: b.kind, bold, italic, ink, marker, line: li, text: piece, x, y, width, size: st.size, pitch: st.pitch, dx, dy });
                    x += width;
                }
            }
        }
        if let (Some(first), Some(last)) = (line.first(), line.last()) {
            // from the line's first word to the end of its last (the spaces between words
            // included, those after the last left out)
            let (from, to) = (first.off, last.off + last.text.len());
            let x_of = |i: usize| t.inset_left + first.at + m.width(&b.text[from..i]);
            // a marker per stretch of one color, from the stretch's first character to its last
            let mut i = from;
            while i < to {
                let color = b.paints[i].marker;
                let j = (i..to).find(|&k| b.paints[k].marker != color).unwrap_or(to);
                if color != 0 {
                    markers.push(Marker { block: bi, marker: color, x: x_of(i), y: y + MARKER_INSET, width: x_of(j) - x_of(i), height: st.pitch - 2.0 * MARKER_INSET });
                }
                i = j;
            }
            for (flag, lift) in [(Flags::UNDERLINE, -0.1), (Flags::STRIKE, 0.3)] {
                let mut i = from;
                while i < to {
                    // every byte of a character has its mark: a change is at a character boundary
                    if !b.marks[i].contains(flag) {
                        i += 1;
                        continue;
                    }
                    let j = (i..to).find(|&k| !b.marks[k].contains(flag)).unwrap_or(to);
                    bar(rules, x_of(i), y + base - lift * st.size, x_of(j) - x_of(i), thick);
                    i = j;
                }
            }
        }
    }
}

/// A level box in the paper's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The wash of `from..to` of one text block (wrapped to `starts`) whose lines start at `top`; `closes`
/// when the range goes on into a later block, so the last line gets its pad. See [`Layout::selection_rects`].
#[allow(clippy::too_many_arguments)]
fn wash_block(text: &str, starts: &[usize], m: &Measure, st: Style, left: f64, top: f64, (from, to): (usize, usize), closes: bool, out: &mut Vec<Rect>) {
    for (li, &ls) in starts.iter().enumerate() {
        let last = li + 1 == starts.len();
        let le = if last { text.len() } else { starts[li + 1] };
        if (from >= le && !last) || to < ls {
            continue;
        }
        let (a, e) = (from.clamp(ls, le), to.clamp(ls, le));
        let pad = if last { closes } else { to > le };
        if a >= e && !pad {
            continue;
        }
        // the space a wrap breaks at is not shown washed: the pad stands for it
        let shown_end = if last { e } else { e.min(ls + text[ls..le].trim_end().len()).max(a) };
        let x0 = m.width(&text[ls..a]);
        let x1 = m.width(&text[ls..shown_end]);
        let tail = if pad { st.size * 0.3 } else { 0.0 };
        out.push(Rect { x: left + x0, y: top + li as f64 * st.pitch, width: x1 - x0 + tail, height: st.pitch });
    }
}

/// One block's text broken into lines: the words of each line, and the byte offset each line starts at.
struct Wrapped {
    lines: Vec<Vec<Word>>,
    starts: Vec<usize>,
}

struct BlockLayout {
    kind: Kind,
    /// Where the block starts, in the paper's frame.
    top: f64,
    /// The text this was wrapped from (queries measure in it).
    text: Rc<str>,
    lines: Rc<Wrapped>,
    /// x (from the text area's left edge), width and height of a picture block.
    image: Option<(f64, f64, f64)>,
}

/// A note's geometry at one paper size and one set of tokens: where every block starts and where
/// every line of it wraps. Built once; every question about where text is answers from it.
///
/// It keeps its own copy of each block's text and never looks at the document again, so it cannot be
/// asked about a document it was not built from: when the text, the paper, the tokens or a picture's
/// size changes, build another.
pub struct Layout {
    paper: (f64, f64),
    tokens: Tokens,
    blocks: Vec<BlockLayout>,
    /// The bottom of the last line (what the note scrolls through).
    bottom: f64,
    runs: Vec<Run>,
    rules: Vec<Rule>,
    markers: Vec<Marker>,
}

impl Layout {
    /// Lay `blocks` out in a paper of size `paper` (logical px), wrapping each text block once.
    /// `image_size` gives a picture's size in pixels (an image block is as wide as the text area and as tall as its aspect ratio makes it; a missing
    /// size: one body line).
    pub fn build(blocks: &[Block], paper: (f64, f64), t: &Tokens, image_size: &dyn Fn(&Block) -> Option<(u32, u32)>) -> Layout {
        count(Count::LayoutBuild);
        let avail = paper.0 - t.inset_left - t.inset_right;
        let (mut runs, mut rules, mut markers) = (vec![], vec![], vec![]);
        let mut laid = Vec::with_capacity(blocks.len());
        let mut top = t.inset_top;
        for (bi, b) in blocks.iter().enumerate() {
            let st = t.style(b.kind);
            if bi > 0 && group(blocks[bi - 1].kind) != group(b.kind) {
                top += if b.kind == Kind::Heading { t.heading_gap } else { t.body_gap };
            }
            if b.kind == Kind::Image {
                let image = image_size(b).map_or((0.0, avail, st.pitch), |px| b.image_box(avail, px));
                // a picture has one empty line, as far as the caret is concerned
                let lines = vec![vec![]];
                let starts = line_starts(&lines);
                laid.push(BlockLayout { kind: b.kind, top, text: Rc::from(&*b.text), lines: Rc::new(Wrapped { lines, starts }), image: Some(image) });
                top += image.2;
                continue;
            }
            let m = t.measure(st.size);
            let lines = wrap(&b.text, avail, &m);
            draw_block(b, bi, top, &lines, &m, paper, t, &mut runs, &mut rules, &mut markers);
            let starts = line_starts(&lines);
            let count = lines.len();
            laid.push(BlockLayout { kind: b.kind, top, text: Rc::from(&*b.text), lines: Rc::new(Wrapped { lines, starts }), image: None });
            // every block takes at least one line, even when empty
            top += count as f64 * st.pitch;
        }
        Layout { paper, tokens: *t, blocks: laid, bottom: top, runs, rules, markers }
    }

    /// The paper size this was laid out in.
    pub fn paper(&self) -> (f64, f64) {
        self.paper
    }

    /// The tokens this was laid out with.
    pub fn tokens(&self) -> &Tokens {
        &self.tokens
    }

    /// Every word (or piece of a word) to draw, upright, with its offset onto the tilted baseline.
    pub fn runs(&self) -> &[Run] {
        &self.runs
    }

    /// The bars: underlines, strike-throughs and list bullets.
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// The markers behind colored text.
    pub fn markers(&self) -> &[Marker] {
        &self.markers
    }

    /// The bottom of the last line in the paper's frame.
    pub fn content_height(&self) -> f64 {
        self.bottom
    }

    /// How many blocks the layout was built from.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Where block `block` starts, in the paper's frame.
    pub fn block_top(&self, block: usize) -> Option<f64> {
        self.blocks.get(block).map(|b| b.top)
    }

    fn measure(&self, b: &BlockLayout) -> Measure {
        self.tokens.measure(self.tokens.style(b.kind).size)
    }

    /// The visual line and x of the caret at byte `offset` of block `block`. Where a line wraps without a
    /// space the same offset ends one line and starts the next: `line_end` picks the end of the line
    /// before it (see [`CaretLine`]).
    pub fn caret_line(&self, block: usize, offset: usize, line_end: bool) -> Option<CaretLine> {
        let b = self.blocks.get(block)?;
        Some(caret_line_in(&b.text, &b.lines.starts, &self.measure(b), offset, line_end))
    }

    /// The byte offset on visual `line` of block `block` (clamped to its last line) nearest to `x`, and
    /// whether it is the end of `line` where the next line starts at the same offset.
    pub fn offset_on_line(&self, block: usize, line: usize, x: f64) -> Option<(usize, bool)> {
        let b = self.blocks.get(block)?;
        Some(offset_in(&b.text, &b.lines.lines, &b.lines.starts, &self.measure(b), line, x))
    }

    /// Where the caret at byte `offset` of text block `block` is drawn: its x, and the top and bottom
    /// of its line box. `line_end` as in [`Self::caret_line`]. An image has no caret.
    pub fn caret_box(&self, block: usize, offset: usize, line_end: bool) -> Option<(f64, f64, f64)> {
        count(Count::CaretBox);
        let b = self.blocks.get(block).filter(|b| b.kind != Kind::Image)?;
        let pitch = self.tokens.style(b.kind).pitch;
        let at = caret_line_in(&b.text, &b.lines.starts, &self.measure(b), offset, line_end);
        let y = b.top + at.line as f64 * pitch;
        Some((self.tokens.inset_left + at.x, y, y + pitch))
    }

    /// The top and bottom of the line holding the caret (byte `offset` of block `block`).
    pub fn caret_span(&self, block: usize, offset: usize) -> Option<(f64, f64)> {
        let b = self.blocks.get(block)?;
        let pitch = self.tokens.style(b.kind).pitch;
        let y = b.top + caret_line_in(&b.text, &b.lines.starts, &self.measure(b), offset, false).line as f64 * pitch;
        Some((y, y + pitch))
    }

    /// The caret under `point` (level paper frame, x from the paper's left edge): the block whose top is
    /// the last one above the point, the visual line within it, then the nearest character boundary.
    /// Above the first block is the start of the note, below the last its end. Anywhere on an image
    /// is that image's block (an image caret has offset 0), so a click selects the picture. Also
    /// whether the caret is at the end of the line under the point (see [`Self::offset_on_line`]).
    pub fn hit_on_line(&self, point: (f64, f64)) -> (Caret, bool) {
        let Some(last) = self.blocks.len().checked_sub(1) else { return (Caret { block: 0, offset: 0 }, false) };
        if point.1 < self.blocks[0].top {
            return (Caret { block: 0, offset: 0 }, false);
        }
        if point.1 >= self.bottom {
            let b = &self.blocks[last];
            return (Caret { block: last, offset: if b.kind == Kind::Image { 0 } else { b.text.len() } }, false);
        }
        let block = self.blocks.iter().rposition(|b| b.top <= point.1).unwrap_or(0);
        let b = &self.blocks[block];
        if b.kind == Kind::Image {
            return (Caret { block, offset: 0 }, false);
        }
        let pitch = self.tokens.style(b.kind).pitch;
        let line = ((point.1 - b.top) / pitch).floor().max(0.0) as usize;
        let (offset, line_end) = offset_in(&b.text, &b.lines.lines, &b.lines.starts, &self.measure(b), line, point.0 - self.tokens.inset_left);
        (Caret { block, offset }, line_end)
    }

    /// The wash of a selection: a level rect for each visual line the range touches, from the caret x of
    /// where the range starts on the line to where it ends. A line that ends inside the range (a wrap,
    /// or a paragraph break) gets a trailing pad of 0.3 em, so an empty paragraph shows a sliver. An
    /// image in the range is washed over its whole size.
    pub fn selection_rects(&self, range: Range) -> Vec<Rect> {
        let (start, end) = range.ordered();
        if start == end {
            return vec![];
        }
        let t = &self.tokens;
        let mut out = vec![];
        for bi in start.block..=end.block.min(self.blocks.len().saturating_sub(1)) {
            let Some(b) = self.blocks.get(bi) else { break };
            if let Some((x, width, height)) = b.image {
                out.push(Rect { x: t.inset_left + x, y: b.top, width, height });
                continue;
            }
            let from = if bi == start.block { start.offset.min(b.text.len()) } else { 0 };
            let to = if bi == end.block { end.offset.min(b.text.len()) } else { b.text.len() };
            wash_block(&b.text, &b.lines.starts, &self.measure(b), t.style(b.kind), t.inset_left, b.top, (from, to), bi < end.block, &mut out);
        }
        out
    }

    /// Where the picture of block `block` is drawn (level paper frame); `None` when it is no picture.
    pub fn image_rect(&self, block: usize) -> Option<Rect> {
        let b = self.blocks.get(block)?;
        let (x, width, height) = b.image?;
        Some(Rect { x: self.tokens.inset_left + x, y: b.top, width, height })
    }
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
        letter_spacing: 0.0,
    };

    const PAPER: (f64, f64) = (363.0, 433.0);

    fn lay(blocks: &[Block], paper: (f64, f64), t: &Tokens) -> Layout {
        Layout::build(blocks, paper, t, &|_| None)
    }

    fn runs_of(blocks: &[Block], paper: (f64, f64), t: &Tokens) -> Vec<Run> {
        lay(blocks, paper, t).runs().to_vec()
    }

    fn all_of(blocks: &[Block], paper: (f64, f64), t: &Tokens) -> (Vec<Run>, Vec<Rule>, Vec<Marker>) {
        let l = lay(blocks, paper, t);
        (l.runs().to_vec(), l.rules().to_vec(), l.markers().to_vec())
    }

    /// A layout of one block alone, as the first of a note.
    fn one(b: &Block) -> Layout {
        lay(std::slice::from_ref(b), PAPER, &TOKENS)
    }

    /// The caret of byte `offset` of `b` (the start of the next line where a line wraps without a space).
    fn caret_line(b: &Block, offset: usize) -> CaretLine {
        one(b).caret_line(0, offset, false).unwrap()
    }

    /// The offset nearest to `x` on `line` of `b`.
    fn offset_at(b: &Block, line: usize, x: f64) -> usize {
        one(b).offset_on_line(0, line, x).unwrap().0
    }

    const MOCKUP: &str = "# Endfield Note\n## **Thermite Cutter : POG & Akekuri**\nCurrently POG no Weapon : Ideal for him are Never Rest or White Night Nova. The last can be get in a box";

    #[test]
    fn block_tops_and_content_height_are_where_the_runs_start_and_the_last_line_ends() {
        let doc = Document::parse(MOCKUP);
        let layout = lay(doc.blocks(), PAPER, &TOKENS);
        for bi in 0..doc.blocks().len() {
            let first = layout.runs().iter().find(|r| r.block == bi).unwrap();
            assert!((first.y - layout.block_top(bi).unwrap()).abs() < 1e-9);
        }
        assert_eq!(layout.block_top(doc.blocks().len()), None);
        let last = layout.runs().last().unwrap();
        assert!((layout.content_height() - (last.y + last.pitch)).abs() < 1e-9);
        // the gaps: none above the first block, a heading gap before the heading, a body gap before the body
        assert_eq!(layout.block_top(0), Some(TOKENS.inset_top));
        let t1 = layout.block_top(1).unwrap();
        assert!((t1 - (TOKENS.inset_top + TOKENS.title.pitch + TOKENS.heading_gap)).abs() < 1e-9);
        let t2 = layout.block_top(2).unwrap();
        assert!((t2 - (t1 + 2.0 * TOKENS.heading.pitch + TOKENS.body_gap)).abs() < 1e-9);
    }

    #[test]
    fn caret_span_is_the_line_box_of_the_caret() {
        let doc = Document::parse(MOCKUP);
        let layout = lay(doc.blocks(), PAPER, &TOKENS);
        let last_len = doc.blocks()[2].text.len();
        let (top, bottom) = layout.caret_span(2, last_len).unwrap();
        let last = layout.runs().last().unwrap();
        assert_eq!((top, bottom), (last.y, last.y + last.pitch));
        assert_eq!(layout.caret_span(9, 0), None);
    }

    fn line_texts(runs: &[Run], block: usize) -> Vec<String> {
        let n = runs.iter().filter(|r| r.block == block).map(|r| r.line).max().map_or(0, |l| l + 1);
        (0..n)
            .map(|l| runs.iter().filter(|r| r.block == block && r.line == l).map(|r| r.text.as_str()).collect::<Vec<_>>().join(" "))
            .collect()
    }

    #[test]
    fn the_mockup_breaks_where_the_mockup_does() {
        let runs = runs_of(Document::parse(MOCKUP).blocks(), PAPER, &TOKENS);
        assert_eq!(line_texts(&runs, 0), ["Endfield Note"]);
        assert_eq!(line_texts(&runs, 1), ["Thermite Cutter : POG &", "Akekuri"]);
        // the body is one block per stored line, but the mockup's body is one long line
        assert_eq!(line_texts(&runs, 2), ["Currently POG no Weapon : Ideal", "for him are Never Rest or White", "Night Nova. The last can be get", "in a box"]);
    }

    #[test]
    fn words_on_one_line_follow_the_papers_tilt() {
        let runs = runs_of(Document::parse(MOCKUP).blocks(), PAPER, &TOKENS);
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
        let runs = runs_of(Document::parse(MOCKUP).blocks(), PAPER, &tokens);
        let whole = runs_of(Document::parse(MOCKUP).blocks(), PAPER, &TOKENS);
        // same words, same line breaks, same text
        assert_eq!(runs.iter().map(|r| r.text.as_str()).collect::<String>(), whole.iter().map(|r| r.text.as_str()).collect::<String>());
        assert_eq!(runs.iter().map(|r| r.line + 10 * r.block).max(), whole.iter().map(|r| r.line + 10 * r.block).max());
        assert!(runs.len() > whole.len() * 2);
        // every piece sits within 0.3 px of the exact tilt of its own left and right end
        let s = tokens.tilt_deg.to_radians().sin();
        for r in &runs {
            for end in [r.x, r.x + r.width] {
                let exact = tilt_offset(end, r.y + r.pitch / 2.0, PAPER, tokens.tilt_deg).1;
                let _ = s;
                assert!((exact - r.dy).abs() < 0.3, "{:?}: {} vs {}", r.text, exact, r.dy);
            }
        }
    }

    #[test]
    fn the_paper_centre_does_not_move() {
        let (dx, dy) = tilt_offset(181.5, 216.5, PAPER, 1.45);
        assert!(dx.abs() < 1e-9 && dy.abs() < 1e-9);
    }

    #[test]
    fn a_bigger_paper_re_wraps_but_keeps_insets() {
        let narrow = runs_of(Document::parse(MOCKUP).blocks(), PAPER, &TOKENS);
        let wide = runs_of(Document::parse(MOCKUP).blocks(), (600.0, 433.0), &TOKENS);
        assert_eq!(narrow[0].x, wide[0].x);
        assert_eq!(narrow[0].y, wide[0].y);
        assert_eq!(line_texts(&wide, 1), ["Thermite Cutter : POG & Akekuri"]);
    }

    #[test]
    fn an_empty_title_still_takes_a_line_and_a_long_word_breaks_anywhere() {
        let runs = runs_of(Document::parse("\nx").blocks(), PAPER, &TOKENS);
        assert_eq!(runs.len(), 1);
        assert!((runs[0].y - (38.2 + 60.0 + 8.1)).abs() < 1e-9);
        let long = "w".repeat(80);
        let runs = runs_of(Document::parse(&format!("t
{long}")).blocks(), PAPER, &TOKENS);
        assert!(runs.iter().filter(|r| r.block == 1).map(|r| r.line).max().unwrap() >= 1);
    }

    #[test]
    fn lines_break_after_hyphens_like_the_edit_fields_do() {
        // no spaces at all: only the hyphens let it break between words
        let text = "t\nlibopenssl-sys-bfe5c79-libcrypto-lib-ct-b64";
        let runs = runs_of(Document::parse(text).blocks(), PAPER, &TOKENS);
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
        let (runs, rules, _) = all_of(styled.blocks(), PAPER, &TOKENS);
        let plain_runs = runs_of(plain.blocks(), PAPER, &TOKENS);
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
        let first = caret_line(body, 0);
        assert_eq!((first.line, first.lines, first.x), (0, 4, 0.0));
        // "for" starts the second line
        let second = caret_line(body, "Currently POG no Weapon : Ideal ".len());
        assert_eq!((second.line, second.x), (1, 0.0));
        let last = caret_line(body, body.text.len());
        assert_eq!(last.line, 3);
        assert!(last.x > 50.0);
        // an empty block is one line
        assert_eq!(caret_line(&Block::new(Kind::Body, ""), 0).lines, 1);
    }

    #[test]
    fn offset_at_is_the_inverse_of_caret_line() {
        let d = Document::parse(MOCKUP);
        for b in d.blocks() {
            for o in (0..=b.text.len()).filter(|&o| b.text.is_char_boundary(o)) {
                let c = caret_line(b, o);
                let back = offset_at(b, c.line, c.x);
                // a caret after a wrapping space sits where the next line starts, so compare the position
                assert_eq!(caret_line(b, back), c, "{:?} at {o}", b.text);
            }
        }
    }

    /// The centre of the caret's line box for `c`, in the paper's frame (level).
    fn point_of(d: &Document, c: Caret) -> (f64, f64) {
        let layout = lay(d.blocks(), PAPER, &TOKENS);
        let b = &d.blocks()[c.block];
        let cl = layout.caret_line(c.block, c.offset, false).unwrap();
        let pitch = TOKENS.style(b.kind).pitch;
        (TOKENS.inset_left + cl.x, layout.block_top(c.block).unwrap() + (cl.line as f64 + 0.5) * pitch)
    }

    #[test]
    fn hit_is_the_inverse_of_the_caret_position_for_every_offset() {
        let d = Document::parse(MOCKUP);
        let layout = lay(d.blocks(), PAPER, &TOKENS);
        for (block, b) in d.blocks().iter().enumerate() {
            for offset in (0..=b.text.len()).filter(|&o| b.text.is_char_boundary(o)) {
                let c = Caret { block, offset };
                let got = layout.hit_on_line(point_of(&d, c)).0;
                assert_eq!(got.block, block);
                // a caret after a wrapping space sits where the next line starts: compare positions
                assert_eq!(layout.caret_line(got.block, got.offset, false), layout.caret_line(block, offset, false), "block {block} offset {offset}");
            }
        }
    }

    #[test]
    fn hit_snaps_above_the_first_block_below_the_last_and_to_the_sides() {
        let d = Document::parse(MOCKUP);
        let layout = lay(d.blocks(), PAPER, &TOKENS);
        let hit = |at: (f64, f64)| layout.hit_on_line(at).0;
        assert_eq!(hit((100.0, 2.0)), Caret { block: 0, offset: 0 });
        let last = d.blocks().len() - 1;
        let len = d.blocks()[last].text.len();
        assert_eq!(hit((100.0, 5000.0)), Caret { block: last, offset: len });
        // left of the text area is the start of the line, right of it the end
        let (_, y) = point_of(&d, Caret { block: 0, offset: 0 });
        assert_eq!(hit((-50.0, y)).offset, 0);
        assert_eq!(hit((5000.0, y)).offset, d.blocks()[0].text.len());
        // the gap between two blocks belongs to the block above it
        assert_eq!(hit((5000.0, layout.block_top(1).unwrap() - 1.0)).block, 0);
    }

    #[test]
    fn hit_anywhere_on_an_image_is_that_image_block() {
        let d = Document::from_blocks(vec![Block::new(Kind::Title, "t"), Block::image(1), Block::new(Kind::Body, "after")]);
        let size = |b: &Block| b.image_id.map(|_| (200, 100));
        let layout = Layout::build(d.blocks(), PAPER, &TOKENS, &size);
        let top = layout.block_top(1).unwrap();
        let h = image_height(363.0 - TOKENS.inset_left - TOKENS.inset_right, (200, 100));
        assert_eq!(layout.hit_on_line((100.0, top + h * 0.25)), (Caret { block: 1, offset: 0 }, false));
        assert_eq!(layout.hit_on_line((100.0, top + h * 0.75)), (Caret { block: 1, offset: 0 }, false));
    }

    #[test]
    fn clicking_to_the_right_of_a_wrapped_sentence_snaps_to_the_end_of_the_sentence_not_trailing_whitespace() {
        let b = Block::new(Kind::Body, "Sentence one wraps here. Sentence two continues on the next line.");
        let line0_end = "Sentence one wraps here.".len();
        let at_end = offset_at(&b, 0, 5000.0);
        assert_eq!(at_end, line0_end, "Caret must sit directly after the sentence on line 0, before any wrapped spaces");
    }

    #[test]
    fn clicking_to_the_right_of_trailing_whitespace_on_final_line_snaps_to_end_of_text() {
        // Single-line block with trailing spaces
        let single = Block::new(Kind::Body, "Single line with trailing spaces   ");
        let at_single_end = offset_at(&single, 0, 5000.0);
        assert_eq!(at_single_end, single.text.len(), "Single-line block must snap caret to end of text including trailing spaces");

        // Multi-line block with trailing spaces on the final line
        let multi = Block::new(Kind::Body, "Sentence one wraps here. Sentence two continues on the next line with trailing   ");
        let last_line = caret_line(&multi, multi.text.len()).line;
        assert!(last_line > 0, "multi block must wrap into multiple lines");

        // Intermediate lines continue to position caret directly after the sentence, before wrapped whitespace
        let line0_end = "Sentence one wraps here.".len();
        let at_line0 = offset_at(&multi, 0, 5000.0);
        assert_eq!(at_line0, line0_end, "Soft-wrapped line 0 must continue to snap before wrapped whitespace");

        // Final line positions caret at the end of the text, after trailing whitespace
        let at_final_line = offset_at(&multi, last_line, 5000.0);
        assert_eq!(at_final_line, multi.text.len(), "Final line of multi-line block must snap caret to end of text including trailing spaces");

        // Clamping to a line index beyond the last line also lands at the end of text
        let at_clamped = offset_at(&multi, 999, 5000.0);
        assert_eq!(at_clamped, multi.text.len(), "Line index beyond last must clamp to final line and snap to end of text");
    }

    fn rects(d: &Document, range: Range) -> Vec<Rect> {
        lay(d.blocks(), PAPER, &TOKENS).selection_rects(range)
    }

    #[test]
    fn selection_rects_cover_exactly_the_lines_in_the_range() {
        let d = Document::parse(MOCKUP);
        // the whole body: four lines, one level rect each, stacked at the body's pitch
        let body = Range { anchor: Caret { block: 2, offset: 0 }, focus: Caret { block: 2, offset: d.blocks()[2].text.len() } };
        let r = rects(&d, body);
        let tops = [0, 1, 2].map(|i| lay(d.blocks(), PAPER, &TOKENS).block_top(i).unwrap());
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
        let (x0, x1) = (caret_line(b, 2).x, caret_line(b, 6).x);
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
        let w = caret_line(b, 2).x - caret_line(b, 1).x;
        assert!((r[0].width - (w + pad)).abs() < 1e-9);
        // the empty paragraph: nothing but the pad
        assert!((r[1].width - TOKENS.body.size * 0.3).abs() < 1e-9 && r[1].width > 1.0);
        // the last line ends in the range's end: no pad
        let c = caret_line(&d.blocks()[2], 1).x;
        assert!((r[2].width - c).abs() < 1e-9);
    }

    #[test]
    fn an_image_inside_the_range_is_washed_over_its_whole_size() {
        let d = Document::from_blocks(vec![Block::new(Kind::Title, "t"), Block::image(1), Block::new(Kind::Body, "after")]);
        let size = |b: &Block| b.image_id.map(|_| (200, 100));
        let range = Range { anchor: Caret { block: 0, offset: 0 }, focus: Caret { block: 2, offset: 3 } };
        let layout = Layout::build(d.blocks(), PAPER, &TOKENS, &size);
        let r = layout.selection_rects(range);
        let tops = [0, 1, 2].map(|i| layout.block_top(i).unwrap());
        let avail = 363.0 - TOKENS.inset_left - TOKENS.inset_right;
        let img = r.iter().find(|s| (s.y - tops[1]).abs() < 1e-9).expect("a rect at the image's top");
        assert!((img.x - TOKENS.inset_left).abs() < 1e-9 && (img.width - avail).abs() < 1e-9);
        assert!((img.height - image_height(avail, (200, 100))).abs() < 1e-9);
    }

    /// The runs and markers of block 2 of the mockup, with ink and markers set at `spans`
    /// `(start, end, ink, marker)` (None leaves that one alone).
    fn painted(tokens: &Tokens, spans: &[(usize, usize, Option<u8>, Option<u8>)]) -> (Vec<Run>, Vec<Marker>) {
        let mut d = Document::parse(MOCKUP);
        for &(s, e, ink, marker) in spans {
            if let Some(i) = ink {
                d.set_ink(2, s, e, i);
            }
            if let Some(m) = marker {
                d.set_marker(2, s, e, m);
            }
        }
        let (runs, _, markers) = all_of(d.blocks(), PAPER, tokens);
        (runs.into_iter().filter(|r| r.block == 2).collect(), markers.into_iter().filter(|m| m.block == 2).collect())
    }

    #[test]
    fn runs_split_where_the_paint_changes_like_they_do_where_the_style_does() {
        // "POG" blue ink; "POG no" yellow marker; "pon" of "Weapon" red ink
        let (runs, _) = painted(&TOKENS, &[(10, 13, Some(6), None), (10, 16, None, Some(4)), (20, 23, Some(9), None)]);
        let find = |t: &str| runs.iter().find(|r| r.text == t).unwrap_or_else(|| panic!("no run {t:?}"));
        assert_eq!((find("POG").ink, find("POG").marker), (6, 4));
        assert_eq!((find("no").ink, find("no").marker), (0, 4));
        assert_eq!((find("Wea").ink, find("pon").ink), (0, 9));
        assert!((find("Wea").x + find("Wea").width - find("pon").x).abs() < 1e-9);
        assert_eq!((find("Currently").ink, find("Currently").marker), (0, 0));
        // an uncolored note keeps its runs
        let (plain, _) = painted(&TOKENS, &[]);
        assert!(plain.iter().all(|r| r.ink == 0 && r.marker == 0));
    }

    #[test]
    fn a_marker_is_one_rect_per_line_span_merged_across_pieces() {
        for tokens in [TOKENS, Tokens { max_run: 16.0, ..TOKENS }] {
            let (runs, markers) = painted(&tokens, &[(10, 16, None, Some(4))]);
            assert_eq!(markers.len(), 1, "{markers:?}");
            let (first, last) = (runs.iter().find(|r| r.marker == 4).unwrap(), runs.iter().rfind(|r| r.marker == 4).unwrap());
            let m = &markers[0];
            assert_eq!(m.marker, 4);
            assert!((m.x - first.x).abs() < 1e-9 && (m.x + m.width - (last.x + last.width)).abs() < 1e-9, "{m:?}");
            // the line box minus 2 px
            assert!((m.y - (first.y + 1.0)).abs() < 1e-9 && (m.height - (first.pitch - 2.0)).abs() < 1e-9);
        }
        // a marker over the whole body: one rect on each of its four lines
        let len = Document::parse(MOCKUP).blocks()[2].text.len();
        let (runs, markers) = painted(&TOKENS, &[(0, len, None, Some(6))]);
        assert_eq!(markers.len(), 4);
        // a wrapping space is not washed in the marker: the rect ends with the line's last word
        let line0_end = runs.iter().filter(|r| r.line == 0).map(|r| r.x + r.width).fold(0.0, f64::max);
        assert!((markers[0].x + markers[0].width - line0_end).abs() < 1e-9);
    }

    #[test]
    fn neighbouring_markers_touch_but_never_overlap() {
        let (runs, markers) = painted(&TOKENS, &[(10, 13, None, Some(6)), (13, 16, None, Some(9))]);
        assert_eq!(markers.iter().map(|m| m.marker).collect::<Vec<_>>(), [6, 9]);
        assert!(markers[0].x + markers[0].width <= markers[1].x + 1e-9);
        let no = runs.iter().find(|r| r.text == "no").unwrap();
        assert!(markers[1].x + markers[1].width <= no.x + no.width + 1e-9);
        // a marker that skips a word leaves a gap there
        let (_, gap) = painted(&TOKENS, &[(10, 13, None, Some(6)), (14, 16, None, Some(6))]);
        assert_eq!(gap.len(), 2);
        assert!(gap[0].x + gap[0].width < gap[1].x);
    }

    #[test]
    fn the_wash_of_a_selection_across_blocks_is_redrawn_over_the_markers_it_meets() {
        let mut d = Document::parse(MOCKUP);
        d.set_marker(2, 10, 16, 4);
        let (_, _, markers) = all_of(d.blocks(), PAPER, &TOKENS);
        let m = &markers[0];
        let sel = [Rect { x: m.x + 5.0, y: 0.0, width: 10.0, height: 1000.0 }];
        let over = marker_wash(&markers, &sel);
        assert_eq!(over.len(), 1);
        let want = [m.x + 5.0, 10.0, m.y, m.height];
        for (got, want) in [over[0].x, over[0].width, over[0].y, over[0].height].into_iter().zip(want) {
            assert!((got - want).abs() < 1e-9, "{over:?} vs {m:?}");
        }
        assert!(marker_wash(&markers, &[]).is_empty());
    }

    #[test]
    fn offset_at_picks_the_nearest_boundary_on_the_line_and_clamps() {
        let d = Document::parse(MOCKUP);
        let body = &d.blocks()[2];
        assert_eq!(offset_at(body, 0, 0.0), 0);
        assert_eq!(offset_at(body, 1, 0.0), "Currently POG no Weapon : Ideal ".len());
        assert_eq!(offset_at(body, 3, 1e6), body.text.len());
        assert_eq!(offset_at(body, 99, 0.0), "Currently POG no Weapon : Ideal for him are Never Rest or White Night Nova. The last can be get ".len());
        // far right of a wrapped line stays on that line
        let end0 = offset_at(body, 0, 1e6);
        assert_eq!(caret_line(body, end0).line, 0);
        let multi = Block::new(Kind::Body, "héllo wörld");
        assert!(multi.text.is_char_boundary(offset_at(&multi, 0, 20.0)));
    }

    /// A body block of one unbroken word, so it wraps without a space: the same offset ends line 0 and
    /// starts line 1.
    fn long_word() -> Block {
        Block::new(Kind::Body, &"w".repeat(80))
    }

    #[test]
    fn a_wrap_without_a_space_has_a_caret_on_either_side_of_it() {
        let b = long_word();
        let layout = one(&b);
        let (end, at_end) = layout.offset_on_line(0, 0, 1e6).unwrap();
        assert!(at_end && end > 0 && end < b.text.len(), "line 0 ends inside the word: {end}");
        assert_eq!(layout.offset_on_line(0, 1, 0.0), Some((end, false)), "line 1 starts at the same offset");
        // the start of the next line, or the end of the line before it
        let (next, prev) = (layout.caret_line(0, end, false).unwrap(), layout.caret_line(0, end, true).unwrap());
        assert_eq!((next.line, next.x), (1, 0.0));
        assert_eq!(prev.line, 0);
        assert!(prev.x > 100.0);
        // the first line's start has no line before it
        assert_eq!(layout.caret_line(0, 0, true).unwrap().line, 0);
        // the same two places are drawn on two lines
        let (px, py, _) = layout.caret_box(0, end, true).unwrap();
        let (nx, ny, _) = layout.caret_box(0, end, false).unwrap();
        assert!((px - (TOKENS.inset_left + prev.x)).abs() < 1e-9 && (nx - TOKENS.inset_left).abs() < 1e-9);
        assert!((ny - py - TOKENS.body.pitch).abs() < 1e-9);
        // a click at the far right of line 0 is at its end, one at the far left of line 1 at its start
        let top = layout.block_top(0).unwrap();
        assert_eq!(layout.hit_on_line((5000.0, top + 5.0)), (Caret { block: 0, offset: end }, true));
        assert_eq!(layout.hit_on_line((0.0, top + TOKENS.body.pitch + 5.0)), (Caret { block: 0, offset: end }, false));
    }

    #[test]
    fn the_caret_box_is_the_line_box_of_the_caret_and_an_image_has_none() {
        let d = Document::from_blocks(vec![Block::new(Kind::Title, "ab"), Block::image(1), Block::new(Kind::Body, "cd")]);
        let size = |b: &Block| b.image_id.map(|_| (200, 100));
        let layout = Layout::build(d.blocks(), PAPER, &TOKENS, &size);
        let (x, top, bottom) = layout.caret_box(0, 1, false).unwrap();
        let width_of_a = caret_line(&d.blocks()[0], 1).x;
        assert!((x - (TOKENS.inset_left + width_of_a)).abs() < 1e-9);
        assert_eq!((top, bottom), (TOKENS.inset_top, TOKENS.inset_top + TOKENS.title.pitch));
        assert_eq!(layout.caret_box(1, 0, false), None);
        assert_eq!(layout.caret_box(7, 0, false), None);
        // below the picture: block 2 starts after the picture's height and the gap to a body
        let avail = 363.0 - TOKENS.inset_left - TOKENS.inset_right;
        let (_, top2, _) = layout.caret_box(2, 0, false).unwrap();
        assert!((top2 - (layout.block_top(1).unwrap() + image_height(avail, (200, 100)) + TOKENS.body_gap)).abs() < 1e-9);
    }

    #[test]
    fn queries_past_the_last_block_answer_none() {
        let layout = lay(Document::parse(MOCKUP).blocks(), PAPER, &TOKENS);
        assert_eq!(layout.caret_line(3, 0, false), None);
        assert_eq!(layout.offset_on_line(3, 0, 0.0), None);
        assert_eq!(layout.caret_span(3, 0), None);
        assert_eq!(layout.caret_box(3, 0, false), None);
        assert_eq!(layout.image_rect(3), None);
        assert_eq!(layout.block_top(3), None);
        // an offset past the end is the end
        let doc = Document::parse(MOCKUP);
        let body = &doc.blocks()[2].text;
        assert_eq!(layout.caret_line(2, body.len() + 50, false), layout.caret_line(2, body.len(), false));
    }

    #[test]
    fn an_image_rect_is_the_pictures_box_in_every_alignment_and_none_for_text() {
        let avail = 363.0 - TOKENS.inset_left - TOKENS.inset_right;
        let mut left = Block::image(1);
        left.image_pct = 100;
        let mut centre = Block::image(2);
        centre.align = crate::document::Align::Center;
        centre.image_pct = 50;
        let mut right = Block::image(3);
        right.align = crate::document::Align::Right;
        right.image_pct = 25;
        let d = Document::from_blocks(vec![Block::new(Kind::Title, "t"), left, centre, right, Block::image(4)]);
        // pictures 1 to 3 are 200x100, the fourth has no size yet
        let size = |b: &Block| b.image_id.filter(|&id| id < 4).map(|_| (200, 100));
        let layout = Layout::build(d.blocks(), PAPER, &TOKENS, &size);
        assert_eq!(layout.image_rect(0), None);
        assert_eq!(layout.image_rect(5), None);
        let want = |bi: usize, x: f64, width: f64, height: f64| Rect { x: TOKENS.inset_left + x, y: layout.block_top(bi).unwrap(), width, height };
        assert_eq!(layout.image_rect(1), Some(want(1, 0.0, avail, avail / 2.0)));
        assert_eq!(layout.image_rect(2), Some(want(2, avail / 4.0, avail / 2.0, avail / 4.0)));
        assert_eq!(layout.image_rect(3), Some(want(3, avail * 0.75, avail / 4.0, avail / 8.0)));
        // a picture with no size yet takes the text width and one body line
        assert_eq!(layout.image_rect(4), Some(want(4, 0.0, avail, TOKENS.body.pitch)));
        // the pictures stack: each starts where the one above it ends
        assert!((layout.block_top(2).unwrap() - (layout.block_top(1).unwrap() + avail / 2.0)).abs() < 1e-9);
        assert!((layout.content_height() - (layout.block_top(4).unwrap() + TOKENS.body.pitch)).abs() < 1e-9);
    }
}

/// What building a `Layout` costs: one wrap per text block, and none for any question asked of it.
#[cfg(test)]
mod layout_wraps {
    use super::*;
    use crate::document::Document;

    const NOTES: [&str; 3] = [
        include_str!("../../docs/design/fixtures/colored.md"),
        include_str!("../../docs/design/fixtures/long.md"),
        include_str!("../../tools/snap/tests/fixtures/mixed-note.md"),
    ];

    fn size(b: &Block) -> Option<(u32, u32)> {
        b.image_id.map(|_| (200, 100))
    }

    #[test]
    fn building_wraps_each_text_block_exactly_once_and_queries_never_wrap() {
        for text in NOTES {
            let mut blocks = Document::parse(text).blocks().to_vec();
            blocks.insert(1, Block::image(1));
            let doc = Document::from_blocks(blocks);
            let text_blocks = doc.blocks().iter().filter(|b| b.kind != Kind::Image).count();
            reset_wrap_calls();
            let layout = Layout::build(doc.blocks(), (363.0, 433.0), &super::tests::TOKENS, &size);
            assert_eq!(wrap_calls(), text_blocks, "wraps to build");
            let _ = (
                layout.runs(),
                layout.content_height(),
                layout.block_top(0),
                layout.caret_box(0, 0, false),
                layout.caret_span(1, 0),
                layout.caret_line(0, 3, true),
                layout.hit_on_line((50.0, 100.0)),
                layout.selection_rects(doc.all()),
                layout.image_rect(1),
                layout.offset_on_line(0, 0, 10.0),
            );
            assert_eq!(wrap_calls(), text_blocks, "queries wrap nothing");
        }
    }
}
