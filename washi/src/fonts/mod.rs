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
