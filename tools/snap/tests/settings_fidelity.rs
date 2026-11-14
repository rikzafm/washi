//! The Settings page against the mockup ("Setting Page" artboard) and against overflow.
//!
//! Each tab is rendered by the real app and compared with `docs/design/reference/settings-<tab>.png`,
//! a render of the mockup's HTML at the same size (the window: the Sidebar's strip, the Paper at its
//! natural height and the margins, at 1x on the desk color). The mockup's text is drawn by a browser
//! (sub-pixel anti-aliasing, 1px differences in where a glyph sits), so the comparison is made on 8px
//! blocks of luminance: layout, colors, shapes and text blocks have to line up, single glyph pixels do
//! not. The Paper's height must be the mockup's (`min-height: 600`, 38/44 padding, the content's own
//! height) and nothing may sit in its bottom padding.

use snap::{tab, Harness};
use std::path::{Path, PathBuf};
use std::time::Duration;
use washi::storage::Geometry;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Every tab in Sidebar order (the index is its `snap::tab` number), by its reference image's name.
const TABS: [&str; 5] = ["general", "appearance", "style", "shortcuts", "privacy"];
/// The tabs the mockup has (Appearance came after it).
const MOCKUP_TABS: [i32; 4] = [tab::GENERAL, tab::STYLE, tab::SHORTCUTS, tab::PRIVACY];
/// The mockup's Paper heights (its DOM, 1x): the content's height plus 38 above and 44 below, at least
/// 600. Appearance is not in the mockup; its one row is far under the minimum.
const MOCKUP_PAPER_H: [f32; 5] = [658.4, 600.0, 603.2, 766.8, 600.0];
/// The Appearance Paper (not in the mockup), as laid out with both "Same for both sidebars" switches
/// on: UI size, the Sidebar group and Settings height (the 583 they needed before the Theme rows, under
/// the 600 minimum) plus the Theme rows' 116 (UI size's rule 2, the switch row 58, the dropdown row
/// 56). Under the Tallest tab cap (Shortcuts), so it does not scroll.
const APPEARANCE_PAPER_H: f32 = 583.0 + 116.0;
/// The General row added after the mockup ("Notes left in All Notes"): its height with the rule.
const PARK_ROW_H: f32 = 82.0;
/// The General row added after that ("Keep text tilted while editing", above it): one line of hint, 56
/// minimum and its 2px rule.
const TILT_ROW_H: f32 = 58.0;
/// The Font & Style rows taken out since the mockup: Fade text edge (80, its hint on two lines, and its
/// 2px rule) and UI size (the 56 minimum, the last row; the Tape color row above it loses its 2px rule
/// instead).
const STYLE_REMOVED_H: f32 = 82.0 + 56.0 + 2.0;
/// General's rows above the new one end about here in the window: only this much of General's Paper
/// can be held against its mockup (everything below moved down by the new row).
const GENERAL_COMPARED_H: u32 = 248;
/// Font & Style's rows above the Font size row end about here in the window (the Font row's bottom): only
/// this much of its Paper can be held against the mockup, whose Text size row was three pills and is now
/// a slider and a number box (Font size). The Letter spacing row and the removed rows differ too.
const STYLE_COMPARED_H: u32 = 314;
/// The Sidebar's header and first tab end about here in a window as tall as the mockup's: the tabs
/// under it are not the mockup's since Appearance came in second. (On General and Font & Style the
/// window is not the mockup's height, and the Sidebar, centred on it, is not where the mockup has it.)
const SIDEBAR_COMPARED_H: u32 = 256;

/// The Paper height a tab should have: the mockup's, plus the row added to General since, less the
/// rows taken out of Font & Style, at least the 600 minimum.
fn want_paper_h(i: usize) -> f32 {
    if i as i32 == tab::APPEARANCE {
        return APPEARANCE_PAPER_H;
    }
    let changed = match i as i32 {
        tab::GENERAL => PARK_ROW_H + TILT_ROW_H,
        tab::STYLE => -STYLE_REMOVED_H,
        _ => 0.0,
    };
    (MOCKUP_PAPER_H[i] + changed).max(600.0)
}

/// The top `rows` of an image, inside the Sidebar's strip.
fn strip_top_rows(img: &(u32, u32, Vec<u8>), rows: u32) -> (u32, u32, Vec<u8>) {
    let w = STRIP as u32;
    let mut out = Vec::with_capacity((w * rows * 3) as usize);
    for y in 0..rows {
        let start = ((y * img.0) * 3) as usize;
        out.extend_from_slice(&img.2[start..start + (w * 3) as usize]);
    }
    (w, rows, out)
}

