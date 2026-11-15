//! The note's words turned onto the tilted baseline, as small pictures.
//!
//! The software renderer cannot turn text, so a word drawn by a `Text` is level and lands on whole
//! pixels: along a tilted line the words (and the pieces of one) step up and down by a pixel. Here
//! a word is drawn the way the renderer would draw it turned: each glyph's outline is turned by the
//! tilt and filled at its own place on the baseline, to a quarter of a pixel. The result is an
//! alpha mask (white, the coverage in alpha) that the note shows 1:1, tinted with the word's ink.
//!
//! The glyphs come from the active font's own faces (`fonts::current`, the same files Slint draws
//! from), unhinted like Slint's, and are filled by the rasteriser Slint's text uses (`zeno`), so a
//! turned word weighs what the level one does.

use crate::fonts::{face_index, ActiveFont};
use ttf_parser::Face;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use zeno::{Command, Mask, Point};

/// Places per pixel a glyph is drawn at, each way: a glyph is within 1/8 px of where it belongs.
const BINS: i64 = 4;

/// The glyph masks kept (see [`GLYPHS`]); at this many they are all dropped and made again as needed.
const MAX_GLYPHS: usize = 3000;

/// A glyph's coverage and where its top-left is from the glyph's (whole pixel) origin.
struct GlyphMask {
    left: i32,
    top: i32,
    width: usize,
    height: usize,
    data: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    epoch: u64,
    face: u8,
    glyph: u16,
    /// Bits of the size in physical px and of the tilt in degrees.
    size: u32,
    tilt: u32,
    /// Which quarter of a pixel the origin is at, across and down.
    phase: (u8, u8),
}

thread_local! {
    /// The glyphs drawn so far, turned, by size and place within the pixel. Shared by every note
    /// of the thread: a resize draws the same few hundred masks over and over.
    static GLYPHS: RefCell<HashMap<GlyphKey, Rc<GlyphMask>>> = RefCell::new(HashMap::new());
}

/// Forget every glyph drawn (the notes are suspended, or the font changed).
pub fn forget_glyphs() {
    GLYPHS.with(|g| g.borrow_mut().clear());
}

/// How many glyph masks are kept (tests).
pub fn glyphs_kept() -> usize {
    GLYPHS.with(|g| g.borrow().len())
}

/// A glyph's outline as path commands: scaled to px, y down, turned clockwise, moved by `at`.
struct Outline {
    cmds: Vec<Command>,
    k: f32,
    cos: f32,
    sin: f32,
    at: (f32, f32),
}

impl Outline {
    fn pt(&self, x: f32, y: f32) -> Point {
        let (u, v) = (x * self.k, -y * self.k);
        Point::new(u * self.cos - v * self.sin + self.at.0, u * self.sin + v * self.cos + self.at.1)
    }
}

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        self.cmds.push(Command::MoveTo(p));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        self.cmds.push(Command::LineTo(p));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (c, p) = (self.pt(x1, y1), self.pt(x, y));
        self.cmds.push(Command::QuadTo(c, p));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (c1, c2, p) = (self.pt(x1, y1), self.pt(x2, y2), self.pt(x, y));
        self.cmds.push(Command::CurveTo(c1, c2, p));
    }
    fn close(&mut self) {
        self.cmds.push(Command::Close);
    }
}

/// What words are drawn from: faces (regular, bold, italic, bold italic) and which font they are of
/// (the pictures kept for another epoch are not reused).
pub trait FaceSource {
    fn face(&self, bold: bool, italic: bool) -> &Face<'_>;
    fn epoch(&self) -> u64;
}

impl FaceSource for ActiveFont {
    fn face(&self, bold: bool, italic: bool) -> &Face<'_> {
        self.faces.face(bold, italic)
    }
    fn epoch(&self) -> u64 {
        self.epoch as u64
    }
}

