//! The note as a document: an ordered list of blocks (paragraphs), the first always the title.
//!
//! This is the one model the editor, the renderer and storage share. The stored form is a small
//! Markdown subset, one block per line (`parse` / `to_text`):
//!
//! ```text
//! # Endfield Note                    Title (always the first line)
//! ## **Thermite Cutter : POG**       Heading (+ bold)
//! Currently POG no Weapon            Body
//! - list item                        ListItem
//! ![](img:42)                        Image
//! ```
//!
//! Style flags wrap the whole block's text: `**` bold, `*` italic, `~~` strike, `<u>..</u>` underline
//! (outermost). Plain text from older versions (no markers) parses as a title followed by body
//! lines. A first character that could be read as a marker is escaped with a backslash, so every
//! document survives `to_text` then `parse` exactly, and no input is malformed.
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

/// Bold / italic / underline / strike of a whole block.
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

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn from_bits(bits: u8) -> Style {
        Style(bits & 15)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: Kind,
    /// One line; empty for an image.
    pub text: String,
    pub style: Style,
    /// The stored image of an `Image` block.
    pub image_id: Option<i64>,
}

impl Block {
    pub fn new(kind: Kind, text: &str) -> Block {
        Block { kind, text: one_line(text), style: Style::NONE, image_id: None }
    }

    pub fn image(id: i64) -> Block {
        Block { kind: Kind::Image, text: String::new(), style: Style::NONE, image_id: Some(id) }
    }

    pub fn with_style(mut self, style: Style) -> Block {
        self.style = style;
        self
    }

    /// Drawn in the bold face: headings always, any block with the bold flag.
    pub fn is_bold(&self) -> bool {
        self.kind == Kind::Heading || self.style.contains(Style::BOLD)
    }

    /// Whether the block has a text field (everything but an image).
    pub fn is_text(&self) -> bool {
        self.kind != Kind::Image
    }
}

/// A caret position: block index and byte offset into its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caret {
    pub block: usize,
    pub offset: usize,
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

/// Characters that start a marker somewhere in the format; a text starting with one is escaped.
const SPECIAL: [char; 7] = ['\\', '*', '~', '<', '#', '-', '!'];

fn escape(text: &str) -> String {
    match text.chars().next() {
        Some(c) if SPECIAL.contains(&c) => format!("\\{text}"),
        _ => text.to_string(),
    }
}

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

fn wrap(style: Style, text: &str) -> String {
    let mut s = escape(text);
    if style.contains(Style::ITALIC) {
        s = format!("*{s}*");
    }
    if style.contains(Style::BOLD) {
        s = format!("**{s}**");
    }
    if style.contains(Style::STRIKE) {
        s = format!("~~{s}~~");
    }
    if style.contains(Style::UNDERLINE) {
        s = format!("<u>{s}</u>");
    }
    s
}

fn unwrap_inline(mut s: &str) -> (Style, String) {
    let mut style = Style::NONE;
    for (open, close, flag) in [("<u>", "</u>", Style::UNDERLINE), ("~~", "~~", Style::STRIKE), ("**", "**", Style::BOLD), ("*", "*", Style::ITALIC)] {
        if s.len() >= open.len() + close.len() && s.starts_with(open) && s.ends_with(close) {
            s = &s[open.len()..s.len() - close.len()];
            style = Style(style.0 | flag.0);
        }
    }
    (style, unescape(s))
}

