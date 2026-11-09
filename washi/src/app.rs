//! The app: opens the encrypted store, shows the open notes, and owns their lifecycle.

use crate::all_notes::AllNotesView;
use crate::delete_dialog::{calculate_dialog_position, DeleteDialog};
use crate::document::{Align, IMAGE_PCT_MAX, IMAGE_PCT_MIN};
use crate::dropdown::{calculate_dropdown_position, extract_title_and_preview, format_relative_timestamp, shorten_preview, DropdownMenu, DropdownMetrics, Rect as DropdownRect};
use crate::images::{self, ImageError, Source, Stored};
use crate::image_viewer::{ImageViewer, ViewerEvent};
use crate::note_window::{app_icon, note_title, Note, NoteEvent};
use crate::platform::*;
use crate::suspend::{SuspendController, SuspendHost};
use crate::storage::{keystore, AllNoteRow, Db, DbError, Geometry, KeyStoreError, NoteRow};
use slint::ComponentHandle;
use std::cell::{Cell, RefCell};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// Everything Washi takes from the outside world. Real Windows pieces in `crate::win`,
/// fakes in the `snap` harness.
#[derive(Clone)]
pub struct Env {
    /// Folder holding `notes.db` and `key.bin` (`%APPDATA%\Washi` in the real app).
    pub data_dir: PathBuf,
    pub protector: Rc<dyn KeyProtector>,
    pub clock: Rc<dyn Clock>,
    pub signals: Rc<dyn WindowSignals>,
    pub power: Rc<dyn ProcessPower>,
    pub menu: Rc<dyn ContextMenu>,
    pub startup: Rc<dyn StartupRegistration>,
    pub instance: Rc<dyn SingleInstance>,
    pub dialogs: Rc<dyn Dialogs>,
    pub clipboard: Rc<dyn ImageClipboard>,
    pub window_control: Rc<dyn WindowControl>,
}

#[derive(Debug)]
pub enum StartError {
    Key(KeyStoreError),
    Db(DbError),
    Io(std::io::Error),
    Ui(slint::PlatformError),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartError::Key(e) => e.fmt(f),
            StartError::Db(e) => e.fmt(f),
            StartError::Io(e) => write!(f, "Washi could not prepare its data folder: {e}"),
            StartError::Ui(e) => write!(f, "Washi could not open a note window: {e}"),
        }
    }
}
impl std::error::Error for StartError {}

/// A note closed within the exit window (30s) as a candidate for reopening if the app exits.
#[derive(Clone, Debug, PartialEq)]
pub struct ClosedCandidate {
    pub id: NoteId,
    pub closed_at: Duration,
    pub was_empty: bool,
    pub geometry: Option<Geometry>,
}

struct Inner {
    env: Env,
    db: Rc<Db>,
    notes: RefCell<Vec<Rc<Note>>>,
    quit: Cell<bool>,
    suspend: Rc<SuspendController>,
    /// Candidates for reopening if the app exits via consecutive closes within 30s.
    exit_candidates: RefCell<Vec<ClosedCandidate>>,
    exit_timer: slint::Timer,
    /// Stacking order (bottom to top) captured when exit candidate sequence begins.
    exit_stacking: RefCell<Vec<NoteId>>,
    /// Pictures being prepared on their worker threads, and the timer that collects them (it runs
    /// only while there are any).
    imports: RefCell<Vec<Import>>,
    import_timer: slint::Timer,
    /// Retries `attach_windows` until every note window exists (at startup and after a note opens).
    attach_timer: slint::Timer,
    /// Hands idle memory back a moment after the notes settle (see `schedule_trim`).
    trim_timer: slint::Timer,
    /// Set by the real app (`watch_window_state`); the test harness never trims.
    trim_enabled: Cell<bool>,
    /// Open picture viewers (double click on a picture), by the number each got when it opened.
    viewers: RefCell<Vec<(u64, Rc<ImageViewer>)>>,
    next_viewer: Cell<u64>,
    /// Currently open dropdown menu floating window, if any.
    dropdown: RefCell<Option<Rc<DropdownMenu>>>,
    /// Currently open delete confirmation dialog window, if any.
    delete_dialog: RefCell<Option<Rc<DeleteDialog>>>,
    /// Currently open All Notes window, if any.
    all_notes: RefCell<Option<Rc<AllNotesView>>>,
}

/// A picture on its way into a note.
struct Import {
    note: NoteId,
    /// The block it goes after.
    after: usize,
    result: Receiver<Result<Stored, ImageError>>,
}

/// How often the worker threads' results are looked for while a picture is being prepared.
const IMPORT_POLL: Duration = Duration::from_millis(20);
/// Quiet time after the last window opened before the working set is trimmed once.
const TRIM_DELAY: Duration = Duration::from_millis(1500);
/// If notes are closed within this window, they are grouped as candidates for app exit.
pub const EXIT_WINDOW: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct App(Rc<Inner>);

const MENU_NEW: u32 = 1;
const MENU_OPEN: u32 = 2;
const MENU_CLOSE: u32 = 3;
const MENU_DELETE: u32 = 4;
const MENU_TOP: u32 = 5;
const MENU_STARTUP: u32 = 6;
const MENU_EXPORT: u32 = 7;
const MENU_PASTE: u32 = 8;
const MENU_PASTE_IMAGE: u32 = 9;
const MENU_FADE: u32 = 10;
/// The menu of a picture: cut it, copy it, delete it, align it (`MENU_IMAGE_ALIGN + 0|1|2` for left, center, right),
/// or show it `n` percent wide (`MENU_IMAGE_SIZE + n`).
const MENU_IMAGE_CUT: u32 = 18;
const MENU_IMAGE_COPY: u32 = 19;
const MENU_IMAGE_DELETE: u32 = 20;
const MENU_IMAGE_ALIGN: u32 = 21;
const MENU_IMAGE_SIZE: u32 = 200;
const IMAGE_SIZES: [u8; 4] = [25, 50, 75, 100];
/// Closed notes in the "Open note" submenu get `MENU_OPEN_BASE + position`.
const MENU_OPEN_BASE: u32 = 1000;

/// A new note opens this far (logical px, right and down) from the note it was made from.
pub const NEW_NOTE_OFFSET: i32 = 30;

impl App {
    /// The real entry point: make sure this is the only Washi, then start. `Ok(None)` means
    /// Washi was already running, has been asked to open a new note, and this process should exit.
    pub fn launch(env: Env) -> Result<Option<App>, StartError> {
        if env.instance.acquire() == Launch::HandedOff {
            return Ok(None);
        }
        let instance = env.instance.clone();
        let app = App::start(env)?;
        let weak = Rc::downgrade(&app.0);
        instance.listen(Rc::new(move || {
            if let Some(inner) = weak.upgrade() {
                App(inner).new_note_on_request();
            }
        }));
        Ok(Some(app))
    }

    /// A later launch asked for a note: open an empty one next to the newest open note.
    fn new_note_on_request(&self) {
        self.new_note();
    }

