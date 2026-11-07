//! Output of `--compare` / `--invariants`: the delta table, the verdict against the known-failure
//! baseline, the images and the report page.

use crate::compare::{delta_e, Comparison, Row};
use crate::image::Image;
use std::collections::BTreeSet;
use std::path::Path;

/// What the run means for CI once the baseline of known failures is applied.
#[derive(Debug, Default, PartialEq)]
pub struct Verdict {
    /// Failing groups that are not in the baseline: regressions, they fail the run.
    pub regressions: Vec<String>,
    /// Failing groups the baseline allows (work not yet done).
    pub known: Vec<String>,
    /// Baseline entries whose group passes now: remove them, so the gain cannot be lost again.
    pub stale: Vec<String>,
}

impl Verdict {
    pub fn ok(&self) -> bool {
        self.regressions.is_empty() && self.stale.is_empty()
    }
}

/// Parse a baseline file: one `<report>:<group>` per line, `#` starts a comment.
pub fn parse_baseline(text: &str) -> Vec<String> {
    text.lines().map(|l| l.split('#').next().unwrap_or("").trim().to_string()).filter(|l| !l.is_empty()).collect()
}

/// Judge `rows` of the report `name` against the baseline entries (`name:group`).
pub fn judge(name: &str, rows: &[Row], baseline: &[String]) -> Verdict {
    let mut groups: Vec<&str> = vec![];
    for r in rows {
        if !groups.contains(&r.group.as_str()) {
            groups.push(&r.group);
        }
    }
    let mut v = Verdict::default();
    let prefix = format!("{name}:");
    for g in groups {
        let failing = rows.iter().any(|r| r.group == g && !r.pass);
        let listed = baseline.iter().any(|b| *b == format!("{prefix}{g}"));
        match (failing, listed) {
            (true, false) => v.regressions.push(g.to_string()),
            (true, true) => v.known.push(g.to_string()),
            (false, true) => v.stale.push(g.to_string()),
            (false, false) => {}
        }
    }
    // entries naming groups this report does not have at all
    for b in baseline.iter().filter(|b| b.starts_with(&prefix)) {
        let g = &b[prefix.len()..];
        if !rows.iter().any(|r| r.group == g) && !v.stale.iter().any(|s| s == g) {
            v.stale.push(g.to_string());
        }
    }
    v
}

fn pad(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(w - n))
    }
}

/// The delta table as aligned text.
pub fn table_text(rows: &[Row], v: &Verdict) -> String {
    let w = |f: &dyn Fn(&Row) -> usize, min: usize| rows.iter().map(f).max().unwrap_or(0).max(min);
    let (wg, wi) = (w(&|r| r.group.chars().count(), 5), w(&|r| r.item.chars().count(), 4));
    let (we, wa, wd, wt) = (w(&|r| r.expected.chars().count(), 8), w(&|r| r.actual.chars().count(), 6), w(&|r| r.delta.chars().count(), 5), w(&|r| r.tol.chars().count(), 3));
    let mut out = format!("{}  {}  {}  {}  {}  {}  result\n", pad("group", wg), pad("item", wi), pad("expected", we), pad("actual", wa), pad("delta", wd), pad("tol", wt));
    for r in rows {
        let status = if r.pass {
            "PASS"
        } else if v.known.contains(&r.group) {
            "FAIL (known)"
        } else {
            "FAIL"
        };
        out.push_str(&format!("{}  {}  {}  {}  {}  {}  {status}\n", pad(&r.group, wg), pad(&r.item, wi), pad(&r.expected, we), pad(&r.actual, wa), pad(&r.delta, wd), pad(&r.tol, wt)));
    }
    out
}

pub fn hints_text(rows: &[Row]) -> String {
    let hints: Vec<&String> = rows.iter().filter_map(|r| r.hint.as_ref()).collect();
    hints.iter().map(|h| format!("  * {h}\n")).collect()
}

pub fn summary_line(name: &str, rows: &[Row], v: &Verdict) -> String {
    let fails = rows.iter().filter(|r| !r.pass).count();
    let status = if v.ok() { "OK" } else { "FAIL" };
    let mut s = format!("{name}: {status}: {} checks, {fails} failing", rows.len());
    if !v.regressions.is_empty() {
        s.push_str(&format!("; REGRESSIONS in {}", v.regressions.join(", ")));
    }
    if !v.known.is_empty() {
        s.push_str(&format!("; known failures in {}", v.known.join(", ")));
    }
    if !v.stale.is_empty() {
        s.push_str(&format!("; these pass now, remove them from the baseline: {}", v.stale.iter().map(|g| format!("{name}:{g}")).collect::<Vec<_>>().join(", ")));
    }
    s
}

