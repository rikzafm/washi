//! The Dropdown menu, the right-click menu and the Delete dialog.

use super::App;
use crate::context_menu::{ClosedItem, ContextMenu};
use crate::delete_dialog::DeleteDialog;
use crate::dropdown::{extract_title_and_preview, format_relative_timestamp, shorten_preview, DropdownMenu};
use crate::all_notes::AllNotesView;
use crate::note_window::{note_title, Note};
use crate::platform::{NoteId, ScreenRect};
use slint::ComponentHandle;
use std::rc::Rc;

fn note_menu_trigger_rect(note: &Note) -> ScreenRect {
    let scale = note.ui().window().scale_factor();
    let phys_pos = note.ui().window().position();
    let (btn_x, btn_y, btn_w, btn_h) = note.menu_button_rect();
    ScreenRect::new(
        phys_pos.x + (btn_x * scale).round() as i32,
        phys_pos.y + (btn_y * scale).round() as i32,
        (btn_w * scale).round() as i32,
        (btn_h * scale).round() as i32,
    )
}

impl App {
    /// Clone the handle to the open dropdown menu, if any.
    pub fn dropdown(&self) -> Option<Rc<DropdownMenu>> {
        self.0.popups.dropdown()
    }

    /// Dismiss the open dropdown menu and reset the parent note's button state.
    pub fn dismiss_dropdown(&self) {
        let closed = self.0.popups.dismiss_dropdown();
        self.dropdown_closed(closed);
    }

    /// Give the menu button of the Note whose Dropdown menu just closed back to it.
    pub(super) fn dropdown_closed(&self, note_id: Option<NoteId>) {
        let Some(note_id) = note_id else { return };
        self.with_note(note_id, |n| n.set_dropdown_open(false));
        if let Some(v) = self.all_notes() {
            v.ui().set_dropdown_open(false);
        }
    }

    /// Dismiss the open dropdown menu if it belongs to note `id`.
    pub(super) fn dismiss_dropdown_if(&self, id: NoteId) {
        let closed = self.0.popups.dismiss_menus_of(id);
        self.dropdown_closed(closed);
    }

    /// Toggle the dropdown menu for note `id`.
    pub fn toggle_dropdown(&self, id: NoteId) {
        if let Some(open_id) = self.dropdown().map(|d| d.note_id()) {
            if open_id == id {
                self.dismiss_dropdown();
                return;
            }
            self.dismiss_dropdown();
        }
        self.open_dropdown(id);
    }

    /// Toggle the trimmed dropdown of the All Notes editor.
    pub(super) fn toggle_editor_dropdown(&self) {
        let Some(v) = self.all_notes() else { return };
        let id = v.editor_note_id();
        if self.dropdown().map(|d| d.note_id()) == Some(id) {
            self.dismiss_dropdown();
            return;
        }
        self.dismiss_dropdown();
        self.open_editor_dropdown(&v);
    }

