//! The app: opens the encrypted store, shows the open notes, and owns their lifecycle.

use crate::all_notes::AllNotesView;
use crate::settings::SettingsView;
use crate::delete_dialog::DeleteDialog;

use crate::context_menu::{ClosedItem, ContextMenu};
use crate::dropdown::{extract_title_and_preview, format_relative_timestamp, shorten_preview, DropdownMenu};
use crate::images::{self, ImageError, Source, Stored};
use crate::image_viewer::ImageViewer;
use crate::note_window::{app_icon, note_title, Note, NoteEvent};
use crate::platform::*;
use crate::suspend::{SuspendController, SuspendHost};
use crate::storage::{keystore, Db, DbError, Geometry, KeyStoreError, NoteRow};
use crate::storage::db::Choice;
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
    /// Open picture viewers (double click on a picture).
    viewers: RefCell<Vec<Rc<ImageViewer>>>,
    /// Currently open dropdown menu floating window, if any.
    dropdown: RefCell<Option<Rc<DropdownMenu>>>,
    /// Currently open right-click menu window, if any.
    context_menu: RefCell<Option<Rc<ContextMenu>>>,
    /// How many right-click menus have been opened (a test hook).
    menus_opened: Cell<usize>,
    /// Currently open delete confirmation dialog window, if any.
    delete_dialog: RefCell<Option<Rc<DeleteDialog>>>,
    /// Currently open All Notes window, if any.
    all_notes: RefCell<Option<Rc<AllNotesView>>>,
    /// Currently open Settings page, if any.
    settings: RefCell<Option<Rc<SettingsView>>>,
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

