//! Empirical Challenger Stress Tests for Milestone 1:
//! Placement, Centering, and Work Area Boundary Logic.

use std::cell::RefCell;
use std::rc::Rc;
use washi::image_viewer::{window_size, ImageViewer};
use washi::placement::{center_over, find_work_area};
use washi::platform::ScreenRect;
use snap::fakes::{FakeClipboard, FakeWindowControl};

const PRIMARY: ScreenRect = ScreenRect { x: 0, y: 0, w: 1920, h: 1040 };
const SECONDARY_RIGHT: ScreenRect = ScreenRect { x: 1920, y: 0, w: 1920, h: 1040 };
const TERTIARY_RIGHT: ScreenRect = ScreenRect { x: 3840, y: 0, w: 2560, h: 1440 };
const SECONDARY_LEFT: ScreenRect = ScreenRect { x: -1920, y: 0, w: 1920, h: 1040 };
const SECONDARY_ABOVE: ScreenRect = ScreenRect { x: 0, y: -1080, w: 1920, h: 1080 };

// =========================================================================
// 1. Stress Tests: placement::find_work_area
// =========================================================================

#[test]
fn stress_find_work_area_monitor_boundaries_half_open() {
    let monitors = [PRIMARY, SECONDARY_RIGHT];

    // Left edge of PRIMARY is inclusive:
    assert_eq!(find_work_area((0, 500), &monitors), Some(&PRIMARY));
    // Right edge of PRIMARY (1920) is exclusive for PRIMARY and inclusive for SECONDARY:
    assert_eq!(find_work_area((1919, 500), &monitors), Some(&PRIMARY));
    assert_eq!(find_work_area((1920, 500), &monitors), Some(&SECONDARY_RIGHT));
    // Right edge of SECONDARY (3840) is outside:
    assert_eq!(find_work_area((3839, 500), &monitors), Some(&SECONDARY_RIGHT));
    assert_eq!(find_work_area((3840, 500), &monitors), Some(&PRIMARY), "Offscreen point falls back to primary");

    // Top edge of PRIMARY is inclusive:
    assert_eq!(find_work_area((500, 0), &monitors), Some(&PRIMARY));
    // Bottom edge of PRIMARY (1040) is exclusive:
    assert_eq!(find_work_area((500, 1039), &monitors), Some(&PRIMARY));
    assert_eq!(find_work_area((500, 1040), &monitors), Some(&PRIMARY), "Past bottom falls back to primary");
}

#[test]
fn stress_find_work_area_vertical_monitor_boundaries() {
    let monitors = [SECONDARY_ABOVE, PRIMARY];

    // Inside secondary above:
    assert_eq!(find_work_area((500, -1080), &monitors), Some(&SECONDARY_ABOVE));
    assert_eq!(find_work_area((500, -1), &monitors), Some(&SECONDARY_ABOVE));
    // Boundary at y = 0:
    assert_eq!(find_work_area((500, 0), &monitors), Some(&PRIMARY));
    assert_eq!(find_work_area((500, 1039), &monitors), Some(&PRIMARY));
}

#[test]
fn stress_find_work_area_triple_monitors_including_negative() {
    let monitors = [SECONDARY_LEFT, PRIMARY, TERTIARY_RIGHT];

    // Left monitor (negative coordinates):
    assert_eq!(find_work_area((-1920, 500), &monitors), Some(&SECONDARY_LEFT));
    assert_eq!(find_work_area((-1, 500), &monitors), Some(&SECONDARY_LEFT));
    // Primary monitor:
    assert_eq!(find_work_area((0, 500), &monitors), Some(&PRIMARY));
    assert_eq!(find_work_area((1919, 500), &monitors), Some(&PRIMARY));
    // Right tertiary monitor (large display):
    assert_eq!(find_work_area((3840, 500), &monitors), Some(&TERTIARY_RIGHT));
    assert_eq!(find_work_area((5000, 1200), &monitors), Some(&TERTIARY_RIGHT));
}

#[test]
fn stress_find_work_area_empty_and_offscreen() {
    // Empty returns None:
    assert_eq!(find_work_area((100, 100), &[]), None);
    assert_eq!(find_work_area((0, 0), &[]), None);
    assert_eq!(find_work_area((-1000, -1000), &[]), None);

    // Single monitor: offscreen points always fall back to that monitor
    let monitors = [PRIMARY];
    assert_eq!(find_work_area((-999999, -999999), &monitors), Some(&PRIMARY));
    assert_eq!(find_work_area((999999, 999999), &monitors), Some(&PRIMARY));

    // Gap between disjoint monitors:
    let mon_a = ScreenRect::new(0, 0, 1000, 800);
    let mon_b = ScreenRect::new(2000, 0, 1000, 800); // 1000px gap
    let disjoint = [mon_a, mon_b];
    assert_eq!(find_work_area((1500, 400), &disjoint), Some(&mon_a), "Point in monitor gap falls back to primary");
}

// =========================================================================
// 2. Stress Tests: placement::center_over
// =========================================================================

