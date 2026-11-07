//! Screenshots of the real note component: the same app code path as the product, on the stand-in
//! platform, composited on the desk color at the size of the mockup canvas.

use crate::{platform, Harness};
use slint::platform::software_renderer::PremultipliedRgbaColor;
use slint::platform::{WindowAdapter, WindowEvent};
use slint::{LogicalPosition, PhysicalSize};
use std::time::Duration;

/// Transparent margin around the paper inside the window, in logical px (spec: layout.window_margin).
pub const WINDOW_MARGIN: f64 = 42.0;

pub struct Options {
    /// Content of the note.
    pub text: String,
    /// Physical pixels per logical pixel.
    pub scale: f64,
    /// Paper size in logical px; the window is the paper plus a margin on every side.
    pub paper: (f64, f64),
    /// Move a real pointer over the note before drawing.
    pub hover: bool,
    /// The note has the keyboard focus (level text fields, caret shown) instead of resting
    /// (words on the tilted baseline).
    pub editing: bool,
    /// Output size in physical px (the mockup canvas).
    pub canvas: (u32, u32),
    pub desk: [u8; 3],
}

impl Default for Options {
    fn default() -> Self {
        Options { text: String::new(), scale: 1.5, paper: (363.0, 384.0), hover: false, editing: false, canvas: (720, 840), desk: [0xD9, 0xDD, 0xE1] }
    }
}

pub struct Snapshot {
    pub width: u32,
    pub height: u32,
    rgb: Vec<u8>,
}

impl Snapshot {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * self.width + x) * 3) as usize;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }

    pub fn rgb(&self) -> &[u8] {
        &self.rgb
    }

    /// PNG bytes (8-bit RGB, default encoder settings: the same pixels always give the same bytes).
    pub fn png(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, self.width, self.height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("png header");
        w.write_image_data(&self.rgb).expect("png data");
        drop(w);
        out
    }
}

/// Render one note.
pub fn snapshot(o: &Options) -> Result<Snapshot, String> {
    let mut h = Harness::new();
    h.start().map_err(|e| e.to_string())?;
    h.set_note_text(0, &o.text);

    let adapter = platform::windows().pop().ok_or("the note has no window")?;
    let win = adapter.window();
    let (lw, lh) = (o.paper.0 + 2.0 * WINDOW_MARGIN, o.paper.1 + 2.0 * WINDOW_MARGIN);
    let (pw, ph) = ((lw * o.scale).round() as u32, (lh * o.scale).round() as u32);
    win.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: o.scale as f32 });
    win.set_size(PhysicalSize::new(pw, ph));
    // Lay the note out first (rows of the text blocks appear on the first draw and focus their
    // field), then show it unfocused: no caret, like the mockups.
    win.request_redraw();
    adapter.draw_if_needed(|r| {
        r.render(&mut vec![PremultipliedRgbaColor::default(); (pw * ph) as usize], pw as usize);
    });
    win.dispatch_event(WindowEvent::WindowActiveChanged(o.editing));
    // Slint animations start when first drawn: let the focus change through, draw once, then wait
    // for the text to finish settling (flat when editing, tilted at rest).
    h.advance(Duration::from_millis(2));
    win.request_redraw();
    adapter.draw_if_needed(|r| {
        r.render(&mut vec![PremultipliedRgbaColor::default(); (pw * ph) as usize], pw as usize);
    });
    if o.hover {
        win.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new((lw / 2.0) as f32, (lh / 2.0) as f32) });
    }
    h.advance(Duration::from_millis(300)); // let any hover fade finish

    let mut buf = vec![PremultipliedRgbaColor::default(); (pw * ph) as usize];
    win.request_redraw();
    adapter.draw_if_needed(|r| {
        r.render(&mut buf, pw as usize);
    });

    // composite over the desk, centred on the canvas
    let (cw, ch) = o.canvas;
    let mut rgb = Vec::with_capacity((cw * ch * 3) as usize);
    let (ox, oy) = ((cw as i64 - pw as i64) / 2, (ch as i64 - ph as i64) / 2);
    for y in 0..ch as i64 {
        for x in 0..cw as i64 {
            let (sx, sy) = (x - ox, y - oy);
            if sx >= 0 && sy >= 0 && sx < pw as i64 && sy < ph as i64 {
                let p = buf[(sy as u32 * pw + sx as u32) as usize];
                let inv = 255 - p.alpha as u32;
                let over = |src: u8, desk: u8| (src as u32 + (desk as u32 * inv + 127) / 255).min(255) as u8;
                rgb.extend_from_slice(&[over(p.red, o.desk[0]), over(p.green, o.desk[1]), over(p.blue, o.desk[2])]);
            } else {
                rgb.extend_from_slice(&o.desk);
            }
        }
    }
    Ok(Snapshot { width: cw, height: ch, rgb })
}
