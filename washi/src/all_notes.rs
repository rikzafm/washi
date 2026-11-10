//! All Notes ("Notes") in place.
//!
//! Opening it from a note grows that note's window to the left by the sidebar's strip and puts the
//! list there. The paper stays where it is on screen and becomes a real editor (a `Note` of its
//! own, with the action bar in place of the formatting toolbar); picking another row in the list
//! shows that note in the same editor window (never a new one, so nothing flashes). The note a desktop window already shows
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

/// The strip the window grows by on the left (logical px).
pub const SIDEBAR_WIDTH: i32 = crate::sidebar_panel::STRIP as i32;

/// The transparent margin around a note's paper (`PaperTheme.margin`).
const MARGIN: i32 = 42;
/// Space between the All Notes window and a note parked beside it, and the cascade step of the
/// next note parked on the same side (logical px).
pub const PARK_GAP: i32 = 12;
pub const PARK_CASCADE: i32 = 30;

/// Which side of the All Notes window a note was parked on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Right,
    Left,
}

/// A note All Notes switched away from and moved out of its way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parked {
    /// Where the note was before (its saved place): where it goes back to.
    pub home: Geometry,
    /// Where it was put, and on which side (`None`: left in place, just behind).
    pub at: Geometry,
    pub side: Option<Side>,
}

fn overlaps(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// What is visible of the note window `g`: its paper (x, y, w, h).
fn paper_box(g: Geometry) -> (i32, i32, i32, i32) {
    (g.x + MARGIN, g.y + MARGIN, g.w as i32 - 2 * MARGIN, g.h as i32 - 2 * MARGIN)
}

/// What is visible of the All Notes window `g`: the sidebar from its left edge, then the paper.
fn all_notes_box(g: Geometry) -> (i32, i32, i32, i32) {
    (g.x, g.y + MARGIN, g.w as i32 - MARGIN, g.h as i32 - 2 * MARGIN)
}

/// Where to put the note window `note` so its paper sits beside the All Notes window `editor`:
/// right first, then left, `k` cascade steps in on that side. `None` when its saved place does not
/// overlap All Notes (it can stay), or when neither side has room (it goes behind instead).
pub fn park_beside(note: Geometry, editor: Geometry, k: [usize; 2], areas: &[ScreenRect]) -> Option<(Geometry, Side)> {
    let e = all_notes_box(editor);
    if !overlaps(paper_box(note), e) {
        return None;
    }
    let (cx, cy) = (editor.x + editor.w as i32 / 2, editor.y + editor.h as i32 / 2);
    let area = areas.iter().find(|a| cx >= a.x && cx < a.x + a.w && cy >= a.y && cy < a.y + a.h).or(areas.first())?;
    let (nw, nh) = (note.w as i32, note.h as i32);
    let place = |x: i32, step: i32| {
        // as near the note's own height as the screen allows
        let y = (note.y + step).min(area.y + area.h - nh + MARGIN).max(area.y - MARGIN);
        Geometry { x, y, ..note }
    };
    let fits = |g: Geometry| {
        let p = paper_box(g);
        p.0 >= area.x && p.1 >= area.y && p.0 + p.2 <= area.x + area.w && p.1 + p.3 <= area.y + area.h && !overlaps(p, e)
    };
    let step = |k: usize| k as i32 * PARK_CASCADE;
    let right = place(e.0 + e.2 + PARK_GAP - MARGIN + step(k[0]), step(k[0]));
    if fits(right) {
        return Some((right, Side::Right));
    }
    let left = place(e.0 - PARK_GAP + MARGIN - nw - step(k[1]), step(k[1]));
    if fits(left) {
        return Some((left, Side::Left));
    }
    None
}

/// How long both windows stay on screen when one takes over from the other (opening and closing
/// All Notes): long enough for the newly shown one to draw its first frame.
pub const SWAP_OVERLAP: Duration = Duration::from_millis(40);

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
    /// A note the editor stops standing in for: put its desktop window back, out of the way of All
    /// Notes (beside it or behind it, as Settings › General says).
    pub park: Rc<dyn Fn(NoteId)>,
    /// The real editor, at this window geometry (logical px, the sidebar's strip included).
    pub make_editor: Rc<dyn Fn(NoteId, &str, Geometry) -> Result<Note, slint::PlatformError>>,
    /// Connect the window's All Notes callbacks to the app.
    pub wire: Rc<dyn Fn(&NoteWindow)>,
    /// Show or hide the desktop window of a note (the one the editor stands in for).
    pub desktop_visible: Rc<dyn Fn(NoteId, bool)>,
}

