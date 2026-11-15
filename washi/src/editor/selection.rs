//! Selection (ADR 0002): the note-level wash, caret motion, hit testing, focus and reveal.

use super::*;

impl Editor {
    pub fn selection(&self) -> Range {
        self.selection.get()
    }

    /// Something is selected: a range of text, or a picture.
    pub fn has_selection(&self) -> bool {
        let r = self.selection.get();
        r.anchor != r.focus || self.selected_image(&self.doc()).is_some()
    }

    /// Text now stays tilted while editing (true) or settles flat (false).
    pub fn set_tilted(&self, ui: &NoteWindow, on: bool) {
        self.tilted.set(on);
        ui.set_tilted_editing(on);
        if let Ok(doc) = self.doc.try_borrow() {
            self.fill_sel_rects(&doc, &self.layout(ui));
        }
    }

    /// The letters now stand `em` apart (Settings › Font & Style, already saved): the lines break
    /// again, in the words and in the edit fields alike.
    pub fn set_letter_spacing(&self, ui: &NoteWindow, em: f32) {
        ui.global::<Theme>().set_letter_spacing_em(em);
        self.relayout(ui);
    }

    /// The active font changed (`fonts::set_current`, already done): put its families and numbers on
    /// the window's `Theme` and lay the note out again, in the words and in the edit fields alike.
    pub fn set_font(&self, ui: &NoteWindow) {
        crate::fonts::current().apply(&ui.global::<Theme>());
        self.relayout(ui);
    }

    /// The selection the note washes itself: one across blocks always, and with tilted editing one
    /// inside a block too (the field's level wash is off then).
    fn washed(&self) -> Option<Range> {
        self.range.get().or_else(|| {
            let s = self.selection.get();
            (self.tilted.get() && s.anchor != s.focus).then_some(s)
        })
    }

    pub fn has_range(&self) -> bool {
        self.range.get().is_some()
    }

    /// Make `range` the selection, wash it and focus its field: the entry point for a selection
    /// change that has a window. Caret motions that carry what the caret holds (the side of a wrap, the
    /// goal column) and the pointer go through [`Self::select_with`], the same code with that
    /// state added.
    pub fn set_selection(&self, ui: &NoteWindow, range: Range) {
        self.select_with(ui, range, caret_motion::Held::default());
    }

    /// [`Self::set_selection`] with what the new caret holds: with `held.line_end` (a caret only),
    /// the caret sits at the end of a line whose next line starts at the same offset; `held.goal_x`
    /// is the column the next vertical move keeps.
    pub(super) fn select_with(&self, ui: &NoteWindow, range: Range, held: caret_motion::Held) {
        let range = self.clamp(range);
        self.line_end.set((held.line_end && range.anchor == range.focus).then_some(range.focus));
        self.goal_x.set(None);
        let (start, end) = range.ordered();
        let across = start.block != end.block;
        self.store(range);
        ui.set_wash(!across);
        if across {
            self.range.set(Some(range));
            self.fill_sel_rects(&self.doc(), &self.layout(ui));
            self.focus_with(ui, range.focus, None);
        } else {
            self.range.set(None);
            // empty unless tilted editing washes a selection inside this block
            self.fill_sel_rects(&self.doc(), &self.layout(ui));
            self.focus_with(ui, range.focus, Some(range.anchor.offset));
        }
        self.typing.set(None);
        self.typing_paint.set(None);
        self.show_formats(ui);
        self.show_caret(ui);
        // after the field has reported its own caret, which forgets a goal column
        self.goal_x.set(held.goal_x.map(|x| (range.focus, x)));
    }

    pub fn select_all(&self, ui: &NoteWindow) {
        let all = self.doc().all();
        self.set_selection(ui, all);
    }

    pub fn end_range(&self, ui: &NoteWindow) {
        self.range.set(None);
        self.sel_rects.set_vec(vec![]);
        self.marker_wash.set_vec(vec![]);
        ui.set_wash(true);
    }

    pub fn clamp(&self, range: Range) -> Range {
        let doc = self.doc();
        let fix = |c: Caret| {
            let block = c.block.min(doc.len().saturating_sub(1));
            let text = doc.blocks().get(block).map_or("", |b| b.text.as_str());
            Caret { block, offset: floor_boundary(text, c.offset) }
        };
        Range { anchor: fix(range.anchor), focus: fix(range.focus) }
    }

