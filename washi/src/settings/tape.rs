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

/// Give the Font & Style preview its two tapes (size and tilt are the preview's, fixed).
pub fn set_preview_tapes(ui: &crate::note_window::SettingsWindow) {
    use crate::note_window::TapeShape;
    let shape = |angle: f32| {
        let (w, h) = (74.0, 24.0);
        let (bw, bh) = tape_box(w, h, angle);
        TapeShape { outline: outline_path(w, h, angle).into(), stripes: stripe_path(w, h, angle, 12.0).into(), w: bw, h: bh }
    };
    ui.set_preview_tapes(slint::ModelRc::new(slint::VecModel::from(vec![shape(-8.0), shape(6.0)])));
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