/// `report.md` and `result.json` for any set of rows.
pub fn write_report(dir: &Path, name: &str, title: &str, rows: &[Row], v: &Verdict, extra_md: &str, images: &[&str]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut md = format!("# {title}\n\n**{}**\n\n", summary_line(name, rows, v));
    md.push_str(extra_md);
    md.push_str("| group | item | expected | actual | delta | tol | result |\n|---|---|---|---|---|---|---|\n");
    for r in rows {
        let status = if r.pass { "PASS" } else if v.known.contains(&r.group) { "FAIL (known)" } else { "**FAIL**" };
        md.push_str(&format!("| {} | {} | {} | {} | {} | {} | {status} |\n", r.group, r.item, r.expected, r.actual, r.delta, r.tol));
    }
    let hints = hints_text(rows);
    if !hints.is_empty() {
        md.push_str(&format!("\n## Hints\n\n{}", hints.replace("  * ", "* ")));
    }
    if !images.is_empty() {
        md.push_str("\n## Images\n\n");
        for i in images {
            md.push_str(&format!("* [{i}]({i})\n"));
        }
    }
    std::fs::write(dir.join("report.md"), md).map_err(|e| e.to_string())?;
    let failing: BTreeSet<&str> = rows.iter().filter(|r| !r.pass).map(|r| r.group.as_str()).collect();
    let json = serde_json::json!({
        "name": name,
        "title": title,
        "ok": v.ok(),
        "checks": rows.len(),
        "failing_rows": rows.iter().filter(|r| !r.pass).count(),
        "failing_groups": failing,
        "regressions": v.regressions,
        "known": v.known,
        "stale": v.stale,
        "images": images,
    });
    std::fs::write(dir.join("result.json"), serde_json::to_string_pretty(&json).unwrap() + "\n").map_err(|e| e.to_string())
}

fn heat(de: f64) -> [u8; 3] {
    let t = (de / 20.0).clamp(0.0, 1.0);
    let r = (2.0 * t).clamp(0.0, 1.0);
    let g = (2.0 * t - 1.0).clamp(0.0, 1.0);
    [(255.0 * r) as u8, (255.0 * g) as u8, if t == 0.0 { 0 } else { 40 }]
}

/// Images for one comparison: heatmap, overlay, side-by-side, flicker GIF and 4x crops of failures.
/// Returns the file names written.
pub fn write_images(dir: &Path, c: &Comparison) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut names = vec![];
    let (w, h) = (c.mock.w, c.mock.h);

    let mut heatmap = Image::new(w, h, [0, 0, 0]);
    let mut overlay = Image::new(w, h, [0, 0, 0]);
    for y in 0..h {
        for x in 0..w {
            let (a, b) = (c.mock.get(x, y), c.snap.get(x, y));
            heatmap.set(x, y, heat(delta_e(a, b)));
            overlay.set(x, y, [((a[0] as u16 + b[0] as u16) / 2) as u8, ((a[1] as u16 + b[1] as u16) / 2) as u8, ((a[2] as u16 + b[2] as u16) / 2) as u8]);
        }
    }
    let side = c.mock.beside(&c.snap, 8, [0xFF, 0x00, 0xFF]);
    for (file, img) in [("heatmap.png", &heatmap), ("overlay.png", &overlay), ("side-by-side.png", &side)] {
        img.save_png(&dir.join(file))?;
        names.push(file.to_string());
    }

    // flicker: mockup and snapshot alternating every 400 ms
    {
        let file = std::fs::File::create(dir.join("flicker.gif")).map_err(|e| e.to_string())?;
        let mut enc = gif::Encoder::new(std::io::BufWriter::new(file), w as u16, h as u16, &[]).map_err(|e| e.to_string())?;
        enc.set_repeat(gif::Repeat::Infinite).map_err(|e| e.to_string())?;
        for img in [&c.mock, &c.snap] {
            let mut rgb = img.rgb_bytes();
            let mut frame = gif::Frame::from_rgb_speed(w as u16, h as u16, &mut rgb, 10);
            frame.delay = 40;
            enc.write_frame(&frame).map_err(|e| e.to_string())?;
        }
        names.push("flicker.gif".into());
    }

    for (group, rect) in &c.fail_rects {
        let a = c.mock.crop(*rect).zoom(4);
        let b = c.snap.crop(*rect).zoom(4);
        let file = format!("crop-{}.png", group.replace(' ', "-"));
        a.beside(&b, 8, [0xFF, 0x00, 0xFF]).save_png(&dir.join(&file))?;
        names.push(file);
    }
    Ok(names)
}
