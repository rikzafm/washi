//! Reading a font package: an untrusted `.zip` (as Google Fonts serves it) or a bare `.ttf`.
//!
//! Everything stays in memory. Entry names are only looked at to choose files; they are never
//! turned into paths.

use super::source::{has_fvar, inspect, is_italic};
use super::{FontPackage, BUNDLED_FAMILIES};
use std::io::{Cursor, Read};

/// Largest archive (or font file) accepted.
pub const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
/// Most entries an archive may hold.
pub const MAX_ENTRIES: usize = 600;
/// Largest font entry, uncompressed.
pub const MAX_FONT_BYTES: usize = 24 * 1024 * 1024;
/// Most font bytes read from one archive while looking for the right file.
const MAX_READ_TOTAL: usize = 128 * 1024 * 1024;
const MAX_LICENSE_BYTES: usize = 1024 * 1024;

/// Why a package was refused. `Display` is the plain reason shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageError {
    /// Not a font, not a zip, or damaged.
    NotAFont,
    /// A zip with no `.ttf` in it.
    NoFont,
    /// More than one `.ttf` and none of them is the Regular style.
    NoRegular,
    /// Over a cap; says which.
    TooLarge(&'static str),
    /// A compression method other than deflate or stored, or a password-protected entry.
    Unsupported,
    /// OpenType with CFF outlines (`.otf`): not supported in this version.
    Cff,
    /// Missing basic Latin (A-Z, a-z, 0-9, space).
    NoLatin,
    /// The font has no family name.
    NoName,
    /// The family is named like a font Washi ships.
    NameClash(String),
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageError::NotAFont => f.write_str("This file is not a usable font."),
            PackageError::NoFont => f.write_str("There is no .ttf font file in this zip."),
            PackageError::NoRegular => f.write_str("Could not tell which file in this zip is the regular style."),
            PackageError::TooLarge(what) => write!(f, "This font package is too large ({what})."),
            PackageError::Unsupported => f.write_str("This zip uses a compression or protection Washi cannot read."),
            PackageError::Cff => f.write_str("This font uses OpenType (CFF) outlines, which Washi does not support yet. Use the .ttf version."),
            PackageError::NoLatin => f.write_str("This font is missing basic Latin letters, digits or the space."),
            PackageError::NoName => f.write_str("This font has no family name."),
            PackageError::NameClash(n) => write!(f, "A font named \"{n}\" is already part of Washi."),
        }
    }
}

impl std::error::Error for PackageError {}

/// Read a `.zip` or a single font file. A zip is recognised by its signature, not by any name.
pub fn read_package(bytes: &[u8]) -> Result<FontPackage, PackageError> {
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(PackageError::TooLarge("over 64 MB"));
    }
    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        read_zip(bytes)
    } else {
        if bytes.len() > MAX_FONT_BYTES {
            return Err(PackageError::TooLarge("a font over 24 MB"));
        }
        let src = inspect(bytes)?;
        clash(&src.family)?;
        Ok(FontPackage { family: src.family, axes: src.axes, regular: bytes.to_vec(), bold: None, license: None })
    }
}

fn clash(family: &str) -> Result<(), PackageError> {
    let f = family.trim();
    if f.to_lowercase().starts_with("washi ") || BUNDLED_FAMILIES.iter().any(|b| b.eq_ignore_ascii_case(f)) {
        return Err(PackageError::NameClash(f.to_string()));
    }
    Ok(())
}

fn base_name(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

fn stem_lower(base: &str) -> String {
    let lower = base.to_lowercase();
    lower.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or(lower)
}

struct Entry {
    idx: usize,
    name: String,
    stem: String,
    root: bool,
}

struct Zip<'a> {
    archive: zip::ZipArchive<Cursor<&'a [u8]>>,
    budget: usize,
    cache: std::collections::HashMap<usize, Vec<u8>>,
}

impl Zip<'_> {
    fn read(&mut self, idx: usize, cap: usize, cap_msg: &'static str) -> Result<&Vec<u8>, PackageError> {
        if !self.cache.contains_key(&idx) {
            let mut f = self.archive.by_index(idx).map_err(|e| match e {
                zip::result::ZipError::UnsupportedArchive(_) => PackageError::Unsupported,
                _ => PackageError::NotAFont,
            })?;
            if !matches!(f.compression(), zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated) {
                return Err(PackageError::Unsupported);
            }
            if f.size() > cap as u64 {
                return Err(PackageError::TooLarge(cap_msg));
            }
            // The declared size is not trusted: stop reading at the cap.
            let mut out = Vec::new();
            (&mut f).take(cap as u64 + 1).read_to_end(&mut out).map_err(|_| PackageError::NotAFont)?;
            if out.len() > cap {
                return Err(PackageError::TooLarge(cap_msg));
            }
            self.budget = self.budget.checked_sub(out.len()).ok_or(PackageError::TooLarge("too much font data"))?;
            self.cache.insert(idx, out);
        }
        Ok(&self.cache[&idx])
    }
}

