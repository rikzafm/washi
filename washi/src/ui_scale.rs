//! UI size (Settings › Appearance): every window Washi draws (Notes, the All notes Sidebar in its
//! Note, Settings, the Dropdown menu, the delete dialog, the picture viewer) is drawn at the
//! platform's own scale (the monitor's DPI) times the UI size. When a window moves to a monitor with
//! another DPI the platform puts its own scale back on the window, and the window's owner puts the
//! UI size back on top ([`WindowScale::resync`]): movable windows (Notes, Settings, the delete dialog,
//! the picture viewer) when their `scale-factor` property changes (and Notes when a move is reported). The scale goes in as the Slint window's scale factor, so every
//! logical length shrinks evenly (text, chrome, margins, hit-testing) and stacks with the per-Note
//! text size, while the .slint files and the layout code keep working in logical px.
//!
//! Logical px therefore depend on the UI size. What has to mean the same place on screen whatever the
//! UI size is converted where it is kept: the database stores note geometry at Large (see
//! `Db::save_geometry`), and whatever holds a geometry in memory across a change rescales it by the
//! factor [`App::set_ui_scale`](crate::app::App) passes on.

use slint::platform::WindowEvent;
use std::cell::Cell;

thread_local! {
    /// The UI size applied app-wide now (1.0 until the app starts).
    static APP_SCALE: Cell<f32> = const { Cell::new(1.0) };
}

/// The UI size every window is drawn at (new windows pick it up when they attach).
pub fn app_scale() -> f32 {
    APP_SCALE.with(|s| s.get())
}

pub fn set_app_scale(scale: f32) {
    APP_SCALE.with(|s| s.set(scale));
}

const EPS: f32 = 1e-4;

/// One window's scale: the platform's own (`os`) and the UI size on top of it.
#[derive(Debug)]
pub struct WindowScale {
    os: Cell<f32>,
    ui: Cell<f32>,
}

impl WindowScale {
    /// A new window, before it is sized, placed or shown: draw it at the app's UI size.
    pub fn attach(w: &slint::Window) -> WindowScale {
        let s = WindowScale { os: Cell::new(w.scale_factor()), ui: Cell::new(app_scale()) };
        s.dispatch(w);
        s
    }

    /// The scale factor the window should have.
    pub fn factor(&self) -> f32 {
        self.os.get() * self.ui.get()
    }

    /// The UI size this window is drawn at.
    pub fn ui(&self) -> f32 {
        self.ui.get()
    }

    fn dispatch(&self, w: &slint::Window) {
        if (w.scale_factor() - self.factor()).abs() > EPS {
            w.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: self.factor() });
        }
    }

    /// After `show()`, or after the platform may have changed the window's scale itself (winit sets
    /// the monitor's scale when it creates the native window and when the window moves to a monitor
    /// with another DPI): take that as the platform's scale and put the UI size back on top.
    /// `keep_logical` keeps the window's logical size and place (a window that has just shown);
    /// otherwise its size on screen is kept. Nothing happens while the scale is still ours.
    /// Returns whether the platform's scale was new.
    pub fn resync(&self, w: &slint::Window, keep_logical: bool) -> bool {
        let now = w.scale_factor();
        if (now - self.factor()).abs() <= EPS {
            return false;
        }
        let old_factor = self.factor();
        let size = w.size().to_logical(old_factor);
        self.os.set(now);
        if keep_logical {
            let phys_pos = w.position();
            self.dispatch(w);
            w.set_size(size);
            w.set_position(slint::WindowPosition::Physical(phys_pos));
            return true;
        }
        if (now - self.factor()).abs() <= EPS {
            return true; // at Large the platform's scale is already right
        }
        self.set_ui(w, self.ui.get());
        true
    }

    /// Draw the window at UI size `ui` from now on, keeping its size on screen: what it shows reflows
    /// into the new logical size.
    pub fn set_ui(&self, w: &slint::Window, ui: f32) {
        self.ui.set(ui);
        let physical = w.size();
        if (w.scale_factor() - self.factor()).abs() <= EPS {
            return;
        }
        self.dispatch(w);
        if physical.width > 0 && physical.height > 0 {
            w.dispatch_event(WindowEvent::Resized { size: physical.to_logical(self.factor()) });
        }
    }
}