/// The top `rows` of an image, right of the Sidebar's strip.
fn paper_top_rows(img: &(u32, u32, Vec<u8>), rows: u32) -> (u32, u32, Vec<u8>) {
    let left = STRIP as u32;
    let w = img.0 - left;
    let mut out = Vec::with_capacity((w * rows * 3) as usize);
    for y in 0..rows {
        let start = ((y * img.0 + left) * 3) as usize;
        out.extend_from_slice(&img.2[start..start + (w * 3) as usize]);
    }
    (w, rows, out)
}
const MARGIN: f32 = 42.0;
const STRIP: f32 = 282.0;
const PAPER_W: f32 = 520.0;

fn reference_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design/reference")
}

/// Settings open on `tab`, the transition and the tab switch finished.
fn open_on(tab: i32) -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    if tab != 0 {
        h.settings_click_tab(tab);
        h.advance_frames(ms(600));
    }
    h
}

fn load_png_rgb(path: &Path) -> (u32, u32, Vec<u8>) {
    let file = std::fs::File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    let mut reader = png::Decoder::new(file).read_info().expect("read png info");
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("decode png frame");
    let (w, h) = (info.width, info.height);
    let rgb = match info.color_type {
        png::ColorType::Rgb => buf[..(w * h * 3) as usize].to_vec(),
        png::ColorType::Rgba => buf[..(w * h * 4) as usize].chunks_exact(4).flat_map(|c| [c[0], c[1], c[2]]).collect(),
        other => panic!("unsupported png color type: {other:?}"),
    };
    (w, h, rgb)
}

fn write_png_rgb(path: &Path, width: u32, height: u32, rgb: &[u8]) {
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("png header");
        w.write_image_data(rgb).expect("png data");
    }
    std::fs::write(path, out).expect("write png file");
}

fn luma(rgb: &[u8], i: usize) -> f32 {
    0.299 * rgb[i * 3] as f32 + 0.587 * rgb[i * 3 + 1] as f32 + 0.114 * rgb[i * 3 + 2] as f32
}

/// How far two same-width images are apart on 8px blocks of luminance, over the rows both have:
/// (mean absolute difference, share of blocks that differ by more than 30 levels).
fn block_difference(a: &(u32, u32, Vec<u8>), b: &(u32, u32, Vec<u8>)) -> (f32, f32) {
    assert_eq!(a.0, b.0, "widths");
    let (w, h) = (a.0 as usize, a.1.min(b.1) as usize);
    let (bw, bh) = (w / 8, h / 8);
    let (mut sum, mut far) = (0.0f32, 0usize);
    for by in 0..bh {
        for bx in 0..bw {
            let (mut sa, mut sb) = (0.0f32, 0.0f32);
            for y in by * 8..by * 8 + 8 {
                for x in bx * 8..bx * 8 + 8 {
                    sa += luma(&a.2, y * w + x);
                    sb += luma(&b.2, y * w + x);
                }
            }
            let d = (sa - sb).abs() / 64.0;
            sum += d;
            if d > 30.0 {
                far += 1;
            }
        }
    }
    (sum / (bw * bh) as f32, far as f32 / (bw * bh) as f32)
}

fn rendered(h: &Harness) -> (u32, u32, Vec<u8>) {
    let shot = h.render_settings();
    (shot.width, shot.height, shot.rgb().to_vec())
}

/// How far left of the mockup's `nav` the Sidebar's panel sits: the mockup centres it at x = 161
/// (its right edge under the Paper); the app uses the All notes panel, centred at 143.
const SIDEBAR_SHIFT: u32 = 161 - 143;

/// The mockup render of `tab`, with its Sidebar strip moved `SIDEBAR_SHIFT` left to where the app
/// draws the panel (the Paper's side is the mockup's as rendered).
fn reference(tab: &str) -> (u32, u32, Vec<u8>) {
    let (w, h, mut rgb) = load_png_rgb(&reference_dir().join(format!("settings-{tab}.png")));
    let src = rgb.clone();
    for y in 0..h {
        for x in 0..STRIP as u32 {
            let (to, from) = (((y * w + x) * 3) as usize, ((y * w + x + SIDEBAR_SHIFT) * 3) as usize);
            rgb[to..to + 3].copy_from_slice(&src[from..from + 3]);
        }
    }
    (w, h, rgb)
}

