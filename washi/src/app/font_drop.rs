//! Files dragged onto the Settings window: the thin end that turns the window system's drag events
//! into the Font manager's install (the decisions are `settings::font_drop`).

use super::App;
use crate::settings::font_drop::{react, FileDrag};
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use std::rc::Rc;

impl App {
    /// A file was held over, taken away from, or dropped on the Settings window. Counts only while
    /// the Font manager is open; a dropped file is installed exactly as the picker's choice is.
    pub fn settings_file_drag(&self, drag: FileDrag) {
        let Some(settings) = self.settings() else { return };
        let reaction = react(settings.is_manager_open(), drag);
        settings.ui().set_manager_install_hover(reaction.hover);
        if let Some(path) = reaction.install {
            self.install_font_file(path);
        }
    }

    /// Listen for dropped files on the Settings window (the real window only: ADR 0003, S3).
    pub(super) fn watch_settings_drops(&self, window: &slint::Window) {
        let weak = Rc::downgrade(&self.0);
        window.on_winit_window_event(move |_, event| {
            use winit::event::WindowEvent;
            let drag = match event {
                WindowEvent::HoveredFile(_) => FileDrag::Hover,
                WindowEvent::HoveredFileCancelled => FileDrag::Cancel,
                WindowEvent::DroppedFile(path) => FileDrag::Drop(path.clone()),
                _ => return EventResult::Propagate,
            };
            if let Some(inner) = weak.upgrade() {
                App(inner).settings_file_drag(drag);
            }
            EventResult::Propagate
        });
    }
}
