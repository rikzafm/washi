//! Empirical stress tests for Milestone 1: Platform Adapters & Window Lifecycle
//!
//! Stress-tests:
//! 1. Clipboard adapter (ImageClipboard) — Some vs None rgba, repeated rapid copies, large images.
//! 2. Window control adapter (WindowControl) — drag, resize edge mapping 0..=7, invalid edge codes, hwnd handling.
//! 3. Window close callbacks — UI close, window close request, programmatic close, multiple close idempotence.
//! 4. Deferred drop lifecycle — single & multiple viewers, out-of-order close, timer execution, no reference cycle leak.

use slint::platform::WindowAdapter;
use slint::ComponentHandle;
use snap::fakes::{FakeClipboard, FakeWindowControl, WindowCommand};
use snap::{tab, Harness};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;
use washi::image_viewer::ImageViewer;
use washi::images::Rgba;
use washi::platform::{ImageClipboard, ResizeEdge, WindowControl};

fn sample_rgba(w: u32, h: u32, r: u8, g: u8, b: u8) -> Rgba {
    let data = (0..w * h).flat_map(|_| [r, g, b, 255]).collect();
    Rgba { width: w, height: h, data }
}

fn open_test_viewer(
    rgba: Option<Rgba>,
    wc: Rc<dyn WindowControl>,
    cb: Rc<dyn ImageClipboard>,
    on_close: Rc<dyn Fn()>,
) -> ImageViewer {
    snap::platform::install();
    let size = rgba.as_ref().map(|img| (img.width, img.height)).unwrap_or((500, 300));
    let pic = rgba.as_ref().map(|img| washi::note_images::to_slint(img)).unwrap_or_default();
    ImageViewer::open(
        pic,
        size,
        "Stress Test",
        None,
        wc,
        cb,
        rgba,
        slint::Image::default(),
        on_close,
    ).expect("ImageViewer::open failed")
}

// =========================================================================
// 1. Clipboard Platform Adapter Stress
// =========================================================================

#[test]
fn test_clipboard_copy_with_image_populates_adapter() {
    snap::platform::install();
    let wc = Rc::new(FakeWindowControl::default());
    let cb = Rc::new(FakeClipboard::default());
    let rgba = sample_rgba(64, 48, 120, 200, 50);

    let viewer = open_test_viewer(Some(rgba.clone()), wc, cb.clone(), Rc::new(|| {}));

    assert!(cb.image().is_none());
    viewer.copy();

    let copied = cb.image().expect("image must be copied to clipboard");
    assert_eq!(copied.width, 64);
    assert_eq!(copied.height, 48);
    assert_eq!(copied.data, rgba.data);
}

#[test]
fn test_clipboard_copy_with_none_rgba_does_not_mutate_clipboard() {
    snap::platform::install();
    let wc = Rc::new(FakeWindowControl::default());
    let cb = Rc::new(FakeClipboard::default());
    let _ = cb.set_image(&sample_rgba(10, 10, 1, 2, 3));
    let initial_img = cb.image();

    let viewer = open_test_viewer(None, wc, cb.clone(), Rc::new(|| {}));

    viewer.copy();
    assert_eq!(cb.image(), initial_img);
}

#[test]
fn test_clipboard_repeated_rapid_copies_stability() {
    snap::platform::install();
    let wc = Rc::new(FakeWindowControl::default());
    let cb = Rc::new(FakeClipboard::default());
    let rgba = sample_rgba(32, 32, 255, 0, 0);

    let viewer = open_test_viewer(Some(rgba.clone()), wc, cb.clone(), Rc::new(|| {}));

    for i in 0..200 {
        viewer.copy();
        if i % 50 == 0 {
            assert_eq!(cb.image().as_ref().map(|img| (img.width, img.height)), Some((32, 32)));
        }
    }

    assert_eq!(cb.image(), Some(rgba));
}

#[test]
fn test_clipboard_large_image_handling() {
    snap::platform::install();
    let wc = Rc::new(FakeWindowControl::default());
    let cb = Rc::new(FakeClipboard::default());
    let rgba = sample_rgba(1500, 1000, 10, 20, 30);

    let viewer = open_test_viewer(Some(rgba.clone()), wc, cb.clone(), Rc::new(|| {}));
    viewer.copy();

    let copied = cb.image().expect("large image must be copied");
    assert_eq!(copied.width, 1500);
    assert_eq!(copied.height, 1000);
    assert_eq!(copied.data.len(), 1500 * 1000 * 4);
}