/// The Paper's logical height as the window has it.
fn paper_h(shot: &(u32, u32, Vec<u8>)) -> f32 {
    shot.1 as f32 - 2.0 * MARGIN
}

/// How many pixels in the Paper's bottom padding (the 34px above its last 6, clear of the blue tape)
/// are not paper-colored: content that ran past where the Paper ends.
fn ink_in_bottom_padding(shot: &(u32, u32, Vec<u8>)) -> usize {
    let (w, h, rgb) = (shot.0 as usize, shot.1 as usize, &shot.2);
    let ph = paper_h(shot);
    let tilt = 1.5f32.to_radians();
    let (cs, sn) = (tilt.cos(), tilt.sin());
    let (cx, cy) = (STRIP + MARGIN + PAPER_W / 2.0, h as f32 / 2.0);
    let mut bad = 0;
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            let a = dx * cs + dy * sn + PAPER_W / 2.0;
            let b = -dx * sn + dy * cs + ph / 2.0;
            if (50.0..400.0).contains(&a) && (ph - 40.0..ph - 6.0).contains(&b) {
                let p = &rgb[(y * w + x) * 3..(y * w + x) * 3 + 3];
                if p[0] < 0xF2 || p[1] < 0xF2 || p[2] < 0xF0 {
                    bad += 1;
                }
            }
        }
    }
    bad
}

fn dump(name: &str, shot: &(u32, u32, Vec<u8>)) {
    if let Some(dir) = std::env::var_os("WASHI_SNAP_OUT_DIR") {
        write_png_rgb(&Path::new(&dir).join(name), shot.0, shot.1, &shot.2);
    }
}

#[test]
fn every_tab_looks_like_the_mockup() {
    for t in MOCKUP_TABS {
        let (i, tab) = (t as usize, TABS[t as usize]);
        let mut h = open_on(t);
        h.settings_show_mockup_values();
        let shot = rendered(&h);
        dump(&format!("settings-{tab}.png"), &shot);
        let want = reference(tab);
        assert_eq!(shot.0, want.0, "{tab}: window width");
        let extra = (want_paper_h(i) - MOCKUP_PAPER_H[i]).round() as i32;
        assert!((shot.1 as i32 - want.1 as i32 - extra).abs() <= 3, "{tab}: window height {} against the mockup's {} + {extra}", shot.1, want.1);
        let mut parts = vec![match t {
            tab::GENERAL => (paper_top_rows(&shot, GENERAL_COMPARED_H), paper_top_rows(&want, GENERAL_COMPARED_H)),
            tab::STYLE => (paper_top_rows(&shot, STYLE_COMPARED_H), paper_top_rows(&want, STYLE_COMPARED_H)),
            _ => (paper_top_rows(&shot, shot.1), paper_top_rows(&want, want.1)),
        }];
        // The mockup's Sidebar is 600 tall and centred; the app's (Match note, the default) is as
        // tall as the Paper, so the two only line up where the Paper is at its 600 minimum.
        if extra == 0 && MOCKUP_PAPER_H[i] == 600.0 {
            parts.push((strip_top_rows(&shot, SIDEBAR_COMPARED_H), strip_top_rows(&want, SIDEBAR_COMPARED_H)));
        }
        for (shot, want) in parts {
            let (mean, far) = block_difference(&shot, &want);
            assert!(mean < 2.6, "{tab}: blocks differ by {mean:.2} levels on average");
            assert!(far < 0.02, "{tab}: {:.1}% of the blocks are far from the mockup", far * 100.0);
        }
    }
}

/// The Default theme's slate fill, #D6DBE2 (the desk the shot is drawn on, #D9DDE1, is 3 levels off
/// in red).
const SLATE: [u8; 3] = [0xD6, 0xDB, 0xE2];
/// The Paper theme's fill: the Note's paper.
const PAPER_FILL: [u8; 3] = [0xFF, 0xFF, 0xFD];

