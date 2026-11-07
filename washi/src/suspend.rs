//! Suspension (plan 7.1): when nothing of Washi can be seen, give everything back and use no CPU.
//!
//! The controller is a small state machine on the UI thread. It owns no windows: the app
//! implements [`SuspendHost`] and the controller tells it what to save, stop and release.
//!
//! [`should_suspend`] is the one place that decides whether the notes are hidden. The platform
//! signals call `App::window_state_changed` when something changes. Windows never announces
//! coverage, so while Washi is in the background the controller re-checks every
//! [`COVER_CHECK_INTERVAL`]; that timer does not exist in the foreground or while suspended.

use crate::platform::{NoteId, ProcessPower, WindowSignals, WindowState};
use slint::{Timer, TimerMode};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

/// How long the notes must stay hidden before Washi suspends.
pub const GRACE_PERIOD: Duration = Duration::from_secs(10);

/// How often coverage is re-checked while Washi is in the background and not suspended.
pub const COVER_CHECK_INTERVAL: Duration = Duration::from_secs(5);

/// What the controller needs from the app.
pub trait SuspendHost {
    fn open_notes(&self) -> Vec<NoteId>;
    /// Write pending autosaves and window positions to the database.
    fn save_all(&self);
    /// Stop every timer: autosave debounce, caret blink, animations.
    fn stop_timers(&self);
    /// Drop decoded images, glyph caches and SQLite's page cache.
    fn release_caches(&self);
    /// Ask every note for a full repaint and start its timers again.
    fn resume(&self);
}

/// Everything that decides whether to suspend, as one comparable value: any difference during
/// the grace period restarts it.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Picture {
    notes: Vec<(NoteId, WindowState)>,
    background: bool,
}

/// True when none of Washi can be seen: every open note is minimized, or Washi is in the
/// background and every note is minimized or fully covered.
pub fn should_suspend(notes: &[(NoteId, WindowState)], app_in_background: bool) -> bool {
    !notes.is_empty() && notes.iter().all(|(_, s)| s.minimized || (app_in_background && s.covered))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Active,
    /// Hidden; the grace timer is running.
    Grace,
    Suspended,
}

pub struct SuspendController {
    signals: Rc<dyn WindowSignals>,
    power: Rc<dyn ProcessPower>,
    host: RefCell<Option<Weak<dyn SuspendHost>>>,
    phase: Cell<Phase>,
    picture: RefCell<Option<Picture>>,
    timer: Timer,
    /// The periodic coverage check; runs only in the background and not suspended.
    cover_check: Timer,
}

impl SuspendController {
    pub fn new(signals: Rc<dyn WindowSignals>, power: Rc<dyn ProcessPower>) -> Rc<Self> {
        Rc::new(SuspendController {
            signals,
            power,
            host: RefCell::new(None),
            phase: Cell::new(Phase::Active),
            picture: RefCell::new(None),
            timer: Timer::default(),
            cover_check: Timer::default(),
        })
    }

    pub fn attach(&self, host: Weak<dyn SuspendHost>) {
        *self.host.borrow_mut() = Some(host);
    }

    pub fn is_suspended(&self) -> bool {
        self.phase.get() == Phase::Suspended
    }

    fn host(&self) -> Option<Rc<dyn SuspendHost>> {
        self.host.borrow().as_ref().and_then(Weak::upgrade)
    }

    fn look(&self, host: &dyn SuspendHost) -> Picture {
        let notes = host.open_notes().into_iter().map(|id| (id, self.signals.window_state(id))).collect();
        Picture { notes, background: self.signals.app_in_background() }
    }

    /// Something about the windows may have changed (minimize, restore, focus, a note opened
    /// or closed). Cheap: call it as often as needed.
    pub fn evaluate(self: &Rc<Self>) {
        let Some(host) = self.host() else { return };
        let picture = self.look(&*host);
        let hidden = should_suspend(&picture.notes, picture.background);
        self.manage_cover_check(picture.background);
        match self.phase.get() {
            Phase::Suspended => {
                let focused = picture.notes.iter().any(|(_, s)| s.focused);
                if hidden && !focused {
                    return;
                }
                self.wake_up(&*host);
                self.evaluate();
            }
            Phase::Grace => {
                if !hidden {
                    self.timer.stop();
                    self.phase.set(Phase::Active);
                } else if self.picture.borrow().as_ref() != Some(&picture) {
                    self.start_grace(picture);
                }
            }
            Phase::Active => {
                if hidden {
                    self.start_grace(picture);
                }
            }
        }
    }

    /// The coverage check runs while Washi is in the background and not suspended, and never
    /// otherwise.
    fn manage_cover_check(self: &Rc<Self>, background: bool) {
        let wanted = background && self.phase.get() != Phase::Suspended;
        if !wanted {
            self.cover_check.stop();
        } else if !self.cover_check.running() {
            let weak = Rc::downgrade(self);
            self.cover_check.start(TimerMode::Repeated, COVER_CHECK_INTERVAL, move || {
                if let Some(c) = weak.upgrade() {
                    c.evaluate();
                }
            });
        }
    }

    fn start_grace(self: &Rc<Self>, picture: Picture) {
        crate::trace::mark(&format!("grace-start {:?}", picture));
        *self.picture.borrow_mut() = Some(picture);
        self.phase.set(Phase::Grace);
        let weak = Rc::downgrade(self);
        self.timer.start(TimerMode::SingleShot, GRACE_PERIOD, move || {
            if let Some(c) = weak.upgrade() {
                c.grace_elapsed();
            }
        });
    }

    fn grace_elapsed(self: &Rc<Self>) {
        let Some(host) = self.host() else { return };
        let picture = self.look(&*host);
        // Any change would have restarted the timer; check once more regardless.
        if self.phase.get() == Phase::Grace && should_suspend(&picture.notes, picture.background) {
            self.suspend(&*host);
        } else {
            self.phase.set(Phase::Active);
            self.evaluate();
        }
    }

    fn suspend(&self, host: &dyn SuspendHost) {
        crate::trace::mark("suspend-begin");
        host.save_all();
        self.timer.stop();
        self.cover_check.stop();
        host.stop_timers();
        host.release_caches();
        self.power.release_memory();
        self.power.set_efficiency_mode(true);
        self.phase.set(Phase::Suspended);
        crate::trace::mark("suspend-end");
    }

    fn wake_up(&self, host: &dyn SuspendHost) {
        crate::trace::mark("wake-begin");
        self.power.set_efficiency_mode(false);
        self.phase.set(Phase::Active);
        host.resume();
        // the event loop's next turn is where the repaint requested by `resume` is drawn
        Timer::single_shot(Duration::ZERO, || crate::trace::mark("wake-next-turn"));
    }

    /// Hover over a note, or Windows asking a note to paint: show up if suspended.
    pub fn poke(self: &Rc<Self>) {
        if self.phase.get() != Phase::Suspended {
            return;
        }
        let Some(host) = self.host() else { return };
        self.wake_up(&*host);
        self.evaluate();
    }
}
