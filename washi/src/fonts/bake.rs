//! Baking: a [`FontPackage`] at given Axis values becomes a Face set of five static TrueType faces.
//!
//! skrifa draws every glyph at the chosen variation location (composites come out flattened) and
//! write-fonts writes plain `glyf` faces: `OS/2 cmap glyf head hhea hmtx loca maxp name post` and
//! nothing else, so there is no kerning, positioning or substitution table and shaped advances
//! equal `hmtx` advances. See ADR 0003 (S2) for what was measured.

use super::package::PackageError;
use super::source::{axes_of, inspect};
use super::{AxisRanges, AxisValues, FontPackage, SHEAR_MAX_DEG};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::tables::glyf::CurvePoint;
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};
use std::sync::atomic::{AtomicU32, Ordering};
use write_fonts::tables::cmap::Cmap;
use write_fonts::tables::glyf::{Bbox, Contour, GlyfLocaBuilder, SimpleGlyph};
use write_fonts::tables::head::{Head, MacStyle};
use write_fonts::tables::hhea::Hhea;
use write_fonts::tables::hmtx::{Hmtx, LongMetric};
use write_fonts::tables::maxp::Maxp;
use write_fonts::tables::name::{Name, NameRecord};
use write_fonts::tables::os2::{Os2, SelectionFlags};
use write_fonts::tables::post::Post;
use write_fonts::types::{FWord, Fixed, NameId, UfWord};
use write_fonts::FontBuilder;

/// The step Bold adds on a weight axis.
const BOLD_STEP: f32 = 300.0;
/// The further lean Italic adds on top of the Regular's (tan of the angle, as `tools/fonts.py slant`).
const ITALIC_SHEAR: f32 = 0.25;
/// The Caret face's cut-down ascent and descent, in 1/1024 em (as `tools/fonts.py blank`).
const CARET_ASCENT: f32 = 861.0;
const CARET_DESCENT: f32 = 100.0;

/// One fixed sample of UI labels: `avg_advance_em` is measured over it.
pub const SAMPLE_UI_TEXT: &str = "Settings Appearance Font & Style Shortcuts Privacy New note Delete Pin on top \
    Tape color Letter spacing Font size Installed fonts Weight Width Slant All notes Cancel Remove";

/// Why a Face set could not be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BakeError {
    /// The font could not be read (should not happen to a validated package).
    Unreadable,
    /// An outline with cubic curves (CFF): refused when reading, kept here as a guard.
    CubicOutline,
    /// A table could not be written.
    Write(String),
}

impl std::fmt::Display for BakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BakeError::Unreadable => f.write_str("The font could not be read."),
            BakeError::CubicOutline => f.write_str("This font uses OpenType (CFF) outlines, which Washi does not support yet."),
            BakeError::Write(e) => write!(f, "The font could not be prepared: {e}"),
        }
    }
}

impl std::error::Error for BakeError {}

/// Everything the layout and the UI need to know about a baked Regular face, in em.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    /// (ascent + descent) / units per em, from the Regular face's `hhea`.
    pub line_em: f32,
    /// The advance of the space.
    pub space_em: f32,
    /// The furthest any glyph reaches left of its origin.
    pub lean_em: f32,
    /// The deepest any glyph reaches below the baseline.
    pub descent_em: f32,
    /// How far the Italic leans past a glyph's advance, at most (right side).
    pub italic_overhang_em: f32,
    /// Mean advance of the characters of [`SAMPLE_UI_TEXT`] the font has.
    pub avg_advance_em: f32,
}

/// The five faces of a font at some Axis values. Each Face set has a fresh private family name
/// (`Washi Text <n>`, `Washi Caret <n>`): register Regular, Bold, Italic and Bold Italic under
/// `text_family` with weights 400 / 700 and normal / italic style, and Caret under
/// `caret_family`.
#[derive(Clone, Debug)]
pub struct FaceSet {
    pub regular: Vec<u8>,
    pub bold: Vec<u8>,
    pub italic: Vec<u8>,
    pub bold_italic: Vec<u8>,
    pub caret: Vec<u8>,
    pub text_family: String,
    pub caret_family: String,
    pub metrics: FontMetrics,
}

/// Regular only: what a slider drag needs.
#[derive(Clone, Debug)]
pub struct RegularFace {
    pub face: Vec<u8>,
    pub family: String,
    pub metrics: FontMetrics,
}

static NEXT_FAMILY: AtomicU32 = AtomicU32::new(1);

fn fresh_families() -> (String, String) {
    let n = NEXT_FAMILY.fetch_add(1, Ordering::Relaxed);
    (format!("Washi Text {n}"), format!("Washi Caret {n}"))
}