    pub fn start(env: Env) -> Result<App, StartError> {
        std::fs::create_dir_all(&env.data_dir).map_err(StartError::Io)?;
        let now = env.clock.unix_millis();
        let key = keystore::load_or_create(&env.data_dir.join("key.bin"), &*env.protector).map_err(StartError::Key)?;
        let db = Db::open(&env.data_dir.join("notes.db"), &key, now).map_err(StartError::Db)?;
        drop(key); // zeroized: the raw key is wiped as soon as the database is unlocked
        let db = Rc::new(db);

        let mut rows = db.open_notes().map_err(StartError::Db)?;
        if rows.is_empty() {
            let id = db.create_note(now).map_err(StartError::Db)?;
            rows.push(NoteRow { id, content: String::new(), geometry: None, always_on_top: false });
        }
        let suspend = SuspendController::new(env.signals.clone(), env.power.clone());
        let inner = Rc::new(Inner {
            env,
            db,
            notes: RefCell::new(Vec::new()),
            quit: Cell::new(false),
            suspend,
            exit_candidates: RefCell::new(Vec::new()),
            exit_timer: slint::Timer::default(),
            exit_stacking: RefCell::new(Vec::new()),
            imports: RefCell::new(Vec::new()),
            import_timer: slint::Timer::default(),
            attach_timer: slint::Timer::default(),
            trim_timer: slint::Timer::default(),
            trim_enabled: Cell::new(false),
            viewers: RefCell::new(Vec::new()),
            next_viewer: Cell::new(0),
            dropdown: RefCell::new(None),
            delete_dialog: RefCell::new(None),
            all_notes: RefCell::new(None),
        });
        let host: Rc<dyn SuspendHost> = inner.clone();
        inner.suspend.attach(Rc::downgrade(&host));
        // Windows ending the session (shutdown, restart, sign-out): save everything before returning
        let weak = Rc::downgrade(&inner);
        inner.env.signals.set_session_end_handler(Rc::new(move || {
            if let Some(inner) = weak.upgrade() {
                inner.save_all();
                inner.restore_exit_candidates();
            }
        }));
        let app = App(inner);
        for row in &rows {
            app.open_row(row)?;
        }
        app.0.suspend.evaluate();
        Ok(app)
    }

    /// Show a window for a stored note.
    fn open_row(&self, row: &NoteRow) -> Result<(), StartError> {
        let weak = Rc::downgrade(&self.0);
        let on_event: Rc<dyn Fn(NoteId, NoteEvent)> = Rc::new(move |id, ev| {
            if let Some(inner) = weak.upgrade() {
                App(inner).handle(id, ev);
            }
        });
        let weak = Rc::downgrade(&self.0);
        let paste_image: Rc<dyn Fn(NoteId) -> bool> = Rc::new(move |id| weak.upgrade().is_some_and(|inner| App(inner).paste_image(id)));
        let note = Note::open(
            row.id,
            &row.content,
            self.0.db.clone(),
            self.0.env.clock.clone(),
            row.geometry,
            row.always_on_top,
            on_event,
            paste_image,
            self.0.env.clipboard.clone(),
        )
        .map_err(StartError::Ui)?;
        note.set_fade_edge(self.0.db.fade_text_edge());
        if row.geometry.is_some() {
            // a note saved on a monitor that is gone comes back on the main screen
            note.keep_on_screen(&self.0.env.window_control.work_areas());
        }
        match note.hwnd() {
            Some(hwnd) => self.0.env.signals.attach(row.id, hwnd),
            // Slint creates the native window once its event loop runs: `attach_windows` retries then
            None => {
                crate::trace::mark("hwnd-pending");
                // at startup `watch_window_state` starts the retries; a note opened later starts them here
                if self.0.trim_enabled.get() {
                    self.start_attaching();
                }
            }
        }
        self.0.notes.borrow_mut().push(Rc::new(note));
        self.0.suspend.evaluate();
        self.schedule_trim();
        Ok(())
    }

    fn handle(&self, id: NoteId, ev: NoteEvent) {
        match ev {
            NoteEvent::WindowClosed | NoteEvent::Close => self.close_note(id),
            NoteEvent::New => self.new_note_from(id),
            NoteEvent::Delete => self.delete_note(id),
            NoteEvent::InsertImage => {
                self.dismiss_dropdown();
                self.insert_image_from_file(id);
            }
            NoteEvent::OpenImage(block) => {
                self.dismiss_dropdown();
                self.open_viewer(id, block);
            }
            NoteEvent::ContextMenu { at, image } => {
                self.dismiss_dropdown();
                self.show_menu(id, at, image);
            }
            NoteEvent::PointerMoved => self.0.suspend.poke(),
            NoteEvent::PointerDown => {
                self.dismiss_dropdown();
            }
            NoteEvent::StartMove => {
                self.dismiss_dropdown();
                let hwnd = self.with_note(id, |n| n.hwnd()).flatten();
                self.0.env.window_control.begin_move(id, hwnd);
            }
            NoteEvent::StartResize(edge) => {
                self.dismiss_dropdown();
                let hwnd = self.with_note(id, |n| n.hwnd()).flatten();
                self.0.env.window_control.begin_resize(id, hwnd, edge);
            }
            NoteEvent::OpenDropdown => {
                self.toggle_dropdown(id);
            }
            NoteEvent::TextChanged => {
                if self.is_all_notes_open() {
                    self.refresh_all_notes(None);
                }
            }
        }
    }

    /// Show the picture of `block` of note `id` in a window of its own.
    fn open_viewer(&self, id: NoteId, block: usize) {
        let Some((picture, size)) = self.with_note(id, |n| n.picture_at(block)).flatten() else { return };
        let rgba = self.with_note(id, |n| n.image_rgba(block)).flatten();
        let caption = self.saved_text(id).and_then(|t| note_title(&t)).unwrap_or_else(|| "Picture".to_string());
        let number = self.0.next_viewer.get();
        self.0.next_viewer.set(number + 1);
        let weak = Rc::downgrade(&self.0);
        let on_event: Rc<dyn Fn(ViewerEvent)> = Rc::new(move |ev| {
            if let Some(inner) = weak.upgrade() {
                if ev == ViewerEvent::Copy {
                    if let Some(ref img) = rgba {
                        let _ = inner.env.clipboard.set_image(img);
                    }
                    return;
                }
                App(inner).viewer_event(number, ev);
            }
        });
        let max = self.0.env.window_control.work_areas().first().map(|r| (r.w as f32, r.h as f32));
        match ImageViewer::open(picture, size, &caption, max, app_icon(), on_event) {
            Ok(viewer) => self.0.viewers.borrow_mut().push((number, Rc::new(viewer))),
            Err(e) => eprintln!("washi: could not open the picture viewer: {e}"),
        }
    }

    fn viewer_event(&self, number: u64, ev: ViewerEvent) {
        let viewer = self.0.viewers.borrow().iter().find(|(n, _)| *n == number).map(|(_, v)| v.clone());
        let Some(viewer) = viewer else { return };
        match ev {
            ViewerEvent::Close => self.0.viewers.borrow_mut().retain(|(n, _)| *n != number),
            ViewerEvent::StartMove => self.0.env.window_control.begin_move(0, viewer.hwnd()),
            ViewerEvent::StartResize(edge) => self.0.env.window_control.begin_resize(0, viewer.hwnd(), edge),
            ViewerEvent::Copy => {}
        }
    }

    /// The platform saw a note move or resize: remember where it is once the user stops.
    pub fn geometry_changed(&self, id: NoteId) {
        self.dismiss_dropdown_if(id);
        self.with_note(id, |n| n.geometry_changed());
    }

    /// Clone the handle to the open dropdown menu, if any.
    fn dropdown(&self) -> Option<Rc<DropdownMenu>> {
        self.0.dropdown.borrow().clone()
    }

    /// Whether the dropdown menu is open for any note.
    pub fn is_dropdown_open(&self) -> bool {
        self.0.dropdown.borrow().is_some()
    }

    /// The note whose dropdown menu is currently open, if any.
    pub fn dropdown_note_id(&self) -> Option<NoteId> {
        self.0.dropdown.borrow().as_ref().map(|d| d.note_id())
    }

