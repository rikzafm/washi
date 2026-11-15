//! Windows implementations of the `crate::platform` interfaces.

mod clipboard;
mod dialogs;
mod dpapi;
mod instance;
mod process;
mod signals;
mod startup;
mod system;
mod window_control;

pub use clipboard::Win32Clipboard;
pub use dialogs::Win32Dialogs;
pub use dpapi::DpapiProtector;
pub use instance::Win32Instance;
pub use process::Win32Power;
pub use signals::Win32Signals;
pub use startup::Win32Startup;
pub use system::{Win32Shell, Win32Stats};
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

/// Give a window the Washi logo (the .exe's icon resource, id 1) for its taskbar button, thumbnail
/// and Alt+Tab entry. Slint's own `icon` property does not reach these for our frameless windows.
pub fn set_window_icon(hwnd: isize) {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    // SAFETY: plain Win32 calls; the icons are shared (LR_SHARED), so they need no cleanup.
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        for (kind, cx, cy) in [
            (ICON_BIG, GetSystemMetrics(SM_CXICON), GetSystemMetrics(SM_CYICON)),
            (ICON_SMALL, GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON)),
        ] {
            let icon = LoadImageW(module, 1 as _, IMAGE_ICON, cx, cy, LR_SHARED);
            if !icon.is_null() {
                SendMessageW(hwnd as _, WM_SETICON, kind as usize, icon as isize);
                // The taskbar's preview list falls back to the window class icon for its title row.
                let slot = if kind == ICON_BIG { GCLP_HICON } else { GCLP_HICONSM };
                SetClassLongPtrW(hwnd as _, slot, icon as isize);
            }
        }
    }
}

thread_local! {
    /// Whether this process is the real app ([`set_app_identity`] ran), whose windows are native ones.
    static NATIVE_WINDOWS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Like [`set_window_icon`] for a window that was just shown: winit creates the native window a
/// little after `show()`, so there is no handle yet. Look again every few ms until it exists.
///
/// Only in the real app. Anywhere else (the test harness draws without native windows) a handle
/// never comes, and the retries would be timers left running: a suspended app must have none.
pub fn set_window_icon_when_ready<W: slint::ComponentHandle + 'static>(ui: &W) {
    if !NATIVE_WINDOWS.with(|n| n.get()) {
        return;
    }
    fn attempt<W: slint::ComponentHandle + 'static>(weak: slint::Weak<W>, tries: u32) {
        let Some(ui) = weak.upgrade() else { return };
        if let Some(h) = crate::popup::hwnd_of(ui.window()) {
            set_window_icon(h);
        } else if tries < 400 {
            slint::Timer::single_shot(Duration::from_millis(5), move || attempt(weak, tries + 1));
        }
    }
    attempt(ui.as_weak(), 0);
}

/// Group every note's taskbar button under one Washi identity. Call once at startup, before
/// any window exists.
pub fn set_app_identity() {
    NATIVE_WINDOWS.with(|n| n.set(true));
    let id = wide("Washi.Notes");
    // SAFETY: NUL-terminated string, copied by the call.
    unsafe { windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(id.as_ptr()) };
}