pub struct AllNotesView {
    host_note_id: NoteId,
    editor: Rc<Note>,
    hooks: Hooks,
    rows: RefCell<Vec<AllNoteRow>>,
    now_ms: Cell<i64>,
    open_items: RefCell<Vec<AllNoteItem>>,
    closed_items: RefCell<Vec<AllNoteItem>>,
    /// Hides the host's desktop window just after the editor first shows (see `open`).
    swap_timer: slint::Timer,
    /// Notes moved out of the way while All Notes is open; they go back when it closes.
    parked: RefCell<std::collections::HashMap<NoteId, Parked>>,
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
        // The host hides once the editor has drawn its first frame (both show the same paper in the
        // same place): hiding it at once left a frame with neither window on screen, a flash.
        let swap_timer = slint::Timer::default();
        {
            let hide = hooks.desktop_visible.clone();
            swap_timer.start(slint::TimerMode::SingleShot, SWAP_OVERLAP, move || hide(host_note_id, false));
        }
        let view = Self {
            host_note_id,
            editor,
            hooks,
            rows: RefCell::new(rows.to_vec()),
            now_ms: Cell::new(now_ms),
            open_items: RefCell::new(Vec::new()),
            closed_items: RefCell::new(Vec::new()),
            swap_timer,
            parked: RefCell::new(std::collections::HashMap::new()),
        };
        view.refresh(rows, Some(host_note_id), now_ms);
        Ok(view)
    }

    fn make(hooks: &Hooks, id: NoteId, content: &str, g: Geometry) -> Result<Note, slint::PlatformError> {
        let note = (hooks.make_editor)(id, content, g)?;
        note.set_saves_geometry(false);
        note.set_autofocus(false);
        note.ui().set_all_notes_open(true);
        (hooks.wire)(note.ui());
        Ok(note)
    }

    fn editor(&self) -> Rc<Note> {
        self.editor.clone()
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

    /// Show `row` in the editor, in the same window.
    fn switch_editor(&self, row: &AllNoteRow) {
        let old = self.editor.id();
        if old == row.id {
            return;
        }
        self.editor.switch_to(row.id, &row.content);
        (self.hooks.park)(old);
        (self.hooks.desktop_visible)(row.id, false);
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

    /// The parked note `id`, if it is.
    pub fn parked(&self, id: NoteId) -> Option<Parked> {
        self.parked.borrow().get(&id).copied()
    }

    /// How many notes are parked on each side now (right, left), not counting `except`.
    pub fn parked_counts(&self, except: NoteId) -> [usize; 2] {
        let p = self.parked.borrow();
        let n = |s: Side| p.iter().filter(|(id, v)| **id != except && v.side == Some(s)).count();
        [n(Side::Right), n(Side::Left)]
    }

    pub fn set_parked(&self, id: NoteId, p: Parked) {
        self.parked.borrow_mut().insert(id, p);
    }

    /// Draw the editor (and its Sidebar) at UI size `ui`; the parked notes' places are carried into the
    /// new logical px by `k` (old UI size / new).
    pub fn set_ui_scale(&self, ui: f32, k: f32) {
        let paper = self.paper_geometry().scaled(k);
        self.editor.set_ui_scale(ui, k);
        let new_window = Geometry {
            x: paper.x - SIDEBAR_WIDTH,
            y: paper.y,
            w: paper.w + SIDEBAR_WIDTH as u32,
            h: paper.h,
        };
        self.editor.place(new_window);
        for p in self.parked.borrow_mut().values_mut() {
            p.home = p.home.scaled(k);
            p.at = p.at.scaled(k);
        }
    }

    /// Every parked note, emptied (All Notes closes).
    pub fn take_parked(&self) -> Vec<(NoteId, Parked)> {
        self.parked.borrow_mut().drain().collect()
    }

    /// Put the caret in the editor, at the start of the note (a note just made here).
    pub fn focus_editor(&self) {
        self.editor.focus_start();
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
    /// Close: the desktop window of the note shown comes back first, and the editor goes a moment
    /// later, once that window has drawn (so nothing flashes).
    pub fn close(&self) {
        let editor = self.editor();
        editor.flush();
        self.swap_timer.stop();
        (self.hooks.desktop_visible)(editor.id(), true);
        slint::Timer::single_shot(SWAP_OVERLAP, move || editor.set_visible(false));
    }
}