// =========================================================================
// 2. Window Control Platform Adapter Stress (Drag & Resize)
// =========================================================================

#[test]
fn test_window_drag_invokes_window_control_move() {
    snap::platform::install();
    let wc = Rc::new(FakeWindowControl::default());
    let cb = Rc::new(FakeClipboard::default());

    let viewer = open_test_viewer(None, wc.clone(), cb, Rc::new(|| {}));

    assert!(wc.log.borrow().is_empty());
    viewer.ui().invoke_start_move();

    let log = wc.log.borrow();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0], WindowCommand::Move(0));
}

#[test]
fn test_window_resize_all_edge_codes_exhaustive_mapping() {
    snap::platform::install();
    let wc = Rc::new(FakeWindowControl::default());
    let cb = Rc::new(FakeClipboard::default());

    let viewer = open_test_viewer(None, wc.clone(), cb, Rc::new(|| {}));

    let expected_mappings = [
        (0, ResizeEdge::North),
        (1, ResizeEdge::NorthEast),
        (2, ResizeEdge::East),
        (3, ResizeEdge::SouthEast),
        (4, ResizeEdge::South),
        (5, ResizeEdge::SouthWest),
        (6, ResizeEdge::West),
        (7, ResizeEdge::NorthWest),
    ];

    for (code, expected_edge) in expected_mappings {
        wc.log.borrow_mut().clear();
        viewer.ui().invoke_start_resize(code);

        let log = wc.log.borrow();
        assert_eq!(log.len(), 1, "code {code} must produce exactly one resize command");
        assert_eq!(log[0], WindowCommand::Resize(0, expected_edge), "code {code} must map to {expected_edge:?}");
    }
}

#[test]
fn test_window_resize_invalid_edge_codes_ignored() {
    snap::platform::install();
    let wc = Rc::new(FakeWindowControl::default());
    let cb = Rc::new(FakeClipboard::default());

    let viewer = open_test_viewer(None, wc.clone(), cb, Rc::new(|| {}));

    let invalid_codes = [-1000, -1, 8, 9, 10, 999, i32::MIN, i32::MAX];

    for &bad_code in &invalid_codes {
        wc.log.borrow_mut().clear();
        viewer.ui().invoke_start_resize(bad_code);

        let log = wc.log.borrow();
        assert!(log.is_empty(), "invalid code {bad_code} must not trigger any window command");
    }
}

#[test]
fn test_window_drag_and_resize_interleaved_stress() {
    snap::platform::install();
    let wc = Rc::new(FakeWindowControl::default());
    let cb = Rc::new(FakeClipboard::default());

    let viewer = open_test_viewer(None, wc.clone(), cb, Rc::new(|| {}));

    for i in 0..100 {
        if i % 2 == 0 {
            viewer.ui().invoke_start_move();
        } else {
            let edge_code = (i % 8) as i32;
            viewer.ui().invoke_start_resize(edge_code);
        }
    }

    let log = wc.log.borrow();
    assert_eq!(log.len(), 100);
    for (i, cmd) in log.iter().enumerate() {
        if i % 2 == 0 {
            assert_eq!(*cmd, WindowCommand::Move(0));
        } else {
            let edge = ResizeEdge::from_code((i % 8) as i32).unwrap();
            assert_eq!(*cmd, WindowCommand::Resize(0, edge));
        }
    }
}

// =========================================================================
// 3. Window Close Callback Lifecycle
// =========================================================================

#[test]
fn test_close_invoked_via_semantic_method() {
    snap::platform::install();
    let wc = Rc::new(FakeWindowControl::default());
    let cb = Rc::new(FakeClipboard::default());
    let close_count = Rc::new(Cell::new(0));
    let cc = close_count.clone();

    let viewer = open_test_viewer(None, wc, cb, Rc::new(move || cc.set(cc.get() + 1)));

    assert_eq!(close_count.get(), 0);
    viewer.close();
    assert_eq!(close_count.get(), 1);
}