/// The leftmost and rightmost Sidebar-panel pixels (exactly `fill`, within a level) in the strip on
/// row `y`.
fn panel_edges(w: u32, rgb: &[u8], y: usize, fill: [u8; 3]) -> Option<(usize, usize)> {
    let is_fill = |x: usize| {
        let i = (y * w as usize + x) * 3;
        (0..3).all(|c| (rgb[i + c] as i32 - fill[c] as i32).abs() <= 1)
    };
    let xs: Vec<usize> = (0..STRIP as usize).filter(|&x| is_fill(x)).collect();
    Some((*xs.first()?, *xs.last()?))
}

#[test]
fn the_settings_sidebar_has_the_all_notes_sidebars_panel_outline() {
    let h = open_on(0);
    let settings = rendered(&h);

    let mut notes = Harness::new();
    notes.start().unwrap();
    notes.set_note_text(0, "# Solo");
    notes.toggle_dropdown(0);
    notes.invoke_dropdown_all_notes();
    notes.advance_frames(ms(600));
    let (nw, nh, px) = notes.render_all_notes().unwrap();
    let notes_rgb: Vec<u8> = px.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    dump("sidebar-outline-settings.png", &settings);
    dump("sidebar-outline-all-notes.png", &(nw, nh, notes_rgb.clone()));

    // Both panels are centred on their window's height: compare rows at the same offsets from it.
    let (s_mid, n_mid) = (settings.1 as i32 / 2, nh as i32 / 2);
    for dy in [-120, -60, 0, 60, 120] {
        let s = panel_edges(settings.0, &settings.2, (s_mid + dy) as usize, SLATE).expect("settings panel drawn");
        let n = panel_edges(nw, &notes_rgb, (n_mid + dy) as usize, SLATE).expect("all notes panel drawn");
        assert!(
            (s.0 as i32 - n.0 as i32).abs() <= 1 && (s.1 as i32 - n.1 as i32).abs() <= 1,
            "row {dy:+} from the middle: Settings panel spans {s:?}, All notes {n:?}"
        );
    }
    // Centred at 143, 270 wide: x = 8 to 278, clear of the Paper.
    let (l, r) = panel_edges(settings.0, &settings.2, s_mid as usize, SLATE).unwrap();
    assert!((l as i32 - 8).abs() <= 1 && (r as i32 - 277).abs() <= 1, "Settings panel spans {l}..={r}");
}

/// The Sidebar Themes in dropdown order, by their reference shot's name.
const THEMES: [&str; 4] = ["default", "transparent", "paper", "glass"];
/// The Appearance Tab's first Theme dropdown and, after it, its options (see appearance-tab.slint).
const THEME_DROPDOWN: i32 = 41;

/// Settings open on Appearance with `theme` chosen for both Sidebars, the pointer off every control.
fn open_with_theme(theme: usize) -> Harness {
    let mut h = open_on(tab::APPEARANCE);
    h.settings_appearance_click(THEME_DROPDOWN);
    h.settings_appearance_click(THEME_DROPDOWN + 1 + theme as i32);
    h.settings_pointer_away();
    h
}

#[test]
fn the_paper_theme_keeps_the_panel_outline_and_its_tilt() {
    let h = open_with_theme(2);
    let shot = rendered(&h);
    let mid = shot.1 as i32 / 2;
    let (l, r) = panel_edges(shot.0, &shot.2, mid as usize, PAPER_FILL).expect("a paper panel");
    assert!((l as i32 - 8).abs() <= 1 && (r as i32 - 277).abs() <= 1, "Paper panel spans {l}..={r}");
    // -1deg: going down, the left edge moves right (tan 1deg over 400px is 7px).
    let (top, bottom) = (
        panel_edges(shot.0, &shot.2, (mid - 200) as usize, PAPER_FILL).unwrap(),
        panel_edges(shot.0, &shot.2, (mid + 200) as usize, PAPER_FILL).unwrap(),
    );
    // The Note's shadow hangs under the panel's bottom edge.
    let paper_at = |y: usize| {
        let i = (y * shot.0 as usize + 143) * 3;
        (0..3).all(|c| (shot.2[i + c] as i32 - PAPER_FILL[c] as i32).abs() <= 1)
    };
    let bottom_edge = (0..shot.1 as usize).filter(|&y| paper_at(y)).last().expect("a paper panel");
    let under = (bottom_edge + 6) * shot.0 as usize + 143;
    let p = &shot.2[under * 3..under * 3 + 3];
    let desk = snap::DESK;
    assert!((0..3).all(|c| p[c] + 3 <= desk[c]), "a shadow under the panel: {p:?}");
    let lean = bottom.0 as i32 - top.0 as i32;
    assert!((5..=9).contains(&lean), "left edge {} at the top, {} at the bottom", top.0, bottom.0);
}