#[test]
fn stress_center_over_note_straddling_monitor_boundary() {
    let monitors = [PRIMARY, SECONDARY_RIGHT];
    let dialog_size = (300, 200);

    // Case A: Note centered exactly on the boundary at x = 1920.
    // Target note at (1720, 400), width 400 -> center is 1720 + 200 = 1920.
    // Center point 1920 is contained in SECONDARY_RIGHT!
    let target_straddle_right = ScreenRect::new(1720, 400, 400, 300);
    let pos_a = center_over(target_straddle_right, dialog_size, &monitors);
    // Unclamped x = 1720 + (400 - 300) / 2 = 1770.
    // Clamped against SECONDARY_RIGHT (x in [1920, 3840 - 300 = 3540]):
    // Since 1770 < 1920, x clamps to 1920!
    assert_eq!(pos_a.x, 1920, "Dialog must clamp to left edge of secondary monitor");
    assert_eq!(pos_a.y, 450);
    assert!(pos_a.x >= 1920 && pos_a.right() <= 3840, "Dialog is entirely inside secondary monitor");

    // Case B: Note centered 1 px to the left of the boundary at x = 1919.
    // Target note at (1719, 400), width 400 -> center is 1719 + 200 = 1919.
    // Center point 1919 is contained in PRIMARY!
    let target_straddle_left = ScreenRect::new(1719, 400, 400, 300);
    let pos_b = center_over(target_straddle_left, dialog_size, &monitors);
    // Unclamped x = 1719 + (400 - 300) / 2 = 1769.
    // Clamped against PRIMARY (x in [0, 1920 - 300 = 1620]):
    // Since 1769 + 300 = 2069 > 1920, x clamps to 1920 - 300 = 1620!
    assert_eq!(pos_b.x, 1620, "Dialog must clamp to right edge of primary monitor");
    assert_eq!(pos_b.y, 450);
    assert!(pos_b.x >= 0 && pos_b.right() <= 1920, "Dialog is entirely inside primary monitor");
}

#[test]
fn stress_center_over_negative_coordinate_monitors() {
    let monitors = [SECONDARY_LEFT, PRIMARY];
    let dialog_size = (300, 200);

    // Target on left monitor (x in [-1920, 0]):
    let target_left = ScreenRect::new(-1000, 400, 400, 300);
    let pos = center_over(target_left, dialog_size, &monitors);
    assert_eq!(pos, ScreenRect::new(-950, 450, 300, 200));

    // Target near extreme left edge of left monitor:
    let target_far_left = ScreenRect::new(-2000, 400, 400, 300); // extends left of -1920
    let pos_far_left = center_over(target_far_left, dialog_size, &monitors);
    // Unclamped x = -2000 + 50 = -1950 < -1920 -> clamped to -1920:
    assert_eq!(pos_far_left.x, -1920);

    // Target on monitor above:
    let monitors_vertical = [SECONDARY_ABOVE, PRIMARY];
    let target_above = ScreenRect::new(500, -800, 400, 300);
    let pos_above = center_over(target_above, dialog_size, &monitors_vertical);
    assert_eq!(pos_above, ScreenRect::new(550, -750, 300, 200));
}

#[test]
fn stress_center_over_extremely_large_dialog_exceeding_work_area() {
    // Dialog is 2500 x 1500, larger than PRIMARY (1920 x 1040)
    let dialog_size = (2500, 1500);
    let target = ScreenRect::new(500, 300, 400, 300);
    let pos = center_over(target, dialog_size, &[PRIMARY]);

    // Unclamped x = 500 + (400 - 2500) / 2 = 500 - 1050 = -550.
    // Clamp checks:
    // 1. x + 2500 = 1950 > 1920 -> x = 1920 - 2500 = -580.
    // 2. x < 0 -> -580 < 0 -> x = 0!
    // Same for y: clamped to 0!
    assert_eq!(pos.x, 0, "Top-left x must clamp to monitor origin so titlebar is visible");
    assert_eq!(pos.y, 0, "Top-left y must clamp to monitor origin so titlebar is visible");
    assert_eq!((pos.w, pos.h), (2500, 1500));
}

#[test]
fn stress_center_over_extremely_small_dialog_and_target() {
    // 0x0 dialog on 0x0 target:
    let target_zero = ScreenRect::new(500, 400, 0, 0);
    let pos_zero = center_over(target_zero, (0, 0), &[PRIMARY]);
    assert_eq!(pos_zero, ScreenRect::new(500, 400, 0, 0));

    // 1x1 dialog on 1x1 target:
    let target_one = ScreenRect::new(500, 400, 1, 1);
    let pos_one = center_over(target_one, (1, 1), &[PRIMARY]);
    assert_eq!(pos_one, ScreenRect::new(500, 400, 1, 1));
}

#[test]
fn stress_center_over_target_completely_offscreen() {
    let dialog_size = (300, 200);

    // Target far negative (-50000, -50000):
    let target_neg = ScreenRect::new(-50000, -50000, 400, 300);
    let pos_neg = center_over(target_neg, dialog_size, &[PRIMARY]);
    // Falls back to PRIMARY, clamps to top-left (0, 0):
    assert_eq!(pos_neg, ScreenRect::new(0, 0, 300, 200));

    // Target far positive (50000, 50000):
    let target_pos = ScreenRect::new(50000, 50000, 400, 300);
    let pos_pos = center_over(target_pos, dialog_size, &[PRIMARY]);
    // Falls back to PRIMARY, clamps to bottom-right:
    assert_eq!(pos_pos, ScreenRect::new(1920 - 300, 1040 - 200, 300, 200));
}