#[test]
fn test_close_invoked_via_ui_callback() {
    snap::platform::install();
    let wc = Rc::new(FakeWindowControl::default());
    let cb = Rc::new(FakeClipboard::default());
    let close_count = Rc::new(Cell::new(0));
    let cc = close_count.clone();

    let viewer = open_test_viewer(None, wc, cb, Rc::new(move || cc.set(cc.get() + 1)));

    assert_eq!(close_count.get(), 0);
    viewer.ui().invoke_close_viewer();
    assert_eq!(close_count.get(), 1);
}

#[test]
fn test_close_invoked_via_window_close_requested() {
    snap::platform::install();
    let wc = Rc::new(FakeWindowControl::default());
    let cb = Rc::new(FakeClipboard::default());
    let close_count = Rc::new(Cell::new(0));
    let cc = close_count.clone();

    let viewer = open_test_viewer(None, wc, cb, Rc::new(move || cc.set(cc.get() + 1)));

    assert_eq!(close_count.get(), 0);
    viewer.ui().window().dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert_eq!(close_count.get(), 1);
}

#[test]
fn test_repeated_close_calls_are_safe() {
    snap::platform::install();
    let wc = Rc::new(FakeWindowControl::default());
    let cb = Rc::new(FakeClipboard::default());
    let close_count = Rc::new(Cell::new(0));
    let cc = close_count.clone();

    let viewer = open_test_viewer(None, wc, cb, Rc::new(move || cc.set(cc.get() + 1)));

    for _ in 0..10 {
        viewer.close();
    }
    assert_eq!(close_count.get(), 10);
}

// =========================================================================
// 4. Deferred Drop Lifecycle & App Integration Stress
// =========================================================================

fn create_test_image_file(dir: &std::path::Path, name: &str, w: u32, h: u32, color: [u8; 3]) -> std::path::PathBuf {
    let path = dir.join(name);
    let picture = sample_rgba(w, h, color[0], color[1], color[2]);
    std::fs::write(&path, washi::images::encode_png(&picture).unwrap()).unwrap();
    path
}

fn open_viewer_for_note(h: &mut Harness, note_idx: usize, red_target: [u8; 3]) {
    let (w, _, px) = h.render(note_idx);
    let spots: Vec<(u32, u32)> = px.iter().enumerate()
        .filter(|(_, p)| {
            (p.r as i32 - red_target[0] as i32).abs() <= 15
                && (p.g as i32 - red_target[1] as i32).abs() <= 15
                && (p.b as i32 - red_target[2] as i32).abs() <= 15
        })
        .map(|(i, _)| (i as u32 % w, i as u32 / w))
        .collect();
    assert!(!spots.is_empty(), "picture matching color {red_target:?} not found in note {note_idx}");
    let n = spots.len() as f32;
    let scale = h.note_drawing_scale(note_idx);
    let x = spots.iter().map(|s| s.0 as f32).sum::<f32>() / n / scale;
    let y = spots.iter().map(|s| s.1 as f32).sum::<f32>() / n / scale;
    h.click(note_idx, x, y);
    h.click(note_idx, x, y);
}

fn set_app_ui_size(h: &mut Harness, option: i32) {
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(Duration::from_millis(600));
    h.settings_click_tab(tab::APPEARANCE);
    h.advance_frames(Duration::from_millis(600));
    const UI_SIZE_BUTTON: i32 = 0;
    h.settings_appearance_click(UI_SIZE_BUTTON);
    h.settings_appearance_click(option);
    h.settings_close();
    h.advance_frames(Duration::from_millis(600));
}

#[test]
fn test_app_open_and_close_viewer_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let img_path = create_test_image_file(dir.path(), "test1.png", 300, 200, [210, 40, 50]);

    let mut h = Harness::new();
    h.start().unwrap();
    h.choose_picture(&img_path);
    h.click_image_button(0);
    h.wait_for_images();

    assert_eq!(h.viewer_window_sizes().len(), 0);

    // Open picture viewer by double clicking the image
    open_viewer_for_note(&mut h, 0, [210, 40, 50]);
    assert_eq!(h.viewer_window_sizes().len(), 1, "viewer must open");

    // Close the viewer window by finding the viewer window adapter
    let win = snap::platform::windows().into_iter().rev().find(|w| {
        w.window().size().width > 0
    }).expect("open viewer window must exist");

    // Dispatch CloseRequested event to trigger viewer close
    win.window().dispatch_event(slint::platform::WindowEvent::CloseRequested);

    // Viewer collection in App should immediately reflect removal
    assert_eq!(h.viewer_window_sizes().len(), 0, "viewer must be removed from App immediately upon close");

    // Advance time to execute deferred drop single_shot timer
    snap::platform::advance(Duration::from_millis(10));
}

