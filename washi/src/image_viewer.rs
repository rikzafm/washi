//! The picture viewer: one picture of a note in a resizable paper window of its own, opened by a
//! double click on the picture (see `ui/viewer.slint` for its look).
//!
//! The picture is the one the note stores (at most [`images::MAX_STORED`] px on a side), shown at
//! its own size when it fits the screen.
//!
//! [`images::MAX_STORED`]: crate::images::MAX_STORED

use crate::note_window::{CaptionPiece, ImageViewerWindow};
use crate::placement;
use crate::platform::{ImageClipboard, ResizeEdge, ScreenRect, WindowControl};
use crate::textflow;
use slint::ComponentHandle;
use std::rc::Rc;

// The chrome around the picture, logical px; the same numbers as `ViewerMetrics` and
// `PaperTheme.margin` in the .slint files.
const MARGIN: f32 = 42.0;
const PAD_X: f32 = 22.0;
const PAD_Y: f32 = 32.0 + 62.0 + 10.0;
const MIN_PAPER: (f32, f32) = (360.0, 280.0);

/// The window size (logical px) that shows a picture of `size` px at 100%, no larger than `max`
/// (the screen's work area, if known) and no smaller than the viewer's minimum.
pub fn window_size(size: (u32, u32), max: Option<(f32, f32)>) -> (f32, f32) {
    let w = (size.0 as f32 + 2.0 * PAD_X).max(MIN_PAPER.0) + 2.0 * MARGIN;
    let h = (size.1 as f32 + PAD_Y).max(MIN_PAPER.1) + 2.0 * MARGIN;
    match max {
        Some((mw, mh)) => (w.min((mw * 0.9).max(MIN_PAPER.0 + 2.0 * MARGIN)), h.min((mh * 0.9).max(MIN_PAPER.1 + 2.0 * MARGIN))),
        None => (w, h),
    }
}

// Font sizes of the caption lines (the same as in `viewer.slint`) and the longest piece of a line
// drawn on its own (within a fraction of a pixel of the tilted baseline, as the note's words).
const TITLE_SIZE: f64 = 33.0;
const INFO_SIZE: f64 = 20.0;
const MAX_PIECE: f64 = 16.0;

fn pieces(text: &str, size: f64) -> slint::ModelRc<CaptionPiece> {
    let v: Vec<CaptionPiece> = textflow::line_pieces(text, size, MAX_PIECE)
        .into_iter()
        .map(|(text, x, w)| CaptionPiece { text: text.as_str().into(), dx: x as f32, width: w as f32 })
        .collect();
    slint::ModelRc::from(Rc::new(slint::VecModel::from(v)))
}

/// The paper's tilt, degrees clockwise (`PaperTheme.paper-tilt`).
const TILT_DEG: f64 = 1.5;

fn hwnd_of(ui: &ImageViewerWindow) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let window_handle = ui.window().window_handle();
    let handle = window_handle.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
        _ => None,
    }
}

pub struct ImageViewer {
    ui: ImageViewerWindow,
    /// The window's scale: the monitor's times the UI size.
    scale: Rc<crate::ui_scale::WindowScale>,
}