    /// The pinned state of the currently open dropdown menu, if any.
    pub fn dropdown_pinned(&self) -> Option<bool> {
        self.0.dropdown.borrow().as_ref().map(|d| d.pinned())
    }

    /// Invoke the toggle pin callback on the active dropdown (for testing).
    pub fn invoke_dropdown_toggle_pin(&self) {
        if let Some(d) = self.dropdown() {
            d.invoke_toggle_pin();
        }
    }

    /// Invoke the delete note callback on the active dropdown (for testing).
    pub fn invoke_dropdown_delete_note(&self) {
        if let Some(d) = self.dropdown() {
            d.invoke_delete_note();
        }
    }

    /// Invoke the new note callback on the active dropdown (for testing).
    pub fn invoke_dropdown_new_note(&self) {
        if let Some(d) = self.dropdown() {
            d.invoke_new_note();
        }
    }

    /// Invoke the all notes callback on the active dropdown (for testing).
    pub fn invoke_dropdown_all_notes(&self) {
        if let Some(d) = self.dropdown() {
            d.invoke_all_notes();
        }
    }

    /// Clone the handle to the open All Notes view, if any.
    pub fn all_notes(&self) -> Option<Rc<AllNotesView>> {
        self.0.all_notes.borrow().clone()
    }

    /// Whether the All Notes window is currently open.
    pub fn is_all_notes_open(&self) -> bool {
        self.0.all_notes.borrow().is_some()
    }

    /// The currently selected note ID in the open All Notes window, if any.
    pub fn all_notes_selected_id(&self) -> Option<NoteId> {
        self.all_notes().and_then(|v| v.selected_note_id())
    }

    /// Open note count displayed in the open All Notes window, if any.
    pub fn all_notes_open_count(&self) -> Option<usize> {
        self.all_notes().map(|v| v.open_count())
    }

    /// Closed note count displayed in the open All Notes window, if any.
    pub fn all_notes_closed_count(&self) -> Option<usize> {
        self.all_notes().map(|v| v.closed_count())
    }

    /// Open note items in the open All Notes window.
    pub fn all_notes_open_items(&self) -> Vec<crate::note_window::AllNoteItem> {
        self.all_notes().map(|v| v.open_notes()).unwrap_or_default()
    }

    /// Closed note items in the open All Notes window.
    pub fn all_notes_closed_items(&self) -> Vec<crate::note_window::AllNoteItem> {
        self.all_notes().map(|v| v.closed_notes()).unwrap_or_default()
    }

    /// Status line text in the open All Notes window, if any.
    pub fn all_notes_status_text(&self) -> Option<String> {
        self.all_notes().map(|v| v.status_text())
    }

    /// Preview text runs in the open All Notes window.
    pub fn all_notes_preview_runs(&self) -> Vec<crate::note_window::PreviewRun> {
        self.all_notes().map(|v| v.preview_runs()).unwrap_or_default()
    }

    /// Whether the inline delete confirmation bar is active in All Notes.
    pub fn all_notes_confirming_delete(&self) -> Option<bool> {
        self.all_notes().map(|v| v.confirming_delete())
    }

    /// Set the inline delete confirmation state in All Notes (for testing).
    pub fn set_all_notes_confirming_delete(&self, confirming: bool) {
        if let Some(v) = self.all_notes() {
            v.set_confirming_delete(confirming);
        }
    }

    /// Select note `id` in the open All Notes window and reset inline delete confirmation.
    pub fn select_all_notes_note(&self, id: NoteId) {
        if let Some(v) = self.all_notes() {
            v.set_confirming_delete(false);
            let rows = self.all_notes_rows();
            let now_ms = self.0.env.clock.unix_millis();
            v.refresh(&rows, Some(id), now_ms);
        }
    }

    /// Create a new note from the All Notes window, refresh All Notes, and select the new note.
    pub fn all_notes_new_note(&self) {
        if let Some(v) = self.all_notes() {
            v.set_confirming_delete(false);
        }
        let from = self.dropdown_note_id().or_else(|| self.0.notes.borrow().last().map(|n| n.id()));
        let new_id = self.create_new_note(from);
        self.refresh_all_notes(new_id);
    }

    /// Open or focus the currently selected note in the All Notes window.
    pub fn all_notes_open_or_focus(&self) {
        let Some(selected_id) = self.all_notes_selected_id() else { return };
        if let Some(v) = self.all_notes() {
            v.set_confirming_delete(false);
        }
        self.activate_note(selected_id);
    }

    /// Close the desktop window of the currently selected note in the All Notes window.
    pub fn all_notes_close_note(&self) {
        let Some(selected_id) = self.all_notes_selected_id() else { return };
        if let Some(v) = self.all_notes() {
            v.set_confirming_delete(false);
        }
        if self.with_note(selected_id, |_| ()).is_some() {
            self.close_note(selected_id);
        } else {
            self.refresh_all_notes(Some(selected_id));
        }
    }

    /// Export the currently selected note in the All Notes window as Markdown.
    pub fn all_notes_export_note(&self) {
        let Some(selected_id) = self.all_notes_selected_id() else { return };
        self.export_note(selected_id);
    }

    /// Request inline delete confirmation for the currently selected note in the All Notes window.
    pub fn all_notes_request_delete(&self) {
        if self.all_notes_selected_id().is_some() {
            self.set_all_notes_confirming_delete(true);
        }
    }

    /// Cancel inline delete confirmation in the All Notes window.
    pub fn all_notes_cancel_delete(&self) {
        self.set_all_notes_confirming_delete(false);
    }

    /// Permanently delete the currently selected note in the All Notes window and select the next available note.
    pub fn all_notes_confirm_delete(&self) {
        let Some(selected_id) = self.all_notes_selected_id() else { return };
        self.set_all_notes_confirming_delete(false);
        self.confirm_delete_note(selected_id);
    }

    /// Invoke the select-note callback on the open All Notes window (for testing).
    pub fn invoke_all_notes_select_note(&self, id: NoteId) {
        if let Some(v) = self.all_notes() {
            v.invoke_select_note(id);
        }
    }

    /// Invoke the new-note callback on the open All Notes window (for testing).
    pub fn invoke_all_notes_new_note(&self) {
        if let Some(v) = self.all_notes() {
            v.invoke_new_note();
        }
    }

    /// Invoke the open-or-focus-note callback on the open All Notes window (for testing).
    pub fn invoke_all_notes_open_or_focus(&self) {
        if let Some(v) = self.all_notes() {
            v.invoke_open_or_focus_note();
        }
    }

    /// Invoke the close-note callback on the open All Notes window (for testing).
    pub fn invoke_all_notes_close_note(&self) {
        if let Some(v) = self.all_notes() {
            v.invoke_close_note();
        }
    }

    /// Invoke the export-note callback on the open All Notes window (for testing).
    pub fn invoke_all_notes_export_note(&self) {
        if let Some(v) = self.all_notes() {
            v.invoke_export_note();
        }
    }

    /// Invoke the request-delete callback on the open All Notes window (for testing).
    pub fn invoke_all_notes_request_delete(&self) {
        if let Some(v) = self.all_notes() {
            v.invoke_request_delete();
        }
    }

    /// Invoke the cancel-delete callback on the open All Notes window (for testing).
    pub fn invoke_all_notes_cancel_delete(&self) {
        if let Some(v) = self.all_notes() {
            v.invoke_cancel_delete();
        }
    }

    /// Invoke the confirm-delete callback on the open All Notes window (for testing).
    pub fn invoke_all_notes_confirm_delete(&self) {
        if let Some(v) = self.all_notes() {
            v.invoke_confirm_delete();
        }
    }

