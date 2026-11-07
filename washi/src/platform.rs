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

/// Window-state signals used by the suspend controller (a later ticket).
pub trait WindowSignals {
    fn window_state(&self, note: NoteId) -> WindowState;
    fn app_in_background(&self) -> bool;
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

/// Message boxes and file pickers.
pub trait Dialogs {
    fn confirm(&self, message: &str) -> bool;
    fn error(&self, message: &str);
    fn pick_open_file(&self, filter: &str) -> Option<PathBuf>;
    fn pick_save_file(&self, suggested_name: &str, filter: &str) -> Option<PathBuf>;
}
