//! The font engine: reads font packages and makes Face sets (ADR 0003, `docs/adr/`).
//!
//! No UI, no window, no disk. Two entry points:
//!
//! - [`read_package`]: an untrusted Google Fonts `.zip` or a `.ttf` (as bytes) becomes a validated
//!   [`FontPackage`], or a [`PackageError`] with a plain reason.
//! - [`bake`]: a [`FontPackage`] at given [`AxisValues`] becomes a [`FaceSet`]: five static
//!   TrueType faces (Regular, Bold, Italic, Bold Italic, Caret) that keep the three properties
//!   `textflow` relies on (identical advances across the four text faces, no kerning or shaping
//!   tables, a blank Caret face with those advances), plus the [`FontMetrics`] the UI consumes.
//!
//! Nothing outside this module parses font files.

mod bake;
mod package;
mod source;

pub use bake::{bake, bake_regular, package_from_font, BakeError, FaceSet, FontMetrics, RegularFace, SAMPLE_UI_TEXT};
pub use package::{read_package, PackageError, MAX_ARCHIVE_BYTES, MAX_ENTRIES, MAX_FONT_BYTES};

/// Families Washi ships; an installed font may not take their names (nor start with "Washi ",
/// which the private family names of a Face set use).
pub const BUNDLED_FAMILIES: [&str; 4] = ["Shadows Into Light", "Gelasio", "Noto Serif", "Washi Caret"];

/// The largest lean, in degrees, the UI offers for a font without a slant axis (it is sheared).
pub const SHEAR_MAX_DEG: f32 = 12.0;

/// What one axis allows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisRange {
    pub min: f32,
    pub default: f32,
    pub max: f32,
}

/// The axes a font has. `None` means the font has no such axis. Weight is the usual 100..900
/// number, width a percentage (100 = normal). **Slant is lean in degrees, positive to the
/// right** (0 = upright): OpenType's `slnt` axis counts leaning right as negative, so this range
/// is the font's `slnt` range negated. A font with no `slnt` axis reports `None` and is sheared
/// by Washi instead (up to [`SHEAR_MAX_DEG`]); other axes (optical size, grade, ...) are ignored.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AxisRanges {
    pub weight: Option<AxisRange>,
    pub width: Option<AxisRange>,
    pub slant: Option<AxisRange>,
}

/// The values a Face set is made at, in the units of [`AxisRanges`]. `None` = the font's default
/// (for slant on a font without a slant axis: 0). Values outside the range are clamped.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AxisValues {
    pub weight: Option<f32>,
    pub width: Option<f32>,
    pub slant: Option<f32>,
}

/// A validated font, in memory.
#[derive(Clone, Debug)]
pub struct FontPackage {
    /// Family name from the name table (typographic family, else family).
    pub family: String,
    pub axes: AxisRanges,
    /// The font file Faces are made from: the variable file, or the Regular static file.
    pub regular: Vec<u8>,
    /// A static Bold file kept as a bold outline source. Only set when `regular` has no weight axis.
    pub bold: Option<Vec<u8>>,
    /// `OFL.txt` / `LICENSE.txt` from the zip.
    pub license: Option<Vec<u8>>,
}

// ---- The active font: what the note text layout and the UI theme read ----
//
// `textflow` and the `Theme` globals read the four text faces and every font-dependent number from
// one [`ActiveFont`], one per thread ([`current`]). The app replaces it with [`set_current`] and
// then rebuilds every note's `Layout` (`App::set_active_font`). The Default font (Shadows Into
// Light, bundled) is active until a Face set is activated with [`ActiveFont::from_face_set`].

use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;
use ttf_parser::Face;

const REGULAR: &[u8] = include_bytes!("../../../fonts/ShadowsIntoLight.ttf");
const BOLD: &[u8] = include_bytes!("../../../fonts/ShadowsIntoLight-Bold.ttf");
const ITALIC: &[u8] = include_bytes!("../../../fonts/ShadowsIntoLight-Italic.ttf");
const BOLD_ITALIC: &[u8] = include_bytes!("../../../fonts/ShadowsIntoLight-BoldItalic.ttf");

/// The Slint family names of the Default font.
pub const DEFAULT_TEXT_FAMILY: &str = "Shadows Into Light";
pub const DEFAULT_CARET_FAMILY: &str = "Washi Caret";

/// Index of a face in [`FaceData`]: regular, bold, italic, bold italic.
pub const fn face_index(bold: bool, italic: bool) -> usize {
    (bold as usize) | ((italic as usize) << 1)
}

