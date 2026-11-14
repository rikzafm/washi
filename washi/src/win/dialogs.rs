use super::wide;
use crate::platform::Dialogs;
use std::path::PathBuf;
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONERROR, MB_ICONWARNING, MB_OK, MB_YESNO,
};

pub struct Win32Dialogs;

/// `filter` is "Description|*.ext" pairs separated by `|`, e.g. "Markdown|*.md".
fn filter_buf(filter: &str) -> Vec<u16> {
    let mut v: Vec<u16> = Vec::new();
    for part in filter.split('|') {
        v.extend(part.encode_utf16());
        v.push(0);
    }
    v.push(0);
    v
}

fn run_dialog(save: bool, name: &str, filter: &str) -> Option<PathBuf> {
    let mut file = vec![0u16; 1024];
    for (i, c) in name.encode_utf16().take(1000).enumerate() {
        file[i] = c;
    }
    let filter = filter_buf(filter);
    // SAFETY: zeroed OPENFILENAMEW is the documented starting state; buffers outlive the call.
    let ok = unsafe {
        let mut ofn: OPENFILENAMEW = std::mem::zeroed();
        ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        ofn.lpstrFilter = filter.as_ptr();
        ofn.lpstrFile = file.as_mut_ptr();
        ofn.nMaxFile = file.len() as u32;
        if save {
            ofn.Flags = OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST;
            GetSaveFileNameW(&mut ofn)
        } else {
            ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST;
            GetOpenFileNameW(&mut ofn)
        }
    };
    if ok == 0 {
        return None;
    }
    let len = file.iter().position(|&c| c == 0).unwrap_or(file.len());
    Some(PathBuf::from(String::from_utf16_lossy(&file[..len])))
}

impl Dialogs for Win32Dialogs {
    fn confirm(&self, message: &str) -> bool {
        // SAFETY: both strings are NUL-terminated and live across the call.
        unsafe { MessageBoxW(std::ptr::null_mut(), wide(message).as_ptr(), wide("Washi Note").as_ptr(), MB_YESNO | MB_ICONWARNING) == IDYES }
    }
    fn error(&self, message: &str) {
        // SAFETY: as above.
        unsafe {
            MessageBoxW(std::ptr::null_mut(), wide(message).as_ptr(), wide("Washi Note").as_ptr(), MB_OK | MB_ICONERROR);
        }
    }
    fn pick_open_file(&self, filter: &str) -> Option<PathBuf> {
        run_dialog(false, "", filter)
    }
    fn pick_save_file(&self, suggested_name: &str, filter: &str) -> Option<PathBuf> {
        run_dialog(true, suggested_name, filter)
    }
}
