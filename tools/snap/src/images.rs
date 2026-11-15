//! Harness actions and checks for pictures in notes: the toolbar's image button, pasting from the
//! clipboard, what each image block shows, and the stored bytes.

use crate::Harness;
use slint::{ComponentHandle, Model};
use std::path::Path;
use std::time::Duration;
use washi::images::Rgba;

/// An image block as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageBlock {
    /// Pixel size of the stored picture.
    pub size: (i32, i32),
    /// Whether the decoded pixels are held right now.
    pub decoded: bool,
}

/// A picture as stored in the database.
pub struct StoredPicture {
    pub id: i64,
    pub mime: String,
    pub size: (u32, u32),
    pub data: Vec<u8>,
}

impl Harness {
    /// Answer the next file dialog with this picture.
    pub fn choose_picture(&self, path: &Path) {
        *self.dialogs.open_path.borrow_mut() = Some(path.to_path_buf());
    }

    /// Put a picture on the clipboard (no text with it).
    pub fn copy_picture(&self, pixels: Rgba) {
        *self.clipboard.image.borrow_mut() = Some(pixels);
    }

    /// Hover the note so the toolbar shows, then click the toolbar's image button where it really is
    /// (the paper's tilt moves it a few px from where an untilted toolbar would have it).
    pub fn click_image_button(&mut self, note: usize) {
        self.pointer_move(note, 5.0, 5.0);
        self.advance(Duration::from_millis(300));
        let (w, h) = self.note_size(note);
        let margin = self.app().with_note(self.id(note), |n| n.ui().get_window_margin()).unwrap();
        let (pw, ph) = (w as f32 - 2.0 * margin, h as f32 - 2.0 * margin);
        // toolbar.slint: first button 23.2 from the paper's left, 40 wide, pitch 42.667; buttons
        // 55.8 above the paper's bottom, 40 high; the eighth is the image button
        let u = 23.2 + 20.0 + 7.0 * 42.667 - pw / 2.0;
        let v = ph / 2.0 - 55.8 + 20.0;
        let tilt = 1.5f32.to_radians();
        let (x, y) = (w as f32 / 2.0 + u * tilt.cos() - v * tilt.sin(), h as f32 / 2.0 + u * tilt.sin() + v * tilt.cos());
        self.pointer_move(note, x, y);
        self.press_button(note, x, y, false);
        self.release_button(note, x, y, false);
    }

    /// Let the picture threads finish and the app add their results.
    pub fn wait_for_images(&mut self) {
        for _ in 0..2000 {
            self.advance(Duration::from_millis(20));
            if self.app().pending_images() == 0 {
                // The window reacts to the new block when it draws its next frame (the focus moves
                // to the paragraph below the picture): draw one, and ask for another so that a
                // test's own render still has something to draw.
                self.advance(Duration::ZERO);
                for note in 0..self.open_note_count() {
                    self.render(note);
                    self.app().with_note(self.id(note), |n| n.ui().window().request_redraw());
                }
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("pictures were still being prepared after 4 s");
    }

    /// Pictures being prepared right now.
    pub fn pending_images(&self) -> usize {
        self.app().pending_images()
    }

    /// The kinds of the note's blocks, in order: "title", "heading", "body", "list-item", "image".
    pub fn block_kinds(&self, note: usize) -> Vec<String> {
        self.app()
            .with_note(self.id(note), |n| n.ui().get_blocks().iter().map(|b| format!("{:?}", b.kind).to_lowercase()).collect())
            .unwrap()
    }

    /// The note's image blocks, in order.
    pub fn image_blocks(&self, note: usize) -> Vec<ImageBlock> {
        self.app()
            .with_note(self.id(note), |n| {
                n.ui()
                    .get_blocks()
                    .iter()
                    .filter(|b| format!("{:?}", b.kind) == "Image")
                    .map(|b| ImageBlock { size: (b.img_w, b.img_h), decoded: b.img.size().width > 0 })
                    .collect()
            })
            .unwrap()
    }

    /// Bytes of decoded pictures all open notes hold.
    pub fn decoded_image_bytes(&self) -> usize {
        self.app().decoded_image_bytes()
    }

    /// The pictures the note's saved text refers to, as stored.
    pub fn stored_pictures(&self, note: usize) -> Vec<StoredPicture> {
        let text = self.saved_text(note);
        text.lines()
            .filter_map(|l| l.strip_prefix("![](img:")?.strip_suffix(')')?.parse::<i64>().ok())
            .filter_map(|id| self.app().stored_image(id).map(|(mime, w, h, data)| StoredPicture { id, mime, size: (w, h), data }))
            .collect()
    }

    /// Number of pictures stored in the database for all notes (referenced or not).
    pub fn stored_picture_count(&self) -> usize {
        self.app().stored_image_count()
    }
}
