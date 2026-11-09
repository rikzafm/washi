//! All Notes ("Notes") in place.
//!
//! Opening it from a note grows that note's window to the left by the sidebar's strip and puts the
//! list there. The paper stays where it is on screen and becomes a real editor (a `Note` of its
//! own, with the action bar in place of the formatting toolbar); picking another row in the list
//! swaps in an editor for that note, in the same place. The note a desktop window already shows
//! stays in sync with the editor (the app copies each text change across), and the desktop window
//! of the note being edited is hidden while the editor stands in for it.

use crate::dropdown::{extract_title_and_preview, format_relative_timestamp};
use crate::note_window::{AllNoteItem, Note, NoteWindow};
use crate::platform::{NoteId, ScreenRect};
use crate::storage::{AllNoteRow, Geometry};
use slint::{ComponentHandle, Model};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

/// The strip the window grows by on the left (logical px). Matches `AllNotesMetrics.sidebar-w`.
pub const SIDEBAR_WIDTH: i32 = 282;

pub fn format_status_line(row: &AllNoteRow, now_ms: i64) -> String {
    let rel = format_relative_timestamp(row.updated_at, now_ms);
    let rel_clean = rel.strip_prefix("· ").unwrap_or(&rel).trim();
    let rel_str = if rel_clean.is_empty() { "today" } else { rel_clean };
    if row.is_open {
        format!("Open on your desktop · edited {rel_str}")
    } else {
        format!("Closed · edited {rel_str}")
    }
}

/// Keep a window of geometry `g` inside the work area it mostly lies on: moved left or up as far as
/// needed (never past the area's own edge). Without areas, unchanged.
pub fn clamp_to_screen(g: Geometry, areas: &[ScreenRect]) -> Geometry {
    let (cx, cy) = (g.x + g.w as i32 / 2, g.y + g.h as i32 / 2);
    let area = areas
        .iter()
        .find(|a| cx >= a.x && cx < a.x + a.w && cy >= a.y && cy < a.y + a.h)
        .or_else(|| areas.first());
    let Some(a) = area else { return g };
    let x = g.x.min(a.x + a.w - g.w as i32).max(a.x);
    let y = g.y.min(a.y + a.h - g.h as i32).max(a.y);
    Geometry { x, y, ..g }
}

/// What the view needs from the app.
#[derive(Clone)]
pub struct Hooks {
    /// A real editor for a note, at this window geometry (logical px, the sidebar's strip included).
    pub make_editor: Rc<dyn Fn(NoteId, &str, Geometry) -> Result<Note, slint::PlatformError>>,
    /// Connect the window's All Notes callbacks to the app (runs for every editor made).
    pub wire: Rc<dyn Fn(&NoteWindow)>,
    /// Show or hide the desktop window of a note (the one the editor stands in for).
    pub desktop_visible: Rc<dyn Fn(NoteId, bool)>,
}

pub struct AllNotesView {
    host_note_id: NoteId,
    editor: RefCell<Rc<Note>>,
    hooks: Hooks,
    rows: RefCell<Vec<AllNoteRow>>,
    now_ms: Cell<i64>,
    open_items: RefCell<Vec<AllNoteItem>>,
    closed_items: RefCell<Vec<AllNoteItem>>,
    /// Editors that were replaced: dropped a moment later, not inside their own callback.
    retired: Rc<RefCell<Vec<Rc<Note>>>>,
    retire_timer: slint::Timer,
}

impl AllNotesView {
    /// Grow the window of `host_note_id` (at `host`, logical px) leftward and show the list.
    pub fn open(
        host_note_id: NoteId,
        host: Geometry,
        rows: &[AllNoteRow],
        now_ms: i64,
        areas: &[ScreenRect],
        hooks: Hooks,
    ) -> Result<Self, slint::PlatformError> {
        let content = rows.iter().find(|r| r.id == host_note_id).map(|r| r.content.clone()).unwrap_or_default();
        let g = clamp_to_screen(
            Geometry { x: host.x - SIDEBAR_WIDTH, y: host.y, w: host.w + SIDEBAR_WIDTH as u32, h: host.h },
            areas,
        );
        let editor = Rc::new(Self::make(&hooks, host_note_id, &content, g)?);
        (hooks.desktop_visible)(host_note_id, false);
        let view = Self {
            host_note_id,
            editor: RefCell::new(editor),
            hooks,
            rows: RefCell::new(rows.to_vec()),
            now_ms: Cell::new(now_ms),
            open_items: RefCell::new(Vec::new()),
            closed_items: RefCell::new(Vec::new()),
            retired: Rc::new(RefCell::new(Vec::new())),
            retire_timer: slint::Timer::default(),
        };
        view.refresh(rows, Some(host_note_id), now_ms);
        Ok(view)
    }