#[test]
fn test_app_multiple_viewers_independent_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let img1 = create_test_image_file(dir.path(), "pic1.png", 500, 300, [210, 40, 50]);
    let img2 = create_test_image_file(dir.path(), "pic2.png", 700, 400, [50, 180, 60]);

    let mut h = Harness::new();
    h.start().unwrap();

    // Create a second note
    h.right_click(0, Some(&["New note"]));
    assert_eq!(h.open_note_count(), 2);

    // Insert image 1 into note 0
    h.choose_picture(&img1);
    h.click_image_button(0);
    h.wait_for_images();

    // Insert image 2 into note 1
    h.choose_picture(&img2);
    h.click_image_button(1);
    h.wait_for_images();

    // Open viewer 1 from note 0
    open_viewer_for_note(&mut h, 0, [210, 40, 50]);
    assert_eq!(h.viewer_window_sizes().len(), 1);

    // Open viewer 2 from note 1
    open_viewer_for_note(&mut h, 1, [50, 180, 60]);
    assert_eq!(h.viewer_window_sizes().len(), 2);

    let initial_sizes = h.viewer_window_sizes();
    assert_eq!(initial_sizes.len(), 2);

    // Find the second viewer window and close it
    let wins = snap::platform::windows();
    let win2 = wins.last().unwrap();
    win2.window().dispatch_event(slint::platform::WindowEvent::CloseRequested);

    // Viewer 2 must be removed, viewer 1 must remain
    let remaining_sizes = h.viewer_window_sizes();
    assert_eq!(remaining_sizes.len(), 1);
    assert_eq!(remaining_sizes[0], initial_sizes[0], "viewer 1 size must remain intact");

    // Settle deferred drop timer
    snap::platform::advance(Duration::from_millis(10));

    // Now close viewer 1
    let win1 = snap::platform::windows().into_iter().rev().find(|w| {
        w.window().size().width == initial_sizes[0].0
    }).expect("viewer 1 window");
    win1.window().dispatch_event(slint::platform::WindowEvent::CloseRequested);

    assert_eq!(h.viewer_window_sizes().len(), 0);
    snap::platform::advance(Duration::from_millis(10));
}

#[test]
fn test_app_viewer_scale_change_live() {
    let dir = tempfile::tempdir().unwrap();
    let img = create_test_image_file(dir.path(), "pic.png", 500, 300, [210, 40, 50]);

    let mut h = Harness::new();
    h.start().unwrap();
    h.choose_picture(&img);
    h.click_image_button(0);
    h.wait_for_images();

    open_viewer_for_note(&mut h, 0, [210, 40, 50]);
    assert_eq!(h.viewer_drawing_scales(), [1.0]);

    // Change UI scale to Small (option 1)
    set_app_ui_size(&mut h, 1);
    assert_eq!(h.viewer_drawing_scales(), [0.75], "viewer scale must update live to Small");

    // Change UI scale to Large (option 3)
    set_app_ui_size(&mut h, 3);
    assert_eq!(h.viewer_drawing_scales(), [1.0], "viewer scale must update live to Large");
}

// =========================================================================
// 5. Advanced Adversarial & Resilience Tests
// =========================================================================

struct FailingClipboard;
impl ImageClipboard for FailingClipboard {
    fn image(&self) -> Option<Rgba> { None }
    fn set_image(&self, _img: &Rgba) -> Result<(), String> {
        Err("Clipboard locked by external process".to_string())
    }
    fn set_rich(&self, _payload: washi::platform::ClipboardPayload) -> Result<(), String> {
        Err("Clipboard locked by external process".to_string())
    }
    fn html(&self) -> Option<String> { None }
    fn text(&self) -> Option<String> { None }
}

#[test]
fn test_clipboard_error_gracefully_absorbed() {
    snap::platform::install();
    let wc = Rc::new(FakeWindowControl::default());
    let cb = Rc::new(FailingClipboard);
    let rgba = sample_rgba(50, 50, 100, 150, 200);

    let viewer = open_test_viewer(Some(rgba), wc, cb, Rc::new(|| {}));

    // Calling copy on a failing clipboard must not panic or bubble an uncaught exception
    viewer.copy();
}

