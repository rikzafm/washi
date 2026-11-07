//! The app: opens the encrypted store, shows the open notes, and owns their lifecycle.

use crate::images::{self, ImageError, Source, Stored};
use crate::note_window::{note_title, Note, NoteEvent};
use crate::platform::*;
use crate::suspend::{SuspendController, SuspendHost};
use crate::storage::{keystore, Db, DbError, Geometry, KeyStoreError, NoteRow};
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

struct Inner {
    env: Env,
    db: Rc<Db>,
    notes: RefCell<Vec<Rc<Note>>>,
    quit: Cell<bool>,
    suspend: Rc<SuspendController>,
    /// Pictures being prepared on their worker threads, and the timer that collects them (it runs
    /// only while there are any).
    imports: RefCell<Vec<Import>>,
    import_timer: slint::Timer,
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

#[derive(Clone)]
pub struct App(Rc<Inner>);

const MENU_NEW: u32 = 1;
const MENU_OPEN: u32 = 2;
const MENU_CLOSE: u32 = 3;
const MENU_DELETE: u32 = 4;
const MENU_TOP: u32 = 5;
const MENU_STARTUP: u32 = 6;
const MENU_EXPORT: u32 = 7;
/// Closed notes in the "Open note" submenu get `MENU_OPEN_BASE + position`.
const MENU_OPEN_BASE: u32 = 1000;

/// A new note opens this far (logical px, right and down) from the note it was made from.
const NEW_NOTE_OFFSET: i32 = 30;

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
        let last = self.0.notes.borrow().last().map(|n| n.id());
        match last {
            Some(id) => self.new_note_from(id),
            None => {} // Washi is quitting; nothing to attach to.
        }
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
        let inner = Rc::new(Inner { env, db, notes: RefCell::new(Vec::new()), quit: Cell::new(false), suspend, imports: RefCell::new(Vec::new()), import_timer: slint::Timer::default() });
        let host: Rc<dyn SuspendHost> = inner.clone();
        inner.suspend.attach(Rc::downgrade(&host));
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
        let note = Note::open(row.id, &row.content, self.0.db.clone(), self.0.env.clock.clone(), row.geometry, row.always_on_top, on_event, paste_image)
            .map_err(StartError::Ui)?;
        if row.geometry.is_some() {
            // a note saved on a monitor that is gone comes back on the main screen
            note.keep_on_screen(&self.0.env.window_control.work_areas());
        }
        if let Some(hwnd) = note.hwnd() {
            self.0.env.signals.attach(row.id, hwnd);
        }
        self.0.notes.borrow_mut().push(Rc::new(note));
        self.0.suspend.evaluate();
        Ok(())
    }

    fn handle(&self, id: NoteId, ev: NoteEvent) {
        match ev {
            NoteEvent::WindowClosed | NoteEvent::Close => self.close_note(id),
            NoteEvent::New => self.new_note_from(id),
            NoteEvent::Delete => self.delete_note(id),
            NoteEvent::InsertImage => self.insert_image_from_file(id),
            NoteEvent::ContextMenu { at } => self.show_menu(id, at),
            NoteEvent::PointerMoved => self.0.suspend.poke(),
            NoteEvent::StartMove => {
                let hwnd = self.with_note(id, |n| n.hwnd()).flatten();
                self.0.env.window_control.begin_move(id, hwnd);
            }
            NoteEvent::StartResize(edge) => {
                let hwnd = self.with_note(id, |n| n.hwnd()).flatten();
                self.0.env.window_control.begin_resize(id, hwnd, edge);
            }
        }
    }

    /// The platform saw a note move or resize: remember where it is once the user stops.
    pub fn geometry_changed(&self, id: NoteId) {
        self.with_note(id, |n| n.geometry_changed());
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
        self.0.suspend.evaluate();
    }

    /// Windows asked a note to paint (for example because it was uncovered).
    pub fn paint_requested(&self, _id: NoteId) {
        self.0.suspend.poke();
    }

    /// Real app: have the platform tell us about minimize, restore, focus, activation and
    /// paint requests as they happen (no polling, so a suspended app sleeps until a message).
    pub fn watch_window_state(&self) {
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

    pub fn is_suspended(&self) -> bool {
        self.0.suspend.is_suspended()
    }

    pub fn has_quit(&self) -> bool {
        self.0.quit.get()
    }

    /// Close a note: save it, remember where it was, mark it closed and release its window.
    /// Closing the last open note quits Washi.
    pub fn close_note(&self, id: NoteId) {
        self.with_note(id, |n| {
            n.flush();
            n.prune_images();
            if let Err(e) = self.0.db.save_geometry(id, n.geometry()) {
                eprintln!("washi: could not save the position of note {id}: {e}");
            }
        });
        if let Err(e) = self.0.db.set_open(id, false) {
            eprintln!("washi: could not mark note {id} closed: {e}");
        }
        self.remove_note(id);
    }

    /// Delete a note for good, after asking.
    pub fn delete_note(&self, id: NoteId) {
        if !self.0.env.dialogs.confirm("Delete this note permanently? This cannot be undone.") {
            return;
        }
        if let Err(e) = self.0.db.delete_note(id) {
            self.0.env.dialogs.error(&format!("Washi could not delete the note: {e}"));
            return;
        }
        self.remove_note(id);
    }

    /// Open an empty note, offset from `from`.
    pub fn new_note_from(&self, from: NoteId) {
        let geometry = self
            .with_note(from, |n| n.geometry())
            .map(|g| Geometry { x: g.x + NEW_NOTE_OFFSET, y: g.y + NEW_NOTE_OFFSET, ..g });
        let id = match self.0.db.create_note(self.0.env.clock.unix_millis()) {
            Ok(id) => id,
            Err(e) => return self.0.env.dialogs.error(&format!("Washi could not create a note: {e}")),
        };
        if let Some(g) = geometry {
            let _ = self.0.db.save_geometry(id, g);
        }
        if let Err(e) = self.open_row(&NoteRow { id, content: String::new(), geometry, always_on_top: false }) {
            self.0.env.dialogs.error(&e.to_string());
        }
    }

    /// Reopen a closed note where it was.
    pub fn reopen_note(&self, id: NoteId) {
        if self.with_note(id, |_| ()).is_some() {
            return;
        }
        let Ok(Some(row)) = self.0.db.note(id) else { return };
        if self.0.db.set_open(id, true).is_ok() {
            if let Err(e) = self.open_row(&row) {
                self.0.env.dialogs.error(&e.to_string());
            }
        }
    }

    /// Keep a note's window above the others (or not), and remember it.
    pub fn set_always_on_top(&self, id: NoteId, on: bool) {
        self.with_note(id, |n| n.set_always_on_top(on));
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
        self.0.imports.borrow_mut().push(Import { note, after, result: images::spawn_prepare(source) });
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
        let Some((text, title)) = self.with_note(id, |n| {
            n.flush();
            (n.text(), n.title())
        }) else {
            return;
        };
        let Some(path) = self.0.env.dialogs.pick_save_file(&crate::export::suggested_name(&title), "Markdown|*.md") else { return };
        if let Err(e) = crate::export::write(&self.0.db, &text, &path) {
            self.0.env.dialogs.error(&format!("Washi could not export the note: {e}"));
        }
    }

    fn show_menu(&self, id: NoteId, at: (i32, i32)) {
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
        let items = [
            item(MENU_NEW, "New note", true, vec![]),
            item(MENU_OPEN, "Open note", !open_children.is_empty(), open_children),
            item(MENU_CLOSE, "Close", true, vec![]),
            item(MENU_DELETE, "Delete", true, vec![]),
            toggle(MENU_TOP, "Always on top", pinned),
            toggle(MENU_STARTUP, "Start with Windows", self.0.env.startup.is_enabled()),
            item(MENU_EXPORT, "Export as Markdown\u{2026}", true, vec![]),
        ];
        match self.0.env.menu.show(&items, at) {
            Some(MENU_NEW) => self.new_note_from(id),
            Some(MENU_CLOSE) => self.close_note(id),
            Some(MENU_DELETE) => self.delete_note(id),
            Some(MENU_TOP) => self.set_always_on_top(id, !pinned),
            Some(MENU_EXPORT) => self.export_note(id),
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
        let (note, now_empty) = {
            let mut notes = self.0.notes.borrow_mut();
            let at = notes.iter().position(|n| n.id() == id);
            (at.map(|i| notes.remove(i)), notes.is_empty())
        };
        if let Some(note) = note {
            self.0.env.signals.detach(id);
            slint::Timer::single_shot(Duration::ZERO, move || drop(note));
            self.0.suspend.evaluate();
            if now_empty {
                self.0.quit.set(true);
                let _ = slint::quit_event_loop();
            }
        }
    }

    /// Save everything immediately. Notes stay open, so the next start reopens them.
    pub fn quit(&self) {
        for n in self.0.notes.borrow().iter() {
            n.flush();
            let _ = self.0.db.save_geometry(n.id(), n.geometry());
        }
        self.0.quit.set(true);
    }
}

impl SuspendHost for Inner {
    fn open_notes(&self) -> Vec<NoteId> {
        self.notes.borrow().iter().map(|n| n.id()).collect()
    }

    fn save_all(&self) {
        let notes: Vec<_> = self.notes.borrow().clone();
        for n in notes {
            n.flush();
            if let Err(e) = self.db.save_geometry(n.id(), n.geometry()) {
                eprintln!("washi: could not save the position of note {}: {e}", n.id());
            }
        }
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
