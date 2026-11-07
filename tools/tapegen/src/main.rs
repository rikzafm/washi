//! Dev-only washi tape generator. Draws the two tape images from the values in
//! `docs/design/spec.json` (layout.tapes: size, angle, stripe pitch and angle, colors, alpha, edge).
//!
//!   cargo run --manifest-path tools/tapegen/Cargo.toml            # rewrites washi/ui/assets/tape-*.png
//!   cargo test --manifest-path tools/tapegen/Cargo.toml           # the committed images are up to date
//!
//! A tape is a plain rectangle (spec: edge "square"), already rotated by its angle on the
//! screen (angle relative to the paper plus the paper tilt), drawn at `IMAGE_SCALE` pixels per
//! logical px so the app scales it down for any display. Colors in the spec are the colors seen
//! over the white paper; the tape is translucent, so the stored colors are solved from them.

use serde_json::Value;
use std::path::{Path, PathBuf};

/// Pixels per logical px of the tape images. The app divides the image size by this (ui/paper-theme.slint).
pub const IMAGE_SCALE: f64 = 3.0;
/// Subsamples per pixel side for anti-aliasing.
const SUPER: usize = 4;

pub struct Tape {
    pub width: u32,
    pub height: u32,
    /// Straight (not premultiplied) RGBA.
    pub rgba: Vec<u8>,
}

fn num(v: &Value, what: &str) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("spec.json: {what} is not a number"))
}

fn hex(v: &Value, what: &str) -> [f64; 3] {
    let s = v.as_str().unwrap_or_else(|| panic!("spec.json: {what} is not a string"));
    let n = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or_else(|_| panic!("spec.json: {what} is not a color"));
    [(n >> 16 & 255) as f64, (n >> 8 & 255) as f64, (n & 255) as f64]
}

/// Draw one tape (`"pink"` or `"blue"`) from the parsed spec.
pub fn render(spec: &Value, which: &str) -> Tape {
    let tapes = &spec["layout"]["tapes"];
    assert_eq!(tapes["edge"].as_str(), Some("square"), "spec.json: only square tape ends are drawn");
    let (len, wid) = (num(&tapes["size"][0], "tape length"), num(&tapes["size"][1], "tape width"));
    let angle = (num(&tapes["angle_relative_to_paper_deg"], "tape angle") + num(&spec["paper_tilt_deg"]["value"], "paper tilt")).to_radians();
    let pitch = num(&tapes["stripe_pitch"], "stripe pitch");
    let stripe_angle = num(&tapes["stripe_angle_relative_to_tape_deg"], "stripe angle").to_radians();
    let duty = num(&tapes["stripe_duty"], "stripe duty");
    let phase = num(&tapes["stripe_phase"], "stripe phase");
    let alpha = num(&tapes["alpha"], "alpha");
    let paper = hex(&spec["paper_color"], "paper color");
    // seen over the paper = alpha * stored + (1 - alpha) * paper
    let stored = |c: [f64; 3]| [0, 1, 2].map(|k| ((c[k] - (1.0 - alpha) * paper[k]) / alpha).clamp(0.0, 255.0));
    let base = stored(hex(&tapes[which]["base"], "tape base"));
    let stripe = stored(hex(&tapes[which]["stripe"], "tape stripe"));

    let (ca, sa) = (angle.cos(), angle.sin());
    let (bw, bh) = (len * ca.abs() + wid * sa.abs(), len * sa.abs() + wid * ca.abs());
    let (w, h) = ((bw * IMAGE_SCALE).ceil() as u32 + 2, (bh * IMAGE_SCALE).ceil() as u32 + 2);
    let (n_x, n_y) = (-stripe_angle.sin(), stripe_angle.cos());
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let (mut cov, mut acc) = (0.0, [0.0f64; 3]);
            for sy in 0..SUPER {
                for sx in 0..SUPER {
                    // logical px from the image center, y down
                    let px = (x as f64 + (sx as f64 + 0.5) / SUPER as f64 - w as f64 / 2.0) / IMAGE_SCALE;
                    let py = (y as f64 + (sy as f64 + 0.5) / SUPER as f64 - h as f64 / 2.0) / IMAGE_SCALE;
                    // into the tape's own frame (undo the rotation)
                    let (qx, qy) = (px * ca + py * sa, -px * sa + py * ca);
                    if qx.abs() > len / 2.0 || qy.abs() > wid / 2.0 {
                        continue;
                    }
                    let s = (qx * n_x + qy * n_y) / pitch + phase;
                    let c = if s - s.floor() < duty { base } else { stripe };
                    cov += 1.0;
                    for k in 0..3 {
                        acc[k] += c[k];
                    }
                }
            }
            if cov > 0.0 {
                let i = ((y * w + x) * 4) as usize;
                for k in 0..3 {
                    rgba[i + k] = (acc[k] / cov).round() as u8;
                }
                rgba[i + 3] = (alpha * 255.0 * cov / (SUPER * SUPER) as f64).round() as u8;
            }
        }
    }
    Tape { width: w, height: h, rgba }
}