/// Each Sidebar Theme's Settings Sidebar against its reference shot: `docs/design/reference/
/// sidebar-theme-<theme>.png`, the app's own render of the Sidebar strip (not a mockup). A missing
/// shot is written when `WASHI_BLESS` is set (look at it before committing it).
#[test]
fn each_sidebar_theme_matches_its_reference_shot() {
    for (i, theme) in THEMES.iter().enumerate() {
        let h = open_with_theme(i);
        let shot = rendered(&h);
        let strip = strip_top_rows(&shot, shot.1);
        let name = format!("sidebar-theme-{theme}.png");
        dump(&name, &strip);
        let path = reference_dir().join(&name);
        if !path.exists() && std::env::var_os("WASHI_BLESS").is_some() {
            write_png_rgb(&path, strip.0, strip.1, &strip.2);
        }
        let want = load_png_rgb(&path);
        assert_eq!((strip.0, strip.1), (want.0, want.1), "{theme}: shot size");
        let (mean, far) = block_difference(&strip, &want);
        assert!(mean < 1.0, "{theme}: blocks differ by {mean:.2} levels on average");
        assert!(far < 0.005, "{theme}: {:.1}% of the blocks are far from the reference", far * 100.0);
    }
}

#[test]
fn the_theme_shots_differ_from_each_other() {
    // The reference comparison can fail: no two themes render alike.
    let shots: Vec<_> = (0..THEMES.len()).map(|i| strip_top_rows(&rendered(&open_with_theme(i)), 600)).collect();
    for a in 0..shots.len() {
        for b in a + 1..shots.len() {
            let (mean, _) = block_difference(&shots[a], &shots[b]);
            assert!(mean > 1.0, "{} and {} look alike ({mean:.2})", THEMES[a], THEMES[b]);
        }
    }
}

#[test]
fn the_appearance_tab_fits_its_paper() {
    // Not in the mockup: only its height and its bottom padding are checked.
    let h = open_on(tab::APPEARANCE);
    let shot = rendered(&h);
    dump("settings-appearance.png", &shot);
    assert!((paper_h(&shot) - want_paper_h(tab::APPEARANCE as usize)).abs() <= 3.0, "Paper {}", paper_h(&shot));
    assert_eq!(ink_in_bottom_padding(&shot), 0);
}

#[test]
fn the_comparison_can_fail() {
    // The Shortcuts page is not the General reference.
    let mut h = open_on(tab::SHORTCUTS);
    h.settings_show_mockup_values();
    let shot = rendered(&h);
    let wrong = reference("general");
    let (mean, far) = block_difference(&paper_top_rows(&shot, shot.1), &paper_top_rows(&wrong, wrong.1));
    assert!(mean >= 2.6 || far >= 0.02, "a different tab matched ({mean:.2}, {far:.3})");
}

#[test]
fn each_paper_is_as_tall_as_its_content_needs() {
    let mut h = open_on(0);
    for (i, tab) in TABS.iter().enumerate() {
        if i > 0 {
            h.settings_click_tab(i as i32);
            h.advance_frames(ms(600));
        }
        let (_, _, _, wh) = h.settings_window_geometry().unwrap();
        let paper = wh as f32 - 2.0 * MARGIN;
        let bottoms = h.settings_content_bottoms();
        assert!(paper >= bottoms[i] + 44.0 - 0.5, "{tab}: Paper {paper} is shorter than its content {} plus the 44px padding", bottoms[i]);
        assert!(paper >= 600.0, "{tab}: Paper {paper} is under the 600px minimum");
        assert!((paper - want_paper_h(i)).abs() <= 3.0, "{tab}: Paper {paper} against {}", want_paper_h(i));
    }
}

#[test]
fn no_tab_puts_content_in_the_papers_bottom_padding() {
    let mut h = open_on(0);
    for (i, tab) in TABS.iter().enumerate() {
        if i > 0 {
            h.settings_click_tab(i as i32);
            h.advance_frames(ms(600));
        }
        h.settings_show_mockup_values();
        let shot = rendered(&h);
        assert_eq!(ink_in_bottom_padding(&shot), 0, "{tab}: something is drawn in the bottom padding");
    }
}

