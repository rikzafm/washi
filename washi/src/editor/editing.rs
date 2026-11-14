//! Text editing and formatting: typing, Enter/Backspace/Delete, styles, ink and marker.

use super::*;

/// What [`Editor::format`] changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Formatted {
    /// Nothing in the note (at most the style the next typing takes).
    Unchanged,
    /// The style of some words.
    Restyled,
    /// Paragraphs' kinds (a check or list toggle): a check item's box indents its words, so they
    /// may have rewrapped and the caret moved out of view.
    Reindented,
}

impl Formatted {
    /// Whether the note changed (an undo step was made).
    pub fn changed(self) -> bool {
        self != Formatted::Unchanged
    }

    /// Whether the caret's lines may have moved, so it should be scrolled back into view.
    pub fn rewrapped(self) -> bool {
        self == Formatted::Reindented
    }
}

impl Editor {
    pub fn edit(&self, i: usize, text: &str, cursor: usize) -> Option<Caret> {
        if !text.contains(['\n', '\r']) {
            // only an edit that auto-formats is an undo step of its own, so only then is the note
            // as it was needed (a copy of it)
            let (old, before) = {
                let doc = self.doc();
                let old = doc.blocks().get(i).map(|b| b.text.clone()).unwrap_or_default();
                let before = doc.would_auto_format(i, &old, text, cursor).then(|| doc.clone());
                (old, before)
            };
            let mut doc = self.doc_mut();
            let typing = self.typing.take().filter(|(c, _)| c.block == i).map(|(_, s)| s);
            let paint = self.typing_paint.take().filter(|(c, _)| c.block == i).map(|(_, p)| p);
            doc.edit_text(i, text, cursor, typing, paint);
            let caret = doc.auto_format(i, &old, cursor);
            drop(doc);
            debug_assert_eq!(caret.is_some(), before.is_some(), "auto-format fires exactly when it was predicted");
            if let (Some(_), Some(before)) = (caret, before) {
                self.structural(before);
            }
            return caret;
        }
        let before = self.doc().clone();
        let mut doc = self.doc_mut();
        let old = match doc.blocks().get(i) {
            Some(b) if b.is_text() => b.text.clone(),
            _ => return None,
        };
        let mut cursor = cursor.min(text.len());
        while !text.is_char_boundary(cursor) {
            cursor -= 1;
        }
        let mut end = old.len() - (text.len() - cursor).min(old.len());
        while !old.is_char_boundary(end) {
            end += 1;
        }
        let mut start = old.bytes().zip(text.bytes()).take_while(|(a, b)| a == b).count().min(end).min(cursor);
        while !old.is_char_boundary(start) || !text.is_char_boundary(start) {
            start -= 1;
        }
        let caret = doc.insert_text(i, start, end, &text[start..cursor]);
        drop(doc);
        self.structural(before);
        Some(caret)
    }

    /// Enter, Backspace or Delete pressed in block `i` with its field's caret at `offset`: the caret
    /// after the edit, or None when the key is no edit of the note's.
    pub fn edit_key(&self, i: usize, key: &str, offset: usize) -> Option<Caret> {
        use slint::platform::Key;
        let pressed = caret_motion::decode_key(key).filter(|k| matches!(k, Key::Return | Key::Backspace | Key::Delete))?;
        let before = self.doc().clone();
        if before.is_image(i) {
            return self.image_key(i, key, before);
        }
        let carried = if self.typing.get().is_some_and(|(c, _)| c == (Caret { block: i, offset })) {
            self.typing.get().map(|(_, s)| s)
        } else {
            before.blocks().get(i).map(|b| b.style_for_insert(offset))
        };
        let mut doc = self.doc_mut();
        let edit = match pressed {
            Key::Return => {
                let c = doc.split(i, offset);
                self.typing.set(carried.filter(|&s| s != Flags::NONE).map(|s| (c, s)));
                Some(c)
            }
            Key::Backspace => doc.backspace(i, offset),
            _ => doc.delete(i, offset),
        };
        let caret = edit?;
        drop(doc);
        self.structural(before);
        Some(caret)
    }

    /// A press on a check box ticks or unticks its item (one undo step). False when the point is no box.
    pub fn toggle_check_at(&self, ui: &NoteWindow, at: (f32, f32)) -> bool {
        let Some(block) = self.layout(ui).check_at((at.0 as f64, at.1 as f64)) else { return false };
        let before = self.doc().clone();
        let done = self.doc_mut().toggle_checked(block);
        if done {
            self.structural(before);
        }
        done
    }

    pub fn typing_style(&self) -> Flags {
        let i = self.caret_block();
        let cursor = self.selection.get().focus.offset;
        match self.typing.get() {
            Some((c, style)) if c == (Caret { block: i, offset: cursor }) => style,
            _ => self.doc().blocks().get(i).map_or(Flags::NONE, |b| b.style_for_insert(cursor)),
        }
    }

