//! `--invariants`: what must not change when the paper gets bigger.
//!
//! There is no reference image at other sizes, so a default-size snap (the hover snap, laid out
//! like the hover mockup) is compared with a larger one: tilt, tape size and corner offset,
//! text insets and font sizes, toolbar geometry, and the text area staying above the toolbar band.

use crate::compare::{bbox, corners, f, fmt, Row, Rows};
use crate::image::{lum, Image};
use crate::spec::{Rect, Region, Spec};
use crate::measure_region;
use serde_json::Value;

pub const TILT_DEG: f64 = 1.5;
pub const TOL_TILT: f64 = 0.1;
pub const TOL: f64 = 1.0;
/// Height of the toolbar band at the bottom of the paper, in logical px.
pub const BAND_LOGICAL: f64 = 56.0;

pub struct Options {
    /// Snapshot pixels per logical pixel.
    pub scale: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options { scale: 1.5 }
    }
}

struct Paper {
    tl: (f64, f64),
    br: (f64, f64),
    bl: (f64, f64),
    tilt: f64,
    width: f64,
    height: f64,
}

fn paper_of(img: &Image, region: &Region) -> Option<Paper> {
    let m = measure_region(img, region);
    if m["found"] != true {
        return None;
    }
    let c = corners(&m)?;
    Some(Paper { tl: c[0], br: c[2], bl: c[3], tilt: f(&m["tilt_deg"]), width: f(&m["width"]), height: f(&m["height"]) })
}

fn shifted(rect: &Rect, dx: f64, dy: f64, grow_x: f64) -> Rect {
    [rect[0] + dx.round() as i64, rect[1] + dy.round() as i64, rect[2] + (dx + grow_x).round() as i64, rect[3] + dy.round() as i64]
}

/// Paper-frame coordinates (u along the top edge, v down the left edge) of an image point.
fn uv(p: &Paper, x: f64, y: f64) -> (f64, f64) {
    let (s, c) = p.tilt.to_radians().sin_cos();
    let (dx, dy) = (x - p.tl.0, y - p.tl.1);
    (dx * c + dy * s, -dx * s + dy * c)
}

struct TextLine {
    u: f64,
    v: f64,
    /// Ink height with the slope of a tilted line taken out, so lines of different length compare.
    height: f64,
}

fn first_line(m: &Value, p: &Paper) -> Option<TextLine> {
    let line = m["lines"].as_array()?.first()?;
    let b = bbox(&line["bbox"])?;
    let (u, v) = uv(p, b[0], f(&line["baseline_left"]));
    let slope = (b[2] - b[0]) * p.tilt.to_radians().tan();
    Some(TextLine { u, v, height: f(&line["ink_height"]) - slope })
}

