use super::wide;
use crate::platform::StartupRegistration;
use windows_sys::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "Washi";

/// "Start with Windows": the per-user `Run` value pointing at this executable.
pub struct Win32Startup;

impl StartupRegistration for Win32Startup {
    fn is_enabled(&self) -> bool {
        let (key, name) = (wide(RUN_KEY), wide(VALUE));
        let mut size = 0u32;
        // SAFETY: size-only query; every pointer is valid for the call.
        let status = unsafe {
            RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), std::ptr::null_mut(), &mut size)
        };
        status == 0
    }

    fn set_enabled(&self, on: bool) {
        let (key, name) = (wide(RUN_KEY), wide(VALUE));
        // SAFETY: strings are NUL-terminated; the data length counts bytes including the NUL.
        unsafe {
            if on {
                let Ok(exe) = std::env::current_exe() else { return };
                let command = wide(&format!("\"{}\"", exe.display()));
                RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_SZ, command.as_ptr().cast(), (command.len() * 2) as u32);
            } else {
                RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr());
            }
        }
    }
}