/// How a word is drawn: its face, size and the tilt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub bold: bool,
    pub italic: bool,
    /// Font size and the space added after every character, physical px.
    pub size: f64,
    pub spacing: f64,
    /// Degrees clockwise.
    pub tilt_deg: f64,
}

fn glyph_mask(font: &dyn FaceSource, look: &Look, glyph: ttf_parser::GlyphId, phase: (u8, u8)) -> Rc<GlyphMask> {
    let key = GlyphKey {
        epoch: font.epoch(),
        face: face_index(look.bold, look.italic) as u8,
        glyph: glyph.0,
        size: (look.size as f32).to_bits(),
        tilt: (look.tilt_deg as f32).to_bits(),
        phase,
    };
    if let Some(m) = GLYPHS.with(|g| g.borrow().get(&key).cloned()) {
        return m;
    }
    let face = font.face(look.bold, look.italic);
    let (sin, cos) = look.tilt_deg.to_radians().sin_cos();
    let mut outline = Outline {
        cmds: vec![],
        k: (look.size / face.units_per_em() as f64) as f32,
        cos: cos as f32,
        sin: sin as f32,
        at: (phase.0 as f32 / BINS as f32, phase.1 as f32 / BINS as f32),
    };
    let mask = if face.outline_glyph(glyph, &mut outline).is_some() && !outline.cmds.is_empty() {
        let (data, at) = Mask::new(&outline.cmds).render();
        GlyphMask { left: at.left, top: at.top, width: at.width as usize, height: at.height as usize, data }
    } else {
        // a space, or a glyph with no outline
        GlyphMask { left: 0, top: 0, width: 0, height: 0, data: vec![] }
    };
    let mask = Rc::new(mask);
    GLYPHS.with(|g| {
        let mut g = g.borrow_mut();
        if g.len() >= MAX_GLYPHS {
            g.clear();
        }
        g.insert(key, mask.clone());
    });
    mask
}

/// `x` (px) as a whole pixel and the quarter within it, rounded to the nearest quarter.
fn split(x: f64) -> (i64, u8) {
    let q = (x * BINS as f64).round() as i64;
    (q.div_euclid(BINS), q.rem_euclid(BINS) as u8)
}

/// Which quarter of a pixel a word's start is at, across and down: words at the same phase (and
/// alike otherwise) are the same picture.
pub fn phase_of(origin: (f64, f64)) -> (u8, u8) {
    (split(origin.0).1, split(origin.1).1)
}

/// The whole pixel a word's start belongs to (the picture's place is counted from it).
pub fn pixel_of(origin: (f64, f64)) -> (i64, i64) {
    (split(origin.0).0, split(origin.1).0)
}

