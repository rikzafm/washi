//! Window signals, attaching native windows, trimming memory, and suspend.

use super::{App, Inner};
use crate::platform::{NoteId, SignalEvent};
use crate::suspend::SuspendHost;
use std::rc::Rc;
use std::time::Duration;

/// Quiet time after the last window opened before the working set is trimmed once.
const TRIM_DELAY: Duration = Duration::from_millis(1500);

impl App {
    /// The platform saw a note minimize, restore, gain focus or get covered or uncovered.
    pub fn window_state_changed(&self) {
        // a note that lost the focus must not hold unsaved text: it saves now, not after the debounce
        let notes: Vec<_> = self.0.notes.borrow().clone();
        for n in notes {
            if !self.0.env.signals.window_state(n.id()).focused {
                n.flush_text();
            }
        }
        if let Some(open_id) = self.dropdown().map(|d| d.note_id()) {
            let state = self.0.env.signals.window_state(open_id);
            if state.minimized || (!state.focused && self.0.env.signals.app_in_background()) {
                self.dismiss_dropdown();
            }
        }
        self.0.suspend.evaluate();
    }

    /// Windows asked a note to paint (for example because it was uncovered).
    pub fn paint_requested(&self, _id: NoteId) {
        self.0.suspend.poke();
    }

    /// Real app: have the platform tell us about minimize, restore, focus, activation and
    /// paint requests as they happen (no polling, so a suspended app sleeps until a message).
    pub fn watch_window_state(&self) {
        self.0.trim_enabled.set(true);
        self.start_attaching();
        let weak = Rc::downgrade(&self.0);
        self.0.env.signals.set_notifier(Rc::new(move |ev| {
            if let Some(inner) = weak.upgrade() {
                match ev {
                    SignalEvent::StateChanged => App(inner).window_state_changed(),
                    SignalEvent::Paint(id) => App(inner).paint_requested(id),
                    SignalEvent::Geometry(id) => App(inner).geometry_changed(id),
                }
            }
        }));
    }

    /// Slint creates a native window only once its event loop runs, a few milliseconds after the
    /// note opens: poll until every note has its handle, then stop (no timer is left behind).
    /// Without the handle a note gets no window signals and no fix for the swallowed button-up
    /// after a move or resize (win/signals.rs), so this runs for notes opened later too.
    pub(super) fn start_attaching(&self) {
        let weak = Rc::downgrade(&self.0);
        let mut tries = 0;
        self.0.attach_timer.start(slint::TimerMode::Repeated, Duration::from_millis(5), move || {
            let Some(inner) = weak.upgrade() else { return };
            tries += 1;
            if App(inner.clone()).attach_windows() || tries >= 400 {
                inner.attach_timer.stop();
            }
        });
    }

    /// Tell the platform signals about every note window that exists now (idempotent), then
    /// look at the windows once: without the handles nothing can be seen as minimized or covered.
    /// Returns true when every note has a window now.
    fn attach_windows(&self) -> bool {
        let notes: Vec<_> = self.0.notes.borrow().clone();
        let mut attached = 0;
        for n in &notes {
            if let Some(hwnd) = n.hwnd() {
                self.0.env.signals.attach(n.id(), hwnd);
                attached += 1;
            }
        }
        let all = attached == notes.len();
        if all {
            crate::trace::mark("windows-attached");
            self.0.suspend.evaluate();
            self.schedule_trim();
        }
        all
    }

    /// Opening windows leaves freed heap pages and one-time start-up pages in the working set.
    /// Shortly after the last window opened, compact the heap and trim the working set once; what
    /// the app needs while idle is faulted back in (a soft fault), the rest stays out. One-shot:
    /// no timer is left running.
    pub(super) fn schedule_trim(&self) {
        // WASHI_NO_TRIM: lets tools/memprobe.ps1 -NoTrim measure the untrimmed working set
        if !self.0.trim_enabled.get() || std::env::var_os("WASHI_NO_TRIM").is_some() {
            return;
        }
        let weak = Rc::downgrade(&self.0);
        self.0.trim_timer.start(slint::TimerMode::SingleShot, TRIM_DELAY, move || {
            if let Some(inner) = weak.upgrade() {
                if !inner.suspend.is_suspended() {
                    inner.env.power.release_memory();
                }
            }
        });
    }
}

impl SuspendHost for Inner {
    fn open_notes(&self) -> Vec<NoteId> {
        self.notes.borrow().iter().map(|n| n.id()).collect()
    }

    fn save_all(&self) {
        // `try_borrow`: this may run inside a window message while the list is being changed
        let Ok(notes) = self.notes.try_borrow().map(|n| n.clone()) else { return };
        for n in notes {
            n.flush();
            if let Err(e) = self.db.save_geometry(n.id(), n.geometry()) {
                eprintln!("washi: could not save the position of note {}: {e}", n.id());
            }
        }
        self.save_stacking();
    }

    fn stop_timers(&self) {
        let notes: Vec<_> = self.notes.borrow().clone();
        for n in notes {
            n.pause();
        }
    }

    fn release_caches(&self) {
        // decoded pictures first (the big allocations), then SQLite's caches
        let notes: Vec<_> = self.notes.borrow().clone();
        for n in notes {
            n.free_images();
        }
        self.db.shrink_memory();
    }

    fn resume(&self) {
        let notes: Vec<_> = self.notes.borrow().clone();
        for n in notes {
            n.resume(self.env.signals.window_state(n.id()).focused);
        }
    }
}
