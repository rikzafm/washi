//! The Settings page.
//!
//! Pressing Settings in a note's dropdown opens one global Settings window at that note's place
//! (its Paper keeps its top-left; the window grows left by the Sidebar's strip) and hides only the
//! host note's desktop window. Closing puts the host back exactly where it was. Same pattern as
//! All notes: the Settings window is a separate window, the note stays alive underneath.

pub mod anim;

use crate::note_window::{app_icon, SettingsWindow};
use crate::platform::{NoteId, ScreenRect};
use crate::storage::Geometry;
use anim::{fit_on_screen, logical_areas, paper_of_note, window_bounds, RectF, PAPER_MIN_H, PAPER_W, SIDEBAR_STRIP};
use slint::ComponentHandle;
use std::cell::Cell;
use std::rc::{Rc, Weak};

/// What the view needs from the app.
#[derive(Clone)]
pub struct Hooks {
    /// Show or hide the host note's desktop window.
    pub desktop_visible: Rc<dyn Fn(NoteId, bool)>,
    /// Put the host note back at this window geometry (logical px).
    pub restore_host: Rc<dyn Fn(NoteId, Geometry)>,
    /// The view has closed: the app drops it.
    pub closed: Rc<dyn Fn()>,
    pub work_areas: Rc<dyn Fn() -> Vec<ScreenRect>>,
    pub begin_move: Rc<dyn Fn(Option<isize>)>,
}

pub struct SettingsView {
    ui: SettingsWindow,
    hooks: Hooks,
    host: NoteId,
    /// The host note's window geometry when Settings opened: what closing restores.
    host_window: Geometry,
    /// Where the Settings window was put (to tell a drag by the user from where we placed it).
    placed: Cell<Geometry>,
}

impl SettingsView {
    /// Open Settings over the note `host` (its window at `host_window`, logical px).
    pub fn open(host: NoteId, host_window: Geometry, pinned: bool, hooks: Hooks) -> Result<Rc<Self>, slint::PlatformError> {
        let ui = SettingsWindow::new()?;
        ui.set_app_icon(app_icon());
        ui.set_pinned(pinned);
        let view = Rc::new(SettingsView {
            ui,
            hooks,
            host,
            host_window,
            placed: Cell::new(host_window),
        });
        view.wire(Rc::downgrade(&view));
        let bounds = view.open_bounds();
        view.place(bounds);
        view.ui.show()?;
        (view.hooks.desktop_visible)(host, false);
        Ok(view)
    }

    /// Settings at rest: the Paper at the Settings size, anchored at the host Paper's top-left and
    /// kept on screen, with the Sidebar's strip out.
    fn open_bounds(&self) -> Geometry {
        let note = paper_of_note(self.host_window);
        let want = RectF { x: note.x, y: note.y, w: PAPER_W, h: PAPER_MIN_H };
        let areas = logical_areas(&(self.hooks.work_areas)(), self.ui.window().scale_factor());
        window_bounds(fit_on_screen(want, SIDEBAR_STRIP, &areas), SIDEBAR_STRIP)
    }

    fn place(&self, g: Geometry) {
        self.placed.set(g);
        let w = self.ui.window();
        w.set_size(slint::LogicalSize::new(g.w as f32, g.h as f32));
        w.set_position(slint::LogicalPosition::new(g.x as f32, g.y as f32));
    }

    fn wire(&self, me: Weak<SettingsView>) {
        {
            let me = me.clone();
            self.ui.on_close_clicked(move || {
                if let Some(v) = me.upgrade() {
                    v.close();
                }
            });
        }
        self.ui.on_start_move(move || {
            if let Some(v) = me.upgrade() {
                (v.hooks.begin_move)(v.hwnd());
            }
        });
    }

    /// X or Esc: put the host back and hide Settings.
    pub fn close(&self) {
        // (a host deleted or closed meanwhile is no longer a note: both hooks then do nothing)
        (self.hooks.restore_host)(self.host, self.host_after_moves());
        (self.hooks.desktop_visible)(self.host, true);
        let _ = self.ui.hide();
        (self.hooks.closed)();
    }

    /// If the user dragged the Settings page, the host comes back moved by the same amount. A
    /// shift made only to keep Settings on screen is undone.
    fn host_after_moves(&self) -> Geometry {
        let w = self.ui.window();
        let s = w.scale_factor();
        let at = w.position();
        let placed = self.placed.get();
        let (dx, dy) = ((at.x as f32 / s).round() as i32 - placed.x, (at.y as f32 / s).round() as i32 - placed.y);
        let g = self.host_window;
        Geometry { x: g.x + dx, y: g.y + dy, ..g }
    }

    pub fn host(&self) -> NoteId {
        self.host
    }

    pub fn ui(&self) -> &SettingsWindow {
        &self.ui
    }

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
