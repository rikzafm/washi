//! `--compare`: register a snapshot on the mockup, measure both, and judge every property
//! against the tolerances of STEP_BY_STEP_PLAN ("What matches exactly means").

use crate::image::{parse_hex, r3, Image, Rgb};
use crate::spec::{Rect, Region, Spec};
use crate::{measure_region, measure_regions};
use serde_json::Value;

/// Desk color the snapshots are composited on (also used to fill registration gaps).
pub const DESK: Rgb = [0xD9, 0xDD, 0xE1];

// Tolerances, in mockup pixels unless stated.
pub const TOL_PAPER_BBOX: f64 = 1.0;
pub const TOL_PAPER_TILT: f64 = 0.1;
pub const TOL_PAPER_RADIUS: f64 = 1.0;
pub const TOL_TAPE_BBOX: f64 = 2.0;
pub const TOL_TAPE_ANGLE: f64 = 0.5;
pub const TOL_TAPE_PITCH: f64 = 1.0;
pub const TOL_DELTA_E: f64 = 2.0;
pub const TOL_TEXT_POS: f64 = 2.0;
pub const TOL_TEXT_HEIGHT: f64 = 1.0;
/// Relative tolerance on ink coverage (stroke weight).
pub const TOL_TEXT_COVERAGE: f64 = 0.05;
pub const TOL_ICON: f64 = 1.0;
pub const TOL_SHADOW_MAE: f64 = 2.0;
pub const SHADOW_RING: f64 = 40.0;
/// Pixels with delta-E above this count as different.
pub const PIXEL_DE: f64 = 5.0;
/// Share of pixels allowed to differ, in percent.
pub const TOL_IMAGE_PCT: f64 = 2.0;

pub struct Options {
    /// Translate the snapshot so its paper centre lands on the mockup's.
    pub register: bool,
    /// Mockup pixels per logical pixel (for hints).
    pub scale: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options { register: true, scale: 1.5 }
    }
}

#[derive(Clone, Debug)]
pub struct Row {
    /// The spec region it belongs to ("title", "paper", ...), or "image" / "shadow".
    pub group: String,
    pub item: String,
    pub expected: String,
    pub actual: String,
    pub delta: String,
    pub tol: String,
    pub pass: bool,
    pub hint: Option<String>,
}

pub struct Comparison {
    pub name: String,
    pub rows: Vec<Row>,
    /// Translation applied to the snapshot.
    pub shift: (i64, i64),
    pub diff_pixels: usize,
    pub diff_pct: f64,
    /// Mockup and registered snapshot, both with the masks painted over.
    pub mock: Image,
    pub snap: Image,
    /// Pixel rectangles to crop for each failing group.
    pub fail_rects: Vec<(String, Rect)>,
}

impl Comparison {
    pub fn passed(&self) -> bool {
        self.rows.iter().all(|r| r.pass)
    }
}

// ---------- colour ----------

