//! Settings › Appearance: app-wide look-and-feel preferences, kept in the database `meta` table.
//!
//! The tab only shows what is stored and reports choices: every value is read back from the database
//! after a write, so a failed write shows the truth. Writes are best effort: a failure is logged and
//! never blocks the UI.

use crate::note_window::SettingsWindow;
use crate::storage::db::{Choice, SettingsHeight, SharedSwitch, Sidebar, SidebarAnchor, SidebarHeight, SidebarLayout};
use crate::storage::{Db, DbError, SidebarTheme};
use slint::ComponentHandle;
use std::rc::Rc;

pub use crate::storage::db::UI_SCALES;

/// The dropdown index of a stored scale (Large for anything that is not one of the choices).
pub fn ui_size_index(scale: f32) -> i32 {
    UI_SCALES.iter().position(|s| (s - scale).abs() < 1e-3).unwrap_or(UI_SCALES.len() - 1) as i32
}

fn logged(what: &str, r: Result<(), DbError>) {
    if let Err(e) = r {
        eprintln!("washi: could not save the {what}: {e}");
    }
}

/// A value each Sidebar has, under a "Same for both sidebars" switch: the Theme, or the Height and
/// Anchor. Both Sidebars' values are stored even while they are shared.
struct PerSidebar<T> {
    /// What it is called in a log line.
    what: &'static str,
    switch: SharedSwitch,
    get: fn(&Db, Sidebar) -> T,
    set: fn(&Db, Sidebar, T) -> Result<(), DbError>,
}

const THEME: PerSidebar<SidebarTheme> =
    PerSidebar { what: "sidebar theme", switch: SharedSwitch::Theme, get: Db::sidebar_theme, set: Db::set_sidebar_theme };

const LAYOUT: PerSidebar<SidebarLayout> =
    PerSidebar { what: "sidebar layout", switch: SharedSwitch::Layout, get: Db::sidebar_layout, set: Db::set_sidebar_layout };

impl<T: Copy> PerSidebar<T> {
    /// Turning the switch on gives both Sidebars the Settings sidebar's value; turning it off
    /// leaves each with the value it had (the shared one), so nothing changes on screen.
    fn set_shared(&self, db: &Db, shared: bool) {
        if shared {
            logged(self.what, (self.set)(db, Sidebar::Notes, (self.get)(db, Sidebar::Settings)));
        }
        logged(&format!("{} switch", self.what), db.set_sidebars_shared(self.switch, shared));
    }

    /// Change Sidebar `s`'s value: both Sidebars' while the switch is on.
    fn change(&self, db: &Db, s: Sidebar, change: impl FnOnce(T) -> T) {
        let value = change((self.get)(db, s));
        let targets = if db.sidebars_shared(self.switch) { Sidebar::ALL } else { std::slice::from_ref(&s) };
        for &t in targets {
            logged(self.what, (self.set)(db, t, value));
        }
    }
}

pub struct Appearance {
    ui: slint::Weak<SettingsWindow>,
    db: Rc<Db>,
    /// Tell the open All notes that the Sidebar layout changed.
    layout_changed: Rc<dyn Fn()>,
    /// A Sidebar Theme changed: the app redraws any open All notes Sidebar.
    themes_changed: Rc<dyn Fn()>,
    /// Redraw every window at a new UI size (the app's `set_ui_scale`).
    ui_scale_changed: Rc<dyn Fn(f32)>,
}

impl Appearance {
    pub fn new(
        ui: &SettingsWindow,
        db: Rc<Db>,
        layout_changed: Rc<dyn Fn()>,
        themes_changed: Rc<dyn Fn()>,
        ui_scale_changed: Rc<dyn Fn(f32)>,
    ) -> Rc<Appearance> {
        let appearance =
            Rc::new(Appearance { ui: ui.as_weak(), db, layout_changed, themes_changed, ui_scale_changed });
        let me = Rc::downgrade(&appearance);
        ui.on_set_ui_size(move |i| {
            if let Some(a) = me.upgrade() {
                a.set_ui_size(i);
            }
        });
        let me = Rc::downgrade(&appearance);
        ui.on_set_theme_shared(move |shared| {
            if let Some(a) = me.upgrade() {
                THEME.set_shared(&a.db, shared);
                a.themes_refreshed();
            }
        });
        let me = Rc::downgrade(&appearance);
        ui.on_set_theme(move |s, theme| {
            if let (Some(a), Some(s), Some(theme)) = (me.upgrade(), Sidebar::from_index(s), SidebarTheme::from_index(theme)) {
                THEME.change(&a.db, s, |_| theme);
                a.themes_refreshed();
            }
        });
        let me = Rc::downgrade(&appearance);
        ui.on_set_sidebar_layout_shared(move |shared| {
            if let Some(a) = me.upgrade() {
                LAYOUT.set_shared(&a.db, shared);
                a.layout_refreshed();
            }
        });
        let me = Rc::downgrade(&appearance);
        ui.on_set_sidebar_height(move |s, i| {
            if let (Some(a), Some(s), Some(height)) = (me.upgrade(), Sidebar::from_index(s), SidebarHeight::from_index(i)) {
                LAYOUT.change(&a.db, s, |l| SidebarLayout { height, ..l });
                a.layout_refreshed();
            }
        });
        let me = Rc::downgrade(&appearance);
        ui.on_set_sidebar_anchor(move |s, i| {
            if let (Some(a), Some(s), Some(anchor)) = (me.upgrade(), Sidebar::from_index(s), SidebarAnchor::from_index(i)) {
                LAYOUT.change(&a.db, s, |l| SidebarLayout { anchor, ..l });
                a.layout_refreshed();
            }
        });
        let me = Rc::downgrade(&appearance);
        ui.on_set_settings_height(move |i| {
            if let (Some(a), Some(h)) = (me.upgrade(), SettingsHeight::from_index(i)) {
                // the Paper's cap changes at once: the window binds it, and a new height refits the page
                logged("Settings height", a.db.set_settings_height(h));
                a.refresh();
            }
        });
        appearance.refresh();
        appearance
    }

    fn set_ui_size(&self, i: i32) {
        let Some(&scale) = UI_SCALES.get(i as usize) else { return };
        logged("UI size", self.db.set_ui_scale(scale));
        self.refresh();
        // what is stored is what is shown, even if the write failed
        (self.ui_scale_changed)(self.db.ui_scale());
    }

    fn themes_refreshed(&self) {
        self.refresh();
        (self.themes_changed)();
    }

    fn layout_refreshed(&self) {
        self.refresh();
        (self.layout_changed)();
    }

    /// Show what is stored.
    fn refresh(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let db = &self.db;
        ui.set_ui_size(ui_size_index(db.ui_scale()));
        ui.set_theme_shared(db.sidebars_shared(SharedSwitch::Theme));
        ui.set_theme_settings(db.sidebar_theme(Sidebar::Settings).index());
        ui.set_theme_notes(db.sidebar_theme(Sidebar::Notes).index());
        ui.set_sidebar_layout_shared(db.sidebars_shared(SharedSwitch::Layout));
        let settings = db.sidebar_layout(Sidebar::Settings);
        let notes = db.sidebar_layout(Sidebar::Notes);
        ui.set_settings_sidebar_height(settings.height.index());
        ui.set_settings_sidebar_anchor(settings.anchor.index());
        ui.set_notes_sidebar_height(notes.height.index());
        ui.set_notes_sidebar_anchor(notes.anchor.index());
        ui.set_settings_height(db.settings_height().index());
    }
}
