//! Caret motion: where an arrow key takes the selection.
//!
//! A pure function of the document and the selection, with no window, field or database, so it is
//! tested through its interface alone (see ADR-0002). The editor applies the result through
//! `Editor::set_selection`.

use crate::document::{floor_boundary, Caret, Document, Kind, Range};

/// A horizontal motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Horizontal {
    Left,
    Right,
}

impl Horizontal {
    /// The motion a Slint key text stands for, if it is Left or Right.
    pub fn from_key(key: &str) -> Option<Horizontal> {
        use slint::platform::Key;
        if key == char::from(Key::LeftArrow).to_string() {
            Some(Horizontal::Left)
        } else if key == char::from(Key::RightArrow).to_string() {
            Some(Horizontal::Right)
        } else {
            None
        }
    }
}

/// Where Left or Right takes `sel`; `extend` (Shift) moves only the focus end and keeps the anchor.
///
/// Returns `None` when the motion is the text field's own to make: the caret has a character to
/// step over inside its paragraph and no range spans blocks. Otherwise the new range: a caret
/// crossing a paragraph's edge lands at the end of the previous paragraph or the start of the next
/// (an image block is entered at its only offset, 0), stays put at the note's ends, and a plain
/// key over a non-empty range collapses it to the edge the key points at.
pub fn horizontal(doc: &Document, sel: Range, motion: Horizontal, extend: bool) -> Option<Range> {
    if !extend && sel.anchor != sel.focus {
        let (start, end) = sel.ordered();
        let to = if motion == Horizontal::Left { start } else { end };
        return Some(Range { anchor: to, focus: to });
    }
    let across = sel.anchor.block != sel.focus.block;
    let focus = sel.focus;
    let text = doc.blocks().get(focus.block).map_or("", |b| b.text.as_str());
    let offset = floor_boundary(text, focus.offset);
    let to = match motion {
        Horizontal::Left if offset > 0 => {
            if !across {
                return None;
            }
            Caret { block: focus.block, offset: floor_boundary(text, offset - 1) }
        }
        Horizontal::Left if focus.block > 0 => boundary(doc, focus.block - 1, false),
        Horizontal::Right if offset < text.len() => {
            if !across {
                return None;
            }
            let step = text[offset..].chars().next().map_or(1, char::len_utf8);
            Caret { block: focus.block, offset: offset + step }
        }
        Horizontal::Right if focus.block + 1 < doc.len() => boundary(doc, focus.block + 1, true),
        _ => Caret { block: focus.block, offset },
    };
    Some(Range { anchor: if extend { sel.anchor } else { to }, focus: to })
}

/// Landing caret on a block entered from the side: its start, or its end for text entered from
/// the right; an image block has only offset 0.
fn boundary(doc: &Document, block: usize, at_start: bool) -> Caret {
    let offset = if at_start || doc.blocks().get(block).is_some_and(|b| b.kind == Kind::Image) {
        0
    } else {
        doc.blocks().get(block).map_or(0, |b| b.text.len())
    };
    Caret { block, offset }
}

/// A vertical motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vertical {
    Up,
    Down,
}

impl Vertical {
    /// The motion a Slint key text stands for, if it is Up or Down.
    pub fn from_key(key: &str) -> Option<Vertical> {
        use slint::platform::Key;
        if key == char::from(Key::UpArrow).to_string() {
            Some(Vertical::Up)
        } else if key == char::from(Key::DownArrow).to_string() {
            Some(Vertical::Down)
        } else {
            None
        }
    }
}

/// Where a caret sits in its paragraph's wrapped lines: the line (0 first), how many lines the
/// paragraph has, and the distance from the line's left edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineAt {
    pub line: usize,
    pub lines: usize,
    pub x: f64,
}

