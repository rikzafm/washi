//! The app: opens the encrypted store, shows the open notes, and owns their lifecycle.

mod all_notes;
mod exit;
mod font_drop;
mod fonts;
mod notes;
mod pictures;
mod popups;
mod settings;
mod windows;

pub use exit::{ClosedCandidate, EXIT_WINDOW};
use exit::ExitSequence;
use pictures::Import;

use crate::all_notes::AllNotesView;
use crate::image_viewer::ImageViewer;
use crate::note_window::{Note, NoteEvent};
use crate::platform::*;
use crate::popups::Popups;
use crate::settings::SettingsView;
use crate::storage::{keystore, Db, DbError, Geometry, KeyStoreError, NoteRow};
use crate::suspend::{SuspendController, SuspendHost};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

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
    pub startup: Rc<dyn StartupRegistration>,
    pub instance: Rc<dyn SingleInstance>,
    pub dialogs: Rc<dyn Dialogs>,
    pub clipboard: Rc<dyn ImageClipboard>,
    pub window_control: Rc<dyn WindowControl>,
    pub stats: Rc<dyn ProcessStats>,
    pub shell: Rc<dyn Shell>,
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

struct Inner {
    env: Env,
    db: Rc<Db>,
    notes: RefCell<Vec<Rc<Note>>>,
    quit: Cell<bool>,
    suspend: Rc<SuspendController>,
    /// Notes closed one after another within 30s, reopened if the app exits (see `exit`), and
    /// the timer that ends the sequence 30s after the last close.
    exit: RefCell<ExitSequence>,
    exit_timer: slint::Timer,
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
    /// Open picture viewers (double click on a picture).
    viewers: RefCell<Vec<Rc<ImageViewer>>>,
    /// The open Dropdown menu, right-click menu and Delete dialog.
    popups: Popups,
    /// How many right-click menus have been opened (a test hook).
    menus_opened: Cell<usize>,
    /// Currently open All Notes window, if any.
    all_notes: RefCell<Option<Rc<AllNotesView>>>,
    /// Currently open Settings page, if any.
    settings: RefCell<Option<Rc<SettingsView>>>,
    /// Fonts being installed on their worker threads, and the timer that collects them (it runs
    /// only while there are any).
    font_installs: RefCell<Vec<fonts::FontInstall>>,
    font_timer: slint::Timer,
    /// The axes of the installed fonts read so far.
    font_axes: RefCell<fonts::AxesCache>,
    /// A slider of the Font manager's Axis panel being dragged.
    axis_drag: RefCell<fonts::AxisDrag>,
}

#[derive(Clone)]
pub struct App(Rc<Inner>);

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
        // every window from now on is drawn at the saved UI size
        crate::ui_scale::set_app_scale(db.ui_scale());
        // and every note's text at the saved Font size
        crate::fonts::set_size(db.font_size());
        // and in the saved font: loaded here, before any window exists (a missing or broken one
        // falls back to the Default font and is reported once)
        fonts::load_startup(&env, &db);

        let mut rows = db.open_notes().map_err(StartError::Db)?;
        if rows.is_empty() {
            let id = db.create_note(now).map_err(StartError::Db)?;
            let on_top = db.apply_new_note_defaults(id);
            rows.push(NoteRow { id, content: String::new(), geometry: None, always_on_top: on_top });
        }
        let suspend = SuspendController::new(env.signals.clone(), env.power.clone());
        let inner = Rc::new(Inner {
            env,
            db,
            notes: RefCell::new(Vec::new()),
            quit: Cell::new(false),
            suspend,
            exit: RefCell::default(),
            exit_timer: slint::Timer::default(),
            imports: RefCell::new(Vec::new()),
            import_timer: slint::Timer::default(),
            attach_timer: slint::Timer::default(),
            trim_timer: slint::Timer::default(),
            trim_enabled: Cell::new(false),
            viewers: RefCell::new(Vec::new()),
            popups: Popups::default(),
            menus_opened: Cell::new(0),
            all_notes: RefCell::new(None),
            settings: RefCell::new(None),
            font_installs: RefCell::new(Vec::new()),
            font_timer: slint::Timer::default(),
            font_axes: RefCell::new(Default::default()),
            axis_drag: RefCell::new(Default::default()),
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
    pub(super) fn open_row(&self, row: &NoteRow) -> Result<(), StartError> {
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
            NoteEvent::ContextMenu { at, .. } => {
                self.dismiss_dropdown();
                self.show_menu(id, at);
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
                if let Some(v) = self.all_notes() {
                    if let Some(text) = self.with_note(id, |n| n.text()) {
                        v.on_desktop_note_text_changed(id, &text);
                    }
                }
            }
        }
    }

    /// The platform saw a note move or resize: remember where it is once the user stops.
    pub fn geometry_changed(&self, id: NoteId) {
        self.dismiss_dropdown_if(id);
        self.with_note(id, |n| n.geometry_changed());
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

    pub fn is_suspended(&self) -> bool {
        self.0.suspend.is_suspended()
    }

    pub fn has_quit(&self) -> bool {
        self.0.quit.get()
    }
}
