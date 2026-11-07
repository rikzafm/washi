//! The font-match run: score every candidate font file against every role of the hover mockup.

use super::reference::{derotate, ink_bitmap};
use super::render::{load_fonts, FontFace, TextRenderer};
use super::score::{align_iou_ignoring, Bitmap};
use crate::image::{Image, Rgb};
use crate::spec::{Rect, Spec};
use crate::{measure_region, Region};
use serde_json::{json, Value};
use std::path::Path;

/// Score every role must reach before a font is accepted.
pub const PASS_IOU: f64 = 0.85;

/// How far the font size may stray from the size that matches the ink height (factor).
const SCALE_SPAN: f64 = 0.16;
/// Alignment search radius around the bounding-box centres, in px.
const RADIUS: i64 = 5;
/// How many front runners per role get the fine search.
const DEEP_TOP: usize = 3;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// any upright face
    Text,
    /// real bold face required
    Bold,
    /// real italic face required
    Italic,
    /// regular upright face (the app draws the underline / strike-through itself)
    Regular,
}

impl Kind {
    fn accepts(self, f: &FontFace) -> bool {
        match self {
            Kind::Text => !f.italic,
            Kind::Bold => f.bold && !f.italic,
            Kind::Italic => f.italic && !f.bold,
            Kind::Regular => !f.bold && !f.italic,
        }
    }
}

/// One line (or glyph) to reproduce: the string and the ink mask it must cover.
struct Target {
    text: String,
    mask: Bitmap,
    /// the same line leveled with slightly different tilts (each line's own slope varies a little)
    alts: Vec<Bitmap>,
    /// rows of `mask` that belong to decoration drawn by the app, not by the font
    ignore: Vec<bool>,
}

struct Role {
    name: &'static str,
    kind: Kind,
    targets: Vec<Target>,
}

fn rowcount(mask: &Bitmap, y: usize) -> usize {
    (0..mask.w).filter(|&x| mask.on[y * mask.w + x]).count()
}

/// Text roles: level the region, then cut it into one mask per line at the emptiest row between
/// the measured baselines.
fn text_role(name: &'static str, kind: Kind, img: &Image, spec: &Spec, region: &str, tilt: f64, lines: &[&str]) -> Result<Role, String> {
    let Region::Text { rect, paper, .. } = &spec.regions[region] else { return Err(format!("region {region} is not a text region")) };
    let measured = measure_region(img, &spec.regions[region]);
    let found = measured["lines"].as_array().ok_or("no lines")?;
    if found.len() != lines.len() {
        return Err(format!("{region}: expected {} lines, measured {}", lines.len(), found.len()));
    }
    let baselines: Vec<f64> = found.iter().map(|l| l["baseline"].as_f64().unwrap() - rect[1] as f64).collect();
    let paper = crate::image::parse_hex(paper)?;
    let flat = derotate(img, *rect, tilt);
    let full = ink_bitmap(&flat, paper, INK);
    let rows = |lo: f64, hi: f64| (lo.floor().max(0.0) as usize, (hi.ceil() as usize).min(full.h));
    let mut cuts = vec![0usize];
    for (i, pair) in baselines.windows(2).enumerate() {
        let pitch = pair[1] - pair[0];
        let (lo, hi) = rows(pair[0] + 0.1 * pitch, pair[1] - 0.55 * pitch);
        let mid = (lo + hi) as f64 / 2.0;
        let cut = (lo..hi.max(lo + 1))
            .min_by(|&a, &b| {
                rowcount(&full, a)
                    .cmp(&rowcount(&full, b))
                    .then((a as f64 - mid).abs().partial_cmp(&(b as f64 - mid).abs()).unwrap())
            })
            .ok_or_else(|| format!("{region}: no cut row for line {i}"))?;
        cuts.push(cut);
    }
    cuts.push(full.h);
    let band = |full: &Bitmap, i: usize| Bitmap::from_fn(full.w, full.h, |x, y| y >= cuts[i] && y < cuts[i + 1] && full.on[y * full.w + x]);
    let alt_fulls: Vec<Bitmap> = [-0.15, -0.1, -0.05, 0.05, 0.1, 0.15].iter().map(|d| ink_bitmap(&derotate(img, *rect, tilt + d), paper, INK)).collect();
    let targets = lines
        .iter()
        .enumerate()
        .map(|(i, text)| Target {
            text: text.to_string(),
            mask: band(&full, i),
            alts: alt_fulls.iter().map(|f| band(f, i)).collect(),
            ignore: vec![],
        })
        .collect();
    Ok(Role { name, kind, targets })
}