impl ImageViewer {
    /// Create, position, and show the picture viewer window.
    pub fn open(
        picture: slint::Image,
        size: (u32, u32),
        caption: &str,
        target: Option<ScreenRect>,
        window_control: Rc<dyn WindowControl>,
        clipboard: Rc<dyn ImageClipboard>,
        rgba: Option<crate::images::Rgba>,
        icon: slint::Image,
        on_close: Rc<dyn Fn()>,
    ) -> Result<ImageViewer, slint::PlatformError> {
        let ui = ImageViewerWindow::new()?;
        // drawn at the UI size, like every window
        let scale = Rc::new(crate::ui_scale::WindowScale::attach(ui.window()));
        {
            // moved to a monitor with another DPI: put the UI size back on top
            let (weak, scale) = (ui.as_weak(), scale.clone());
            ui.on_scale_factor_changed(move || {
                if let Some(ui) = weak.upgrade() {
                    scale.resync(ui.window(), false);
                }
            });
        }
        // The software renderer cannot turn a picture, so it is turned here, once, by the paper's tilt.
        let shown = picture.to_rgba8().map(|buf| crate::note_images::turn(&buf, TILT_DEG));
        match shown {
            Some(t) => {
                ui.set_picture_rw(t.width() as i32);
                ui.set_picture_rh(t.height() as i32);
                ui.set_picture(slint::Image::from_rgba8(t));
            }
            None => {
                ui.set_picture_rw(size.0 as i32);
                ui.set_picture_rh(size.1 as i32);
                ui.set_picture(picture);
            }
        }
        ui.set_picture_w(size.0 as i32);
        ui.set_picture_h(size.1 as i32);
        ui.set_caption(caption.into());
        let info = format!("{} × {} · ", size.0, size.1);
        ui.set_title_pieces(pieces(caption, TITLE_SIZE));
        ui.set_info_pieces(pieces(&info, INFO_SIZE));
        ui.set_info_width(textflow::line_width(&info, INFO_SIZE) as f32);
        ui.set_app_icon(icon);

        // Window positioning via placement.rs
        let work_areas = window_control.work_areas();
        let target_wa = if let Some(target_rect) = target {
            let center = (target_rect.x + target_rect.w / 2, target_rect.y + target_rect.h / 2);
            placement::find_work_area(center, &work_areas).copied()
        } else {
            work_areas.first().copied()
        };

        let factor = scale.factor();
        let max_area = target_wa.map(|wa| (wa.w as f32 / factor, wa.h as f32 / factor));
        let (w, h) = window_size(size, max_area);
        ui.window().set_size(slint::LogicalSize::new(w, h));

        let phys_w = (w * factor).round() as i32;
        let phys_h = (h * factor).round() as i32;
        let target_anchor = target.or(target_wa).unwrap_or(ScreenRect::new(0, 0, 1920, 1080));
        let pos = placement::center_over(target_anchor, (phys_w, phys_h), &work_areas);
        ui.window().set_position(slint::PhysicalPosition::new(pos.x, pos.y));

        // Direct window dragging via platform window_control adapter
        {
            let wc = window_control.clone();
            let weak = ui.as_weak();
            ui.on_start_move(move || {
                if let Some(ui) = weak.upgrade() {
                    wc.begin_move(0, hwnd_of(&ui));
                }
            });
        }

        // Direct window resizing via platform window_control adapter
        {
            let wc = window_control.clone();
            let weak = ui.as_weak();
            ui.on_start_resize(move |code| {
                if let Some(edge) = ResizeEdge::from_code(code) {
                    if let Some(ui) = weak.upgrade() {
                        wc.begin_resize(0, hwnd_of(&ui), edge);
                    }
                }
            });
        }

        // Direct clipboard copy via platform clipboard adapter
        {
            let cb = clipboard.clone();
            let img = rgba.clone();
            ui.on_copy_picture(move || {
                if let Some(ref img) = img {
                    let _ = cb.set_image(img);
                }
            });
        }

        // Window closure handling
        {
            let on_close = on_close.clone();
            ui.on_close_viewer(move || on_close());
        }
        {
            let on_close = on_close.clone();
            ui.window().on_close_requested(move || {
                on_close();
                slint::CloseRequestResponse::HideWindow
            });
        }

        ui.show()?;
        scale.resync(ui.window(), true);
        Ok(ImageViewer { ui, scale })
    }

    /// Draw at UI size `ui` from now on, keeping the window's size on screen.
    pub fn set_ui_scale(&self, ui: f32) {
        self.scale.set_ui(self.ui.window(), ui);
    }

    /// How many screen px one logical px covers: the monitor's scale times the UI size.
    pub fn scale_factor(&self) -> f32 {
        self.ui.window().scale_factor()
    }

    /// The physical window size on screen.
    pub fn window_size(&self) -> (u32, u32) {
        let s = self.ui.window().size();
        (s.width, s.height)
    }

    /// The native window handle, once the window exists.
    pub fn hwnd(&self) -> Option<isize> {
        hwnd_of(&self.ui)
    }

    /// The underlying Slint component.
    pub fn ui(&self) -> &ImageViewerWindow {
        &self.ui
    }

