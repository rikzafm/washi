//! The note as a document: an ordered list of blocks (paragraphs), the first always the title.
//!
//! This is the one model the editor, the renderer and storage share. The stored form is a small
//! Markdown subset, one block per line (`parse` / `to_text`):
//!
//! ```text
//! # Endfield Note                    Title (always the first line)
//! ## **Thermite Cutter : POG**       Heading (+ bold)
//! Currently **POG** no *Weapon*      Body, with bold and italic words
//! - list item                        ListItem
//! ![](img:42)                        Image
//! ```
//!
//! Every character has its own style (bold, italic, underline, strike). In the stored line a marker
//! switches a style on or off where it changes: `**` bold, `*` italic, `~~` strike, `<u>` and `</u>`
//! underline. Every `\`, `*`, `~` and `<` of the text is escaped with a backslash, and so is a first
//! character that could be read as a block marker (`#`, `-`, `!`), so every document survives
//! `to_text` then `parse` exactly. A line that is not exactly what `to_text` writes (notes from
//! older versions, where the markers could only wrap a whole line and text was not escaped) is
//! read the old way, so no input is malformed and no old note changes its meaning.
//!
//! Editing operations are pure methods on [`Document`]. Offsets are byte offsets into a block's
//! text (what the text field reports), on char boundaries (clamped if not). Block text never
//! contains a line break.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Title,
    Heading,
    Body,
    ListItem,
    Image,
}

/// Bold / italic / underline / strike of a character (or, as a set, of a stretch of text).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style(u8);

impl Style {
    pub const NONE: Style = Style(0);
    pub const BOLD: Style = Style(1);
    pub const ITALIC: Style = Style(2);
    pub const UNDERLINE: Style = Style(4);
    pub const STRIKE: Style = Style(8);

    pub fn contains(self, flag: Style) -> bool {
        self.0 & flag.0 == flag.0
    }

    pub fn toggled(self, flag: Style) -> Style {
        Style(self.0 ^ flag.0)
    }

    /// This style with `flag` switched on or off.
    pub fn with(self, flag: Style, on: bool) -> Style {
        if on { Style(self.0 | flag.0) } else { Style(self.0 & !flag.0) }
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn from_bits(bits: u8) -> Style {
        Style(bits & 15)
    }
}

/// Where a picture narrower than the text area sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// How wide a picture is shown, in percent of the text area (the height follows the aspect ratio).
pub const IMAGE_PCT_MIN: u8 = 10;
pub const IMAGE_PCT_MAX: u8 = 100;
/// The ink and marker color of a character: indices into [`PALETTE`] (0 = default ink / no marker).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Paint {
    pub ink: u8,
    pub marker: u8,
}

/// One palette entry: the color's name (stored lowercase in `<c=name>` / `<m=name>`), its ink as
/// `0xRRGGBB`, and its marker (highlight) as `0xRRGGBB`, none for the default entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swatch {
    pub name: &'static str,
    pub ink: u32,
    pub marker: Option<u32>,
}

/// The ten colors, in order; a [`Paint`] indexes this table. The renderer and the popover read it.
pub const PALETTE: [Swatch; 10] = [
    Swatch { name: "Default", ink: 0x2B2F36, marker: None },
    Swatch { name: "Gray", ink: 0x6B6F76, marker: Some(0xE9E9E7) },
    Swatch { name: "Brown", ink: 0x8A5A44, marker: Some(0xEEE0DA) },
    Swatch { name: "Orange", ink: 0xB8581A, marker: Some(0xFADEC9) },
    Swatch { name: "Yellow", ink: 0x8F6A00, marker: Some(0xFDECC8) },
    Swatch { name: "Green", ink: 0x3E7353, marker: Some(0xDBEDDB) },
    Swatch { name: "Blue", ink: 0x3A6D96, marker: Some(0xD3E5EF) },
    Swatch { name: "Purple", ink: 0x7A56A0, marker: Some(0xE8DEEE) },
    Swatch { name: "Pink", ink: 0xA8447C, marker: Some(0xF5E0E9) },
    Swatch { name: "Red", ink: 0xB8453D, marker: Some(0xFFE2DD) },
];

/// The last palette index: the one bound every picked or stored index is held to.
pub const PALETTE_MAX: u8 = PALETTE.len() as u8 - 1;

/// `i` held to the palette (an index past the table becomes the last entry).
pub fn clamp_palette(i: u8) -> u8 {
    i.min(PALETTE_MAX)
}

/// The palette index of a stored color name (case-insensitive); `None` for an unknown name and
/// for the default entry, which is never written.
fn palette_index(name: &str) -> Option<u8> {
    (1..PALETTE.len()).find(|&i| PALETTE[i].name.eq_ignore_ascii_case(name)).map(|i| i as u8)
}

fn palette_name(i: u8) -> String {
    PALETTE[i as usize].name.to_ascii_lowercase()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: Kind,
    /// One line; empty for an image.
    pub text: String,
    /// The style of every byte of `text` (all bytes of a character have the character's style).
    pub marks: Vec<Style>,
    /// The ink and marker of every byte of `text`, kept the same length as `marks`.
    pub paints: Vec<Paint>,
    /// The stored image of an `Image` block.
    pub image_id: Option<i64>,
    /// An image's shown width, percent of the text area (`IMAGE_PCT_MIN..=IMAGE_PCT_MAX`).
    pub image_pct: u8,
    /// An image's place in the text area.
    pub align: Align,
}

impl Block {
    pub fn new(kind: Kind, text: &str) -> Block {
        let text = one_line(text);
        Block { kind, marks: vec![Style::NONE; text.len()], paints: vec![Paint::default(); text.len()], text, image_id: None, image_pct: IMAGE_PCT_MAX, align: Align::Left }
    }

    pub fn image(id: i64) -> Block {
        Block { kind: Kind::Image, text: String::new(), marks: vec![], paints: vec![], image_id: Some(id), image_pct: IMAGE_PCT_MAX, align: Align::Left }
    }

    /// The box of an image `avail` wide whose pixels are `px`: its x offset from the text area's
    /// left edge, its width and its height.
    pub fn image_box(&self, avail: f64, px: (u32, u32)) -> (f64, f64, f64) {
        let width = avail * self.image_pct as f64 / 100.0;
        let height = if px.0 == 0 { 0.0 } else { width * px.1 as f64 / px.0 as f64 };
        let x = match self.align {
            Align::Left => 0.0,
            Align::Center => (avail - width) / 2.0,
            Align::Right => avail - width,
        };
        (x, width, height)
    }

    /// The whole text in one style.
    pub fn with_style(mut self, style: Style) -> Block {
        self.marks = vec![style; self.text.len()];
        self
    }

    /// The style the character at byte `at` is drawn in: its own, and bold in a heading.
    pub fn shown_at(&self, at: usize) -> Style {
        let s = self.marks.get(at).copied().unwrap_or_default();
        if self.kind == Kind::Heading { s.with(Style::BOLD, true) } else { s }
    }

    /// The style of text typed at byte `at`: that of the character before it, or at the start of
    /// the block that of the character after it.
    pub fn style_for_insert(&self, at: usize) -> Style {
        let at = floor_boundary(&self.text, at);
        if at > 0 { self.marks[at - 1] } else { self.marks.first().copied().unwrap_or_default() }
    }

    /// The paint of text typed at byte `at`: that of the character before it, or at the start of
    /// the block that of the character after it.
    pub fn paint_for_insert(&self, at: usize) -> Paint {
        let at = floor_boundary(&self.text, at);
        if at > 0 { self.paints[at - 1] } else { self.paints.first().copied().unwrap_or_default() }
    }

    /// The ink every character of `start..end` has; `None` when they differ or the range is empty.
    pub fn common_ink(&self, start: usize, end: usize) -> Option<u8> {
        self.common_of(start, end, |p| p.ink)
    }

    /// The marker every character of `start..end` has; `None` when they differ or the range is empty.
    pub fn common_marker(&self, start: usize, end: usize) -> Option<u8> {
        self.common_of(start, end, |p| p.marker)
    }

    fn common_of(&self, start: usize, end: usize, f: impl Fn(&Paint) -> u8) -> Option<u8> {
        let (s, e) = (floor_boundary(&self.text, start.min(end)), floor_boundary(&self.text, start.max(end)));
        if s == e {
            return None;
        }
        let first = f(&self.paints[s]);
        self.paints[s..e].iter().all(|p| f(p) == first).then_some(first)
    }

    /// The flags every character of `start..end` has (none for an empty range).
    pub fn common_style(&self, start: usize, end: usize) -> Style {
        let (s, e) = (floor_boundary(&self.text, start.min(end)), floor_boundary(&self.text, start.max(end)));
        if s == e {
            return Style::NONE;
        }
        Style(self.marks[s..e].iter().fold(15, |acc, m| acc & m.0))
    }

    /// The stretches of one style and paint: `(start, end, style, paint)`, never empty.
    fn runs(&self) -> Vec<(usize, usize, Style, Paint)> {
        let mut out: Vec<(usize, usize, Style, Paint)> = vec![];
        for (i, (&m, &p)) in self.marks.iter().zip(&self.paints).enumerate() {
            match out.last_mut() {
                Some(last) if last.2 == m && last.3 == p => last.1 = i + 1,
                _ => out.push((i, i + 1, m, p)),
            }
        }
        out
    }

    /// The stretches of one style, longest first-to-last: `(start, end, style)`, never empty.
    pub fn spans(&self) -> Vec<(usize, usize, Style)> {
        let mut out: Vec<(usize, usize, Style)> = vec![];
        for (i, &m) in self.marks.iter().enumerate() {
            match out.last_mut() {
                Some(last) if last.2 == m => last.1 = i + 1,
                _ => out.push((i, i + 1, m)),
            }
        }
        out
    }

    /// Whether the block has a text field (everything but an image).
    pub fn is_text(&self) -> bool {
        self.kind != Kind::Image
    }

    /// Replace `start..end` (char boundaries) by `insert`, in `style` and `paint`.
    fn splice(&mut self, start: usize, end: usize, insert: &str, style: Style, paint: Paint) {
        self.text.replace_range(start..end, insert);
        self.marks.splice(start..end, std::iter::repeat_n(style, insert.len()));
        self.paints.splice(start..end, std::iter::repeat_n(paint, insert.len()));
    }

    /// One mark per byte, the same for every byte of a character.
    fn normalize_marks(&mut self) {
        self.marks.resize(self.text.len(), Style::NONE);
        self.paints.resize(self.text.len(), Paint::default());
        let known = |i: u8| if i <= PALETTE_MAX { i } else { 0 };
        for (i, c) in self.text.char_indices() {
            let (m, p) = (self.marks[i], self.paints[i]);
            let p = Paint { ink: known(p.ink), marker: known(p.marker) };
            self.marks[i..i + c.len_utf8()].fill(m);
            self.paints[i..i + c.len_utf8()].fill(p);
        }
    }
}

