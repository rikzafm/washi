use super::wide;
use crate::platform::{ProcessStats, Shell};
use std::path::{Path, PathBuf};
use std::time::Duration;
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX2};
use windows_sys::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// Washi's own process numbers from Win32.
pub struct Win32Stats;

impl ProcessStats for Win32Stats {
    fn private_working_set(&self) -> u64 {
        // SAFETY: `c` is zeroed with its size set; the call fills it.
        unsafe {
            let mut c: PROCESS_MEMORY_COUNTERS_EX2 = std::mem::zeroed();
            c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
            if GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut _, c.cb) == 0 {
                return 0;
            }
            c.PrivateWorkingSetSize as u64
        }
    }

    fn cpu_time(&self) -> Duration {
        // SAFETY: all four out-pointers are valid FILETIMEs.
        unsafe {
            let mut creation: FILETIME = std::mem::zeroed();
            let mut exit: FILETIME = std::mem::zeroed();
            let mut kernel: FILETIME = std::mem::zeroed();
            let mut user: FILETIME = std::mem::zeroed();
            if GetProcessTimes(GetCurrentProcess(), &mut creation, &mut exit, &mut kernel, &mut user) == 0 {
                return Duration::ZERO;
            }
            let to_nanos = |f: FILETIME| (((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64) * 100;
            Duration::from_nanos(to_nanos(kernel) + to_nanos(user))
        }
    }

    fn processors(&self) -> u32 {
        // SAFETY: `si` is a zeroed out-struct.
        unsafe {
            let mut si: SYSTEM_INFO = std::mem::zeroed();
            GetSystemInfo(&mut si);
            si.dwNumberOfProcessors.max(1)
        }
    }
}

/// Inno Setup's `unins000.exe`, `unins001.exe`, ... Byte-wise, so a non-ASCII name cannot panic.
fn is_uninstaller_name(n: &str) -> bool {
    n.len() == 12 && n.starts_with("unins") && n.ends_with(".exe") && n.as_bytes()[5..8].iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::is_uninstaller_name;

    #[test]
    fn recognises_inno_uninstallers() {
        assert!(is_uninstaller_name("unins000.exe"));
        assert!(is_uninstaller_name("unins012.exe"));
        assert!(!is_uninstaller_name("unins00.exe"));
        assert!(!is_uninstaller_name("uninsabc.exe"));
        assert!(!is_uninstaller_name("unins000.dll"));
    }

    #[test]
    fn non_ascii_twelve_byte_name_does_not_panic() {
        // 12 bytes, with a 3-byte char where the digits should be.
        let n = "unins\u{20ac}.exe";
        assert_eq!(n.len(), 12);
        assert!(!is_uninstaller_name(n));
    }
}

/// Explorer and the install location.
pub struct Win32Shell;

impl Shell for Win32Shell {
    fn install_dir(&self) -> PathBuf {
        std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default()
    }

    fn uninstaller(&self) -> Option<PathBuf> {
        // Inno Setup writes unins000.exe (unins001.exe, ... after a reinstall) beside the app.
        std::fs::read_dir(self.install_dir())
            .ok()?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(is_uninstaller_name)
            })
            .min()
    }

    fn launch(&self, exe: &Path) -> Result<(), String> {
        std::process::Command::new(exe).spawn().map(|_| ()).map_err(|e| e.to_string())
    }

    fn open_folder(&self, path: &Path) {
        let (verb, target) = (wide("open"), wide(&path.to_string_lossy()));
        // SAFETY: NUL-terminated strings; the shell opens the folder itself, no command line is built.
        unsafe {
            ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), target.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL);
        }
    }
}
