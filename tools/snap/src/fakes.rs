//! Test fakes for every `washi::platform` interface.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use washi::platform::*;

/// Reversible fake of DPAPI: XOR plus a checksum byte, so any altered byte fails to unwrap.
pub struct FakeProtector {
    seed: u8,
    rng_fails: bool,
}

impl FakeProtector {
    pub fn new(seed: u8) -> Self {
        FakeProtector { seed, rng_fails: false }
    }

    /// A protector whose random generator is broken: `generate_key` fails.
    pub fn with_broken_rng() -> Self {
        FakeProtector { seed: 0, rng_fails: true }
    }
}

impl KeyProtector for FakeProtector {
    fn generate_key(&self) -> Result<Zeroizing<[u8; 32]>, ProtectError> {
        if self.rng_fails {
            return Err(ProtectError("the random generator failed".into()));
        }
        let mut k = Zeroizing::new([0u8; 32]);
        for (i, b) in k.iter_mut().enumerate() {
            *b = self.seed.wrapping_mul(31).wrapping_add((i as u8).wrapping_mul(7)).wrapping_add(1);
        }
        Ok(k)
    }

    fn protect(&self, raw: &[u8]) -> Result<Vec<u8>, ProtectError> {
        let sum = raw.iter().fold(0u8, |a, b| a.wrapping_add(*b));
        let mut out: Vec<u8> = raw.iter().map(|b| b ^ 0xA5).collect();
        out.push(sum);
        Ok(out)
    }

    fn unprotect(&self, wrapped: &[u8]) -> Result<Zeroizing<Vec<u8>>, ProtectError> {
        let (&sum, body) = wrapped.split_last().ok_or_else(|| ProtectError("empty blob".into()))?;
        let raw: Vec<u8> = body.iter().map(|b| b ^ 0xA5).collect();
        if raw.iter().fold(0u8, |a, b| a.wrapping_add(*b)) != sum {
            return Err(ProtectError("blob was altered".into()));
        }
        Ok(Zeroizing::new(raw))
    }
}

/// Fake clock: wall time = fixed epoch + fake monotonic time.
pub struct FakeClock {
    mono: Rc<Cell<Duration>>,
}

impl FakeClock {
    pub fn new(mono: Rc<Cell<Duration>>) -> Self {
        FakeClock { mono }
    }
}

impl Clock for FakeClock {
    fn unix_millis(&self) -> i64 {
        1_700_000_000_000 + self.mono.get().as_millis() as i64
    }
    fn monotonic(&self) -> Duration {
        self.mono.get()
    }
}

/// Window-state signals that tests set directly.
#[derive(Default)]
pub struct FakeSignals {
    pub states: RefCell<HashMap<NoteId, WindowState>>,
    pub background: Cell<bool>,
    session_end: RefCell<Option<Rc<dyn Fn()>>>,
}

impl FakeSignals {
    /// Windows is ending the session (WM_QUERYENDSESSION / WM_ENDSESSION).
    pub fn end_session(&self) {
        let handler = self.session_end.borrow().clone();
        if let Some(handler) = handler {
            handler();
        }
    }
}

impl WindowSignals for FakeSignals {
    fn set_session_end_handler(&self, handler: Rc<dyn Fn()>) {
        *self.session_end.borrow_mut() = Some(handler);
    }
    fn window_state(&self, note: NoteId) -> WindowState {
        self.states.borrow().get(&note).copied().unwrap_or_default()
    }
    fn app_in_background(&self) -> bool {
        self.background.get()
    }
}

/// Records efficiency-mode and memory-release calls.
#[derive(Default)]
pub struct FakePower {
    pub efficiency: Cell<bool>,
    pub releases: Cell<u32>,
    /// Every call in order: `release_memory`, `efficiency_on`, `efficiency_off`.
    pub log: RefCell<Vec<String>>,
}