    /// Dispatch a window event to the active All Notes window (for testing).
    pub fn dispatch_all_notes_event(&self, event: slint::platform::WindowEvent) {
        if let Some(v) = self.all_notes() {
            v.dispatch_event(event);
        }
    }

    /// Close the open All Notes window, if any.
    pub fn dismiss_all_notes(&self) {
        if let Some(v) = self.0.all_notes.borrow_mut().take() {
            v.close();
            if self.0.notes.borrow().is_empty() {
                self.0.restore_exit_candidates();
                self.0.quit.set(true);
                let _ = slint::quit_event_loop();
            }
        }
    }

    fn all_notes_rows(&self) -> Vec<AllNoteRow> {
        for n in self.0.notes.borrow().iter() {
            n.flush_text();
        }
        self.0.save_stacking();
        let mut rows = self.0.db.all_notes().unwrap_or_default();
        for r in &mut rows {
            if let Some(live) = self.with_note(r.id, |n| n.text()) {
                r.content = live;
            }
        }
        rows
    }

    /// Refresh the open All Notes window if it is open.
    pub fn refresh_all_notes(&self, preferred_selected_id: Option<NoteId>) {
        if let Some(v) = self.all_notes() {
            let rows = self.all_notes_rows();
            let now_ms = self.0.env.clock.unix_millis();
            v.refresh(&rows, preferred_selected_id, now_ms);
        }
    }

    /// Open the All Notes window with `initial_selected_id` selected.
    pub fn open_all_notes(&self, initial_selected_id: NoteId) {
        let rows = self.all_notes_rows();
        let now_ms = self.0.env.clock.unix_millis();
        if let Some(v) = self.all_notes() {
            v.refresh(&rows, Some(initial_selected_id), now_ms);
            let _ = v.ui().show();
            self.0.env.window_control.bring_to_front(0, v.hwnd());
            return;
        }

        let weak = Rc::downgrade(&self.0);
        let on_close = Rc::new(move || {
            if let Some(inner) = weak.upgrade() {
                App(inner).dismiss_all_notes();
            }
        });

        match AllNotesView::open(initial_selected_id, &rows, now_ms, on_close) {
            Ok(view) => {
                {
                    let weak = Rc::downgrade(&self.0);
                    view.ui().on_all_notes_select_note(move |selected_id| {
                        if let Some(inner) = weak.upgrade() {
                            App(inner).select_all_notes_note(selected_id as NoteId);
                        }
                    });
                }
                {
                    let weak = Rc::downgrade(&self.0);
                    view.ui().on_all_notes_new_note(move || {
                        if let Some(inner) = weak.upgrade() {
                            App(inner).all_notes_new_note();
                        }
                    });
                }
                {
                    let weak = Rc::downgrade(&self.0);
                    view.ui().on_all_notes_open_or_focus(move || {
                        if let Some(inner) = weak.upgrade() {
                            App(inner).all_notes_open_or_focus();
                        }
                    });
                }
                {
                    let weak = Rc::downgrade(&self.0);
                    view.ui().on_all_notes_close_note(move || {
                        if let Some(inner) = weak.upgrade() {
                            App(inner).all_notes_close_note();
                        }
                    });
                }
                {
                    let weak = Rc::downgrade(&self.0);
                    view.ui().on_all_notes_export_note(move || {
                        if let Some(inner) = weak.upgrade() {
                            App(inner).all_notes_export_note();
                        }
                    });
                }
                {
                    let weak = Rc::downgrade(&self.0);
                    view.ui().on_all_notes_request_delete(move || {
                        if let Some(inner) = weak.upgrade() {
                            App(inner).all_notes_request_delete();
                        }
                    });
                }
                {
                    let weak = Rc::downgrade(&self.0);
                    view.ui().on_all_notes_cancel_delete(move || {
                        if let Some(inner) = weak.upgrade() {
                            App(inner).all_notes_cancel_delete();
                        }
                    });
                }
                {
                    let weak = Rc::downgrade(&self.0);
                    view.ui().on_all_notes_confirm_delete(move || {
                        if let Some(inner) = weak.upgrade() {
                            App(inner).all_notes_confirm_delete();
                        }
                    });
                }
                *self.0.all_notes.borrow_mut() = Some(Rc::new(view));
            }
            Err(e) => eprintln!("washi: could not open All Notes window: {e}"),
        }
    }

    /// The list of notes in the currently open dropdown menu.
    pub fn dropdown_notes(&self) -> Vec<crate::note_window::DropdownNoteItem> {
        self.dropdown().map(|d| d.notes()).unwrap_or_default()
    }

    /// Invoke the select note callback on the active dropdown (for testing).
    pub fn invoke_dropdown_select_note(&self, id: NoteId) {
        if let Some(d) = self.dropdown() {
            d.invoke_select_note(id);
        }
    }

    /// The bounding rectangle of the menu button of note `id` in window logical coordinates.
    pub fn menu_button_rect(&self, id: NoteId) -> Option<(f32, f32, f32, f32)> {
        self.with_note(id, |n| n.menu_button_rect())
    }

    /// Opacity of the menu button of note `id`.
    pub fn menu_button_opacity(&self, id: NoteId) -> Option<f32> {
        self.with_note(id, |n| n.menu_button_opacity())
    }

    /// Whether the menu button of note `id` is currently shown.
    pub fn menu_button_shown(&self, id: NoteId) -> Option<bool> {
        self.with_note(id, |n| n.menu_button_shown())
    }

    /// The screen position of the open dropdown menu window in logical pixels, if any.
    pub fn dropdown_position(&self) -> Option<(f32, f32)> {
        self.dropdown().map(|d| d.position())
    }

    /// The size of the open dropdown menu window in logical pixels, if any.
    pub fn dropdown_size(&self) -> Option<(f32, f32)> {
        self.dropdown().map(|d| d.size())
    }

    /// Dispatch a window event to the active dropdown window (for testing).
    pub fn dispatch_dropdown_event(&self, event: slint::platform::WindowEvent) {
        if let Some(d) = self.dropdown() {
            d.dispatch_event(event);
        }
    }

    /// Invoke dismiss on the active dropdown (for testing).
    pub fn invoke_dropdown_dismiss(&self) {
        if let Some(d) = self.dropdown() {
            d.invoke_dismiss();
        }
    }

    /// Dismiss the open dropdown menu and reset the parent note's button state.
    pub fn dismiss_dropdown(&self) {
        let menu = self.0.dropdown.borrow_mut().take();
        if let Some(m) = menu {
            m.close();
            let note_id = m.note_id();
            self.with_note(note_id, |n| n.set_dropdown_open(false));
        }
    }

    /// Dismiss the open dropdown menu if it belongs to note `id`.
    fn dismiss_dropdown_if(&self, id: NoteId) {
        if self.dropdown_note_id() == Some(id) {
            self.dismiss_dropdown();
        }
    }

    /// Toggle the dropdown menu for note `id`.
    pub fn toggle_dropdown(&self, id: NoteId) {
        if let Some(open_id) = self.dropdown_note_id() {
            if open_id == id {
                self.dismiss_dropdown();
                return;
            }
            self.dismiss_dropdown();
        }
        self.open_dropdown(id);
    }