    fn make(hooks: &Hooks, id: NoteId, content: &str, g: Geometry) -> Result<Note, slint::PlatformError> {
        let note = (hooks.make_editor)(id, content, g)?;
        note.set_saves_geometry(false);
        note.ui().set_all_notes_open(true);
        (hooks.wire)(note.ui());
        Ok(note)
    }

    fn editor(&self) -> Rc<Note> {
        self.editor.borrow().clone()
    }

    pub fn host_note_id(&self) -> NoteId {
        self.host_note_id
    }

    /// The editor (the `Note` that shows the selected note).
    pub fn editor_handle(&self) -> Rc<Note> {
        self.editor()
    }

    /// The note the editor shows.
    pub fn editor_note_id(&self) -> NoteId {
        self.editor().id()
    }

    pub fn editor_text(&self) -> String {
        self.editor().text()
    }

    pub fn set_editor_text(&self, text: &str) {
        self.editor().set_text(text);
    }

    /// Swap in an editor for `row`, in the window's place.
    fn switch_editor(&self, row: &AllNoteRow) {
        let old = self.editor();
        if old.id() == row.id {
            return;
        }
        old.flush_text();
        let g = old.geometry();
        match Self::make(&self.hooks, row.id, &row.content, g) {
            Ok(new) => {
                let new = Rc::new(new);
                self.apply_lists(new.ui());
                (self.hooks.desktop_visible)(old.id(), true);
                (self.hooks.desktop_visible)(row.id, false);
                *self.editor.borrow_mut() = new;
                self.retired.borrow_mut().push(old);
                self.retire_timer.start(slint::TimerMode::SingleShot, Duration::from_millis(0), {
                    let retired = self.retired.clone();
                    move || {
                        // dropping hides the old window
                        retired.borrow_mut().clear();
                    }
                });
            }
            Err(e) => eprintln!("washi: could not open the editor of note {}: {e}", row.id),
        }
    }

    fn apply_lists(&self, ui: &NoteWindow) {
        let open = self.open_items.borrow().clone();
        let closed = self.closed_items.borrow().clone();
        ui.set_all_notes_open_count(open.len() as i32);
        ui.set_all_notes_closed_count(closed.len() as i32);
        ui.set_all_notes_open_list(slint::ModelRc::from(Rc::new(slint::VecModel::from(open))));
        ui.set_all_notes_closed_list(slint::ModelRc::from(Rc::new(slint::VecModel::from(closed))));
    }

    fn apply_selection(&self, row: Option<&AllNoteRow>) {
        let ui = self.editor().ui().clone_strong();
        match row {
            Some(row) => {
                ui.set_all_notes_selected_id(row.id as i32);
                ui.set_all_notes_selected_is_open(row.is_open);
                ui.set_all_notes_status_text(format_status_line(row, self.now_ms.get()).into());
            }
            None => {
                ui.set_all_notes_selected_id(-1);
                ui.set_all_notes_selected_is_open(false);
                ui.set_all_notes_status_text("".into());
            }
        }
    }

    pub fn refresh(&self, rows: &[AllNoteRow], preferred_selected_id: Option<NoteId>, now_ms: i64) {
        *self.rows.borrow_mut() = rows.to_vec();
        self.now_ms.set(now_ms);

        let mut open_items = Vec::new();
        let mut closed_items = Vec::new();
        for r in rows {
            let (title, _) = extract_title_and_preview(&r.content);
            let item = AllNoteItem { id: r.id as i32, title: title.into(), is_open: r.is_open };
            if r.is_open {
                open_items.push(item);
            } else {
                closed_items.push(item);
            }
        }
        *self.open_items.borrow_mut() = open_items;
        *self.closed_items.borrow_mut() = closed_items;

        let current = self.editor().id();
        let target = preferred_selected_id
            .filter(|&id| rows.iter().any(|r| r.id == id))
            .or_else(|| Some(current).filter(|&id| rows.iter().any(|r| r.id == id)))
            .or_else(|| rows.first().map(|r| r.id));
        let target_row = target.and_then(|id| rows.iter().find(|r| r.id == id));
        if let Some(row) = target_row {
            self.switch_editor(row);
        }
        self.apply_lists(self.editor().ui());
        self.apply_selection(target_row);
    }

