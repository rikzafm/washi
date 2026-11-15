//! Spike S2: bake static TrueType faces from a (variable) font with skrifa + write-fonts.
//! Throwaway; the production version is ticket 02 (`washi/src/fonts/bake.rs`).

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};
use write_fonts::tables::cmap::Cmap;
use skrifa::raw::tables::glyf::CurvePoint;
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

/// The axis values a face is made at. `None` = the font's default (or no such axis).
#[derive(Clone, Copy, Debug, Default)]
pub struct Axes {
    pub wght: Option<f32>,
    pub wdth: Option<f32>,
    pub slnt: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
pub enum BoldSource<'a> {
    /// Outlines at weight + 300 on the font's own weight axis (clamped by skrifa to the axis).
    Axis(f32),
    /// Outlines from another font file (the package's static Bold).
    File(&'a [u8]),
    /// Regular contours plus copies shifted by +-em/64.
    DoubleStrike,
}

pub struct FaceSet {
    pub regular: Vec<u8>,
    pub bold: Vec<u8>,
    pub italic: Vec<u8>,
    pub bold_italic: Vec<u8>,
    pub caret: Vec<u8>,
    pub advances: Vec<u16>,
    pub upem: u16,
}

type Pts = Vec<Vec<(f32, f32, bool)>>;

#[derive(Default)]
struct Collect {
    contours: Pts,
    cur: Vec<(f32, f32, bool)>,
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
        panic!("cubic outline: CFF fonts are refused (P5)");
    }
    fn close(&mut self) {
        self.flush();
    }
}

fn loc_of(font: &FontRef, a: Axes, wght_add: f32) -> skrifa::instance::Location {
    let mut v: Vec<(&str, f32)> = Vec::new();
    if let Some(w) = a.wght {
        v.push(("wght", w + wght_add));
    } else if wght_add != 0.0 {
        // the axis default + the step; skrifa clamps to the axis range
        if let Some(ax) = font.axes().iter().find(|x| x.tag() == skrifa::Tag::new(b"wght")) {
            v.push(("wght", ax.default_value() + wght_add));
        }
    }
    if let Some(w) = a.wdth {
        v.push(("wdth", w));
    }
    if let Some(s) = a.slnt {
        v.push(("slnt", s));
    }
    font.axes().location(v)
}

fn outline_of(font: &FontRef, loc: &skrifa::instance::Location, gid: GlyphId) -> Pts {
    let mut pen = Collect::default();
    if let Some(g) = font.outline_glyphs().get(gid) {
        let _ = g.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::from(loc)), &mut pen);
        pen.flush();
    }
    pen.contours
}

fn advance_of(font: &FontRef, loc: &skrifa::instance::Location, gid: GlyphId) -> f32 {
    font.glyph_metrics(Size::unscaled(), LocationRef::from(loc)).advance_width(gid).unwrap_or(0.0)
}

#[derive(Clone, Copy)]
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
    /// pre-shear x offset per glyph computed by the caller: (regular advance - this face's advance)/2
    weight_class: u16,
}