fn lin(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn lab(c: Rgb) -> [f64; 3] {
    let (r, g, b) = (lin(c[0]), lin(c[1]), lin(c[2]));
    let x = (0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = (0.0193339 * r + 0.1191920 * g + 0.9503041 * b) / 1.08883;
    let f = |t: f64| if t > 216.0 / 24389.0 { t.cbrt() } else { (24389.0 / 27.0 * t + 16.0) / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIE76 colour difference.
pub fn delta_e(a: Rgb, b: Rgb) -> f64 {
    if a == b {
        return 0.0;
    }
    let (la, lb) = (lab(a), lab(b));
    ((la[0] - lb[0]).powi(2) + (la[1] - lb[1]).powi(2) + (la[2] - lb[2]).powi(2)).sqrt()
}

// ---------- registration ----------

/// Copy of `img` moved by (dx, dy), same size, gaps filled with `fill`.
pub fn translate(img: &Image, dx: i64, dy: i64, fill: Rgb) -> Image {
    let mut out = Image::new(img.w, img.h, fill);
    for y in 0..img.h {
        for x in 0..img.w {
            let (sx, sy) = (x as i64 - dx, y as i64 - dy);
            if sx >= 0 && sy >= 0 && (sx as usize) < img.w && (sy as usize) < img.h {
                out.set(x, y, img.get(sx as usize, sy as usize));
            }
        }
    }
    out
}

/// Same-size copy of `img` on a canvas of `w` x `h` (cropped or padded with `fill` at the top-left).
fn fit_canvas(img: &Image, w: usize, h: usize, fill: Rgb) -> Image {
    let mut out = Image::new(w, h, fill);
    for y in 0..h.min(img.h) {
        for x in 0..w.min(img.w) {
            out.set(x, y, img.get(x, y));
        }
    }
    out
}

fn paper_center(m: &Value) -> Option<(f64, f64)> {
    if m["found"] != true {
        return None;
    }
    Some((m["center"]["x"].as_f64()?, m["center"]["y"].as_f64()?))
}

// ---------- row helpers ----------

pub(crate) fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}

pub(crate) fn bbox(v: &Value) -> Option<[f64; 4]> {
    let a = v.as_array()?;
    if a.len() != 4 {
        return None;
    }
    Some([f(&a[0]), f(&a[1]), f(&a[2]), f(&a[3])])
}

pub(crate) fn fmt(v: f64) -> String {
    let r = r3(v);
    if r == r.trunc() {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

pub(crate) struct Rows {
    pub(crate) rows: Vec<Row>,
    pub(crate) scale: f64,
}

impl Rows {
    pub(crate) fn push(&mut self, group: &str, item: &str, expected: String, actual: String, delta: String, tol: String, pass: bool, hint: Option<String>) {
        self.rows.push(Row { group: group.into(), item: item.into(), expected, actual, delta, tol, pass, hint });
    }

    pub(crate) fn num(&mut self, group: &str, item: &str, exp: f64, act: f64, tol: f64, hint: Option<String>) {
        let d = act - exp;
        let pass = d.abs() <= tol + 1e-9;
        self.push(group, item, fmt(exp), fmt(act), format!("{:+}", r3(d)), format!("±{}", fmt(tol)), pass, if pass { None } else { hint });
    }

    pub(crate) fn color(&mut self, group: &str, item: &str, exp: &str, act: &str) {
        let (e, a) = (parse_hex(exp).unwrap_or([0; 3]), parse_hex(act).unwrap_or([0; 3]));
        let de = delta_e(e, a);
        self.push(group, item, exp.into(), act.into(), format!("ΔE {}", fmt(de)), format!("ΔE ≤ {}", fmt(TOL_DELTA_E)), de <= TOL_DELTA_E, None);
    }

    pub(crate) fn flag(&mut self, group: &str, item: &str, ok: bool, expected: &str, actual: &str, hint: Option<String>) {
        self.push(group, item, expected.into(), actual.into(), String::new(), "equal".into(), ok, if ok { None } else { hint });
    }

    /// Compare two boxes edge by edge; one shared "move" hint when the whole box is displaced.
    pub(crate) fn bbox(&mut self, group: &str, label: &str, exp: [f64; 4], act: [f64; 4], tol: f64) {
        let names = ["x0", "y0", "x1", "y1"];
        let d: Vec<f64> = (0..4).map(|i| act[i] - exp[i]).collect();
        let hint = shift_hint(&format!("{group} {label}"), (d[0] + d[2]) / 2.0, (d[1] + d[3]) / 2.0, (d[0] - d[2]).abs(), (d[1] - d[3]).abs(), tol, self.scale);
        let mut first = true;
        for i in 0..4 {
            let h = if first && d[i].abs() > tol { first = false; hint.clone() } else { None };
            self.num(group, &format!("{label} {}", names[i]), exp[i], act[i], tol, h);
        }
    }
}

/// "move left 2 px" style hint for a displacement of the snapshot relative to the mockup.
fn shift_hint(label: &str, dx: f64, dy: f64, xspread: f64, yspread: f64, tol: f64, scale: f64) -> Option<String> {
    let mut parts = vec![];
    // a displacement, not a resize: both edges moved by (nearly) the same amount
    if dx.abs() > tol && xspread <= 1.5 {
        let n = dx.abs().round() as i64;
        parts.push(format!("move {} {} px (≈{} logical px)", if dx > 0.0 { "left" } else { "right" }, n, fmt(dx.abs() / scale)));
    }
    if dy.abs() > tol && yspread <= 1.5 {
        let n = dy.abs().round() as i64;
        parts.push(format!("move {} {} px (≈{} logical px)", if dy > 0.0 { "up" } else { "down" }, n, fmt(dy.abs() / scale)));
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("{label}: {}", parts.join(", ")))
    }
}

// ---------- per-region comparison ----------

fn compare_paper(r: &mut Rows, exp: &Value, act: &Value) {
    if act["found"] != true {
        r.flag("paper", "found", false, "paper", "no paper found", Some("paper: no pixels of the paper color in the snapshot (is the paper drawn, and is it #FFFFFD?)".into()));
        return;
    }
    if let (Some(e), Some(a)) = (bbox(&exp["bbox"]), bbox(&act["bbox"])) {
        r.bbox("paper", "bbox", e, a, TOL_PAPER_BBOX);
    }
    let size_hint = |what: &str, d: f64, scale: f64| Some(format!("paper {what} is {} px too {} → change the paper {what} by ≈{} logical px", fmt(d.abs()), if d > 0.0 { "large" } else { "small" }, fmt(d.abs() / scale)));
    let (dw, dh) = (f(&act["width"]) - f(&exp["width"]), f(&act["height"]) - f(&exp["height"]));
    let s = r.scale;
    r.num("paper", "width", f(&exp["width"]), f(&act["width"]), TOL_PAPER_BBOX, size_hint("width", dw, s));
    r.num("paper", "height", f(&exp["height"]), f(&act["height"]), TOL_PAPER_BBOX, size_hint("height", dh, s));
    let dt = f(&act["tilt_deg"]) - f(&exp["tilt_deg"]);
    r.num("paper", "tilt (deg)", f(&exp["tilt_deg"]), f(&act["tilt_deg"]), TOL_PAPER_TILT, Some(format!("paper: rotate by {:+} deg", r3(-dt))));
    r.num("paper", "corner radius", f(&exp["corner_radius"]), f(&act["corner_radius"]), TOL_PAPER_RADIUS, None);
}

fn compare_tape(r: &mut Rows, group: &str, exp: &Value, act: &Value) {
    if act["found"] != true {
        r.flag(group, "found", false, "tape", "no tape found", Some(format!("{group}: no tape-colored pixels found in the snapshot")));
        return;
    }
    if let (Some(e), Some(a)) = (bbox(&exp["bbox"]), bbox(&act["bbox"])) {
        r.bbox(group, "bbox", e, a, TOL_TAPE_BBOX);
    }
    let da = f(&act["angle_deg"]) - f(&exp["angle_deg"]);
    r.num(group, "angle (deg)", f(&exp["angle_deg"]), f(&act["angle_deg"]), TOL_TAPE_ANGLE, Some(format!("{group}: rotate by {:+} deg", r3(-da))));
    r.num(group, "stripe pitch", f(&exp["stripe_pitch"]), f(&act["stripe_pitch"]), TOL_TAPE_PITCH, Some(format!("{group}: stripe pitch off by {} px", fmt(f(&act["stripe_pitch"]) - f(&exp["stripe_pitch"])))));
    for k in ["base", "stripe"] {
        if let (Some(e), Some(a)) = (exp[k].as_str(), act[k].as_str()) {
            r.color(group, &format!("{k} color"), e, a);
        }
    }
}

fn compare_text(r: &mut Rows, group: &str, exp: &Value, act: &Value) {
    let (el, al) = (exp["lines"].as_array().cloned().unwrap_or_default(), act["lines"].as_array().cloned().unwrap_or_default());
    let mut hint = None;
    if el.len() != al.len() {
        // find the first line whose right edge differs a lot: that is where the wrapping diverged
        let diverge = (0..el.len().min(al.len())).find(|&i| match (bbox(&el[i]["bbox"]), bbox(&al[i]["bbox"])) {
            (Some(e), Some(a)) => (e[2] - a[2]).abs() > 8.0,
            _ => false,
        });
        hint = Some(match diverge {
            Some(i) => {
                let (e, a) = (bbox(&el[i]["bbox"]).unwrap(), bbox(&al[i]["bbox"]).unwrap());
                let d = e[2] - a[2];
                if d > 0.0 {
                    format!("{group}: line {} breaks {} px earlier than the mockup → content width too small by ≈{} logical px", i + 1, fmt(d), fmt(d / r.scale))
                } else {
                    format!("{group}: line {} runs {} px longer than the mockup → content width too large by ≈{} logical px", i + 1, fmt(-d), fmt(-d / r.scale))
                }
            }
            None => format!("{group}: {} lines in the snapshot vs {} in the mockup (wrapping differs; check width and font size)", al.len(), el.len()),
        });
    }
    r.flag(group, "line count", el.len() == al.len(), &el.len().to_string(), &al.len().to_string(), hint);
    for i in 0..el.len().min(al.len()) {
        let label = format!("line {}", i + 1);
        let (Some(e), Some(a)) = (bbox(&el[i]["bbox"]), bbox(&al[i]["bbox"])) else { continue };
        // x, y of the ink box
        let (dx, dy) = (a[0] - e[0], a[1] - e[1]);
        let h = shift_hint(&format!("{group} {label}"), dx, dy, 0.0, 0.0, TOL_TEXT_POS, r.scale);
        r.num(group, &format!("{label} x"), e[0], a[0], TOL_TEXT_POS, h.clone().filter(|_| dx.abs() > TOL_TEXT_POS));
        r.num(group, &format!("{label} y"), e[1], a[1], TOL_TEXT_POS, h.filter(|_| dy.abs() > TOL_TEXT_POS));
        let (eh, ah) = (f(&el[i]["ink_height"]), f(&al[i]["ink_height"]));
        let dh = eh - ah;
        let fs = dh / r.scale / 1.5;
        r.num(group, &format!("{label} ink height"), eh, ah, TOL_TEXT_HEIGHT,
            Some(format!("{group} {label}: ink height {:+} px → font-size ≈ {:+} logical px", r3(ah - eh), fmt_signed(fs))));
        let (ec, ac) = (f(&el[i]["coverage"]), f(&al[i]["coverage"]));
        let rel = if ec > 0.0 { (ac - ec) / ec } else { 0.0 };
        let pass = rel.abs() <= TOL_TEXT_COVERAGE;
        r.push(group, &format!("{label} coverage"), fmt(ec), fmt(ac), format!("{:+}%", r3(rel * 100.0)), format!("±{}%", TOL_TEXT_COVERAGE * 100.0), pass,
            if pass { None } else { Some(format!("{group} {label}: ink coverage {:+}% → font weight is {}", r3(rel * 100.0), if rel < 0.0 { "too light" } else { "too heavy" })) });
    }
}

fn fmt_signed(v: f64) -> String {
    let r = r3(v);
    if r > 0.0 {
        format!("+{}", fmt(r))
    } else {
        fmt(r)
    }
}

fn compare_icons(r: &mut Rows, exp: &Value, act: &Value) {
    let (ec, ac) = (exp["cells"].as_array().cloned().unwrap_or_default(), act["cells"].as_array().cloned().unwrap_or_default());
    r.flag("toolbar", "icon count", ec.len() == ac.len(), &ec.len().to_string(), &ac.len().to_string(), Some(format!("toolbar: {} icons found, the mockup has {}", ac.len(), ec.len())));
    for i in 0..ec.len().min(ac.len()) {
        if let (Some(e), Some(a)) = (bbox(&ec[i]["bbox"]), bbox(&ac[i]["bbox"])) {
            r.bbox("toolbar", &format!("icon {}", i + 1), e, a, TOL_ICON);
        }
    }
    match (bbox(&exp["active_fill"]["rect"]), bbox(&act["active_fill"]["rect"])) {
        (Some(e), Some(a)) => {
            r.bbox("toolbar", "active button", e, a, TOL_ICON);
            r.num("toolbar", "button radius", f(&exp["active_fill"]["radius"]), f(&act["active_fill"]["radius"]), TOL_ICON, None);
        }
        (Some(_), None) => r.flag("toolbar", "active button", false, "filled button", "none", Some("toolbar: no filled active button found".into())),
        _ => {}
    }
}

fn compare_empty(r: &mut Rows, group: &str, exp: &Value, act: &Value) {
    let (e, a) = (f(&exp["non_paper_px"]), f(&act["non_paper_px"]));
    let hint = bbox(&act["bbox"]).map(|b| format!("{group}: {} stray pixels in {:?}: something is drawn where the mockup shows bare paper (toolbar visible while idle?)", fmt(a), b));
    r.num(group, "non-paper pixels", e, a, 0.0, hint);
}

// ---------- shadow and whole image ----------

fn dist_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
    ((p.0 - (a.0 + t * vx)).powi(2) + (p.1 - (a.1 + t * vy)).powi(2)).sqrt()
}

/// Mean absolute error (0-255) in the ring around the paper quad, outside the paper.
fn shadow_mae(mock: &Image, snap: &Image, corners: [(f64, f64); 4]) -> f64 {
    let inside = |p: (f64, f64)| {
        (0..4).all(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0) >= 0.0
        })
    };
    let (mut sum, mut n) = (0.0, 0usize);
    for y in 0..mock.h {
        for x in 0..mock.w {
            let p = (x as f64 + 0.5, y as f64 + 0.5);
            if inside(p) {
                continue;
            }
            let d = (0..4).map(|i| dist_to_segment(p, corners[i], corners[(i + 1) % 4])).fold(f64::MAX, f64::min);
            if d > SHADOW_RING {
                continue;
            }
            let (a, b) = (mock.get(x, y), snap.get(x, y));
            sum += (0..3).map(|k| a[k].abs_diff(b[k]) as f64).sum::<f64>() / 3.0;
            n += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        sum / n as f64
    }
}

pub(crate) fn corners(paper: &Value) -> Option<[(f64, f64); 4]> {
    let c = |k: &str| -> Option<(f64, f64)> { Some((paper["corners"][k][0].as_f64()?, paper["corners"][k][1].as_f64()?)) };
    // clockwise on screen (y down): tl, tr, br, bl
    Some([c("tl")?, c("tr")?, c("br")?, c("bl")?])
}

fn region_rect(spec: &Spec, name: &str, exp: &Value) -> Option<Rect> {
    match spec.regions.get(name)? {
        Region::Empty { rect, .. } | Region::Icons { rect, .. } | Region::Text { rect, .. } => Some(*rect),
        Region::Tape { .. } | Region::Paper { .. } => {
            let b = bbox(&exp["bbox"])?;
            Some([b[0] as i64 - 8, b[1] as i64 - 8, b[2] as i64 + 8, b[3] as i64 + 8])
        }
    }
}

// ---------- entry point ----------

pub fn compare(spec: &Spec, name: &str, mock: &Image, snap: &Image, opts: &Options) -> Result<Comparison, String> {
    let m = spec.mockups.get(name).ok_or_else(|| format!("mockup '{name}' is not in the spec"))?;
    let paper_region = spec.regions.get("paper").ok_or("spec has no 'paper' region")?;

    // registration on the paper centre
    let snap = fit_canvas(snap, mock.w, mock.h, DESK);
    let shift = if opts.register {
        match (paper_center(&measure_region(mock, paper_region)), paper_center(&measure_region(&snap, paper_region))) {
            (Some(a), Some(b)) => ((a.0 - b.0).round() as i64, (a.1 - b.1).round() as i64),
            _ => (0, 0),
        }
    } else {
        (0, 0)
    };
    let mut snap = if shift == (0, 0) { snap } else { translate(&snap, shift.0, shift.1, DESK) };

    // masks: painted over on both, so they cannot differ
    let paper_rgb = parse_hex(&spec.paper_color)?;
    let mut mock = mock.clone();
    let mut masked = vec![false; mock.w * mock.h];
    for rect in &m.mask {
        let (x0, y0, x1, y1) = crate::empty::clamp_rect(&mock, rect);
        for y in y0..y1 {
            for x in x0..x1 {
                mock.set(x, y, paper_rgb);
                snap.set(x, y, paper_rgb);
                masked[y * mock.w + x] = true;
            }
        }
    }

    let exp = measure_regions(&mock, spec, &[], m.check.iter())?;
    let act = measure_regions(&snap, spec, &[], m.check.iter())?;

    let mut r = Rows { rows: vec![], scale: opts.scale };
    for name in &m.check {
        let (e, a) = (&exp[name], &act[name]);
        match &spec.regions[name] {
            Region::Paper { .. } => compare_paper(&mut r, e, a),
            Region::Tape { .. } => compare_tape(&mut r, name, e, a),
            Region::Text { .. } => compare_text(&mut r, name, e, a),
            Region::Icons { .. } => compare_icons(&mut r, e, a),
            Region::Empty { .. } => compare_empty(&mut r, name, e, a),
        }
    }

    // shadow ring (needs the paper)
    if m.check.iter().any(|n| n == "paper") {
        if let (Some(c), true) = (corners(&exp["paper"]), act["paper"]["found"] == true) {
            let mae = shadow_mae(&mock, &snap, c);
            r.num("shadow", "ring MAE (levels)", 0.0, mae, TOL_SHADOW_MAE, Some(format!("shadow: mean error {} levels in the {} px ring around the paper → adjust shadow blur/offset/opacity", fmt(mae), SHADOW_RING)));
        } else {
            r.flag("shadow", "measured", false, "measured", "not measured", Some("shadow: cannot be measured without the paper".into()));
        }
    }

    // whole image
    let (mut diff, mut total) = (0usize, 0usize);
    for y in 0..mock.h {
        for x in 0..mock.w {
            if masked[y * mock.w + x] {
                continue;
            }
            total += 1;
            if delta_e(mock.get(x, y), snap.get(x, y)) > PIXEL_DE {
                diff += 1;
            }
        }
    }
    let pct = if total == 0 { 0.0 } else { diff as f64 * 100.0 / total as f64 };
    r.push("image", "pixels with ΔE > 5", "0%".into(), format!("{}%", fmt(pct)), format!("{diff} px"), format!("≤ {}%", fmt(TOL_IMAGE_PCT)), pct <= TOL_IMAGE_PCT,
        if pct <= TOL_IMAGE_PCT { None } else { Some(format!("image: {}% of pixels differ by more than ΔE 5; see heatmap.png (glyph edges only is expected)", fmt(pct))) });

    let mut fail_rects: Vec<(String, Rect)> = vec![];
    for row in &r.rows {
        if !row.pass && !fail_rects.iter().any(|(g, _)| *g == row.group) {
            if let Some(rect) = region_rect(spec, &row.group, &exp[&row.group]) {
                fail_rects.push((row.group.clone(), rect));
            }
        }
    }
    Ok(Comparison { name: name.into(), rows: r.rows, shift, diff_pixels: diff, diff_pct: pct, mock, snap, fail_rects })
}