/// The four text faces of a font, parsed once and kept: regular, bold, italic, bold italic.
pub struct FaceData {
    // Dropped first: the faces borrow from `_blobs`.
    faces: [Face<'static>; 4],
    _blobs: [Cow<'static, [u8]>; 4],
}

impl FaceData {
    /// Parse the four faces (regular, bold, italic, bold italic).
    pub fn new(blobs: [Cow<'static, [u8]>; 4]) -> Result<FaceData, ttf_parser::FaceParsingError> {
        let parse = |b: &Cow<'static, [u8]>| {
            // SAFETY: the slice lives in the heap block (or the static) `_blobs` owns, which does not
            // move when `FaceData` does and outlives `faces`; the 'static never leaves this type
            // (`face` hands out borrows of `self`).
            let bytes: &'static [u8] = unsafe { std::slice::from_raw_parts(b.as_ptr(), b.len()) };
            Face::parse(bytes, 0)
        };
        let faces = [parse(&blobs[0])?, parse(&blobs[1])?, parse(&blobs[2])?, parse(&blobs[3])?];
        Ok(FaceData { faces, _blobs: blobs })
    }

    pub fn face(&self, bold: bool, italic: bool) -> &Face<'_> {
        &self.faces[face_index(bold, italic)]
    }
}

/// The font-dependent numbers the UI sets on `Theme`, all in em. The room values are the
/// engine's measured [`FontMetrics`] rounded up to the next 0.05 em, plus a little.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeMetrics {
    /// Natural line height: (ascent + descent) / units per em (`Theme.handwriting-line-em`).
    pub line_em: f32,
    /// The advance of a space (`Theme.lead-em`).
    pub space_em: f32,
    /// How far a glyph reaches left of its origin at most, and a little (`Theme.lean-room`).
    pub lean_room_em: f32,
    /// Room below a line box for descenders (`Theme.descender-room`).
    pub descender_room_em: f32,
    /// How far an italic glyph leans out past its advance (`Theme.italic-overhang`).
    pub italic_overhang_em: f32,
}

/// Round up to the next 0.05 em.
fn room(em: f32) -> f32 {
    (em / 0.05 - 1e-3).ceil() * 0.05
}

impl ThemeMetrics {
    /// The Default font's numbers, as the UI hard-coded them before the font could change. A test
    /// checks that [`ThemeMetrics::of`] gives (nearly) these for the bundled faces; they are kept
    /// literal so that its pixels stay exactly as they were.
    pub const DEFAULT: ThemeMetrics = ThemeMetrics {
        line_em: 1.6064,
        space_em: 0.450195,
        lean_room_em: 0.2,
        descender_room_em: 0.5,
        italic_overhang_em: 0.3,
    };

    /// From what the engine measured on a baked Regular face.
    pub fn from_engine(m: &FontMetrics) -> ThemeMetrics {
        ThemeMetrics {
            line_em: m.line_em,
            space_em: m.space_em,
            lean_room_em: room(m.lean_em),
            descender_room_em: room(m.descent_em) + 0.05,
            italic_overhang_em: room(m.italic_overhang_em),
        }
    }

    /// Measure a regular face (the bundled Default font has no bake to ask).
    pub fn of(face: &Face) -> ThemeMetrics {
        let upem = face.units_per_em() as f32;
        let (mut left, mut down) = (0i16, 0i16);
        for g in 0..face.number_of_glyphs() {
            if let Some(b) = face.glyph_bounding_box(ttf_parser::GlyphId(g)) {
                left = left.min(b.x_min);
                down = down.min(b.y_min);
            }
        }
        let space = face.glyph_index(' ').and_then(|g| face.glyph_hor_advance(g)).unwrap_or(0);
        ThemeMetrics {
            line_em: (face.ascender() as f32 - face.descender() as f32 + face.line_gap() as f32) / upem,
            space_em: space as f32 / upem,
            lean_room_em: room(-left as f32 / upem),
            descender_room_em: room(-down as f32 / upem) + 0.05,
            // Washi's italic is a 0.25 shear (`tools/fonts.py slant`), and a little
            italic_overhang_em: ThemeMetrics::DEFAULT.italic_overhang_em,
        }
    }
}

/// The font the notes' text is laid out in and drawn with.
pub struct ActiveFont {
    /// `Theme.handwriting`: the family the words are drawn in.
    pub text_family: String,
    /// `Theme.caret-font`: the family of the edit fields, blank, with the words' advances.
    pub caret_family: String,
    pub faces: FaceData,
    pub metrics: ThemeMetrics,
    /// Differs for every font made by [`set_current`] (0 until then): layouts built for another
    /// font are stale (`Tokens::font_epoch`).
    pub epoch: u32,
}

impl ActiveFont {
    /// The bundled Shadows Into Light.
    pub fn default_font() -> ActiveFont {
        let blobs = [REGULAR, BOLD, ITALIC, BOLD_ITALIC].map(Cow::Borrowed);
        ActiveFont {
            text_family: DEFAULT_TEXT_FAMILY.into(),
            caret_family: DEFAULT_CARET_FAMILY.into(),
            faces: FaceData::new(blobs).expect("embedded font parses"),
            metrics: ThemeMetrics::DEFAULT,
            epoch: 0,
        }
    }

