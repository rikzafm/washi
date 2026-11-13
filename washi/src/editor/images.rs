//! Pictures in the note: selection, resize grips, add, resize, align, delete, copy and cut.

use super::*;

impl Editor {
    pub fn selected_image(&self, doc: &Document) -> Option<usize> {
        let r = self.selection.get();
        (r.anchor == r.focus && doc.is_image(r.focus.block)).then_some(r.focus.block)
    }

    pub fn grip_rect(b: &Block, r: textflow::Rect) -> textflow::Rect {
        let x = if b.align == Align::Right { r.x } else { r.x + r.width - GRIP };
        textflow::Rect { x, y: r.y + r.height - GRIP, width: GRIP, height: GRIP }
    }

    pub fn fill_handles(&self, doc: &Document, layout: &Layout) {
        let rows = self
            .selected_image(doc)
            .and_then(|i| {
                let r = layout.image_rect(i)?;
                Some([r, Self::grip_rect(&doc.blocks()[i], r)])
            })
            .map(|rs| rs.map(|r| SelRect { x: r.x as f32, y: r.y as f32, width: r.width as f32, height: r.height as f32 }).to_vec())
            .unwrap_or_default();
        self.handles.set_vec(rows);
    }

    pub fn grab_grip(&self, ui: &NoteWindow, at: (f32, f32)) -> bool {
        if !self.grip_at(ui, at) {
            return false;
        }
        let doc = self.doc();
        let Some(i) = self.selected_image(&doc) else { return false };
        *self.grip.borrow_mut() = Some((i, doc.clone()));
        true
    }

    pub fn grip_drag(&self, ui: &NoteWindow, at: (f32, f32)) -> Option<bool> {
        let block = self.grip.borrow().as_ref()?.0;
        let t = self.tokens(ui);
        let avail = self.paper.get().0 - t.inset_left - t.inset_right;
        let x = at.0 as f64;
        let mut doc = self.doc_mut();
        let w = match doc.blocks().get(block)?.align {
            Align::Left => x - t.inset_left,
            Align::Center => 2.0 * (x - (t.inset_left + avail / 2.0)),
            Align::Right => t.inset_left + avail - x,
        };
        let pct = (w / avail * 100.0).round().clamp(IMAGE_PCT_MIN as f64, IMAGE_PCT_MAX as f64) as u8;
        Some(doc.set_image_pct(block, pct))
    }

    pub fn release_grip(&self) -> bool {
        let Some((_, before)) = self.grip.borrow_mut().take() else { return false };
        if *self.doc() != before {
            self.structural(before);
        }
        true
    }

    pub fn image_at(&self, ui: &NoteWindow, at: (f32, f32)) -> Option<usize> {
        let block = self.hit(ui, at).block;
        let r = self.layout(ui).image_rect(block)?;
        let (x, y) = (at.0 as f64, at.1 as f64);
        (x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height).then_some(block)
    }

    pub fn grip_at(&self, ui: &NoteWindow, at: (f32, f32)) -> bool {
        let doc = self.doc();
        let Some(i) = self.selected_image(&doc) else { return false };
        let Some(r) = self.layout(ui).image_rect(i) else { return false };
        let g = Self::grip_rect(&doc.blocks()[i], r);
        let slop = 6.0;
        let (x, y) = (at.0 as f64, at.1 as f64);
        !(x < g.x - slop || x > g.x + g.width + slop || y < g.y - slop || y > g.y + g.height + slop)
    }

    pub fn cursor_kind_at(&self, ui: &NoteWindow, at: (f32, f32)) -> i32 {
        if self.grip_at(ui, at) {
            return CURSOR_RESIZE;
        }
        if self.image_at(ui, at).is_some() {
            return CURSOR_DEFAULT;
        }
        if self.layout(ui).check_at((at.0 as f64, at.1 as f64)).is_some() {
            return CURSOR_POINTER;
        }
        CURSOR_TEXT
    }