/// The line layout vertical motion needs, which only the editor can measure (fonts, paper width).
/// Kept behind a trait so the motion is tested with a fake layout.
pub trait Lines {
    /// Where the caret at `offset` of text block `block` is. With `line_end`, an offset that
    /// starts a wrapped line is the end of the line before it.
    fn line_at(&self, block: usize, offset: usize, line_end: bool) -> LineAt;
    /// The offset on `line` of text block `block` nearest to `x` (`usize::MAX` is the last line),
    /// and whether it is the end of that line where the next line starts at the same offset.
    fn offset_on(&self, block: usize, line: usize, x: f64) -> (usize, bool);
}

/// What an Up or Down key does: the new selection, whether its caret is at the end of a wrapped
/// line, and the goal column to keep for the next vertical move (None when the move leaves it
/// behind, as collapsing a range or crossing an image does).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moved {
    pub range: Range,
    pub line_end: bool,
    pub goal_x: Option<f64>,
}

/// Where Up or Down takes `sel`; `extend` (Shift) moves only the focus end and keeps the anchor.
///
/// One rule for both: a line up or down in the paragraph keeping the goal column (`goal_x`, else
/// the caret's own x); past the first or last line the caret enters the neighbouring block, an
/// image at its only offset (0) and text on its last or first line; at the note's ends Shift selects
/// to the start or end of the text and a plain key stays put. Leaving an image lands at the end of
/// the text before it or the start of the text after it. A plain key over a non-empty range collapses it to the edge it
/// points at. `line_end` is whether the focus is at the end of a wrapped line.
pub fn vertical(doc: &Document, lines: &dyn Lines, sel: Range, motion: Vertical, extend: bool, line_end: bool, goal_x: Option<f64>) -> Moved {
    let up = motion == Vertical::Up;
    let done = |to: Caret, line_end: bool, goal_x: Option<f64>| Moved {
        range: Range { anchor: if extend { sel.anchor } else { to }, focus: to },
        line_end,
        goal_x,
    };
    if !extend && sel.anchor != sel.focus {
        let (start, end) = sel.ordered();
        let to = if up { start } else { end };
        return done(to, false, None);
    }
    let focus = sel.focus;
    let kind_of = |b: usize| doc.blocks().get(b).map(|b| b.kind);
    if kind_of(focus.block) == Some(Kind::Image) {
        let to = if up && focus.block > 0 {
            boundary(doc, focus.block - 1, false)
        } else if !up && focus.block + 1 < doc.len() {
            boundary(doc, focus.block + 1, true)
        } else {
            focus
        };
        return done(to, false, None);
    }
    let text = doc.blocks().get(focus.block).map_or("", |b| b.text.as_str());
    let offset = floor_boundary(text, focus.offset);
    let at = lines.line_at(focus.block, offset, line_end);
    let x = goal_x.unwrap_or(at.x);
    let land = |block: usize, line: usize| {
        let (offset, line_end) = lines.offset_on(block, line, x);
        done(Caret { block, offset }, line_end, Some(x))
    };
    if up {
        if at.line > 0 {
            land(focus.block, at.line - 1)
        } else if focus.block == 0 {
            // the note's start: Shift selects to it, a plain key stays where it is
            if extend { done(Caret { block: 0, offset: 0 }, false, Some(x)) } else { done(Caret { block: 0, offset }, line_end, goal_x) }
        } else if kind_of(focus.block - 1) == Some(Kind::Image) {
            done(boundary(doc, focus.block - 1, true), false, None)
        } else {
            land(focus.block - 1, usize::MAX)
        }
    } else if at.line + 1 < at.lines {
        land(focus.block, at.line + 1)
    } else if focus.block + 1 >= doc.len() {
        // the note's end: Shift selects to it, a plain key stays where it is
        if extend { done(Caret { block: focus.block, offset: text.len() }, false, Some(x)) } else { done(Caret { block: focus.block, offset }, line_end, goal_x) }
    } else if kind_of(focus.block + 1) == Some(Kind::Image) {
        done(boundary(doc, focus.block + 1, true), false, None)
    } else {
        land(focus.block + 1, 0)
    }
}