/// A caret position: block index and byte offset into its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Caret {
    pub block: usize,
    pub offset: usize,
}

/// A selection across the note: where it started (`anchor`) and where it ends now (`focus`).
/// `focus` may come before `anchor` (a drag upwards). An image endpoint has offset 0 and the
/// image is part of the range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub anchor: Caret,
    pub focus: Caret,
}

impl Range {
    /// The range `start..end` of one block's text.
    pub fn within(block: usize, start: usize, end: usize) -> Range {
        Range { anchor: Caret { block, offset: start }, focus: Caret { block, offset: end } }
    }

    /// `(start, end)`: the earlier caret first.
    pub fn ordered(self) -> (Caret, Caret) {
        if self.anchor <= self.focus { (self.anchor, self.focus) } else { (self.focus, self.anchor) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    blocks: Vec<Block>,
}

impl Default for Document {
    fn default() -> Self {
        Document::new()
    }
}

fn one_line(s: &str) -> String {
    if s.contains(['\n', '\r']) {
        s.replace("\r\n", " ").replace(['\n', '\r'], " ")
    } else {
        s.to_string()
    }
}

/// Characters that start a marker somewhere in the old format; a text starting with one was escaped.
const SPECIAL: [char; 7] = ['\\', '*', '~', '<', '#', '-', '!'];
/// Characters that start an inline marker (or an escape): escaped wherever they are in the text.
const INLINE: [char; 4] = ['\\', '*', '~', '<'];
/// Characters that start a block marker: escaped as the first character of a line.
const LINE_START: [char; 3] = ['#', '-', '!'];

// ---- the format of older versions: markers around the whole text, only its first character escaped ----

fn unescape(text: &str) -> String {
    let mut chars = text.chars();
    if chars.next() == Some('\\') {
        if let Some(c) = chars.next() {
            if SPECIAL.contains(&c) {
                return text[1..].to_string();
            }
        }
    }
    text.to_string()
}

fn unwrap_inline(mut s: &str) -> (Style, String) {
    let mut style = Style::NONE;
    for (open, close, flag) in [("<u>", "</u>", Style::UNDERLINE), ("~~", "~~", Style::STRIKE), ("**", "**", Style::BOLD), ("*", "*", Style::ITALIC)] {
        if s.len() >= open.len() + close.len() && s.starts_with(open) && s.ends_with(close) {
            s = &s[open.len()..s.len() - close.len()];
            style = style.with(flag, true);
        }
    }
    (style, unescape(s))
}

// ---- the format: a marker wherever a style changes ----

/// The markers that change the style from `from` to `to`: those that end first (innermost
/// first), then those that begin (outermost first). Each flag at most once, so a run of `*` is
/// never ambiguous: `***` is bold and italic.
fn markers(out: &mut String, from: Style, to: Style) {
    for (flag, close) in [(Style::ITALIC, "*"), (Style::BOLD, "**"), (Style::STRIKE, "~~"), (Style::UNDERLINE, "</u>")] {
        if from.contains(flag) && !to.contains(flag) {
            out.push_str(close);
        }
    }
    for (flag, open) in [(Style::UNDERLINE, "<u>"), (Style::STRIKE, "~~"), (Style::BOLD, "**"), (Style::ITALIC, "*")] {
        if to.contains(flag) && !from.contains(flag) {
            out.push_str(open);
        }
    }
}

/// The color markers that change the paint from `from` to `to`: an ink or marker that changes
/// ends (`</c>`, `</m>`) and then begins (`<c=name>`, `<m=name>`).
fn paint_markers(out: &mut String, from: Paint, to: Paint) {
    for (a, b, tag) in [(from.ink, to.ink, 'c'), (from.marker, to.marker, 'm')] {
        if a != b {
            if a != 0 {
                out.push_str(&format!("</{tag}>"));
            }
            if b != 0 {
                out.push_str(&format!("<{tag}={}>", palette_name(b)));
            }
        }
    }
}

/// A block's text with its styles, as stored.
fn write_inline(b: &Block) -> String {
    let mut out = String::new();
    let (mut cur, mut cur_paint) = (Style::NONE, Paint::default());
    for (start, end, style, paint) in b.runs() {
        markers(&mut out, cur, style);
        paint_markers(&mut out, cur_paint, paint);
        for c in b.text[start..end].chars() {
            if INLINE.contains(&c) || (out.is_empty() && LINE_START.contains(&c)) {
                out.push('\\');
            }
            out.push(c);
        }
        cur = style;
        cur_paint = paint;
    }
    markers(&mut out, cur, Style::NONE);
    paint_markers(&mut out, cur_paint, Paint::default());
    out
}

/// Read what [`write_inline`] writes. `None` for anything it would not have written exactly (text
/// from an older version), which is then read the old way.
fn read_inline(s: &str) -> Option<(String, Vec<Style>, Vec<Paint>)> {
    let (mut text, mut marks, mut cur) = (String::new(), vec![], Style::NONE);
    let (mut paints, mut paint) = (vec![], Paint::default());
    // the stored line without the color tags of unknown names, which are dropped
    let mut clean = String::new();
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        let (flag, len, on) = if c == '\\' {
            let n = rest[1..].chars().next().filter(|n| INLINE.contains(n) || LINE_START.contains(n))?;
            text.push(n);
            marks.extend(std::iter::repeat_n(cur, n.len_utf8()));
            paints.extend(std::iter::repeat_n(paint, n.len_utf8()));
            clean.push_str(&rest[..1 + n.len_utf8()]);
            rest = &rest[1 + n.len_utf8()..];
            continue;
        } else if let Some((tag, len, to)) = color_tag(rest) {
            let field = if tag == 'c' { &mut paint.ink } else { &mut paint.marker };
            // a closing tag with nothing open and a tag of an unknown name are dropped
            if to.is_some() || (rest.starts_with("</") && *field != 0) {
                clean.push_str(&rest[..len]);
            } else if *field != 0 {
                // an unknown name switches the color off, as the writer would
                clean.push_str(&format!("</{tag}>"));
            }
            *field = to.unwrap_or(0);
            rest = &rest[len..];
            continue;
        } else if rest.starts_with("<u>") {
            (Style::UNDERLINE, 3, true)
        } else if rest.starts_with("</u>") {
            (Style::UNDERLINE, 4, false)
        } else if rest.starts_with("~~") {
            (Style::STRIKE, 2, !cur.contains(Style::STRIKE))
        } else if rest.starts_with("**") {
            (Style::BOLD, 2, !cur.contains(Style::BOLD))
        } else if c == '*' {
            (Style::ITALIC, 1, !cur.contains(Style::ITALIC))
        } else {
            text.push(c);
            marks.extend(std::iter::repeat_n(cur, c.len_utf8()));
            paints.extend(std::iter::repeat_n(paint, c.len_utf8()));
            clean.push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        };
        cur = cur.with(flag, on);
        clean.push_str(&rest[..len]);
        rest = &rest[len..];
    }
    let b = Block { kind: Kind::Body, text, marks, paints, image_id: None, image_pct: IMAGE_PCT_MAX, align: Align::Left };
    (write_inline(&b) == clean).then_some((b.text, b.marks, b.paints))
}

/// A color tag at the start of `s`: which (`'c'` ink, `'m'` marker), its length, and the palette
/// index it switches to (`None` for a closing tag or an unknown name, both meaning default).
fn color_tag(s: &str) -> Option<(char, usize, Option<u8>)> {
    for tag in ['c', 'm'] {
        if s.starts_with(&format!("</{tag}>")) {
            return Some((tag, 4, None));
        }
        if let Some(r) = s.strip_prefix(&format!("<{tag}=")) {
            let end = r.find('>')?;
            let name = &r[..end];
            if name.contains('<') {
                return None;
            }
            return Some((tag, 4 + end, palette_index(name)));
        }
    }
    None
}

/// A block of `kind` from the stored rest of its line (after the kind's marker).
fn read_block(kind: Kind, rest: &str) -> Block {
    match read_inline(rest) {
        Some((text, marks, paints)) => Block { kind, text, marks, paints, image_id: None, image_pct: IMAGE_PCT_MAX, align: Align::Left },
        None => {
            let (style, text) = unwrap_inline(rest);
            Block::new(kind, &text).with_style(style)
        }
    }
}

/// `![](img:ID)`, or `![](img:ID,PCT,l|c|r)` for a picture that is resized or aligned.
fn image_line(line: &str) -> Option<Block> {
    let inner = line.strip_prefix("![](img:")?.strip_suffix(')')?;
    let mut parts = inner.split(',');
    let mut b = Block::image(parts.next()?.parse().ok()?);
    if let Some(pct) = parts.next().and_then(|p| p.parse::<u8>().ok()) {
        b.image_pct = pct.clamp(IMAGE_PCT_MIN, IMAGE_PCT_MAX);
    }
    b.align = match parts.next() {
        Some("c") => Align::Center,
        Some("r") => Align::Right,
        _ => Align::Left,
    };
    Some(b)
}

/// Where the text of `old` was replaced to make `new`, the caret now at `cursor` in `new` (if
/// known): the replaced range `start..end` of `old` and the end of the inserted text in `new` (it
/// starts at `start`). Typing puts the caret right after what it inserted, which settles what a
/// bare comparison cannot (typing `a` after `a`).
fn diff(old: &str, new: &str, cursor: Option<usize>) -> (usize, usize, usize) {
    let common_prefix = |limit: usize| {
        let mut p = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count().min(limit);
        while !old.is_char_boundary(p) || !new.is_char_boundary(p) {
            p -= 1;
        }
        p
    };
    if let Some(cursor) = cursor.map(|c| floor_boundary(new, c)) {
        let after = &new[cursor..];
        if old.len() >= after.len() && old.ends_with(after) {
            let end = old.len() - after.len();
            let start = common_prefix(end.min(cursor));
            return (start, end, cursor);
        }
    }
    let start = common_prefix(old.len().min(new.len()));
    let mut suffix = old.bytes().rev().zip(new.bytes().rev()).take_while(|(a, b)| a == b).count().min(old.len().min(new.len()) - start);
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    (start, old.len() - suffix, new.len() - suffix)
}

impl Document {
    /// A new note: one empty title.
    pub fn new() -> Document {
        Document { blocks: vec![Block::new(Kind::Title, "")] }
    }

