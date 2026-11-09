//! Geometry of the Settings page (pure: no Slint, no platform). Logical px throughout.

use crate::platform::ScreenRect;
use crate::storage::Geometry;
use std::time::Duration;

/// A rectangle in logical px.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl RectF {
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// The strip the Sidebar adds left of the Paper. Same as `AllNotesMetrics.sidebar-w`.
pub const SIDEBAR_STRIP: f32 = 282.0;
/// `PaperTheme.margin`: the transparent margin around the Paper in every window.
pub const MARGIN: f32 = 42.0;
pub const PAPER_W: f32 = 520.0;
pub const PAPER_MIN_H: f32 = 600.0;

/// The Paper's box inside a note window (the window is the Paper plus the margin).
pub fn paper_of_note(g: Geometry) -> RectF {
    RectF { x: g.x as f32 + MARGIN, y: g.y as f32 + MARGIN, w: g.w as f32 - 2.0 * MARGIN, h: g.h as f32 - 2.0 * MARGIN }
}

/// The window around `paper` with `strip` px of Sidebar out on the left. The edges are rounded
/// separately so the Paper never wobbles by a pixel while the strip changes.
pub fn window_bounds(paper: RectF, strip: f32) -> Geometry {
    let left = (paper.x - MARGIN - strip).round() as i32;
    let right = (paper.right() + MARGIN).round() as i32;
    let top = (paper.y - MARGIN).round() as i32;
    let bottom = (paper.bottom() + MARGIN).round() as i32;
    Geometry { x: left, y: top, w: (right - left).max(1) as u32, h: (bottom - top).max(1) as u32 }
}

/// Work areas arrive in physical px; the Settings math is logical.
pub fn logical_areas(areas: &[ScreenRect], scale: f32) -> Vec<RectF> {
    areas
        .iter()
        .map(|a| RectF { x: a.x as f32 / scale, y: a.y as f32 / scale, w: a.w as f32 / scale, h: a.h as f32 / scale })
        .collect()
}

/// Shift `paper` (never resize it) so the window around it, with `strip` out, fits the work area
/// that holds the Paper's center. Left and top win when it cannot fit at all.
pub fn fit_on_screen(paper: RectF, strip: f32, areas: &[RectF]) -> RectF {
    let (cx, cy) = (paper.x + paper.w / 2.0, paper.y + paper.h / 2.0);
    let Some(a) = areas
        .iter()
        .find(|a| cx >= a.x && cx < a.right() && cy >= a.y && cy < a.bottom())
        .or(areas.first())
    else {
        return paper;
    };
    let min_x = a.x + MARGIN + strip;
    let max_x = a.right() - MARGIN - paper.w;
    let min_y = a.y + MARGIN;
    let max_y = a.bottom() - MARGIN - paper.h;
    RectF { x: paper.x.min(max_x).max(min_x), y: paper.y.min(max_y).max(min_y), ..paper }
}

/// Fade of the host note's text out (and back in), ms.
pub const NOTE_FADE: Duration = Duration::from_millis(90);
/// The Paper's growth to the Settings size (and back), ms.
pub const GROW: Duration = Duration::from_millis(240);
/// One frame of the transition.
pub const TICK: Duration = Duration::from_millis(16);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ease {
    Linear,
    OutCubic,
}

impl Ease {
    fn at(self, t: f32) -> f32 {
        match self {
            Ease::Linear => t,
            Ease::OutCubic => 1.0 - (1.0 - t).powi(3),
        }
    }
}

/// A value moving from `from` to `to` over `dur`, started at `start` on the monotonic clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    from: f32,
    to: f32,
    start: Duration,
    dur: Duration,
    ease: Ease,
}

impl Tween {
    pub fn new(from: f32, to: f32, start: Duration, dur: Duration, ease: Ease) -> Self {
        Tween { from, to, start, dur, ease }
    }