fn image_id(line: &str) -> Option<i64> {
    line.strip_prefix("![](img:")?.strip_suffix(')')?.parse().ok()
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
            b.text = one_line(&b.text);
            if b.kind == Kind::Image {
                b.text.clear();
                b.style = Style::NONE;
            } else {
                b.image_id = None;
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
        if self.is_empty() && self.blocks[0].style == Style::NONE {
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
                    out.push_str(&format!("![](img:{})", b.image_id.unwrap_or(0)));
                    continue;
                }
            }
            out.push_str(&wrap(b.style, &b.text));
        }
        out
    }

    // ---- editing ----

    /// The user edited a block's text (no line breaks; see [`Document::insert_text`] for those).
    pub fn set_text(&mut self, block: usize, text: &str) {
        if let Some(b) = self.blocks.get_mut(block).filter(|b| b.is_text()) {
            b.text = one_line(text);
        }
    }

    /// Enter at `offset`: split the block. After a title or heading the new block is body; a list
    /// item continues the list, and Enter on an empty list item turns it into body.
    pub fn split(&mut self, block: usize, offset: usize) -> Caret {
        let Some(b) = self.blocks.get(block) else { return Caret { block: self.blocks.len() - 1, offset: 0 } };
        if b.kind == Kind::Image {
            self.blocks.insert(block + 1, Block::new(Kind::Body, ""));
            return Caret { block: block + 1, offset: 0 };
        }
        if b.kind == Kind::ListItem && b.text.is_empty() {
            self.blocks[block].kind = Kind::Body;
            self.blocks[block].style = Style::NONE;
            return Caret { block, offset: 0 };
        }
        let at = floor_boundary(&b.text, offset);
        let tail = self.blocks[block].text.split_off(at);
        let new = following(&self.blocks[block], tail);
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
        }
        Some(Caret { block, offset })
    }

    /// Toggle one style flag of a block. False when the block has no text (an image).
    pub fn toggle_style(&mut self, block: usize, flag: Style) -> bool {
        match self.blocks.get_mut(block) {
            Some(b) if b.is_text() => {
                b.style = b.style.toggled(flag);
                true
            }
            _ => false,
        }
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
    /// line after the first becomes a new block, as if Enter had been pressed there. Returns the
    /// caret after the inserted text.
    pub fn insert_text(&mut self, block: usize, start: usize, end: usize, text: &str) -> Caret {
        let Some(b) = self.blocks.get(block).filter(|b| b.is_text()) else {
            return Caret { block: block.min(self.blocks.len() - 1), offset: 0 };
        };
        let start = floor_boundary(&b.text, start);
        let end = floor_boundary(&b.text, end).max(start);
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut lines = normalized.split('\n');
        let first = lines.next().unwrap_or("");
        let tail = b.text[end..].to_string();
        let b = &mut self.blocks[block];
        b.text.truncate(start);
        b.text.push_str(first);
        let mut caret = Caret { block, offset: b.text.len() };
        for line in lines {
            let new = following(&self.blocks[caret.block], line.to_string());
            caret = Caret { block: caret.block + 1, offset: new.text.len() };
            self.blocks.insert(caret.block, new);
        }
        self.blocks[caret.block].text.push_str(&tail);
        caret
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

/// The block that follows `prev` when Enter is pressed in it, holding `text`.
fn following(prev: &Block, text: String) -> Block {
    match prev.kind {
        Kind::ListItem => Block { kind: Kind::ListItem, text, style: prev.style, image_id: None },
        Kind::Body => Block { kind: Kind::Body, text, style: prev.style, image_id: None },
        _ => Block { kind: Kind::Body, text, style: Style::NONE, image_id: None },
    }
}

fn floor_boundary(s: &str, mut i: usize) -> usize {
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
        let (style, text) = unwrap_inline(rest);
        return Block { kind: Kind::Title, text, style, image_id: None };
    }
    if let Some(id) = image_id(line) {
        return Block::image(id);
    }
    let (kind, rest) = if let Some(r) = line.strip_prefix("## ").or_else(|| line.strip_prefix("# ")) {
        (Kind::Heading, r)
    } else if let Some(r) = line.strip_prefix("- ") {
        (Kind::ListItem, r)
    } else {
        (Kind::Body, line)
    };
    let (style, text) = unwrap_inline(rest);
    Block { kind, text, style, image_id: None }
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

    // ---- the format ----

    #[test]
    fn the_mockup_fixture_is_title_heading_body() {
        let d = Document::parse(MOCKUP);
        assert_eq!(kinds(&d), [Kind::Title, Kind::Heading, Kind::Body]);
        assert_eq!(texts(&d), ["Endfield Note", "Thermite Cutter : POG & Akekuri", "Currently POG no Weapon"]);
        assert!(d.blocks()[1].style.contains(Style::BOLD) && d.blocks()[1].is_bold());
        assert_eq!(d.to_text(), MOCKUP);
    }

    #[test]
    fn every_kind_has_its_line() {
        let text = "# t\n## **h**\nb\n- item\n![](img:42)\n**bold** *it* ~~s~~";
        let d = Document::parse(text);
        assert_eq!(kinds(&d), [Kind::Title, Kind::Heading, Kind::Body, Kind::ListItem, Kind::Image, Kind::Body]);
        assert_eq!(d.blocks()[3].text, "item");
        assert_eq!(d.blocks()[4].image_id, Some(42));
        // the last line is a bold-italic-ish run of markers only at its ends: no flags, text kept
        assert_eq!(d.blocks()[5].text, "**bold** *it* ~~s~~");
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
        let mut styled = Document::new();
        styled.toggle_style(0, Style::BOLD);
        assert_eq!(Document::parse(&styled.to_text()), styled);
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
                    b.with_style(Style::from_bits(r.next(16) as u8))
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
        d.toggle_style(0, Style::BOLD);
        d.toggle_style(1, Style::BOLD);
        d.split(0, 2);
        assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Heading]);
        assert_eq!(d.blocks()[1].style, Style::NONE);
        d.split(2, 2);
        assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Heading, Kind::Body]);
        assert_eq!(d.blocks()[3].style, Style::NONE);
        // the middle of a title: the title keeps its head, the rest is body
        let mut e = doc(&[(Kind::Title, "abcd")]);
        e.split(0, 2);
        assert_eq!((kinds(&e), texts(&e)), (vec![Kind::Title, Kind::Body], vec!["ab", "cd"]));
    }

    #[test]
    fn enter_in_a_list_continues_it_and_on_an_empty_item_ends_it() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::ListItem, "milk")]);
        d.toggle_style(1, Style::ITALIC);
        d.split(1, 4);
        assert_eq!(kinds(&d), [Kind::Title, Kind::ListItem, Kind::ListItem]);
        assert_eq!(d.blocks()[2].style, Style::ITALIC, "the new block inherits the style");
        assert_eq!(d.split(2, 0), Caret { block: 2, offset: 0 });
        assert_eq!(kinds(&d), [Kind::Title, Kind::ListItem, Kind::Body]);
        assert_eq!(d.blocks()[2].style, Style::NONE);
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
    fn toggling_a_style_flips_one_flag_of_the_block() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "b")]);
        assert!(d.toggle_style(1, Style::BOLD));
        assert!(d.toggle_style(1, Style::UNDERLINE));
        assert!(d.blocks()[1].style.contains(Style::BOLD) && d.blocks()[1].style.contains(Style::UNDERLINE));
        assert!(d.toggle_style(1, Style::BOLD));
        assert_eq!(d.blocks()[1].style, Style::UNDERLINE);
        assert!(d.toggle_style(0, Style::ITALIC), "a title can be styled");
        assert!(!d.toggle_style(5, Style::BOLD));
        d.insert_image(1, 1);
        assert!(!d.toggle_style(2, Style::BOLD), "an image has no text to style");
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
    fn neighbors_skip_images() {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "b")]);
        d.insert_image(0, 1);
        assert_eq!(d.next_text_block(0), Some(2));
        assert_eq!(d.prev_text_block(2), Some(0));
        assert_eq!(d.prev_text_block(0), None);
        assert_eq!(d.next_text_block(2), None);
    }

    #[test]
    fn newlines_never_enter_block_text() {
        let mut d = Document::new();
        d.set_text(0, "a\nb");
        assert_eq!(d.blocks()[0].text, "a b");
        assert_eq!(Document::parse(&d.to_text()), d);
    }
}
