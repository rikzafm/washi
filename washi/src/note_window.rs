//! One open note: the Slint window plus its 500 ms autosave debounce.

use crate::platform::{Clock, NoteId, ResizeEdge, ScreenRect};
use crate::storage::{Db, Geometry};
use slint::{CloseRequestResponse, ComponentHandle, Timer, TimerMode};
use crate::document::{Block, Caret, Document, Kind};
use crate::history::History;
use crate::textflow::{self, Style, Tokens};
use slint::{Model, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

slint::include_modules!();

/// Quiet period after the last edit before the note is written to the database.
pub const AUTOSAVE_DELAY: Duration = Duration::from_millis(500);

/// Quiet period after the last move or resize before the position and size are written. Its own
/// timer: typing never delays it, and dragging never delays the text.
pub const GEOMETRY_DELAY: Duration = Duration::from_millis(500);

/// A minimized window reports a position around (-32000, -32000); that is not where the note lives.
fn is_minimized_position(x: i32, y: i32) -> bool {
    x <= -30000 && y <= -30000
}

struct GeometrySave {
    id: NoteId,
    db: Rc<Db>,
    ui: slint::Weak<NoteWindow>,
    /// The last geometry seen while the window was in a normal state.
    last: Cell<Option<Geometry>>,
    dirty: Cell<bool>,
}

impl GeometrySave {
    /// Where the window is and how big, in logical pixels; the last good value while minimized.
    fn current(&self) -> Option<Geometry> {
        let ui = self.ui.upgrade()?;
        let w = ui.window();
        let scale = w.scale_factor();
        let (p, s) = (w.position(), w.size());
        if is_minimized_position(p.x, p.y) {
            return self.last.get();
        }
        let g = Geometry {
            x: (p.x as f32 / scale).round() as i32,
            y: (p.y as f32 / scale).round() as i32,
            w: (s.width as f32 / scale).round() as u32,
            h: (s.height as f32 / scale).round() as u32,
        };
        self.last.set(Some(g));
        Some(g)
    }

    fn flush(&self) {
        if !self.dirty.replace(false) {
            return;
        }
        let Some(g) = self.current() else { return };
        if let Err(e) = self.db.save_geometry(self.id, g) {
            eprintln!("washi: could not save the position of note {}: {e}", self.id);
            self.dirty.set(true);
        }
    }
}

struct Autosave {
    id: NoteId,
    db: Rc<Db>,
    clock: Rc<dyn Clock>,
    ui: slint::Weak<NoteWindow>,
    dirty: Cell<bool>,
}

impl Autosave {
    fn flush(&self) {
        if !self.dirty.replace(false) {
            return;
        }
        let Some(ui) = self.ui.upgrade() else { return };
        if let Err(e) = self.db.save_content(self.id, ui.get_text().as_str(), self.clock.unix_millis()) {
            eprintln!("washi: autosave of note {} failed: {e}", self.id);
            self.dirty.set(true);
        }
    }
}

/// A note's title: its first non-blank text, cut to about 40 characters. `None` for an empty note.
pub fn note_title(content: &str) -> Option<String> {
    const MAX: usize = 40;
    let doc = Document::parse(content);
    let line = doc.blocks().iter().map(|b| b.text.trim()).find(|l| !l.is_empty())?;
    if line.chars().count() <= MAX {
        return Some(line.into());
    }
    let cut: String = line.chars().take(MAX).collect();
    Some(format!("{}…", cut.trim_end()))
}

/// Window title: the note's title, or "Washi" when the note is empty.
pub fn window_title(content: &str) -> String {
    note_title(content).unwrap_or_else(|| "Washi".into())
}

fn app_icon() -> slint::Image {
    const SIZE: u32 = 48;
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(SIZE, SIZE);
    for (dst, src) in buf.make_mut_bytes().chunks_mut(4).zip(crate::icon::rgba(SIZE).chunks(4)) {
        dst.copy_from_slice(src);
    }
    slint::Image::from_rgba8(buf)
}

/// What a note window asks the app to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoteEvent {
    /// The user closed the window itself (taskbar, Alt+F4).
    WindowClosed,
    /// Right click; `at` is in screen pixels.
    ContextMenu { at: (i32, i32) },
    /// The mouse moved over the note (wakes a suspended app).
    PointerMoved,
    /// The pink tape or top margin was pressed: drag the window.
    StartMove,
    /// A resize zone was pressed.
    StartResize(ResizeEdge),
    New,
    Close,
    Delete,
}

