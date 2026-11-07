//! Windows implementations of the `crate::platform` interfaces.

mod clipboard;
mod dialogs;
mod dpapi;
mod instance;
mod menu;
mod process;
mod signals;
mod startup;
mod window_control;

pub use clipboard::Win32Clipboard;
pub use dialogs::Win32Dialogs;
pub use dpapi::DpapiProtector;
pub use instance::Win32Instance;
pub use menu::Win32ContextMenu;
pub use process::Win32Power;
pub use signals::Win32Signals;
pub use startup::Win32Startup;
pub use window_control::Win32WindowControl;

use crate::platform::Clock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct SystemClock {
    start: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        SystemClock { start: Instant::now() }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn unix_millis(&self) -> i64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
    }
    fn monotonic(&self) -> Duration {
        self.start.elapsed()
    }
}

/// Group every note's taskbar button under one Washi identity. Call once at startup, before
/// any window exists.
pub fn set_app_identity() {
    let id = wide("Washi.Notes");
    // SAFETY: NUL-terminated string, copied by the call.
    unsafe { windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(id.as_ptr()) };
}