/// A package from a font file already on hand (the bundled Default font), without the name check
/// that keeps installed fonts from clashing with bundled ones.
pub fn package_from_font(bytes: Vec<u8>) -> Result<FontPackage, PackageError> {
    let src = inspect(&bytes)?;
    Ok(FontPackage { family: src.family, axes: src.axes, regular: bytes, bold: None, license: None })
}

type Pts = Vec<Vec<(f32, f32, bool)>>;

#[derive(Default)]
struct Collect {
    contours: Pts,
    cur: Vec<(f32, f32, bool)>,
    cubic: bool,
}

impl Collect {
    fn flush(&mut self) {
        let mut c = std::mem::take(&mut self.cur);
        if c.len() >= 2 && c.first().map(|p| (p.0, p.1)) == c.last().map(|p| (p.0, p.1)) {
            c.pop();
        }
        if c.len() >= 3 {
            self.contours.push(c);
        }
    }
}

impl OutlinePen for Collect {
    fn move_to(&mut self, x: f32, y: f32) {
        self.flush();
        self.cur.push((x, y, true));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.cur.push((x, y, true));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.cur.push((cx, cy, false));
        self.cur.push((x, y, true));
    }
    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
        self.cubic = true;
    }
    fn close(&mut self) {
        self.flush();
    }
}

type Location = skrifa::instance::Location;

/// A variation location from the Axis values. `lean` is degrees to the right; the font's own
/// slant axis counts the other way round. `weight_add` is added to the weight (Bold).
fn location(font: &FontRef, axes: &AxisRanges, v: AxisValues, weight_add: f32) -> Location {
    let mut list: Vec<(&str, f32)> = Vec::new();
    if let Some(r) = axes.weight {
        list.push(("wght", (v.weight.unwrap_or(r.default) + weight_add).clamp(r.min, r.max)));
    }
    if let Some(r) = axes.width {
        list.push(("wdth", v.width.unwrap_or(r.default).clamp(r.min, r.max)));
    }
    if let Some(r) = axes.slant {
        list.push(("slnt", -v.slant.unwrap_or(r.default).clamp(r.min, r.max)));
    }
    font.axes().location(list)
}

fn outline_of(font: &FontRef, loc: &Location, gid: GlyphId) -> Result<Pts, BakeError> {
    let mut pen = Collect::default();
    if let Some(g) = font.outline_glyphs().get(gid) {
        let _ = g.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::from(loc)), &mut pen);
        pen.flush();
    }
    if pen.cubic {
        return Err(BakeError::CubicOutline);
    }
    Ok(pen.contours)
}

fn advance_of(font: &FontRef, loc: &Location, gid: GlyphId) -> f32 {
    font.glyph_metrics(Size::unscaled(), LocationRef::from(loc)).advance_width(gid).unwrap_or(0.0)
}

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Regular,
    Bold,
    Italic,
    BoldItalic,
    Caret,
}

struct Plan<'a> {
    family: &'a str,
    style: Style,
    /// x += shear * y
    shear: f32,
    weight_class: u16,
    /// ascent and descent for `hhea` / `OS/2`
    ascent: i16,
    descent: i16,
}