/// The note's document, shared between the window's model and the stored text.
struct Doc {
    doc: RefCell<Document>,
    model: Rc<VecModel<BlockData>>,
    runs: Rc<VecModel<RunData>>,
    /// The paper's size in logical px (window minus margins), for the stepped layout.
    paper: Cell<(f64, f64)>,
    /// Undo and redo (ticket 12): the snapshots of this note's document.
    history: RefCell<History<Document>>,
}

/// Typing is snapshotted once it pauses this long; structural changes are snapshotted at once.
pub const HISTORY_PAUSE: Duration = Duration::from_millis(500);

/// The keys the block field sends for Ctrl+Z and Ctrl+Y (see ui/block.slint).
const UNDO_KEY: &str = "\u{1a}";
const REDO_KEY: &str = "\u{19}";

/// The shown text of an empty note.
pub const HINT: &str = "Note Something....";

impl Doc {
    fn new() -> Rc<Doc> {
        Rc::new(Doc { doc: RefCell::new(Document::new()), model: Rc::new(VecModel::default()), runs: Rc::new(VecModel::default()), paper: Cell::new((363.0, 433.0)), history: RefCell::new(History::new(Document::new())) })
    }

    /// Replace everything (opening a note, or text set from outside).
    fn load(&self, content: &str) {
        let parsed = Document::parse(content);
        self.model.set_vec(parsed.blocks().iter().map(block_data).collect::<Vec<_>>());
        *self.history.borrow_mut() = History::new(parsed.clone());
        *self.doc.borrow_mut() = parsed;
    }

    /// Snapshot the document as it is now (typing paused, or an undo is about to step back).
    fn checkpoint(&self) {
        self.history.borrow_mut().record(self.doc.borrow().clone());
    }

    /// A structural change just happened to the document that was `before`: snapshot the typing
    /// before it, then the change.
    fn structural(&self, before: Document) {
        let mut h = self.history.borrow_mut();
        h.record(before);
        h.record(self.doc.borrow().clone());
    }

    /// Ctrl+Z (`redo` false) or Ctrl+Y: show the previous or next snapshot. The caret to place,
    /// or `None` when there is no such snapshot.
    fn step(&self, redo: bool) -> Option<Caret> {
        self.checkpoint();
        let target = {
            let mut h = self.history.borrow_mut();
            if redo { h.redo() } else { h.undo() }?.clone()
        };
        let mut doc = self.doc.borrow_mut();
        let at = doc.blocks().iter().zip(target.blocks()).take_while(|(a, b)| a == b).count();
        let block = at.min(target.len() - 1);
        let offset = target.blocks()[block].text.len();
        *doc = target;
        Some(Caret { block, offset })
    }

    fn tokens(ui: &NoteWindow) -> Tokens {
        let th = ui.global::<Theme>();
        let style = |size: f32, pitch: f32| Style { size: size as f64, pitch: pitch as f64 };
        Tokens {
            inset_left: th.get_text_inset_left() as f64,
            inset_right: th.get_text_inset_right() as f64,
            inset_top: th.get_text_inset_top() as f64,
            heading_gap: th.get_heading_gap() as f64,
            body_gap: th.get_body_gap() as f64,
            title: style(th.get_title_size(), th.get_title_pitch()),
            heading: style(th.get_heading_size(), th.get_heading_pitch()),
            body: style(th.get_body_size(), th.get_body_pitch()),
            tilt_deg: th.get_text_tilt() as f64,
            max_run: th.get_step_width() as f64,
        }
    }