    /// The physical window position on screen.
    pub fn position(&self) -> (i32, i32) {
        let pos = self.ui.window().position();
        (pos.x, pos.y)
    }

    /// Invoke close programmatically.
    pub fn close(&self) {
        self.ui.invoke_close_viewer();
    }

    /// Invoke copy programmatically.
    pub fn copy(&self) {
        self.ui.invoke_copy_picture();
    }
}

impl Drop for ImageViewer {
    fn drop(&mut self) {
        let _ = self.ui.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::NoteId;
    use std::cell::{Cell, RefCell};

    #[test]
    fn a_small_picture_gets_the_minimum_window() {
        assert_eq!(window_size((100, 80), None), (360.0 + 84.0, 280.0 + 84.0));
    }

    #[test]
    fn a_picture_fits_at_full_size_when_the_screen_allows() {
        assert_eq!(window_size((662, 400), None), (662.0 + 44.0 + 84.0, 400.0 + 104.0 + 84.0));
    }

    #[test]
    fn a_big_picture_is_held_to_nine_tenths_of_the_screen() {
        let (w, h) = window_size((662, 662), Some((800.0, 600.0)));
        assert_eq!((w, h), (720.0, 540.0));
    }

    #[derive(Default)]
    struct TestWindowControl {
        moved: Cell<bool>,
        resized: Cell<Option<ResizeEdge>>,
        areas: Vec<ScreenRect>,
    }

    impl WindowControl for TestWindowControl {
        fn begin_move(&self, _note: NoteId, _hwnd: Option<isize>) {
            self.moved.set(true);
        }
        fn begin_resize(&self, _note: NoteId, _hwnd: Option<isize>, edge: ResizeEdge) {
            self.resized.set(Some(edge));
        }
        fn work_areas(&self) -> Vec<ScreenRect> {
            self.areas.clone()
        }
        fn stacking_order(&self, notes: &[(NoteId, Option<isize>)]) -> Vec<NoteId> {
            notes.iter().map(|(id, _)| *id).collect()
        }
    }

    #[derive(Default)]
    struct TestClipboard {
        copied: RefCell<Option<crate::images::Rgba>>,
    }

    impl ImageClipboard for TestClipboard {
        fn image(&self) -> Option<crate::images::Rgba> {
            self.copied.borrow().clone()
        }
        fn set_image(&self, img: &crate::images::Rgba) -> Result<(), String> {
            *self.copied.borrow_mut() = Some(img.clone());
            Ok(())
        }
        fn set_rich(&self, payload: crate::platform::ClipboardPayload) -> Result<(), String> {
            *self.copied.borrow_mut() = payload.image;
            Ok(())
        }
        fn html(&self) -> Option<String> { None }
        fn text(&self) -> Option<String> { None }
    }

    fn sample_rgba(w: u32, h: u32) -> crate::images::Rgba {
        crate::images::Rgba {
            width: w,
            height: h,
            data: vec![255; (w * h * 4) as usize],
        }
    }

    #[test]
    fn image_viewer_opens_and_centers_over_target() {
        crate::popup::init_test_platform();
        let wc = Rc::new(TestWindowControl {
            areas: vec![ScreenRect::new(0, 0, 1920, 1040)],
            ..Default::default()
        });
        let cb = Rc::new(TestClipboard::default());
        let rgba = sample_rgba(200, 150);
        let pic = slint::Image::default();
        let target = ScreenRect::new(500, 400, 400, 300);

        let viewer = ImageViewer::open(
            pic,
            (200, 150),
            "Test Caption",
            Some(target),
            wc.clone(),
            cb.clone(),
            Some(rgba),
            slint::Image::default(),
            Rc::new(|| {}),
        ).expect("ImageViewer::open");

        let (w, h) = window_size((200, 150), Some((1920.0, 1040.0)));
        let expected = placement::center_over(target, (w.round() as i32, h.round() as i32), &wc.work_areas());
        assert_eq!(viewer.position(), (expected.x, expected.y));
    }

    #[test]
    fn image_viewer_centers_on_primary_work_area_when_target_is_none() {
        crate::popup::init_test_platform();
        let wa = ScreenRect::new(0, 0, 1920, 1040);
        let wc = Rc::new(TestWindowControl {
            areas: vec![wa],
            ..Default::default()
        });
        let cb = Rc::new(TestClipboard::default());
        let pic = slint::Image::default();

        let viewer = ImageViewer::open(
            pic,
            (200, 150),
            "No Target",
            None,
            wc.clone(),
            cb.clone(),
            None,
            slint::Image::default(),
            Rc::new(|| {}),
        ).expect("ImageViewer::open");

        let (w, h) = window_size((200, 150), Some((1920.0, 1040.0)));
        let expected = placement::center_over(wa, (w.round() as i32, h.round() as i32), &wc.work_areas());
        assert_eq!(viewer.position(), (expected.x, expected.y));
    }

    #[test]
    fn image_viewer_target_on_secondary_monitor_places_on_secondary_monitor() {
        crate::popup::init_test_platform();
        let primary = ScreenRect::new(0, 0, 800, 600);
        let secondary = ScreenRect::new(1920, 0, 2560, 1440);
        let wc = Rc::new(TestWindowControl {
            areas: vec![primary, secondary],
            ..Default::default()
        });
        let cb = Rc::new(TestClipboard::default());
        let pic = slint::Image::default();
        let target = ScreenRect::new(2200, 300, 400, 300);

        let viewer = ImageViewer::open(
            pic,
            (1200, 900),
            "Secondary Monitor",
            Some(target),
            wc.clone(),
            cb.clone(),
            None,
            slint::Image::default(),
            Rc::new(|| {}),
        ).expect("ImageViewer::open");

        assert!(viewer.position().0 >= 1920, "viewer x ({}) must be on secondary monitor", viewer.position().0);
    }

    #[test]
    fn image_viewer_copy_invokes_clipboard() {
        crate::popup::init_test_platform();
        let wc = Rc::new(TestWindowControl {
            areas: vec![ScreenRect::new(0, 0, 1920, 1040)],
            ..Default::default()
        });
        let cb = Rc::new(TestClipboard::default());
        let rgba = sample_rgba(100, 100);

        let viewer = ImageViewer::open(
            slint::Image::default(),
            (100, 100),
            "Copy Test",
            None,
            wc,
            cb.clone(),
            Some(rgba.clone()),
            slint::Image::default(),
            Rc::new(|| {}),
        ).expect("ImageViewer::open");

        assert!(cb.copied.borrow().is_none());
        viewer.copy();
        assert_eq!(cb.copied.borrow().as_ref(), Some(&rgba));
    }

    #[test]
    fn image_viewer_close_invokes_on_close_callback() {
        crate::popup::init_test_platform();
        let wc = Rc::new(TestWindowControl {
            areas: vec![ScreenRect::new(0, 0, 1920, 1040)],
            ..Default::default()
        });
        let cb = Rc::new(TestClipboard::default());
        let closed = Rc::new(Cell::new(false));
        let closed_clone = closed.clone();

        let viewer = ImageViewer::open(
            slint::Image::default(),
            (100, 100),
            "Close Test",
            None,
            wc,
            cb,
            None,
            slint::Image::default(),
            Rc::new(move || closed_clone.set(true)),
        ).expect("ImageViewer::open");

        assert!(!closed.get());
        viewer.close();
        assert!(closed.get());
    }

    #[test]
    fn image_viewer_drag_and_resize_invokes_window_control() {
        crate::popup::init_test_platform();
        let wc = Rc::new(TestWindowControl {
            areas: vec![ScreenRect::new(0, 0, 1920, 1040)],
            ..Default::default()
        });
        let cb = Rc::new(TestClipboard::default());

        let viewer = ImageViewer::open(
            slint::Image::default(),
            (100, 100),
            "Drag and Resize Test",
            None,
            wc.clone(),
            cb,
            None,
            slint::Image::default(),
            Rc::new(|| {}),
        ).expect("ImageViewer::open");

        assert!(!wc.moved.get());
        viewer.ui().invoke_start_move();
        assert!(wc.moved.get());

        assert_eq!(wc.resized.get(), None);
        viewer.ui().invoke_start_resize(3); // 3 = SouthEast
        assert_eq!(wc.resized.get(), Some(ResizeEdge::SouthEast));
    }
}