#[allow(clippy::too_many_arguments)]
fn write_face(
    src: &FontRef,
    glyph_outlines: Vec<Pts>,
    advances: &[u16],
    upem: u16,
    plan: Plan,
) -> Vec<u8> {
    let n = advances.len();
    let mut glyf_b = GlyfLocaBuilder::new();
    let mut metrics = Vec::with_capacity(n);
    let (mut max_pts, mut max_cont) = (0u16, 0u16);
    let (mut bx0, mut by0, mut bx1, mut by1) = (i16::MAX, i16::MAX, i16::MIN, i16::MIN);
    for (gid, outline) in glyph_outlines.iter().enumerate() {
        let mut contours = Vec::new();
        for c in outline {
            let pts: Vec<CurvePoint> = c
                .iter()
                .map(|&(x, y, on)| {
                    let xs = x + plan.shear * y;
                    let (xr, yr) = (xs.round() as i16, y.round() as i16);
                    if on {
                        CurvePoint::on_curve(xr, yr)
                    } else {
                        CurvePoint::off_curve(xr, yr)
                    }
                })
                .collect();
            contours.push(Contour::from(pts));
        }
        let mut g = SimpleGlyph { bbox: Bbox::default(), contours, instructions: vec![] };
        g.recompute_bounding_box();
        let lsb = if g.contours.is_empty() { 0 } else { g.bbox.x_min };
        if !g.contours.is_empty() {
            bx0 = bx0.min(g.bbox.x_min);
            by0 = by0.min(g.bbox.y_min);
            bx1 = bx1.max(g.bbox.x_max);
            by1 = by1.max(g.bbox.y_max);
            max_pts = max_pts.max(g.contours.iter().map(|c| c.len() as u16).sum());
            max_cont = max_cont.max(g.contours.len() as u16);
        }
        glyf_b.add_glyph(&g).unwrap();
        metrics.push(LongMetric { advance: advances[gid], side_bearing: lsb });
    }
    let (glyf, loca, fmt) = glyf_b.build();
    if bx0 > bx1 {
        (bx0, by0, bx1, by1) = (0, 0, 0, 0);
    }

    let (asc, desc) = match plan.style {
        Style::Caret => ((861.0 * upem as f32 / 1024.0).round() as i16, (100.0 * upem as f32 / 1024.0).round() as i16),
        _ => {
            let m = src.metrics(Size::unscaled(), LocationRef::default());
            (m.ascent.round() as i16, (-m.descent).round() as i16)
        }
    };
    let (bold, italic) = match plan.style {
        Style::Bold => (true, false),
        Style::Italic => (false, true),
        Style::BoldItalic => (true, true),
        _ => (false, false),
    };

    let head = Head {
        units_per_em: upem,
        x_min: bx0,
        y_min: by0,
        x_max: bx1,
        y_max: by1,
        mac_style: {
            let mut m = MacStyle::empty();
            if bold {
                m |= MacStyle::BOLD;
            }
            if italic {
                m |= MacStyle::ITALIC;
            }
            m
        },
        index_to_loc_format: fmt as i16,
        lowest_rec_ppem: 6,
        font_direction_hint: 2,
        font_revision: Fixed::from_f64(1.0),
        ..Head::default()
    };
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

    // cmap: copy the source's Unicode mappings
    let cmap = Cmap::from_mappings(src.charmap().mappings().filter_map(|(cp, gid)| {
        char::from_u32(cp).map(|c| (c, write_fonts::types::GlyphId::new(gid.to_u32())))
    }))
    .expect("cmap conflict");

    let style_name = match plan.style {
        Style::Regular => "Regular",
        Style::Bold => "Bold",
        Style::Italic => "Italic",
        Style::BoldItalic => "Bold Italic",
        Style::Caret => "Regular",
    };
    let full = format!("{} {}", plan.family, style_name);
    let ps = format!("{}-{}", plan.family.replace(' ', ""), style_name.replace(' ', ""));
    let mk = |id: u16, s: &str| NameRecord::new(3, 1, 0x409, NameId::new(id), s.to_string().into());
    let mut recs = vec![mk(1, plan.family), mk(2, style_name), mk(3, &ps), mk(4, &full), mk(6, &ps)];
    recs.sort_by_key(|r| (r.platform_id, r.encoding_id, r.language_id, r.name_id));
    let name = Name::new(recs);

    let os2 = Os2 {
        x_avg_char_width: (advances.iter().map(|&a| a as u32).sum::<u32>() / n.max(1) as u32) as i16,
        us_weight_class: plan.weight_class,
        us_width_class: 5,
        fs_selection: {
            let mut f = SelectionFlags::empty();
            if italic {
                f |= SelectionFlags::ITALIC;
            }
            if bold {
                f |= SelectionFlags::BOLD;
            }
            if !bold && !italic {
                f |= SelectionFlags::REGULAR;
            }
            f | SelectionFlags::USE_TYPO_METRICS
        },
        s_typo_ascender: asc,
        s_typo_descender: -desc,
        s_typo_line_gap: 0,
        us_win_ascent: asc as u16,
        us_win_descent: desc as u16,
        us_first_char_index: 0x20,
        us_last_char_index: 0xFFFF,
        ach_vend_id: write_fonts::types::Tag::new(b"WASH"),
        ..Os2::default()
    };
    let post = Post { version: write_fonts::types::Version16Dot16::VERSION_3_0, ..Post::default() };

    let mut b = FontBuilder::new();
    b.add_table(&head).unwrap();
    b.add_table(&hhea).unwrap();
    b.add_table(&maxp).unwrap();
    b.add_table(&hmtx).unwrap();
    b.add_table(&cmap).unwrap();
    b.add_table(&glyf).unwrap();
    b.add_table(&loca).unwrap();
    b.add_table(&name).unwrap();
    b.add_table(&os2).unwrap();
    b.add_table(&post).unwrap();
    b.build()
}

