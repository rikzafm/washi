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

const DEFAULT_CARD_WIDTH: f32 = 268.0;
const TRIGGER_OFFSET_X: f32 = 14.0;

fn rect_center(r: &ScreenRect) -> (i32, i32) {
    (r.x + r.w / 2, r.y + r.h / 2)
}

fn rect_contains_point(r: &ScreenRect, p: (i32, i32)) -> bool {
    p.0 >= r.x && p.0 < r.right() && p.1 >= r.y && p.1 < r.bottom()
}

/// Find the work area containing `point` (half-open: `[x, right)` and `[y, bottom)`),
/// falling back to the first work area if no work area contains it.
pub fn find_work_area<'a>(point: (i32, i32), work_areas: &'a [ScreenRect]) -> Option<&'a ScreenRect> {
    work_areas
        .iter()
        .find(|wa| rect_contains_point(wa, point))
        .or_else(|| work_areas.first())
}

/// Calculate the screen placement for an auxiliary popup anchored below a trigger button.
///
/// * `trigger`: Physical screen rectangle of the trigger button.
/// * `size`: `(width, height)` in physical pixels of the popup window (including shadow margins).
/// * `gap`: Spacing in physical pixels between the button and the inner card.
/// * `margin`: Transparent shadow margin on each side of the popup card.
/// * `work_areas`: Available desktop work areas in physical pixels.
///
/// The card is initially positioned below the button (`trigger.bottom() + gap`), offset
/// horizontally by `14 * (card_w / 268)` so the button aligns nicely with the card header.
/// If placing below would overflow the bottom of the work area, the card flips above the button
/// (`trigger.y - card_h - gap`). If it cannot fit above either, it is clamped to the bottom edge.
/// Horizontal position is clamped within the work area.
/// Returns the full window rectangle (card adjusted by `margin`).
pub fn anchor_below(
    trigger: ScreenRect,
    size: (i32, i32),
    gap: i32,
    margin: i32,
    work_areas: &[ScreenRect],
) -> ScreenRect {
    let card_w = size.0 - 2 * margin;
    let card_h = size.1 - 2 * margin;

    let offset_x = (TRIGGER_OFFSET_X * (card_w as f32 / DEFAULT_CARD_WIDTH)).round() as i32;
    let mut card_x = trigger.x - offset_x;
    let mut card_y = trigger.bottom() + gap;

    let center = rect_center(&trigger);
    if let Some(wa) = find_work_area(center, work_areas) {
        // If placing below overflows the bottom of work area, flip above:
        if card_y + card_h > wa.bottom() {
            let candidate_y = trigger.y - card_h - gap;
            if candidate_y >= wa.y {
                card_y = candidate_y;
            } else {
                card_y = (wa.bottom() - card_h).max(wa.y);
            }
        }

        // Clamp horizontally within work area:
        if card_x + card_w > wa.right() {
            card_x = wa.right() - card_w;
        }
        if card_x < wa.x {
            card_x = wa.x;
        }
    }

    ScreenRect::new(card_x - margin, card_y - margin, size.0, size.1)
}