impl ProcessPower for FakePower {
    fn set_efficiency_mode(&self, on: bool) {
        self.efficiency.set(on);
        self.log.borrow_mut().push(if on { "efficiency_on" } else { "efficiency_off" }.into());
    }
    fn release_memory(&self) {
        self.releases.set(self.releases.get() + 1);
        self.log.borrow_mut().push("release_memory".into());
    }
}

/// Records the menus shown and answers with a queued choice: either a raw item id, or a path
/// of labels (e.g. `["Open note", "shopping list"]`) looked up in the menu actually shown.
#[derive(Default)]
pub struct FakeMenu {
    pub shown: RefCell<Vec<Vec<MenuItem>>>,
    pub next_choice: Cell<Option<u32>>,
    pub next_path: RefCell<Option<Vec<String>>>,
}

fn find<'a>(items: &'a [MenuItem], path: &[String]) -> Option<&'a MenuItem> {
    let (first, rest) = path.split_first()?;
    let it = items.iter().find(|i| &i.label == first)?;
    if rest.is_empty() {
        Some(it)
    } else {
        find(&it.children, rest)
    }
}

impl ContextMenu for FakeMenu {
    fn show(&self, items: &[MenuItem], _at: (i32, i32)) -> Option<u32> {
        self.shown.borrow_mut().push(items.to_vec());
        if let Some(path) = self.next_path.borrow_mut().take() {
            let it = find(items, &path).unwrap_or_else(|| panic!("no menu item {path:?} in {items:?}"));
            assert!(it.enabled, "menu item {path:?} is disabled");
            return Some(it.id);
        }
        self.next_choice.take()
    }
}

/// Scripted dialogs.
#[derive(Default)]
pub struct FakeDialogs {
    pub confirm_answer: Cell<bool>,
    pub errors: RefCell<Vec<String>>,
    pub open_path: RefCell<Option<PathBuf>>,
    pub save_path: RefCell<Option<PathBuf>>,
}

impl Dialogs for FakeDialogs {
    fn confirm(&self, _message: &str) -> bool {
        self.confirm_answer.get()
    }
    fn error(&self, message: &str) {
        self.errors.borrow_mut().push(message.to_string());
    }
    fn pick_open_file(&self, _filter: &str) -> Option<PathBuf> {
        self.open_path.borrow().clone()
    }
    fn pick_save_file(&self, _suggested_name: &str, _filter: &str) -> Option<PathBuf> {
        self.save_path.borrow().clone()
    }
}

/// The clipboard's picture and rich text, set by tests.
#[derive(Default)]
pub struct FakeClipboard {
    pub image: RefCell<Option<washi::images::Rgba>>,
    pub html: RefCell<Option<String>>,
    pub text: RefCell<Option<String>>,
}

impl ImageClipboard for FakeClipboard {
    fn image(&self) -> Option<washi::images::Rgba> {
        self.image.borrow().clone()
    }

    fn set_image(&self, img: &washi::images::Rgba) -> Result<(), String> {
        *self.image.borrow_mut() = Some(img.clone());
        *self.text.borrow_mut() = None;
        *self.html.borrow_mut() = None;
        Ok(())
    }

    fn set_rich(&self, payload: ClipboardPayload) -> Result<(), String> {
        *self.image.borrow_mut() = payload.image;
        *self.html.borrow_mut() = payload.html;
        *self.text.borrow_mut() = payload.plain;
        Ok(())
    }

    fn html(&self) -> Option<String> {
        self.html.borrow().clone()
    }

    fn text(&self) -> Option<String> {
        self.text.borrow().clone()
    }

    fn has_image(&self) -> bool {
        self.image.borrow().is_some()
    }

    fn has_text(&self) -> bool {
        self.text.borrow().is_some() || self.html.borrow().is_some()
    }
}

/// What the platform was asked to do with a note's window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowCommand {
    Move(NoteId),
    Resize(NoteId, ResizeEdge),
    BringToFront(NoteId),
}