    pub fn image_key(&self, i: usize, key: &str, before: Document) -> Option<Caret> {
        use slint::platform::Key;
        if matches!(caret_motion::decode_key(key), Some(Key::Backspace | Key::Delete)) {
            let range = Range { anchor: Caret { block: i, offset: 0 }, focus: Caret { block: i + 1, offset: 0 } };
            let caret = self.doc_mut().replace_range(range, "");
            self.structural(before);
            return Some(caret);
        }
        None
    }

    pub fn insert_position(&self, ui: &NoteWindow) -> usize {
        let last = self.doc().len().saturating_sub(1);
        let focused = ui.get_text_focused_block();
        let at = if focused >= 0 { focused } else { ui.get_last_focused_block() };
        if at < 0 { last } else { (at as usize).min(last) }
    }

    pub fn add_image(&self, stored: &Stored, after: usize) -> Result<Caret, DbError> {
        let image_id = self.db.add_image(self.id.get(), stored.mime, stored.width, stored.height, &stored.data)?;
        let pixels = crate::images::decode(&stored.data).ok().map(|p| crate::note_images::to_slint(&p));
        self.images.add(image_id, (stored.width, stored.height), pixels);
        let before = self.doc().clone();
        let at = self.doc_mut().insert_image(after, image_id);
        self.structural(before);
        let caret = Caret { block: at + 1, offset: 0 };
        self.notify_change(Some(caret));
        Ok(caret)
    }

    pub fn image_state(&self, block: usize) -> Option<(u8, Align)> {
        self.doc().blocks().get(block).filter(|b| b.kind == Kind::Image).map(|b| (b.image_pct, b.align))
    }

    pub fn edit_image(&self, edit: impl FnOnce(&mut Document) -> Option<Option<Caret>>) -> bool {
        let before = self.doc().clone();
        let Some(caret) = edit(&mut self.doc_mut()) else { return false };
        self.structural(before);
        self.notify_change(caret);
        true
    }

    pub fn set_image_pct(&self, block: usize, pct: u8) -> bool {
        self.edit_image(|d| d.set_image_pct(block, pct).then_some(None))
    }

    pub fn set_image_align(&self, block: usize, align: Align) -> bool {
        self.edit_image(|d| d.set_image_align(block, align).then_some(None))
    }

    pub fn delete_image(&self, block: usize) -> bool {
        self.edit_image(|d| d.delete_image(block).map(Some))
    }

    pub fn image_rgba(&self, block: usize) -> Option<Rgba> {
        let doc = self.doc();
        let b = doc.blocks().get(block).filter(|b| b.kind == Kind::Image)?;
        self.images.rgba(b)
    }

    pub fn copy_image(&self, block: usize) -> bool {
        let Some(rgba) = self.image_rgba(block) else { return false };
        self.clipboard.set_image(&rgba).is_ok()
    }

    pub fn cut_image(&self, block: usize) -> Option<Caret> {
        let doc = self.doc();
        let b = doc.blocks().get(block).filter(|b| b.kind == Kind::Image)?;
        let rgba = self.images.rgba(b)?;
        drop(doc);
        if self.clipboard.set_image(&rgba).is_err() {
            return None;
        }
        let before = self.doc().clone();
        let caret = self.doc_mut().delete_image(block)?;
        self.structural(before);
        self.notify_change(Some(caret));
        Some(caret)
    }

    pub fn picture_at(&self, block: usize) -> Option<(slint::Image, (u32, u32))> {
        let doc = self.doc();
        let b = doc.blocks().get(block)?;
        let size = self.images.size(b)?;
        let pixels = self.images.pixels(b);
        (pixels.size().width > 0).then_some((pixels, size))
    }

    pub fn decoded_image_bytes(&self) -> usize {
        self.images.decoded_bytes()
    }

    pub fn free_images(&self) {
        self.drop_layout();
        self.images.free();
        self.sync_model();
    }

    pub fn thaw_images(&self) {
        self.images.thaw(&self.doc());
        self.sync_model();
    }

    pub fn prune_images(&self) -> Result<(), DbError> {
        let keep: Vec<i64> = self.doc().blocks().iter().filter_map(|b| b.image_id).collect();
        self.db.prune_images(self.id.get(), &keep)
    }
}