#[test]
fn stress_center_over_empty_work_areas_fallback() {
    let target = ScreenRect::new(500, 400, 400, 300);
    let dialog_size = (300, 200);
    let pos = center_over(target, dialog_size, &[]);
    // No work areas to clamp against -> pure centering:
    assert_eq!(pos, ScreenRect::new(550, 450, 300, 200));
}

// =========================================================================
// 3. Stress Tests: image_viewer::window_size
// =========================================================================

#[test]
fn stress_window_size_extreme_pictures() {
    let screen = Some((1920.0, 1040.0));

    // Extremely large picture (10000 x 10000):
    let (w, h) = window_size((10000, 10000), screen);
    assert_eq!((w, h), (1920.0 * 0.9, 1040.0 * 0.9), "Capped to 90% of screen");

    // Extremely small picture (0 x 0):
    let (w_min, h_min) = window_size((0, 0), screen);
    assert_eq!((w_min, h_min), (360.0 + 84.0, 280.0 + 84.0), "Minimum paper + margin");

    // Extremely small picture (1 x 1):
    let (w_one, h_one) = window_size((1, 1), screen);
    assert_eq!((w_one, h_one), (444.0, 364.0));

    // Ultra-wide picture (10000 x 10):
    let (w_wide, h_wide) = window_size((10000, 10), screen);
    assert_eq!(w_wide, 1920.0 * 0.9, "Width capped to 90%");
    assert_eq!(h_wide, 364.0, "Height at minimum");

    // Ultra-tall picture (10 x 10000):
    let (w_tall, h_tall) = window_size((10, 10000), screen);
    assert_eq!(w_tall, 444.0, "Width at minimum");
    assert_eq!(h_tall, 1040.0 * 0.9, "Height capped to 90%");

    // No screen limit (None):
    let (w_none, h_none) = window_size((5000, 3000), None);
    assert_eq!((w_none, h_none), (5000.0 + 44.0 + 84.0, 3000.0 + 104.0 + 84.0));
}

// =========================================================================
// 4. End-to-End Stress Tests: ImageViewer::open via Snap Harness Platform
// =========================================================================

#[test]
fn stress_image_viewer_open_on_tertiary_and_negative_monitors() {
    snap::platform::install();

    let monitors = vec![SECONDARY_LEFT, PRIMARY, TERTIARY_RIGHT];
    let wc = Rc::new(FakeWindowControl {
        areas: RefCell::new(monitors),
        ..Default::default()
    });
    let cb = Rc::new(FakeClipboard::default());

    // 1. Open over target on negative left monitor:
    let target_left = ScreenRect::new(-1000, 300, 400, 300);
    let viewer_left = ImageViewer::open(
        slint::Image::default(),
        (300, 200),
        "Left Monitor",
        Some(target_left),
        wc.clone(),
        cb.clone(),
        None,
        slint::Image::default(),
        Rc::new(|| {}),
    ).expect("open on left monitor");
    assert!(viewer_left.position().0 < 0, "Position x ({}) must be negative on left monitor", viewer_left.position().0);
    assert!(viewer_left.position().0 >= -1920, "Position x must be inside left monitor");

    // 2. Open over target on tertiary right monitor (x in [3840, 6400]):
    let target_right = ScreenRect::new(4500, 300, 400, 300);
    let viewer_right = ImageViewer::open(
        slint::Image::default(),
        (300, 200),
        "Right Monitor",
        Some(target_right),
        wc.clone(),
        cb.clone(),
        None,
        slint::Image::default(),
        Rc::new(|| {}),
    ).expect("open on right monitor");
    assert!(viewer_right.position().0 >= 3840, "Position x ({}) must be on tertiary monitor", viewer_right.position().0);

    // 3. Open with target = None:
    let viewer_none = ImageViewer::open(
        slint::Image::default(),
        (300, 200),
        "No Target",
        None,
        wc.clone(),
        cb.clone(),
        None,
        slint::Image::default(),
        Rc::new(|| {}),
    ).expect("open with no target");
    // When target is None, target_wa falls back to first work area (SECONDARY_LEFT):
    assert!(viewer_none.position().0 < 0, "Falls back to first work area");

    // 4. Open with empty work areas slice:
    let wc_empty = Rc::new(FakeWindowControl {
        areas: RefCell::new(vec![]),
        ..Default::default()
    });
    let viewer_empty = ImageViewer::open(
        slint::Image::default(),
        (300, 200),
        "Empty Work Areas",
        None,
        wc_empty,
        cb,
        None,
        slint::Image::default(),
        Rc::new(|| {}),
    ).expect("open with empty work areas slice must not panic");
    assert_eq!(viewer_empty.position(), (738, 346), "Centered over fallback 1920x1080 rect");
}