const INK: Rgb = [0x2B, 0x2F, 0x36];

/// Toolbar glyph: crop one cell around its measured bbox, level it with the paper tilt, and
/// threshold between the local background and the glyph's own ink color.
fn glyph_role(name: &'static str, kind: Kind, img: &Image, cell_bbox: &Value, tilt: f64, text: &str, decorated: bool) -> Result<Role, String> {
    let b: Vec<i64> = cell_bbox.as_array().ok_or("cell bbox")?.iter().map(|v| v.as_i64().unwrap()).collect();
    let rect: Rect = [b[0] - 8, b[1] - 8, b[2] + 8, b[3] + 8];
    let flat = derotate(img, rect, tilt);
    // background = median of the crop's border, ink = darkest 2% of the crop
    let mut border: Vec<f64> = vec![];
    for y in 0..flat.h {
        for x in 0..flat.w {
            if x < 2 || y < 2 || x + 2 >= flat.w || y + 2 >= flat.h {
                border.push(flat.lum(x, y));
            }
        }
    }
    border.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let bg = border[border.len() / 2].round() as u8;
    let mut all: Vec<f64> = (0..flat.h).flat_map(|y| (0..flat.w).map(move |x| (x, y))).map(|(x, y)| flat.lum(x, y)).collect();
    all.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let ink = all[(all.len() / 50).min(all.len() - 1)].round() as u8;
    let full = ink_bitmap(&flat, [bg; 3], [ink; 3]);
    let mut ignore = vec![false; full.h];
    let mut mask = full.clone();
    if decorated {
        let (x0, _, x1, _) = full.bbox().ok_or("empty glyph cell")?;
        for y in 0..full.h {
            if rowcount(&full, y) as f64 >= 0.8 * (x1 - x0) as f64 {
                ignore[y] = true;
                for x in 0..full.w {
                    mask.on[y * full.w + x] = false;
                }
            }
        }
    }
    Ok(Role { name, kind, targets: vec![Target { text: text.to_string(), mask, alts: vec![], ignore }] })
}

fn bitmap_of(cov: &super::render::Coverage) -> Bitmap {
    Bitmap::from_fn(cov.w, cov.h, |x, y| cov.a[y * cov.w + x] >= 0.5)
}

/// The best placement found for one font on one target.
#[derive(Clone, Copy, Default)]
struct Fit {
    iou: f64,
    px: f32,
    /// sub-pixel offset of the text
    ox: f32,
    oy: f32,
    /// extra letter spacing (px)
    ls: f32,
    /// 0 = the base mask, i > 0 = `alts[i - 1]`
    mask: usize,
    /// whole-pixel offset of the render onto the reference
    dx: i64,
    dy: i64,
}