/// A new note opens this far (logical px, right and down) from the note it was made from.
pub const NEW_NOTE_OFFSET: i32 = 30;

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
            exit_candidates: RefCell::new(Vec::new()),
            exit_timer: slint::Timer::default(),
            exit_stacking: RefCell::new(Vec::new()),
            imports: RefCell::new(Vec::new()),
            import_timer: slint::Timer::default(),
            attach_timer: slint::Timer::default(),
            trim_timer: slint::Timer::default(),
            trim_enabled: Cell::new(false),
            viewers: RefCell::new(Vec::new()),
            dropdown: RefCell::new(None),
            context_menu: RefCell::new(None),
            menus_opened: Cell::new(0),
            delete_dialog: RefCell::new(None),
            all_notes: RefCell::new(None),
            settings: RefCell::new(None),
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

    /// Show the picture of `block` of note `id` in a window of its own.
    fn open_viewer(&self, id: NoteId, block: usize) {
        let Some((picture, size)) = self.with_note(id, |n| n.picture_at(block)).flatten() else { return };
        let rgba = self.with_note(id, |n| n.image_rgba(block)).flatten();
        let caption = self.saved_text(id).and_then(|t| note_title(&t)).unwrap_or_else(|| "Picture".to_string());
        let target = self.with_note(id, |n| {
            let p = n.ui().window().position();
            let s = n.ui().window().size();
            ScreenRect::new(p.x, p.y, s.width as i32, s.height as i32)
        });

        let weak = Rc::downgrade(&self.0);
        let slot: Rc<RefCell<Option<std::rc::Weak<ImageViewer>>>> = Rc::new(RefCell::new(None));
        let slot_close = slot.clone();
        let on_close = Rc::new(move || {
            if let Some(inner) = weak.upgrade() {
                if let Some(weak_viewer) = slot_close.borrow_mut().take() {
                    if let Some(v) = weak_viewer.upgrade() {
                        inner.viewers.borrow_mut().retain(|other| !Rc::ptr_eq(other, &v));
                        slint::Timer::single_shot(std::time::Duration::ZERO, move || drop(v));
                    }
                }
            }
        });

        match ImageViewer::open(
            picture,
            size,
            &caption,
            target,
            self.0.env.window_control.clone(),
            self.0.env.clipboard.clone(),
            rgba,
            app_icon(),
            on_close,
        ) {
            Ok(viewer) => {
                let v = Rc::new(viewer);
                *slot.borrow_mut() = Some(Rc::downgrade(&v));
                self.0.viewers.borrow_mut().push(v);
            }
            Err(e) => eprintln!("washi: could not open the picture viewer: {e}"),
        }
    }

    /// The platform saw a note move or resize: remember where it is once the user stops.
    pub fn geometry_changed(&self, id: NoteId) {
        self.dismiss_dropdown_if(id);
        self.with_note(id, |n| n.geometry_changed());
    }

    /// Clone the handle to the open dropdown menu, if any.
    pub fn dropdown(&self) -> Option<Rc<DropdownMenu>> {
        self.0.dropdown.borrow().clone()
    }

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
        };
        match SettingsView::open(host, geometry, pinned, hooks) {
            Ok(view) => {
                self.0.env.window_control.bring_to_front(0, view.hwnd());
                *self.0.settings.borrow_mut() = Some(view);
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
        self.dismiss_dropdown();
        self.dismiss_delete_dialog();
        let notes: Vec<Rc<Note>> = self.0.notes.borrow().clone();
        for n in &notes {
            n.set_ui_scale(scale, k);
        }
        if let Some(v) = self.all_notes() {
            v.set_ui_scale(scale, k);
        }
        for c in self.0.exit_candidates.borrow_mut().iter_mut() {
            c.geometry = c.geometry.map(|g| g.scaled(k));
        }
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

    /// The Settings page has closed: drop it on the next turn of the loop, never inside its own callback.
    fn settings_closed(&self) {
        let view = self.0.settings.borrow_mut().take();
        slint::Timer::single_shot(std::time::Duration::ZERO, move || drop(view));
    }

    /// Clone the handle to the open All Notes view, if any.
    pub fn all_notes(&self) -> Option<Rc<AllNotesView>> {
        self.0.all_notes.borrow().clone()
    }

    /// Open All Notes from note `initial_selected_id`: its window grows to the left for the list.
    pub fn open_all_notes(&self, initial_selected_id: NoteId) {
        if let Some(v) = self.all_notes() {
            v.refresh_rows(Some(initial_selected_id));
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

        let (db, clock, icon, window_control, clipboard) = (
            self.0.db.clone(),
            self.0.env.clock.clone(),
            app_icon(),
            self.0.env.window_control.clone(),
            self.0.env.clipboard.clone(),
        );

        match AllNotesView::open(
            initial_selected_id,
            db,
            clock,
            icon,
            window_control,
            clipboard,
            Rc::new(self.clone()),
            on_close,
        ) {
            Ok(view) => {
                *self.0.all_notes.borrow_mut() = Some(view);
            }
            Err(e) => eprintln!("washi: could not open All Notes: {e}"),
        }
    }

    /// Close All Notes: the window gives the sidebar's strip back and the host note keeps the
    /// place and size its paper has now.
    pub fn dismiss_all_notes(&self) {
        let Some(v) = self.0.all_notes.borrow_mut().take() else { return };
        let (host, paper) = (v.host_note_id(), v.paper_geometry());
        v.unpark_notes();
        v.close();
        if self.with_note(host, |n| {
            n.set_saves_geometry(true);
            n.place(paper);
        }).is_some() {
            let hwnd = self.with_note(host, |n| n.hwnd()).flatten();
            self.0.env.window_control.bring_to_front(host, hwnd);
        }
        if self.0.notes.borrow().is_empty() {
            self.0.restore_exit_candidates();
            self.0.quit.set(true);
            let _ = slint::quit_event_loop();
        }
    }

    /// Give the open All notes Sidebar the Height and Anchor Settings › Appearance stores for it.
    fn apply_notes_sidebar_layout(&self) {
        if let Some(v) = self.all_notes() {
            v.apply_sidebar_layout();
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

    /// How many screen px one logical px of each open picture viewer covers, oldest first.
    pub fn viewer_scale_factors(&self) -> Vec<f32> {
        self.0.viewers.borrow().iter().map(|v| v.scale_factor()).collect()
    }

    /// Window sizes (physical px) of open picture viewers, oldest first.
    pub fn viewer_window_sizes(&self) -> Vec<(u32, u32)> {
        self.0.viewers.borrow().iter().map(|v| v.window_size()).collect()
    }

    /// Dismiss the open dropdown menu and reset the parent note's button state.
    pub fn dismiss_dropdown(&self) {
        self.dismiss_context_menu();
        let menu = self.0.dropdown.borrow_mut().take();
        if let Some(m) = menu {
            m.close();
            let note_id = m.note_id();
            self.with_note(note_id, |n| n.set_dropdown_open(false));
            if let Some(v) = self.all_notes() {
                v.ui().set_dropdown_open(false);
            }
        }
    }

    /// Dismiss the open dropdown menu if it belongs to note `id`.
    fn dismiss_dropdown_if(&self, id: NoteId) {
        if self.context_menu().map(|m| m.note_id()) == Some(id) {
            self.dismiss_context_menu();
        }
        if self.dropdown().map(|d| d.note_id()) == Some(id) {
            self.dismiss_dropdown();
        }
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
    fn toggle_editor_dropdown(&self) {
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
        *self.0.dropdown.borrow_mut() = Some(Rc::new(menu));
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
        if let Some(open_id) = self.dropdown().map(|d| d.note_id()) {
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
            if let Some(v) = self.all_notes() {
                v.on_desktop_note_closed(id);
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
        if let Some(v) = self.all_notes() {
            v.on_desktop_note_closed(id);
        }
    }

    /// Clone the handle to the open delete dialog, if any.
    pub fn delete_dialog(&self) -> Option<Rc<DeleteDialog>> {
        self.0.delete_dialog.borrow().clone()
    }

    /// Dismiss the open delete dialog if any.
    pub fn dismiss_delete_dialog(&self) {
        if let Some(dialog) = self.0.delete_dialog.borrow_mut().take() {
            let _ = dialog.ui().hide();
        }
    }

    /// Dismiss the open delete dialog if it belongs to note `id`.
    pub fn dismiss_delete_dialog_if(&self, id: NoteId) {
        if self.delete_dialog().map(|d| d.note_id()) == Some(id) {
            self.dismiss_delete_dialog();
        }
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
                *self.0.delete_dialog.borrow_mut() = Some(Rc::new(dialog));
            }
            Err(e) => eprintln!("washi: could not open delete dialog: {e}"),
        }
    }

    /// Delete a note for good from the database and remove its window.
    pub fn confirm_delete_note(&self, id: NoteId) {
        self.0.remove_exit_candidate(id);
        if let Err(e) = self.0.db.delete_note(id) {
            self.0.env.dialogs.error(&format!("Washi could not delete the note: {e}"));
            return;
        }
        self.remove_note(id);
        if let Some(v) = self.all_notes() {
            v.on_note_deleted(id);
        }
    }

    fn create_new_note(&self, from: Option<NoteId>) -> Option<NoteId> {
        // Offset from the note it is opened from, but at the new-note size for the UI size (not that
        // note's own size).
        let (w, h) = crate::note_window::new_note_window_size();
        let geometry = from
            .and_then(|from_id| self.with_note(from_id, |n| n.geometry()))
            .map(|g| Geometry { x: g.x + NEW_NOTE_OFFSET, y: g.y + NEW_NOTE_OFFSET, w, h });
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
        let on_top = self.0.db.apply_new_note_defaults(id);
        if let Err(e) = self.open_row(&NoteRow { id, content: String::new(), geometry, always_on_top: on_top }) {
            self.0.env.dialogs.error(&e.to_string());
            return None;
        }
        Some(id)
    }

    /// Open a fresh empty note. If an open note exists, offsets from it.
    pub fn new_note(&self) {
        let from = self.dropdown().map(|d| d.note_id()).or_else(|| self.0.notes.borrow().last().map(|n| n.id()));
        if let Some(new_id) = self.create_new_note(from) {
            if let Some(v) = self.all_notes() {
                v.on_desktop_note_opened(new_id);
            }
        }
    }

    /// Open an empty note, offset from `from`.
    pub fn new_note_from(&self, from: NoteId) {
        if let Some(new_id) = self.create_new_note(Some(from)) {
            if let Some(v) = self.all_notes() {
                v.on_desktop_note_opened(new_id);
            }
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
            if let Some(v) = self.all_notes() {
                v.on_desktop_note_opened(id);
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
            if let Some(v) = self.all_notes() {
                v.on_desktop_note_opened(id);
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

    /// Open the right-click menu of note `id` at `at` (screen px): see `ui/context-menu.slint`.
    fn show_menu(&self, id: NoteId, at: (i32, i32)) {
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
        *self.0.context_menu.borrow_mut() = Some(Rc::new(menu));
    }

    /// How many right-click menus have been opened.
    pub fn menus_opened(&self) -> usize {
        self.0.menus_opened.get()
    }

    /// The open right-click menu, if any.
    pub fn context_menu(&self) -> Option<Rc<ContextMenu>> {
        self.0.context_menu.borrow().clone()
    }

    /// Close the right-click menu.
    pub fn dismiss_context_menu(&self) {
        let menu = self.0.context_menu.borrow_mut().take();
        if let Some(m) = menu {
            m.close();
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
            if now_empty && self.all_notes().is_none() {
                self.0.restore_exit_candidates();
                self.0.quit.set(true);
                let _ = slint::quit_event_loop();
            }
        }
    }

    /// Exit for the uninstaller that was just started: the same exit as closing the last note, so
    /// open notes come back if Washi is installed again.
    pub fn quit_for_uninstall(&self) {
        self.0.save_all();
        self.0.restore_exit_candidates();
        self.0.quit.set(true);
        let _ = slint::quit_event_loop();
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

impl crate::all_notes::AllNotesDelegate for App {
    fn desktop_note_geometry(&self, id: NoteId) -> Option<Geometry> {
        self.with_note(id, |n| n.geometry())
    }

    fn desktop_note_hwnd(&self, id: NoteId) -> Option<isize> {
        self.with_note(id, |n| n.hwnd()).flatten()
    }

    fn set_desktop_note_visible(&self, id: NoteId, visible: bool) {
        let show = visible && !self.settings().is_some_and(|s| s.host() == id);
        self.with_note(id, |n| n.set_visible(show));
    }

    fn park_desktop_note(&self, id: NoteId, at: (i32, i32)) {
        if self.settings().is_some_and(|s| s.host() == id) {
            return;
        }
        self.with_note(id, |n| {
            n.set_saves_geometry(false);
            n.ui().window().set_position(slint::LogicalPosition::new(at.0 as f32, at.1 as f32));
            n.set_visible(true);
        });
    }

    fn restore_desktop_note(&self, id: NoteId, at: Geometry, home: Geometry) {
        self.with_note(id, |n| {
            n.set_saves_geometry(true);
            let now = n.geometry();
            if (now.x, now.y) != (at.x, at.y) {
                n.geometry_changed();
            } else if at != home {
                n.place(home);
            }
        });
    }

    fn desktop_note_text(&self, id: NoteId) -> Option<String> {
        self.with_note(id, |n| n.text())
    }

    fn set_desktop_note_text(&self, id: NoteId, text: &str) {
        self.with_note(id, |n| {
            if n.text() != text {
                n.set_text(text);
            }
        });
    }

    fn flush_open_notes(&self) {
        for n in self.0.notes.borrow().iter() {
            n.flush_text();
        }
        self.0.save_stacking();
    }

    fn open_desktop_note(&self, id: NoteId) {
        self.activate_note(id);
    }

    fn close_desktop_note(&self, id: NoteId) {
        self.close_note(id);
    }

    fn create_new_note(&self, from: Option<NoteId>) -> Option<NoteId> {
        self.create_new_note(from)
    }

    fn delete_note(&self, id: NoteId) {
        self.confirm_delete_note(id);
    }

    fn export_note(&self, id: NoteId) {
        self.export_note(id);
    }

    fn toggle_dropdown(&self, _id: NoteId) {
        self.toggle_editor_dropdown();
    }

    fn dismiss_dropdown(&self) {
        self.dismiss_dropdown();
    }

    fn poke_suspend(&self) {
        self.0.suspend.poke();
    }

    fn is_settings_host(&self, id: NoteId) -> bool {
        self.settings().is_some_and(|s| s.host() == id)
    }
}