    /// Build from blocks; the first is made the title, no other block is one, and an empty list is a new note.
    pub fn from_blocks(mut blocks: Vec<Block>) -> Document {
        if blocks.is_empty() {
            return Document::new();
        }
        for (i, b) in blocks.iter_mut().enumerate() {
            // the title is the first block and only the first
            if i == 0 {
                b.kind = Kind::Title;
            } else if b.kind == Kind::Title {
                b.kind = Kind::Body;
            }
            if b.text.contains(['\n', '\r']) {
                // a line break and the one or two bytes of it become one space
                let mut text = String::new();
                let mut marks = vec![];
                let mut paints = vec![];
                let mut chars = b.text.char_indices().peekable();
                while let Some((at, c)) = chars.next() {
                    let m = b.marks.get(at).copied().unwrap_or_default();
                    let p = b.paints.get(at).copied().unwrap_or_default();
                    let c = if c == '\r' || c == '\n' {
                        if c == '\r' && chars.peek().is_some_and(|&(_, n)| n == '\n') {
                            chars.next();
                        }
                        ' '
                    } else {
                        c
                    };
                    text.push(c);
                    marks.extend(std::iter::repeat_n(m, c.len_utf8()));
                    paints.extend(std::iter::repeat_n(p, c.len_utf8()));
                }
                b.text = text;
                b.marks = marks;
                b.paints = paints;
            }
            if b.kind == Kind::Image {
                b.text.clear();
                b.marks.clear();
                b.paints.clear();
            } else {
                b.image_id = None;
                b.image_pct = IMAGE_PCT_MAX;
                b.align = Align::Left;
                b.normalize_marks();
            }
        }
        Document { blocks }
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Nothing written yet: a lone empty title (the note shows its hint).
    pub fn is_empty(&self) -> bool {
        self.blocks.len() == 1 && self.blocks[0].text.is_empty()
    }

    /// Parse the stored text. Never fails: whatever the text is, there is a document.
    pub fn parse(content: &str) -> Document {
        let blocks = content
            .split('\n')
            .enumerate()
            .map(|(i, line)| {
                let line = line.strip_suffix('\r').unwrap_or(line);
                parse_line(i, line)
            })
            .collect();
        Document { blocks }
    }

    /// The stored text (inverse of [`Document::parse`]).
    pub fn to_text(&self) -> String {
        // a note nobody wrote in is stored as before: no content at all
        if self.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        for (i, b) in self.blocks.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            match b.kind {
                Kind::Title => out.push_str("# "),
                Kind::Heading => out.push_str("## "),
                Kind::ListItem => out.push_str("- "),
                Kind::Body => {}
                Kind::Image => {
                    out.push_str(&format!("![](img:{}", b.image_id.unwrap_or(0)));
                    if b.image_pct != IMAGE_PCT_MAX || b.align != Align::Left {
                        let a = match b.align {
                            Align::Left => 'l',
                            Align::Center => 'c',
                            Align::Right => 'r',
                        };
                        out.push_str(&format!(",{},{a}", b.image_pct.clamp(IMAGE_PCT_MIN, IMAGE_PCT_MAX)));
                    }
                    out.push(')');
                    continue;
                }
            }
            out.push_str(&write_inline(b));
        }
        out
    }

    // ---- editing ----

    /// The user edited a block's text (no line breaks; see [`Document::insert_text`] for those).
    /// What was typed takes the style of the text before it.
    pub fn set_text(&mut self, block: usize, text: &str) {
        if let Some(b) = self.blocks.get_mut(block).filter(|b| b.is_text()) {
            let text = one_line(text);
            let (start, end, inserted) = diff(&b.text, &text, None);
            let (style, paint) = (b.style_for_insert(start), b.paint_for_insert(start));
            b.splice(start, end, &text[start..inserted], style, paint);
        }
    }

    /// The user edited a block's text in its field to `text`, the caret now at byte `cursor`.
    /// What was typed takes `typing` if given, else the style of the text it was typed after.
    pub fn edit_text(&mut self, block: usize, text: &str, cursor: usize, typing: Option<Style>, typing_paint: Option<Paint>) {
        let Some(b) = self.blocks.get_mut(block).filter(|b| b.is_text()) else { return };
        let text = one_line(text);
        let (start, end, inserted) = diff(&b.text, &text, Some(cursor));
        let style = typing.unwrap_or_else(|| b.style_for_insert(start));
        let paint = typing_paint.unwrap_or_else(|| b.paint_for_insert(start));
        b.splice(start, end, &text[start..inserted], style, paint);
        debug_assert_eq!(b.text, text);
    }

    /// Enter at `offset`: split the block. After a title or heading the new block is body; a list
    /// item continues the list, and Enter on an empty list item turns it into body. The text after
    /// the caret keeps its styles.
    pub fn split(&mut self, block: usize, offset: usize) -> Caret {
        let Some(b) = self.blocks.get(block) else { return Caret { block: self.blocks.len() - 1, offset: 0 } };
        if b.kind == Kind::Image {
            self.blocks.insert(block + 1, Block::new(Kind::Body, ""));
            return Caret { block: block + 1, offset: 0 };
        }
        if b.kind == Kind::ListItem && b.text.is_empty() {
            self.blocks[block].kind = Kind::Body;
            return Caret { block, offset: 0 };
        }
        let at = floor_boundary(&b.text, offset);
        let b = &mut self.blocks[block];
        let tail = b.text.split_off(at);
        let marks = b.marks.split_off(at);
        let paints = b.paints.split_off(at);
        let new = following(&self.blocks[block], tail, marks, paints);
        self.blocks.insert(block + 1, new);
        Caret { block: block + 1, offset: 0 }
    }

    /// Backspace at the start of a block: merge it into the previous one (or delete a previous
    /// image). `None` when nothing happens: not at the start, or the title.
    pub fn backspace(&mut self, block: usize, offset: usize) -> Option<Caret> {
        if offset != 0 || block == 0 || !self.blocks.get(block)?.is_text() {
            return None;
        }
        if self.blocks[block - 1].kind == Kind::Image {
            self.blocks.remove(block - 1);
            return Some(Caret { block: block - 1, offset: 0 });
        }
        let cur = self.blocks.remove(block);
        let prev = &mut self.blocks[block - 1];
        let at = prev.text.len();
        prev.text.push_str(&cur.text);
        prev.marks.extend(cur.marks);
        prev.paints.extend(cur.paints);
        Some(Caret { block: block - 1, offset: at })
    }

    /// Delete at the end of a block: merge the next block in (or delete a next image).
    pub fn delete(&mut self, block: usize, offset: usize) -> Option<Caret> {
        let b = self.blocks.get(block)?;
        if !b.is_text() || offset != b.text.len() || block + 1 >= self.blocks.len() {
            return None;
        }
        let next = self.blocks.remove(block + 1);
        if next.kind != Kind::Image {
            self.blocks[block].text.push_str(&next.text);
            self.blocks[block].marks.extend(next.marks);
            self.blocks[block].paints.extend(next.paints);
        }
        Some(Caret { block, offset })
    }

    /// Typing a space after `#` or `-` at the start of a body block (`old` was the block's text
    /// before the edit, the caret is now at `cursor`) turns it into a heading or list item and
    /// drops the marker. Returns the caret (the start of the block) when it did.
    pub fn auto_format(&mut self, block: usize, old: &str, cursor: usize) -> Option<Caret> {
        let b = self.blocks.get_mut(block)?;
        if b.kind != Kind::Body || cursor != 2 {
            return None;
        }
        let kind = match b.text.get(..2)? {
            "# " => Kind::Heading,
            "- " | "* " => Kind::ListItem,
            _ => return None,
        };
        if old.starts_with(&b.text[..2]) {
            return None;
        }
        b.kind = kind;
        b.text.drain(..2);
        b.marks.drain(..2);
        b.paints.drain(..2);
        Some(Caret { block, offset: 0 })
    }

    /// Toggle one style flag over `start..end` of a block: off if every character there has it,
    /// else on for all of them. False when nothing changes (an empty range, an image).
    pub fn toggle_style(&mut self, block: usize, start: usize, end: usize, flag: Style) -> bool {
        let Some(b) = self.blocks.get_mut(block).filter(|b| b.is_text()) else { return false };
        let (s, e) = (floor_boundary(&b.text, start.min(end)), floor_boundary(&b.text, start.max(end)));
        if s == e {
            return false;
        }
        let on = !b.common_style(s, e).contains(flag);
        for m in &mut b.marks[s..e] {
            *m = m.with(flag, on);
        }
        true
    }

    /// Toggle one style flag over the whole note (a select-all): off if every character has it,
    /// else on everywhere. False when the note has no text.
    pub fn toggle_style_all(&mut self, flag: Style) -> bool {
        self.toggle_style_range(self.all(), flag)
    }

    /// The flags every character of the note has (none for a note without text).
    pub fn common_style_all(&self) -> Style {
        self.common_style_range(self.all())
    }

    /// The range's carets, earlier first, with their blocks kept inside the document.
    fn ordered_in(&self, range: Range) -> (Caret, Caret) {
        let (s, e) = range.ordered();
        let last = self.blocks.len() - 1;
        (Caret { block: s.block.min(last), ..s }, Caret { block: e.block.min(last), ..e })
    }

    /// The range from the start of the note to its end (a select-all).
    pub fn all(&self) -> Range {
        let last = self.blocks.len() - 1;
        Range { anchor: Caret { block: 0, offset: 0 }, focus: Caret { block: last, offset: self.blocks[last].text.len() } }
    }

    /// The non-empty text stretches `(block, start, end)` of a range, one per text block in it.
    fn range_spans(&self, range: Range) -> Vec<(usize, usize, usize)> {
        let (s, e) = self.ordered_in(range);
        (s.block..=e.block)
            .filter_map(|i| {
                let b = &self.blocks[i];
                if !b.is_text() {
                    return None;
                }
                let from = if i == s.block { floor_boundary(&b.text, s.offset) } else { 0 };
                let to = if i == e.block { floor_boundary(&b.text, e.offset) } else { b.text.len() };
                (from < to).then_some((i, from, to))
            })
            .collect()
    }

    /// Toggle one style flag over a range: off if every character in it has the flag, else on for
    /// all of them. False when the range holds no text.
    pub fn toggle_style_range(&mut self, range: Range, flag: Style) -> bool {
        let spans = self.range_spans(range);
        if spans.is_empty() {
            return false;
        }
        let on = !spans.iter().all(|&(i, s, e)| self.blocks[i].common_style(s, e).contains(flag));
        for (i, s, e) in spans {
            for m in &mut self.blocks[i].marks[s..e] {
                *m = m.with(flag, on);
            }
        }
        true
    }

    /// The flags every character of a range has (none for a range without text).
    pub fn common_style_range(&self, range: Range) -> Style {
        let spans = self.range_spans(range);
        if spans.is_empty() {
            return Style::NONE;
        }
        Style(spans.iter().fold(15, |acc, &(i, s, e)| acc & self.blocks[i].common_style(s, e).0))
    }

    /// Set the ink of every character in a range to palette index `ink` (clamped to the palette).
    /// Setting, not toggling: a range that already has it is unchanged. True when something changed.
    pub fn set_ink_range(&mut self, range: Range, ink: u8) -> bool {
        self.set_paint_range(range, |p| &mut p.ink, ink)
    }

    /// Set the marker of every character in a range (see [`Document::set_ink_range`]).
    pub fn set_marker_range(&mut self, range: Range, marker: u8) -> bool {
        self.set_paint_range(range, |p| &mut p.marker, marker)
    }

