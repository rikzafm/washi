//! The pictures of one open note: sizes (always known) and decoded pixels (only while shown).
//!
//! The stored form is a small encoded image in the encrypted database. Decoded pixels exist only
//! between a note opening (or waking from suspension) and it closing (or suspending): [`free`]
//! drops them, [`thaw`] decodes them again from the database.
//!
//! [`free`]: NoteImages::free
//! [`thaw`]: NoteImages::thaw

use crate::document::{Block, Document};
use crate::images::{self, Rgba};
use crate::platform::NoteId;
use crate::storage::Db;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

struct Slot {
    /// Pixel size of the stored image (the block's height follows from it, decoded or not).
    size: (u32, u32),
    pixels: Option<slint::Image>,
}

pub struct NoteImages {
    note: NoteId,
    db: Rc<Db>,
    slots: RefCell<HashMap<i64, Slot>>,
    /// Freed (suspended): new pictures are known by size only until [`NoteImages::thaw`].
    frozen: Cell<bool>,
}

pub fn to_slint(px: &Rgba) -> slint::Image {
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(px.width, px.height);
    buf.make_mut_bytes().copy_from_slice(&px.data);
    slint::Image::from_rgba8(buf)
}

impl NoteImages {
    pub fn new(note: NoteId, db: Rc<Db>) -> NoteImages {
        NoteImages { note, db, slots: RefCell::new(HashMap::new()), frozen: Cell::new(false) }
    }

    fn decode_stored(&self, id: i64) -> Option<((u32, u32), Option<slint::Image>)> {
        let stored = match self.db.image(id) {
            Ok(Some(s)) => s,
            Ok(None) => return None,
            Err(e) => {
                eprintln!("washi: could not read image {id} of note {}: {e}", self.note);
                return None;
            }
        };
        let pixels = images::decode(&stored.data).ok().map(|p| to_slint(&p));
        Some(((stored.width, stored.height), pixels))
    }

    /// Learn the images a document refers to, decoding the ones not yet known, and forget the rest.
    pub fn load(&self, doc: &Document) {
        let wanted: Vec<i64> = doc.blocks().iter().filter_map(|b| b.image_id).collect();
        let mut slots = self.slots.borrow_mut();
        slots.retain(|id, _| wanted.contains(id));
        for id in wanted {
            let known = slots.get(&id).is_some_and(|s| s.pixels.is_some() || self.frozen.get());
            if known {
                continue;
            }
            if self.frozen.get() {
                if let Ok(Some(size)) = self.db.image_size(id) {
                    slots.insert(id, Slot { size, pixels: None });
                }
            } else if let Some((size, pixels)) = self.decode_stored(id) {
                slots.insert(id, Slot { size, pixels });
            }
        }
    }

    /// Register an image that was just stored and show it.
    pub fn add(&self, id: i64, size: (u32, u32), pixels: Option<slint::Image>) {
        let pixels = if self.frozen.get() { None } else { pixels };
        self.slots.borrow_mut().insert(id, Slot { size, pixels });
    }

    pub fn size(&self, block: &Block) -> Option<(u32, u32)> {
        self.slots.borrow().get(&block.image_id?).map(|s| s.size)
    }

    /// The decoded picture of a block (empty while freed).
    pub fn pixels(&self, block: &Block) -> slint::Image {
        block.image_id.and_then(|id| self.slots.borrow().get(&id).and_then(|s| s.pixels.clone())).unwrap_or_default()
    }

    /// Drop every decoded picture (the note closes or Washi suspends); sizes stay.
    pub fn free(&self) {
        self.frozen.set(true);
        for s in self.slots.borrow_mut().values_mut() {
            s.pixels = None;
        }
    }

    /// Waking: decode the pictures of `doc` again.
    pub fn thaw(&self, doc: &Document) {
        self.frozen.set(false);
        self.load(doc);
    }

    /// Bytes of decoded pixels held right now.
    pub fn decoded_bytes(&self) -> usize {
        self.slots.borrow().values().filter(|s| s.pixels.is_some()).map(|s| s.size.0 as usize * s.size.1 as usize * 4).sum()
    }
}
