//! Geometry of the Settings page (pure: no Slint, no platform). Logical px throughout.

use crate::platform::ScreenRect;
use crate::storage::Geometry;

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

#[cfg(test)]
mod tests {
    use super::*;

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
