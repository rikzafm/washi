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
//! 1. numbered item                  Numbered (its number counted again on reading)
//! - [ ] to do / - [x] done           Check / Checked
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
    /// A paragraph with its number in front (`1.`, `2.` ...). The number is not stored on the block:
    /// it counts the unbroken run of numbered items it is in ([`Document::number`]).
    Numbered,
    /// A paragraph with a box in front, not ticked yet.
    Check,
    /// A check paragraph that is ticked: drawn grey and struck through.
    Checked,
    Image,
}

impl Kind {
    /// Whether this is a check paragraph, ticked or not.
    pub fn is_check(self) -> bool {
        matches!(self, Kind::Check | Kind::Checked)
    }

    /// Whether this is a bulleted or numbered list item.
    pub fn is_list(self) -> bool {
        matches!(self, Kind::ListItem | Kind::Numbered)
    }

    /// Whether this is a bulleted or numbered item: the kinds Tab can move. Check items are not
    /// part of it, they never move.
    pub fn is_item(self) -> bool {
        self.is_list()
    }
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
    /// Whether the list this item belongs to is moved in (Tab). Only meaningful on list and check
    /// items, and only the first item of a list counts ([`indents_of`]); [`Document`] keeps every
    /// item of a list the same.
    pub indented: bool,
}

impl Block {
    pub fn new(kind: Kind, text: &str) -> Block {
        let text = one_line(text);
        Block { kind, marks: vec![Style::NONE; text.len()], paints: vec![Paint::default(); text.len()], text, image_id: None, image_pct: IMAGE_PCT_MAX, align: Align::Left, indented: false }
    }

    pub fn image(id: i64) -> Block {
        Block { kind: Kind::Image, text: String::new(), marks: vec![], paints: vec![], image_id: Some(id), image_pct: IMAGE_PCT_MAX, align: Align::Left, indented: false }
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

#[derive(Debug, PartialEq, Eq)]
pub struct Document {
    blocks: Vec<Block>,
}

impl Clone for Document {
    fn clone(&self) -> Self {
        crate::trace::count(crate::trace::counts::Count::DocumentClone);
        Document { blocks: self.blocks.clone() }
    }
}

impl Default for Document {
    fn default() -> Self {
        Document::new()
    }
}

/// The kind a body block turns into when its text becomes `text` with the caret at `cursor` after
/// the marker's space was typed (`old` is the text before the edit).
fn auto_format_kind(kind: Kind, old: &str, text: &str, cursor: usize) -> Option<(Kind, usize)> {
    const MARKERS: [(&str, Kind); 8] = [
        ("# ", Kind::Heading),
        ("- ", Kind::ListItem),
        ("* ", Kind::ListItem),
        ("1. ", Kind::Numbered),
        ("1, ", Kind::Numbered),
        ("[] ", Kind::Check),
        ("[ ] ", Kind::Check),
        ("[x] ", Kind::Checked),
    ];
    if kind != Kind::Body {
        return None;
    }
    let (marker, to) = MARKERS.iter().find(|(m, _)| m.len() == cursor && text.starts_with(m))?;
    if old.starts_with(marker) {
        return None;
    }
    Some((*to, marker.len()))
}

fn one_line(s: &str) -> String {
    if s.contains(['\n', '\r']) {
        s.replace("\r\n", " ").replace(['\n', '\r'], " ")
    } else {
        s.to_string()
    }
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

    /// Whether `block` exists and is an image.
    pub fn is_image(&self, block: usize) -> bool {
        self.blocks.get(block).is_some_and(|b| b.kind == Kind::Image)
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
                text_format::parse_line(i, line)
            })
            .collect();
        let mut doc = Document { blocks };
        doc.settle_indents();
        doc
    }