    pub fn fill_sel_rects(&self, doc: &Document, layout: &Layout) {
        let rects = match self.washed() {
            Some(range) if !doc.is_empty() => layout.selection_rects(range),
            _ => vec![],
        };
        let rect = |r: &textflow::Rect| SelRect { x: r.x as f32, y: r.y as f32, width: r.width as f32, height: r.height as f32 };
        self.sel_rects.set_vec(rects.iter().map(rect).collect::<Vec<_>>());
        self.marker_wash.set_vec(textflow::marker_wash(&self.markers.borrow(), &rects).iter().map(rect).collect::<Vec<_>>());
        self.fill_handles(doc, layout);
    }

    /// What the caret at `focus` held after the last move, for the next one.
    fn held_at(&self, focus: Caret) -> caret_motion::Held {
        caret_motion::Held {
            line_end: self.line_end.get() == Some(focus),
            goal_x: self.goal_x.get().filter(|(c, _)| *c == focus).map(|(_, x)| x),
        }
    }

    /// A key forwarded from block `i`, its field's caret at `offset`, with `mods` held. True when it
    /// is a motion the note made (it moved the caret or the selection, and the field does nothing).
    /// False when the note leaves the key to the field: it is no motion (typing, Enter and the
    /// edits), the field moves itself (a step inside its paragraph, Ctrl with an arrow), or the key
    /// goes nowhere (see `caret_motion`).
    pub(super) fn try_move(&self, ui: &NoteWindow, key: &str, mods: &KeyMods, i: usize, offset: usize) -> bool {
        use caret_motion::{Edge, Horizontal, Moved, Vertical};
        use slint::platform::Key;
        if !matches!(
            caret_motion::decode_key(key),
            Some(Key::LeftArrow | Key::RightArrow | Key::UpArrow | Key::DownArrow | Key::Home | Key::End)
        ) {
            return false;
        }
        let (shift, ctrl) = (mods.shift, mods.control);
        let here = Caret { block: i, offset };
        let held = self.selection.get();
        // the range the note holds, or else the caret the field reports
        let sel = if shift || held.anchor != held.focus { held } else { Range { anchor: here, focus: here } };
        let moved = {
            let doc = self.doc();
            let layout = self.layout(ui);
            let lines = NoteLines { layout: &layout };
            let held = self.held_at(sel.focus);
            if let Some(edge) = Edge::from_key(key, ctrl) {
                caret_motion::to_edge(&doc, &lines, sel, edge, shift, held.line_end)
            } else if ctrl {
                None
            } else if let Some(m) = Vertical::from_key(key) {
                caret_motion::vertical(&doc, &lines, sel, m, shift, held)
            } else if let Some(m) = Horizontal::from_key(key) {
                caret_motion::horizontal(&doc, sel, m, shift).map(|range| Moved { range, held: caret_motion::Held::default() })
            } else {
                None
            }
        };
        let Some(Moved { range, held }) = moved else { return false };
        self.select_with(ui, range, held);
        true
    }

    /// A key over a selection across blocks that is not a motion (those go through `navigate`):
    /// (1, None) a modifier key alone, (2, caret) the key replaced the selection, (0, None) any other.
    pub fn range_key(&self, key: &str) -> (i32, Option<Caret>) {
        use slint::platform::Key;
        let mut chars = key.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else { return (0, None) };
        if ('\u{10}'..='\u{18}').contains(&c) {
            return (1, None);
        }
        let is = |k: Key| c == char::from(k);
        let text = if is(Key::Backspace) || is(Key::Delete) {
            String::new()
        } else if is(Key::Return) {
            "\n".to_string()
        } else if !c.is_control() && !('\u{f700}'..='\u{f8ff}').contains(&c) {
            c.to_string()
        } else {
            return (0, None);
        };
        (2, Some(self.replace_range(&text)))
    }

    pub fn replace_range(&self, text: &str) -> Caret {
        let range = self.clamp(self.selection.get());
        let before = self.doc().clone();
        let caret = self.doc_mut().replace_range(range, text);
        self.typing.set(None);
        self.typing_paint.set(None);
        self.structural(before);
        caret
    }

    pub fn range_text(&self) -> String {
        self.doc().plain_range(self.clamp(self.selection.get()))
    }

    pub fn hit(&self, ui: &NoteWindow, at: (f32, f32)) -> Caret {
        self.hit_on_line(ui, at).0
    }

    /// [`Self::hit`], and whether the caret is at the end of the line under the point where the
    /// next line starts at the same offset.
    pub fn hit_on_line(&self, ui: &NoteWindow, at: (f32, f32)) -> (Caret, bool) {
        self.layout(ui).hit_on_line((at.0 as f64, at.1 as f64))
    }