    pub fn paint_at_caret(&self) -> Paint {
        let i = self.caret_block();
        let cursor = self.selection.get().focus.offset;
        match self.typing_paint.get() {
            Some((c, paint)) if c == (Caret { block: i, offset: cursor }) => paint,
            _ => self.doc().blocks().get(i).map_or(Paint::default(), |b| b.paint_for_insert(cursor)),
        }
    }

    /// Apply format `what` (a `FORMAT_*`) to the selection, or to the word or typing at the caret,
    /// and say what it changed.
    pub fn format(&self, what: i32) -> Formatted {
        let i = self.caret_block();
        let flag = match what {
            FORMAT_BOLD => Flags::BOLD,
            FORMAT_ITALIC => Flags::ITALIC,
            FORMAT_UNDERLINE => Flags::UNDERLINE,
            FORMAT_STRIKE => Flags::STRIKE,
            FORMAT_CHECK | FORMAT_LIST => {
                let before = self.doc().clone();
                let sel = self.selection.get();
                let done = if what == FORMAT_CHECK {
                    self.doc_mut().toggle_check_range(sel)
                } else {
                    self.doc_mut().toggle_list_range(sel)
                };
                if !done {
                    return Formatted::Unchanged;
                }
                self.structural(before);
                return Formatted::Reindented;
            }
            _ => return Formatted::Unchanged,
        };
        let sel = self.selection.get();
        let (cursor, collapsed) = (sel.focus.offset, sel.anchor == sel.focus);
        let before = self.doc().clone();
        let done = {
            let mut doc = self.doc_mut();
            if !collapsed {
                doc.toggle_style_range(sel, flag)
            } else if let Some((start, end)) = doc.word_at(i, cursor).filter(|&(start, end)| cursor > start && cursor < end) {
                doc.toggle_style(i, start, end, flag)
            } else {
                false
            }
        };
        if done {
            self.typing.set(None);
            self.structural(before);
        } else if collapsed {
            let style = self.typing_style().toggled(flag);
            self.typing.set(Some((Caret { block: i, offset: cursor }, style)));
        }
        if done { Formatted::Restyled } else { Formatted::Unchanged }
    }

    pub fn apply_paint(&self, ink: Option<u8>, marker: Option<u8>) -> bool {
        let sel = self.clamp(self.selection.get());
        if sel.anchor == sel.focus {
            let mut paint = self.paint_at_caret();
            paint.ink = clamp_palette(ink.unwrap_or(paint.ink));
            paint.marker = clamp_palette(marker.unwrap_or(paint.marker));
            self.typing_paint.set(Some((sel.focus, paint)));
            return false;
        }
        let before = self.doc().clone();
        let done = {
            let mut doc = self.doc_mut();
            let a = ink.is_some_and(|i| doc.set_ink_range(sel, i));
            let b = marker.is_some_and(|m| doc.set_marker_range(sel, m));
            a | b
        };
        if done {
            self.structural(before);
        }
        done
    }

    pub fn show_formats(&self, ui: &NoteWindow) {
        let style = {
            let doc = self.doc();
            let sel = self.selection.get();
            match doc.blocks().get(self.caret_block()) {
                Some(_) if sel.anchor != sel.focus => doc.common_style_range(sel),
                Some(_) => {
                    drop(doc);
                    self.typing_style()
                }
                None => Flags::NONE,
            }
        };
        let doc = self.doc();
        let b = doc.blocks().get(self.caret_block());
        let heading = b.is_some_and(|b| b.kind == Kind::Heading);
        ui.set_fmt_bold(heading || style.contains(Flags::BOLD));
        ui.set_fmt_italic(style.contains(Flags::ITALIC));
        ui.set_fmt_underline(style.contains(Flags::UNDERLINE));
        ui.set_fmt_strike(style.contains(Flags::STRIKE));
        ui.set_fmt_list(b.is_some_and(|b| b.kind == Kind::ListItem));
        ui.set_fmt_check(b.is_some_and(|b| b.kind.is_check()));
        self.fill_handles(&doc, &self.layout(ui));
        let sel = self.selection.get();
        let (ink, marker) = if b.is_none() {
            (Some(0), Some(0))
        } else if sel.anchor != sel.focus {
            (doc.common_ink_range(sel), doc.common_marker_range(sel))
        } else {
            drop(doc);
            let p = self.paint_at_caret();
            (Some(p.ink), Some(p.marker))
        };
        ui.set_fmt_ink(ink.map_or(-1, i32::from));
        ui.set_fmt_marker(marker.map_or(-1, i32::from));
    }
}