    /// Make the window's blocks match the document, touching only the rows that differ so the
    /// fields that stay keep their focus.
    fn sync_model(&self) {
        let new: Vec<BlockData> = self.doc.borrow().blocks().iter().map(block_data).collect();
        let old: Vec<BlockData> = self.model.iter().collect();
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let suffix = old[prefix..].iter().rev().zip(new[prefix..].iter().rev()).take_while(|(a, b)| a == b).count();
        let (old_mid, new_mid) = (old.len() - prefix - suffix, new.len() - prefix - suffix);
        for k in 0..old_mid.min(new_mid) {
            self.model.set_row_data(prefix + k, new[prefix + k].clone());
        }
        for _ in new_mid..old_mid {
            self.model.remove(prefix + new_mid);
        }
        for k in old_mid..new_mid {
            self.model.insert(prefix + k, new[prefix + k].clone());
        }
    }

    /// Lay the words out again (text or paper size changed) from the tokens in ui/theme.slint.
    fn relayout(&self, ui: &NoteWindow) {
        let tokens = Self::tokens(ui);
        let doc = self.doc.borrow();
        let mut runs = textflow::layout(doc.blocks(), self.paper.get(), &tokens);
        ui.set_content_height(textflow::extent(doc.blocks(), self.paper.get().0, &tokens).1 as f32);
        let hint = doc.is_empty();
        if hint {
            runs = textflow::layout(&[Block::new(Kind::Title, HINT)], self.paper.get(), &tokens);
        }
        self.runs.set_vec(
            runs.iter()
                .map(|r| RunData {
                    text: r.text.as_str().into(),
                    x: r.x as f32,
                    y: r.y as f32,
                    dx: r.dx as f32,
                    dy: r.dy as f32,
                    width: r.width as f32,
                    size: r.size as f32,
                    pitch: r.pitch as f32,
                    bold: r.bold,
                    hint,
                })
                .collect::<Vec<_>>(),
        );
    }

    fn text(&self) -> String {
        self.doc.borrow().to_text()
    }

    /// Scroll the least that brings the caret's line into the text area (the top of the note when
    /// the caret is on its first line).
    fn reveal(&self, ui: &NoteWindow, block: usize, offset: usize) {
        let tokens = Self::tokens(ui);
        let doc = self.doc.borrow();
        let Some((top, bottom)) = textflow::caret_span(doc.blocks(), block, offset, self.paper.get().0, &tokens) else { return };
        let (_, content) = textflow::extent(doc.blocks(), self.paper.get().0, &tokens);
        let view = ui.get_text_view_height() as f64;
        let cur = ui.get_scroll_y() as f64;
        let mut to = if top <= tokens.inset_top + 0.5 { 0.0 } else if top < cur { top } else if bottom > cur + view { bottom - view } else { cur };
        to = to.min((content - view).max(0.0)).max(0.0);
        if (to - cur).abs() > 0.01 {
            ui.set_scroll_y(to as f32);
        }
    }

    /// Block `i` was edited to `text`, the caret now at `cursor`. Line breaks in it (a paste) make
    /// new blocks; then the caret to move to is returned.
    fn edit(&self, i: usize, text: &str, cursor: usize) -> Option<Caret> {
        if !text.contains(['\n', '\r']) {
            self.doc.borrow_mut().set_text(i, text);
            return None;
        }
        let before = self.doc.borrow().clone();
        let mut doc = self.doc.borrow_mut();
        let old = match doc.blocks().get(i) {
            Some(b) if b.is_text() => b.text.clone(),
            _ => return None,
        };
        // the field now holds old[..start] + inserted + old[end..], the caret right after `inserted`
        let mut cursor = cursor.min(text.len());
        while !text.is_char_boundary(cursor) {
            cursor -= 1;
        }
        let mut end = old.len() - (text.len() - cursor).min(old.len());
        while !old.is_char_boundary(end) {
            end += 1;
        }
        let mut start = old.bytes().zip(text.bytes()).take_while(|(a, b)| a == b).count().min(end).min(cursor);
        while !old.is_char_boundary(start) || !text.is_char_boundary(start) {
            start -= 1;
        }
        let caret = doc.insert_text(i, start, end, &text[start..cursor]);
        drop(doc);
        self.structural(before);
        Some(caret)
    }