    /// The editor's dropdown: pin, close, delete (as the action bar does it), and New.
    fn open_editor_dropdown(&self, v: &Rc<AllNotesView>) {
        let id = v.editor_note_id();
        let trigger = {
            let (x, y, w, h) = v.menu_button_rect();
            let scale = v.ui().window().scale_factor();
            let pos = v.ui().window().position();
            ScreenRect::new(
                pos.x + (x * scale).round() as i32,
                pos.y + (y * scale).round() as i32,
                (w * scale).round() as i32,
                (h * scale).round() as i32,
            )
        };
        let work_areas = self.0.env.window_control.work_areas();

        let weak = Rc::downgrade(&self.0);
        let on_dismiss = Rc::new(move |_id| {
            if let Some(inner) = weak.upgrade() {
                App(inner).dismiss_dropdown();
            }
        });
        let menu = match DropdownMenu::open_compact(id, trigger, &work_areas, v.hwnd(), on_dismiss) {
            Ok(menu) => menu,
            Err(e) => {
                eprintln!("washi: could not open dropdown menu: {e}");
                return;
            }
        };
        menu.set_pinned(v.ui().get_pinned());
        {
            let weak = Rc::downgrade(&self.0);
            menu.ui().on_toggle_pin(move || {
                let Some(inner) = weak.upgrade() else { return };
                let app = App(inner);
                let Some(v) = app.all_notes() else { return };
                let on = !v.ui().get_pinned();
                v.ui().set_pinned(on);
                app.set_always_on_top(id, on);
                if let Some(d) = app.dropdown() {
                    d.set_pinned(on);
                }
            });
        }
        {
            let weak = Rc::downgrade(&self.0);
            menu.ui().on_close_note(move || {
                if let Some(inner) = weak.upgrade() {
                    let app = App(inner);
                    app.dismiss_dropdown();
                    if let Some(v) = app.all_notes() {
                        v.close_selected_note();
                    }
                }
            });
        }
        {
            let weak = Rc::downgrade(&self.0);
            menu.ui().on_delete_note(move || {
                if let Some(inner) = weak.upgrade() {
                    let app = App(inner);
                    app.dismiss_dropdown();
                    if let Some(v) = app.all_notes() {
                        v.request_delete_selected_note();
                    }
                }
            });
        }
        {
            let weak = Rc::downgrade(&self.0);
            menu.ui().on_new_note(move || {
                if let Some(inner) = weak.upgrade() {
                    let app = App(inner);
                    app.dismiss_dropdown();
                    if let Some(v) = app.all_notes() {
                        v.new_note();
                    }
                }
            });
        }
        v.ui().set_dropdown_open(true);
        self.0.popups.show_dropdown(menu);
    }

    /// Open the dropdown menu for note `id`.
    pub fn open_dropdown(&self, id: NoteId) {
        for n in self.0.notes.borrow().iter() {
            n.flush_text();
        }

        let note = self.0.notes.borrow().iter().find(|n| n.id() == id).cloned();
        let Some(note) = note else { return };

        let trigger = note_menu_trigger_rect(&note);
        let work_areas = self.0.env.window_control.work_areas();

        let weak = Rc::downgrade(&self.0);
        let on_dismiss = Rc::new(move |_id| {
            if let Some(inner) = weak.upgrade() {
                App(inner).dismiss_dropdown();
            }
        });

        let owner_hwnd = note.hwnd();
        match DropdownMenu::open(id, trigger, &work_areas, owner_hwnd, on_dismiss) {
            Ok(menu) => {
                let recent = self.0.db.recent_notes(id).unwrap_or_default();
                let now_ms = self.0.env.clock.unix_millis();
                let items: Vec<crate::note_window::DropdownNoteItem> = recent
                    .into_iter()
                    .map(|r| {
                        let content = self.with_note(r.id, |n| n.text()).unwrap_or(r.content);
                        let (title, preview) = extract_title_and_preview(&content);
                        let timestamp = format_relative_timestamp(r.updated_at, now_ms);
                        crate::note_window::DropdownNoteItem {
                            id: r.id as i32,
                            title: title.into(),
                            preview: shorten_preview(&preview).into(),
                            timestamp: timestamp.into(),
                            has_tape: r.is_open,
                        }
                    })
                    .collect();
                menu.ui().set_notes(slint::ModelRc::from(Rc::new(slint::VecModel::from(items))));

                let pinned = note.always_on_top();
                menu.set_pinned(pinned);

                let weak = Rc::downgrade(&self.0);
                menu.ui().on_select_note(move |selected_id| {
                    if let Some(inner) = weak.upgrade() {
                        let app = App(inner);
                        app.dismiss_dropdown();
                        app.activate_note(selected_id as NoteId);
                    }
                });

                let weak = Rc::downgrade(&self.0);
                menu.ui().on_toggle_pin(move || {
                    if let Some(inner) = weak.upgrade() {
                        let app = App(inner);
                        let current = app.with_note(id, |n| n.always_on_top()).unwrap_or(false);
                        app.set_always_on_top(id, !current);
                    }
                });

                let weak = Rc::downgrade(&self.0);
                menu.ui().on_delete_note(move || {
                    if let Some(inner) = weak.upgrade() {
                        let app = App(inner);
                        app.dismiss_dropdown();
                        app.delete_note(id);
                    }
                });

                let weak = Rc::downgrade(&self.0);
                menu.ui().on_close_note(move || {
                    if let Some(inner) = weak.upgrade() {
                        let app = App(inner);
                        app.dismiss_dropdown();
                        app.close_note(id);
                    }
                });

                let weak = Rc::downgrade(&self.0);
                menu.ui().on_new_note(move || {
                    if let Some(inner) = weak.upgrade() {
                        let app = App(inner);
                        app.dismiss_dropdown();
                        app.new_note_from(id);
                    }
                });

                let weak = Rc::downgrade(&self.0);
                menu.ui().on_all_notes(move || {
                    if let Some(inner) = weak.upgrade() {
                        let app = App(inner);
                        app.dismiss_dropdown();
                        app.open_all_notes(id);
                    }
                });

                let weak = Rc::downgrade(&self.0);
                menu.ui().on_settings(move || {
                    if let Some(inner) = weak.upgrade() {
                        let app = App(inner);
                        app.dismiss_dropdown();
                        app.open_settings(id);
                    }
                });

                note.set_dropdown_open(true);
                self.0.popups.show_dropdown(menu);
            }
            Err(e) => eprintln!("washi: could not open dropdown menu: {e}"),
        }
    }

