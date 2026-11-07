use super::wide;
use crate::platform::{ContextMenu, MenuItem};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetForegroundWindow, TrackPopupMenu, HMENU, MF_CHECKED, MF_GRAYED,
    MF_POPUP, MF_STRING, TPM_NONOTIFY, TPM_RETURNCMD,
};

pub struct Win32ContextMenu;

/// Item ids must be non-zero for TrackPopupMenu's return value to be distinguishable from "dismissed".
fn build(items: &[MenuItem]) -> HMENU {
    // SAFETY: plain Win32 menu construction; strings are NUL-terminated and copied by the call.
    unsafe {
        let menu = CreatePopupMenu();
        for it in items {
            let mut flags = MF_STRING;
            if it.checked {
                flags |= MF_CHECKED;
            }
            if !it.enabled {
                flags |= MF_GRAYED;
            }
            let label = wide(&it.label);
            if it.children.is_empty() {
                AppendMenuW(menu, flags, it.id as usize, label.as_ptr());
            } else {
                let sub = build(&it.children);
                AppendMenuW(menu, flags | MF_POPUP, sub as usize, label.as_ptr());
            }
        }
        menu
    }
}

impl ContextMenu for Win32ContextMenu {
    fn show(&self, items: &[MenuItem], at: (i32, i32)) -> Option<u32> {
        // SAFETY: the menu is destroyed (with its submenus) after tracking.
        unsafe {
            let menu = build(items);
            let id = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY, at.0, at.1, 0, GetForegroundWindow(), std::ptr::null());
            DestroyMenu(menu);
            (id != 0).then_some(id as u32)
        }
    }
}