    /// Enter, Backspace, Delete, Up or Down in block `i` at `offset`. `Some((caret, changed))` when
    /// the note takes the key (the document changed, or only the caret moves to another block);
    /// `None` when the field keeps it.
    fn key(&self, i: usize, key: &str, offset: usize, ui: &NoteWindow) -> Option<(Caret, bool)> {
        use slint::platform::Key;
        let before = self.doc.borrow().clone();
        let mut doc = self.doc.borrow_mut();
        let is = |k: Key| key == char::from(k).to_string();
        let edit = if is(Key::Return) {
            Some(doc.split(i, offset))
        } else if is(Key::Backspace) {
            doc.backspace(i, offset)
        } else if is(Key::Delete) {
            doc.delete(i, offset)
        } else {
            None
        };
        if let Some(c) = edit {
            drop(doc);
            self.structural(before);
            return Some((c, true));
        }
        if is(Key::Return) || is(Key::Backspace) || is(Key::Delete) {
            return None;
        }
        let up = is(Key::UpArrow);
        if !up && !is(Key::DownArrow) {
            return None;
        }
        let tokens = Self::tokens(ui);
        let width = self.paper.get().0;
        let at = textflow::caret_line(doc.blocks().get(i)?, offset, width, &tokens);
        let target = if up {
            if at.line > 0 {
                return None;
            }
            doc.prev_text_block(i)?
        } else {
            if at.line + 1 < at.lines {
                return None;
            }
            doc.next_text_block(i)?
        };
        let line = if up { usize::MAX } else { 0 };
        let to = textflow::offset_at(&doc.blocks()[target], line, at.x, width, &tokens);
        Some((Caret { block: target, offset: to }, false))
    }
}

fn block_data(b: &Block) -> BlockData {
    let kind = match b.kind {
        Kind::Title => BlockKind::Title,
        Kind::Heading => BlockKind::Heading,
        Kind::Body => BlockKind::Body,
        Kind::ListItem => BlockKind::ListItem,
        Kind::Image => BlockKind::Image,
    };
    BlockData { kind, text: b.text.as_str().into(), bold: b.is_bold() }
}

pub struct Note {
    id: NoteId,
    ui: NoteWindow,
    doc: Rc<Doc>,
    autosave: Rc<Autosave>,
    timer: Rc<Timer>,
    /// Snapshots typing once it pauses (stopped while suspended; the next undo snapshots anyway).
    history_timer: Rc<Timer>,
    geometry: Rc<GeometrySave>,
    geometry_timer: Rc<Timer>,
}

