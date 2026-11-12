//! Path data for the mini tapes on the Font & Style preview.
//!
//! The software renderer cannot draw a gradient on a tilted shape, so a tape is a flat parallelogram in
//! its base color with diagonal stripe polygons on top. Everything is computed here, in the tape's own
//! box (`tape_box`), and handed to Slint as SVG path commands.

/// A point in the tape's local frame (origin at its center, y down).
type P = (f32, f32);

/// Width and height of the box that holds a `w` by `h` tape rotated by `angle_deg`.
pub fn tape_box(w: f32, h: f32, angle_deg: f32) -> (f32, f32) {
    let (s, c) = angle_deg.to_radians().sin_cos();
    (w * c.abs() + h * s.abs(), w * s.abs() + h * c.abs())
}

/// The tape's outline as a closed path inside `tape_box`.
pub fn outline_path(w: f32, h: f32, angle_deg: f32) -> String {
    let rect = [(-w / 2.0, -h / 2.0), (w / 2.0, -h / 2.0), (w / 2.0, h / 2.0), (-w / 2.0, h / 2.0)];
    polygon_path(&rect, w, h, angle_deg)
}

/// The stripes of a tape: bands half a `pitch` wide, one every `pitch`, running at 45 degrees to the
/// tape and clipped to it. One closed sub-path per band that touches the tape.
pub fn stripe_path(w: f32, h: f32, angle_deg: f32, pitch: f32) -> String {
    let rect = vec![(-w / 2.0, -h / 2.0), (w / 2.0, -h / 2.0), (w / 2.0, h / 2.0), (-w / 2.0, h / 2.0)];
    let half = pitch / 2.0;
    let mut out = String::new();
    // A band is a <= x - y <= a + half.
    let mut a = -(w + h) / 2.0;
    while a < (w + h) / 2.0 {
        let clipped = clip(clip(rect.clone(), 1.0, -1.0, a), -1.0, 1.0, -(a + half));
        if clipped.len() >= 3 {
            out.push_str(&polygon_path(&clipped, w, h, angle_deg));
        }
        a += pitch;
    }
    out
}

/// Keep the part of `poly` where `nx * x + ny * y >= d` (Sutherland-Hodgman against one half-plane).
fn clip(poly: Vec<P>, nx: f32, ny: f32, d: f32) -> Vec<P> {
    let side = |p: P| nx * p.0 + ny * p.1 - d;
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let (sp, sq) = (side(p), side(q));
        if sp >= 0.0 {
            out.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            let t = sp / (sp - sq);
            out.push((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t));
        }
    }
    out
}

/// The Paper's tilt in the Settings page (`PaperTheme.paper-tilt`), degrees.
const PAPER_TILT: f32 = 1.5;
/// The mockup's tapes lean -35 degrees inside the Paper, so on screen they lean this much.
const TAPE_ANGLE: f32 = -35.0 + PAPER_TILT;
/// A CSS `repeating-linear-gradient(45deg, a 0 p, b p 2p)` band `p` wide, as `stripe_path`'s pitch:
/// the bands are measured along the gradient line, `stripe_path` along x - y (a factor of 2 root 2).
fn css_pitch(p: f32) -> f32 {
    p * 2.0 * std::f32::consts::SQRT_2
}

/// A dotted rule of `len` px along the Paper's tilt: the mockup's `2px dotted` (round dots 2px wide,
/// 2px apart), as circles in a box that has 2px of room on every side (the rule's top-left is at
/// (2, 2) in the box).
pub fn dotted_rule(len: f32, tilt_deg: f32) -> (String, f32, f32) {
    let (s, c) = tilt_deg.to_radians().sin_cos();
    let mut path = String::new();
    let n = (len / 4.0).floor() as i32;
    for k in 0..n {
        let (u, v) = (2.0 + 4.0 * k as f32, 1.0);
        let (x, y) = (2.0 + u * c - v * s, 2.0 + u * s + v * c);
        path.push_str(&format!("M {:.2} {:.2} a 1 1 0 1 0 2 0 a 1 1 0 1 0 -2 0 Z ", x - 1.0, y));
    }
    (path, len * c + 4.0, len * s + 6.0)
}

