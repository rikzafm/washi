//! Where a restored note may appear: a note saved on a monitor that is gone must come back
//! on the main screen (never lost off-screen).

use crate::platform::ScreenRect;

/// A saved window rectangle (physical px) made safe to show. `margin` is the window's transparent
/// margin around the paper. Returns `None` when the window can stay where it is: its top strip
/// (the part that moves the note) is on some work area. Otherwise the note is moved onto the
/// first work area (the primary one), shrunk if it is bigger than that.
pub fn recover(window: ScreenRect, margin: i32, work_areas: &[ScreenRect]) -> Option<ScreenRect> {
    let primary = work_areas.first()?;
    let paper = ScreenRect::new(window.x + margin, window.y + margin, window.w - 2 * margin, window.h - 2 * margin);
    // The user must be able to grab the note: its whole top strip, at least two margins wide.
    let strip = ScreenRect::new(paper.x, paper.y, paper.w, margin / 2);
    let wanted_w = paper.w.min(2 * margin);
    let reachable = work_areas
        .iter()
        .filter_map(|a| a.intersect(&strip))
        .any(|i| i.w >= wanted_w && i.h >= strip.h);
    if reachable {
        return None;
    }
    // Paper (not window) goes inside the work area: the margin is transparent.
    let pw = paper.w.min(primary.w);
    let ph = paper.h.min(primary.h);
    let px = paper.x.clamp(primary.x, primary.right() - pw);
    let py = paper.y.clamp(primary.y, primary.bottom() - ph);
    Some(ScreenRect::new(px - margin, py - margin, pw + 2 * margin, ph + 2 * margin))
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: i32 = 63; // 42 logical at 150%
    const PRIMARY: ScreenRect = ScreenRect { x: 0, y: 0, w: 1920, h: 1040 };
    const SECOND: ScreenRect = ScreenRect { x: 1920, y: 0, w: 1920, h: 1040 };

    fn win(x: i32, y: i32) -> ScreenRect {
        ScreenRect::new(x, y, 700, 700)
    }

    #[test]
    fn a_note_on_a_screen_stays_put() {
        assert_eq!(recover(win(100, 100), M, &[PRIMARY]), None);
        assert_eq!(recover(win(2500, 100), M, &[PRIMARY, SECOND]), None);
    }

    #[test]
    fn a_note_partly_off_screen_but_grabbable_stays_put() {
        // half the paper is past the right edge; the margin and the left of the paper are not.
        assert_eq!(recover(win(1700, 500), M, &[PRIMARY]), None);
        // the bottom is cut off: the top strip is what matters
        assert_eq!(recover(win(100, 900), M, &[PRIMARY]), None);
    }

    #[test]
    fn a_note_on_a_missing_monitor_moves_to_the_primary_work_area() {
        let r = recover(win(2500, 100), M, &[PRIMARY]).unwrap();
        let paper = ScreenRect::new(r.x + M, r.y + M, r.w - 2 * M, r.h - 2 * M);
        assert!(PRIMARY.intersect(&paper) == Some(paper), "paper is inside the work area: {r:?}");
        assert_eq!((r.w, r.h), (700, 700), "size is kept");
    }

    #[test]
    fn a_note_above_or_left_of_every_screen_comes_back() {
        assert!(recover(win(-5000, -5000), M, &[PRIMARY]).is_some());
        // top strip above the work area
        assert!(recover(win(100, -400), M, &[PRIMARY]).is_some());
    }

    #[test]
    fn minimized_windows_position_is_recovered() {
        assert!(recover(win(-32000, -32000), M, &[PRIMARY]).is_some());
    }

    #[test]
    fn a_note_bigger_than_the_screen_is_shrunk_to_fit() {
        let r = recover(ScreenRect::new(9000, 9000, 4000, 3000), M, &[PRIMARY]).unwrap();
        assert_eq!((r.w - 2 * M, r.h - 2 * M), (PRIMARY.w, PRIMARY.h));
        assert_eq!((r.x + M, r.y + M), (PRIMARY.x, PRIMARY.y));
    }

    #[test]
    fn no_work_areas_means_nothing_to_do() {
        assert_eq!(recover(win(9000, 9000), M, &[]), None);
    }
}