    fn set_paint_range(&mut self, range: Range, field: fn(&mut Paint) -> &mut u8, to: u8) -> bool {
        let to = clamp_palette(to);
        let mut changed = false;
        for (i, s, e) in self.range_spans(range) {
            for p in &mut self.blocks[i].paints[s..e] {
                let f = field(p);
                changed |= *f != to;
                *f = to;
            }
        }
        changed
    }

    /// Set the ink of `start..end` of one block (see [`Document::set_ink_range`]).
    pub fn set_ink(&mut self, block: usize, start: usize, end: usize, ink: u8) -> bool {
        self.set_ink_range(Range::within(block, start, end), ink)
    }

    /// Set the marker of `start..end` of one block.
    pub fn set_marker(&mut self, block: usize, start: usize, end: usize, marker: u8) -> bool {
        self.set_marker_range(Range::within(block, start, end), marker)
    }

    /// The ink every character of a range has; `None` when they differ or the range has no text.
    pub fn common_ink_range(&self, range: Range) -> Option<u8> {
        self.common_paint_range(range, Block::common_ink)
    }

    /// The marker every character of a range has; `None` when they differ or the range has no text.
    pub fn common_marker_range(&self, range: Range) -> Option<u8> {
        self.common_paint_range(range, Block::common_marker)
    }

    fn common_paint_range(&self, range: Range, f: fn(&Block, usize, usize) -> Option<u8>) -> Option<u8> {
        let mut found = None;
        for (i, s, e) in self.range_spans(range) {
            let c = f(&self.blocks[i], s, e)?;
            if found.is_some_and(|x| x != c) {
                return None;
            }
            found = Some(c);
        }
        found
    }

    /// The word around byte `offset` of a block, when the offset is inside one (word characters
    /// on both sides): its byte range.
    pub fn word_at(&self, block: usize, offset: usize) -> Option<(usize, usize)> {
        let t = &self.blocks.get(block)?.text;
        let at = floor_boundary(t, offset);
        let word = |c: char| c.is_alphanumeric() || c == '_' || c == '\'';
        if !t[..at].chars().next_back().is_some_and(word) || !t[at..].chars().next().is_some_and(word) {
            return None;
        }
        let start = t[..at].char_indices().rev().take_while(|&(_, c)| word(c)).last().map_or(at, |(i, _)| i);
        let end = t[at..].char_indices().find(|&(_, c)| !word(c)).map_or(t.len(), |(i, _)| at + i);
        Some((start, end))
    }

    /// Toggle a body block into a list item and back. False for titles, headings and images.
    pub fn toggle_list(&mut self, block: usize) -> bool {
        match self.blocks.get_mut(block) {
            Some(b) if b.kind == Kind::Body => {
                b.kind = Kind::ListItem;
                true
            }
            Some(b) if b.kind == Kind::ListItem => {
                b.kind = Kind::Body;
                true
            }
            _ => false,
        }
    }

    /// The list button over a range: if every body or list paragraph in it is a list item they
    /// all become body, else they all become list items. False when it holds neither.
    pub fn toggle_list_range(&mut self, range: Range) -> bool {
        let (s, e) = self.ordered_in(range);
        let eligible: Vec<usize> =
            (s.block..=e.block).filter(|&i| matches!(self.blocks[i].kind, Kind::Body | Kind::ListItem)).collect();
        if eligible.is_empty() {
            return false;
        }
        let to = if eligible.iter().all(|&i| self.blocks[i].kind == Kind::ListItem) { Kind::Body } else { Kind::ListItem };
        for i in eligible {
            self.blocks[i].kind = to;
        }
        true
    }

    /// Show the picture at `block` `pct` percent wide (clamped). False when it is no picture or nothing changes.
    pub fn set_image_pct(&mut self, block: usize, pct: u8) -> bool {
        let pct = pct.clamp(IMAGE_PCT_MIN, IMAGE_PCT_MAX);
        match self.blocks.get_mut(block) {
            Some(b) if b.kind == Kind::Image && b.image_pct != pct => {
                b.image_pct = pct;
                true
            }
            _ => false,
        }
    }

    /// Place the picture at `block`. False when it is no picture or nothing changes.
    pub fn set_image_align(&mut self, block: usize, align: Align) -> bool {
        match self.blocks.get_mut(block) {
            Some(b) if b.kind == Kind::Image && b.align != align => {
                b.align = align;
                true
            }
            _ => false,
        }
    }

    /// Remove the picture at `block`; the caret goes to the block that takes its place (always a
    /// text block: an image never ends the note). `None` when `block` is no picture.
    pub fn delete_image(&mut self, block: usize) -> Option<Caret> {
        if self.blocks.get(block)?.kind != Kind::Image {
            return None;
        }
        self.blocks.remove(block);
        if block >= self.blocks.len() {
            self.blocks.push(Block::new(Kind::Body, ""));
        }
        let at = (block..self.blocks.len()).find(|&i| self.blocks[i].is_text()).unwrap_or_else(|| {
            self.blocks.push(Block::new(Kind::Body, ""));
            self.blocks.len() - 1
        });
        Some(Caret { block: at, offset: 0 })
    }

    /// Insert an image block after `block`; an image never ends the note, a body line follows.
    /// Returns the image's index.
    pub fn insert_image(&mut self, block: usize, id: i64) -> usize {
        let at = block.min(self.blocks.len() - 1) + 1;
        self.blocks.insert(at, Block::image(id));
        if at + 1 == self.blocks.len() {
            self.blocks.push(Block::new(Kind::Body, ""));
        }
        at
    }

    /// Replace `start..end` of a block's text with `text`, which may span lines (a paste): every
    /// line after the first becomes a new block, as if Enter had been pressed there. The inserted
    /// text takes the style of the text before it. Returns the caret after the inserted text.
    pub fn insert_text(&mut self, block: usize, start: usize, end: usize, text: &str) -> Caret {
        let Some(b) = self.blocks.get(block).filter(|b| b.is_text()) else {
            return Caret { block: block.min(self.blocks.len() - 1), offset: 0 };
        };
        let start = floor_boundary(&b.text, start);
        let end = floor_boundary(&b.text, end).max(start);
        let (style, paint) = (b.style_for_insert(start), b.paint_for_insert(start));
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut lines = normalized.split('\n');
        let first = lines.next().unwrap_or("");
        let b = &mut self.blocks[block];
        let tail = b.text.split_off(end);
        let tail_marks = b.marks.split_off(end);
        let tail_paints = b.paints.split_off(end);
        b.splice(start, end, first, style, paint);
        let mut caret = Caret { block, offset: b.text.len() };
        for line in lines {
            let new = following(&self.blocks[caret.block], line.to_string(), vec![style; line.len()], vec![paint; line.len()]);
            caret = Caret { block: caret.block + 1, offset: new.text.len() };
            self.blocks.insert(caret.block, new);
        }
        let last = &mut self.blocks[caret.block];
        last.text.push_str(&tail);
        last.marks.extend(tail_marks);
        last.paints.extend(tail_paints);
        caret
    }

    /// The note's words as plain lines (what select-all copies); pictures are left out.
    pub fn plain_text(&self) -> String {
        self.plain_range(self.all())
    }

