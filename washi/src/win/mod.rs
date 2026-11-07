//! Windows implementations of the `crate::platform` interfaces.

mod dialogs;
mod dpapi;
mod menu;
mod process;
mod signals;

pub use dialogs::Win32Dialogs;
pub use dpapi::DpapiProtector;
pub use menu::Win32ContextMenu;
pub use process::Win32Power;
pub use signals::Win32Signals;

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
