use super::wide;
use crate::platform::{Launch, SingleInstance};
use std::cell::RefCell;
use std::rc::Rc;
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject, INFINITE};

const MUTEX_NAME: &str = "Local\\Washi-SingleInstance";
const EVENT_NAME: &str = "Local\\Washi-NewNote";

thread_local! {
    /// Set on the UI thread by `listen`; the waiter thread asks the event loop to run it.
    static ON_REQUEST: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

fn new_note_event() -> usize {
    let name = wide(EVENT_NAME);
    // SAFETY: auto-reset, initially unsignalled; opens the existing event if there is one.
    unsafe { CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) as usize }
}

/// A named mutex marks the running Washi; a named event carries "open a new note" to it.
pub struct Win32Instance;

impl SingleInstance for Win32Instance {
    fn acquire(&self) -> Launch {
        let name = wide(MUTEX_NAME);
        // SAFETY: the mutex handle is deliberately kept for the life of the process.
        let already = unsafe {
            CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            GetLastError() == ERROR_ALREADY_EXISTS
        };
        if !already {
            return Launch::First;
        }
        let event = new_note_event();
        // SAFETY: `event` is a valid event handle (or 0, which SetEvent rejects harmlessly).
        unsafe { SetEvent(event as _) };
        Launch::HandedOff
    }

    fn listen(&self, on_request: Rc<dyn Fn()>) {
        ON_REQUEST.with(|c| *c.borrow_mut() = Some(on_request));
        let event = new_note_event();
        std::thread::spawn(move || loop {
            // SAFETY: the event handle lives as long as the process.
            if unsafe { WaitForSingleObject(event as _, INFINITE) } != WAIT_OBJECT_0 {
                return;
            }
            let _ = slint::invoke_from_event_loop(|| {
                let f = ON_REQUEST.with(|c| c.borrow().clone());
                if let Some(f) = f {
                    f();
                }
            });
        });
    }
}
