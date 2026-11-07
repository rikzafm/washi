//! `uidiff --measure <png> [--spec docs/design/spec.json] [--mockup hover|idle]`
//! `uidiff --update-expected [--spec docs/design/spec.json]`
//!
//! `uidiff --font-match [--fonts <dir>] [--spec docs/design/spec.json]`
//!
//! --font-match scores every .ttf in the font dir (default `fonts/`) against the hover mockup for each
//! role and prints the report as JSON; exit code 1 when a role's best IoU is below 0.85.
//! --measure prints the measurements of the spec regions that apply to the mockup as JSON.
//! The mockup defaults to the one whose file name matches the PNG, else every region is measured.
//! --update-expected measures every mockup and stores the results in the spec's "expected" section.

use std::path::{Path, PathBuf};
use uidiff::{measure_mockup, measure_regions, Image, Spec};

fn usage() -> ! {
    eprintln!("usage: uidiff --measure <png> [--spec <spec.json>] [--mockup <name>]\n       uidiff --update-expected [--spec <spec.json>]\n       uidiff --font-match [--fonts <dir>] [--spec <spec.json>]");
    std::process::exit(2)
}

/// Pretty JSON with 2-space indent, but arrays of scalars stay on one line (rects, bboxes).
fn pretty(v: &serde_json::Value, indent: usize, out: &mut String) {
    use serde_json::Value;
    let pad = "  ".repeat(indent + 1);
    match v {
        Value::Object(m) if !m.is_empty() => {
            out.push_str("{\n");
            for (i, (k, val)) in m.iter().enumerate() {
                out.push_str(&pad);
                out.push_str(&serde_json::to_string(k).unwrap());
                out.push_str(": ");
                pretty(val, indent + 1, out);
                out.push_str(if i + 1 < m.len() { ",\n" } else { "\n" });
            }
            out.push_str(&"  ".repeat(indent));
            out.push('}');
        }
        Value::Array(a) if a.iter().any(|x| x.is_object() || x.is_array()) => {
            out.push_str("[\n");
            for (i, val) in a.iter().enumerate() {
                out.push_str(&pad);
                pretty(val, indent + 1, out);
                out.push_str(if i + 1 < a.len() { ",\n" } else { "\n" });
            }
            out.push_str(&"  ".repeat(indent));
            out.push(']');
        }
        Value::Array(a) => {
            out.push('[');
            for (i, val) in a.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&serde_json::to_string(val).unwrap());
            }
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap()),
    }
}

fn load_spec(path: &Path) -> Result<(Spec, serde_json::Value), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let raw: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let spec: Spec = serde_json::from_value(raw.clone()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((spec, raw))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut spec_path = PathBuf::from("docs/design/spec.json");
    let (mut measure, mut mockup, mut update) = (None, None, false);
    let (mut font_match, mut fonts_dir) = (false, PathBuf::from("fonts"));
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--measure" => {
                i += 1;
                measure = Some(PathBuf::from(args.get(i).ok_or("--measure needs a PNG path")?));
            }
            "--spec" => {
                i += 1;
                spec_path = PathBuf::from(args.get(i).ok_or("--spec needs a path")?);
            }
            "--mockup" => {
                i += 1;
                mockup = Some(args.get(i).ok_or("--mockup needs a name")?.clone());
            }
            "--update-expected" => update = true,
            "--font-match" => font_match = true,
            "--fonts" => {
                i += 1;
                fonts_dir = PathBuf::from(args.get(i).ok_or("--fonts needs a directory")?);
            }
            _ => usage(),
        }
        i += 1;
    }
    let (spec, mut raw) = load_spec(&spec_path)?;
    let dir = spec_path.parent().unwrap_or(Path::new(".")).to_path_buf();

    if font_match {
        let report = uidiff::fontmatch::run(&spec, &raw, &dir, &fonts_dir)?;
        let mut text = String::new();
        pretty(&report, 0, &mut text);
        println!("{text}");
        if report["all_roles_pass"] != true {
            return Err(format!("a role's best IoU is below {}", uidiff::fontmatch::PASS_IOU));
        }
        return Ok(());
    }

    if update {
        let mut expected = serde_json::Map::new();
        for name in spec.mockups.keys() {
            expected.insert(name.clone(), measure_mockup(&spec, name, &dir)?);
        }
        raw["expected"] = serde_json::Value::Object(expected);
        let mut text = String::new();
        pretty(&raw, 0, &mut text);
        text.push('\n');
        std::fs::write(&spec_path, text).map_err(|e| e.to_string())?;
        println!("updated expected values in {}", spec_path.display());
        return Ok(());
    }

    let Some(png) = measure else { usage() };
    let img = Image::load_png(&png)?;
    let stem = png.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let chosen = mockup.or_else(|| spec.mockups.iter().find(|(_, m)| m.file.ends_with(stem)).map(|(n, _)| n.clone()));
    let result = match chosen {
        Some(name) => {
            let m = &spec.mockups[&name];
            measure_regions(&img, &spec, &m.mask, &m.check)?
        }
        None => measure_regions(&img, &spec, &[], spec.regions.keys())?,
    };
    let mut text = String::new();
    pretty(&result, 0, &mut text);
    println!("{text}");
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("uidiff: {e}");
        std::process::exit(1);
    }
}
