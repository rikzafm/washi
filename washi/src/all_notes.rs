//! All Notes ("Notes") in place.
//!
//! Opening it from a note grows that note's window to the left by the sidebar's strip and puts the
//! list there. The paper stays where it is on screen and becomes a real editor (consuming `Editor`
//! directly, with the action bar in place of the formatting toolbar); picking another row in the list
//! shows that note in the same editor window (never a new one, so nothing flashes). The note a desktop window already shows
//! stays in sync with the editor, and the desktop window of the note being edited is hidden while the editor stands in for it.

use crate::document::Caret;
use crate::dropdown::{extract_title_and_preview, format_relative_timestamp};
use crate::editor::Editor;
use crate::note_window::{AllNoteItem, NoteWindow};
use crate::platform::{Clock, ImageClipboard, NoteId, ResizeEdge, ScreenRect, WindowControl};
use crate::storage::{db::{Choice, Sidebar}, AllNoteRow, Db, Geometry};
use crate::ui_scale::WindowScale;
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

/// The editor window's geometry for a given paper geometry (expanded leftward by the sidebar strip).
pub fn expand_for_sidebar(paper: Geometry) -> Geometry {
    Geometry {
        x: paper.x - SIDEBAR_WIDTH,
        y: paper.y,
        w: paper.w + SIDEBAR_WIDTH as u32,
        h: paper.h,
    }
}

fn hwnd_of(ui: &NoteWindow) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let window_handle = ui.window().window_handle();
    let handle = window_handle.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
        _ => None,
    }
}

/// Host operations requested by the All Notes view for desktop note coordination.
pub trait AllNotesDelegate {
    fn desktop_note_geometry(&self, id: NoteId) -> Option<Geometry>;
    fn desktop_note_hwnd(&self, id: NoteId) -> Option<isize>;
    fn set_desktop_note_visible(&self, id: NoteId, visible: bool);
    fn park_desktop_note(&self, id: NoteId, at: (i32, i32));
    fn restore_desktop_note(&self, id: NoteId, at: Geometry, home: Geometry);
    fn desktop_note_text(&self, id: NoteId) -> Option<String>;
    fn set_desktop_note_text(&self, id: NoteId, text: &str);
    fn flush_open_notes(&self);
    fn open_desktop_note(&self, id: NoteId);
    fn close_desktop_note(&self, id: NoteId);
    fn create_new_note(&self, from: Option<NoteId>) -> Option<NoteId>;
    fn delete_note(&self, id: NoteId);
    fn export_note(&self, id: NoteId);
    fn toggle_dropdown(&self, id: NoteId);
    fn dismiss_dropdown(&self);
    fn poke_suspend(&self);
    fn is_settings_host(&self, id: NoteId) -> bool;
}

pub struct AllNotesView {
    host_note_id: NoteId,
    ui: NoteWindow,
    editor: Rc<Editor>,
    scale: Rc<WindowScale>,
    db: Rc<Db>,
    clock: Rc<dyn Clock>,
    window_control: Rc<dyn WindowControl>,
    delegate: Rc<dyn AllNotesDelegate>,
    on_close: Rc<dyn Fn()>,
    rows: RefCell<Vec<AllNoteRow>>,
    now_ms: Cell<i64>,
    open_items: RefCell<Vec<AllNoteItem>>,
    closed_items: RefCell<Vec<AllNoteItem>>,
    /// Hides the host's desktop window just after the editor first shows (see `open`).
    swap_timer: slint::Timer,
    autosave_dirty: Cell<bool>,
    autosave_timer: slint::Timer,
    /// Notes moved out of the way while All Notes is open; they go back when it closes.
    parked: RefCell<std::collections::HashMap<NoteId, Parked>>,
}