pub fn check(spec: &Spec, mock: &Image, default: &Image, large: &Image, opts: &Options) -> Result<Vec<Row>, String> {
    let paper_region = spec.regions.get("paper").ok_or("spec has no 'paper' region")?;
    let mut r = Rows { rows: vec![], scale: opts.scale };

    let Some(pm) = paper_of(mock, paper_region) else { return Err("the hover mockup has no paper".into()) };
    let (pd, pl) = (paper_of(default, paper_region), paper_of(large, paper_region));

    for (label, p) in [("default", &pd), ("large", &pl)] {
        match p {
            Some(p) => r.num("tilt", &format!("{label} paper tilt (deg)"), TILT_DEG, p.tilt, TOL_TILT, Some(format!("{label} snap: paper tilt is {} deg, it must be a constant {TILT_DEG} deg at every size", fmt(p.tilt)))),
            None => r.flag("tilt", &format!("{label} paper found"), false, "paper", "none", Some(format!("{label} snap: no paper found (paper color #{}?)", "FFFFFD"))),
        }
    }
    let (Some(pd), Some(pl)) = (pd, pl) else { return Ok(r.rows) };

    // tapes: same size, same offset from their corner
    for (name, corner) in [("tape_pink", (|p: &Paper| p.tl) as fn(&Paper) -> (f64, f64)), ("tape_blue", (|p: &Paper| p.br) as fn(&Paper) -> (f64, f64))] {
        let Some(Region::Tape { mask, .. }) = spec.regions.get(name) else { continue };
        let region = Region::Tape { rect: None, mask: mask.clone() };
        let (md, ml) = (measure_region(default, &region), measure_region(large, &region));
        if md["found"] != true || ml["found"] != true {
            r.flag("tapes", &format!("{name} found"), false, "tape", "missing", Some(format!("{name}: not found in both snaps")));
            continue;
        }
        for k in ["length", "width"] {
            r.num("tapes", &format!("{name} {k}"), f(&md[k]), f(&ml[k]), TOL, Some(format!("{name}: tape {k} changes with the note size; tapes have a fixed size")));
        }
        let off = |m: &Value, p: &Paper| (f(&m["centroid"]["x"]) - corner(p).0, f(&m["centroid"]["y"]) - corner(p).1);
        let (od, ol) = (off(&md, &pd), off(&ml, &pl));
        let hint = Some(format!("{name}: offset from its paper corner is ({}, {}) px in the large snap vs ({}, {}) px at the default size; tapes are pinned to their corner", fmt(ol.0), fmt(ol.1), fmt(od.0), fmt(od.1)));
        r.num("tapes", &format!("{name} corner offset x"), od.0, ol.0, TOL, hint.clone());
        r.num("tapes", &format!("{name} corner offset y"), od.1, ol.1, TOL, hint);
    }

    // text: insets and font size (first line of title, heading and body), anchored at the top-left corner
    let grow = (pl.width - pd.width).max(0.0);
    for name in ["title", "heading", "body"] {
        let Some(Region::Text { rect, paper, ink_lum_max }) = spec.regions.get(name) else { continue };
        let reg = |dx: f64, dy: f64, grow_x: f64| Region::Text { rect: shifted(rect, dx, dy, grow_x), paper: paper.clone(), ink_lum_max: *ink_lum_max };
        let md = measure_region(default, &reg(pd.tl.0 - pm.tl.0, pd.tl.1 - pm.tl.1, 0.0));
        let ml = measure_region(large, &reg(pl.tl.0 - pm.tl.0, pl.tl.1 - pm.tl.1, grow));
        let (Some(ld), Some(ll)) = (first_line(&md, &pd), first_line(&ml, &pl)) else {
            r.flag("insets", &format!("{name} first line found"), false, "a text line", "none", Some(format!("{name}: no text found in one of the snaps (use a fixture with title, heading and body)")));
            continue;
        };
        let s = opts.scale;
        r.num("insets", &format!("{name} left inset (u)"), ld.u, ll.u, TOL, Some(format!("{name}: left inset is {} px vs {} px at the default size → insets are fixed logical values (≈{} logical px off)", fmt(ll.u), fmt(ld.u), fmt((ll.u - ld.u) / s))));
        r.num("insets", &format!("{name} top (v)"), ld.v, ll.v, TOL, Some(format!("{name}: first baseline is {} px below the top edge vs {} px at the default size → top inset must not scale (≈{} logical px off)", fmt(ll.v), fmt(ld.v), fmt((ll.v - ld.v) / s))));
        r.num("fonts", &format!("{name} ink height"), ld.height, ll.height, TOL, Some(format!("{name}: font size changed with the note size (ink height {} vs {})", fmt(ll.height), fmt(ld.height))));
    }

    // toolbar: anchored at the bottom-left corner
    if let Some(Region::Icons { rect, cells, paper, fill, ink_lum_max }) = spec.regions.get("toolbar") {
        let reg = |dx: f64, dy: f64| Region::Icons { rect: shifted(rect, dx, dy, 0.0), cells: *cells, paper: paper.clone(), fill: fill.clone(), ink_lum_max: *ink_lum_max };
        let md = measure_region(default, &reg(pd.bl.0 - pm.bl.0, pd.bl.1 - pm.bl.1));
        let ml = measure_region(large, &reg(pl.bl.0 - pm.bl.0, pl.bl.1 - pm.bl.1));
        let (cd, cl) = (md["cells"].as_array().cloned().unwrap_or_default(), ml["cells"].as_array().cloned().unwrap_or_default());
        r.flag("toolbar", "icon count", cd.len() == cl.len(), &cd.len().to_string(), &cl.len().to_string(), Some("toolbar: icons missing in one of the snaps (is the toolbar visible, i.e. was the snap hovered?)".into()));
        let rel = |b: [f64; 4], p: &Paper| [b[0] - p.bl.0, b[1] - p.bl.1, b[2] - p.bl.0, b[3] - p.bl.1];
        for i in 0..cd.len().min(cl.len()) {
            if let (Some(a), Some(b)) = (bbox(&cd[i]["bbox"]), bbox(&cl[i]["bbox"])) {
                r.bbox("toolbar", &format!("icon {} (from bottom-left)", i + 1), rel(a, &pd), rel(b, &pl), TOL);
            }
        }
        if let (Some(a), Some(b)) = (bbox(&md["active_fill"]["rect"]), bbox(&ml["active_fill"]["rect"])) {
            r.bbox("toolbar", "active button (from bottom-left)", rel(a, &pd), rel(b, &pl), TOL);
        }

        // text area: nothing but toolbar icons inside the band
        let band_top = pl.height - BAND_LOGICAL * opts.scale;
        let icons: Vec<[f64; 4]> = cl.iter().filter_map(|c| bbox(&c["bbox"])).collect();
        let near_icon = |x: f64, y: f64| icons.iter().any(|b| x >= b[0] - 3.0 && x < b[2] + 3.0 && y >= b[1] - 3.0 && y < b[3] + 3.0);
        let (mut stray, mut first) = (0usize, None);
        for y in 0..large.h {
            for x in 0..large.w {
                if lum(large.get(x, y)) >= *ink_lum_max {
                    continue;
                }
                let (u, v) = uv(&pl, x as f64 + 0.5, y as f64 + 0.5);
                if u < 0.0 || u > pl.width || v < band_top || v > pl.height || near_icon(x as f64, y as f64) {
                    continue;
                }
                stray += 1;
                first.get_or_insert((x, y));
            }
        }
        r.push("text area", "ink inside the toolbar band", "0 px".into(), format!("{stray} px"), format!("{stray:+}"), "0".into(), stray == 0,
            first.map(|(x, y)| format!("text area: {stray} dark pixels inside the {BAND_LOGICAL} px toolbar band (first at {x},{y}) → the text area must end above the band's top edge")));
    }
    Ok(r.rows)
}
