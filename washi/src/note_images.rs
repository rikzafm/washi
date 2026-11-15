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
    /// The pixels turned by the text's tilt, made the first time they are asked for (the software
    /// renderer cannot turn a picture). Dropped with the pixels.
    turned: Option<slint::Image>,
}

pub struct NoteImages {
    note: Cell<NoteId>,
    db: Rc<Db>,
    slots: RefCell<HashMap<i64, Slot>>,
    /// Freed (suspended): new pictures are known by size only until [`NoteImages::thaw`].
    frozen: Cell<bool>,
    /// Changes whenever a picture's size is added, removed or changed (see [`NoteImages::generation`]).
    generation: Cell<u64>,
}

/// The text's tilt, degrees clockwise (`Theme.text-tilt`): a picture at rest lies on it.
pub const TEXT_TILT_DEG: f64 = 1.45;

/// Catmull-Rom weights of the four samples around a point `t` (0..1) past the second one.
fn catmull_rom(t: f64) -> [f64; 4] {
    let (t2, t3) = (t * t, t * t * t);
    [
        0.5 * (-t3 + 2.0 * t2 - t),
        0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    ]
}

/// `src` turned `deg` clockwise about its center, on a transparent canvas just big enough, with
/// smooth edges. (Bicubic sampling; the corners and edges fade out over one pixel.)
pub fn turn(src: &slint::SharedPixelBuffer<slint::Rgba8Pixel>, deg: f64) -> slint::SharedPixelBuffer<slint::Rgba8Pixel> {
    let (sw, sh) = (src.width() as f64, src.height() as f64);
    let (sin, cos) = deg.to_radians().sin_cos();
    let (ow, oh) = ((sw * cos.abs() + sh * sin.abs()).ceil() + 1.0, (sw * sin.abs() + sh * cos.abs()).ceil() + 1.0);
    let mut out = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(ow as u32, oh as u32);
    let px = src.as_slice();
    let at = |x: i64, y: i64| px[(y.clamp(0, src.height() as i64 - 1) as usize) * src.width() as usize + x.clamp(0, src.width() as i64 - 1) as usize];
    let dst = out.make_mut_slice();
    for oy in 0..oh as usize {
        for ox in 0..ow as usize {
            // where this output pixel's center comes from in the source
            let (dx, dy) = (ox as f64 + 0.5 - ow / 2.0, oy as f64 + 0.5 - oh / 2.0);
            let (u, v) = (dx * cos + dy * sin + sw / 2.0, -dx * sin + dy * cos + sh / 2.0);
            let edge = u.min(sw - u).min(v).min(sh - v);
            let coverage = (edge + 0.5).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let (fx, fy) = (u - 0.5, v - 0.5);
            let (x0, y0) = (fx.floor(), fy.floor());
            let (tx, ty) = (fx - x0, fy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            // Catmull-Rom over the 4x4 neighbours: keeps detail that bilinear averages away.
            let (wx, wy) = (catmull_rom(tx), catmull_rom(ty));
            let mut taps = [[slint::Rgba8Pixel { r: 0, g: 0, b: 0, a: 0 }; 4]; 4];
            for (j, row) in taps.iter_mut().enumerate() {
                for (i, t) in row.iter_mut().enumerate() {
                    *t = at(x0 - 1 + i as i64, y0 - 1 + j as i64);
                }
            }
            let mix = |c: fn(&slint::Rgba8Pixel) -> u8| {
                let mut sum = 0.0;
                for j in 0..4 {
                    for i in 0..4 {
                        sum += c(&taps[j][i]) as f64 * wx[i] * wy[j];
                    }
                }
                sum.clamp(0.0, 255.0)
            };
            dst[oy * ow as usize + ox] = slint::Rgba8Pixel {
                r: mix(|p| p.r).round() as u8,
                g: mix(|p| p.g).round() as u8,
                b: mix(|p| p.b).round() as u8,
                a: (mix(|p| p.a) * coverage).round() as u8,
            };
        }
    }
    out
}

pub fn to_slint(px: &Rgba) -> slint::Image {
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(px.width, px.height);
    buf.make_mut_bytes().copy_from_slice(&px.data);
    slint::Image::from_rgba8(buf)
}

impl NoteImages {
    pub fn new(note: NoteId, db: Rc<Db>) -> NoteImages {
        NoteImages { note: Cell::new(note), db, slots: RefCell::new(HashMap::new()), frozen: Cell::new(false), generation: Cell::new(0) }
    }

    fn decode_stored(&self, id: i64) -> Option<((u32, u32), Option<slint::Image>)> {
        let stored = match self.db.image(id) {
            Ok(Some(s)) => s,
            Ok(None) => return None,
            Err(e) => {
                eprintln!("washi: could not read image {id} of note {}: {e}", self.note.get());
                return None;
            }
        };
        let pixels = images::decode(&stored.data).ok().map(|p| to_slint(&p));
        Some(((stored.width, stored.height), pixels))
    }

    /// Show another note's pictures from now on (the All Notes editor swaps notes in place). The
    /// pictures of the old note are forgotten by the next [`NoteImages::load`].
    pub fn retarget(&self, note: NoteId) {
        self.note.set(note);
        self.bump();
    }

    fn bump(&self) {
        self.generation.set(self.generation.get() + 1);
    }

    /// A number that is another one whenever the size of any picture was added, removed or changed:
    /// what a layout built from these sizes must be checked against.
    pub fn generation(&self) -> u64 {
        self.generation.get()
    }

    /// Learn the images a document refers to, decoding the ones not yet known, and forget the rest.
    pub fn load(&self, doc: &Document) {
        crate::trace::count(crate::trace::counts::Count::ImagesLoad);
        let wanted: Vec<i64> = doc.blocks().iter().filter_map(|b| b.image_id).collect();
        let mut slots = self.slots.borrow_mut();
        let known = slots.len();
        slots.retain(|id, _| wanted.contains(id));
        if slots.len() != known {
            self.bump();
        }
        for id in wanted {
            let have = slots.get(&id).is_some_and(|s| s.pixels.is_some() || self.frozen.get());
            if have {
                continue;
            }
            if self.frozen.get() {
                if let Ok(Some(size)) = self.db.image_size(id) {
                    slots.insert(id, Slot { size, pixels: None, turned: None });
                    self.bump();
                }
            } else if let Some((size, pixels)) = self.decode_stored(id) {
                slots.insert(id, Slot { size, pixels, turned: None });
                self.bump();
            }
        }
    }

    /// Whether every one of `ids` has been learned (a picture whose load failed has not).
    pub fn knows(&self, ids: &[i64]) -> bool {
        let slots = self.slots.borrow();
        ids.iter().all(|id| slots.contains_key(id))
    }

    /// Register an image that was just stored and show it.
    pub fn add(&self, id: i64, size: (u32, u32), pixels: Option<slint::Image>) {
        let pixels = if self.frozen.get() { None } else { pixels };
        self.slots.borrow_mut().insert(id, Slot { size, pixels, turned: None });
        self.bump();
    }

    pub fn size(&self, block: &Block) -> Option<(u32, u32)> {
        self.slots.borrow().get(&block.image_id?).map(|s| s.size)
    }

    /// The decoded original full-resolution picture of a block (read from the database).
    pub fn rgba(&self, block: &Block) -> Option<Rgba> {
        let id = block.image_id?;
        let stored = self.db.image(id).ok()??;
        crate::images::decode(&stored.data).ok()
    }

    /// The decoded picture of a block (empty while freed).
    pub fn pixels(&self, block: &Block) -> slint::Image {
        block.image_id.and_then(|id| self.slots.borrow().get(&id).and_then(|s| s.pixels.clone())).unwrap_or_default()
    }

    /// The decoded picture of a block turned by the text's tilt (empty while freed).
    pub fn turned(&self, block: &Block) -> slint::Image {
        let Some(id) = block.image_id else { return Default::default() };
        let mut slots = self.slots.borrow_mut();
        let Some(slot) = slots.get_mut(&id) else { return Default::default() };
        if slot.turned.is_none() {
            slot.turned = slot.pixels.as_ref().and_then(|p| p.to_rgba8()).map(|buf| slint::Image::from_rgba8(turn(&buf, TEXT_TILT_DEG)));
        }
        slot.turned.clone().unwrap_or_default()
    }

    /// Drop every decoded picture (the note closes or Washi suspends); sizes stay.
    pub fn free(&self) {
        self.frozen.set(true);
        for s in self.slots.borrow_mut().values_mut() {
            s.pixels = None;
            s.turned = None;
        }
    }

    /// Waking: decode the pictures of `doc` again.
    pub fn thaw(&self, doc: &Document) {
        self.frozen.set(false);
        self.load(doc);
    }

    /// Bytes of decoded pixels held right now.
    pub fn decoded_bytes(&self) -> usize {
        let bytes = |i: &Option<slint::Image>| i.as_ref().map_or(0, |i| i.size().width as usize * i.size().height as usize * 4);
        self.slots.borrow().values().map(|s| bytes(&s.pixels) + bytes(&s.turned)).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turned_picture_has_a_transparent_corner_and_an_opaque_middle() {
        let mut src = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(100, 80);
        src.make_mut_slice().fill(slint::Rgba8Pixel { r: 200, g: 100, b: 50, a: 255 });
        let t = turn(&src, 1.5);
        assert!(t.width() > 100 && t.height() > 80 && t.width() < 110 && t.height() < 92);
        assert_eq!(t.as_slice()[0].a, 0);
        let mid = t.as_slice()[(t.height() / 2 * t.width() + t.width() / 2) as usize];
        assert_eq!((mid.r, mid.g, mid.b, mid.a), (200, 100, 50, 255));
    }
}