impl Note {
    /// Create and show the window. `on_event` runs when the user acts on it.
    pub fn open(
        id: NoteId,
        content: &str,
        db: Rc<Db>,
        clock: Rc<dyn Clock>,
        geometry: Option<Geometry>,
        always_on_top: bool,
        on_event: Rc<dyn Fn(NoteId, NoteEvent)>,
    ) -> Result<Note, slint::PlatformError> {
        let ui = NoteWindow::new()?;
        let doc = Doc::new();
        doc.load(content);
        ui.set_blocks(slint::ModelRc::from(doc.model.clone()));
        ui.set_runs(slint::ModelRc::from(doc.runs.clone()));
        doc.relayout(&ui);
        ui.set_text(doc.text().as_str().into());
        ui.set_window_title(window_title(content).into());
        ui.set_pinned(always_on_top);
        ui.set_app_icon(app_icon());
        if let Some(g) = geometry {
            ui.window().set_size(slint::LogicalSize::new(g.w as f32, g.h as f32));
            ui.window().set_position(slint::LogicalPosition::new(g.x as f32, g.y as f32));
        }
        let autosave = Rc::new(Autosave { id, db, clock, ui: ui.as_weak(), dirty: Cell::new(false) });
        let timer = Rc::new(Timer::default());
        let history_timer = Rc::new(Timer::default());
        // everything that changes the text ends here: relayout, stored text, title, autosave
        let changed: Rc<dyn Fn(Option<Caret>)> = {
            let (autosave, timer, weak, doc) = (autosave.clone(), timer.clone(), ui.as_weak(), doc.clone());
            Rc::new(move |focus: Option<Caret>| {
                let text = doc.text();
                if let Some(ui) = weak.upgrade() {
                    doc.sync_model();
                    doc.relayout(&ui);
                    ui.set_text(text.as_str().into());
                    ui.set_window_title(window_title(&text).into());
                    if let Some(c) = focus {
                        ui.set_focus_block(c.block as i32);
                        ui.set_focus_offset(c.offset as i32);
                        ui.set_focus_serial(ui.get_focus_serial() + 1);
                        doc.reveal(&ui, c.block, c.offset);
                    }
                }
                autosave.dirty.set(true);
                let a = autosave.clone();
                timer.start(TimerMode::SingleShot, AUTOSAVE_DELAY, move || a.flush());
            })
        };
        {
            let (changed, doc, history_timer) = (changed.clone(), doc.clone(), history_timer.clone());
            ui.on_block_edited(move |i, block_text, cursor| {
                let caret = doc.edit(i.max(0) as usize, block_text.as_str(), cursor.max(0) as usize);
                if caret.is_none() {
                    // typing: snapshot when it pauses
                    let doc = doc.clone();
                    history_timer.start(TimerMode::SingleShot, HISTORY_PAUSE, move || doc.checkpoint());
                }
                changed(caret);
            });
        }
        {
            let (changed, doc, weak) = (changed.clone(), doc.clone(), ui.as_weak());
            ui.on_block_key(move |i, key, offset| {
                let Some(ui) = weak.upgrade() else { return false };
                if key == UNDO_KEY || key == REDO_KEY {
                    // taken even with nothing to step to: the field has no history of its own
                    if let Some(caret) = doc.step(key == REDO_KEY) {
                        changed(Some(caret));
                    }
                    return true;
                }
                let Some((caret, edited)) = doc.key(i.max(0) as usize, key.as_str(), offset.max(0) as usize, &ui) else { return false };
                if edited {
                    changed(Some(caret));
                } else {
                    // only the caret moves, to another block
                    ui.set_focus_block(caret.block as i32);
                    ui.set_focus_offset(caret.offset as i32);
                    ui.set_focus_serial(ui.get_focus_serial() + 1);
                    doc.reveal(&ui, caret.block, caret.offset);
                }
                true
            });
        }
        {
            let (doc, weak) = (doc.clone(), ui.as_weak());
            ui.on_caret_moved(move |block, offset| {
                if let Some(ui) = weak.upgrade() {
                    doc.reveal(&ui, block.max(0) as usize, offset.max(0) as usize);
                }
            });
        }
        {
            let (weak, doc) = (ui.as_weak(), doc.clone());
            ui.on_paper_resized(move |w, h| {
                doc.paper.set((w as f64, h as f64));
                if let Some(ui) = weak.upgrade() {
                    doc.relayout(&ui);
                }
            });
        }
        {
            let on_event = on_event.clone();
            ui.window().on_close_requested(move || {
                on_event(id, NoteEvent::WindowClosed);
                CloseRequestResponse::HideWindow
            });
        }
        {
            let (on_event, weak) = (on_event.clone(), ui.as_weak());
            ui.on_context_menu(move |x, y| {
                let Some(ui) = weak.upgrade() else { return };
                let w = ui.window();
                let scale = w.scale_factor();
                let origin = w.position();
                let at = (origin.x + (x * scale) as i32, origin.y + (y * scale) as i32);
                on_event(id, NoteEvent::ContextMenu { at });
            });
        }
        {
            let on_event = on_event.clone();
            ui.on_pointer_moved(move || on_event(id, NoteEvent::PointerMoved));
        }
        {
            let on_event = on_event.clone();
            ui.on_start_move(move || on_event(id, NoteEvent::StartMove));
        }
        {
            let on_event = on_event.clone();
            ui.on_start_resize(move |code| {
                if let Some(edge) = ResizeEdge::from_code(code) {
                    on_event(id, NoteEvent::StartResize(edge));
                }
            });
        }
        {
            let on_event = on_event.clone();
            ui.on_new_note(move || on_event(id, NoteEvent::New));
        }
        {
            let on_event = on_event.clone();
            ui.on_close_note(move || on_event(id, NoteEvent::Close));
        }
        ui.on_delete_note(move || on_event(id, NoteEvent::Delete));
        ui.show()?;
        let geometry = Rc::new(GeometrySave { id, db: autosave.db.clone(), ui: ui.as_weak(), last: Cell::new(None), dirty: Cell::new(false) });
        geometry.current(); // remember where the window opened
        Ok(Note { id, ui, doc, autosave, timer, history_timer, geometry, geometry_timer: Rc::new(Timer::default()) })
    }

