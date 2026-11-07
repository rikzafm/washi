//! The scoring seam: ink-mask IoU after the best alignment of a candidate render onto a reference.

use uidiff::fontmatch::{align_iou, align_iou_ignoring, Bitmap};

fn ring(w: usize, h: usize, cx: i32, cy: i32, r_out: i32, r_in: i32) -> Bitmap {
    Bitmap::from_fn(w, h, |x, y| {
        let d2 = (x as i32 - cx).pow(2) + (y as i32 - cy).pow(2);
        d2 <= r_out * r_out && d2 >= r_in * r_in
    })
}

fn bar(w: usize, h: usize, x0: usize, y0: usize, bw: usize, bh: usize) -> Bitmap {
    Bitmap::from_fn(w, h, |x, y| x >= x0 && x < x0 + bw && y >= y0 && y < y0 + bh)
}

#[test]
fn ignored_rows_count_for_neither_mask() {
    // reference: a ring plus a 2 px underline the app draws itself; candidate: the bare ring
    let bare = ring(80, 70, 30, 30, 15, 9);
    let reference = Bitmap::from_fn(80, 70, |x, y| bare.get(x as i64, y as i64) || (y >= 50 && y < 52 && x >= 10 && x < 55));
    let mut ignore = vec![false; 70];
    ignore[50] = true;
    ignore[51] = true;
    let (iou, _) = align_iou_ignoring(&reference, &bare, 2, &ignore);
    assert!((iou - 1.0).abs() < 1e-9, "iou {iou}");
    let (without, _) = align_iou(&reference, &bare, 2);
    assert!(without < 0.95, "the underline should hurt when not ignored: {without}");
}

#[test]
fn identical_shape_at_another_position_scores_one() {
    let a = ring(80, 60, 30, 30, 15, 9);
    let b = ring(80, 60, 41, 27, 15, 9);
    let (iou, _) = align_iou(&a, &b, 6);
    assert!((iou - 1.0).abs() < 1e-9, "iou {iou}");
}

#[test]
fn a_different_shape_scores_clearly_lower() {
    let a = ring(80, 60, 30, 30, 15, 9);
    let b = bar(80, 60, 20, 22, 22, 16);
    let (iou, _) = align_iou(&a, &b, 6);
    assert!(iou < 0.6, "iou {iou}");
}

#[test]
fn one_pixel_thinner_strokes_cost_a_known_amount() {
    // ring of width 6 vs width 4 (same outer radius): intersection = area(width 4),
    // union = area(width 6). Areas by pixel count, computed independently of align_iou.
    let a = ring(80, 60, 30, 30, 15, 9);
    let b = ring(80, 60, 30, 30, 15, 11);
    let (na, nb) = (a.count(), b.count());
    let (iou, _) = align_iou(&a, &b, 3);
    assert!((iou - nb as f64 / na as f64).abs() < 1e-9, "iou {iou}, expected {}", nb as f64 / na as f64);
}