/// Where Home or End takes the caret: the line's start or end (Home, End), or the note's
/// (Ctrl+Home, Ctrl+End).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    LineStart,
    LineEnd,
    NoteStart,
    NoteEnd,
}

impl Edge {
    /// The edge a Slint key text stands for, if it is Home or End; `ctrl` takes it to the note's ends.
    pub fn from_key(key: &str, ctrl: bool) -> Option<Edge> {
        use slint::platform::Key;
        if key == char::from(Key::Home).to_string() {
            Some(if ctrl { Edge::NoteStart } else { Edge::LineStart })
        } else if key == char::from(Key::End).to_string() {
            Some(if ctrl { Edge::NoteEnd } else { Edge::LineEnd })
        } else {
            None
        }
    }
}

/// Where Home, End, Ctrl+Home or Ctrl+End takes `sel`'s focus; `extend` (Shift) keeps the anchor.
///
/// A line edge is that of the wrapped line the focus is on (`line_end`: whether it is at the end of
/// one); an image has none, so the key leaves it where it is. A plain key over a range collapses
/// it to the new focus. The goal column is forgotten.
pub fn to_edge(doc: &Document, lines: &dyn Lines, sel: Range, edge: Edge, extend: bool, line_end: bool) -> Moved {
    let focus = sel.focus;
    let (to, at_line_end) = match edge {
        Edge::NoteStart => (Caret { block: 0, offset: 0 }, false),
        Edge::NoteEnd => {
            let block = doc.len().saturating_sub(1);
            (Caret { block, offset: doc.blocks().get(block).map_or(0, |b| b.text.len()) }, false)
        }
        Edge::LineStart | Edge::LineEnd => match doc.blocks().get(focus.block) {
            Some(b) if b.kind != Kind::Image => {
                let offset = floor_boundary(&b.text, focus.offset);
                let at = lines.line_at(focus.block, offset, line_end);
                // far past the end of any line is the end of the line it is on
                let x = if edge == Edge::LineStart { 0.0 } else { 1e9 };
                let (offset, at_end) = lines.offset_on(focus.block, at.line, x);
                (Caret { block: focus.block, offset }, at_end)
            }
            _ => (focus, false),
        },
    };
    Moved { range: Range { anchor: if extend { sel.anchor } else { to }, focus: to }, line_end: at_line_end, goal_x: None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Block;
    use Horizontal::{Left, Right};

    fn c(block: usize, offset: usize) -> Caret {
        Caret { block, offset }
    }

    fn caret(block: usize, offset: usize) -> Range {
        Range { anchor: c(block, offset), focus: c(block, offset) }
    }

    fn range(a: (usize, usize), f: (usize, usize)) -> Range {
        Range { anchor: c(a.0, a.1), focus: c(f.0, f.1) }
    }

    /// Title(5) / "Paragraph 1"(11) / image / "Last"(4) / "éa"(3 bytes)
    fn doc() -> Document {
        Document::from_blocks(vec![
            Block::new(Kind::Title, "Title"),
            Block::new(Kind::Body, "Paragraph 1"),
            Block::image(1),
            Block::new(Kind::Body, "Last"),
            Block::new(Kind::Body, "\u{e9}a"),
        ])
    }

    /// (name, selection, motion, extend, expected)
    #[allow(clippy::type_complexity)]
    fn table() -> Vec<(&'static str, Range, Horizontal, bool, Option<Range>)> {
        vec![
            ("left mid-text is the field's", caret(1, 5), Left, false, None),
            ("right mid-text is the field's", caret(1, 5), Right, false, None),
            ("shift+left mid-text is the field's", caret(1, 5), Left, true, None),
            ("shift+right inside one block is the field's", range((1, 2), (1, 5)), Right, true, None),
            ("left at paragraph start goes to the end of the one before", caret(1, 0), Left, false, Some(caret(0, 5))),
            ("right at paragraph end goes to the start of the next", caret(0, 5), Right, false, Some(caret(1, 0))),
            ("left at the note's start stays", caret(0, 0), Left, false, Some(caret(0, 0))),
            ("right at the note's end stays", caret(4, 3), Right, false, Some(caret(4, 3))),
            ("right at the end of the paragraph before an image enters it", caret(1, 11), Right, false, Some(caret(2, 0))),
            ("left at the start of the paragraph after an image enters it", caret(3, 0), Left, false, Some(caret(2, 0))),
            ("left off an image lands at the end of the text before", caret(2, 0), Left, false, Some(caret(1, 11))),
            ("right off an image lands at the start of the text after", caret(2, 0), Right, false, Some(caret(3, 0))),
            ("shift+left at paragraph start extends over the edge", caret(1, 0), Left, true, Some(range((1, 0), (0, 5)))),
            ("shift+right at paragraph end extends over the edge", caret(0, 5), Right, true, Some(range((0, 5), (1, 0)))),
            ("shift+right onto an image extends to it", caret(1, 11), Right, true, Some(range((1, 11), (2, 0)))),
            ("shift+left at the note's start stays", range((1, 3), (0, 0)), Left, true, Some(range((1, 3), (0, 0)))),
            ("shift+right at the note's end stays", range((0, 0), (4, 3)), Right, true, Some(range((0, 0), (4, 3)))),
            ("shift+left over a spanning range steps one char", range((3, 2), (1, 5)), Left, true, Some(range((3, 2), (1, 4)))),
            ("shift+right over a spanning range steps one char", range((3, 2), (1, 5)), Right, true, Some(range((3, 2), (1, 6)))),
            ("shift+right steps a whole multi-byte char", range((0, 0), (4, 0)), Right, true, Some(range((0, 0), (4, 2)))),
            ("shift+left steps a whole multi-byte char", range((0, 0), (4, 2)), Left, true, Some(range((0, 0), (4, 0)))),
            ("shift+left shrinks a spanning range back into one block", range((0, 5), (1, 0)), Left, true, Some(range((0, 5), (0, 5)))),
            ("shift+left from an image extends to the text before", range((2, 0), (2, 0)), Left, true, Some(range((2, 0), (1, 11)))),
            ("plain left over a spanning range collapses to its start", range((3, 2), (1, 5)), Left, false, Some(caret(1, 5))),
            ("plain right over a spanning range collapses to its end", range((3, 2), (1, 5)), Right, false, Some(caret(3, 2))),
            ("plain left over a backwards range collapses to its start", range((1, 5), (3, 2)), Left, false, Some(caret(1, 5))),
        ]
    }

    #[test]
    fn left_and_right_move_the_selection_as_the_table_says() {
        let doc = doc();
        for (name, sel, motion, extend, expected) in table() {
            assert_eq!(horizontal(&doc, sel, motion, extend), expected, "{name}");
        }
    }

    #[test]
    fn keys_map_to_motions() {
        use slint::platform::Key;
        assert_eq!(Horizontal::from_key(&char::from(Key::LeftArrow).to_string()), Some(Left));
        assert_eq!(Horizontal::from_key(&char::from(Key::RightArrow).to_string()), Some(Right));
        assert_eq!(Horizontal::from_key(&char::from(Key::UpArrow).to_string()), None);
    }

    /// Text wrapped four bytes to a line, each byte 10 px wide.
    struct Fake<'a>(&'a Document);

    impl Lines for Fake<'_> {
        fn line_at(&self, block: usize, offset: usize, _line_end: bool) -> LineAt {
            let len = self.0.blocks()[block].text.len();
            LineAt { line: (offset / 4).min(len.saturating_sub(1) / 4), lines: len.div_ceil(4).max(1), x: (offset % 4) as f64 * 10.0 }
        }

        fn offset_on(&self, block: usize, line: usize, x: f64) -> (usize, bool) {
            let len = self.0.blocks()[block].text.len();
            let line = line.min(len.saturating_sub(1) / 4);
            let start = line * 4;
            ((start + (x / 10.0).round() as usize).min((start + 4).min(len)), false)
        }
    }

    /// "abcdefgh"(2 lines) / "xy" / image / "ABCDEFGH"(2 lines) / "tail"
    fn tall() -> Document {
        Document::from_blocks(vec![
            Block::new(Kind::Body, "abcdefgh"),
            Block::new(Kind::Body, "xy"),
            Block::image(1),
            Block::new(Kind::Body, "ABCDEFGH"),
            Block::new(Kind::Body, "tail"),
        ])
    }

    fn go(sel: Range, motion: Vertical, extend: bool, goal: Option<f64>) -> Moved {
        let doc = tall();
        vertical(&doc, &Fake(&doc), sel, motion, extend, false, goal)
    }

    /// (name, caret, motion, where the focus lands). Each row is run with and without Shift.
    #[allow(clippy::type_complexity)]
    fn vertical_table() -> Vec<(&'static str, (usize, usize), Vertical, (usize, usize))> {
        use Vertical::{Down, Up};
        vec![
            ("down a line in the paragraph keeps the column", (0, 2), Down, (0, 6)),
            ("up a line in the paragraph keeps the column", (0, 6), Up, (0, 2)),
            ("down off the last line enters the next paragraph at the column", (0, 6), Down, (1, 2)),
            ("up from the first line lands on the last line before", (1, 1), Up, (0, 5)),
            ("down onto an image enters it", (1, 1), Down, (2, 0)),
            ("up onto an image enters it", (3, 2), Up, (2, 0)),
            ("down off an image lands at the start of the text after", (2, 0), Down, (3, 0)),
            ("up off an image lands at the end of the text before", (2, 0), Up, (1, 2)),
        ]
    }

    #[test]
    fn up_and_down_land_where_the_table_says_with_and_without_shift() {
        for (name, from, motion, to) in vertical_table() {
            let sel = caret(from.0, from.1);
            let plain = go(sel, motion, false, None);
            let shifted = go(sel, motion, true, None);
            assert_eq!(plain.range, caret(to.0, to.1), "{name}");
            assert_eq!(shifted.range, range(from, to), "{name}: shift keeps the anchor");
            assert_eq!((plain.line_end, plain.goal_x), (shifted.line_end, shifted.goal_x), "{name}");
        }
    }

    #[test]
    fn shift_up_from_the_paragraph_after_an_image_enters_the_image_like_plain_up() {
        // it used to skip the image and land in the paragraph before it
        assert_eq!(go(caret(3, 2), Vertical::Up, true, None).range, range((3, 2), (2, 0)));
        assert_eq!(go(caret(1, 1), Vertical::Down, true, None).range, range((1, 1), (2, 0)));
    }

    #[test]
    fn the_goal_column_survives_a_short_paragraph_with_and_without_shift() {
        let doc = tall();
        let lines = Fake(&doc);
        for extend in [false, true] {
            let first = vertical(&doc, &lines, caret(0, 7), Vertical::Down, extend, false, None);
            assert_eq!(first.range.focus, c(1, 2), "extend={extend}: clamped to the short paragraph");
            assert_eq!(first.goal_x, Some(30.0));
            let second = vertical(&doc, &lines, first.range, Vertical::Up, extend, first.line_end, first.goal_x);
            assert_eq!(second.range.focus, c(0, 7), "extend={extend}: the column is back");
        }
    }

    #[test]
    fn at_the_note_s_ends_shift_selects_to_them_and_a_plain_key_stays() {
        assert_eq!(go(caret(0, 2), Vertical::Up, true, None).range, range((0, 2), (0, 0)));
        assert_eq!(go(caret(4, 1), Vertical::Down, true, None).range, range((4, 1), (4, 4)));
        assert_eq!(go(caret(0, 2), Vertical::Up, false, None).range, caret(0, 2));
        assert_eq!(go(caret(4, 1), Vertical::Down, false, None).range, caret(4, 1));
    }

    #[test]
    fn plain_up_and_down_collapse_a_range_to_the_edge_they_point_at() {
        let sel = range((3, 2), (1, 1));
        assert_eq!(go(sel, Vertical::Up, false, None).range, caret(1, 1));
        assert_eq!(go(sel, Vertical::Down, false, None).range, caret(3, 2));
    }

    #[test]
    fn keys_map_to_vertical_motions() {
        use slint::platform::Key;
        assert_eq!(Vertical::from_key(&char::from(Key::UpArrow).to_string()), Some(Vertical::Up));
        assert_eq!(Vertical::from_key(&char::from(Key::DownArrow).to_string()), Some(Vertical::Down));
        assert_eq!(Vertical::from_key(&char::from(Key::LeftArrow).to_string()), None);
    }

    /// (name, caret, edge, where the focus lands). Each row is run with and without Shift.
    fn edge_table() -> Vec<(&'static str, (usize, usize), Edge, (usize, usize))> {
        use Edge::{LineEnd, LineStart, NoteEnd, NoteStart};
        vec![
            ("home goes to the start of the line", (0, 6), LineStart, (0, 4)),
            ("home on the first line goes to the paragraph's start", (0, 2), LineStart, (0, 0)),
            ("end goes to the end of the line", (0, 1), LineEnd, (0, 4)),
            ("end on the last line goes to the paragraph's end", (0, 5), LineEnd, (0, 8)),
            ("ctrl+home goes to the note's start", (3, 5), NoteStart, (0, 0)),
            ("ctrl+end goes to the note's end", (1, 1), NoteEnd, (4, 4)),
        ]
    }

    #[test]
    fn home_end_and_ctrl_home_end_land_where_the_table_says_with_and_without_shift() {
        let doc = tall();
        for (name, from, edge, to) in edge_table() {
            let sel = caret(from.0, from.1);
            let plain = to_edge(&doc, &Fake(&doc), sel, edge, false, false);
            let shifted = to_edge(&doc, &Fake(&doc), sel, edge, true, false);
            assert_eq!(plain.range, caret(to.0, to.1), "{name}");
            assert_eq!(shifted.range, range(from, to), "{name}: shift keeps the anchor");
            assert_eq!(plain.goal_x, None, "{name}: forgets the goal column");
        }
    }

    #[test]
    fn plain_home_and_end_over_a_range_collapse_to_the_line_edge_of_the_focus() {
        let doc = tall();
        let sel = range((3, 2), (0, 6));
        assert_eq!(to_edge(&doc, &Fake(&doc), sel, Edge::LineStart, false, false).range, caret(0, 4));
        assert_eq!(to_edge(&doc, &Fake(&doc), sel, Edge::LineEnd, false, false).range, caret(0, 8));
    }

    #[test]
    fn home_and_end_on_an_image_stay_put() {
        let doc = tall();
        assert_eq!(to_edge(&doc, &Fake(&doc), caret(2, 0), Edge::LineStart, false, false).range, caret(2, 0));
        assert_eq!(to_edge(&doc, &Fake(&doc), caret(2, 0), Edge::LineEnd, true, false).range, caret(2, 0));
    }

    #[test]
    fn keys_map_to_edges() {
        use slint::platform::Key;
        let home = char::from(Key::Home).to_string();
        let end = char::from(Key::End).to_string();
        assert_eq!(Edge::from_key(&home, false), Some(Edge::LineStart));
        assert_eq!(Edge::from_key(&end, false), Some(Edge::LineEnd));
        assert_eq!(Edge::from_key(&home, true), Some(Edge::NoteStart));
        assert_eq!(Edge::from_key(&end, true), Some(Edge::NoteEnd));
        assert_eq!(Edge::from_key("a", false), None);
    }
}