/// Bake the five faces. `family` is the private base ("Washi Text 7"); the caret face is
/// registered as "<family> Caret" by the caller.
pub fn bake(src_bytes: &[u8], axes: Axes, bold_src: BoldSource, family: &str, caret_family: &str) -> FaceSet {
    let src = FontRef::new(src_bytes).expect("parse source");
    let upem = src.head().unwrap().units_per_em();
    let n = src.maxp().unwrap().num_glyphs() as usize;
    let reg_loc = loc_of(&src, axes, 0.0);
    let advances: Vec<u16> =
        (0..n).map(|g| advance_of(&src, &reg_loc, GlyphId::new(g as u32)).round().max(0.0) as u16).collect();
    let slant = |a: Axes| a; // (shear for fonts without a slnt axis is applied by the caller through Axes::slnt = None + shear)
    let _ = slant;

    let has_slnt = src.axes().iter().any(|a| a.tag() == skrifa::Tag::new(b"slnt"));
    let default_shear = match axes.slnt {
        Some(deg) if !has_slnt => (deg.to_radians()).tan(),
        _ => 0.0,
    };
    let reg_axes = if has_slnt { axes } else { Axes { slnt: None, ..axes } };
    let reg_loc = loc_of(&src, reg_axes, 0.0);

    let regular_outlines: Vec<Pts> = (0..n).map(|g| outline_of(&src, &reg_loc, GlyphId::new(g as u32))).collect();

    // bold, centred in the regular advance
    let bold_outlines: Vec<Pts> = match bold_src {
        BoldSource::Axis(step) => {
            let bl = loc_of(&src, reg_axes, step);
            (0..n)
                .map(|g| {
                    let gid = GlyphId::new(g as u32);
                    let o = outline_of(&src, &bl, gid);
                    let dx = (advances[g] as f32 - advance_of(&src, &bl, gid)) / 2.0;
                    o.into_iter().map(|c| c.into_iter().map(|(x, y, on)| (x + dx, y, on)).collect()).collect()
                })
                .collect()
        }
        BoldSource::File(bytes) => {
            let bf = FontRef::new(bytes).expect("bold parse");
            let bl = bf.axes().location::<[(&str, f32); 0]>([]);
            // glyph ids differ between files: go through the cmap
            let bcm = bf.charmap();
            let scm = src.charmap();
            let mut gid_map = vec![None; n];
            for (cp, sg) in scm.mappings() {
                if let Some(bg) = bcm.map(cp) {
                    gid_map[sg.to_u32() as usize] = Some(bg);
                }
            }
            (0..n)
                .map(|g| match gid_map[g] {
                    Some(bg) => {
                        let o = outline_of(&bf, &bl, bg);
                        let dx = (advances[g] as f32 - advance_of(&bf, &bl, bg)) / 2.0;
                        o.into_iter().map(|c| c.into_iter().map(|(x, y, on)| (x + dx, y, on)).collect()).collect()
                    }
                    None => regular_outlines[g].clone(),
                })
                .collect()
        }
        BoldSource::DoubleStrike => {
            let d = upem as f32 / 64.0;
            regular_outlines
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
                .collect()
        }
    };

    let w_reg = axes.wght.unwrap_or(400.0).round() as u16;
    let w_bold = match bold_src {
        BoldSource::Axis(step) => (axes.wght.unwrap_or(400.0) + step).round() as u16,
        _ => 700,
    };
    let italic_shear = 0.25 + default_shear;
    let mk = |outlines: Vec<Pts>, style: Style, shear: f32, fam: &str, w: u16| {
        write_face(&src, outlines, &advances, upem, Plan { family: fam, style, shear, weight_class: w })
    };
    let blank: Vec<Pts> = vec![Vec::new(); n];
    FaceSet {
        regular: mk(regular_outlines.clone(), Style::Regular, default_shear, family, w_reg),
        bold: mk(bold_outlines.clone(), Style::Bold, default_shear, family, w_bold),
        italic: mk(regular_outlines, Style::Italic, italic_shear, family, w_reg),
        bold_italic: mk(bold_outlines, Style::BoldItalic, italic_shear, family, w_bold),
        caret: mk(blank, Style::Caret, 0.0, caret_family, 400),
        advances,
        upem,
    }
}

/// Regular only (the live-drag path).
pub fn bake_regular(src_bytes: &[u8], axes: Axes, family: &str) -> Vec<u8> {
    let src = FontRef::new(src_bytes).expect("parse source");
    let upem = src.head().unwrap().units_per_em();
    let n = src.maxp().unwrap().num_glyphs() as usize;
    let has_slnt = src.axes().iter().any(|a| a.tag() == skrifa::Tag::new(b"slnt"));
    let shear = match axes.slnt {
        Some(deg) if !has_slnt => deg.to_radians().tan(),
        _ => 0.0,
    };
    let reg_axes = if has_slnt { axes } else { Axes { slnt: None, ..axes } };
    let loc = loc_of(&src, reg_axes, 0.0);
    let advances: Vec<u16> =
        (0..n).map(|g| advance_of(&src, &loc, GlyphId::new(g as u32)).round().max(0.0) as u16).collect();
    let outlines: Vec<Pts> = (0..n).map(|g| outline_of(&src, &loc, GlyphId::new(g as u32))).collect();
    write_face(&src, outlines, &advances, upem, Plan { family, style: Style::Regular, shear, weight_class: axes.wght.unwrap_or(400.0) as u16 })
}