/// The Settings page's tapes: 0 the Paper's 113 by 33 strips, 1 the preview's 56 by 17 ones, 2 a
/// 26 by 11 chip of the Tape color picker, 3 the dotted row rule.
pub fn set_preview_tapes(ui: &crate::note_window::SettingsWindow) {
    use crate::note_window::TapeShape;
    let tape = |w: f32, h: f32, angle: f32, band: f32| {
        let (bw, bh) = tape_box(w, h, angle);
        TapeShape { outline: outline_path(w, h, angle).into(), stripes: stripe_path(w, h, angle, css_pitch(band)).into(), w: bw, h: bh }
    };
    let (dots, dw, dh) = dotted_rule(448.0, PAPER_TILT);
    let rule = TapeShape { outline: dots.into(), stripes: "".into(), w: dw, h: dh };
    let shapes = vec![tape(113.0, 33.0, TAPE_ANGLE, 8.0), tape(56.0, 17.0, TAPE_ANGLE, 5.0), tape(26.0, 11.0, 0.0, 4.0), rule];
    ui.set_preview_tapes(slint::ModelRc::new(slint::VecModel::from(shapes)));
}

/// A recolored tape of a note (the Tape color choice): the Paper's 113 by 33 strip as it lies on the
/// desk, with the same stripes the Settings page draws.
pub fn note_tape_shape() -> crate::note_window::TapeShape {
    let (w, h) = (113.0, 33.0);
    let (bw, bh) = tape_box(w, h, TAPE_ANGLE);
    crate::note_window::TapeShape {
        outline: outline_path(w, h, TAPE_ANGLE).into(),
        stripes: stripe_path(w, h, TAPE_ANGLE, css_pitch(8.0)).into(),
        w: bw,
        h: bh,
    }
}

/// One of the two upright 12 by 22 chips of a colorway in the right-click menu.
pub fn menu_chip_shape() -> crate::note_window::TapeShape {
    let (w, h) = (12.0, 22.0);
    crate::note_window::TapeShape {
        outline: outline_path(w, h, 0.0).into(),
        stripes: stripe_path(w, h, 0.0, css_pitch(4.0)).into(),
        w,
        h,
    }
}

/// Rotate `pts` and place them in the tape's box.
fn polygon_path(pts: &[P], w: f32, h: f32, angle_deg: f32) -> String {
    let (bw, bh) = tape_box(w, h, angle_deg);
    let (s, c) = angle_deg.to_radians().sin_cos();
    let mut out = String::new();
    for (i, (x, y)) in pts.iter().enumerate() {
        let (rx, ry) = (x * c - y * s + bw / 2.0, x * s + y * c + bh / 2.0);
        out.push_str(&format!("{} {:.2} {:.2} ", if i == 0 { "M" } else { "L" }, rx, ry));
    }
    out.push_str("Z ");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(path: &str) -> Vec<f32> {
        path.split_whitespace().filter_map(|t| t.parse().ok()).collect()
    }

    #[test]
    fn a_60_by_20_tape_with_pitch_10_has_eight_stripes() {
        // x - y spans -40..=40 across the tape, so bands start at -40, -30, ... 30.
        let p = stripe_path(60.0, 20.0, 0.0, 10.0);
        assert_eq!(p.matches('M').count(), 8);
    }

    #[test]
    fn every_stripe_point_stays_inside_the_box() {
        for angle in [-7.0, 0.0, 6.0, 30.0] {
            let (bw, bh) = tape_box(70.0, 22.0, angle);
            for path in [stripe_path(70.0, 22.0, angle, 11.0), outline_path(70.0, 22.0, angle)] {
                for (i, v) in numbers(&path).iter().enumerate() {
                    let limit = if i % 2 == 0 { bw } else { bh };
                    assert!((-0.01..=limit + 0.01).contains(v), "{v} outside 0..{limit} at {angle} degrees");
                }
            }
        }
    }

    #[test]
    fn unrotated_box_is_the_tape_itself() {
        assert_eq!(tape_box(60.0, 20.0, 0.0), (60.0, 20.0));
        assert_eq!(outline_path(60.0, 20.0, 0.0), "M 0.00 0.00 L 60.00 0.00 L 60.00 20.00 L 0.00 20.00 Z ");
    }
}