    /// Clone the handle to the open delete dialog, if any.
    pub fn delete_dialog(&self) -> Option<Rc<DeleteDialog>> {
        self.0.popups.delete_dialog()
    }

    /// Dismiss the open delete dialog if any.
    pub fn dismiss_delete_dialog(&self) {
        self.0.popups.dismiss_delete_dialog();
    }

    /// Dismiss the open delete dialog if it belongs to note `id`.
    pub fn dismiss_delete_dialog_if(&self, id: NoteId) {
        self.0.popups.dismiss_delete_dialog_of(id);
    }

    /// Delete a note for good, after asking with the themed delete dialog.
    pub fn delete_note(&self, id: NoteId) {
        self.dismiss_dropdown_if(id);

        if let Some(open_id) = self.delete_dialog().map(|d| d.note_id()) {
            if open_id == id {
                if let Some(d) = self.delete_dialog() {
                    self.0.env.window_control.bring_to_front(0, d.hwnd());
                }
                return;
            }
            self.dismiss_delete_dialog();
        }

        let note = self.0.notes.borrow().iter().find(|n| n.id() == id).cloned();
        let Some(note) = note else { return };

        let phys_pos = note.ui().window().position();
        let phys_size = note.ui().window().size();
        let target = ScreenRect::new(phys_pos.x, phys_pos.y, phys_size.width as i32, phys_size.height as i32);
        let work_areas = self.0.env.window_control.work_areas();

        let weak = Rc::downgrade(&self.0);
        let on_keep = Rc::new(move |_note_id| {
            if let Some(inner) = weak.upgrade() {
                App(inner).dismiss_delete_dialog();
            }
        });

        let weak = Rc::downgrade(&self.0);
        let on_confirm = Rc::new(move |note_id| {
            if let Some(inner) = weak.upgrade() {
                let app = App(inner);
                app.dismiss_delete_dialog();
                app.confirm_delete_note(note_id);
            }
        });

        let weak = Rc::downgrade(&self.0);
        let on_start_move = Rc::new(move |hwnd| {
            if let Some(inner) = weak.upgrade() {
                inner.env.window_control.begin_move(0, hwnd);
            }
        });

        match DeleteDialog::open(id, target, &work_areas, note.hwnd(), on_keep, on_confirm, on_start_move) {
            Ok(dialog) => {
                self.0.popups.show_delete_dialog(dialog);
            }
            Err(e) => eprintln!("washi: could not open delete dialog: {e}"),
        }
    }