    /// The words of a range as plain lines (what copy takes); pictures are left out.
    pub fn plain_range(&self, range: Range) -> String {
        let (s, e) = self.ordered_in(range);
        (s.block..=e.block)
            .filter(|&i| self.blocks[i].is_text())
            .map(|i| {
                let t = &self.blocks[i].text;
                let from = if i == s.block { floor_boundary(t, s.offset) } else { 0 };
                let to = if i == e.block { floor_boundary(t, e.offset) } else { t.len() };
                &t[from..to.max(from)]
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Select-all then typing, pasting or Delete: the whole note becomes `text` (which may span
    /// lines). Returns the caret after it.
    pub fn replace_all(&mut self, text: &str) -> Caret {
        self.replace_range(self.all(), text)
    }

    /// Delete a range. The blocks it touches merge into one; returns the caret where the range was.
    pub fn delete_range(&mut self, range: Range) -> Caret {
        self.replace_range(range, "")
    }

    /// Replace a range with `text`, which may span lines. The first and last blocks merge and keep
    /// the first block's kind, and the blocks between them go, images included (an image at either
    /// end goes too). Inside one block this is [`Document::insert_text`]. Returns the caret after
    /// the inserted text.
    pub fn replace_range(&mut self, range: Range, text: &str) -> Caret {
        let (s, e) = self.ordered_in(range);
        if s.block == e.block {
            return self.insert_text(s.block, s.offset, e.offset, text);
        }
        let first = &self.blocks[s.block];
        let end = &self.blocks[e.block];
        let mut merged = if first.is_text() {
            let mut b = first.clone();
            let at = floor_boundary(&b.text, s.offset);
            b.text.truncate(at);
            b.marks.truncate(at);
            b.paints.truncate(at);
            b
        } else if end.is_text() {
            Block::new(end.kind, "")
        } else {
            Block::new(Kind::Body, "")
        };
        let from = merged.text.len();
        if end.is_text() {
            let at = floor_boundary(&end.text, e.offset);
            merged.text.push_str(&end.text[at..]);
            merged.marks.extend_from_slice(&end.marks[at..]);
            merged.paints.extend_from_slice(&end.paints[at..]);
        }
        self.blocks.splice(s.block..=e.block, [merged]);
        self.insert_text(s.block, from, from, text)
    }

    /// The nearest block with a text field before `block`.
    pub fn prev_text_block(&self, block: usize) -> Option<usize> {
        (0..block.min(self.blocks.len())).rev().find(|&i| self.blocks[i].is_text())
    }

    /// The nearest block with a text field after `block`.
    pub fn next_text_block(&self, block: usize) -> Option<usize> {
        (block + 1..self.blocks.len()).find(|&i| self.blocks[i].is_text())
    }
}

/// The block that follows `prev` when Enter is pressed in it, holding `text` styled `marks`.
fn following(prev: &Block, text: String, marks: Vec<Style>, paints: Vec<Paint>) -> Block {
    let kind = if prev.kind == Kind::ListItem { Kind::ListItem } else { Kind::Body };
    Block { kind, text, marks, paints, image_id: None, image_pct: IMAGE_PCT_MAX, align: Align::Left }
}

pub(crate) fn floor_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn parse_line(index: usize, line: &str) -> Block {
    if index == 0 {
        // The title: `# ` (or `## `, older notes) and the rest, or the whole line of a plain note.
        let rest = line.strip_prefix("# ").or_else(|| line.strip_prefix("## ")).unwrap_or(line);
        return read_block(Kind::Title, rest);
    }
    if let Some(b) = image_line(line) {
        return b;
    }
    let (kind, rest) = if let Some(r) = line.strip_prefix("## ").or_else(|| line.strip_prefix("# ")) {
        (Kind::Heading, r)
    } else if let Some(r) = line.strip_prefix("- ") {
        (Kind::ListItem, r)
    } else {
        (Kind::Body, line)
    };
    read_block(kind, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOCKUP: &str = "# Endfield Note\n## **Thermite Cutter : POG & Akekuri**\nCurrently POG no Weapon";

    fn texts(d: &Document) -> Vec<&str> {
        d.blocks().iter().map(|b| b.text.as_str()).collect()
    }
    fn kinds(d: &Document) -> Vec<Kind> {
        d.blocks().iter().map(|b| b.kind).collect()
    }
    fn doc(blocks: &[(Kind, &str)]) -> Document {
        Document::from_blocks(blocks.iter().map(|&(k, t)| Block::new(k, t)).collect())
    }

    #[test]
    fn hash_or_dash_then_space_at_the_start_of_a_body_makes_a_heading_or_list_item() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "# ")]);
        assert_eq!(d.auto_format(1, "#", 2), Some(Caret { block: 1, offset: 0 }));
        assert_eq!((d.blocks()[1].kind, d.blocks()[1].text.as_str()), (Kind::Heading, ""));
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "- milk")]);
        assert_eq!(d.auto_format(1, "-milk", 2), Some(Caret { block: 1, offset: 0 }));
        assert_eq!((d.blocks()[1].kind, d.blocks()[1].text.as_str()), (Kind::ListItem, "milk"));
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "* eggs")]);
        assert_eq!(d.auto_format(1, "*eggs", 2), Some(Caret { block: 1, offset: 0 }));
        assert_eq!((d.blocks()[1].kind, d.blocks()[1].text.as_str()), (Kind::ListItem, "eggs"));
    }

    #[test]
    fn the_markers_do_nothing_elsewhere() {
        let mut d = doc(&[(Kind::Title, "# "), (Kind::Body, "a # "), (Kind::Body, "- x")]);
        assert_eq!(d.auto_format(0, "#", 2), None, "the title stays the title");
        assert_eq!(d.auto_format(1, "a #", 4), None);
        assert_eq!(d.auto_format(2, "- x", 2), None, "already there before the edit");
        assert_eq!(d.auto_format(9, "", 2), None);
    }

    // ---- the format ----

    #[test]
    fn the_mockup_fixture_is_title_heading_body() {
        let d = Document::parse(MOCKUP);
        assert_eq!(kinds(&d), [Kind::Title, Kind::Heading, Kind::Body]);
        assert_eq!(texts(&d), ["Endfield Note", "Thermite Cutter : POG & Akekuri", "Currently POG no Weapon"]);
        assert!(d.blocks()[1].marks.iter().all(|m| m.contains(Style::BOLD)));
        assert_eq!(d.to_text(), MOCKUP);
    }

    #[test]
    fn every_kind_has_its_line() {
        let text = "# t\n## **h**\nb\n- item\n![](img:42)\n**bold** *it* ~~s~~";
        let d = Document::parse(text);
        assert_eq!(kinds(&d), [Kind::Title, Kind::Heading, Kind::Body, Kind::ListItem, Kind::Image, Kind::Body]);
        assert_eq!(d.blocks()[3].text, "item");
        assert_eq!(d.blocks()[4].image_id, Some(42));
        // markers inside a line style the words between them
        let last = &d.blocks()[5];
        assert_eq!(last.text, "bold it s");
        assert_eq!(last.spans(), [(0, 4, Style::BOLD), (4, 5, Style::NONE), (5, 7, Style::ITALIC), (7, 8, Style::NONE), (8, 9, Style::STRIKE)]);
        assert_eq!(d.to_text(), text);
        assert_eq!(Document::parse(&d.to_text()), d);
    }

    #[test]
    fn style_flags_wrap_the_whole_block() {
        let all = Style::BOLD.toggled(Style::ITALIC).toggled(Style::UNDERLINE).toggled(Style::STRIKE);
        let d = Document::from_blocks(vec![Block::new(Kind::Title, "t"), Block::new(Kind::Body, "x").with_style(all)]);
        assert_eq!(d.to_text(), "# t\n<u>~~***x***~~</u>");
        assert_eq!(Document::parse(&d.to_text()), d);
    }

    #[test]
    fn an_empty_note_is_stored_as_no_text() {
        assert_eq!(Document::new().to_text(), "");
        assert_eq!(Document::parse(&Document::new().to_text()), Document::new());
    }

    #[test]
    fn plain_text_of_older_notes_is_a_title_and_body_lines() {
        let d = Document::parse("shopping list\nmilk\n\neggs");
        assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Body, Kind::Body]);
        assert_eq!(texts(&d), ["shopping list", "milk", "", "eggs"]);
        assert_eq!(Document::parse("a\r\nb").blocks()[0].text, "a");
    }

    #[test]
    fn the_first_block_is_always_the_title_and_empty_text_is_one_empty_title() {
        for s in ["", "- x", "## y", "![](img:3)", "\n\n"] {
            assert_eq!(Document::parse(s).blocks()[0].kind, Kind::Title, "{s:?}");
        }
        assert!(Document::parse("").is_empty());
        assert!(!Document::parse("a").is_empty());
        assert!(!Document::parse("\n").is_empty());
        assert_eq!(Document::from_blocks(vec![Block::image(1)]).blocks()[0].kind, Kind::Title);
    }

    #[test]
    fn text_that_looks_like_markers_survives() {
        for s in ["# x", "## **y**", "- z", "![](img:1)", "**b**", "*i*", "~~s~~", "<u>u</u>", "\\", "\\*", "\\\\n", "-", "#", "", "*", "**", "****", "a*"] {
            for kind in [Kind::Title, Kind::Heading, Kind::Body, Kind::ListItem] {
                for style in 0..16 {
                    let d = Document::from_blocks(vec![Block::new(Kind::Title, ""), Block::new(kind, s).with_style(Style::from_bits(style))]);
                    assert_eq!(Document::parse(&d.to_text()), d, "{kind:?} {s:?} {style}: {:?}", d.to_text());
                    let t = Document::from_blocks(vec![Block::new(kind, s).with_style(Style::from_bits(style))]);
                    assert_eq!(Document::parse(&t.to_text()), t, "title {s:?} {style}");
                }
            }
        }
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self, n: usize) -> usize {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as usize) % n
        }
    }

    fn random_text(r: &mut Rng) -> String {
        const PIECES: [&str; 22] = ["a", "b", " ", "*", "**", "~~", "~", "<u>", "</u>", "<", "\\", "#", "## ", "- ", "!", "![](img:", ")", "7", "é", "日本", "\u{1F600}", "-"];
        (0..r.next(8)).map(|_| PIECES[r.next(PIECES.len())]).collect()
    }

    #[test]
    fn property_every_document_round_trips() {
        let mut r = Rng(1);
        for _ in 0..5000 {
            let blocks = (0..1 + r.next(6))
                .map(|_| {
                    let b = match r.next(6) {
                        0 => Block::new(Kind::Heading, &random_text(&mut r)),
                        1 => Block::new(Kind::ListItem, &random_text(&mut r)),
                        2 => Block::image(r.next(100) as i64 - 50),
                        3 => Block::new(Kind::Title, &random_text(&mut r)),
                        _ => Block::new(Kind::Body, &random_text(&mut r)),
                    };
                    let mut b = b.with_style(Style::from_bits(r.next(16) as u8));
                    if r.next(2) == 0 {
                        // a style per character
                        let starts: Vec<usize> = b.text.char_indices().map(|(i, _)| i).collect();
                        for (k, &i) in starts.iter().enumerate() {
                            let end = starts.get(k + 1).copied().unwrap_or(b.text.len());
                            let s = Style::from_bits(r.next(16) as u8);
                            b.marks[i..end].fill(s);
                        }
                    }
                    b
                })
                .collect();
            let d = Document::from_blocks(blocks);
            assert_eq!(Document::parse(&d.to_text()), d, "{:?}", d.to_text());
        }
    }

    #[test]
    fn property_malformed_text_never_panics_and_settles() {
        let mut r = Rng(7);
        for _ in 0..5000 {
            let s: String = (0..r.next(6)).map(|_| if r.next(4) == 0 { "\n".to_string() } else { random_text(&mut r) }).collect();
            let d = Document::parse(&s);
            assert_eq!(d.blocks()[0].kind, Kind::Title);
            let again = Document::parse(&d.to_text());
            assert_eq!(again, d, "{s:?}");
        }
        for s in ["![](img:", "![](img:999999999999999999999)", "![](img:x)", "<u>", "</u>", "<u></u>", "~~", "* ", "\\", "# ", "## **", "\r", "\u{0}"] {
            let _ = Document::parse(s);
            let _ = Document::parse(&format!("t\n{s}"));
        }
    }

    // ---- Enter ----

    #[test]
    fn enter_splits_the_paragraph_at_the_caret() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "hello world")]);
        assert_eq!(d.split(1, 5), Caret { block: 2, offset: 0 });
        assert_eq!(texts(&d), ["t", "hello", " world"]);
        assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Body]);
    }

    #[test]
    fn enter_at_the_boundaries_makes_an_empty_block() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "ab")]);
        d.split(1, 2);
        assert_eq!(texts(&d), ["t", "ab", ""]);
        d.split(1, 0);
        assert_eq!(texts(&d), ["t", "", "ab", ""]);
        // an offset inside a character or past the end is clamped
        let mut e = doc(&[(Kind::Title, "é")]);
        e.split(0, 1);
        assert_eq!(texts(&e), ["", "é"]);
        e.split(1, 99);
        assert_eq!(texts(&e), ["", "é", ""]);
    }

    #[test]
    fn enter_after_title_or_heading_starts_body() {
        let mut d = doc(&[(Kind::Title, "ti"), (Kind::Heading, "he")]);
        d.toggle_style(0, 0, 2, Style::BOLD);
        d.split(0, 2);
        assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Heading]);
        assert!(d.blocks()[1].marks.is_empty());
        d.split(2, 2);
        assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Heading, Kind::Body]);
        // the middle of a title: the title keeps its head, the rest is body
        let mut e = doc(&[(Kind::Title, "abcd")]);
        e.split(0, 2);
        assert_eq!((kinds(&e), texts(&e)), (vec![Kind::Title, Kind::Body], vec!["ab", "cd"]));
    }

    #[test]
    fn enter_in_a_list_continues_it_and_on_an_empty_item_ends_it() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::ListItem, "milk")]);
        d.toggle_style(1, 2, 4, Style::ITALIC);
        d.split(1, 2);
        assert_eq!(kinds(&d), [Kind::Title, Kind::ListItem, Kind::ListItem]);
        assert_eq!(d.blocks()[2].spans(), [(0, 2, Style::ITALIC)], "the text after the caret keeps its style");
        d.set_text(2, "");
        assert_eq!(d.split(2, 0), Caret { block: 2, offset: 0 });
        assert_eq!(kinds(&d), [Kind::Title, Kind::ListItem, Kind::Body]);
        assert_eq!(d.len(), 3);
    }

    #[test]
    fn enter_on_an_image_adds_body_after_it() {
        let mut d = doc(&[(Kind::Title, "t")]);
        d.insert_image(0, 5);
        assert_eq!(d.split(1, 0), Caret { block: 2, offset: 0 });
        assert_eq!(kinds(&d), [Kind::Title, Kind::Image, Kind::Body, Kind::Body]);
    }

    // ---- Backspace / Delete ----

    #[test]
    fn backspace_at_the_start_merges_into_the_previous_block() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Heading, "head"), (Kind::Body, "body")]);
        assert_eq!(d.backspace(2, 0), Some(Caret { block: 1, offset: 4 }));
        assert_eq!((kinds(&d), texts(&d)), (vec![Kind::Title, Kind::Heading], vec!["t", "headbody"]));
        assert_eq!(d.backspace(1, 0), Some(Caret { block: 0, offset: 1 }));
        assert_eq!((kinds(&d), texts(&d)), (vec![Kind::Title], vec!["theadbody"]));
    }

    #[test]
    fn backspace_does_nothing_inside_a_block_and_the_title_cannot_be_merged_away() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "b")]);
        assert_eq!(d.backspace(1, 1), None);
        assert_eq!(d.backspace(0, 0), None);
        assert_eq!(d.backspace(9, 0), None);
        assert_eq!(d.len(), 2);
        // an empty body right after the title merges into it, never the other way round
        let mut e = doc(&[(Kind::Title, "t"), (Kind::Body, "")]);
        e.backspace(1, 0);
        assert_eq!((kinds(&e), texts(&e)), (vec![Kind::Title], vec!["t"]));
    }

    #[test]
    fn backspace_after_an_image_deletes_the_image() {
        let mut d = doc(&[(Kind::Title, "t")]);
        d.insert_image(0, 9);
        d.set_text(2, "x");
        assert_eq!(d.backspace(2, 0), Some(Caret { block: 1, offset: 0 }));
        assert_eq!((kinds(&d), texts(&d)), (vec![Kind::Title, Kind::Body], vec!["t", "x"]));
    }

    #[test]
    fn delete_at_the_end_merges_the_next_block_in() {
        let mut d = doc(&[(Kind::Title, "ab"), (Kind::Heading, "cd"), (Kind::ListItem, "ef")]);
        assert_eq!(d.delete(0, 2), Some(Caret { block: 0, offset: 2 }));
        assert_eq!((kinds(&d), texts(&d)), (vec![Kind::Title, Kind::ListItem], vec!["abcd", "ef"]));
        assert_eq!(d.delete(0, 1), None);
        assert_eq!(d.delete(1, 2), None, "the last block has nothing after it");
        assert_eq!(d.delete(7, 0), None);
    }

    #[test]
    fn delete_before_an_image_deletes_the_image() {
        let mut d = doc(&[(Kind::Title, "t")]);
        d.insert_image(0, 9);
        assert_eq!(d.delete(0, 1), Some(Caret { block: 0, offset: 1 }));
        assert_eq!(kinds(&d), [Kind::Title, Kind::Body]);
    }

    // ---- styles, lists, images ----

    #[test]
    fn toggling_a_style_flips_one_flag_of_the_selection() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "one two")]);
        assert!(d.toggle_style(1, 4, 7, Style::BOLD));
        assert_eq!(d.blocks()[1].spans(), [(0, 4, Style::NONE), (4, 7, Style::BOLD)]);
        assert!(d.toggle_style(1, 7, 2, Style::UNDERLINE), "the ends can come in either order");
        assert_eq!(d.blocks()[1].spans(), [(0, 2, Style::NONE), (2, 4, Style::UNDERLINE), (4, 7, Style::BOLD.with(Style::UNDERLINE, true))]);
        // part of the selection is bold: the whole of it becomes bold; all of it is: none of it stays
        assert!(d.toggle_style(1, 0, 7, Style::BOLD));
        assert_eq!(d.blocks()[1].common_style(0, 7), Style::BOLD);
        assert!(d.toggle_style(1, 0, 7, Style::BOLD));
        assert_eq!(d.blocks()[1].common_style(0, 7), Style::NONE);
        assert_eq!(d.to_text(), "# t\non<u>e two</u>");
        assert!(!d.toggle_style(1, 3, 3, Style::BOLD), "nothing selected");
        assert!(d.toggle_style(0, 0, 1, Style::ITALIC), "a title can be styled");
        assert!(!d.toggle_style(5, 0, 1, Style::BOLD));
        d.insert_image(1, 1);
        assert!(!d.toggle_style(2, 0, 1, Style::BOLD), "an image has no text to style");
    }

    #[test]
    fn the_list_button_over_a_range_makes_every_paragraph_a_list_item_or_none() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "a"), (Kind::ListItem, "b"), (Kind::Body, "c")]);
        let r = Range { anchor: Caret { block: 3, offset: 0 }, focus: Caret { block: 0, offset: 1 } };
        assert!(d.toggle_list_range(r));
        assert_eq!(d.blocks().iter().map(|b| b.kind).collect::<Vec<_>>(), [Kind::Title, Kind::ListItem, Kind::ListItem, Kind::ListItem]);
        assert!(d.toggle_list_range(r));
        assert_eq!(d.blocks().iter().map(|b| b.kind).collect::<Vec<_>>(), [Kind::Title, Kind::Body, Kind::Body, Kind::Body]);
        let only_title = Range { anchor: Caret { block: 0, offset: 0 }, focus: Caret { block: 0, offset: 1 } };
        assert!(!d.toggle_list_range(only_title));
    }

    #[test]
    fn a_select_all_styles_the_whole_note() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "ab")]);
        d.toggle_style(1, 0, 1, Style::BOLD);
        assert_eq!(d.common_style_all(), Style::NONE);
        assert!(d.toggle_style_all(Style::BOLD));
        assert_eq!(d.common_style_all(), Style::BOLD);
        assert!(d.toggle_style_all(Style::BOLD));
        assert_eq!(d.common_style_all(), Style::NONE);
        assert!(!Document::new().toggle_style_all(Style::BOLD));
    }

    #[test]
    fn the_word_at_the_caret() {
        let d = doc(&[(Kind::Title, "héllo wörld, x")]);
        assert_eq!(d.word_at(0, 2), Some((0, 6)));
        assert_eq!(d.word_at(0, 9), Some((7, 13)));
        assert_eq!(d.word_at(0, 0), None, "at the start of a word");
        assert_eq!(d.word_at(0, 6), None, "at its end");
        assert_eq!(d.word_at(0, 14), None);
    }

    // ---- styles while editing ----

    #[test]
    fn typed_text_takes_the_style_before_it() {
        let mut d = doc(&[(Kind::Title, "ab")]);
        d.toggle_style(0, 0, 1, Style::BOLD);
        // "a|b" -> "ax|b": after the bold a
        d.edit_text(0, "axb", 2, None, None);
        assert_eq!(d.blocks()[0].spans(), [(0, 2, Style::BOLD), (2, 3, Style::NONE)]);
        // at the start: the style of what follows
        d.edit_text(0, "yaxb", 1, None, None);
        assert_eq!(d.blocks()[0].spans(), [(0, 3, Style::BOLD), (3, 4, Style::NONE)]);
        // with a style asked for (a toolbar button with nothing selected)
        d.edit_text(0, "yaxbz", 5, Some(Style::ITALIC), None);
        assert_eq!(d.blocks()[0].spans(), [(0, 3, Style::BOLD), (3, 4, Style::NONE), (4, 5, Style::ITALIC)]);
    }

    #[test]
    fn the_caret_settles_where_repeated_letters_were_typed() {
        let mut d = doc(&[(Kind::Title, "aa")]);
        d.toggle_style(0, 1, 2, Style::BOLD);
        // the plain a was doubled: the caret after the first a
        d.edit_text(0, "aaa", 1, None, None);
        assert_eq!(d.blocks()[0].spans(), [(0, 2, Style::NONE), (2, 3, Style::BOLD)]);
        // a selection replaced
        d.edit_text(0, "aZ", 2, None, None);
        assert_eq!(d.blocks()[0].text, "aZ");
        // a deletion
        d.edit_text(0, "Z", 0, None, None);
        assert_eq!(d.blocks()[0].text, "Z");
        assert_eq!(d.blocks()[0].marks.len(), 1);
    }

    #[test]
    fn merges_and_pastes_keep_the_styles() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "ab"), (Kind::Body, "cd")]);
        d.toggle_style(2, 0, 2, Style::ITALIC);
        d.backspace(2, 0);
        assert_eq!(d.blocks()[1].spans(), [(0, 2, Style::NONE), (2, 4, Style::ITALIC)]);
        d.insert_text(1, 4, 4, "x\ny");
        assert_eq!(d.blocks()[1].spans(), [(0, 2, Style::NONE), (2, 5, Style::ITALIC)]);
        assert_eq!(d.blocks()[2].spans(), [(0, 1, Style::ITALIC)]);
        d.delete(1, 5);
        assert_eq!(d.blocks()[1].text, "abcdxy");
        assert_eq!(d.blocks()[1].marks.len(), 6);
    }

    // ---- inline styles in the stored text ----

    #[test]
    fn styled_words_are_stored_with_markers_where_the_style_changes() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "plain bold both under")]);
        d.toggle_style(1, 6, 15, Style::BOLD);
        d.toggle_style(1, 11, 15, Style::ITALIC);
        d.toggle_style(1, 16, 21, Style::UNDERLINE);
        assert_eq!(d.to_text(), "# t\nplain **bold *both*** <u>under</u>");
        assert_eq!(Document::parse(&d.to_text()), d);
        let mut s = doc(&[(Kind::Title, r"a*b~c<d\e")]);
        s.toggle_style(0, 1, 4, Style::STRIKE);
        assert_eq!(s.to_text(), r"# a~~\*b\~~~c\<d\\e");
        assert_eq!(Document::parse(&s.to_text()), s);
    }

    #[test]
    fn older_notes_keep_their_meaning() {
        // whole-line markers around text with a marker character inside
        let d = Document::parse("t\n**a*b**\n*x* and *");
        assert_eq!(d.blocks()[1].text, "a*b");
        assert_eq!(d.blocks()[1].spans(), [(0, 3, Style::BOLD)]);
        // an odd marker: read as before (the outer pair wraps the line)
        assert_eq!((d.blocks()[2].text.as_str(), d.blocks()[2].spans()), ("x* and ", vec![(0, 7, Style::ITALIC)]));
        // a backslash that escapes nothing, and a first-character escape
        let e = Document::parse("C:\\Users\n\\**b** c");
        assert_eq!(e.blocks()[0].text, r"C:\Users");
        assert_eq!(e.blocks()[1].text, "**b** c");
        for d in [d, e] {
            assert_eq!(Document::parse(&d.to_text()), d);
        }
    }

    #[test]
    fn toggling_the_list_switches_body_and_list_item_only() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "b"), (Kind::Heading, "h")]);
        assert!(d.toggle_list(1));
        assert_eq!(d.blocks()[1].kind, Kind::ListItem);
        assert!(d.toggle_list(1));
        assert_eq!(d.blocks()[1].kind, Kind::Body);
        assert!(!d.toggle_list(0) && !d.toggle_list(2) && !d.toggle_list(9));
    }

    #[test]
    fn inserting_an_image_goes_after_the_block_and_never_ends_the_note() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "b")]);
        assert_eq!(d.insert_image(0, 7), 1);
        assert_eq!(kinds(&d), [Kind::Title, Kind::Image, Kind::Body]);
        assert_eq!(d.len(), 3);
        assert_eq!(d.insert_image(2, 8), 3);
        assert_eq!(kinds(&d), [Kind::Title, Kind::Image, Kind::Body, Kind::Image, Kind::Body]);
        assert_eq!(d.insert_image(99, 9), 5, "past the end appends");
        assert_eq!(Document::parse(&d.to_text()), d);
    }

    // ---- paste ----

    #[test]
    fn a_single_line_paste_replaces_the_selection() {
        let mut d = doc(&[(Kind::Title, "hello world")]);
        assert_eq!(d.insert_text(0, 0, 5, "HEY"), Caret { block: 0, offset: 3 });
        assert_eq!(texts(&d), ["HEY world"]);
    }

    #[test]
    fn a_multi_line_paste_is_split_into_paragraphs() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "ab|cd")]);
        let c = d.insert_text(1, 3, 3, "x\r\ny\nz");
        assert_eq!(c, Caret { block: 3, offset: 1 });
        assert_eq!(texts(&d), ["t", "ab|x", "y", "zcd"]);
        assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Body, Kind::Body]);
    }

    #[test]
    fn pasting_lines_into_the_title_or_a_list() {
        let mut d = doc(&[(Kind::Title, "")]);
        d.insert_text(0, 0, 0, "one\ntwo\n\nthree");
        assert_eq!((kinds(&d), texts(&d)), (vec![Kind::Title, Kind::Body, Kind::Body, Kind::Body], vec!["one", "two", "", "three"]));
        let mut e = doc(&[(Kind::Title, "t"), (Kind::ListItem, "a")]);
        e.insert_text(1, 1, 1, "\nb");
        assert_eq!(kinds(&e), [Kind::Title, Kind::ListItem, Kind::ListItem]);
        let c = e.insert_text(1, 0, 99, "\n");
        assert_eq!((c, texts(&e)), (Caret { block: 2, offset: 0 }, vec!["t", "", "", "b"]));
    }

    // ---- navigation helpers ----

    #[test]
    fn a_picture_remembers_its_size_and_place_in_the_stored_text() {
        let mut d = doc(&[(Kind::Title, "t")]);
        d.insert_image(0, 5);
        assert_eq!(d.to_text(), "# t\n![](img:5)\n");
        assert!(d.set_image_pct(1, 40));
        assert!(!d.set_image_pct(1, 40), "nothing changes");
        assert!(d.set_image_align(1, Align::Right));
        assert!(!d.set_image_align(0, Align::Right), "the title is no picture");
        assert_eq!(d.to_text(), "# t\n![](img:5,40,r)\n");
        let back = Document::parse(&d.to_text());
        assert_eq!((back.blocks()[1].image_pct, back.blocks()[1].align), (40, Align::Right));
        assert!(d.set_image_pct(1, 1));
        assert_eq!(d.blocks()[1].image_pct, IMAGE_PCT_MIN, "clamped");
        assert_eq!(Document::parse("# t\n![](img:5,500,c)").blocks()[1].image_pct, IMAGE_PCT_MAX);
        assert_eq!(Document::parse("# t\n![](img:5,x,y)").blocks()[1].image_pct, IMAGE_PCT_MAX);
    }

    #[test]
    fn the_box_of_a_picture_follows_its_width_and_place() {
        let mut b = Block::image(1);
        assert_eq!(b.image_box(200.0, (100, 50)), (0.0, 200.0, 100.0));
        b.image_pct = 50;
        b.align = Align::Center;
        assert_eq!(b.image_box(200.0, (100, 50)), (50.0, 100.0, 50.0));
        b.align = Align::Right;
        assert_eq!(b.image_box(200.0, (100, 50)), (100.0, 100.0, 50.0));
    }

    #[test]
    fn deleting_a_picture_leaves_a_text_block_for_the_caret() {
        let mut d = doc(&[(Kind::Title, "t")]);
        d.insert_image(0, 9);
        assert_eq!(d.delete_image(1), Some(Caret { block: 1, offset: 0 }));
        assert_eq!(kinds(&d), [Kind::Title, Kind::Body]);
        assert_eq!(d.delete_image(0), None, "the title is no picture");
        let mut e = Document::from_blocks(vec![Block::new(Kind::Title, "t"), Block::image(1)]);
        assert_eq!(e.delete_image(1), Some(Caret { block: 1, offset: 0 }));
        assert_eq!(kinds(&e), [Kind::Title, Kind::Body]);
    }

    #[test]
    fn neighbors_skip_images() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "b")]);
        d.insert_image(0, 1);
        assert_eq!(d.next_text_block(0), Some(2));
        assert_eq!(d.prev_text_block(2), Some(0));
        assert_eq!(d.prev_text_block(0), None);
        assert_eq!(d.next_text_block(2), None);
    }

    #[test]
    fn plain_text_is_the_lines_without_pictures() {
        let mut d = doc(&[(Kind::Title, "T"), (Kind::Body, "a"), (Kind::Body, "b")]);
        d.insert_image(1, 7);
        assert_eq!(d.plain_text(), "T\na\nb");
    }

    #[test]
    fn replacing_everything_starts_over_with_the_text() {
        let mut d = doc(&[(Kind::Title, "T"), (Kind::Body, "a"), (Kind::Body, "b")]);
        let c = d.replace_all("x\ny");
        assert_eq!(texts(&d), ["x", "y"]);
        assert_eq!(c, Caret { block: 1, offset: 1 });
        let c = d.replace_all("");
        assert!(d.is_empty());
        assert_eq!(c, Caret { block: 0, offset: 0 });
    }

    #[test]
    fn newlines_never_enter_block_text() {
        let mut d = Document::new();
        d.set_text(0, "a\nb");
        assert_eq!(d.blocks()[0].text, "a b");
        assert_eq!(Document::parse(&d.to_text()), d);
    }

    // ---- ranges ----

    fn c(block: usize, offset: usize) -> Caret {
        Caret { block, offset }
    }
    fn r(a: Caret, f: Caret) -> Range {
        Range { anchor: a, focus: f }
    }
    /// Title "Title", body "alpha", image, body "omega", list item "tail".
    fn five() -> Document {
        Document::from_blocks(vec![
            Block::new(Kind::Title, "Title"),
            Block::new(Kind::Body, "alpha"),
            Block::image(7),
            Block::new(Kind::Body, "omega"),
            Block::new(Kind::ListItem, "tail"),
        ])
    }

    #[test]
    fn ordered_puts_the_earlier_caret_first_whichever_way_it_was_dragged() {
        let (a, b) = (c(1, 3), c(3, 1));
        assert_eq!(r(a, b).ordered(), (a, b));
        assert_eq!(r(b, a).ordered(), (a, b));
        assert_eq!(r(c(2, 4), c(2, 1)).ordered(), (c(2, 1), c(2, 4)));
    }

    #[test]
    fn replace_range_inside_one_block_is_insert_text() {
        let mut a = five();
        let mut b = five();
        let got = a.replace_range(r(c(1, 1), c(1, 4)), "X
Y");
        assert_eq!(got, b.insert_text(1, 1, 4, "X
Y"));
        assert_eq!(a, b);
        assert_eq!(texts(&a), ["Title", "aX", "Ya", "", "omega", "tail"]);
    }

    #[test]
    fn replace_range_across_two_blocks_merges_them_and_keeps_the_first_kind() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Heading, "Hello"), (Kind::ListItem, "world")]);
        let caret = d.replace_range(r(c(1, 2), c(2, 3)), "x");
        assert_eq!(texts(&d), ["t", "Hexld"]);
        assert_eq!(kinds(&d), [Kind::Title, Kind::Heading]);
        assert_eq!(caret, c(1, 3));
    }

    #[test]
    fn delete_range_across_three_blocks_drops_the_image_and_the_blocks_between() {
        let mut d = five();
        let caret = d.delete_range(r(c(1, 2), c(3, 2)));
        assert_eq!(texts(&d), ["Title", "alega", "tail"]);
        assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::ListItem]);
        assert_eq!(caret, c(1, 2));
    }

    #[test]
    fn a_reversed_range_behaves_like_the_ordered_one() {
        let mut a = five();
        let mut b = five();
        let ca = a.replace_range(r(c(3, 2), c(1, 2)), "Q");
        let cb = b.replace_range(r(c(1, 2), c(3, 2)), "Q");
        assert_eq!((a, ca), (b, cb));
    }

    #[test]
    fn replace_range_from_the_start_to_the_end_of_the_note_is_replace_all() {
        let mut d = five();
        let caret = d.replace_range(r(c(0, 0), c(4, 4)), "x
y");
        assert_eq!(texts(&d), ["x", "y"]);
        assert_eq!(kinds(&d), [Kind::Title, Kind::Body]);
        assert_eq!(caret, c(1, 1));
        let mut d = five();
        assert_eq!(d.replace_range(r(c(4, 4), c(0, 0)), ""), c(0, 0));
        assert_eq!(texts(&d), [""]);
        assert!(d.is_empty());
    }

    #[test]
    fn a_range_at_the_start_or_end_of_the_note() {
        let mut d = five();
        assert_eq!(d.delete_range(r(c(0, 0), c(0, 3))), c(0, 0));
        assert_eq!(d.blocks()[0].text, "le");
        let mut d = five();
        assert_eq!(d.delete_range(r(c(3, 2), c(4, 4))), c(3, 2));
        assert_eq!(texts(&d), ["Title", "alpha", "", "om"]);
        let mut d = five();
        assert_eq!(d.delete_range(r(c(0, 3), c(1, 0))), c(0, 3));
        assert_eq!(texts(&d)[..2], ["Titalpha", ""]);
    }

    #[test]
    fn a_collapsed_range_inserts_text_and_deletes_nothing() {
        let mut d = five();
        assert_eq!(d.replace_range(r(c(1, 2), c(1, 2)), "--"), c(1, 4));
        assert_eq!(d.blocks()[1].text, "al--pha");
        let before = five();
        let mut d = five();
        assert_eq!(d.delete_range(r(c(1, 2), c(1, 2))), c(1, 2));
        assert_eq!(d, before);
    }

    #[test]
    fn an_image_at_either_end_of_a_range_is_deleted_with_it() {
        let mut d = five();
        let caret = d.delete_range(r(c(1, 3), c(2, 0)));
        assert_eq!(texts(&d), ["Title", "alp", "omega", "tail"]);
        assert_eq!(caret, c(1, 3));
        let mut d = five();
        let caret = d.delete_range(r(c(2, 0), c(3, 2)));
        assert_eq!(texts(&d), ["Title", "alpha", "ega", "tail"]);
        assert_eq!(caret, c(2, 0));
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "a")]);
        d.insert_image(1, 3);
        let mut d2 = d.clone();
        // image to image with no text beside them leaves one empty body line
        d2.insert_image(2, 4);
        d2.delete_range(r(c(2, 0), c(3, 0)));
        assert!(d2.blocks().iter().all(|b| b.kind != Kind::Image));
    }

    #[test]
    fn plain_range_joins_lines_with_newlines_and_skips_images() {
        let d = five();
        assert_eq!(d.plain_range(r(c(1, 2), c(4, 2))), "pha
omega
ta");
        assert_eq!(d.plain_range(r(c(4, 2), c(1, 2))), "pha
omega
ta");
        assert_eq!(d.plain_range(r(c(1, 1), c(1, 3))), "lp");
        assert_eq!(d.plain_range(r(c(1, 1), c(1, 1))), "");
        assert_eq!(d.plain_range(r(c(0, 0), c(4, 4))), d.plain_text());
    }

    #[test]
    fn toggle_style_range_across_blocks_makes_everything_bold_if_only_part_is() {
        let mut d = Document::from_blocks(vec![
            Block::new(Kind::Title, "t"),
            Block::new(Kind::Body, "abc").with_style(Style::BOLD),
            Block::new(Kind::Body, "def"),
        ]);
        let range = r(c(1, 1), c(2, 2));
        assert_eq!(d.common_style_range(range), Style::NONE);
        assert!(d.toggle_style_range(range, Style::BOLD));
        assert_eq!(d.common_style_range(range), Style::BOLD);
        assert_eq!(d.blocks()[1].marks, [Style::BOLD; 3]);
        assert_eq!(d.blocks()[2].marks, [Style::BOLD, Style::BOLD, Style::NONE]);
        assert!(d.toggle_style_range(range, Style::BOLD));
        assert_eq!(d.blocks()[1].marks, [Style::BOLD, Style::NONE, Style::NONE]);
        assert_eq!(d.blocks()[2].marks, [Style::NONE; 3]);
    }

    #[test]
    fn style_over_a_range_without_text_does_nothing() {
        let mut d = five();
        assert!(!d.toggle_style_range(r(c(1, 2), c(1, 2)), Style::BOLD));
        assert!(!d.toggle_style_range(r(c(2, 0), c(2, 0)), Style::BOLD));
        assert_eq!(d.common_style_range(r(c(1, 2), c(1, 2))), Style::NONE);
        let mut one = doc(&[(Kind::Title, "ab"), (Kind::Body, "cd")]);
        let mut same = one.clone();
        assert!(one.toggle_style_range(r(c(1, 1), c(1, 0)), Style::ITALIC));
        assert!(same.toggle_style(1, 0, 1, Style::ITALIC));
        assert_eq!(one, same);
    }

    // ---- color ----

    fn colored(d: &Document, block: usize) -> Vec<(u8, u8)> {
        d.blocks()[block].paints.iter().map(|p| (p.ink, p.marker)).collect()
    }

    #[test]
    fn colors_mixed_with_every_style_round_trip_on_one_line() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "plain bold ital under strike end")]);
        d.set_ink(1, 6, 15, 6);
        d.set_marker(1, 10, 21, 4);
        d.toggle_style(1, 6, 10, Style::BOLD);
        d.toggle_style(1, 11, 17, Style::ITALIC);
        d.toggle_style(1, 14, 25, Style::UNDERLINE);
        d.toggle_style(1, 19, 27, Style::STRIKE);
        let text = d.to_text();
        assert!(text.contains("<c=blue>") && text.contains("<m=yellow>") && text.contains("</c>") && text.contains("</m>"), "{text}");
        assert_eq!(Document::parse(&text), d, "{text}");
    }

    #[test]
    fn a_line_without_color_markers_reads_and_writes_as_before() {
        let d = Document::parse("# t\n**a** *b* ~~c~~ <u>d</u> \\<c=blue>x");
        assert!(d.blocks()[1].paints.iter().all(|p| *p == Paint::default()));
        assert_eq!(d.blocks()[1].text, "a b c d <c=blue>x");
        assert_eq!(d.to_text(), "# t\n**a** *b* ~~c~~ <u>d</u> \\<c=blue>x");
    }

    #[test]
    fn the_stored_markers_read_by_name() {
        let d = Document::parse("# t\nx <c=red>y<m=green>z</c> w</m>!");
        assert_eq!(d.blocks()[1].text, "x yz w!");
        assert_eq!(colored(&d, 1), [(0, 0), (0, 0), (9, 0), (9, 5), (0, 5), (0, 5), (0, 0)]);
    }

    #[test]
    fn an_unknown_color_name_is_plain_text_color_and_the_tag_is_dropped() {
        let d = Document::parse("# t\na<c=chartreuse>b</c>c<m=none>d</m>**e**");
        assert_eq!(d.blocks()[1].text, "abcde");
        assert!(d.blocks()[1].paints.iter().all(|p| *p == Paint::default()));
        assert!(d.blocks()[1].marks[4].contains(Style::BOLD), "the rest of the line is still read the new way");
    }

    #[test]
    fn property_colored_documents_round_trip() {
        let mut r = Rng(7);
        for _ in 0..3000 {
            let blocks = (0..1 + r.next(5))
                .map(|_| {
                    let mut b = Block::new(Kind::Body, &random_text(&mut r));
                    let starts: Vec<usize> = b.text.char_indices().map(|(i, _)| i).collect();
                    for (k, &i) in starts.iter().enumerate() {
                        let end = starts.get(k + 1).copied().unwrap_or(b.text.len());
                        b.marks[i..end].fill(Style::from_bits(r.next(16) as u8));
                        b.paints[i..end].fill(Paint { ink: r.next(10) as u8, marker: r.next(10) as u8 });
                    }
                    b
                })
                .collect();
            let d = Document::from_blocks(blocks);
            assert_eq!(Document::parse(&d.to_text()), d, "{:?}", d.to_text());
        }
    }

    #[test]
    fn a_range_over_three_blocks_takes_one_ink_and_a_mixed_range_has_no_common_color() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "abc"), (Kind::Body, "def"), (Kind::Body, "ghi")]);
        let range = r(c(1, 1), c(3, 2));
        assert_eq!(d.common_ink_range(range), Some(0));
        assert!(d.set_ink_range(range, 6));
        assert_eq!(d.common_ink_range(range), Some(6));
        assert_eq!(colored(&d, 1), [(0, 0), (6, 0), (6, 0)]);
        assert_eq!(colored(&d, 3), [(6, 0), (6, 0), (0, 0)]);
        assert_eq!(d.common_ink_range(r(c(1, 0), c(3, 3))), None);
        assert_eq!(d.common_marker_range(r(c(1, 0), c(3, 3))), Some(0));
        assert!(d.set_marker(1, 0, 1, 2));
        assert_eq!(d.common_marker_range(r(c(1, 0), c(3, 3))), None);
    }

    #[test]
    fn setting_a_color_is_idempotent_and_clear_sets_default() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "abc")]);
        let all = d.all();
        assert!(d.set_ink_range(all, 3));
        assert!(!d.set_ink_range(all, 3), "picking the color the range has is a no-op, not a toggle");
        assert_eq!(d.common_ink_range(all), Some(3));
        assert!(d.set_marker_range(all, 5));
        assert!(d.set_ink_range(all, 0) && d.set_marker_range(all, 0));
        assert_eq!(colored(&d, 1), [(0, 0); 3]);
    }

    #[test]
    fn color_over_an_empty_range_or_an_image_changes_nothing() {
        let mut d = Document::from_blocks(vec![Block::new(Kind::Title, "t"), Block::image(1), Block::new(Kind::Body, "ab")]);
        assert!(!d.set_ink_range(r(c(1, 0), c(1, 0)), 2));
        assert_eq!(d.common_ink_range(r(c(1, 0), c(1, 0))), None);
        assert!(d.set_ink_range(r(c(0, 0), c(2, 2)), 2), "an image in the range is skipped");
        assert_eq!(colored(&d, 2), [(2, 0); 2]);
        assert!(d.blocks()[1].paints.is_empty());
    }

    #[test]
    fn typing_takes_the_paint_of_the_character_before_or_the_next_at_a_block_start() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "ab")]);
        d.set_ink(1, 0, 1, 6);
        d.set_marker(1, 0, 1, 4);
        d.edit_text(1, "aXb", 2, None, None);
        assert_eq!(colored(&d, 1), [(6, 4), (6, 4), (0, 0)]);
        d.edit_text(1, "YaXb", 1, None, None);
        assert_eq!(colored(&d, 1)[0], (6, 4), "at the start it takes the next character's");
        assert_eq!(d.blocks()[1].paint_for_insert(0), Paint { ink: 6, marker: 4 });
    }

    #[test]
    fn paints_stay_as_long_as_the_text_through_every_operation() {
        let mut d = doc(&[(Kind::Title, "title"), (Kind::Body, "héllo wörld"), (Kind::Body, "second")]);
        d.set_ink_range(d.all(), 9);
        let ok = |d: &Document| {
            for b in d.blocks() {
                assert_eq!(b.paints.len(), b.text.len(), "{b:?}");
                assert_eq!(b.paints.len(), b.marks.len());
            }
        };
        d.split(1, 3);
        ok(&d);
        d.backspace(2, 0);
        ok(&d);
        d.delete(1, d.blocks()[1].text.len());
        ok(&d);
        d.insert_text(1, 2, 4, "a\nbc\nd");
        ok(&d);
        d.replace_range(r(c(0, 2), c(2, 1)), "zz");
        ok(&d);
        d.delete_range(r(c(0, 0), c(d.len() - 1, 1)));
        ok(&d);
        d.set_text(0, "new text");
        ok(&d);
        let mut e = doc(&[(Kind::Title, "t"), (Kind::Body, "# x")]);
        e.auto_format(1, "#x", 2);
        ok(&e);
    }

    #[test]
    fn paints_survive_a_clone_for_undo_and_redo() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "abc")]);
        let before = d.clone();
        d.set_ink_range(d.all(), 2);
        let after = d.clone();
        assert_ne!(before, after);
        d = before.clone();
        assert_eq!(colored(&d, 1), [(0, 0); 3]);
        d = after;
        assert_eq!(colored(&d, 1), [(2, 0); 3]);
    }

    #[test]
    fn the_palette_has_ten_entries_with_the_spec_colors() {
        assert_eq!(PALETTE.len(), 10);
        assert_eq!((PALETTE[0].name, PALETTE[0].ink, PALETTE[0].marker), ("Default", 0x2B2F36, None));
        assert_eq!((PALETTE[6].name, PALETTE[6].ink, PALETTE[6].marker), ("Blue", 0x3A6D96, Some(0xD3E5EF)));
        assert_eq!(PALETTE[9].marker, Some(0xFFE2DD));
    }
}