#[test]
fn the_padding_check_can_fail() {
    // A window the app failed to grow: the content runs into where the padding should be.
    let mut h = open_on(tab::SHORTCUTS);
    h.settings_squeeze_window(60.0);
    let shot = rendered(&h);
    assert!(ink_in_bottom_padding(&shot) > 100, "a squeezed Shortcuts page should overflow");
}

#[test]
fn settings_opens_at_the_height_of_the_general_tab() {
    // General is taller than 600: the page must not open at 600 and wait for a tab switch.
    let h = open_on(0);
    let (_, _, _, wh) = h.settings_window_geometry().unwrap();
    assert!((wh as f32 - 2.0 * MARGIN - want_paper_h(0)).abs() <= 3.0, "opened at window height {wh}");
}

#[test]
fn switching_tabs_mid_tween_still_ends_at_the_right_height() {
    let mut h = open_on(0);
    h.settings_click_tab(tab::SHORTCUTS);
    h.advance_frames(ms(80));
    h.settings_click_tab(tab::STYLE);
    h.advance_frames(ms(60));
    h.settings_click_tab(tab::PRIVACY);
    h.advance_frames(ms(60));
    h.settings_click_tab(tab::APPEARANCE);
    h.advance_frames(ms(60));
    h.settings_click_tab(tab::GENERAL);
    h.advance_frames(ms(600));
    let (_, _, _, wh) = h.settings_window_geometry().unwrap();
    let paper = wh as f32 - 2.0 * MARGIN;
    assert!((paper - want_paper_h(0)).abs() <= 3.0, "ended at {paper}");
    let shot = rendered(&h);
    assert_eq!(ink_in_bottom_padding(&shot), 0);
}

#[test]
fn a_bigger_preview_text_stays_on_the_paper_and_the_lists_stay_in_the_window() {
    let mut h = open_on(tab::STYLE);
    let before = h.settings_content_bottoms()[tab::STYLE as usize];
    // Font size 40 (the slider's right end): the preview text is cut at its card, so the content does
    // not grow and the rows do not move under the pointer.
    h.settings_style_click(140);
    h.advance_frames(ms(100));
    let after = h.settings_content_bottoms()[tab::STYLE as usize];
    assert_eq!(after, before, "the content went from {before} to {after}");
    let (_, _, _, wh) = h.settings_window_geometry().unwrap();
    let ph = wh as f32 - 2.0 * MARGIN;
    assert!(ph >= after + 44.0 - 0.5, "the Paper {ph} holds the content {after} and its padding");
    let shot = rendered(&h);
    assert_eq!(ink_in_bottom_padding(&shot), 0, "the larger preview overflowed");
    // The lists hang inside the window (the Paper plus its margin) whatever the size.
    assert!(h.settings_style_lists_bottom() <= ph + MARGIN - 14.0, "a list reaches {} of {ph}", h.settings_style_lists_bottom());
    // And back to 12: the card is the same.
    h.settings_style_click(112);
    h.advance_frames(ms(100));
    assert_eq!(h.settings_content_bottoms()[tab::STYLE as usize], before);
    assert_eq!(ink_in_bottom_padding(&rendered(&h)), 0);
}

#[test]
fn the_open_lists_fit_in_the_window_at_the_default_size() {
    let mut h = open_on(tab::STYLE);
    let (_, _, _, wh) = h.settings_window_geometry().unwrap();
    let ph = wh as f32 - 2.0 * MARGIN;
    let reach = h.settings_style_lists_bottom();
    assert!(reach <= ph + MARGIN - 14.0, "a list reaches {reach} of a {ph} Paper");
    // Open each list and compare it with the mockup's open dropdown.
    for (button, name) in [(10, "style-tape-list"), (0, "style-font-list")] {
        h.settings_style_click(button);
        h.settings_pointer_away();
        let shot = rendered(&h);
        dump(&format!("settings-{name}.png"), &shot);
        let want = reference(name);
        assert_eq!(shot.0, want.0, "{name}: window width");
        // The Paper above the rows taken out since the mockup (see `STYLE_COMPARED_H`).
        let (mean, far) = block_difference(&paper_top_rows(&shot, STYLE_COMPARED_H), &paper_top_rows(&want, STYLE_COMPARED_H));
        assert!(mean < 2.6, "{name}: blocks differ by {mean:.2} levels on average");
        assert!(far < 0.02, "{name}: {:.1}% of the blocks are far from the mockup", far * 100.0);
    }
}
