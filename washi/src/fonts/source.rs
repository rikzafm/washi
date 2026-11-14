//! Reading one font file: family name, outline kind, Latin coverage and the axes.

use super::{AxisRange, AxisRanges, PackageError};
use skrifa::raw::TableProvider;
use skrifa::string::StringId;
use skrifa::{FontRef, MetadataProvider, Tag};

pub(crate) struct Source {
    pub family: String,
    pub axes: AxisRanges,
}

/// Parse and validate a single font file: TrueType outlines and basic Latin.
pub(crate) fn inspect(bytes: &[u8]) -> Result<Source, PackageError> {
    if bytes.starts_with(b"OTTO") {
        return Err(PackageError::Cff);
    }
    let font = FontRef::new(bytes).map_err(|_| PackageError::NotAFont)?;
    if font.table_data(Tag::new(b"glyf")).is_none() || font.table_data(Tag::new(b"loca")).is_none() {
        return Err(if font.table_data(Tag::new(b"CFF ")).is_some() || font.table_data(Tag::new(b"CFF2")).is_some() {
            PackageError::Cff
        } else {
            PackageError::NotAFont
        });
    }
    // The tables Washi reads must be there and sane.
    if font.head().is_err() || font.maxp().is_err() || font.hhea().is_err() || font.hmtx().is_err() {
        return Err(PackageError::NotAFont);
    }
    let cmap = font.charmap();
    let latin = std::iter::once(' ').chain('0'..='9').chain('A'..='Z').chain('a'..='z');
    if latin.into_iter().any(|c| cmap.map(c).is_none_or(|g| g.to_u32() == 0)) {
        return Err(PackageError::NoLatin);
    }
    let family = [StringId::TYPOGRAPHIC_FAMILY_NAME, StringId::FAMILY_NAME]
        .into_iter()
        .find_map(|id| font.localized_strings(id).english_or_first().map(|s| s.to_string()))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or(PackageError::NoName)?;
    Ok(Source { family, axes: axes_of(&font) })
}

pub(crate) fn axes_of(font: &FontRef) -> AxisRanges {
    let mut out = AxisRanges::default();
    for a in font.axes().iter() {
        let r = AxisRange { min: a.min_value(), default: a.default_value(), max: a.max_value() };
        match &a.tag().to_be_bytes() {
            b"wght" => out.weight = Some(r),
            b"wdth" => out.width = Some(r),
            // lean to the right is negative on `slnt`
            b"slnt" => out.slant = Some(AxisRange { min: -r.max, default: -r.default, max: -r.min }),
            _ => {}
        }
    }
    out
}

pub(crate) fn has_fvar(bytes: &[u8]) -> bool {
    FontRef::new(bytes).map(|f| f.table_data(Tag::new(b"fvar")).is_some()).unwrap_or(false)
}

/// True when the file is an italic design (flags, angle, or an `ital` axis fixed on).
pub(crate) fn is_italic(bytes: &[u8]) -> bool {
    let Ok(f) = FontRef::new(bytes) else { return false };
    f.os2().map(|o| o.fs_selection().contains(skrifa::raw::tables::os2::SelectionFlags::ITALIC)).unwrap_or(false)
        || f.head().map(|h| h.mac_style().contains(skrifa::raw::tables::head::MacStyle::ITALIC)).unwrap_or(false)
        || f.post().map(|p| p.italic_angle().to_f64() != 0.0).unwrap_or(false)
}