/// Best IoU of `face` on one target, with the font size searched around the size that makes the
/// ink heights equal, then the sub-pixel phase on a quarter-pixel grid.
fn match_target(r: &mut TextRenderer, face: &FontFace, t: &Target, start: Option<Fit>, grain: i32) -> Fit {
    let kept = Bitmap::from_fn(t.mask.w, t.mask.h, |x, y| t.mask.on[y * t.mask.w + x] && !t.ignore.get(y).copied().unwrap_or(false));
    let Some((_, ry0, _, ry1)) = kept.bbox() else { return Fit::default() };
    let ref_h = (ry1 - ry0) as f64;
    let probe = bitmap_of(&r.render(face, &t.text, 100.0));
    let Some((_, py0, _, py1)) = probe.bbox() else { return Fit::default() };
    let s0 = 100.0 * ref_h / (py1 - py0) as f64;
    let mut best = start.unwrap_or(Fit { iou: -1.0, px: s0 as f32, ..Default::default() });
    let mut try_at = |px: f32, ox: f32, oy: f32, ls: f32, m: usize, best: &mut Fit| {
        let cand = bitmap_of(&r.render_at(face, &t.text, px, ox, oy, ls));
        let (iou, (dx, dy)) = align_iou_ignoring(if m == 0 { &t.mask } else { &t.alts[m - 1] }, &cand, RADIUS, &t.ignore);
        if iou > best.iou {
            *best = Fit { iou, px, ox, oy, ls, mask: m, dx, dy };
        }
    };
    if start.is_none() {
        let steps = 16;
        for i in 0..=steps {
            try_at((s0 * (1.0 - SCALE_SPAN / 2.0 + SCALE_SPAN * i as f64 / steps as f64)) as f32, 0.0, 0.0, 0.0, 0, &mut best);
        }
    }
    // Coordinate descent over font size, letter spacing, sub-pixel phase and tilt. The mockup's
    // glyphs sit at arbitrary sub-pixel positions while the renderer snaps glyphs to whole pixels,
    // so the landscape is rugged: sweep each axis, twice. `grain` > 1 sweeps more finely.
    let n_phase = 8 * grain;
    for _ in 0..2 {
        let b = best;
        for i in -8 * grain..=8 * grain {
            if i != 0 {
                try_at((b.px as f64 + i as f64 * 0.002 / grain as f64 * s0) as f32, b.ox, b.oy, b.ls, b.mask, &mut best);
            }
        }
        let b = best;
        for i in -10 * grain..=10 * grain {
            if i != 0 {
                try_at(b.px, b.ox, b.oy, b.ls + i as f32 * 0.05 / grain as f32, b.mask, &mut best);
            }
        }
        let b = best;
        for fy in 0..n_phase {
            for fx in 0..n_phase {
                try_at(b.px, fx as f32 / n_phase as f32, fy as f32 / n_phase as f32, b.ls, b.mask, &mut best);
            }
        }
        let b = best;
        for m in 0..=t.alts.len() {
            try_at(b.px, b.ox, b.oy, b.ls, m, &mut best);
        }
    }
    best
}

