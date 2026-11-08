//! The picture viewer: one picture of a note in a resizable paper window of its own, opened by a
//! double click on the picture (see `ui/viewer.slint` for its look).
//!
//! The picture is the one the note stores (at most [`images::MAX_STORED`] px on a side), shown at
//! its own size when it fits the screen.
//!
//! [`images::MAX_STORED`]: crate::images::MAX_STORED

use crate::note_window::{CaptionPiece, ImageViewerWindow};
use crate::textflow;
use crate::platform::ResizeEdge;
use slint::ComponentHandle;
use std::rc::Rc;

/// What a viewer window asks the app to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewerEvent {
    /// The close button, Esc, or the window itself was closed.
    Close,
    StartMove,
    StartResize(ResizeEdge),
    /// Ctrl+C was pressed to copy the picture.
    Copy,
}

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

pub struct ImageViewer {
    ui: ImageViewerWindow,
}

impl ImageViewer {
    /// Create and show the window. `max` is the screen's work area in physical px.
    pub fn open(
        picture: slint::Image,
        size: (u32, u32),
        caption: &str,
        max: Option<(f32, f32)>,
        icon: slint::Image,
        on_event: Rc<dyn Fn(ViewerEvent)>,
    ) -> Result<ImageViewer, slint::PlatformError> {
        let ui = ImageViewerWindow::new()?;
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
        let scale = ui.window().scale_factor();
        let (w, h) = window_size(size, max.map(|(mw, mh)| (mw / scale, mh / scale)));
        ui.window().set_size(slint::LogicalSize::new(w, h));
        {
            let on_event = on_event.clone();
            ui.on_close_viewer(move || on_event(ViewerEvent::Close));
        }
        {
            let on_event = on_event.clone();
            ui.on_start_move(move || on_event(ViewerEvent::StartMove));
        }
        {
            let on_event = on_event.clone();
            ui.on_start_resize(move |code| {
                if let Some(edge) = ResizeEdge::from_code(code) {
                    on_event(ViewerEvent::StartResize(edge));
                }
            });
        }
        {
            let on_event = on_event.clone();
            ui.on_copy_picture(move || on_event(ViewerEvent::Copy));
        }
        {
            let on_event = on_event.clone();
            ui.window().on_close_requested(move || {
                on_event(ViewerEvent::Close);
                slint::CloseRequestResponse::HideWindow
            });
        }
        ui.show()?;
        Ok(ImageViewer { ui })
    }

    /// The native window handle, once the window exists.
    pub fn hwnd(&self) -> Option<isize> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let window_handle = self.ui.window().window_handle();
        let handle = window_handle.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
            _ => None,
        }
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
}
