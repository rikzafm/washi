//! Clipboard: copying, cutting and pasting ranges, rich text and pictures.

use super::*;

impl Editor {
    pub fn copy_range(&self, clipboard: &Rc<dyn ImageClipboard>) -> bool {
        let doc = self.doc.borrow();
        let range = self.selection.get();
        if let Some(i) = self.selected_image(&doc) {
            if let Some(b) = doc.blocks().get(i) {
                if let Some(rgba) = self.images.rgba(b) {
                    return clipboard.set_image(&rgba).is_ok();
                }
            }
        }
        let (plain, html) = crate::clipboard_html::to_html_and_text(doc.blocks(), range, &|b| self.images.rgba(b));
        clipboard.set_rich(crate::platform::ClipboardPayload {
            plain: Some(plain),
            html: Some(html),
            image: None,
        }).is_ok()
    }

    pub fn cut_range(&self, clipboard: &Rc<dyn ImageClipboard>) -> Option<Caret> {
        let doc = self.doc.borrow();
        if let Some(i) = self.selected_image(&doc) {
            if let Some(b) = doc.blocks().get(i) {
                if let Some(rgba) = self.images.rgba(b) {
                    let _ = clipboard.set_image(&rgba);
                    drop(doc);
                    let before = self.doc.borrow().clone();
                    let caret = self.doc.borrow_mut().delete_image(i)?;
                    self.structural(before);
                    return Some(caret);
                }
            }
        }
        drop(doc);
        if !self.copy_range(clipboard) {
            return None;
        }
        Some(self.replace_range(""))
    }

    pub fn paste_rich_or_image(&self) -> bool {
        use crate::clipboard_html::ParsedBlock;
        if let Some(html) = self.clipboard.html() {
            let parsed = crate::clipboard_html::from_html(&html);
            let is_plain_single_body = parsed.len() == 1
                && matches!(&parsed[0], ParsedBlock::Body { marks, paints, .. } if marks.iter().all(|m| *m == Flags::NONE) && paints.iter().all(|p| *p == Paint::default()));
            if !parsed.is_empty() && !is_plain_single_body {
                let before = self.doc.borrow().clone();
                let range = self.selection.get();
                let db = self.db.clone();
                let images = &self.images;
                let id = self.id.get();
                let mut add_image = |rgba: Rgba| -> Option<i64> {
                    let (w, h) = (rgba.width, rgba.height);
                    let png = crate::images::encode_png(&rgba).ok()?;
                    let image_id = db.add_image(id, "image/png", w, h, &png).ok()?;
                    let pixels = Some(crate::note_images::to_slint(&rgba));
                    images.add(image_id, (w, h), pixels);
                    Some(image_id)
                };
                let caret = self.doc.borrow_mut().replace_range_rich(range, parsed, &mut add_image);
                self.structural(before);
                self.notify_change(Some(caret));
                return true;
            }
        }
        if let Some(rgba) = self.clipboard.image() {
            let after = {
                let last = self.doc.borrow().len().saturating_sub(1);
                let range = self.selection.get();
                range.focus.block.min(last)
            };
            let (w, h) = (rgba.width, rgba.height);
            if let Ok(png) = crate::images::encode_png(&rgba) {
                if let Ok(image_id) = self.db.add_image(self.id.get(), "image/png", w, h, &png) {
                    let pixels = Some(crate::note_images::to_slint(&rgba));
                    self.images.add(image_id, (w, h), pixels);
                    let before = self.doc.borrow().clone();
                    let at = self.doc.borrow_mut().insert_image(after, image_id);
                    self.structural(before);
                    self.notify_change(Some(Caret { block: at + 1, offset: 0 }));
                    return true;
                }
            }
        }
        false
    }

    pub fn paste(&self) -> bool {
        if self.paste_rich_or_image() {
            return true;
        }
        if let Some(text) = self.clipboard.text() {
            if !text.is_empty() {
                let caret = self.replace_range(&text);
                self.notify_change(Some(caret));
                return true;
            }
        }
        false
    }

    pub fn paste_image(&self) -> bool {
        if let Some(rgba) = self.clipboard.image() {
            let after = {
                let last = self.doc.borrow().len().saturating_sub(1);
                let range = self.selection.get();
                range.focus.block.min(last)
            };
            let (w, h) = (rgba.width, rgba.height);
            if let Ok(png) = crate::images::encode_png(&rgba) {
                if let Ok(image_id) = self.db.add_image(self.id.get(), "image/png", w, h, &png) {
                    let pixels = Some(crate::note_images::to_slint(&rgba));
                    self.images.add(image_id, (w, h), pixels);
                    let before = self.doc.borrow().clone();
                    let at = self.doc.borrow_mut().insert_image(after, image_id);
                    self.structural(before);
                    self.notify_change(Some(Caret { block: at + 1, offset: 0 }));
                    return true;
                }
            }
        }
        false
    }
}