/// Debug aid (env UIDIFF_FONTMATCH_DUMP=<dir>): reference ink in red, best render in blue,
/// overlap in dark purple.
fn dump_overlay(dir: &Path, name: &str, r: &mut TextRenderer, face: &FontFace, t: &Target, fit: &Fit) {
    let cand = bitmap_of(&r.render_at(face, &t.text, fit.px, fit.ox, fit.oy, fit.ls));
    let mask = if fit.mask == 0 { &t.mask } else { &t.alts[fit.mask - 1] };
    let mut img = Image::new(t.mask.w, t.mask.h, [255, 255, 255]);
    for y in 0..t.mask.h {
        for x in 0..t.mask.w {
            let a = mask.on[y * mask.w + x];
            let b = cand.get(x as i64 - fit.dx, y as i64 - fit.dy);
            img.set(x, y, match (a, b) {
                (true, true) => [70, 40, 90],
                (true, false) => [230, 60, 60],
                (false, true) => [60, 110, 230],
                _ => [255, 255, 255],
            });
        }
    }
    let _ = std::fs::create_dir_all(dir);
    let file = dir.join(format!("{name}.png"));
    if let Ok(f) = std::fs::File::create(&file) {
        let mut enc = png::Encoder::new(f, img.w as u32, img.h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        if let Ok(mut w) = enc.write_header() {
            let mut data = Vec::with_capacity(img.w * img.h * 3);
            for y in 0..img.h {
                for x in 0..img.w {
                    data.extend_from_slice(&img.get(x, y));
                }
            }
            let _ = w.write_image_data(&data);
        }
    }
}

/// Ink-area weighted mean of the per-line IoUs.
fn role_iou(role: &Role, per: &[Fit]) -> f64 {
    let weights: Vec<f64> = role.targets.iter().map(|t| t.mask.count() as f64).collect();
    per.iter().zip(&weights).map(|(f, w)| f.iou * w).sum::<f64>() / weights.iter().sum::<f64>()
}

/// Score all candidate fonts in `fonts_dir` against the hover mockup; returns the report.
pub fn run(spec: &Spec, raw: &Value, spec_dir: &Path, fonts_dir: &Path) -> Result<Value, String> {
    let mock = spec.mockups.get("hover").ok_or("the spec has no hover mockup")?;
    let img = Image::load_png(&spec_dir.join(&mock.file))?;
    let text_tilt = raw["text_tilt_deg"]["value"].as_f64().ok_or("spec.text_tilt_deg.value")?;
    let paper_tilt = raw["paper_tilt_deg"]["value"].as_f64().ok_or("spec.paper_tilt_deg.value")?;
    let scale = raw["scale"]["value"].as_f64().ok_or("spec.scale.value")?;
    let faces = load_fonts(fonts_dir)?;
    if faces.is_empty() {
        return Err(format!("no .ttf files in {}", fonts_dir.display()));
    }

    let toolbar = measure_region(&img, &spec.regions["toolbar"]);
    let cells = toolbar["cells"].as_array().ok_or("toolbar cells")?;
    let cell = |i: usize| &cells[i]["bbox"];
    let roles = vec![
        text_role("title", Kind::Text, &img, spec, "title", text_tilt, &["Endfield Note"])?,
        text_role("heading", Kind::Text, &img, spec, "heading", text_tilt, &["Thermite Cutter : POG &", "Akekuri"])?,
        text_role(
            "body",
            Kind::Text,
            &img,
            spec,
            "body",
            text_tilt,
            &["Currently POG no Weapon : Ideal", "for him are Never Rest or White", "Night Nova. The last can be get", "in a box"],
        )?,
        glyph_role("glyph_B", Kind::Bold, &img, cell(0), paper_tilt, "B", false)?,
        glyph_role("glyph_I", Kind::Italic, &img, cell(1), paper_tilt, "I", false)?,
        glyph_role("glyph_U", Kind::Regular, &img, cell(2), paper_tilt, "U", true)?,
        glyph_role("glyph_ab", Kind::Regular, &img, cell(3), paper_tilt, "ab", true)?,
    ];

    let mut renderer = TextRenderer::new();
    let mut out = serde_json::Map::new();
    let mut all_pass = true;
    for role in &roles {
        let mut ranking: Vec<(f64, &FontFace, Vec<Fit>)> = vec![];
        for face in faces.iter().filter(|f| role.kind.accepts(f)) {
            let per: Vec<Fit> = role.targets.iter().map(|t| match_target(&mut renderer, face, t, None, 1)).collect();
            ranking.push((role_iou(role, &per), face, per));
        }
        ranking.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        // a finer search for the front runners only (it is the expensive part)
        for entry in ranking.iter_mut().take(DEEP_TOP) {
            let per: Vec<Fit> = role.targets.iter().zip(&entry.2).map(|(t, f)| match_target(&mut renderer, entry.1, t, Some(*f), 2)).collect();
            *entry = (role_iou(role, &per), entry.1, per);
        }
        ranking.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let best = ranking.first().ok_or_else(|| format!("no candidate font fits role {}", role.name))?;
        if let Ok(dir) = std::env::var("UIDIFF_FONTMATCH_DUMP") {
            for (i, (t, fit)) in role.targets.iter().zip(&best.2).enumerate() {
                dump_overlay(Path::new(&dir), &format!("{}-{}", role.name, i), &mut renderer, best.1, t, fit);
            }
        }
        let pass = best.0 >= PASS_IOU;
        all_pass &= pass;
        let describe = |(iou, face, per): &(f64, &FontFace, Vec<Fit>)| {
            json!({
                "font": face.family,
                "iou": crate::image::r3(*iou),
                "font_px": per.iter().map(|f| crate::image::r3(f.px as f64)).collect::<Vec<_>>(),
                "tilt_offset_deg": per.iter().map(|f| crate::image::r3(if f.mask == 0 { 0.0 } else { [-0.15, -0.1, -0.05, 0.05, 0.1, 0.15][f.mask - 1] })).collect::<Vec<_>>(),
                "letter_spacing_px": per.iter().map(|f| crate::image::r3(f.ls as f64)).collect::<Vec<_>>(),
                "font_logical_px": per.iter().map(|f| crate::image::r3(f.px as f64 / scale)).collect::<Vec<_>>(),
                "line_iou": per.iter().map(|f| crate::image::r3(f.iou)).collect::<Vec<_>>(),
            })
        };
        out.insert(
            role.name.to_string(),
            json!({
                "text": role.targets.iter().map(|t| t.text.clone()).collect::<Vec<_>>(),
                "pass": pass,
                "best": describe(best),
                "ranking": ranking.iter().map(|r| json!({"font": r.1.family, "iou": crate::image::r3(r.0)})).collect::<Vec<_>>(),
            }),
        );
    }
    Ok(json!({ "pass_iou": PASS_IOU, "fonts_dir": fonts_dir.display().to_string(), "all_roles_pass": all_pass, "roles": Value::Object(out) }))
}