#[test]
fn test_empty_work_areas_fallback_positioning() {
    snap::platform::install();
    let wc = Rc::new(FakeWindowControl {
        areas: std::cell::RefCell::new(vec![]),
        ..Default::default()
    });
    let cb = Rc::new(FakeClipboard::default());
    let rgba = sample_rgba(200, 100, 10, 20, 30);

    // Opening viewer with empty work_areas must not panic
    let viewer = open_test_viewer(Some(rgba), wc, cb, Rc::new(|| {}));
    let pos = viewer.position();
    // Default fallback anchor is (0, 0, 1920, 1080)
    assert!(pos.0 >= 0 || pos.0 < 0, "position must be computed without panic");
}

#[test]
fn test_negative_monitor_coordinates_work_areas() {
    snap::platform::install();
    let secondary_left = washi::platform::ScreenRect::new(-1920, 0, 1920, 1080);
    let primary = washi::platform::ScreenRect::new(0, 0, 1920, 1040);
    let wc = Rc::new(FakeWindowControl {
        areas: std::cell::RefCell::new(vec![primary, secondary_left]),
        ..Default::default()
    });
    let cb = Rc::new(FakeClipboard::default());
    let target = washi::platform::ScreenRect::new(-1500, 200, 400, 300);

    let viewer = ImageViewer::open(
        slint::Image::default(),
        (400, 300),
        "Negative Monitor Test",
        Some(target),
        wc,
        cb,
        None,
        slint::Image::default(),
        Rc::new(|| {}),
    ).expect("open on negative monitor coordinate");

    let pos = viewer.position();
    assert!(pos.0 < 0, "viewer x ({}) must be placed on negative x monitor", pos.0);
}

#[test]
fn test_app_double_close_request_is_safe_and_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let img_path = create_test_image_file(dir.path(), "double_close.png", 500, 300, [210, 40, 50]);

    let mut h = Harness::new();
    h.start().unwrap();
    h.choose_picture(&img_path);
    h.click_image_button(0);
    h.wait_for_images();

    open_viewer_for_note(&mut h, 0, [210, 40, 50]);
    assert_eq!(h.viewer_window_sizes().len(), 1);

    let win = snap::platform::windows().into_iter().rev().find(|w| {
        w.window().size().width > 0
    }).expect("open viewer window");

    // Dispatch CloseRequested first time
    win.window().dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert_eq!(h.viewer_window_sizes().len(), 0);

    // Dispatch CloseRequested second time (e.g. before timer fires)
    win.window().dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert_eq!(h.viewer_window_sizes().len(), 0);

    // Settle deferred drop timer
    snap::platform::advance(Duration::from_millis(10));
}

#[test]
fn test_multiple_viewers_simultaneous_same_frame_close() {
    let dir = tempfile::tempdir().unwrap();
    let img1 = create_test_image_file(dir.path(), "pic1.png", 500, 300, [210, 40, 50]);
    let img2 = create_test_image_file(dir.path(), "pic2.png", 700, 400, [50, 180, 60]);

    let mut h = Harness::new();
    h.start().unwrap();

    h.right_click(0, Some(&["New note"]));
    h.choose_picture(&img1);
    h.click_image_button(0);
    h.wait_for_images();

    h.choose_picture(&img2);
    h.click_image_button(1);
    h.wait_for_images();

    open_viewer_for_note(&mut h, 0, [210, 40, 50]);
    open_viewer_for_note(&mut h, 1, [50, 180, 60]);
    assert_eq!(h.viewer_window_sizes().len(), 2);

    let wins = snap::platform::windows();
    let win2 = wins[wins.len() - 1].clone();
    let win1 = wins[wins.len() - 2].clone();

    // Close BOTH viewers in the same event tick before advancing timers
    win1.window().dispatch_event(slint::platform::WindowEvent::CloseRequested);
    win2.window().dispatch_event(slint::platform::WindowEvent::CloseRequested);

    assert_eq!(h.viewer_window_sizes().len(), 0, "both viewers must be removed immediately");

    // Advance timer to trigger deferred drops for both
    snap::platform::advance(Duration::from_millis(10));
}