    /// Open the right-click menu of note `id` at `at` (screen px): see `ui/context-menu.slint`.
    pub(super) fn show_menu(&self, id: NoteId, at: (i32, i32)) {
        let Some(note) = self.0.notes.borrow().iter().find(|n| n.id() == id).cloned() else { return };
        let clipboard = &self.0.env.clipboard;
        let has_image = clipboard.has_image();
        let work_areas = self.0.env.window_control.work_areas();

        let weak = Rc::downgrade(&self.0);
        let on_dismiss = Rc::new(move |_id| {
            if let Some(inner) = weak.upgrade() {
                App(inner).dismiss_context_menu();
            }
        });
        let menu = match ContextMenu::open(id, at, &work_areas, note.hwnd(), on_dismiss) {
            Ok(menu) => menu,
            Err(e) => {
                eprintln!("washi: could not open the right-click menu: {e}");
                return;
            }
        };
        let ui = menu.ui();
        ui.set_can_paste(has_image || clipboard.has_text());
        ui.set_can_paste_image(has_image);
        ui.set_has_selection(note.has_selection());
        ui.set_can_undo(note.can_undo());
        ui.set_can_redo(note.can_redo());
        ui.set_pinned(note.always_on_top());
        ui.set_tape_style(note.tape_style());
        let now = self.0.env.clock.unix_millis();
        let closed: Vec<ClosedItem> = self
            .0
            .db
            .closed_notes()
            .unwrap_or_default()
            .iter()
            .map(|c| ClosedItem { id: c.id as i32, title: title_of(&c.preview).into(), when: closed_when(c.updated_at, now).into() })
            .collect();
        ui.set_closed(slint::ModelRc::from(Rc::new(slint::VecModel::from(closed))));

        // an action that takes the menu away, then does its work on the note
        let then = |work: fn(&Note)| {
            let weak = Rc::downgrade(&self.0);
            move || {
                if let Some(inner) = weak.upgrade() {
                    let app = App(inner);
                    app.dismiss_context_menu();
                    app.with_note(id, |n| work(n));
                }
            }
        };
        ui.on_paste(then(|n| {
            n.paste();
        }));
        ui.on_paste_image(then(|n| {
            n.paste_image();
        }));
        ui.on_cut(then(|n| {
            n.cut();
        }));
        ui.on_copy(then(|n| {
            n.copy();
        }));
        ui.on_undo(then(|n| n.undo()));
        ui.on_redo(then(|n| n.redo()));
        {
            let weak = Rc::downgrade(&self.0);
            ui.on_toggle_pin(move || {
                let Some(inner) = weak.upgrade() else { return };
                let app = App(inner);
                let on = !app.with_note(id, |n| n.always_on_top()).unwrap_or(false);
                app.set_always_on_top(id, on);
                if let Some(m) = app.context_menu() {
                    m.ui().set_pinned(on);
                }
            });
        }
        {
            let weak = Rc::downgrade(&self.0);
            ui.on_pick_tape(move |style| {
                if let Some(inner) = weak.upgrade() {
                    let app = App(inner);
                    app.with_note(id, |n| n.set_tape_style(style));
                    if let Some(m) = app.context_menu() {
                        m.ui().set_tape_style(style);
                    }
                }
            });
        }
        {
            let weak = Rc::downgrade(&self.0);
            ui.on_reopen(move |closed_id| {
                if let Some(inner) = weak.upgrade() {
                    let app = App(inner);
                    app.dismiss_context_menu();
                    app.reopen_note(closed_id as NoteId);
                }
            });
        }
        self.0.menus_opened.set(self.0.menus_opened.get() + 1);
        self.0.popups.show_context_menu(menu);
    }

    /// How many right-click menus have been opened.
    pub fn menus_opened(&self) -> usize {
        self.0.menus_opened.get()
    }

    /// The open right-click menu, if any.
    pub fn context_menu(&self) -> Option<Rc<ContextMenu>> {
        self.0.popups.context_menu()
    }

    /// Close the right-click menu.
    pub fn dismiss_context_menu(&self) {
        self.0.popups.dismiss_context_menu();
    }
}

/// When a closed note was last edited, as the right-click menu says it: "Today", "Yesterday", "Oct 14".
fn closed_when(updated_at_ms: i64, now_ms: i64) -> String {
    let when = format_relative_timestamp(updated_at_ms, now_ms);
    let when = when.trim_start_matches(|c: char| !c.is_alphanumeric());
    let mut chars = when.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// Menu title of a note: its first line, cut to about 40 characters.
fn title_of(content: &str) -> String {
    note_title(content).unwrap_or_else(|| "(empty note)".into())
}