    /// The value at `now`, and whether the tween has finished.
    pub fn sample(&self, now: Duration) -> (f32, bool) {
        let t = if self.dur.is_zero() { 1.0 } else { (now.saturating_sub(self.start).as_secs_f32() / self.dur.as_secs_f32()).min(1.0) };
        (self.from + (self.to - self.from) * self.ease.at(t), t >= 1.0)
    }

    /// The same tween with its clock restarted at `now`.
    pub fn restarted(&self, now: Duration) -> Self {
        Tween { start: now, ..*self }
    }
}

/// How far along the page is: the Paper's box and how much of the Sidebar's strip is out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub paper: RectF,
    pub strip: f32,
}

fn lerp(a: f32, b: f32, e: f32) -> f32 {
    a + (b - a) * e
}

/// The Paper at `e` (0 is the note, 1 is the Settings page); the Paper's top-left moves only as far
/// as keeping it on screen needs, the Sidebar's strip grows with `e`.
pub fn grow_pose(from: RectF, to: RectF, e: f32) -> Pose {
    Pose {
        paper: RectF { x: lerp(from.x, to.x, e), y: lerp(from.y, to.y, e), w: lerp(from.w, to.w, e), h: lerp(from.h, to.h, e) },
        strip: SIDEBAR_STRIP * e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn tween_eases_out_and_finishes() {
        let t = Tween::new(0.0, 1.0, ms(100), ms(200), Ease::OutCubic);
        assert_eq!(t.sample(ms(50)), (0.0, false));
        let (mid, done) = t.sample(ms(200));
        assert!(!done && mid > 0.5 && mid < 1.0, "ease-out is past halfway at half time: {mid}");
        assert_eq!(t.sample(ms(300)), (1.0, true));
        assert_eq!(t.sample(ms(900)), (1.0, true));
    }

    #[test]
    fn tween_runs_backwards() {
        let t = Tween::new(0.6, 0.0, ms(0), ms(240), Ease::OutCubic);
        assert_eq!(t.sample(ms(0)).0, 0.6);
        assert_eq!(t.sample(ms(240)), (0.0, true));
    }

    #[test]
    fn grow_pose_ends_are_the_note_and_the_page() {
        let note = RectF { x: 342.0, y: 142.0, w: 363.0, h: 384.0 };
        let page = RectF { x: 400.0, y: 100.0, w: PAPER_W, h: PAPER_MIN_H };
        assert_eq!(grow_pose(note, page, 0.0), Pose { paper: note, strip: 0.0 });
        assert_eq!(grow_pose(note, page, 1.0), Pose { paper: page, strip: SIDEBAR_STRIP });
    }

    #[test]
    fn note_paper_and_window_round_trip() {
        let g = Geometry { x: 300, y: 120, w: 447, h: 468 };
        assert_eq!(window_bounds(paper_of_note(g), 0.0), g);
    }

    #[test]
    fn strip_extends_the_window_to_the_left_only() {
        let g = Geometry { x: 300, y: 120, w: 447, h: 468 };
        let w = window_bounds(paper_of_note(g), SIDEBAR_STRIP);
        assert_eq!((w.x, w.y, w.h), (300 - 282, 120, 468));
        assert_eq!(w.x + w.w as i32, g.x + g.w as i32);
    }

    #[test]
    fn fit_keeps_the_strip_on_screen() {
        let area = [RectF { x: 0.0, y: 0.0, w: 1920.0, h: 1040.0 }];
        let p = fit_on_screen(RectF { x: 50.0, y: 900.0, w: PAPER_W, h: PAPER_MIN_H }, SIDEBAR_STRIP, &area);
        assert_eq!(p.x, MARGIN + SIDEBAR_STRIP);
        assert_eq!(p.bottom() + MARGIN, 1040.0);
    }

    #[test]
    fn fit_leaves_a_paper_that_already_fits() {
        let area = [RectF { x: 0.0, y: 0.0, w: 1920.0, h: 1040.0 }];
        let p = RectF { x: 800.0, y: 100.0, w: PAPER_W, h: PAPER_MIN_H };
        assert_eq!(fit_on_screen(p, SIDEBAR_STRIP, &area), p);
    }
}