/// Calculate the screen placement to center a dialog over a target note.
///
/// * `target`: Target note's physical ScreenRect.
/// * `size`: `(width, height)` in physical pixels of the dialog window.
/// * `work_areas`: Available desktop work areas in physical pixels.
///
/// Centers the dialog over `target` and clamps within the matching work area.
pub fn center_over(
    target: ScreenRect,
    size: (i32, i32),
    work_areas: &[ScreenRect],
) -> ScreenRect {
    let mut x = target.x + (target.w - size.0) / 2;
    let mut y = target.y + (target.h - size.1) / 2;

    let center = rect_center(&target);
    if let Some(wa) = find_work_area(center, work_areas) {
        if x + size.0 > wa.right() {
            x = wa.right() - size.0;
        }
        if x < wa.x {
            x = wa.x;
        }
        if y + size.1 > wa.bottom() {
            y = wa.bottom() - size.1;
        }
        if y < wa.y {
            y = wa.y;
        }
    }

    ScreenRect::new(x, y, size.0, size.1)
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

    // --- anchor_below tests ---

    #[test]
    fn anchor_below_normal_placement_below_button() {
        // Trigger at (100, 100), size 24x24
        let trigger = ScreenRect::new(100, 100, 24, 24);
        // Popup window size 300x352, margin 16 -> card is 268x320
        let size = (300, 352);
        let margin = 16;
        let gap = 4;
        // offset_x = round(14.0 * 268 / 268) = 14
        // card_x = 100 - 14 = 86
        // card_y = 100 + 24 + 4 = 128
        // card fits within PRIMARY (128 + 320 = 448 <= 1040)
        // window_x = 86 - 16 = 70
        // window_y = 128 - 16 = 112
        let pos = anchor_below(trigger, size, gap, margin, &[PRIMARY]);
        assert_eq!(pos, ScreenRect::new(70, 112, 300, 352));
    }

    #[test]
    fn anchor_below_flips_above_when_overflowing_bottom() {
        // Trigger at (100, 800), size 24x24
        let trigger = ScreenRect::new(100, 800, 24, 24);
        let size = (300, 352); // card: 268x320
        let margin = 16;
        let gap = 4;
        // card_y below = 800 + 24 + 4 = 828.
        // 828 + 320 = 1148 > 1040 (overflows PRIMARY bottom!)
        // candidate_y above = 800 - 320 - 4 = 476 >= 0 (PRIMARY.y)
        // card_y = 476
        // card_x = 100 - 14 = 86
        // window_x = 86 - 16 = 70
        // window_y = 476 - 16 = 460
        let pos = anchor_below(trigger, size, gap, margin, &[PRIMARY]);
        assert_eq!(pos, ScreenRect::new(70, 460, 300, 352));
    }

    #[test]
    fn anchor_below_clamps_to_bottom_when_it_cannot_fit_above_either() {
        // Short work area: height 400
        let short_wa = ScreenRect::new(0, 0, 1920, 400);
        // Trigger near middle: (100, 150), size 24x24
        let trigger = ScreenRect::new(100, 150, 24, 24);
        let size = (300, 352); // card: 268x320
        let margin = 16;
        let gap = 4;
        // Below: 150 + 24 + 4 = 178 + 320 = 498 > 400 (overflows)
        // Above: 150 - 320 - 4 = -174 < 0 (does not fit above)
        // Clamped: (wa.bottom() - card_h).max(wa.y) = (400 - 320).max(0) = 80
        // card_y = 80 -> window_y = 80 - 16 = 64
        let pos = anchor_below(trigger, size, gap, margin, &[short_wa]);
        assert_eq!(pos.y, 64);
    }

    #[test]
    fn anchor_below_clamps_horizontally() {
        let size = (300, 352); // card: 268x320
        let margin = 16;
        let gap = 4;

        // Left edge overflow: trigger near x = 5
        let trigger_left = ScreenRect::new(5, 100, 24, 24);
        // card_x = 5 - 14 = -9 < 0 -> clamped to 0
        // window_x = 0 - 16 = -16
        let pos_left = anchor_below(trigger_left, size, gap, margin, &[PRIMARY]);
        assert_eq!(pos_left.x, -16);

        // Right edge overflow: trigger near x = 1900
        let trigger_right = ScreenRect::new(1900, 100, 24, 24);
        // card_x = 1900 - 14 = 1886. 1886 + 268 = 2154 > 1920 -> clamped to 1920 - 268 = 1652
        // window_x = 1652 - 16 = 1636
        let pos_right = anchor_below(trigger_right, size, gap, margin, &[PRIMARY]);
        assert_eq!(pos_right.x, 1636);
    }

    #[test]
    fn anchor_below_resolves_multi_monitor() {
        // SECOND is at x=1920..3840, y=0..1040
        let trigger = ScreenRect::new(2100, 100, 24, 24);
        let size = (300, 352);
        let margin = 16;
        let gap = 4;
        let pos = anchor_below(trigger, size, gap, margin, &[PRIMARY, SECOND]);
        // card_x = 2100 - 14 = 2086 (inside SECOND: 1920..3840)
        // window_x = 2086 - 16 = 2070
        // card_y = 100 + 24 + 4 = 128
        // window_y = 128 - 16 = 112
        assert_eq!(pos, ScreenRect::new(2070, 112, 300, 352));
    }

    #[test]
    fn anchor_below_empty_work_areas_fallback() {
        let trigger = ScreenRect::new(100, 100, 24, 24);
        let size = (300, 352);
        let margin = 16;
        let gap = 4;
        let pos = anchor_below(trigger, size, gap, margin, &[]);
        assert_eq!(pos, ScreenRect::new(70, 112, 300, 352));
    }

    // --- center_over tests ---

    #[test]
    fn center_over_normal_centering() {
        // Target note at (500, 400), size 400x300
        let target = ScreenRect::new(500, 400, 400, 300);
        let dialog_size = (250, 150);
        // x = 500 + (400 - 250) / 2 = 575
        // y = 400 + (300 - 150) / 2 = 475
        let pos = center_over(target, dialog_size, &[PRIMARY]);
        assert_eq!(pos, ScreenRect::new(575, 475, 250, 150));
    }

    #[test]
    fn center_over_clamps_within_work_area() {
        let dialog_size = (300, 200);

        // Target near right and bottom edge: (1800, 950, 400, 300)
        let target_rb = ScreenRect::new(1800, 950, 400, 300);
        // x = 1800 + (400 - 300) / 2 = 1850 -> 1850 + 300 = 2150 > 1920 -> clamped to 1920 - 300 = 1620
        // y = 950 + (300 - 200) / 2 = 1000 -> 1000 + 200 = 1200 > 1040 -> clamped to 1040 - 200 = 840
        let pos_rb = center_over(target_rb, dialog_size, &[PRIMARY]);
        assert_eq!(pos_rb, ScreenRect::new(1620, 840, 300, 200));

        // Target near top-left (partly offscreen): (-100, -50, 200, 200)
        let target_lt = ScreenRect::new(-100, -50, 200, 200);
        // x = -100 + (200 - 300) / 2 = -150 -> clamped to 0
        // y = -50 + (200 - 200) / 2 = -50 -> clamped to 0
        let pos_lt = center_over(target_lt, dialog_size, &[PRIMARY]);
        assert_eq!(pos_lt, ScreenRect::new(0, 0, 300, 200));
    }

    #[test]
    fn center_over_resolves_multi_monitor() {
        // Target on SECOND monitor: x = 2200, y = 300
        let target = ScreenRect::new(2200, 300, 400, 300);
        let dialog_size = (300, 200);
        // x = 2200 + (400 - 300) / 2 = 2250 (inside SECOND: 1920..3840)
        // y = 300 + (300 - 200) / 2 = 350 (inside SECOND: 0..1040)
        let pos = center_over(target, dialog_size, &[PRIMARY, SECOND]);
        assert_eq!(pos, ScreenRect::new(2250, 350, 300, 200));
    }

    #[test]
    fn center_over_empty_work_areas_fallback() {
        let target = ScreenRect::new(500, 400, 400, 300);
        let dialog_size = (250, 150);
        let pos = center_over(target, dialog_size, &[]);
        assert_eq!(pos, ScreenRect::new(575, 475, 250, 150));
    }

    // --- find_work_area tests ---

    #[test]
    fn find_work_area_locates_correct_monitor() {
        assert_eq!(find_work_area((500, 500), &[PRIMARY, SECOND]), Some(&PRIMARY));
        assert_eq!(find_work_area((2500, 500), &[PRIMARY, SECOND]), Some(&SECOND));
    }

    #[test]
    fn find_work_area_half_open_boundaries() {
        // [0, 0] is top-left of PRIMARY: inside PRIMARY
        assert_eq!(find_work_area((0, 0), &[PRIMARY, SECOND]), Some(&PRIMARY));
        // [1919, 1039] is inside PRIMARY
        assert_eq!(find_work_area((1919, 1039), &[PRIMARY, SECOND]), Some(&PRIMARY));
        // [1920, 0] is right boundary of PRIMARY, which is left boundary of SECOND: inside SECOND
        assert_eq!(find_work_area((1920, 0), &[PRIMARY, SECOND]), Some(&SECOND));
        // [3840, 0] is right boundary of SECOND: outside all work areas -> falls back to first (PRIMARY)
        assert_eq!(find_work_area((3840, 0), &[PRIMARY, SECOND]), Some(&PRIMARY));
    }

    #[test]
    fn find_work_area_falls_back_to_first_when_offscreen() {
        assert_eq!(find_work_area((-100, -100), &[PRIMARY, SECOND]), Some(&PRIMARY));
    }

    #[test]
    fn find_work_area_empty_returns_none() {
        assert_eq!(find_work_area((100, 100), &[]), None);
    }
}