    /// The stored text (inverse of [`Document::parse`]).
    pub fn to_text(&self) -> String {
        crate::trace::count(crate::trace::counts::Count::ToText);
        // a note nobody wrote in is stored as before: no content at all
        if self.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        let (numbers, indents) = (self.numbers(), self.indents());
        for (i, b) in self.blocks.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&text_format::write_block(b, numbers[i], indents[i]));
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
        if (b.kind.is_list() || b.kind.is_check()) && b.text.is_empty() {
            self.blocks[block].kind = Kind::Body;
            self.blocks[block].indented = false;
            self.settle_indents();
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

    /// Backspace at the start of a block: a list item first loses its bullet (becoming body, caret
    /// in place); otherwise merge it into the previous one (or delete a previous
    /// image). `None` when nothing happens: not at the start, or the title.
    pub fn backspace(&mut self, block: usize, offset: usize) -> Option<Caret> {
        if offset != 0 || block == 0 || !self.blocks.get(block)?.is_text() {
            return None;
        }
        if self.blocks[block].kind.is_item() || self.blocks[block].kind.is_check() {
            // an item in a list that is moved in first moves the list back, then loses its marker
            if !self.set_indent_range(Range { anchor: Caret { block, offset: 0 }, focus: Caret { block, offset: 0 } }, false) {
                self.blocks[block].kind = Kind::Body;
                self.blocks[block].indented = false;
                self.settle_indents();
            }
            return Some(Caret { block, offset: 0 });
        }
        if self.blocks[block - 1].kind == Kind::Image {
            self.blocks.remove(block - 1);
            self.settle_indents();
            return Some(Caret { block: block - 1, offset: 0 });
        }
        let cur = self.blocks.remove(block);
        let prev = &mut self.blocks[block - 1];
        let at = prev.text.len();
        prev.text.push_str(&cur.text);
        prev.marks.extend(cur.marks);
        prev.paints.extend(cur.paints);
        self.settle_indents();
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
        self.settle_indents();
        Some(Caret { block, offset })
    }

    /// Typing a space after `#` or `-` at the start of a body block (`old` was the block's text
    /// before the edit, the caret is now at `cursor`) turns it into a heading or list item and
    /// drops the marker. Returns the caret (the start of the block) when it did.
    pub fn auto_format(&mut self, block: usize, old: &str, cursor: usize) -> Option<Caret> {
        let snap = self.item_snapshot();
        let b = self.blocks.get_mut(block)?;
        let (kind, len) = auto_format_kind(b.kind, old, &b.text, cursor)?;
        b.kind = kind;
        b.text.drain(..len);
        b.marks.drain(..len);
        b.paints.drain(..len);
        self.adopt_range(&[block], snap);
        Some(Caret { block, offset: 0 })
    }

    /// Whether [`Self::auto_format`] would fire for an edit of `block` that makes its text `new`
    /// (it was `old`) with the caret at `cursor`: asked before the edit, so that only an edit that
    /// formats needs the document as it was.
    pub fn would_auto_format(&self, block: usize, old: &str, new: &str, cursor: usize) -> bool {
        // the text as `edit_text` writes it, so the two cannot disagree
        let new = one_line(new);
        self.blocks.get(block).is_some_and(|b| b.is_text() && auto_format_kind(b.kind, old, &new, cursor).is_some())
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

    /// The word around byte `offset` of a block: its byte range when at, inside, or at the trailing
    /// boundary of a word before whitespace, punctuation, or EOF.
    pub fn word_at(&self, block: usize, offset: usize) -> Option<(usize, usize)> {
        let t = &self.blocks.get(block)?.text;
        let at = floor_boundary(t, offset);
        let word = |c: char| c.is_alphanumeric() || c == '_' || c == '\'';
        let start = t[..at].char_indices().rev().take_while(|&(_, c)| word(c)).last().map_or(at, |(i, _)| i);
        let end = t[at..].char_indices().find(|&(_, c)| !word(c)).map_or(t.len(), |(i, _)| at + i);
        (start < end).then_some((start, end))
    }

    /// Toggle a body block into a list item and back. False for titles, headings and images.
    pub fn toggle_list(&mut self, block: usize) -> bool {
        let snap = self.item_snapshot();
        match self.blocks.get_mut(block) {
            Some(b) if b.kind == Kind::Body => {
                b.kind = Kind::ListItem;
                self.adopt_range(&[block], snap);
                true
            }
            Some(b) if b.kind == Kind::ListItem => {
                b.kind = Kind::Body;
                self.adopt_range(&[block], snap);
                true
            }
            _ => false,
        }
    }

    /// The bullet list over a range: [`Document::set_list_range`] with bulleted items.
    pub fn toggle_list_range(&mut self, range: Range) -> bool {
        self.set_list_range(range, Kind::ListItem)
    }

    /// A list type picked for a range (`to` is [`Kind::ListItem`] or [`Kind::Numbered`]): if every
    /// paragraph in it that can be a list already is one of that type they all become body, else
    /// they all become that type. Paragraphs outside the range keep their kind. False when it holds
    /// no such paragraph.
    pub fn set_list_range(&mut self, range: Range, to: Kind) -> bool {
        debug_assert!(to.is_list());
        let (s, e) = self.ordered_in(range);
        let eligible = self.list_eligible(s.block, e.block);
        if eligible.is_empty() {
            return false;
        }
        let to = if eligible.iter().all(|&i| self.blocks[i].kind == to) { Kind::Body } else { to };
        let snap = self.item_snapshot();
        for &i in &eligible {
            self.blocks[i].kind = to;
        }
        self.adopt_range(&eligible, snap);
        true
    }

    /// Every list paragraph (bulleted or numbered) in a range becomes body. False when it holds none.
    pub fn clear_list_range(&mut self, range: Range) -> bool {
        let (s, e) = self.ordered_in(range);
        let mut done = false;
        for i in s.block..=e.block {
            if self.blocks[i].kind.is_list() {
                self.blocks[i].kind = Kind::Body;
                self.blocks[i].indented = false;
                done = true;
            }
        }
        self.settle_indents();
        done
    }

    /// The number shown in front of `block`: its place (from 1) in the unbroken run of numbered
    /// items it belongs to. `None` when it is no numbered item.
    pub fn number(&self, block: usize) -> Option<usize> {
        if self.blocks.get(block)?.kind != Kind::Numbered {
            return None;
        }
        Some(1 + self.blocks[..block].iter().rev().take_while(|b| b.kind == Kind::Numbered).count())
    }

    /// [`Document::number`] of every block, 0 for the blocks that are no numbered item.
    pub fn numbers(&self) -> Vec<usize> {
        numbers_of(&self.blocks)
    }

    /// Whether each block is in a list that is moved in (Tab); see [`indents_of`].
    pub fn indents(&self) -> Vec<bool> {
        indents_of(&self.blocks)
    }

    /// The first and last block of the unbroken run of items of one kind (all bulleted, or all
    /// numbered) `block` is in; `None` when it is neither. A neighbouring list of the other kind, or
    /// a check item, is not part of the run.
    pub fn list_run(&self, block: usize) -> Option<(usize, usize)> {
        let kind = self.blocks.get(block)?.kind;
        if !kind.is_item() {
            return None;
        }
        let first = (0..block).rev().take_while(|&i| self.blocks[i].kind == kind).last().unwrap_or(block);
        let last = (block + 1..self.blocks.len()).take_while(|&i| self.blocks[i].kind == kind).last().unwrap_or(block);
        Some((first, last))
    }

    /// Tab (`indented`) or Shift+Tab over a range: every list the range touches moves, whole. False
    /// when the range touches no list or every list it touches is already there.
    pub fn set_indent_range(&mut self, range: Range, indented: bool) -> bool {
        let (s, e) = self.ordered_in(range);
        let before = self.indents();
        let mut i = s.block;
        while i <= e.block {
            match self.list_run(i) {
                Some((first, last)) => {
                    for b in &mut self.blocks[first..=last] {
                        b.indented = indented;
                    }
                    i = last + 1;
                }
                None => i += 1,
            }
        }
        self.indents() != before
    }

    /// Which blocks are list or check items now and where their lists stand: taken before an edit
    /// that makes items, for [`Self::adopt_range`].
    fn item_snapshot(&self) -> (Vec<bool>, Vec<bool>) {
        (self.blocks.iter().map(|b| b.kind.is_item()).collect(), self.indents())
    }

    /// After an edit made items out of `blocks` (`was` and `before` from [`Self::item_snapshot`] taken
    /// before it): each new item takes the position of the nearest item that was in its list before
    /// (the one above first), so a new item does not move the list next to it; a block that is no
    /// item any more loses its position; then every list takes one position.
    fn adopt_range(&mut self, blocks: &[usize], (was, before): (Vec<bool>, Vec<bool>)) {
        for &i in blocks {
            if !self.blocks[i].kind.is_item() {
                self.blocks[i].indented = false;
                continue;
            }
            let kind = self.blocks[i].kind;
            let up = (0..i).rev().take_while(|&j| self.blocks[j].kind == kind).find(|&j| was.get(j) == Some(&true));
            let down = (i + 1..self.blocks.len()).take_while(|&j| self.blocks[j].kind == kind).find(|&j| was.get(j) == Some(&true));
            let src = if was[i] { Some(i) } else { up.or(down) };
            self.blocks[i].indented = src.is_some_and(|j| before[j]);
        }
        self.settle_indents();
    }

    /// Give every list the position of its first item, after an edit that may have joined lists.
    fn settle_indents(&mut self) {
        let ind = self.indents();
        for (b, f) in self.blocks.iter_mut().zip(ind) {
            b.indented = f;
        }
    }

    /// The paragraphs of blocks `from..=to` that can be a list or check item (not title, heading, picture).
    fn list_eligible(&self, from: usize, to: usize) -> Vec<usize> {
        (from..=to).filter(|&i| matches!(self.blocks[i].kind, Kind::Body | Kind::ListItem | Kind::Numbered | Kind::Check | Kind::Checked)).collect()
    }

    /// The check button over a range: if every body, list or check paragraph in it is a check item
    /// they all become body, else they all become check items (ticked ones stay ticked). False when
    /// it holds none of those.
    pub fn toggle_check_range(&mut self, range: Range) -> bool {
        let (s, e) = self.ordered_in(range);
        let eligible = self.list_eligible(s.block, e.block);
        if eligible.is_empty() {
            return false;
        }
        let all = eligible.iter().all(|&i| self.blocks[i].kind.is_check());
        let snap = self.item_snapshot();
        for &i in &eligible {
            let b = &mut self.blocks[i];
            b.kind = if all { Kind::Body } else if b.kind == Kind::Checked { Kind::Checked } else { Kind::Check };
        }
        self.adopt_range(&eligible, snap);
        true
    }

    /// Tick or untick the check item at `block`. False when it is none.
    pub fn toggle_checked(&mut self, block: usize) -> bool {
        match self.blocks.get_mut(block) {
            Some(b) if b.kind.is_check() => {
                b.kind = if b.kind == Kind::Check { Kind::Checked } else { Kind::Check };
                true
            }
            _ => false,
        }
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
        self.settle_indents();
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
        self.settle_indents();
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
        self.settle_indents();
        self.insert_text(s.block, from, from, text)
    }

    /// Replace `range` with parsed rich blocks (formatting, colors, lists, and images).
    pub fn replace_range_rich(
        &mut self,
        range: Range,
        parsed: Vec<crate::clipboard_html::ParsedBlock>,
        add_image: &mut dyn FnMut(crate::images::Rgba) -> Option<i64>,
    ) -> Caret {
        use crate::clipboard_html::ParsedBlock;
        if parsed.is_empty() {
            return self.replace_range(range, "");
        }
        let (s, e) = self.ordered_in(range);
        let caret = if s.block != e.block || s.offset != e.offset {
            self.replace_range(range, "")
        } else {
            s
        };

        let block_idx = caret.block.min(self.blocks.len().saturating_sub(1));
        let offset = if self.blocks[block_idx].is_text() {
            floor_boundary(&self.blocks[block_idx].text, caret.offset)
        } else {
            0
        };

        let mut b = self.blocks[block_idx].clone();
        let tail_text = if b.is_text() { b.text.split_off(offset) } else { String::new() };
        let tail_marks = if b.is_text() { b.marks.split_off(offset) } else { Vec::new() };
        let tail_paints = if b.is_text() { b.paints.split_off(offset) } else { Vec::new() };

        let mut new_blocks: Vec<Block> = Vec::new();
        let mut cur_block: Option<Block> = Some(b);

        for (i, p) in parsed.into_iter().enumerate() {
            match p {
                text_block @ (ParsedBlock::Title { .. }
                | ParsedBlock::Heading { .. }
                | ParsedBlock::ListItem { .. }
                | ParsedBlock::Numbered { .. }
                | ParsedBlock::Check { .. }
                | ParsedBlock::Checked { .. }
                | ParsedBlock::Body { .. }) => {
                    let (kind, text, marks, paints) = match text_block {
                        ParsedBlock::Title { text, marks, paints } => (Kind::Title, text, marks, paints),
                        ParsedBlock::Heading { text, marks, paints } => (Kind::Heading, text, marks, paints),
                        ParsedBlock::ListItem { text, marks, paints } => (Kind::ListItem, text, marks, paints),
                        ParsedBlock::Numbered { text, marks, paints } => (Kind::Numbered, text, marks, paints),
                        ParsedBlock::Check { text, marks, paints } => (Kind::Check, text, marks, paints),
                        ParsedBlock::Checked { text, marks, paints } => (Kind::Checked, text, marks, paints),
                        ParsedBlock::Body { text, marks, paints } => (Kind::Body, text, marks, paints),
                        ParsedBlock::Image { .. } => unreachable!(),
                    };
                    if i == 0 && cur_block.as_ref().is_some_and(|b| b.is_text()) {
                        let b = cur_block.as_mut().unwrap();
                        b.text.push_str(&text);
                        b.marks.extend(marks);
                        b.paints.extend(paints);
                    } else {
                        if let Some(prev) = cur_block.take() {
                            new_blocks.push(prev);
                        }
                        cur_block = Some(Block { kind, text, marks, paints, image_id: None, image_pct: IMAGE_PCT_MAX, align: Align::Left, indented: false });
                    }
                }
                ParsedBlock::Image { rgba } => {
                    if let Some(prev) = cur_block.take() {
                        new_blocks.push(prev);
                    }
                    if let Some(id) = add_image(rgba) {
                        new_blocks.push(Block::image(id));
                    }
                }
            }
        }

        let mut final_block = cur_block.unwrap_or_else(|| Block::new(Kind::Body, ""));
        let caret_offset = final_block.text.len();
        final_block.text.push_str(&tail_text);
        final_block.marks.extend(tail_marks);
        final_block.paints.extend(tail_paints);
        new_blocks.push(final_block);

        let final_count = new_blocks.len();
        self.blocks.splice(block_idx..=block_idx, new_blocks);
        self.settle_indents();
        let res_block = block_idx + final_count - 1;
        Caret { block: res_block, offset: caret_offset }
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
    let kind = match prev.kind {
        Kind::ListItem => Kind::ListItem,
        Kind::Numbered => Kind::Numbered,
        Kind::Check | Kind::Checked => Kind::Check,
        _ => Kind::Body,
    };
    Block { kind, text, marks, paints, image_id: None, image_pct: IMAGE_PCT_MAX, align: Align::Left, indented: kind.is_item() && prev.indented }
}

/// Whether each block is in a list that is moved in: every item of an unbroken run of one kind
/// (bulleted, or numbered) takes the flag of the run's first item; any other block, a check item
/// included, is `false`.
pub(crate) fn indents_of(blocks: &[Block]) -> Vec<bool> {
    let mut run = false;
    blocks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            if !b.kind.is_item() {
                return false;
            }
            if i == 0 || blocks[i - 1].kind != b.kind {
                run = b.indented;
            }
            run
        })
        .collect()
}

/// The number of every block (0 for one that is no numbered item): numbered items count from 1 in
/// each unbroken run of them.
pub(crate) fn numbers_of(blocks: &[Block]) -> Vec<usize> {
    let mut run = 0;
    blocks
        .iter()
        .map(|b| {
            run = if b.kind == Kind::Numbered { run + 1 } else { 0 };
            run
        })
        .collect()
}

pub(crate) fn floor_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests;
mod text_format;