impl AllNotesView {
    /// Grow the window of `host_note_id` leftward and show the All Notes view.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        host_note_id: NoteId,
        db: Rc<Db>,
        clock: Rc<dyn Clock>,
        icon: slint::Image,
        window_control: Rc<dyn WindowControl>,
        clipboard: Rc<dyn ImageClipboard>,
        delegate: Rc<dyn AllNotesDelegate>,
        on_close: Rc<dyn Fn()>,
    ) -> Result<Rc<Self>, slint::PlatformError> {
        let ui = NoteWindow::new()?;
        ui.set_all_notes_open(true);
        ui.set_autofocus(false);
        ui.invoke_focus_keys();
        ui.set_sidebar_theme(db.sidebar_theme(Sidebar::Notes).index());
        let layout = db.sidebar_layout(Sidebar::Notes);
        ui.set_all_notes_height_mode(layout.height.index());
        ui.set_all_notes_anchor(layout.anchor.index());
        ui.set_app_icon(icon);

        let scale = Rc::new(WindowScale::attach(ui.window()));

        let host = delegate.desktop_note_geometry(host_note_id).unwrap_or(Geometry {
            x: 100,
            y: 100,
            w: 363,
            h: 433,
        });

        let sf = scale.factor();
        let areas: Vec<ScreenRect> = window_control
            .work_areas()
            .iter()
            .map(|a| {
                let l = |p: i32| (p as f32 / sf).round() as i32;
                ScreenRect::new(l(a.x), l(a.y), l(a.w), l(a.h))
            })
            .collect();

        let g = clamp_to_screen(expand_for_sidebar(host), &areas);
        let psz = slint::PhysicalSize::new((g.w as f32 * sf).round() as u32, (g.h as f32 * sf).round() as u32);
        let ppos = slint::PhysicalPosition::new((g.x as f32 * sf).round() as i32, (g.y as f32 * sf).round() as i32);
        ui.window().set_size(psz);
        ui.window().set_position(ppos);

        let content = db.content(host_note_id).ok().flatten().unwrap_or_default();
        let content = delegate.desktop_note_text(host_note_id).unwrap_or(content);

        let editor = Rc::new(Editor::new(host_note_id, &content, db.clone(), clipboard));
        editor.set_clock(clock.clone());

        let view = Rc::new(Self {
            host_note_id,
            ui: ui.clone_strong(),
            editor: editor.clone(),
            scale: scale.clone(),
            db: db.clone(),
            clock: clock.clone(),
            window_control: window_control.clone(),
            delegate: delegate.clone(),
            on_close: on_close.clone(),
            rows: RefCell::new(Vec::new()),
            now_ms: Cell::new(clock.unix_millis()),
            open_items: RefCell::new(Vec::new()),
            closed_items: RefCell::new(Vec::new()),
            swap_timer: slint::Timer::default(),
            autosave_dirty: Cell::new(false),
            autosave_timer: slint::Timer::default(),
            parked: RefCell::new(std::collections::HashMap::new()),
        });

        view.wire_ui();
        view.bind_editor();
        view.refresh_rows(Some(host_note_id));

        // The host hides once the editor has drawn its first frame
        {
            let delegate = delegate.clone();
            view.swap_timer.start(slint::TimerMode::SingleShot, SWAP_OVERLAP, move || {
                delegate.set_desktop_note_visible(host_note_id, false);
            });
        }

        ui.show()?;
        window_control.bring_to_front(0, hwnd_of(&ui));

        Ok(view)
    }

    fn bind_editor(self: &Rc<Self>) {
        let weak_this = Rc::downgrade(self);
        let on_changed = {
            let weak_this = weak_this.clone();
            Rc::new(move |_focus: Option<Caret>| {
                if let Some(this) = weak_this.upgrade() {
                    this.autosave_dirty.set(true);
                    let weak_flush = Rc::downgrade(&this);
                    this.autosave_timer.start(slint::TimerMode::SingleShot, Duration::from_millis(500), move || {
                        if let Some(this) = weak_flush.upgrade() {
                            this.flush();
                        }
                    });
                    let id = this.editor.id();
                    let text = this.editor.text();
                    this.delegate.set_desktop_note_text(id, &text);
                    this.update_row_content(id, &text);
                }
            })
        };

        let pointer_down = {
            let delegate = self.delegate.clone();
            Some(Rc::new(move || {
                delegate.dismiss_dropdown();
            }) as Rc<dyn Fn()>)
        };

        let paste_image = Some(Rc::new(|| false) as Rc<dyn Fn() -> bool>);

        self.editor.bind_ui(&self.ui, on_changed, pointer_down, None, paste_image);
    }

    fn wire_ui(self: &Rc<Self>) {
        let ui = &self.ui;

        // Window move
        {
            let window_control = self.window_control.clone();
            let delegate = self.delegate.clone();
            let weak_ui = ui.as_weak();
            ui.on_start_move(move || {
                delegate.dismiss_dropdown();
                if let Some(ui) = weak_ui.upgrade() {
                    window_control.begin_move(0, hwnd_of(&ui));
                }
            });
        }

        // Window resize
        {
            let window_control = self.window_control.clone();
            let weak_ui = ui.as_weak();
            ui.on_start_resize(move |code| {
                if let Some(edge) = ResizeEdge::from_code(code) {
                    if !matches!(edge, ResizeEdge::SouthWest | ResizeEdge::West | ResizeEdge::NorthWest) {
                        if let Some(ui) = weak_ui.upgrade() {
                            window_control.begin_resize(0, hwnd_of(&ui), edge);
                        }
                    }
                }
            });
        }

        // Scale factor changed
        {
            let weak_ui = ui.as_weak();
            let scale = self.scale.clone();
            let editor = self.editor.clone();
            ui.on_scale_factor_changed(move || {
                if let Some(ui) = weak_ui.upgrade() {
                    if scale.resync(ui.window(), false) {
                        editor.relayout(&ui);
                    }
                }
            });
        }

        // Pointer moved
        {
            let delegate = self.delegate.clone();
            ui.on_pointer_moved(move || {
                delegate.poke_suspend();
            });
        }

        // Dropdown menu trigger
        {
            let delegate = self.delegate.clone();
            let weak_this = Rc::downgrade(self);
            ui.on_open_dropdown(move || {
                if let Some(this) = weak_this.upgrade() {
                    delegate.toggle_dropdown(this.editor_note_id());
                }
            });
        }

        // Window close requested
        {
            let on_close = self.on_close.clone();
            ui.on_all_notes_close_view(move || {
                on_close();
            });
        }
        {
            let on_close = self.on_close.clone();
            ui.window().on_close_requested(move || {
                on_close();
                slint::CloseRequestResponse::HideWindow
            });
        }

        // Sidebar list callbacks
        {
            let weak_this = Rc::downgrade(self);
            ui.on_all_notes_select_note(move |id| {
                if let Some(this) = weak_this.upgrade() {
                    this.select_note(id as NoteId);
                }
            });
        }
        {
            let weak_this = Rc::downgrade(self);
            ui.on_all_notes_open_note(move |id| {
                if let Some(this) = weak_this.upgrade() {
                    this.open_note(id as NoteId);
                }
            });
        }
        {
            let weak_this = Rc::downgrade(self);
            ui.on_all_notes_new_note(move || {
                if let Some(this) = weak_this.upgrade() {
                    this.new_note();
                }
            });
        }
        {
            let weak_this = Rc::downgrade(self);
            ui.on_all_notes_open_or_focus(move || {
                if let Some(this) = weak_this.upgrade() {
                    this.open_or_focus_selected_note();
                }
            });
        }
        {
            let weak_this = Rc::downgrade(self);
            ui.on_all_notes_close_note(move || {
                if let Some(this) = weak_this.upgrade() {
                    this.close_selected_note();
                }
            });
        }
        {
            let weak_this = Rc::downgrade(self);
            ui.on_all_notes_export_note(move || {
                if let Some(this) = weak_this.upgrade() {
                    this.export_selected_note();
                }
            });
        }
        {
            let weak_this = Rc::downgrade(self);
            ui.on_all_notes_request_delete(move || {
                if let Some(this) = weak_this.upgrade() {
                    this.request_delete_selected_note();
                }
            });
        }
        {
            let weak_this = Rc::downgrade(self);
            ui.on_all_notes_cancel_delete(move || {
                if let Some(this) = weak_this.upgrade() {
                    this.cancel_delete();
                }
            });
        }
        {
            let weak_this = Rc::downgrade(self);
            ui.on_all_notes_confirm_delete(move || {
                if let Some(this) = weak_this.upgrade() {
                    this.confirm_delete();
                }
            });
        }
    }

    /// The editor engine backing the current paper.
    pub fn editor(&self) -> &Rc<Editor> {
        &self.editor
    }

    pub fn host_note_id(&self) -> NoteId {
        self.host_note_id
    }

    /// The editor (handle to Editor engine).
    pub fn editor_handle(&self) -> &Rc<Editor> {
        &self.editor
    }

    /// The note the editor shows.
    pub fn editor_note_id(&self) -> NoteId {
        self.editor.id()
    }

    pub fn editor_text(&self) -> String {
        self.editor.text()
    }

    pub fn set_editor_text(self: &Rc<Self>, text: &str) {
        self.editor.set_text(&self.ui, text);
        self.autosave_dirty.set(true);
        let weak_flush = Rc::downgrade(self);
        self.autosave_timer.start(slint::TimerMode::SingleShot, Duration::from_millis(500), move || {
            if let Some(this) = weak_flush.upgrade() {
                this.flush();
            }
        });
        let id = self.editor.id();
        self.delegate.set_desktop_note_text(id, text);
        self.update_row_content(id, text);
    }

    pub fn flush(&self) {
        if !self.autosave_dirty.replace(false) {
            return;
        }
        let id = self.editor.id();
        let text = self.editor.text();
        if let Err(e) = self.db.save_content(id, &text, self.clock.unix_millis()) {
            eprintln!("washi: autosave of note {id} failed: {e}");
        }
    }

    /// Show `row` in the editor, in the same window.
    fn switch_editor(&self, row: &AllNoteRow) {
        let old = self.editor.id();
        if old == row.id {
            return;
        }
        self.flush();
        self.editor.switch_to(&self.ui, row.id, &row.content);
        self.park_note(old);
        self.delegate.set_desktop_note_visible(row.id, false);
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
        let ui = &self.ui;
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

    fn update_row_content(&self, id: NoteId, text: &str) {
        {
            let mut rows = self.rows.borrow_mut();
            if let Some(r) = rows.iter_mut().find(|r| r.id == id) {
                r.content = text.to_string();
            }
        }
        let (title, _) = extract_title_and_preview(text);
        {
            let mut open = self.open_items.borrow_mut();
            if let Some(item) = open.iter_mut().find(|i| i.id == id as i32) {
                item.title = title.clone().into();
                self.ui.set_all_notes_open_list(slint::ModelRc::from(Rc::new(slint::VecModel::from(open.clone()))));
            }
        }
        {
            let mut closed = self.closed_items.borrow_mut();
            if let Some(item) = closed.iter_mut().find(|i| i.id == id as i32) {
                item.title = title.into();
                self.ui.set_all_notes_closed_list(slint::ModelRc::from(Rc::new(slint::VecModel::from(closed.clone()))));
            }
        }
        if self.selected_id() == Some(id) {
            let rows = self.rows.borrow();
            if let Some(r) = rows.iter().find(|r| r.id == id) {
                self.ui.set_all_notes_status_text(format_status_line(r, self.now_ms.get()).into());
            }
        }
    }

    pub fn refresh_rows(&self, preferred_selected_id: Option<NoteId>) {
        self.delegate.flush_open_notes();
        let mut rows = self.db.all_notes().unwrap_or_default();
        let active_id = self.editor.id();
        let active_text = self.editor.text();
        for r in &mut rows {
            if let Some(live) = self.delegate.desktop_note_text(r.id) {
                r.content = live;
            }
            if r.id == active_id {
                r.content = active_text.clone();
            }
        }
        self.refresh_with_rows(&rows, preferred_selected_id);
    }

    fn refresh_with_rows(&self, rows: &[AllNoteRow], preferred_selected_id: Option<NoteId>) {
        *self.rows.borrow_mut() = rows.to_vec();
        self.now_ms.set(self.clock.unix_millis());

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

        let current = self.editor.id();
        let target = preferred_selected_id
            .filter(|&id| rows.iter().any(|r| r.id == id))
            .or_else(|| Some(current).filter(|&id| rows.iter().any(|r| r.id == id)))
            .or_else(|| rows.first().map(|r| r.id));
        let target_row = target.and_then(|id| rows.iter().find(|r| r.id == id));
        if let Some(row) = target_row {
            self.switch_editor(row);
        }
        self.apply_lists(&self.ui);
        self.apply_selection(target_row);
    }

    pub fn on_desktop_note_text_changed(&self, id: NoteId, text: &str) {
        if self.editor.id() == id && self.editor.text() != text {
            self.editor.set_text(&self.ui, text);
        }
        self.update_row_content(id, text);
    }

    pub fn on_desktop_note_opened(&self, _id: NoteId) {
        self.refresh_rows(None);
    }

    pub fn on_desktop_note_closed(&self, _id: NoteId) {
        self.refresh_rows(None);
    }

    pub fn on_note_deleted(&self, id: NoteId) {
        self.set_confirming_delete(false);
        let old_rows = self.rows.borrow().clone();
        let old_idx = old_rows.iter().position(|r| r.id == id).unwrap_or(0);

        self.delegate.flush_open_notes();
        let mut new_rows = self.db.all_notes().unwrap_or_default();
        let active_id = self.editor.id();
        let active_text = self.editor.text();
        for r in &mut new_rows {
            if let Some(live) = self.delegate.desktop_note_text(r.id) {
                r.content = live;
            }
            if r.id == active_id {
                r.content = active_text.clone();
            }
        }

        let preferred = if self.selected_id() == Some(id) {
            new_rows.get(old_idx).or_else(|| new_rows.last()).map(|r| r.id)
        } else {
            self.selected_id()
        };

        self.refresh_with_rows(&new_rows, preferred);
    }

    /// Park a desktop note out of the way of the All Notes window.
    pub fn park_note(&self, id: NoteId) {
        if self.delegate.is_settings_host(id) {
            return;
        }
        let Some(now) = self.delegate.desktop_note_geometry(id) else { return };
        let home = self.parked(id).map_or(now, |p| p.home);
        let scale = self.ui.window().scale_factor();
        let areas: Vec<ScreenRect> = self
            .window_control
            .work_areas()
            .iter()
            .map(|a| {
                let l = |p: i32| (p as f32 / scale).round() as i32;
                ScreenRect::new(l(a.x), l(a.y), l(a.w), l(a.h))
            })
            .collect();
        let beside = if self.db.park_notes_beside() {
            park_beside(home, self.geometry(), self.parked_counts(id), &areas)
        } else {
            None
        };
        let (at, side) = beside.map_or((home, None), |(g, s)| (g, Some(s)));
        self.delegate.park_desktop_note(id, (at.x, at.y));
        self.set_parked(id, Parked { home, at, side });
        let hwnd = self.delegate.desktop_note_hwnd(id);
        self.window_control.place_behind(id, hwnd, self.hwnd());
    }

    /// Restore all parked desktop notes when All Notes closes.
    pub fn unpark_notes(&self) {
        for (id, p) in self.take_parked() {
            if id != self.host_note_id {
                self.delegate.restore_desktop_note(id, p.at, p.home);
            }
        }
    }

    /// Whether note `id` is currently parked.
    pub fn is_parked(&self, id: NoteId) -> bool {
        self.parked.borrow().contains_key(&id)
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

    /// Reposition All Notes window to geometry `g`.
    pub fn place(&self, g: Geometry) {
        let sf = self.scale.factor();
        self.ui.window().set_size(slint::PhysicalSize::new(
            (g.w as f32 * sf).round() as u32,
            (g.h as f32 * sf).round() as u32,
        ));
        self.ui.window().set_position(slint::PhysicalPosition::new(
            (g.x as f32 * sf).round() as i32,
            (g.y as f32 * sf).round() as i32,
        ));
    }

    /// Draw the editor (and its Sidebar) at UI size `ui`; the parked notes' places are carried into the
    /// new logical px by `k` (old UI size / new).
    pub fn set_ui_scale(&self, ui: f32, k: f32) {
        let paper = self.paper_geometry().scaled(k);
        self.scale.set_ui(self.ui.window(), ui);
        self.place(expand_for_sidebar(paper));
        self.editor.relayout(&self.ui);
        for p in self.parked.borrow_mut().values_mut() {
            p.home = p.home.scaled(k);
            p.at = p.at.scaled(k);
        }
    }

    pub fn set_tilted_editing(&self, on: bool) {
        self.editor.set_tilted(&self.ui, on);
    }

    pub fn apply_sidebar_layout(&self) {
        let layout = self.db.sidebar_layout(Sidebar::Notes);
        self.ui.set_all_notes_height_mode(layout.height.index());
        self.ui.set_all_notes_anchor(layout.anchor.index());
    }

    pub fn apply_sidebar_theme(&self) {
        self.ui.set_sidebar_theme(self.db.sidebar_theme(Sidebar::Notes).index());
    }

    /// Every parked note, emptied (All Notes closes).
    pub fn take_parked(&self) -> Vec<(NoteId, Parked)> {
        self.parked.borrow_mut().drain().collect()
    }

    /// Put the caret in the editor, at the start of the note (a note just made here).
    pub fn focus_editor(&self) {
        self.editor.focus_start(&self.ui);
    }

    pub fn select_note(&self, id: NoteId) {
        self.set_confirming_delete(false);
        let rows = self.rows.borrow().clone();
        self.refresh_with_rows(&rows, Some(id));
    }

    pub fn open_note(&self, id: NoteId) {
        self.delegate.open_desktop_note(id);
        self.refresh_rows(None);
    }

    pub fn new_note(&self) {
        self.set_confirming_delete(false);
        let from = self.selected_id();
        let new_id = self.delegate.create_new_note(from);
        self.refresh_rows(new_id);
        self.focus_editor();
    }

    pub fn open_or_focus_selected_note(&self) {
        let Some(selected_id) = self.selected_id() else { return };
        self.set_confirming_delete(false);
        self.delegate.open_desktop_note(selected_id);
        self.refresh_rows(None);
    }

    pub fn close_selected_note(&self) {
        let Some(selected_id) = self.selected_id() else { return };
        self.set_confirming_delete(false);
        self.delegate.close_desktop_note(selected_id);
        self.refresh_rows(Some(selected_id));
    }

    pub fn export_selected_note(&self) {
        let Some(selected_id) = self.selected_id() else { return };
        self.delegate.export_note(selected_id);
    }

    pub fn request_delete_selected_note(&self) {
        if self.selected_id().is_some() {
            self.set_confirming_delete(true);
        }
    }

    pub fn cancel_delete(&self) {
        self.set_confirming_delete(false);
    }

    pub fn confirm_delete(&self) {
        let Some(selected_id) = self.selected_id() else { return };
        self.set_confirming_delete(false);
        self.delegate.delete_note(selected_id);
    }

    pub fn invoke_select_note(&self, id: NoteId) {
        self.set_confirming_delete(false);
        self.ui.invoke_all_notes_select_note(id as i32);
    }

    pub fn invoke_open_note(&self, id: NoteId) {
        self.ui.invoke_all_notes_open_note(id as i32);
    }

    pub fn invoke_new_note(&self) {
        self.set_confirming_delete(false);
        self.ui.invoke_all_notes_new_note();
    }

    pub fn invoke_open_or_focus_note(&self) {
        self.set_confirming_delete(false);
        self.ui.invoke_all_notes_open_or_focus();
    }

    pub fn invoke_close_note(&self) {
        self.set_confirming_delete(false);
        self.ui.invoke_all_notes_close_note();
    }

    pub fn invoke_export_note(&self) {
        self.ui.invoke_all_notes_export_note();
    }

    pub fn invoke_request_delete(&self) {
        if self.selected_note_id().is_some() {
            self.set_confirming_delete(true);
            self.ui.invoke_all_notes_request_delete();
        }
    }

    pub fn invoke_cancel_delete(&self) {
        self.set_confirming_delete(false);
        self.ui.invoke_all_notes_cancel_delete();
    }

    pub fn invoke_confirm_delete(&self) {
        self.ui.invoke_all_notes_confirm_delete();
    }

    pub fn invoke_open_dropdown(&self) {
        self.ui.invoke_open_dropdown();
    }

    /// The editor's window (a handle to it).
    pub fn ui(&self) -> NoteWindow {
        self.ui.clone_strong()
    }

    pub fn selected_note_id(&self) -> Option<NoteId> {
        let id = self.ui.get_all_notes_selected_id();
        (id >= 0).then_some(id as NoteId)
    }

    pub fn selected_id(&self) -> Option<NoteId> {
        self.selected_note_id()
    }

    pub fn is_selected_note_open(&self) -> bool {
        self.ui.get_all_notes_selected_is_open()
    }

    pub fn is_confirming_delete(&self) -> bool {
        self.ui.get_all_notes_confirming_delete()
    }

    pub fn confirming_delete(&self) -> bool {
        self.is_confirming_delete()
    }

    pub fn set_confirming_delete(&self, confirming: bool) {
        self.ui.set_all_notes_confirming_delete(confirming);
    }

    pub fn open_count(&self) -> usize {
        self.ui.get_all_notes_open_count().max(0) as usize
    }

    pub fn closed_count(&self) -> usize {
        self.ui.get_all_notes_closed_count().max(0) as usize
    }

    pub fn open_items(&self) -> Vec<AllNoteItem> {
        self.ui.get_all_notes_open_list().iter().collect()
    }

    pub fn closed_items(&self) -> Vec<AllNoteItem> {
        self.ui.get_all_notes_closed_list().iter().collect()
    }

    pub fn open_notes(&self) -> Vec<AllNoteItem> {
        self.open_items()
    }

    pub fn closed_notes(&self) -> Vec<AllNoteItem> {
        self.closed_items()
    }

    pub fn status_text(&self) -> String {
        self.ui.get_all_notes_status_text().to_string()
    }

    pub fn hwnd(&self) -> Option<isize> {
        hwnd_of(&self.ui)
    }

    pub fn dispatch_event(&self, event: slint::platform::WindowEvent) {
        self.ui.window().dispatch_event(event);
    }

    pub fn geometry(&self) -> Geometry {
        let w = self.ui.window();
        let scale = w.scale_factor();
        let (p, s) = (w.position(), w.size());
        Geometry {
            x: (p.x as f32 / scale).round() as i32,
            y: (p.y as f32 / scale).round() as i32,
            w: (s.width as f32 / scale).round() as u32,
            h: (s.height as f32 / scale).round() as u32,
        }
    }

    pub fn menu_button_rect(&self) -> (f32, f32, f32, f32) {
        (
            self.ui.get_menu_button_x(),
            self.ui.get_menu_button_y(),
            self.ui.get_menu_button_width(),
            self.ui.get_menu_button_height(),
        )
    }

    /// Where the paper is: the editor's window less the sidebar's strip (logical px).
    pub fn paper_geometry(&self) -> Geometry {
        let g = self.geometry();
        Geometry {
            x: g.x + SIDEBAR_WIDTH,
            y: g.y,
            w: g.w.saturating_sub(SIDEBAR_WIDTH as u32),
            h: g.h,
        }
    }

    /// Put the text away, give the desktop its window back and drop the editor.
    /// Close: the desktop window of the note shown comes back first, and the editor goes a moment
    /// later, once that window has drawn (so nothing flashes).
    pub fn close(&self) {
        self.flush();
        self.swap_timer.stop();
        self.delegate.set_desktop_note_visible(self.editor.id(), true);
        let ui = self.ui.clone_strong();
        slint::Timer::single_shot(SWAP_OVERLAP, move || {
            let _ = ui.hide();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_for_sidebar() {
        let paper = Geometry { x: 600, y: 100, w: 400, h: 500 };
        let expanded = expand_for_sidebar(paper);
        assert_eq!(expanded.x, 600 - SIDEBAR_WIDTH);
        assert_eq!(expanded.y, 100);
        assert_eq!(expanded.w, 400 + SIDEBAR_WIDTH as u32);
        assert_eq!(expanded.h, 500);
    }

    #[test]
    fn test_paper_geometry_roundtrip() {
        let paper = Geometry { x: 318, y: 100, w: 447, h: 468 };
        let expanded = expand_for_sidebar(paper);
        // paper geometry recovers original position:
        let recovered = Geometry {
            x: expanded.x + SIDEBAR_WIDTH,
            y: expanded.y,
            w: expanded.w.saturating_sub(SIDEBAR_WIDTH as u32),
            h: expanded.h,
        };
        assert_eq!(recovered, paper);
    }

    #[test]
    fn test_clamp_to_screen() {
        let screen = [ScreenRect::new(0, 0, 1920, 1040)];

        // Left overflow (e.g. host at 40 -> expanded at 40 - 282 = -242)
        let off_left = Geometry { x: -242, y: 100, w: 729, h: 468 };
        let clamped_left = clamp_to_screen(off_left, &screen);
        assert_eq!(clamped_left.x, 0, "clamped to left screen edge");
        assert_eq!(clamped_left.y, 100);

        // Right overflow
        let off_right = Geometry { x: 1500, y: 100, w: 729, h: 468 };
        let clamped_right = clamp_to_screen(off_right, &screen);
        assert_eq!(clamped_right.x, 1920 - 729, "clamped to right screen edge");

        // Within screen bounds
        let normal = Geometry { x: 318, y: 100, w: 729, h: 468 };
        let clamped_normal = clamp_to_screen(normal, &screen);
        assert_eq!(clamped_normal, normal, "unchanged when fully inside");

        // Empty areas fallback
        assert_eq!(clamp_to_screen(off_left, &[]), off_left);
    }

    #[test]
    fn test_park_beside_right_and_cascade() {
        let screen = [ScreenRect::new(0, 0, 1920, 1040)];
        let note = Geometry { x: 600, y: 100, w: 447, h: 468 };
        let editor = Geometry { x: 318, y: 100, w: 729, h: 468 };

        let (right, side) = park_beside(note, editor, [0, 0], &screen).expect("parks on right");
        assert_eq!(side, Side::Right);
        assert_eq!(right.x + 42, editor.x + editor.w as i32 - 42 + PARK_GAP);

        // Second note cascades by PARK_CASCADE in x and y
        let (second, side2) = park_beside(note, editor, [1, 0], &screen).expect("cascades");
        assert_eq!(side2, Side::Right);
        assert_eq!(second.x - right.x, PARK_CASCADE);
        assert_eq!(second.y - right.y, PARK_CASCADE);
    }

    #[test]
    fn test_park_beside_left_fallback() {
        let screen = [ScreenRect::new(0, 0, 1920, 1040)];
        let note = Geometry { x: 1432, y: 100, w: 447, h: 468 };
        let editor = Geometry { x: 1150, y: 100, w: 729, h: 468 };

        let (left, side) = park_beside(note, editor, [0, 0], &screen).expect("parks on left");
        assert_eq!(side, Side::Left);
        assert!(left.x + left.w as i32 - 42 <= editor.x - PARK_GAP);
    }

    #[test]
    fn test_park_beside_no_overlap_returns_none() {
        let screen = [ScreenRect::new(0, 0, 1920, 1040)];
        let non_overlapping = Geometry { x: 1400, y: 100, w: 447, h: 468 };
        let editor = Geometry { x: 318, y: 100, w: 729, h: 468 };
        assert_eq!(park_beside(non_overlapping, editor, [0, 0], &screen), None);
    }

    #[test]
    fn test_park_beside_too_narrow_returns_none() {
        let narrow = [ScreenRect::new(0, 0, 900, 1040)];
        let note = Geometry { x: 300, y: 100, w: 447, h: 468 };
        let editor = Geometry { x: 18, y: 100, w: 729, h: 468 };
        assert_eq!(park_beside(note, editor, [0, 0], &narrow), None);
    }

    #[test]
    fn test_format_status_line() {
        let row_open = AllNoteRow {
            id: 1,
            content: "test".into(),
            updated_at: 1000,
            is_open: true,
            z_order: 0,
        };
        let status_open = format_status_line(&row_open, 1000);
        assert_eq!(status_open, "Open on your desktop · edited today");

        let row_closed = AllNoteRow {
            id: 2,
            content: "test".into(),
            updated_at: 1000,
            is_open: false,
            z_order: 0,
        };
        let status_closed = format_status_line(&row_closed, 1000);
        assert_eq!(status_closed, "Closed · edited today");
    }

    #[test]
    fn test_row_partitioning() {
        let rows = vec![
            AllNoteRow { id: 1, content: "# Title 1\nbody".into(), updated_at: 100, is_open: true, z_order: 0 },
            AllNoteRow { id: 2, content: "# Title 2\nbody".into(), updated_at: 200, is_open: false, z_order: 0 },
            AllNoteRow { id: 3, content: "# Title 3\nbody".into(), updated_at: 300, is_open: true, z_order: 0 },
        ];

        let mut open_items = Vec::new();
        let mut closed_items = Vec::new();
        for r in &rows {
            let (title, _) = extract_title_and_preview(&r.content);
            let item = AllNoteItem { id: r.id as i32, title: title.into(), is_open: r.is_open };
            if r.is_open {
                open_items.push(item);
            } else {
                closed_items.push(item);
            }
        }

        assert_eq!(open_items.len(), 2);
        assert_eq!(closed_items.len(), 1);
        assert_eq!(open_items[0].title.as_str(), "Title 1");
        assert_eq!(open_items[1].title.as_str(), "Title 3");
        assert_eq!(closed_items[0].title.as_str(), "Title 2");
    }
}
