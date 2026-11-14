//! Throwaway font spikes (ADR 0003). Shared plumbing: a headless software-rendered window.
slint::include_modules!();

pub mod bake;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, Rgb8Pixel, SharedPixelBuffer};
use std::cell::RefCell;
use std::rc::Rc;

thread_local! { static PENDING: RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(None) }; }

struct P(std::time::Instant);
impl Platform for P {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        PENDING
            .with(|p| p.borrow_mut().take())
            .map(|w| w as Rc<dyn WindowAdapter>)
            .ok_or_else(|| slint::PlatformError::Other("no pending window".into()))
    }
    fn duration_since_start(&self) -> std::time::Duration {
        self.0.elapsed()
    }
}

pub fn install_headless() {
    slint::platform::set_platform(Box::new(P(std::time::Instant::now()))).unwrap();
}

pub struct Rig {
    pub win: Rc<MinimalSoftwareWindow>,
    pub probe: Probe,
}

impl Rig {
    pub fn new() -> Rig {
        let win = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        PENDING.with(|p| *p.borrow_mut() = Some(win.clone()));
        let probe = Probe::new().unwrap();
        win.set_size(slint::PhysicalSize::new(760, 200));
        probe.show().unwrap();
        Rig { win, probe }
    }

    /// Render the current state: (top half = Text, bottom half = TextInput) as RGB.
    pub fn render(&self) -> Shot {
        slint::platform::update_timers_and_animations();
        self.win.request_redraw();
        let mut buf = SharedPixelBuffer::<Rgb8Pixel>::new(760, 200);
        let stride = 760;
        let drawn = self.win.draw_if_needed(|r| {
            r.render(buf.make_mut_slice(), stride);
        });
        assert!(drawn, "window did not draw");
        Shot { w: 760, h: 200, px: buf.as_slice().iter().map(|p| p.r).collect() }
    }
}

impl Default for Rig {
    fn default() -> Self {
        Self::new()
    }
}

/// One red-channel plane (the text is black on white).
pub struct Shot {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Shot {
    fn band(&self, y0: usize, y1: usize) -> impl Iterator<Item = (usize, usize, u8)> + '_ {
        (y0..y1).flat_map(move |y| (0..self.w).map(move |x| (x, y, self.px[y * self.w + x])))
    }
    /// A hash of a horizontal band: same pixels, same hash.
    pub fn hash(&self, y0: usize, y1: usize) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        for (_, _, v) in self.band(y0, y1) {
            h ^= v as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
    /// The ink bounding box (x0, x1) of a band, ink = darker than 160.
    pub fn ink_x(&self, y0: usize, y1: usize) -> Option<(usize, usize)> {
        let xs: Vec<usize> = self.band(y0, y1).filter(|&(_, _, v)| v < 160).map(|(x, _, _)| x).collect();
        Some((*xs.iter().min()?, *xs.iter().max()?))
    }
    pub fn ink_count(&self, y0: usize, y1: usize) -> usize {
        self.band(y0, y1).filter(|&(_, _, v)| v < 160).count()
    }
    /// Write a grey PNG so a human can look at it.
    pub fn save_pgm(&self, path: &str) {
        let path = path.replace(".pgm", ".png");
        let f = std::fs::File::create(path).unwrap();
        let mut e = png::Encoder::new(std::io::BufWriter::new(f), self.w as u32, self.h as u32);
        e.set_color(png::ColorType::Grayscale);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&self.px).unwrap();
    }
}

pub const TOP: (usize, usize) = (0, 95);
pub const BOTTOM: (usize, usize) = (95, 200);

/// Sum of advances for `text` at `px` from a font file, via ttf-parser (cmap + hmtx; no kerning).
pub fn advance_sum(bytes: &[u8], text: &str, px: f32) -> f32 {
    let f = ttf_parser::Face::parse(bytes, 0).unwrap();
    let s = px / f.units_per_em() as f32;
    text.chars().map(|c| f.glyph_index(c).and_then(|g| f.glyph_hor_advance(g)).unwrap_or(0) as f32 * s).sum()
}
