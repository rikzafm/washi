//! `--index <dir>`: one HTML page with every report under the fidelity output folder.

use std::path::{Path, PathBuf};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn strings(v: &serde_json::Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// Write `index.html` in `root`: every sub-folder with a `result.json`. Returns the path.
pub fn write_index(root: &Path) -> Result<PathBuf, String> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .map_err(|e| format!("{}: {e}", root.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.join("result.json").is_file())
        .collect();
    dirs.sort();
    let (mut head, mut body, mut all_ok) = (String::new(), String::new(), true);
    for d in &dirs {
        let dname = d.file_name().unwrap().to_string_lossy().to_string();
        let r: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(d.join("result.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let ok = r["ok"] == true;
        all_ok &= ok;
        head.push_str(&format!(
            "<tr><td><a href=\"#{d}\">{d}</a></td><td class=\"{c}\">{s}</td><td>{n}</td><td>{k}</td><td>{g}</td></tr>\n",
            d = esc(&dname),
            c = if ok { "ok" } else { "bad" },
            s = if ok { "OK" } else { "FAIL" },
            n = r["failing_rows"],
            k = esc(&strings(&r["known"]).join(", ")),
            g = esc(&strings(&r["regressions"]).join(", "))
        ));
        body.push_str(&format!("<section id=\"{}\"><h2>{}</h2>\n", esc(&dname), esc(r["title"].as_str().unwrap_or(&dname))));
        let images = strings(&r["images"]);
        let figure = |f: &String| format!("<figure><figcaption>{}</figcaption><img src=\"{}/{}\"></figure>\n", esc(f), esc(&dname), esc(f));
        for f in images.iter().filter(|f| f.ends_with(".png") && !f.starts_with("crop-")) {
            body.push_str(&figure(f));
        }
        for f in images.iter().filter(|f| f.starts_with("crop-") || f.ends_with(".gif")) {
            body.push_str(&figure(f));
        }
        let md = std::fs::read_to_string(d.join("report.md")).unwrap_or_default();
        body.push_str(&format!("<pre>{}</pre></section>\n", esc(&md)));
    }
    let html = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>Washi fidelity</title>\n<style>body{{font:14px system-ui;margin:24px;background:#fafafa}}table{{border-collapse:collapse}}td,th{{border:1px solid #bbb;padding:4px 10px}}.ok{{background:#cfc}}.bad{{background:#fbb}}figure{{display:inline-block;margin:6px;vertical-align:top}}img{{max-width:48vw;image-rendering:pixelated;border:1px solid #ccc}}pre{{background:#fff;padding:8px;overflow:auto}}</style>\n<h1>Washi fidelity: {}</h1>\n<table><tr><th>report</th><th>status</th><th>failing rows</th><th>known failures</th><th>regressions</th></tr>\n{head}</table>\n{body}",
        if all_ok { "OK" } else { "FAIL" }
    );
    let path = root.join("index.html");
    std::fs::write(&path, html).map_err(|e| e.to_string())?;
    Ok(path)
}