/// A word turned onto the baseline: its picture and where the picture's top-left is from the
/// whole pixel of the word's start ([`pixel_of`]), physical px.
#[derive(Clone)]
pub struct Drawn {
    pub image: slint::Image,
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

/// Draw `text` starting (pen on the baseline) at `phase` within its pixel and running along the
/// tilt. `None` when nothing is inked (spaces only).
pub fn draw(font: &dyn FaceSource, text: &str, look: &Look, phase: (u8, u8)) -> Option<Drawn> {
    let face = font.face(look.bold, look.italic);
    let k = look.size / face.units_per_em() as f64;
    let (sin, cos) = look.tilt_deg.to_radians().sin_cos();
    let start = (phase.0 as f64 / BINS as f64, phase.1 as f64 / BINS as f64);
    let mut pen = 0.0;
    // each glyph's mask and its top-left from the word's pixel
    let mut placed: Vec<(Rc<GlyphMask>, i64, i64)> = vec![];
    for c in text.chars() {
        let glyph = face.glyph_index(c).unwrap_or(ttf_parser::GlyphId(0));
        let ((gx, px), (gy, py)) = (split(start.0 + pen * cos), split(start.1 + pen * sin));
        let mask = glyph_mask(font, look, glyph, (px, py));
        if mask.width > 0 && mask.height > 0 {
            placed.push((mask.clone(), gx + mask.left as i64, gy + mask.top as i64));
        }
        pen += face.glyph_hor_advance(glyph).unwrap_or(0) as f64 * k + look.spacing;
    }
    let left = placed.iter().map(|p| p.1).min()?;
    let top = placed.iter().map(|p| p.2).min()?;
    let right = placed.iter().map(|p| p.1 + p.0.width as i64).max()?;
    let bottom = placed.iter().map(|p| p.2 + p.0.height as i64).max()?;
    let (w, h) = ((right - left) as usize, (bottom - top) as usize);
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w as u32, h as u32);
    let px = buf.make_mut_slice();
    px.fill(slint::Rgba8Pixel { r: 255, g: 255, b: 255, a: 0 });
    for (mask, x, y) in &placed {
        let (ox, oy) = ((x - left) as usize, (y - top) as usize);
        for row in 0..mask.height {
            let src = &mask.data[row * mask.width..(row + 1) * mask.width];
            let dst = &mut px[(oy + row) * w + ox..(oy + row) * w + ox + mask.width];
            for (d, &s) in dst.iter_mut().zip(src) {
                // one glyph over another, as the renderer blends them
                d.a = (d.a as u32 + s as u32 * (255 - d.a as u32) / 255) as u8;
            }
        }
    }
    Some(Drawn { image: slint::Image::from_rgba8(buf), left: left as i32, top: top as i32, width: w as u32, height: h as u32 })
}