    pub fn unit_at(&self, at: Caret, clicks: u8) -> Range {
        let doc = self.doc();
        let Some(b) = doc.blocks().get(at.block) else { return Range { anchor: at, focus: at } };
        let (start, end) = if clicks >= 3 {
            (0, b.text.len())
        } else {
            doc.word_at(at.block, at.offset).unwrap_or((at.offset, at.offset))
        };
        Range { anchor: Caret { block: at.block, offset: start }, focus: Caret { block: at.block, offset: end } }
    }

    pub fn caret_block(&self) -> usize {
        self.selection.get().focus.block.min(self.doc().len().saturating_sub(1))
    }

    pub fn set_caret(&self, c: Caret) {
        self.set_range(Range { anchor: c, focus: c });
    }

    /// Make `range` the selection without touching a window: the one way to select when there is
    /// none. With a window, `set_selection` also washes and focuses.
    pub fn set_range(&self, range: Range) {
        self.line_end.set(None);
        self.goal_x.set(None);
        self.store(range);
    }

    /// The only assignment of the selection: `set_selection` (with a window), `set_range` (without
    /// one) and `caret_moved` (the field's own report) all end here. The last two skip the wash and
    /// the focus on purpose, so they cannot be `set_selection` (see ADR-0002).
    fn store(&self, range: Range) {
        self.selection.set(range);
    }

    pub fn caret_moved(&self, ui: &NoteWindow, block: usize, offset: usize, anchor: usize) {
        let now = self.selection.get();
        let (start, end) = now.ordered();
        if start.block != end.block && now.focus == (Caret { block, offset }) && anchor == offset {
            return;
        }
        let caret = Caret { block, offset };
        self.store(Range { anchor: Caret { block, offset: anchor }, focus: caret });
        // the caret is where the note put it: it keeps its side of a wrap and its column
        if anchor != offset || self.line_end.get().is_some_and(|c| c != caret) {
            self.line_end.set(None);
        }
        if anchor != offset || self.goal_x.get().is_some_and(|(c, _)| c != caret) {
            self.goal_x.set(None);
        }
        if self.typing.get().is_some_and(|(c, _)| c != (Caret { block, offset }) || offset != anchor) {
            self.typing.set(None);
        }
        if self.typing_paint.get().is_some_and(|(c, _)| c != (Caret { block, offset }) || offset != anchor) {
            self.typing_paint.set(None);
        }
        if self.range.get().is_some() {
            self.end_range(ui);
        }
        // a selection the field made (Shift+arrows) is washed by the note too when tilted
        if self.tilted.get() {
            if let Ok(doc) = self.doc.try_borrow() {
                self.fill_sel_rects(&doc, &self.layout(ui));
            }
        }
        self.show_caret(ui);
    }

    pub fn focus_at(&self, ui: &NoteWindow, c: Caret) {
        self.focus_with(ui, c, None);
    }

    pub fn focus_with(&self, ui: &NoteWindow, c: Caret, anchor: Option<usize>) {
        ui.set_focus_block(c.block as i32);
        ui.set_focus_anchor(anchor.map_or(-1, |a| a as i32));
        ui.set_focus_offset(c.offset as i32);
        ui.set_focus_serial(ui.get_focus_serial() + 1);
        self.reveal(ui, c.block, c.offset);
    }

    pub fn focus_start(&self, ui: &NoteWindow) {
        self.set_caret(START);
        self.focus_at(ui, START);
    }

    /// [`Self::reveal`] the caret (the selection's moving end) where it is now.
    pub fn reveal_caret(&self, ui: &NoteWindow) {
        let c = self.selection.get().focus;
        self.reveal(ui, c.block, c.offset);
    }

    pub fn reveal(&self, ui: &NoteWindow, block: usize, offset: usize) {
        let layout = self.layout(ui);
        let tokens = layout.tokens();
        let line_end = self.line_end.get() == Some(Caret { block, offset });
        let span = match layout.caret_box(block, offset, line_end) {
            Some((_, top, bottom)) => Some((top, bottom)),
            None => layout.caret_span(block, offset),
        };
        let Some((top, bottom)) = span else { return };
        let content = layout.content_height();
        let view = ui.get_text_view_height() as f64;
        let cur = ui.get_scroll_y() as f64;
        let mut to = if top <= tokens.inset_top + 0.5 { 0.0 } else if top < cur { top } else if bottom > cur + view { bottom - view } else { cur };
        to = to.min((content - view).max(0.0)).max(0.0);
        if (to - cur).abs() > 0.01 {
            ui.set_scroll_y(to as f32);
        }
    }
}