/// Records native drag requests and reports a screen layout the test can change.
pub struct FakeWindowControl {
    pub log: RefCell<Vec<WindowCommand>>,
    pub areas: RefCell<Vec<ScreenRect>>,
    /// Bottom-to-top order the fake desktop reports; empty means "as asked".
    pub stacking: RefCell<Vec<NoteId>>,
}

impl Default for FakeWindowControl {
    fn default() -> Self {
        FakeWindowControl {
            log: RefCell::new(Vec::new()),
            // one 1920 x 1080 monitor with a 40 px taskbar
            areas: RefCell::new(vec![ScreenRect::new(0, 0, 1920, 1040)]),
            stacking: RefCell::new(Vec::new()),
        }
    }
}

impl WindowControl for FakeWindowControl {
    fn begin_move(&self, note: NoteId, _hwnd: Option<isize>) {
        self.log.borrow_mut().push(WindowCommand::Move(note));
    }
    fn begin_resize(&self, note: NoteId, _hwnd: Option<isize>, edge: ResizeEdge) {
        self.log.borrow_mut().push(WindowCommand::Resize(note, edge));
    }
    fn bring_to_front(&self, note: NoteId, _hwnd: Option<isize>) {
        self.log.borrow_mut().push(WindowCommand::BringToFront(note));
    }
    fn work_areas(&self) -> Vec<ScreenRect> {
        self.areas.borrow().clone()
    }
    fn stacking_order(&self, notes: &[(NoteId, Option<isize>)]) -> Vec<NoteId> {
        let set = self.stacking.borrow();
        let mut out: Vec<NoteId> = notes.iter().map(|(id, _)| *id).filter(|id| !set.contains(id)).collect();
        out.extend(set.iter().copied().filter(|id| notes.iter().any(|(n, _)| n == id)));
        out
    }
}

/// Stands in for the `Run` registry value: a flag that outlives the app, like the registry does.
#[derive(Default)]
pub struct FakeStartup {
    pub enabled: Cell<bool>,
}

impl StartupRegistration for FakeStartup {
    fn is_enabled(&self) -> bool {
        self.enabled.get()
    }
    fn set_enabled(&self, on: bool) {
        self.enabled.set(on);
    }
}

/// Stands in for the process numbers. `samples` counts how often memory was read: one per refresh.
#[derive(Default)]
pub struct FakeStats {
    pub samples: Cell<u32>,
    pub memory: Cell<u64>,
    pub cpu: Cell<Duration>,
}

impl ProcessStats for FakeStats {
    fn private_working_set(&self) -> u64 {
        self.samples.set(self.samples.get() + 1);
        self.memory.get()
    }
    fn cpu_time(&self) -> Duration {
        self.cpu.get()
    }
    fn processors(&self) -> u32 {
        4
    }
}

/// Records which folders were opened in Explorer.
#[derive(Default)]
pub struct FakeShell {
    pub opened: RefCell<Vec<PathBuf>>,
}

impl Shell for FakeShell {
    fn install_dir(&self) -> PathBuf {
        PathBuf::from(r"C:\Program Files\Washi")
    }
    fn open_folder(&self, path: &std::path::Path) {
        self.opened.borrow_mut().push(path.to_path_buf());
    }
}

/// Stands in for the named mutex and the wake-up event between Washi processes.
#[derive(Default)]
pub struct FakeInstance {
    claimed: Cell<bool>,
    listener: RefCell<Option<Rc<dyn Fn()>>>,
}

impl FakeInstance {
    /// The running process ended: the next launch is the first again.
    pub fn process_exited(&self) {
        self.claimed.set(false);
        *self.listener.borrow_mut() = None;
    }
}

impl SingleInstance for FakeInstance {
    fn acquire(&self) -> Launch {
        if !self.claimed.replace(true) {
            return Launch::First;
        }
        let listener = self.listener.borrow().clone();
        if let Some(wake) = listener {
            wake();
        }
        Launch::HandedOff
    }
    fn listen(&self, on_request: Rc<dyn Fn()>) {
        *self.listener.borrow_mut() = Some(on_request);
    }
}
