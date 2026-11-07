//! Everything Washi gets from Windows sits behind these interfaces. The Windows
//! implementations live in `crate::win`; test fakes live in the `snap` harness crate.

use std::path::PathBuf;
use std::time::Duration;

/// Stable id of a note (the `notes.id` row).
pub type NoteId = i64;

/// Wraps and unwraps the database key with a secret tied to the OS user (DPAPI on Windows).
pub trait KeyProtector {
    /// Produce a fresh random 256-bit key.
    fn generate_key(&self) -> [u8; 32];
    fn protect(&self, raw: &[u8]) -> Result<Vec<u8>, ProtectError>;
    /// Must fail (never return garbage) if `wrapped` was altered.
    fn unprotect(&self, wrapped: &[u8]) -> Result<Vec<u8>, ProtectError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectError(pub String);

impl std::fmt::Display for ProtectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ProtectError {}

/// Wall-clock and monotonic time. The fake lets tests advance time.
pub trait Clock {
    /// Milliseconds since the Unix epoch (stored in `created_at` / `updated_at`).
    fn unix_millis(&self) -> i64;
    /// Monotonic time since app start (drives Slint timers in the harness).
    fn monotonic(&self) -> Duration;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowState {
    pub minimized: bool,
    pub covered: bool,
    pub focused: bool,
}

/// Something the platform noticed about Washi's windows, delivered on the UI thread.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SignalEvent {
    /// A note minimized, restored, gained or lost focus, or Washi moved to or from the background.
    StateChanged,
    /// Windows asked a note to paint (for example because it was uncovered).
    Paint(NoteId),
}

/// Window-state signals used by the suspend controller.
pub trait WindowSignals {
    fn window_state(&self, note: NoteId) -> WindowState;
    fn app_in_background(&self) -> bool;
    /// A note's native window now exists / is gone. Fakes ignore this.
    fn attach(&self, _note: NoteId, _hwnd: isize) {}
    fn detach(&self, _note: NoteId) {}
    /// Deliver [`SignalEvent`]s to `notify` (on the UI thread, never from inside a window
    /// procedure) so a suspended app wakes without polling. Fakes ignore this: tests call the
    /// app directly.
    fn set_notifier(&self, _notify: std::rc::Rc<dyn Fn(SignalEvent)>) {}
}

/// Efficiency mode and memory release (suspension, a later ticket).
pub trait ProcessPower {
    fn set_efficiency_mode(&self, on: bool);
    fn release_memory(&self);
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MenuItem {
    pub id: u32,
    pub label: String,
    pub checked: bool,
    pub enabled: bool,
    pub children: Vec<MenuItem>,
}

/// The right-click menu. Returns the chosen item id, or `None` if dismissed.
pub trait ContextMenu {
    fn show(&self, items: &[MenuItem], at: (i32, i32)) -> Option<u32>;
}

/// The "Start with Windows" setting: a per-user registration that launches Washi at sign-in
/// (the `Run` registry value on Windows).
pub trait StartupRegistration {
    fn is_enabled(&self) -> bool;
    fn set_enabled(&self, on: bool);
}

/// What a launch found when it looked for an already running Washi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launch {
    /// This process is the one running Washi.
    First,
    /// Washi was already running and has been asked to open a new note; this process should exit.
    HandedOff,
}

/// One running Washi per user. A later launch hands off to the first.
pub trait SingleInstance {
    fn acquire(&self) -> Launch;
    /// First instance only: `on_request` runs on the UI thread each time a later launch asks
    /// for a new note.
    fn listen(&self, on_request: std::rc::Rc<dyn Fn()>);
}

/// Message boxes and file pickers.
pub trait Dialogs {
    fn confirm(&self, message: &str) -> bool;
    fn error(&self, message: &str);
    fn pick_open_file(&self, filter: &str) -> Option<PathBuf>;
    fn pick_save_file(&self, suggested_name: &str, filter: &str) -> Option<PathBuf>;
}