    pub fn select_note(&self, id: NoteId) {
        self.set_confirming_delete(false);
        let rows = self.rows.borrow().clone();
        self.refresh(&rows, Some(id), self.now_ms.get());
    }

    pub fn invoke_select_note(&self, id: NoteId) {
        self.set_confirming_delete(false);
        self.ui().invoke_all_notes_select_note(id as i32);
    }

    pub fn invoke_open_note(&self, id: NoteId) {
        self.ui().invoke_all_notes_open_note(id as i32);
    }

    pub fn invoke_new_note(&self) {
        self.set_confirming_delete(false);
        self.ui().invoke_all_notes_new_note();
    }

    pub fn invoke_open_or_focus_note(&self) {
        self.set_confirming_delete(false);
        self.ui().invoke_all_notes_open_or_focus();
    }

    pub fn invoke_close_note(&self) {
        self.set_confirming_delete(false);
        self.ui().invoke_all_notes_close_note();
    }

    pub fn invoke_export_note(&self) {
        self.ui().invoke_all_notes_export_note();
    }

    pub fn invoke_request_delete(&self) {
        if self.selected_note_id().is_some() {
            self.set_confirming_delete(true);
            self.ui().invoke_all_notes_request_delete();
        }
    }

    pub fn invoke_cancel_delete(&self) {
        self.set_confirming_delete(false);
        self.ui().invoke_all_notes_cancel_delete();
    }

    pub fn invoke_confirm_delete(&self) {
        self.ui().invoke_all_notes_confirm_delete();
    }

    /// The editor's window (a handle to it).
    pub fn ui(&self) -> NoteWindow {
        self.editor().ui().clone_strong()
    }

    pub fn selected_note_id(&self) -> Option<NoteId> {
        let id = self.ui().get_all_notes_selected_id();
        (id >= 0).then_some(id as NoteId)
    }

    pub fn is_selected_note_open(&self) -> bool {
        self.ui().get_all_notes_selected_is_open()
    }

    pub fn is_confirming_delete(&self) -> bool {
        self.ui().get_all_notes_confirming_delete()
    }

    pub fn confirming_delete(&self) -> bool {
        self.is_confirming_delete()
    }

    pub fn set_confirming_delete(&self, confirming: bool) {
        self.ui().set_all_notes_confirming_delete(confirming);
    }

    pub fn open_count(&self) -> usize {
        self.ui().get_all_notes_open_count().max(0) as usize
    }

    pub fn closed_count(&self) -> usize {
        self.ui().get_all_notes_closed_count().max(0) as usize
    }

    pub fn open_items(&self) -> Vec<AllNoteItem> {
        self.ui().get_all_notes_open_list().iter().collect()
    }

    pub fn closed_items(&self) -> Vec<AllNoteItem> {
        self.ui().get_all_notes_closed_list().iter().collect()
    }

    pub fn open_notes(&self) -> Vec<AllNoteItem> {
        self.open_items()
    }

    pub fn closed_notes(&self) -> Vec<AllNoteItem> {
        self.closed_items()
    }

    pub fn status_text(&self) -> String {
        self.ui().get_all_notes_status_text().to_string()
    }

    pub fn hwnd(&self) -> Option<isize> {
        self.editor().hwnd()
    }

    pub fn dispatch_event(&self, event: slint::platform::WindowEvent) {
        self.ui().window().dispatch_event(event);
    }

    /// Where the paper is: the editor's window less the sidebar's strip (logical px).
    pub fn paper_geometry(&self) -> Geometry {
        let g = self.editor().geometry();
        Geometry { x: g.x + SIDEBAR_WIDTH, y: g.y, w: g.w.saturating_sub(SIDEBAR_WIDTH as u32), h: g.h }
    }

    /// Put the text away, give the desktop its window back and drop the editor.
    pub fn close(&self) {
        let editor = self.editor();
        editor.flush();
        (self.hooks.desktop_visible)(editor.id(), true);
        editor.set_visible(false);
    }
}