/// A bar (an underline, a strike-through) or a list bullet turned by the tilt: a box `width` by
/// `height` px with round ends (a bullet is a dot), about its centre, which is at `phase` within
/// its pixel. The picture's place is counted from that pixel, like a word's.
pub fn draw_bar(width: f64, height: f64, tilt_deg: f64, phase: (u8, u8)) -> Option<Drawn> {
    let (sin, cos) = tilt_deg.to_radians().sin_cos();
    let at = (phase.0 as f64 / BINS as f64, phase.1 as f64 / BINS as f64);
    let r = width.min(height) / 2.0;
    let (hx, hy) = (width / 2.0 - r, height / 2.0 - r);
    // the outline: a quarter circle about each corner's centre, clockwise from the top right
    const STEPS: usize = 6;
    let mut cmds = Vec::with_capacity(4 * (STEPS + 1) + 1);
    for (k, (cx, cy)) in [(hx, -hy), (hx, hy), (-hx, hy), (-hx, -hy)].into_iter().enumerate() {
        for i in 0..=STEPS {
            let a = (k as f64 - 1.0 + i as f64 / STEPS as f64) * std::f64::consts::FRAC_PI_2;
            let (u, v) = (cx + r * a.cos(), cy + r * a.sin());
            let p = Point::new((u * cos - v * sin + at.0) as f32, (u * sin + v * cos + at.1) as f32);
            cmds.push(if cmds.is_empty() { Command::MoveTo(p) } else { Command::LineTo(p) });
        }
    }
    cmds.push(Command::Close);
    let (data, place) = Mask::new(&cmds).render();
    if place.width == 0 || place.height == 0 {
        return None;
    }
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(place.width, place.height);
    for (d, &a) in buf.make_mut_slice().iter_mut().zip(&data) {
        *d = slint::Rgba8Pixel { r: 255, g: 255, b: 255, a };
    }
    Some(Drawn { image: slint::Image::from_rgba8(buf), left: place.left, top: place.top, width: place.width, height: place.height })
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct WordKey {
    text: String,
    epoch: u64,
    face: u8,
    size: u64,
    spacing: u64,
    tilt: u64,
    phase: (u8, u8),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct BarKey {
    width: u64,
    height: u64,
    tilt: u64,
    phase: (u8, u8),
}

/// Pictures by what they show, each with whether the pass under way asked for it.
struct Kept<K>(HashMap<K, (Option<Drawn>, bool)>);

impl<K> Default for Kept<K> {
    fn default() -> Self {
        Kept(HashMap::new())
    }
}

impl<K: std::hash::Hash + Eq> Kept<K> {
    fn begin(&mut self) {
        for v in self.0.values_mut() {
            v.1 = false;
        }
    }

    fn get(&mut self, key: K, draw: impl FnOnce() -> Option<Drawn>) -> Option<Drawn> {
        let entry = self.0.entry(key).or_insert_with(|| (draw(), true));
        entry.1 = true;
        entry.0.clone()
    }

    fn end(&mut self) {
        self.0.retain(|_, v| v.1);
    }

    fn bytes(&self) -> usize {
        self.0.values().filter_map(|v| v.0.as_ref()).map(|d| d.width as usize * d.height as usize * 4).sum()
    }
}

/// The words and bars a note shows turned, kept from one layout to the next: a relayout mostly
/// leaves them what they were, and a word that repeats is one picture.
#[derive(Default)]
pub struct Words {
    words: Kept<WordKey>,
    bars: Kept<BarKey>,
}

impl Words {
    /// Start a pass: the pictures not asked for before [`Words::end`] are dropped then.
    pub fn begin(&mut self) {
        self.words.begin();
        self.bars.begin();
    }

    /// The picture of `text` at `phase`, drawn now if it is not kept.
    pub fn get(&mut self, font: &dyn FaceSource, text: &str, look: &Look, phase: (u8, u8)) -> Option<Drawn> {
        let key = WordKey {
            text: text.to_string(),
            epoch: font.epoch(),
            face: face_index(look.bold, look.italic) as u8,
            size: look.size.to_bits(),
            spacing: look.spacing.to_bits(),
            tilt: look.tilt_deg.to_bits(),
            phase,
        };
        self.words.get(key, || draw(font, text, look, phase))
    }

    /// The picture of a bar ([`draw_bar`]), drawn now if it is not kept.
    pub fn bar(&mut self, width: f64, height: f64, tilt_deg: f64, phase: (u8, u8)) -> Option<Drawn> {
        let key = BarKey { width: width.to_bits(), height: height.to_bits(), tilt: tilt_deg.to_bits(), phase };
        self.bars.get(key, || draw_bar(width, height, tilt_deg, phase))
    }

    /// End a pass: drop what it did not use.
    pub fn end(&mut self) {
        self.words.end();
        self.bars.end();
    }

    /// Drop every picture.
    pub fn clear(&mut self) {
        *self = Words::default();
    }

    /// How many pictures are kept.
    pub fn len(&self) -> usize {
        self.words.0.len() + self.bars.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The bytes of the pictures kept.
    pub fn bytes(&self) -> usize {
        self.words.bytes() + self.bars.bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(tilt_deg: f64) -> Look {
        Look { bold: false, italic: false, size: 24.0, spacing: 0.0, tilt_deg }
    }

    /// The row (from the picture's top, in px from the word's pixel) holding the lowest ink of the
    /// columns `x0..x1`.
    fn lowest_ink(d: &Drawn, x0: u32, x1: u32) -> Option<i32> {
        let buf = d.image.to_rgba8().unwrap();
        let px = buf.as_slice();
        (0..d.height).rev().find(|&y| (x0..x1.min(d.width)).any(|x| px[(y * d.width + x) as usize].a > 128)).map(|y| y as i32 + d.top)
    }

    #[test]
    fn a_level_word_stands_on_its_baseline_and_a_tilted_one_runs_down_along_it() {
        let font = crate::fonts::current();
        let text = "HHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHH";
        let level = draw(&font, text, &look(0.0), (0, 0)).unwrap();
        // the feet of the first and the last H are on the same row
        let (a, b) = (lowest_ink(&level, 0, 12).unwrap(), lowest_ink(&level, level.width - 12, level.width).unwrap());
        assert_eq!(a, b, "level");
        assert!((-1..=1).contains(&a), "the feet are at the baseline (row {a})");
        let tilted = draw(&font, text, &look(1.45), (0, 0)).unwrap();
        let (a, b) = (lowest_ink(&tilted, 0, 12).unwrap(), lowest_ink(&tilted, tilted.width - 12, tilted.width).unwrap());
        let drop = tilted.width as f64 * 1.45f64.to_radians().tan();
        assert!(((b - a) as f64 - drop).abs() <= 2.0, "the last H is {} px lower, the tilt makes it {drop:.1}", b - a);
    }

    #[test]
    fn a_word_moves_down_within_its_pixel_with_its_phase() {
        let font = crate::fonts::current();
        let ink = |d: &Drawn| -> u64 { d.image.to_rgba8().unwrap().as_slice().iter().enumerate().map(|(i, p)| p.a as u64 * (i as u64 / d.width as u64 + 1)).sum() };
        let top = draw(&font, "H", &look(1.45), (0, 0)).unwrap();
        let low = draw(&font, "H", &look(1.45), (0, 3)).unwrap();
        // the same ink, lower in the same rows: the coverage's weight moves down
        assert_eq!((top.width, top.left), (low.width, low.left));
        assert!(low.top > top.top || (low.top == top.top && ink(&low) > ink(&top)), "three quarters of a pixel lower");
    }

    #[test]
    fn spaces_draw_nothing_and_kept_words_are_dropped_when_not_used() {
        let font = crate::fonts::current();
        assert!(draw(&font, "   ", &look(1.45), (0, 0)).is_none());
        let mut words = Words::default();
        words.begin();
        assert!(words.get(&font, "one", &look(1.45), (1, 2)).is_some());
        assert!(words.get(&font, "two", &look(1.45), (1, 2)).is_some());
        words.end();
        assert_eq!(words.len(), 2);
        words.begin();
        assert!(words.get(&font, "two", &look(1.45), (1, 2)).is_some());
        words.end();
        assert_eq!(words.len(), 1);
        assert!(words.bytes() > 0);
    }

    #[test]
    fn a_bar_is_as_long_and_thick_as_asked_and_runs_down_along_the_tilt() {
        let alpha = |d: &Drawn| d.image.to_rgba8().unwrap().as_slice().iter().map(|p| p.a as f64 / 255.0).sum::<f64>();
        let level = draw_bar(80.0, 2.0, 0.0, (0, 0)).unwrap();
        assert_eq!((level.width, level.height, level.left, level.top), (80, 2, -40, -1));
        let bar = draw_bar(80.0, 2.0, 1.45, (0, 0)).unwrap();
        // the ink of an 80 by 2 box less its round ends, whatever the tilt
        for d in [&level, &bar] {
            assert!((alpha(d) - 160.0).abs() < 2.0, "{} px of ink", alpha(d));
        }
        // 80 px along 1.45 degrees drops 2 px
        assert!((4..=5).contains(&bar.height), "2 px thick plus the 2 px drop, on whole pixels: {}", bar.height);
        let px = bar.image.to_rgba8().unwrap();
        // the ink's mean row in a column: 75 px further right it is 1.9 px lower
        let row_of = |x: u32| {
            let col: Vec<f64> = (0..bar.height).map(|y| px.as_slice()[(y * bar.width + x) as usize].a as f64).collect();
            col.iter().enumerate().map(|(y, a)| y as f64 * a).sum::<f64>() / col.iter().sum::<f64>()
        };
        let drop = row_of(bar.width - 3) - row_of(2);
        assert!((drop - 1.9).abs() < 0.3, "the right end is {drop:.2} px lower than the left");
        // a bullet is a dot
        let dot = draw_bar(6.0, 6.0, 1.45, (2, 2)).unwrap();
        assert!((alpha(&dot) - std::f64::consts::PI * 9.0).abs() < 1.0, "{} px of ink", alpha(&dot));
    }
}