fn read_zip(bytes: &[u8]) -> Result<FontPackage, PackageError> {
    let archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| PackageError::NotAFont)?;
    if archive.len() > MAX_ENTRIES {
        return Err(PackageError::TooLarge("over 600 files"));
    }
    let mut zip = Zip { archive, budget: MAX_READ_TOTAL, cache: Default::default() };

    let mut fonts: Vec<Entry> = Vec::new();
    let mut licenses: Vec<Entry> = Vec::new();
    for idx in 0..zip.archive.len() {
        let f = zip.archive.by_index_raw(idx).map_err(|_| PackageError::NotAFont)?;
        if f.is_dir() {
            continue;
        }
        let name = f.name().to_string();
        let base = base_name(&name).to_string();
        if base.is_empty() || base.starts_with("._") || name.starts_with("__MACOSX") {
            continue;
        }
        let lower = base.to_lowercase();
        let e = Entry { idx, root: !name.contains(['/', '\\']), stem: stem_lower(&base), name };
        if lower.ends_with(".ttf") || lower.ends_with(".otf") {
            fonts.push(e);
        } else if ["ofl.txt", "license.txt", "licence.txt", "ofl", "license"].contains(&lower.as_str()) {
            licenses.push(e);
        }
    }
    if fonts.is_empty() {
        return Err(PackageError::NoFont);
    }

    // 1. an upright variable file: Google's `Family-VariableFont_*.ttf` or `Family[wght].ttf`
    //    first, then any other upright file with an `fvar` table.
    let looks_variable = |e: &Entry| e.stem.contains("variablefont") || e.name.contains('[');
    let mut order: Vec<usize> = (0..fonts.len()).filter(|&i| !fonts[i].stem.contains("italic")).collect();
    order.sort_by_key(|&i| (!looks_variable(&fonts[i]), !fonts[i].root, fonts[i].name.len()));
    let mut chosen: Option<usize> = None;
    for &i in &order {
        let bytes = zip.read(fonts[i].idx, MAX_FONT_BYTES, "a font over 24 MB")?;
        if has_fvar(bytes) && !is_italic(bytes) {
            chosen = Some(i);
            break;
        }
    }
    // 2. else the Regular static file, else the only font
    if chosen.is_none() {
        let mut regs: Vec<usize> = (0..fonts.len()).filter(|&i| fonts[i].stem.ends_with("-regular")).collect();
        regs.sort_by_key(|&i| fonts[i].name.len());
        chosen = regs.first().copied().or(if fonts.len() == 1 { Some(0) } else { None });
    }
    let chosen = chosen.ok_or(PackageError::NoRegular)?;
    let regular = zip.read(fonts[chosen].idx, MAX_FONT_BYTES, "a font over 24 MB")?.clone();
    let src = inspect(&regular)?;
    clash(&src.family)?;

    // 3. a Bold static file of the same width, when there is no weight axis to make bold with
    let mut bold = None;
    if src.axes.weight.is_none() {
        let prefix = {
            let s = &fonts[chosen].stem;
            let cut = ["-variablefont", "-regular", "["].iter().filter_map(|m| s.find(m)).min().unwrap_or(s.len());
            s[..cut].to_string()
        };
        let bolds: Vec<usize> = (0..fonts.len()).filter(|&i| fonts[i].stem.ends_with("-bold")).collect();
        let pick = bolds
            .iter()
            .copied()
            .find(|&i| fonts[i].stem.strip_suffix("-bold") == Some(prefix.as_str()))
            .or(if bolds.len() == 1 { Some(bolds[0]) } else { None });
        if let Some(i) = pick {
            if let Ok(b) = zip.read(fonts[i].idx, MAX_FONT_BYTES, "a font over 24 MB") {
                // a Bold of another family, or one we cannot use, is simply not used
                if matches!(inspect(b), Ok(s) if s.family == src.family) {
                    bold = Some(b.clone());
                }
            }
        }
    }

    licenses.sort_by_key(|e| (!e.root, e.name.len()));
    let license = match licenses.first() {
        Some(l) => zip.read(l.idx, MAX_LICENSE_BYTES, "a licence file over 1 MB").ok().cloned(),
        None => None,
    };

    Ok(FontPackage { family: src.family, axes: src.axes, regular, bold, license })
}
