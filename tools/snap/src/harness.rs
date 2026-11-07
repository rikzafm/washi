use crate::fakes::*;
use crate::platform;
use slint::platform::{Key, WindowEvent};
use slint::ComponentHandle;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use washi::platform::{KeyProtector, NoteId};
use washi::{App, Env, StartError};

/// One app instance's world: a temporary data folder, fakes for Windows, and the running app.
pub struct Harness {
    dir: tempfile::TempDir,
    env: Env,
    pub signals: Rc<FakeSignals>,
    pub power: Rc<FakePower>,
    pub menu: Rc<FakeMenu>,
    pub dialogs: Rc<FakeDialogs>,
    app: Option<App>,
    last_ids: Vec<NoteId>,
}

impl Default for Harness {
    fn default() -> Self {
        Self::new()
    }
}

impl Harness {
    pub fn new() -> Harness {
        Self::with_key_seed(1)
    }

    /// `seed` picks the (deterministic) test key.
    pub fn with_key_seed(seed: u8) -> Harness {
        platform::install();
        let dir = tempfile::tempdir().expect("temp data folder");
        let signals = Rc::new(FakeSignals::default());
        let power = Rc::new(FakePower::default());
        let menu = Rc::new(FakeMenu::default());
        let dialogs = Rc::new(FakeDialogs::default());
        let env = Env {
            data_dir: dir.path().to_path_buf(),
            protector: Rc::new(FakeProtector::new(seed)),
            clock: Rc::new(FakeClock::new(platform::time_handle())),
            signals: signals.clone(),
            power: power.clone(),
            menu: menu.clone(),
            dialogs: dialogs.clone(),
        };
        Harness { dir, env, signals, power, menu, dialogs, app: None, last_ids: Vec::new() }
    }

    fn app(&self) -> &App {
        self.app.as_ref().expect("app is not running")
    }

    fn id(&self, note: usize) -> NoteId {
        self.app().note_ids()[note]
    }

    // ---- lifecycle ----

    pub fn start(&mut self) -> Result<(), StartError> {
        self.app = None;
        let app = App::start(self.env.clone())?;
        // Give every window the focus a real one would have.
        for id in app.note_ids() {
            app.with_note(id, |n| n.ui().window().dispatch_event(WindowEvent::WindowActiveChanged(true)));
        }
        self.app = Some(app);
        Ok(())
    }

    /// Quit (saving), drop the app entirely, and start again on the same data folder.
    pub fn restart(&mut self) -> Result<(), StartError> {
        self.quit();
        self.app = None;
        self.start()
    }

    pub fn quit(&mut self) {
        if let Some(app) = &self.app {
            self.last_ids = app.note_ids();
            app.quit();
        }
    }

    pub fn has_quit(&self) -> bool {
        self.app().has_quit()
    }

    // ---- act ----

    pub fn type_text(&mut self, note: usize, text: &str) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let w = n.ui().window();
            for ch in text.chars() {
                w.dispatch_event(WindowEvent::KeyPressed { text: ch.to_string().into() });
                w.dispatch_event(WindowEvent::KeyReleased { text: ch.to_string().into() });
            }
        });
    }

    /// Put text into the note without typing it (fixtures).
    pub fn set_note_text(&mut self, note: usize, text: &str) {
        let id = self.id(note);
        self.app().with_note(id, |n| n.ui().set_text(text.into()));
    }

    pub fn press_backspace(&mut self, note: usize) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let w = n.ui().window();
            w.dispatch_event(WindowEvent::KeyPressed { text: Key::Backspace.into() });
            w.dispatch_event(WindowEvent::KeyReleased { text: Key::Backspace.into() });
        });
    }

    pub fn advance(&mut self, by: Duration) {
        platform::advance(by);
    }

    pub fn close_note(&mut self, note: usize) {
        self.last_ids = self.app().note_ids();
        let id = self.last_ids[note];
        self.app().close_note(id);
    }

    // ---- check ----

    pub fn open_note_count(&self) -> usize {
        self.app().note_ids().len()
    }

    /// Text currently in the note's editor.
    pub fn note_text(&self, note: usize) -> String {
        self.app().with_note(self.id(note), |n| n.text()).unwrap()
    }

    /// Text last written to the database for the note.
    pub fn saved_text(&self, note: usize) -> String {
        self.app().saved_text(self.id(note)).unwrap()
    }

    /// For use after the last note was closed.
    pub fn saved_text_of_only_note(&self) -> String {
        assert_eq!(self.last_ids.len(), 1);
        self.app().saved_text(self.last_ids[0]).unwrap()
    }

    pub fn schema_version(&self) -> u32 {
        self.app().schema_version()
    }

    pub fn pragma(&self, name: &str) -> String {
        self.app().pragma(name)
    }

    /// Render a note's window to RGB bytes (width, height, pixels).
    pub fn render(&self, note: usize) -> (u32, u32, Vec<slint::Rgb8Pixel>) {
        let w = platform::windows().remove(note);
        let (pw, ph) = platform::WINDOW_SIZE;
        let mut buf = vec![slint::Rgb8Pixel::default(); (pw * ph) as usize];
        w.draw_if_needed(|r| {
            r.render(&mut buf, pw as usize);
        });
        (pw, ph, buf)
    }

    /// Raw bytes of the database file and its `-wal` (empty if there is none).
    pub fn raw_db_bytes(&self) -> (Vec<u8>, Vec<u8>) {
        let db = std::fs::read(self.path("notes.db")).unwrap_or_default();
        let wal = std::fs::read(self.path("notes.db-wal")).unwrap_or_default();
        (db, wal)
    }

    // ---- sabotage ----

    pub fn corrupt_key_file(&self) {
        let p = self.path("key.bin");
        let mut b = std::fs::read(&p).unwrap();
        b[3] ^= 1;
        std::fs::write(p, b).unwrap();
    }

    pub fn replace_key_with_another_valid_key(&self) {
        let other = FakeProtector::new(99);
        let wrapped = other.protect(&other.generate_key()).unwrap();
        std::fs::write(self.path("key.bin"), wrapped).unwrap();
    }

    pub fn damage_database_header(&self) {
        let p = self.path("notes.db");
        let mut b = std::fs::read(&p).unwrap();
        for byte in b.iter_mut().take(32) {
            *byte ^= 0xFF;
        }
        std::fs::write(p, b).unwrap();
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
}