    /// A font from four face files and the families Slint knows them by. The metrics are measured
    /// from the regular face.
    pub fn new(text_family: &str, caret_family: &str, blobs: [Cow<'static, [u8]>; 4]) -> Result<ActiveFont, ttf_parser::FaceParsingError> {
        let faces = FaceData::new(blobs)?;
        let metrics = ThemeMetrics::of(faces.face(false, false));
        Ok(ActiveFont { text_family: text_family.into(), caret_family: caret_family.into(), faces, metrics, epoch: 0 })
    }

    /// The active font for a baked [`FaceSet`], already registered with Slint under its families.
    pub fn from_face_set(set: &FaceSet) -> Result<ActiveFont, ttf_parser::FaceParsingError> {
        let blobs = [&set.regular, &set.bold, &set.italic, &set.bold_italic].map(|b| Cow::Owned(b.clone()));
        Ok(ActiveFont {
            text_family: set.text_family.clone(),
            caret_family: set.caret_family.clone(),
            faces: FaceData::new(blobs)?,
            metrics: ThemeMetrics::from_engine(&set.metrics),
            epoch: 0,
        })
    }

    /// Set this font's families and numbers on a window's `Theme` and make Slint shape the window's
    /// text again (`Theme.text-epoch`). Every window has its own `Theme`, so this runs on each.
    pub fn apply(&self, theme: &crate::note_window::Theme<'_>) {
        theme.set_handwriting(self.text_family.as_str().into());
        theme.set_caret_font(self.caret_family.as_str().into());
        theme.set_handwriting_line_em(self.metrics.line_em);
        theme.set_lead_em(self.metrics.space_em);
        theme.set_lean_room(self.metrics.lean_room_em);
        theme.set_descender_room(self.metrics.descender_room_em);
        theme.set_italic_overhang(self.metrics.italic_overhang_em);
        theme.set_text_epoch(theme.get_text_epoch().wrapping_add(1));
    }
}

thread_local! {
    static CURRENT: RefCell<Rc<ActiveFont>> = RefCell::new(Rc::new(ActiveFont::default_font()));
}

/// The active font of this thread (the Default font until [`set_current`]).
pub fn current() -> Rc<ActiveFont> {
    CURRENT.with(|c| c.borrow().clone())
}

/// Make `font` the active font of this thread. Layouts built from now on use it; the ones already
/// built keep their faces until rebuilt (`App::set_active_font` does that for every window).
pub fn set_current(mut font: ActiveFont) -> Rc<ActiveFont> {
    font.epoch = CURRENT.with(|c| c.borrow().epoch).wrapping_add(1);
    let font = Rc::new(font);
    CURRENT.with(|c| *c.borrow_mut() = font.clone());
    font
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn the_metrics_measured_from_the_default_font_are_the_numbers_the_ui_used_to_hard_code() {
        let font = ActiveFont::default_font();
        let m = ThemeMetrics::of(font.faces.face(false, false));
        let d = ThemeMetrics::DEFAULT;
        assert!(close(m.line_em, d.line_em), "line {} vs {}", m.line_em, d.line_em);
        assert!(close(m.space_em, d.space_em), "space {} vs {}", m.space_em, d.space_em);
        assert_eq!(m.lean_room_em, d.lean_room_em, "the j reaches 176/1024 em left");
        assert!((m.descender_room_em - d.descender_room_em).abs() < 1e-5, "descender {}", m.descender_room_em);
        assert_eq!(m.italic_overhang_em, d.italic_overhang_em);
        // and the literals are what the Slint theme shipped with
        assert_eq!((d.line_em, d.space_em), (1.6064, 0.450195));
    }

    #[test]
    fn the_default_font_is_current_until_another_is_set_and_comes_back_when_asked() {
        assert_eq!(current().text_family, DEFAULT_TEXT_FAMILY);
        let other = ActiveFont::new("Gelasio", "Washi Caret", [GELASIO, GELASIO_BOLD, GELASIO, GELASIO_BOLD].map(Cow::Borrowed)).unwrap();
        assert!(other.metrics.space_em != ThemeMetrics::DEFAULT.space_em);
        set_current(other);
        assert_eq!(current().text_family, "Gelasio");
        set_current(ActiveFont::default_font());
        assert_eq!(current().text_family, DEFAULT_TEXT_FAMILY);
    }

    pub const GELASIO: &[u8] = include_bytes!("../../../fonts/Gelasio-Regular.ttf");
    pub const GELASIO_BOLD: &[u8] = include_bytes!("../../../fonts/Gelasio-Bold.ttf");
}