pub fn png_bytes(t: &Tape) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, t.width, t.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Best);
    let mut w = enc.write_header().expect("png header");
    w.write_image_data(&t.rgba).expect("png data");
    drop(w);
    out
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn load_spec() -> Value {
    let p = repo_root().join("docs/design/spec.json");
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).expect("spec.json")
}

pub fn asset_path(which: &str) -> PathBuf {
    repo_root().join(format!("washi/ui/assets/tape-{which}.png"))
}

fn main() {
    let spec = load_spec();
    for which in ["pink", "blue"] {
        let t = render(&spec, which);
        let p = asset_path(which);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, png_bytes(&t)).unwrap();
        println!("{} ({}x{} px, {} logical)", p.display(), t.width, t.height, t.width as f64 / IMAGE_SCALE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The number of a theme token such as `out property <length> margin: 40px;`.
    fn token(theme: &str, name: &str) -> f64 {
        let line = theme.lines().find(|l| l.contains(&format!(" {name}:"))).unwrap_or_else(|| panic!("paper-theme.slint has no token {name}"));
        let v = line.split(&format!(" {name}:")).nth(1).unwrap().trim().trim_end_matches(';').trim();
        v.trim_end_matches("px").trim_end_matches("deg").trim().parse().unwrap_or_else(|_| panic!("token {name} is not a number: {v}"))
    }

    #[test]
    fn theme_tokens_agree_with_the_spec() {
        let spec = load_spec();
        let theme = std::fs::read_to_string(repo_root().join("washi/ui/paper-theme.slint")).unwrap();
        let l = &spec["layout"];
        assert_eq!(token(&theme, "margin"), num(&l["window_margin"]["value"], "margin"));
        assert_eq!(token(&theme, "paper-min-width"), num(&l["paper"]["min"][0], "min w"));
        assert_eq!(token(&theme, "paper-min-height"), num(&l["paper"]["min"][1], "min h"));
        assert_eq!(token(&theme, "paper-default-width"), num(&l["paper"]["default"][0], "w"));
        assert_eq!(token(&theme, "paper-default-height"), num(&l["paper"]["default"][1], "h"));
        assert_eq!(token(&theme, "paper-tilt"), num(&spec["paper_tilt_deg"]["value"], "tilt"));
        assert_eq!(token(&theme, "toolbar-band"), num(&l["toolbar"]["band_height"], "band"));
        assert_eq!(token(&theme, "tape-image-scale"), IMAGE_SCALE);
        let t = &l["tapes"];
        assert_eq!(token(&theme, "tape-pink-offset-u"), num(&t["pink"]["center_offset_from_corner_uv"][0], "pink u"));
        assert_eq!(token(&theme, "tape-pink-offset-v"), num(&t["pink"]["center_offset_from_corner_uv"][1], "pink v"));
        assert_eq!(token(&theme, "tape-blue-offset-u"), num(&t["blue"]["center_offset_from_corner_uv"][0], "blue u"));
        assert_eq!(token(&theme, "tape-blue-offset-v"), num(&t["blue"]["center_offset_from_corner_uv"][1], "blue v"));
        assert!(theme.contains(&spec["paper_color"].as_str().unwrap().to_uppercase()), "paper color differs from the spec");
    }

    #[test]
    fn committed_images_are_what_the_generator_makes() {
        let spec = load_spec();
        for which in ["pink", "blue"] {
            let want = png_bytes(&render(&spec, which));
            let have = std::fs::read(asset_path(which)).unwrap_or_default();
            assert!(want == have, "washi/ui/assets/tape-{which}.png is stale: run `cargo run --manifest-path tools/tapegen/Cargo.toml`");
        }
    }

    #[test]
    fn the_image_is_the_rotated_tape_with_two_colors() {
        let spec = load_spec();
        let t = render(&spec, "pink");
        // 113 x 33.1 rotated -33.5 deg: bbox about 112.5 x 90 logical (the mockup's 168 x 135 px)
        let (lw, lh) = (t.width as f64 / IMAGE_SCALE, t.height as f64 / IMAGE_SCALE);
        assert!((lw - 112.5).abs() < 1.5 && (lh - 90.0).abs() < 1.5, "{lw} x {lh}");
        // corners of the image are empty, the center is the tape
        assert_eq!(t.rgba[3], 0);
        let c = ((t.height / 2 * t.width + t.width / 2) * 4) as usize;
        assert_eq!(t.rgba[c + 3], (0.88f64 * 255.0).round() as u8);
        // every opaque interior pixel is the base or the stripe color
        let mut colors = std::collections::BTreeSet::new();
        for p in t.rgba.chunks(4).filter(|p| p[3] == t.rgba[c + 3]) {
            colors.insert([p[0], p[1], p[2]]);
        }
        // (the stripe edges are anti-aliased, so the colors in between appear too)
        assert_eq!(colors.iter().next(), Some(&[246, 182, 198]), "base solved from #F7BFCD at 88%");
        assert_eq!(colors.iter().next_back(), Some(&[252, 216, 226]), "stripe solved from #FCDDE5 at 88%");
    }
}
