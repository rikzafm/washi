//! The Settings page, and applying what it changes to every open window.

use super::App;
use crate::note_window::Note;
use crate::platform::NoteId;
use crate::settings::SettingsView;
use crate::storage::db::Choice;
use slint::ComponentHandle;
use std::rc::Rc;

impl App {
    /// Clone the handle to the open Settings page, if any.
    pub fn settings(&self) -> Option<Rc<SettingsView>> {
        self.0.settings.borrow().clone()
    }

    pub fn is_settings_open(&self) -> bool {
        self.0.settings.borrow().is_some()
    }

    /// Send a window event to the Settings page (for testing).
    pub fn dispatch_settings_event(&self, event: slint::platform::WindowEvent) {
        if let Some(s) = self.settings() {
            s.ui().window().dispatch_event(event);
        }
    }

    /// Settings from note `host`'s dropdown: one Settings page at a time, so pressing it again
    /// (from any note) brings the existing page to the front.
    pub fn open_settings(&self, host: NoteId) {
        if let Some(s) = self.settings() {
            // (still fading the host's text out: nothing to bring forward yet)
            if s.is_shown() {
                self.0.env.window_control.bring_to_front(0, s.hwnd());
                return;
            }
            *self.0.settings.borrow_mut() = None;
        }
        let Some((geometry, pinned)) = self.with_note(host, |n| {
            n.flush();
            (n.geometry(), n.always_on_top())
        }) else {
            return;
        };
        let weak = Rc::downgrade(&self.0);
        let app = move || weak.upgrade().map(App);
        let hooks = crate::settings::Hooks {
            desktop_visible: Rc::new({
                let app = app.clone();
                move |id, visible| {
                    let Some(a) = app() else { return };
                    // an All notes editor stands in for the note it shows: never show it twice
                    let show = visible && !a.all_notes().is_some_and(|v| v.editor_note_id() == id);
                    a.with_note(id, |n| n.set_visible(show));
                }
            }),
            restore_host: Rc::new({
                let app = app.clone();
                move |id, g| {
                    let Some(a) = app() else { return };
                    if a.with_note(id, |n| n.place(g)).is_some() {
                        let hwnd = a.with_note(id, |n| n.hwnd()).flatten();
                        a.0.env.window_control.bring_to_front(id, hwnd);
                    }
                }
            }),
            host_content: Rc::new({
                let app = app.clone();
                move |id, o| {
                    if let Some(a) = app() {
                        a.with_note(id, |n| n.set_content_opacity(o));
                    }
                }
            }),
            now: Rc::new({
                let clock = self.0.env.clock.clone();
                move || clock.monotonic()
            }),
            set_bounds: Rc::new({
                let app = app.clone();
                move |hwnd, x, y, w, h| app().is_some_and(|a| a.0.env.window_control.set_bounds(hwnd, x, y, w, h))
            }),
            fine_timer: Rc::new({
                let app = app.clone();
                move |on| {
                    if let Some(a) = app() {
                        a.0.env.window_control.fine_timer(on);
                    }
                }
            }),
            general: crate::settings::general::GeneralDeps {
                env: self.0.env.clone(),
                db: self.0.db.clone(),
                open_note_count: Rc::new({
                    let app = app.clone();
                    move || app().map_or(0, |a| a.note_ids().len())
                }),
                quit_for_uninstall: Rc::new({
                    let app = app.clone();
                    move || {
                        if let Some(a) = app() {
                            a.quit_for_uninstall();
                        }
                    }
                }),
                tilted_editing_changed: Rc::new({
                    let app = app.clone();
                    move |on| {
                        if let Some(a) = app() {
                            a.set_tilted_editing(on);
                        }
                    }
                }),
            },
            closed: Rc::new({
                let app = app.clone();
                move || {
                    if let Some(a) = app() {
                        a.settings_closed();
                    }
                }
            }),
            sidebar_layout_changed: Rc::new({
                let app = app.clone();
                move || {
                    if let Some(a) = app() {
                        a.apply_notes_sidebar_layout();
                    }
                }
            }),
            sidebar_themes_changed: Rc::new({
                let app = app.clone();
                move || {
                    let Some(a) = app() else { return };
                    if let Some(v) = a.all_notes() {
                        v.ui().set_sidebar_theme(a.0.db.sidebar_theme(crate::storage::db::Sidebar::Notes).index());
                    }
                }
            }),
            work_areas: Rc::new({
                let app = app.clone();
                move || app().map(|a| a.0.env.window_control.work_areas()).unwrap_or_default()
            }),
            begin_move: Rc::new({
                let app = app.clone();
                move |hwnd| {
                    if let Some(a) = app() {
                        a.0.env.window_control.begin_move(0, hwnd);
                    }
                }
            }),
            ui_scale_changed: Rc::new({
                let app = app.clone();
                move |scale| {
                    if let Some(a) = app() {
                        a.set_ui_scale(scale);
                    }
                }
            }),
            font_size_changed: Rc::new({
                let app = app.clone();
                move |size| {
                    if let Some(a) = app() {
                        a.set_font_size(size);
                    }
                }
            }),
            font_picked: Rc::new({
                let app = app.clone();
                move |row| {
                    if let Some(a) = app() {
                        a.pick_font(row);
                    }
                }
            }),
            font_removed: Rc::new({
                let app = app.clone();
                move |row| {
                    if let Some(a) = app() {
                        a.remove_font(row);
                    }
                }
            }),
            font_install: Rc::new({
                let app = app.clone();
                move || {
                    if let Some(a) = app() {
                        a.pick_font_to_install();
                    }
                }
            }),
            letter_spacing_changed: Rc::new({
                let app = app.clone();
                move |em| {
                    if let Some(a) = app() {
                        a.set_letter_spacing(em);
                    }
                }
            }),
        };
        match SettingsView::open(host, geometry, pinned, hooks) {
            Ok(view) => {
                self.0.env.window_control.bring_to_front(0, view.hwnd());
                self.watch_settings_drops(view.ui().window());
                *self.0.settings.borrow_mut() = Some(view);
                // the Font row and the Font manager's list show the Font in use and the installed fonts
                self.refresh_font_list(None);
            }
            Err(e) => eprintln!("washi: could not open Settings: {e}"),
        }
    }

