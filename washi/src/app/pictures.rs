//! Pictures: importing them into notes off the UI thread, and the picture viewers.

use super::App;
use crate::image_viewer::ImageViewer;
use crate::images::{self, ImageError, Source, Stored};
use crate::note_window::{app_icon, note_title};
use crate::platform::{NoteId, ScreenRect};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

/// A picture on its way into a note.
pub(super) struct Import {
    note: NoteId,
    /// The block it goes after.
    after: usize,
    result: Receiver<Result<Stored, ImageError>>,
}

/// How often the worker threads' results are looked for while a picture is being prepared.
const IMPORT_POLL: Duration = Duration::from_millis(20);

impl App {
    /// Show the picture of `block` of note `id` in a window of its own.
    pub(super) fn open_viewer(&self, id: NoteId, block: usize) {
        let Some((picture, size)) = self.with_note(id, |n| n.picture_at(block)).flatten() else { return };
        let rgba = self.with_note(id, |n| n.image_rgba(block)).flatten();
        let caption = self.saved_text(id).and_then(|t| note_title(&t)).unwrap_or_else(|| "Picture".to_string());
        let target = self.with_note(id, |n| {
            let p = n.ui().window().position();
            let s = n.ui().window().size();
            ScreenRect::new(p.x, p.y, s.width as i32, s.height as i32)
        });

        let weak = Rc::downgrade(&self.0);
        let slot: Rc<RefCell<Option<std::rc::Weak<ImageViewer>>>> = Rc::new(RefCell::new(None));
        let slot_close = slot.clone();
        let on_close = Rc::new(move || {
            if let Some(inner) = weak.upgrade() {
                if let Some(weak_viewer) = slot_close.borrow_mut().take() {
                    if let Some(v) = weak_viewer.upgrade() {
                        inner.viewers.borrow_mut().retain(|other| !Rc::ptr_eq(other, &v));
                        slint::Timer::single_shot(std::time::Duration::ZERO, move || drop(v));
                    }
                }
            }
        });

        match ImageViewer::open(
            picture,
            size,
            &caption,
            target,
            self.0.env.window_control.clone(),
            self.0.env.clipboard.clone(),
            rgba,
            app_icon(),
            on_close,
        ) {
            Ok(viewer) => {
                let v = Rc::new(viewer);
                *slot.borrow_mut() = Some(Rc::downgrade(&v));
                self.0.viewers.borrow_mut().push(v);
            }
            Err(e) => eprintln!("washi: could not open the picture viewer: {e}"),
        }
    }

    /// How many screen px one logical px of each open picture viewer covers, oldest first.
    pub fn viewer_scale_factors(&self) -> Vec<f32> {
        self.0.viewers.borrow().iter().map(|v| v.scale_factor()).collect()
    }

    /// Window sizes (physical px) of open picture viewers, oldest first.
    pub fn viewer_window_sizes(&self) -> Vec<(u32, u32)> {
        self.0.viewers.borrow().iter().map(|v| v.window_size()).collect()
    }

    /// The toolbar's image button: ask for a PNG or JPEG and add it after the current paragraph.
    pub(super) fn insert_image_from_file(&self, id: NoteId) {
        let Some(path) = self.0.env.dialogs.pick_open_file("Pictures|*.png;*.jpg;*.jpeg|All files|*.*") else { return };
        self.start_import(id, Source::File(path));
    }

    /// Ctrl+V in a note: true (and the picture is on its way) when the clipboard holds a picture.
    pub(super) fn paste_image(&self, id: NoteId) -> bool {
        match self.0.env.clipboard.image() {
            Some(pixels) => {
                self.start_import(id, Source::Pixels(pixels));
                true
            }
            None => false,
        }
    }

    /// Prepare a picture on a short-lived thread (typing never stalls) and add it to the note,
    /// after the paragraph that has the caret now, when it is ready.
    fn start_import(&self, note: NoteId, source: Source) {
        let Some(after) = self.with_note(note, |n| n.insert_position()) else { return };
        let result = match images::spawn_prepare(source) {
            Ok(rx) => rx,
            Err(e) => return self.0.env.dialogs.error(&e.to_string()),
        };
        self.0.imports.borrow_mut().push(Import { note, after, result });
        if !self.0.import_timer.running() {
            let weak = Rc::downgrade(&self.0);
            self.0.import_timer.start(slint::TimerMode::Repeated, IMPORT_POLL, move || {
                if let Some(inner) = weak.upgrade() {
                    App(inner).collect_imports();
                }
            });
        }
    }

    /// Add the pictures whose threads are done; stop polling when none are left.
    fn collect_imports(&self) {
        let mut done = Vec::new();
        self.0.imports.borrow_mut().retain(|i| match i.result.try_recv() {
            Ok(r) => {
                done.push((i.note, i.after, r));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => {
                done.push((i.note, i.after, Err(ImageError("The picture could not be prepared.".into()))));
                false
            }
        });
        if self.0.imports.borrow().is_empty() {
            self.0.import_timer.stop();
        }
        for (note, after, result) in done {
            match result {
                Ok(stored) => {
                    // the note may have been closed meanwhile: then the picture is dropped
                    if let Some(Err(e)) = self.with_note(note, |n| n.add_image(&stored, after)) {
                        self.0.env.dialogs.error(&format!("Washi could not save the picture: {e}"));
                    }
                }
                Err(e) => self.0.env.dialogs.error(&e.to_string()),
            }
        }
    }

    /// A stored picture: mime type, size and encoded bytes (what is in the database).
    pub fn stored_image(&self, id: i64) -> Option<(String, u32, u32, Vec<u8>)> {
        self.0.db.image(id).ok().flatten().map(|i| (i.mime, i.width, i.height, i.data))
    }

    /// How many pictures the database holds, for all notes.
    pub fn stored_image_count(&self) -> usize {
        self.0.db.image_count().unwrap_or(0)
    }

    /// Pictures still being prepared.
    pub fn pending_images(&self) -> usize {
        self.0.imports.borrow().len()
    }

    /// Bytes of decoded pictures all open notes hold right now.
    pub fn decoded_image_bytes(&self) -> usize {
        self.0.notes.borrow().iter().map(|n| n.decoded_image_bytes()).sum()
    }
}
