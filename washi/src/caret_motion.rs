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
}