    /// Draw everything at UI size `scale` from now on (Settings › Appearance; the value is already
    /// saved). Open notes keep their windows' size on screen and their content reflows; the open
    /// Settings page and All notes Sidebar redraw at once. The Dropdown menu and the delete dialog
    /// close (they reopen at the new size). Whatever is held in logical px is carried over by
    /// `k` = old / new, so it stays where it was on screen.
    pub fn set_ui_scale(&self, scale: f32) {
        let old = crate::ui_scale::app_scale();
        if (old - scale).abs() < 1e-4 {
            return;
        }
        crate::ui_scale::set_app_scale(scale);
        let k = old / scale;
        let closed = self.0.popups.dismiss_for_ui_scale();
        self.dropdown_closed(closed);
        let notes: Vec<Rc<Note>> = self.0.notes.borrow().clone();
        for n in &notes {
            n.set_ui_scale(scale, k);
        }
        if let Some(v) = self.all_notes() {
            v.set_ui_scale(scale, k);
        }
        self.0.exit.borrow_mut().scale_geometry(k);
        if let Some(s) = self.settings() {
            s.set_ui_scale(scale, k);
        }
        for v in self.0.viewers.borrow().iter() {
            v.set_ui_scale(scale);
        }
    }

    /// The text of every note stays tilted while it is edited (true) or settles flat (false), from
    /// now on (Settings › General; the value is already saved).
    pub fn set_tilted_editing(&self, on: bool) {
        let notes: Vec<Rc<Note>> = self.0.notes.borrow().clone();
        for n in &notes {
            n.set_tilted_editing(on);
        }
        if let Some(v) = self.all_notes() {
            v.set_tilted_editing(on);
        }
    }

    /// The letters of every note's text now stand `em` apart, from now on (Settings › Font & Style;
    /// the value is already saved).
    pub fn set_letter_spacing(&self, em: f32) {
        let notes: Vec<Rc<Note>> = self.0.notes.borrow().clone();
        for n in &notes {
            n.set_letter_spacing(em);
        }
        if let Some(v) = self.all_notes() {
            v.set_letter_spacing(em);
        }
    }

    /// Make `font` the active font (`fonts::set_current`) and rebuild every window with it: each
    /// note's and All Notes' `Layout` is built again and its words are shaped again (like a change of
    /// Letter spacing), and the other windows' `Theme` families are set. Dropdown and delete dialog
    /// read the active font when they are created.
    pub fn set_active_font(&self, font: crate::fonts::ActiveFont) {
        crate::fonts::set_current(font);
        self.refresh_text();
    }

    /// Make `size` (12..=40, already saved) the Font size and lay every window's note text out at it:
    /// titles, headings, line pitches, gaps, check boxes and list indents scale by `size / 22`. A
    /// note whose text got shorter keeps its scroll within the new height (`max-scroll` in note.slint).
    pub fn set_font_size(&self, size: u32) {
        if size == crate::fonts::size() {
            return;
        }
        crate::fonts::set_size(size);
        self.refresh_text();
    }

    /// Put the active font and Font size on every open window and lay the notes out again.
    fn refresh_text(&self) {
        let font = crate::fonts::current();
        let notes: Vec<Rc<Note>> = self.0.notes.borrow().clone();
        for n in &notes {
            n.set_font();
        }
        if let Some(v) = self.all_notes() {
            v.set_font();
        }
        if let Some(s) = self.settings() {
            font.apply(&s.ui().global::<crate::note_window::Theme>());
        }
        for v in self.0.viewers.borrow().iter() {
            font.apply(&v.ui().global::<crate::note_window::Theme>());
        }
        // popups that are open while the font changes (the Settings page is the usual way to get here)
        if let Some(d) = self.dropdown() {
            font.apply(&d.ui().global::<crate::note_window::Theme>());
        }
        if let Some(m) = self.context_menu() {
            font.apply(&m.ui().global::<crate::note_window::Theme>());
        }
        if let Some(d) = self.delete_dialog() {
            font.apply(&d.ui().global::<crate::note_window::Theme>());
        }
    }

    /// The Settings page has closed: drop it on the next turn of the loop, never inside its own callback.
    fn settings_closed(&self) {
        let view = self.0.settings.borrow_mut().take();
        slint::Timer::single_shot(std::time::Duration::ZERO, move || drop(view));
    }
}
