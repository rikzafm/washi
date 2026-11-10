//! Settings › Appearance: app-wide look-and-feel preferences, kept in the database `meta` table.
//!
//! The tab only shows what is stored and reports choices: every value is read back from the database
//! after a write, so a failed write shows the truth. Writes are best effort: a failure is logged and
//! never blocks the UI.

use crate::note_window::SettingsWindow;
use crate::storage::db::{Sidebar, SidebarAnchor, SidebarHeight, SidebarLayout};
use crate::storage::{Db, DbError};
use slint::ComponentHandle;
use std::rc::Rc;

pub use crate::storage::db::UI_SCALES;

/// The dropdown index of a stored scale (Large for anything that is not one of the choices).
pub fn ui_size_index(scale: f32) -> i32 {
    UI_SCALES.iter().position(|s| (s - scale).abs() < 1e-3).unwrap_or(UI_SCALES.len() - 1) as i32
}

/// A Height as the Slint side numbers it (0 Match note, 1 Fit content).
pub fn height_index(h: SidebarHeight) -> i32 {
    SidebarHeight::ALL.iter().position(|&x| x == h).unwrap_or(0) as i32
}

/// An Anchor as the Slint side numbers it (0 Top, 1 Middle, 2 Bottom).
pub fn anchor_index(a: SidebarAnchor) -> i32 {
    SidebarAnchor::ALL.iter().position(|&x| x == a).unwrap_or(1) as i32
}

/// The Sidebar the Appearance Tab means by `i` (0 Settings, 1 Notes).
fn sidebar(i: i32) -> Option<Sidebar> {
    match i {
        0 => Some(Sidebar::Settings),
        1 => Some(Sidebar::Notes),
        _ => None,
    }
}

fn logged(what: &str, r: Result<(), DbError>) {
    if let Err(e) = r {
        eprintln!("washi: could not save the {what}: {e}");
    }
}

pub struct Appearance {
    ui: slint::Weak<SettingsWindow>,
    db: Rc<Db>,
    /// Tell the open All notes that the Sidebar layout changed.
    layout_changed: Rc<dyn Fn()>,
}

impl Appearance {
    pub fn new(ui: &SettingsWindow, db: Rc<Db>, layout_changed: Rc<dyn Fn()>) -> Rc<Appearance> {
        let appearance = Rc::new(Appearance { ui: ui.as_weak(), db, layout_changed });
        let me = Rc::downgrade(&appearance);
        ui.on_set_ui_size(move |i| {
            if let Some(a) = me.upgrade() {
                a.set_ui_size(i);
            }
        });
        let me = Rc::downgrade(&appearance);
        ui.on_toggle_sidebar_layout_shared(move || {
            if let Some(a) = me.upgrade() {
                a.toggle_layout_shared();
            }
        });
        let me = Rc::downgrade(&appearance);
        ui.on_set_sidebar_height(move |s, i| {
            if let (Some(a), Some(&h)) = (me.upgrade(), SidebarHeight::ALL.get(i as usize)) {
                a.change_layout(s, |l| l.height = h);
            }
        });
        let me = Rc::downgrade(&appearance);
        ui.on_set_sidebar_anchor(move |s, i| {
            if let (Some(a), Some(&anchor)) = (me.upgrade(), SidebarAnchor::ALL.get(i as usize)) {
                a.change_layout(s, |l| l.anchor = anchor);
            }
        });
        appearance.refresh();
        appearance
    }

    fn set_ui_size(&self, i: i32) {
        let Some(&scale) = UI_SCALES.get(i as usize) else { return };
        logged("UI size", self.db.set_ui_scale(scale));
        self.refresh();
    }

    /// "Same for both sidebars": turning it off leaves both Sidebars on the values they share;
    /// turning it on gives both the Settings sidebar's values.
    fn toggle_layout_shared(&self) {
        let shared = !self.db.sidebar_layout_shared();
        if shared {
            let settings = self.db.sidebar_layout(Sidebar::Settings);
            logged("Notes sidebar layout", self.db.set_sidebar_layout(Sidebar::Notes, settings));
        }
        logged("sidebar layout switch", self.db.set_sidebar_layout_shared(shared));
        self.layout_refreshed();
    }

    /// Change sidebar `s`'s Height or Anchor: both Sidebars' while they are shared.
    fn change_layout(&self, s: i32, change: impl Fn(&mut SidebarLayout)) {
        let Some(s) = sidebar(s) else { return };
        let targets: &[Sidebar] =
            if self.db.sidebar_layout_shared() { &[Sidebar::Settings, Sidebar::Notes] } else { std::slice::from_ref(&s) };
        let mut layout = self.db.sidebar_layout(s);
        change(&mut layout);
        for &t in targets {
            logged("sidebar layout", self.db.set_sidebar_layout(t, layout));
        }
        self.layout_refreshed();
    }

    fn layout_refreshed(&self) {
        self.refresh();
        (self.layout_changed)();
    }

    /// Show what is stored.
    fn refresh(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        ui.set_ui_size(ui_size_index(self.db.ui_scale()));
        ui.set_sidebar_layout_shared(self.db.sidebar_layout_shared());
        let settings = self.db.sidebar_layout(Sidebar::Settings);
        let notes = self.db.sidebar_layout(Sidebar::Notes);
        ui.set_settings_sidebar_height(height_index(settings.height));
        ui.set_settings_sidebar_anchor(anchor_index(settings.anchor));
        ui.set_notes_sidebar_height(height_index(notes.height));
        ui.set_notes_sidebar_anchor(anchor_index(notes.anchor));
    }
}