fn write_face(src: &FontRef, outlines: &[Pts], advances: &[u16], upem: u16, plan: Plan) -> Result<Vec<u8>, BakeError> {
    let werr = |e: &dyn std::fmt::Debug| BakeError::Write(format!("{e:?}"));
    let n = advances.len();
    let mut glyf_b = GlyfLocaBuilder::new();
    let mut metrics = Vec::with_capacity(n);
    let (mut max_pts, mut max_cont) = (0u16, 0u16);
    let (mut bx0, mut by0, mut bx1, mut by1) = (i16::MAX, i16::MAX, i16::MIN, i16::MIN);
    for (gid, outline) in outlines.iter().enumerate() {
        let contours: Vec<Contour> = outline
            .iter()
            .map(|c| {
                Contour::from(
                    c.iter()
                        .map(|&(x, y, on)| {
                            let (xr, yr) = ((x + plan.shear * y).round() as i16, y.round() as i16);
                            if on {
                                CurvePoint::on_curve(xr, yr)
                            } else {
                                CurvePoint::off_curve(xr, yr)
                            }
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let mut g = SimpleGlyph { bbox: Bbox::default(), contours, instructions: vec![] };
        g.recompute_bounding_box();
        // the left side bearing must be the glyph's x_min (0 when empty)
        let lsb = if g.contours.is_empty() { 0 } else { g.bbox.x_min };
        if !g.contours.is_empty() {
            bx0 = bx0.min(g.bbox.x_min);
            by0 = by0.min(g.bbox.y_min);
            bx1 = bx1.max(g.bbox.x_max);
            by1 = by1.max(g.bbox.y_max);
            max_pts = max_pts.max(g.contours.iter().map(|c| c.len() as u16).sum());
            max_cont = max_cont.max(g.contours.len() as u16);
        }
        glyf_b.add_glyph(&g).map_err(|e| werr(&e))?;
        metrics.push(LongMetric { advance: advances[gid], side_bearing: lsb });
    }
    let (glyf, loca, fmt) = glyf_b.build();
    if bx0 > bx1 {
        (bx0, by0, bx1, by1) = (0, 0, 0, 0);
    }
    let (bold, italic) = match plan.style {
        Style::Bold => (true, false),
        Style::Italic => (false, true),
        Style::BoldItalic => (true, true),
        _ => (false, false),
    };
    let mut mac = MacStyle::empty();
    if bold {
        mac |= MacStyle::BOLD;
    }
    if italic {
        mac |= MacStyle::ITALIC;
    }
    let head = Head {
        units_per_em: upem,
        x_min: bx0,
        y_min: by0,
        x_max: bx1,
        y_max: by1,
        mac_style: mac,
        index_to_loc_format: fmt as i16,
        lowest_rec_ppem: 6,
        font_direction_hint: 2,
        font_revision: Fixed::from_f64(1.0),
        ..Head::default()
    };
    let (asc, desc) = (plan.ascent, plan.descent);
    let hhea = Hhea {
        ascender: FWord::new(asc),
        descender: FWord::new(-desc),
        line_gap: FWord::new(0),
        advance_width_max: UfWord::new(*advances.iter().max().unwrap_or(&0)),
        min_left_side_bearing: FWord::new(0),
        min_right_side_bearing: FWord::new(0),
        x_max_extent: FWord::new(bx1),
        caret_slope_rise: 1,
        caret_slope_run: 0,
        caret_offset: 0,
        number_of_h_metrics: n as u16,
    };
    let hmtx = Hmtx::new(metrics, vec![]);
    let mut maxp = Maxp::new(n as u16);
    maxp.max_points = Some(max_pts);
    maxp.max_contours = Some(max_cont);
    maxp.max_composite_points = Some(0);
    maxp.max_composite_contours = Some(0);
    maxp.max_zones = Some(1);
    maxp.max_twilight_points = Some(0);
    maxp.max_storage = Some(0);
    maxp.max_function_defs = Some(0);
    maxp.max_instruction_defs = Some(0);
    maxp.max_stack_elements = Some(0);
    maxp.max_size_of_instructions = Some(0);
    maxp.max_component_elements = Some(0);
    maxp.max_component_depth = Some(0);

    let cmap = Cmap::from_mappings(src.charmap().mappings().filter_map(|(cp, gid)| {
        char::from_u32(cp).filter(|_| (gid.to_u32() as usize) < n).map(|c| (c, write_fonts::types::GlyphId::new(gid.to_u32())))
    }))
    .map_err(|e| werr(&e))?;

    let style_name = match plan.style {
        Style::Regular | Style::Caret => "Regular",
        Style::Bold => "Bold",
        Style::Italic => "Italic",
        Style::BoldItalic => "Bold Italic",
    };
    let ps = format!("{}-{}", plan.family.replace(' ', ""), style_name.replace(' ', ""));
    let mk = |id: u16, s: &str| NameRecord::new(3, 1, 0x409, NameId::new(id), s.to_string().into());
    let mut recs =
        vec![mk(1, plan.family), mk(2, style_name), mk(3, &ps), mk(4, &format!("{} {}", plan.family, style_name)), mk(6, &ps)];
    recs.sort_by_key(|r| (r.platform_id, r.encoding_id, r.language_id, r.name_id));
    let name = Name::new(recs);

    let mut sel = SelectionFlags::USE_TYPO_METRICS;
    if italic {
        sel |= SelectionFlags::ITALIC;
    }
    if bold {
        sel |= SelectionFlags::BOLD;
    }
    if !bold && !italic {
        sel |= SelectionFlags::REGULAR;
    }
    let os2 = Os2 {
        x_avg_char_width: (advances.iter().map(|&a| a as u32).sum::<u32>() / n.max(1) as u32) as i16,
        us_weight_class: plan.weight_class,
        us_width_class: 5,
        fs_selection: sel,
        s_typo_ascender: asc,
        s_typo_descender: -desc,
        s_typo_line_gap: 0,
        us_win_ascent: asc.max(0) as u16,
        us_win_descent: desc.max(0) as u16,
        us_first_char_index: 0x20,
        us_last_char_index: 0xFFFF,
        ach_vend_id: write_fonts::types::Tag::new(b"WASH"),
        ..Os2::default()
    };
    let post = Post { version: write_fonts::types::Version16Dot16::VERSION_3_0, ..Post::default() };

    let mut b = FontBuilder::new();
    b.add_table(&head).map_err(|e| werr(&e))?;
    b.add_table(&hhea).map_err(|e| werr(&e))?;
    b.add_table(&maxp).map_err(|e| werr(&e))?;
    b.add_table(&hmtx).map_err(|e| werr(&e))?;
    b.add_table(&cmap).map_err(|e| werr(&e))?;
    b.add_table(&glyf).map_err(|e| werr(&e))?;
    b.add_table(&loca).map_err(|e| werr(&e))?;
    b.add_table(&name).map_err(|e| werr(&e))?;
    b.add_table(&os2).map_err(|e| werr(&e))?;
    b.add_table(&post).map_err(|e| werr(&e))?;
    Ok(b.build())
}

/// What Regular is drawn from: outlines, advances and the shear that goes with them.
struct Base<'a> {
    src: FontRef<'a>,
    axes: AxisRanges,
    upem: u16,
    n: usize,
    advances: Vec<u16>,
    outlines: Vec<Pts>,
    /// Washi's shear for the slant when the font has no slant axis.
    shear: f32,
    weight: f32,
    ascent: i16,
    descent: i16,
}

fn base<'a>(pkg: &'a FontPackage, v: AxisValues) -> Result<Base<'a>, BakeError> {
    let src = FontRef::new(&pkg.regular).map_err(|_| BakeError::Unreadable)?;
    let upem = src.head().map_err(|_| BakeError::Unreadable)?.units_per_em();
    let n = src.maxp().map_err(|_| BakeError::Unreadable)?.num_glyphs() as usize;
    let axes = axes_of(&src);
    let shear = if axes.slant.is_none() { v.slant.unwrap_or(0.0).clamp(0.0, SHEAR_MAX_DEG).to_radians().tan() } else { 0.0 };
    let loc = location(&src, &axes, v, 0.0);
    let advances: Vec<u16> = (0..n).map(|g| advance_of(&src, &loc, GlyphId::new(g as u32)).round().clamp(0.0, 65535.0) as u16).collect();
    let outlines = (0..n).map(|g| outline_of(&src, &loc, GlyphId::new(g as u32))).collect::<Result<Vec<_>, _>>()?;
    let m = src.metrics(Size::unscaled(), LocationRef::from(&loc));
    let weight = axes.weight.map(|r| v.weight.unwrap_or(r.default).clamp(r.min, r.max)).unwrap_or(400.0);
    Ok(Base {
        src,
        axes,
        upem,
        n,
        advances,
        outlines,
        shear,
        weight,
        ascent: m.ascent.round() as i16,
        descent: (-m.descent).round() as i16,
    })
}

fn metrics_of(b: &Base) -> FontMetrics {
    let upem = b.upem as f32;
    let (mut lean, mut below, mut over) = (0.0f32, 0.0f32, 0.0f32);
    let italic_shear = b.shear + ITALIC_SHEAR;
    for (g, o) in b.outlines.iter().enumerate() {
        for c in o {
            for &(x, y, _) in c {
                lean = lean.max(-(x + b.shear * y));
                below = below.max(-y);
                over = over.max(x + italic_shear * y - b.advances[g] as f32);
            }
        }
    }
    let cmap = b.src.charmap();
    let adv = |c: char| cmap.map(c).map(|g| b.advances.get(g.to_u32() as usize).copied().unwrap_or(0) as f32);
    let sample: Vec<f32> = SAMPLE_UI_TEXT.chars().filter_map(adv).collect();
    FontMetrics {
        line_em: (b.ascent as f32 + b.descent as f32) / upem,
        space_em: adv(' ').unwrap_or(0.0) / upem,
        lean_em: lean / upem,
        descent_em: below / upem,
        italic_overhang_em: over / upem,
        avg_advance_em: if sample.is_empty() { 0.0 } else { sample.iter().sum::<f32>() / sample.len() as f32 / upem },
    }
}

fn centred(o: Pts, dx: f32, scale: f32) -> Pts {
    o.into_iter().map(|c| c.into_iter().map(|(x, y, on)| (x * scale + dx, y * scale, on)).collect()).collect()
}

fn bold_outlines(pkg: &FontPackage, b: &Base, v: AxisValues) -> Result<Vec<Pts>, BakeError> {
    if b.axes.weight.is_some() {
        // outlines at weight + 300 (clamped to the axis), centred in Regular's advance
        let bl = location(&b.src, &b.axes, v, BOLD_STEP);
        return (0..b.n)
            .map(|g| {
                let gid = GlyphId::new(g as u32);
                let dx = (b.advances[g] as f32 - advance_of(&b.src, &bl, gid)) / 2.0;
                Ok(centred(outline_of(&b.src, &bl, gid)?, dx, 1.0))
            })
            .collect();
    }
    if let Some(bytes) = &pkg.bold {
        if let Ok(bf) = FontRef::new(bytes) {
            let bl = bf.axes().location::<[(&str, f32); 0]>([]);
            let scale = b.upem as f32 / bf.head().map(|h| h.units_per_em()).unwrap_or(b.upem).max(1) as f32;
            // glyph ids differ between files: go through the cmap
            let bcm = bf.charmap();
            let mut gid_map = vec![None; b.n];
            for (cp, sg) in b.src.charmap().mappings() {
                if let (Some(bg), Some(slot)) = (bcm.map(cp), gid_map.get_mut(sg.to_u32() as usize)) {
                    *slot = Some(bg);
                }
            }
            return (0..b.n)
                .map(|g| match gid_map[g] {
                    Some(bg) => {
                        let dx = (b.advances[g] as f32 - advance_of(&bf, &bl, bg) * scale) / 2.0;
                        Ok(centred(outline_of(&bf, &bl, bg)?, dx, scale))
                    }
                    None => Ok(b.outlines[g].clone()),
                })
                .collect();
        }
    }
    // double-strike: Regular plus copies shifted by +-em/64 (non-zero winding fills the union)
    let d = b.upem as f32 / 64.0;
    Ok(b.outlines
        .iter()
        .map(|o| {
            let mut v = o.clone();
            for s in [-d, d] {
                for c in o {
                    v.push(c.iter().map(|&(x, y, on)| (x + s, y, on)).collect());
                }
            }
            v
        })
        .collect())
}

fn weight_class(w: f32) -> u16 {
    w.round().clamp(1.0, 1000.0) as u16
}

/// Make the five faces of `pkg` at `values`.
pub fn bake(pkg: &FontPackage, values: AxisValues) -> Result<FaceSet, BakeError> {
    let b = base(pkg, values)?;
    let (text_family, caret_family) = fresh_families();
    let bold = bold_outlines(pkg, &b, values)?;
    let bold_w = match b.axes.weight {
        Some(r) => (b.weight + BOLD_STEP).min(r.max),
        None => 700.0,
    };
    let (asc, desc) = (b.ascent, b.descent);
    let italic_shear = b.shear + ITALIC_SHEAR;
    let mk = |outlines: &[Pts], style: Style, shear: f32, fam: &str, w: f32, a: i16, d: i16| {
        write_face(&b.src, outlines, &b.advances, b.upem, Plan { family: fam, style, shear, weight_class: weight_class(w), ascent: a, descent: d })
    };
    let blank: Vec<Pts> = vec![Vec::new(); b.n];
    let u = b.upem as f32;
    Ok(FaceSet {
        regular: mk(&b.outlines, Style::Regular, b.shear, &text_family, b.weight, asc, desc)?,
        bold: mk(&bold, Style::Bold, b.shear, &text_family, bold_w, asc, desc)?,
        italic: mk(&b.outlines, Style::Italic, italic_shear, &text_family, b.weight, asc, desc)?,
        bold_italic: mk(&bold, Style::BoldItalic, italic_shear, &text_family, bold_w, asc, desc)?,
        caret: mk(
            &blank,
            Style::Caret,
            0.0,
            &caret_family,
            400.0,
            (CARET_ASCENT * u / 1024.0).round() as i16,
            (CARET_DESCENT * u / 1024.0).round() as i16,
        )?,
        metrics: metrics_of(&b),
        text_family,
        caret_family,
    })
}

/// Make only the Regular face (the live-drag path; 4 to 24 ms in ADR 0003).
pub fn bake_regular(pkg: &FontPackage, values: AxisValues) -> Result<RegularFace, BakeError> {
    let b = base(pkg, values)?;
    let (family, _) = fresh_families();
    let face = write_face(
        &b.src,
        &b.outlines,
        &b.advances,
        b.upem,
        Plan { family: &family, style: Style::Regular, shear: b.shear, weight_class: weight_class(b.weight), ascent: b.ascent, descent: b.descent },
    )?;
    Ok(RegularFace { face, family, metrics: metrics_of(&b) })
}
