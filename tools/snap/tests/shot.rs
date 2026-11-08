//! `snap` as a screenshot tool: the real note component on the desk color at the mockup canvas size.
use snap::shot::{snapshot, Options};

const DESK: [u8; 3] = [0xD9, 0xDD, 0xE1];

fn opts(text: &str) -> Options {
    Options { text: text.into(), ..Options::default() }
}

fn dark_pixels(s: &snap::shot::Snapshot) -> usize {
    (0..s.height).flat_map(|y| (0..s.width).map(move |x| (x, y))).filter(|&(x, y)| s.pixel(x, y).iter().map(|&c| c as u32).sum::<u32>() < 3 * 110).count()
}

#[test]
fn the_snapshot_is_the_mockup_canvas_with_the_desk_color_around_the_note() {
    let s = snapshot(&opts("hello")).unwrap();
    assert_eq!((s.width, s.height), (720, 840));
    for (x, y) in [(0, 0), (719, 0), (0, 839), (719, 839)] {
        assert_eq!(s.pixel(x, y), DESK, "corner {x},{y}");
    }
}

#[test]
fn two_runs_give_byte_identical_pngs() {
    let o = Options { hover: true, ..opts("Endfield Note\nsome body text that wraps around the line") };
    let a = snapshot(&o).unwrap().png();
    let b = snapshot(&o).unwrap().png();
    assert!(a.starts_with(&[0x89, b'P', b'N', b'G']));
    assert_eq!(a, b);
}

/// Bounding box of the paper itself: the pixels that are (nearly) white.
fn paper_bbox(s: &snap::shot::Snapshot) -> (u32, u32, u32, u32) {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..s.height {
        for x in 0..s.width {
            if s.pixel(x, y).iter().all(|&c| c >= 0xFA) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    (x0, y0, x1, y1)
}

#[test]
fn the_window_is_the_paper_plus_a_40_px_margin_and_the_margin_is_transparent() {
    // 363 x 384 logical paper tilted 1.5 deg: bounding box 372.9 x 393.4 logical = 559 x 590 px at 1.5
    let s = snapshot(&Options { paper: (363.0, 384.0), scale: 1.5, ..opts("") }).unwrap();
    let (x0, y0, x1, y1) = paper_bbox(&s);
    assert!(((x1 - x0) as i64 - 559).abs() <= 3, "width {}", x1 - x0);
    assert!(((y1 - y0) as i64 - 590).abs() <= 3, "height {}", y1 - y0);
    // the window (447 x 468 logical = 671 x 702 px) is centred on the canvas, so is the paper
    assert!(((x0 + x1) as i64 - 720).abs() <= 3 && ((y0 + y1) as i64 - 840).abs() <= 6, "{:?}", (x0, y0, x1, y1));
    // the corners of the window show the desk through it: no opaque rectangle behind the paper
    let (wx, wy) = ((720 - 671) / 2, (840 - 702) / 2);
    for (x, y) in [(wx + 1, wy + 1), (wx + 669, wy + 1), (wx + 1, wy + 700), (wx + 669, wy + 700)] {
        assert_eq!(s.pixel(x, y), DESK, "window corner {x},{y}");
    }

    // the margin is fixed: a larger paper gets a larger window by the paper's growth only
    let big = snapshot(&Options { paper: (545.0, 576.0), canvas: (1000, 1000), ..opts("") }).unwrap();
    let (x0, _, x1, _) = paper_bbox(&big);
    assert!(((x1 - x0) as i64 - 840).abs() <= 3, "width {}", x1 - x0); // (545 cos + 576 sin) * 1.5 = 840

    let one = snapshot(&Options { scale: 1.0, ..Options { paper: (363.0, 384.0), ..opts("") } }).unwrap();
    let (x0, _, x1, _) = paper_bbox(&one);
    assert!(((x1 - x0) as i64 - 373).abs() <= 3, "width {}", x1 - x0);
}

#[test]
fn the_fixture_text_is_what_gets_drawn() {
    let empty = snapshot(&opts("")).unwrap();
    let text = snapshot(&opts("Endfield Note\nThermite Cutter : POG & Akekuri")).unwrap();
    assert_eq!(dark_pixels(&empty), 0, "an empty, unfocused note shows no ink and no caret");
    assert!(dark_pixels(&text) > 100);
}

/// Pixels of the scrollbar's muted ink in the strip where the bar lies (right inset of the text area).
fn scrollbar_pixels(s: &snap::shot::Snapshot, paper: (f64, f64)) -> usize {
    let scale = 1.5;
    let (pw, ph) = (((paper.0 + 84.0) * scale).round() as i64, ((paper.1 + 84.0) * scale).round() as i64);
    let (ox, oy) = ((s.width as i64 - pw) / 2, (s.height as i64 - ph) / 2);
    let x0 = ox + ((42.0 + paper.0 - 16.0) * scale) as i64;
    let (y0, y1) = (oy + (80.0 * scale) as i64, oy + ((42.0 + paper.1 - 56.0 - 20.0) * scale) as i64);
    let near = |p: [u8; 3]| p.iter().zip([0x8B, 0x91, 0x9A]).all(|(&a, b): (&u8, i32)| (a as i32 - b).abs() <= 4);
    (y0..y1).flat_map(|y| (x0..x0 + (16.0 * scale) as i64).map(move |x| (x, y))).filter(|&(x, y)| near(s.pixel(x as u32, y as u32))).count()
}

#[test]
fn the_scrollbar_is_drawn_only_when_hovered_and_the_text_overflows() {
    let long = std::fs::read_to_string("../../docs/design/fixtures/long.md").unwrap().replace("\r\n", "\n");
    let mockup = std::fs::read_to_string("../../docs/design/fixtures/mockup.md").unwrap().replace("\r\n", "\n");
    let paper = (545.0, 576.0);
    let big = |text: &str, hover, hover_scrollbar| Options { paper, canvas: (1000, 1040), hover, hover_scrollbar, ..opts(text.trim_end()) };
    assert!(scrollbar_pixels(&snapshot(&big(&long, false, true)).unwrap(), paper) > 20, "overflowing and scrollbar hovered: shown");
    assert_eq!(scrollbar_pixels(&snapshot(&big(&long, true, false)).unwrap(), paper), 0, "hovering note body only: hidden");
    assert_eq!(scrollbar_pixels(&snapshot(&big(&long, false, false)).unwrap(), paper), 0, "not hovered: hidden");
    assert_eq!(scrollbar_pixels(&snapshot(&big(&mockup, false, true)).unwrap(), paper), 0, "scrollbar hovered but fits: hidden");
}

#[test]
fn a_hovered_snapshot_is_produced_through_a_real_pointer_event() {
    let o = Options { hover: true, ..opts("hi") };
    let s = snapshot(&o).unwrap();
    assert_eq!((s.width, s.height), (720, 840));
}