    /// Open the dropdown menu for note `id`.
    pub fn open_dropdown(&self, id: NoteId) {
        for n in self.0.notes.borrow().iter() {
            n.flush_text();
        }

        let note = self.0.notes.borrow().iter().find(|n| n.id() == id).cloned();
        let Some(note) = note else { return };

        let geom = note.geometry();
        let (btn_x, btn_y, btn_w, btn_h) = note.menu_button_rect();
        let button_screen = DropdownRect::new(
            geom.x as f32 + btn_x,
            geom.y as f32 + btn_y,
            btn_w,
            btn_h,
        );

        let metrics = DropdownMetrics::default();

        let work_areas = self.0.env.window_control.work_areas();
        let scale = note.ui().window().scale_factor();
        let work_area = work_areas.first().map(|wa| DropdownRect::new(
            wa.x as f32 / scale,
            wa.y as f32 / scale,
            wa.w as f32 / scale,
            wa.h as f32 / scale,
        ));

        let pos = calculate_dropdown_position(button_screen, &metrics, work_area);

        let weak = Rc::downgrade(&self.0);
        let on_dismiss = Rc::new(move |_id| {
            if let Some(inner) = weak.upgrade() {
                App(inner).dismiss_dropdown();
            }
        });

        let owner_hwnd = note.hwnd();
        match DropdownMenu::open(id, pos, owner_hwnd, on_dismiss) {
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

                note.set_dropdown_open(true);
                *self.0.dropdown.borrow_mut() = Some(Rc::new(menu));
            }
            Err(e) => eprintln!("washi: could not open dropdown menu: {e}"),
        }
    }

    pub fn env(&self) -> &Env {
        &self.0.env
    }

    pub fn note_ids(&self) -> Vec<NoteId> {
        self.0.notes.borrow().iter().map(|n| n.id()).collect()
    }

    pub fn with_note<R>(&self, id: NoteId, f: impl FnOnce(&Note) -> R) -> Option<R> {
        // Clone the handle out first: `f` may trigger window callbacks that re-enter the app.
        let note = self.0.notes.borrow().iter().find(|n| n.id() == id).cloned();
        note.map(|n| f(&n))
    }

    /// The text last written to the database for a note (`None` once it was deleted).
    pub fn saved_text(&self, id: NoteId) -> Option<String> {
        self.0.db.content(id).ok().flatten()
    }

    /// Position and size last written to the database for a note.
    pub fn saved_geometry(&self, id: NoteId) -> Option<Geometry> {
        self.0.db.note(id).ok().flatten().and_then(|r| r.geometry)
    }

    pub fn schema_version(&self) -> u32 {
        self.0.db.schema_version().unwrap_or(0)
    }

    pub fn pragma(&self, name: &str) -> String {
        self.0.db.pragma_string(name).unwrap_or_default()
    }

    /// The platform saw a note minimize, restore, gain focus or get covered or uncovered.
    pub fn window_state_changed(&self) {
        // a note that lost the focus must not hold unsaved text: it saves now, not after the debounce
        let notes: Vec<_> = self.0.notes.borrow().clone();
        for n in notes {
            if !self.0.env.signals.window_state(n.id()).focused {
                n.flush_text();
            }
        }
        if let Some(open_id) = self.dropdown_note_id() {
            let state = self.0.env.signals.window_state(open_id);
            if state.minimized || (!state.focused && self.0.env.signals.app_in_background()) {
                self.dismiss_dropdown();
            }
        }
        self.0.suspend.evaluate();
    }

    /// Windows asked a note to paint (for example because it was uncovered).
    pub fn paint_requested(&self, _id: NoteId) {
        self.0.suspend.poke();
    }

    /// Real app: have the platform tell us about minimize, restore, focus, activation and
    /// paint requests as they happen (no polling, so a suspended app sleeps until a message).
    pub fn watch_window_state(&self) {
        self.0.trim_enabled.set(true);
        self.start_attaching();
        let weak = Rc::downgrade(&self.0);
        self.0.env.signals.set_notifier(Rc::new(move |ev| {
            if let Some(inner) = weak.upgrade() {
                match ev {
                    SignalEvent::StateChanged => App(inner).window_state_changed(),
                    SignalEvent::Paint(id) => App(inner).paint_requested(id),
                    SignalEvent::Geometry(id) => App(inner).geometry_changed(id),
                }
            }
        }));
    }

    /// Slint creates a native window only once its event loop runs, a few milliseconds after the
    /// note opens: poll until every note has its handle, then stop (no timer is left behind).
    /// Without the handle a note gets no window signals and no fix for the swallowed button-up
    /// after a move or resize (win/signals.rs), so this runs for notes opened later too.
    fn start_attaching(&self) {
        let weak = Rc::downgrade(&self.0);
        let mut tries = 0;
        self.0.attach_timer.start(slint::TimerMode::Repeated, Duration::from_millis(5), move || {
            let Some(inner) = weak.upgrade() else { return };
            tries += 1;
            if App(inner.clone()).attach_windows() || tries >= 400 {
                inner.attach_timer.stop();
            }
        });
    }

    /// Tell the platform signals about every note window that exists now (idempotent), then
    /// look at the windows once: without the handles nothing can be seen as minimized or covered.
    /// Returns true when every note has a window now.
    fn attach_windows(&self) -> bool {
        let notes: Vec<_> = self.0.notes.borrow().clone();
        let mut attached = 0;
        for n in &notes {
            if let Some(hwnd) = n.hwnd() {
                self.0.env.signals.attach(n.id(), hwnd);
                attached += 1;
            }
        }
        let all = attached == notes.len();
        if all {
            crate::trace::mark("windows-attached");
            self.0.suspend.evaluate();
            self.schedule_trim();
        }
        all
    }

    /// Opening windows leaves freed heap pages and one-time start-up pages in the working set.
    /// Shortly after the last window opened, compact the heap and trim the working set once; what
    /// the app needs while idle is faulted back in (a soft fault), the rest stays out. One-shot:
    /// no timer is left running.
    fn schedule_trim(&self) {
        // WASHI_NO_TRIM: lets tools/memprobe.ps1 -NoTrim measure the untrimmed working set
        if !self.0.trim_enabled.get() || std::env::var_os("WASHI_NO_TRIM").is_some() {
            return;
        }
        let weak = Rc::downgrade(&self.0);
        self.0.trim_timer.start(slint::TimerMode::SingleShot, TRIM_DELAY, move || {
            if let Some(inner) = weak.upgrade() {
                if !inner.suspend.is_suspended() {
                    inner.env.power.release_memory();
                }
            }
        });
    }

    pub fn is_suspended(&self) -> bool {
        self.0.suspend.is_suspended()
    }

    pub fn has_quit(&self) -> bool {
        self.0.quit.get()
    }

    /// Notes closed within the exit window (30s) that would reopen if the app exits.
    pub fn exit_candidates(&self) -> Vec<ClosedCandidate> {
        self.0.exit_candidates()
    }

    /// Close a note: save it, remember where it was, mark it closed and release its window.
    /// Closing the last open note quits Washi.
    pub fn close_note(&self, id: NoteId) {
        self.dismiss_dropdown_if(id);
        self.dismiss_delete_dialog_if(id);
        let Some((was_empty, geometry)) = self.with_note(id, |n| (n.text().trim().is_empty(), n.geometry())) else {
            return;
        };
        self.0.record_closed_candidate(id, was_empty, geometry);

        // an empty note is not worth keeping: closing it discards it instead of saving it
        if was_empty && self.0.db.delete_note(id).is_ok() {
            self.0.save_stacking();
            self.remove_note(id);
            if self.is_all_notes_open() {
                self.refresh_all_notes(None);
            }
            return;
        }
        self.with_note(id, |n| {
            n.flush();
            n.prune_images();
            if let Err(e) = self.0.db.save_geometry(id, n.geometry()) {
                eprintln!("washi: could not save the position of note {id}: {e}");
            }
        });
        self.0.save_stacking();
        if let Err(e) = self.0.db.set_open(id, false) {
            eprintln!("washi: could not mark note {id} closed: {e}");
        }
        self.remove_note(id);
        if self.is_all_notes_open() {
            self.refresh_all_notes(None);
        }
    }

    /// Clone the handle to the open delete dialog, if any.
    pub fn delete_dialog(&self) -> Option<Rc<DeleteDialog>> {
        self.0.delete_dialog.borrow().clone()
    }

    /// Whether the delete dialog is open for any note.
    pub fn is_delete_dialog_open(&self) -> bool {
        self.0.delete_dialog.borrow().is_some()
    }

    /// The note whose delete dialog is currently open, if any.
    pub fn delete_dialog_note_id(&self) -> Option<NoteId> {
        self.0.delete_dialog.borrow().as_ref().map(|d| d.note_id())
    }

    /// Dismiss the open delete dialog if any.
    pub fn dismiss_delete_dialog(&self) {
        if let Some(dialog) = self.0.delete_dialog.borrow_mut().take() {
            let _ = dialog.ui().hide();
        }
    }

    /// Dismiss the open delete dialog if it belongs to note `id`.
    pub fn dismiss_delete_dialog_if(&self, id: NoteId) {
        if self.delete_dialog_note_id() == Some(id) {
            self.dismiss_delete_dialog();
        }
    }

    /// Invoke "Keep it" callback on the active delete dialog (for testing).
    pub fn invoke_keep_note(&self) {
        if let Some(d) = self.delete_dialog() {
            d.invoke_keep();
        }
    }

    /// Invoke "Delete" callback on the active delete dialog (for testing).
    pub fn invoke_confirm_delete(&self) {
        if let Some(d) = self.delete_dialog() {
            d.invoke_confirm_delete();
        }
    }

    /// Dispatch a window event to the active delete dialog (for testing).
    pub fn dispatch_delete_dialog_event(&self, ev: slint::platform::WindowEvent) {
        if let Some(d) = self.delete_dialog() {
            d.ui().window().dispatch_event(ev);
        }
    }

    /// Delete a note for good, after asking with the themed delete dialog.
    pub fn delete_note(&self, id: NoteId) {
        self.dismiss_dropdown_if(id);

        if let Some(open_id) = self.delete_dialog_note_id() {
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

        let geom = note.geometry();
        let work_areas = self.0.env.window_control.work_areas();
        let scale = note.ui().window().scale_factor();
        let work_area = work_areas.first().map(|wa| (
            wa.x as f32 / scale,
            wa.y as f32 / scale,
            wa.w as f32 / scale,
            wa.h as f32 / scale,
        ));

        let pos = calculate_dialog_position((geom.x, geom.y, geom.w, geom.h), work_area);

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

        match DeleteDialog::open(id, pos, note.hwnd(), on_keep, on_confirm, on_start_move) {
            Ok(dialog) => {
                *self.0.delete_dialog.borrow_mut() = Some(Rc::new(dialog));
            }
            Err(e) => eprintln!("washi: could not open delete dialog: {e}"),
        }
    }

    /// Delete a note for good from the database and remove its window.
    pub fn confirm_delete_note(&self, id: NoteId) {
        let next_selection = if self.is_all_notes_open() {
            if self.all_notes_selected_id() == Some(id) {
                self.set_all_notes_confirming_delete(false);
                let old_rows = self.all_notes_rows();
                let old_idx = old_rows.iter().position(|r| r.id == id).unwrap_or(0);
                Some(Some(old_idx))
            } else {
                Some(None)
            }
        } else {
            None
        };
        self.0.remove_exit_candidate(id);
        if let Err(e) = self.0.db.delete_note(id) {
            self.0.env.dialogs.error(&format!("Washi could not delete the note: {e}"));
            return;
        }
        self.remove_note(id);
        match next_selection {
            Some(Some(old_idx)) => {
                let new_rows = self.all_notes_rows();
                let next_id = new_rows
                    .get(old_idx)
                    .or_else(|| new_rows.last())
                    .map(|r| r.id);
                if let Some(v) = self.all_notes() {
                    let now_ms = self.0.env.clock.unix_millis();
                    v.refresh(&new_rows, next_id, now_ms);
                }
            }
            Some(None) => self.refresh_all_notes(None),
            None => {}
        }
    }

    fn create_new_note(&self, from: Option<NoteId>) -> Option<NoteId> {
        let geometry = from
            .and_then(|from_id| self.with_note(from_id, |n| n.geometry()))
            .map(|g| Geometry { x: g.x + NEW_NOTE_OFFSET, y: g.y + NEW_NOTE_OFFSET, ..g });
        let id = match self.0.db.create_note(self.0.env.clock.unix_millis()) {
            Ok(id) => id,
            Err(e) => {
                self.0.env.dialogs.error(&format!("Washi could not create a note: {e}"));
                return None;
            }
        };
        if let Some(g) = geometry {
            let _ = self.0.db.save_geometry(id, g);
        }
        if let Err(e) = self.open_row(&NoteRow { id, content: String::new(), geometry, always_on_top: false }) {
            self.0.env.dialogs.error(&e.to_string());
            return None;
        }
        Some(id)
    }

    /// Open a fresh empty note. If an open note exists, offsets from it.
    pub fn new_note(&self) {
        let from = self.dropdown_note_id().or_else(|| self.0.notes.borrow().last().map(|n| n.id()));
        if self.create_new_note(from).is_some() && self.is_all_notes_open() {
            self.refresh_all_notes(None);
        }
    }

    /// Open an empty note, offset from `from`.
    pub fn new_note_from(&self, from: NoteId) {
        if self.create_new_note(Some(from)).is_some() && self.is_all_notes_open() {
            self.refresh_all_notes(None);
        }
    }

    /// Reopen a closed note where it was.
    pub fn reopen_note(&self, id: NoteId) {
        if self.with_note(id, |_| ()).is_some() {
            return;
        }
        self.0.remove_exit_candidate(id);
        let Ok(Some(row)) = self.0.db.note(id) else { return };
        if self.0.db.set_open(id, true).is_ok() {
            if let Err(e) = self.open_row(&row) {
                self.0.env.dialogs.error(&e.to_string());
            }
            if self.is_all_notes_open() {
                self.refresh_all_notes(None);
            }
        }
    }

    /// Raise an open note to front or reopen it if closed.
    pub fn activate_note(&self, id: NoteId) {
        let note = self.0.notes.borrow().iter().find(|n| n.id() == id).cloned();
        if let Some(note) = note {
            let _ = note.ui().show();
            let hwnd = note.hwnd();
            self.0.env.window_control.bring_to_front(id, hwnd);
            if self.is_all_notes_open() {
                self.refresh_all_notes(None);
            }
        } else {
            self.reopen_note(id);
        }
    }

    /// Keep a note's window above the others (or not), and remember it.
    pub fn set_always_on_top(&self, id: NoteId, on: bool) {
        self.with_note(id, |n| n.set_always_on_top(on));
        if let Some(d) = self.dropdown() {
            if d.note_id() == id {
                d.set_pinned(on);
                d.ensure_on_top();
                let d_clone = d.clone();
                slint::Timer::single_shot(std::time::Duration::ZERO, move || {
                    d_clone.ensure_on_top();
                });
                let d_clone2 = d.clone();
                slint::Timer::single_shot(std::time::Duration::from_millis(20), move || {
                    d_clone2.ensure_on_top();
                });
            }
        }
        if let Err(e) = self.0.db.set_always_on_top(id, on) {
            eprintln!("washi: could not save always-on-top for note {id}: {e}");
        }
    }

    /// The toolbar's image button: ask for a PNG or JPEG and add it after the current paragraph.
    fn insert_image_from_file(&self, id: NoteId) {
        let Some(path) = self.0.env.dialogs.pick_open_file("Pictures|*.png;*.jpg;*.jpeg|All files|*.*") else { return };
        self.start_import(id, Source::File(path));
    }

    /// Ctrl+V in a note: true (and the picture is on its way) when the clipboard holds a picture.
    fn paste_image(&self, id: NoteId) -> bool {
        match self.0.env.clipboard.image() {
            Some(pixels) => {
                self.start_import(id, Source::Pixels(pixels));
                true
            }
            None => false,
        }
    }

    /// Prepare a picture on a short-lived thread (typing never stalls) and add it to the note,
    /// after the paragraph that has the caret now, when it is ready.
    fn start_import(&self, note: NoteId, source: Source) {
        let Some(after) = self.with_note(note, |n| n.insert_position()) else { return };
        let result = match images::spawn_prepare(source) {
            Ok(rx) => rx,
            Err(e) => return self.0.env.dialogs.error(&e.to_string()),
        };
        self.0.imports.borrow_mut().push(Import { note, after, result });
        if !self.0.import_timer.running() {
            let weak = Rc::downgrade(&self.0);
            self.0.import_timer.start(slint::TimerMode::Repeated, IMPORT_POLL, move || {
                if let Some(inner) = weak.upgrade() {
                    App(inner).collect_imports();
                }
            });
        }
    }

    /// Add the pictures whose threads are done; stop polling when none are left.
    fn collect_imports(&self) {
        let mut done = Vec::new();
        self.0.imports.borrow_mut().retain(|i| match i.result.try_recv() {
            Ok(r) => {
                done.push((i.note, i.after, r));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => {
                done.push((i.note, i.after, Err(ImageError("The picture could not be prepared.".into()))));
                false
            }
        });
        if self.0.imports.borrow().is_empty() {
            self.0.import_timer.stop();
        }
        for (note, after, result) in done {
            match result {
                Ok(stored) => {
                    // the note may have been closed meanwhile: then the picture is dropped
                    if let Some(Err(e)) = self.with_note(note, |n| n.add_image(&stored, after)) {
                        self.0.env.dialogs.error(&format!("Washi could not save the picture: {e}"));
                    }
                }
                Err(e) => self.0.env.dialogs.error(&e.to_string()),
            }
        }
    }

    /// A stored picture: mime type, size and encoded bytes (what is in the database).
    pub fn stored_image(&self, id: i64) -> Option<(String, u32, u32, Vec<u8>)> {
        self.0.db.image(id).ok().flatten().map(|i| (i.mime, i.width, i.height, i.data))
    }

    /// How many pictures the database holds, for all notes.
    pub fn stored_image_count(&self) -> usize {
        self.0.db.image_count().unwrap_or(0)
    }

    /// Pictures still being prepared.
    pub fn pending_images(&self) -> usize {
        self.0.imports.borrow().len()
    }

    /// Bytes of decoded pictures all open notes hold right now.
    pub fn decoded_image_bytes(&self) -> usize {
        self.0.notes.borrow().iter().map(|n| n.decoded_image_bytes()).sum()
    }

    /// "Export as Markdown...": a save dialog, then the note as a plain .md file with its pictures
    /// as PNG files beside it.
    pub fn export_note(&self, id: NoteId) {
        let Some((text, title)) = self
            .with_note(id, |n| {
                n.flush();
                (n.text(), n.title())
            })
            .or_else(|| {
                let text = self.0.db.content(id).ok().flatten()?;
                let title = note_title(&text).unwrap_or_else(|| "Washi".to_string());
                Some((text, title))
            })
        else {
            return;
        };
        let Some(path) = self.0.env.dialogs.pick_save_file(&crate::export::suggested_name(&title), "Markdown|*.md") else { return };
        if let Err(e) = crate::export::write(&self.0.db, &text, &path) {
            self.0.env.dialogs.error(&format!("Washi could not export the note: {e}"));
        }
    }

    fn show_menu(&self, id: NoteId, at: (i32, i32), image: Option<usize>) {
        let closed = self.0.db.closed_notes().unwrap_or_default();
        let item = |id, label: &str, enabled, children| MenuItem {
            id,
            label: label.into(),
            checked: false,
            enabled,
            children,
        };
        let toggle = |id, label: &str, checked| MenuItem { checked, ..item(id, label, true, vec![]) };
        let pinned = self.with_note(id, |n| n.always_on_top()).unwrap_or(false);
        let open_children: Vec<MenuItem> = closed
            .iter()
            .enumerate()
            .map(|(i, c)| item(MENU_OPEN_BASE + i as u32, &title_of(&c.preview), true, vec![]))
            .collect();
        let has_image = self.0.env.clipboard.has_image();
        let has_paste = has_image || self.0.env.clipboard.has_text();
        let mut items = vec![];
        // right click on a picture: its own items come first
        if let Some((pct, align)) = image.and_then(|b| self.with_note(id, |n| n.image_state(b))?) {
            let aligns = [(Align::Left, "Left"), (Align::Center, "Center"), (Align::Right, "Right")];
            let align_items = aligns.iter().enumerate().map(|(i, &(a, label))| toggle(MENU_IMAGE_ALIGN + i as u32, label, a == align)).collect();
            let size_items = IMAGE_SIZES.iter().map(|&s| toggle(MENU_IMAGE_SIZE + s as u32, &format!("{s}%"), s == pct)).collect();
            items.push(item(MENU_IMAGE_CUT, "Cut image", true, vec![]));
            items.push(item(MENU_IMAGE_COPY, "Copy image", true, vec![]));
            items.push(item(MENU_IMAGE_DELETE, "Delete image", true, vec![]));
            items.push(item(0, "Align image", true, align_items));
            items.push(item(0, "Resize image", true, size_items));
        }
        items.extend([
            item(MENU_PASTE, "Paste", has_paste, vec![]),
            item(MENU_PASTE_IMAGE, "Paste image", has_image, vec![]),
            item(MENU_NEW, "New note", true, vec![]),
            item(MENU_OPEN, "Open note", !open_children.is_empty(), open_children),
            item(MENU_CLOSE, "Close", true, vec![]),
            item(MENU_DELETE, "Delete", true, vec![]),
            toggle(MENU_TOP, "Always on top", pinned),
            toggle(MENU_STARTUP, "Start with Windows", self.0.env.startup.is_enabled()),
            toggle(MENU_FADE, "Fade text edge", self.0.db.fade_text_edge()),
            item(MENU_EXPORT, "Export as Markdown\u{2026}", true, vec![]),
        ]);
        let picture = image;
        match self.0.env.menu.show(&items, at) {
            Some(MENU_IMAGE_CUT) if picture.is_some() => {
                self.with_note(id, |n| n.cut_image(picture.unwrap()));
            }
            Some(MENU_IMAGE_COPY) if picture.is_some() => {
                self.with_note(id, |n| n.copy_image(picture.unwrap()));
            }
            Some(MENU_IMAGE_DELETE) if picture.is_some() => {
                self.with_note(id, |n| n.delete_image(picture.unwrap()));
            }
            Some(c @ MENU_IMAGE_ALIGN..=23) if picture.is_some() => {
                let align = [Align::Left, Align::Center, Align::Right][(c - MENU_IMAGE_ALIGN) as usize];
                self.with_note(id, |n| n.set_image_align(picture.unwrap(), align));
            }
            Some(c) if (MENU_IMAGE_SIZE + IMAGE_PCT_MIN as u32..=MENU_IMAGE_SIZE + IMAGE_PCT_MAX as u32).contains(&c) && picture.is_some() => {
                self.with_note(id, |n| n.set_image_pct(picture.unwrap(), (c - MENU_IMAGE_SIZE) as u8));
            }
            Some(MENU_PASTE) => {
                self.with_note(id, |n| n.paste());
            }
            Some(MENU_PASTE_IMAGE) => {
                self.with_note(id, |n| n.paste_image());
            }
            Some(MENU_NEW) => self.new_note_from(id),
            Some(MENU_CLOSE) => self.close_note(id),
            Some(MENU_DELETE) => self.delete_note(id),
            Some(MENU_TOP) => self.set_always_on_top(id, !pinned),
            Some(MENU_EXPORT) => self.export_note(id),
            Some(MENU_FADE) => {
                let on = !self.0.db.fade_text_edge();
                let _ = self.0.db.set_fade_text_edge(on);
                for n in self.0.notes.borrow().iter() {
                    n.set_fade_edge(on);
                }
            }
            Some(MENU_STARTUP) => {
                let startup = &self.0.env.startup;
                startup.set_enabled(!startup.is_enabled());
            }
            Some(c) if c >= MENU_OPEN_BASE => {
                if let Some(note) = closed.get((c - MENU_OPEN_BASE) as usize) {
                    self.reopen_note(note.id);
                }
            }
            _ => {}
        }
    }

    /// Take a note out of the app. Its window is released on the next turn of the event loop,
    /// because this may run inside one of that window's own callbacks.
    fn remove_note(&self, id: NoteId) {
        self.dismiss_dropdown_if(id);
        self.dismiss_delete_dialog_if(id);
        let (note, now_empty) = {
            let mut notes = self.0.notes.borrow_mut();
            let at = notes.iter().position(|n| n.id() == id);
            (at.map(|i| notes.remove(i)), notes.is_empty())
        };
        if let Some(note) = note {
            self.0.env.signals.detach(id);
            slint::Timer::single_shot(Duration::ZERO, move || drop(note));
            self.0.suspend.evaluate();
            if now_empty && !self.is_all_notes_open() {
                self.0.restore_exit_candidates();
                self.0.quit.set(true);
                let _ = slint::quit_event_loop();
            }
        }
    }

    /// Save everything immediately. Notes stay open, so the next start reopens them.
    pub fn quit(&self) {
        self.0.save_all();
        self.0.quit.set(true);
    }
}

impl Inner {
    fn exit_candidates(&self) -> Vec<ClosedCandidate> {
        self.exit_candidates.borrow().clone()
    }

    /// Clears any pending exit sequence candidates, stacking order snapshot, and stops the exit timer.
    fn clear_exit_sequence(&self) {
        self.exit_candidates.borrow_mut().clear();
        self.exit_stacking.borrow_mut().clear();
        self.exit_timer.stop();
    }

    /// Records a closed note as an exit sequence candidate.
    fn record_closed_candidate(self: &Rc<Self>, id: NoteId, was_empty: bool, geometry: Geometry) {
        let now = self.env.clock.monotonic();
        if let Some(last) = self.exit_candidates.borrow().last() {
            if now.saturating_sub(last.closed_at) > EXIT_WINDOW {
                self.clear_exit_sequence();
            }
        }
        if self.exit_candidates.borrow().is_empty() {
            *self.exit_stacking.borrow_mut() = self.current_stacking();
        }
        self.exit_candidates.borrow_mut().push(ClosedCandidate {
            id,
            closed_at: now,
            was_empty,
            geometry: Some(geometry),
        });
        let weak = Rc::downgrade(self);
        self.exit_timer.start(slint::TimerMode::SingleShot, EXIT_WINDOW, move || {
            if let Some(inner) = weak.upgrade() {
                inner.clear_exit_sequence();
            }
        });
    }

    /// Removes an exit candidate if present; clears the exit sequence if none remain.
    fn remove_exit_candidate(&self, id: NoteId) {
        self.exit_candidates.borrow_mut().retain(|c| c.id != id);
        if self.exit_candidates.borrow().is_empty() {
            self.clear_exit_sequence();
        }
    }

    /// Stacking order (bottom to top) of the currently open notes.
    fn current_stacking(&self) -> Vec<NoteId> {
        let Ok(notes) = self.notes.try_borrow() else { return Vec::new() };
        let windows: Vec<(NoteId, Option<isize>)> = notes.iter().map(|n| (n.id(), n.hwnd())).collect();
        drop(notes);
        self.env.window_control.stacking_order(&windows)
    }

    /// Remember which note window is above which, for the next start.
    fn save_stacking(&self) {
        let order = self.current_stacking();
        if let Err(e) = self.db.save_stacking(&order) {
            eprintln!("washi: could not save the stacking order of the notes: {e}");
        }
    }

    /// Restore notes closed during the exit sequence (or session end) back to open in the DB.
    fn restore_exit_candidates(&self) {
        let non_empty: Vec<NoteId> = self.exit_candidates.borrow().iter().filter(|c| !c.was_empty).map(|c| c.id).collect();
        if non_empty.is_empty() {
            self.clear_exit_sequence();
            return;
        }

        for &id in &non_empty {
            if let Err(e) = self.db.set_open(id, true) {
                eprintln!("washi: could not mark note {id} open on exit: {e}");
            }
        }

        // Stacking order: combine currently open notes (if any) and restored candidates.
        let open_ids: Vec<NoteId> = self.notes.try_borrow().map(|n| n.iter().map(|n| n.id()).collect()).unwrap_or_default();
        let mut all_ids: Vec<NoteId> = open_ids;
        for &id in &non_empty {
            if !all_ids.contains(&id) {
                all_ids.push(id);
            }
        }

        let stacking: Vec<NoteId> = self.exit_stacking.borrow().clone();
        let mut ordered: Vec<NoteId> = stacking.into_iter().filter(|id| all_ids.contains(id)).collect();
        for id in all_ids {
            if !ordered.contains(&id) {
                ordered.push(id);
            }
        }

        if let Err(e) = self.db.save_stacking(&ordered) {
            eprintln!("washi: could not save stacking order on exit: {e}");
        }

        self.clear_exit_sequence();
    }
}

impl SuspendHost for Inner {
    fn open_notes(&self) -> Vec<NoteId> {
        self.notes.borrow().iter().map(|n| n.id()).collect()
    }

    fn save_all(&self) {
        // `try_borrow`: this may run inside a window message while the list is being changed
        let Ok(notes) = self.notes.try_borrow().map(|n| n.clone()) else { return };
        for n in notes {
            n.flush();
            if let Err(e) = self.db.save_geometry(n.id(), n.geometry()) {
                eprintln!("washi: could not save the position of note {}: {e}", n.id());
            }
        }
        self.save_stacking();
    }

    fn stop_timers(&self) {
        let notes: Vec<_> = self.notes.borrow().clone();
        for n in notes {
            n.pause();
        }
    }

    fn release_caches(&self) {
        // decoded pictures first (the big allocations), then SQLite's caches
        let notes: Vec<_> = self.notes.borrow().clone();
        for n in notes {
            n.free_images();
        }
        self.db.shrink_memory();
    }

    fn resume(&self) {
        let notes: Vec<_> = self.notes.borrow().clone();
        for n in notes {
            n.resume(self.env.signals.window_state(n.id()).focused);
        }
    }
}

/// Menu title of a note: its first line, cut to about 40 characters.
fn title_of(content: &str) -> String {
    note_title(content).unwrap_or_else(|| "(empty note)".into())
}