    /// The user (or the platform) moved or resized the window: save the new geometry after the
    /// geometry debounce.
    pub fn geometry_changed(&self) {
        self.geometry.dirty.set(true);
        let g = self.geometry.clone();
        self.geometry_timer.start(TimerMode::SingleShot, GEOMETRY_DELAY, move || g.flush());
    }

    /// A restored note must be reachable: if its top strip is on no monitor (it was saved on a
    /// monitor that is gone), move it onto the primary work area. `areas` are physical pixels.
    pub fn keep_on_screen(&self, areas: &[ScreenRect]) {
        let w = self.ui.window();
        let (p, s) = (w.position(), w.size());
        let margin = (self.ui.get_window_margin() * w.scale_factor()).round() as i32;
        let here = ScreenRect::new(p.x, p.y, s.width as i32, s.height as i32);
        if let Some(r) = crate::placement::recover(here, margin, areas) {
            w.set_position(slint::PhysicalPosition::new(r.x, r.y));
            if (r.w, r.h) != (here.w, here.h) {
                w.set_size(slint::PhysicalSize::new(r.w as u32, r.h as u32));
            }
            self.geometry_changed();
        }
    }

    pub fn id(&self) -> NoteId {
        self.id
    }

    /// The native window handle, once the window exists (never in the test harness).
    pub fn hwnd(&self) -> Option<isize> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let window_handle = self.ui.window().window_handle();
        let handle = window_handle.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
            _ => None,
        }
    }

    pub fn ui(&self) -> &NoteWindow {
        &self.ui
    }

    /// Where the window is and how big, in logical pixels.
    pub fn geometry(&self) -> Geometry {
        self.geometry.current().unwrap_or(Geometry { x: 0, y: 0, w: 0, h: 0 })
    }

    pub fn always_on_top(&self) -> bool {
        self.ui.get_pinned()
    }

    pub fn set_always_on_top(&self, on: bool) {
        self.ui.set_pinned(on);
    }

    /// The title the window (and its taskbar button) currently shows.
    pub fn title(&self) -> String {
        self.ui.get_window_title().to_string()
    }

    /// Replace the note's text without typing it.
    pub fn set_text(&self, text: &str) {
        // the fields of the old blocks vanish without saying they lost the focus
        self.ui.set_text_focused_block(-1);
        self.ui.set_focus_block(-1);
        self.doc.load(text);
        self.doc.relayout(&self.ui);
        self.ui.set_text(self.doc.text().as_str().into());
        self.ui.set_window_title(window_title(text).into());
    }

    pub fn text(&self) -> String {
        self.ui.get_text().to_string()
    }

    /// Suspension: stop this note's timers and its caret blink (a window that is not active
    /// does not blink). Unsaved edits must have been flushed first.
    pub fn pause(&self) {
        self.timer.stop();
        self.history_timer.stop();
        self.geometry_timer.stop();
        // the text keeps its look (editing or resting) while nobody can see it: no redraw
        self.ui.set_suspended(true);
        self.ui.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(false));
    }

    /// Waking: draw the whole note again; `focused` notes get their caret blink back.
    pub fn resume(&self, focused: bool) {
        self.ui.set_suspended(false);
        // fields that lost the focus while suspended say so now
        self.ui.set_text_resync(self.ui.get_text_resync() + 1);
        if focused {
            self.ui.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        }
        self.ui.window().request_redraw();
    }

    /// Save now if there are unsaved edits (used on close and quit).
    pub fn flush(&self) {
        self.timer.stop();
        self.autosave.flush();
        self.geometry_timer.stop();
        self.geometry.flush();
    }
}

impl Drop for Note {
    /// Release the window (and with it the native window and its buffers).
    fn drop(&mut self) {
        let _ = self.ui.hide();
    }
}
