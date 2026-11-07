//! Text rendering with Slint's software renderer, the renderer the app will use.
//!
//! Every font file is registered under its own family name (the file stem) with weight 400 and
//! normal style, so a candidate is always picked by file, never by family heuristics. The weight
//! and slant of the file itself are baked into its outlines: the renderer cannot fake them.

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, PhysicalSize, PlatformError, Rgb8Pixel};
use slint::fontique_011::fontique;
use std::path::{Path, PathBuf};
use std::rc::Rc;

slint::slint! {
    export component Probe inherits Window {
        in property <string> label;
        in property <string> face;
        in property <length> size;
        in property <length> ox;
        in property <length> oy;
        in property <length> ls;
        background: white;
        Text {
            x: 8px + root.ox;
            y: 8px + root.oy;
            text: root.label;
            font-family: root.face;
            font-size: root.size;
            font-weight: 400;
            font-italic: false;
            color: black;
            letter-spacing: root.ls;
            wrap: no-wrap;
        }
    }
}

/// A candidate font file registered with the renderer.
#[derive(Clone, Debug)]
pub struct FontFace {
    pub path: PathBuf,
    /// Family name the renderer knows this file by (the file stem).
    pub family: String,
    /// Weight and slant as named by the file (Bold / Italic in the file name).
    pub bold: bool,
    pub italic: bool,
}

/// Ink coverage of a rendered string: 0.0 = paper, 1.0 = full ink.
#[derive(Clone, Debug)]
pub struct Coverage {
    pub w: usize,
    pub h: usize,
    pub a: Vec<f32>,
}

impl Coverage {
    /// Bounding box [x0, y0, x1, y1) of pixels with coverage >= `thr`.
    pub fn ink_bbox(&self, thr: f32) -> Option<(usize, usize, usize, usize)> {
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        for y in 0..self.h {
            for x in 0..self.w {
                if self.a[y * self.w + x] >= thr {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        (x1 > x0).then_some((x0, y0, x1, y1))
    }
}

/// Register every .ttf in `dir` with the renderer. Sorted by file name.
pub fn load_fonts(dir: &Path) -> Result<Vec<FontFace>, String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("ttf")))
        .collect();
    paths.sort();
    platform_window();
    let mut collection = slint::fontique_011::shared_collection();
    let mut faces = vec![];
    for path in paths {
        let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let blob = fontique::Blob::new(std::sync::Arc::new(data));
        let over = fontique::FontInfoOverride {
            family_name: Some(&stem),
            style: Some(fontique::FontStyle::Normal),
            weight: Some(fontique::FontWeight::NORMAL),
            ..Default::default()
        };
        if collection.register_fonts(blob, Some(over)).is_empty() {
            return Err(format!("{}: not a usable font", path.display()));
        }
        let lower = stem.to_lowercase();
        faces.push(FontFace { path, family: stem, bold: lower.contains("bold"), italic: lower.contains("italic") });
    }
    Ok(faces)
}

struct HeadlessPlatform(Rc<MinimalSoftwareWindow>);

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

thread_local! {
    static WINDOW: Rc<MinimalSoftwareWindow> = {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        slint::platform::set_platform(Box::new(HeadlessPlatform(window.clone()))).expect("set platform");
        window
    };
}

/// The headless window of this thread; installs the platform on first use (it must exist
/// before the font collection can be reached).
fn platform_window() -> Rc<MinimalSoftwareWindow> {
    WINDOW.with(|w| w.clone())
}

/// Renders single lines of text at scale factor 1 (one logical px = one pixel).
pub struct TextRenderer {
    window: Rc<MinimalSoftwareWindow>,
    probe: Probe,
}

impl TextRenderer {
    /// One renderer per thread: Slint's platform is thread-global.
    pub fn new() -> Self {
        let window = platform_window();
        let probe = Probe::new().expect("create probe");
        probe.show().expect("show probe");
        TextRenderer { window, probe }
    }

    /// Black text on white, top-left at (8, 8). `px` is the font size in pixels.
    pub fn render(&mut self, face: &FontFace, text: &str, px: f32) -> Coverage {
        self.render_at(face, text, px, 0.0, 0.0, 0.0)
    }

    /// Like [`render`](Self::render) with the text moved by a fractional offset and `ls` px of
    /// extra letter spacing.
    pub fn render_at(&mut self, face: &FontFace, text: &str, px: f32, ox: f32, oy: f32, ls: f32) -> Coverage {
        // generous canvas: text is at most ~0.62 em per char wide and 1.6 em tall
        let w = (text.chars().count() as f32 * (px * 0.75 + ls.max(0.0)) + 32.0).ceil() as u32;
        let h = (px * 2.2 + 32.0).ceil() as u32;
        self.window.set_size(PhysicalSize::new(w, h));
        self.probe.set_label(text.into());
        self.probe.set_face(face.family.as_str().into());
        self.probe.set_size(px);
        self.probe.set_ox(ox);
        self.probe.set_oy(oy);
        self.probe.set_ls(ls);
        let mut buf = vec![Rgb8Pixel { r: 255, g: 255, b: 255 }; (w * h) as usize];
        let mut drawn = false;
        // setting the same property values again does not mark the window dirty
        self.window.request_redraw();
        self.window.draw_if_needed(|renderer| {
            renderer.render(&mut buf, w as usize);
            drawn = true;
        });
        assert!(drawn, "the software renderer did not draw");
        Coverage { w: w as usize, h: h as usize, a: buf.iter().map(|p| 1.0 - p.g as f32 / 255.0).collect() }
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}
