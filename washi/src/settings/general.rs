//! Settings › General: Start with Windows, the new-note Always on top default, the folders and
//! Resource use. Owns the once-a-second stats timer, which runs only while the tab is showing.

use super::stats::{db_size, elide_middle, fmt_bytes, fmt_cpu, CpuSampler};
use crate::app::Env;
use crate::note_window::SettingsWindow;
use crate::storage::Db;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::time::Duration;

/// How often Resource use refreshes while General is showing.
const REFRESH: Duration = Duration::from_secs(1);
/// Longest folder path shown in a row's hint before it is shortened in the middle.
const PATH_CHARS: usize = 38;

/// What General needs from the app.
#[derive(Clone)]
pub struct GeneralDeps {
    pub env: Env,
    pub db: Rc<Db>,
    /// Notes currently open.
    pub open_note_count: Rc<dyn Fn() -> usize>,
    /// Save everything and exit, so a started uninstaller can remove the exe.
    pub quit_for_uninstall: Rc<dyn Fn()>,
}

pub struct General {
    ui: slint::Weak<SettingsWindow>,
    deps: GeneralDeps,
    now: Rc<dyn Fn() -> Duration>,
    timer: slint::Timer,
    sampler: RefCell<CpuSampler>,
    me: Weak<General>,
}

impl General {
    pub fn new(ui: &SettingsWindow, deps: GeneralDeps, now: Rc<dyn Fn() -> Duration>) -> Rc<General> {
        let general = Rc::new_cyclic(|me| General {
            ui: ui.as_weak(),
            deps,
            now,
            timer: slint::Timer::default(),
            sampler: RefCell::new(CpuSampler::new()),
            me: me.clone(),
        });
        general.wire(ui);
        general.refresh_toggles();
        general
    }

    fn wire(&self, ui: &SettingsWindow) {
        let install = self.deps.env.shell.install_dir();
        let data = self.deps.env.data_dir.clone();
        ui.set_install_dir(elide_middle(&install.to_string_lossy(), PATH_CHARS).into());
        ui.set_data_dir(elide_middle(&data.to_string_lossy(), PATH_CHARS).into());
        let me = self.me.clone();
        ui.on_toggle_startup(move || {
            if let Some(g) = me.upgrade() {
                g.toggle_startup();
            }
        });
        let me = self.me.clone();
        ui.on_toggle_on_top(move || {
            if let Some(g) = me.upgrade() {
                g.toggle_on_top();
            }
        });
        let me = self.me.clone();
        ui.on_open_install_dir(move || {
            if let Some(g) = me.upgrade() {
                let dir = g.deps.env.shell.install_dir();
                g.deps.env.shell.open_folder(&dir);
            }
        });
        let me = self.me.clone();
        ui.on_open_data_dir(move || {
            if let Some(g) = me.upgrade() {
                g.deps.env.shell.open_folder(&g.deps.env.data_dir);
            }
        });
        let me = self.me.clone();
        ui.on_uninstall_clicked(move || {
            if let Some(g) = me.upgrade() {
                // a disabled button asks nothing
                if g.deps.env.shell.uninstaller().is_some() {
                    g.set_confirming(true);
                }
            }
        });
        let me = self.me.clone();
        ui.on_uninstall_cancel(move || {
            if let Some(g) = me.upgrade() {
                g.set_confirming(false);
            }
        });
        let me = self.me.clone();
        ui.on_uninstall_confirm(move || {
            if let Some(g) = me.upgrade() {
                g.uninstall_confirmed();
            }
        });
    }

    fn set_confirming(&self, on: bool) {
        if let Some(ui) = self.ui.upgrade() {
            ui.set_uninstall_confirming(on);
        }
    }

    /// Run the uninstaller beside the exe, then exit. Washi never deletes files itself.
    fn uninstall_confirmed(&self) {
        self.set_confirming(false);
        let shell = &self.deps.env.shell;
        let Some(exe) = shell.uninstaller() else {
            self.refresh_uninstall(); // it has gone since the row was drawn
            return;
        };
        match shell.launch(&exe) {
            Ok(()) => (self.deps.quit_for_uninstall)(),
            Err(e) => self.deps.env.dialogs.error(&format!("Washi could not start the uninstaller: {e}")),
        }
    }

    fn refresh_uninstall(&self) {
        if let Some(ui) = self.ui.upgrade() {
            ui.set_uninstall_available(self.deps.env.shell.uninstaller().is_some());
        }
    }

    /// Show what is really stored: the registry value and the database setting.
    fn refresh_toggles(&self) {
        if let Some(ui) = self.ui.upgrade() {
            ui.set_start_with_windows(self.deps.env.startup.is_enabled());
            ui.set_new_notes_on_top(self.deps.db.new_notes_on_top());
        }
        self.refresh_uninstall();
    }

    fn toggle_startup(&self) {
        let s = &self.deps.env.startup;
        s.set_enabled(!s.is_enabled());
        self.refresh_toggles(); // read back: a failed write shows the truth
    }

    fn toggle_on_top(&self) {
        let _ = self.deps.db.set_new_notes_on_top(!self.deps.db.new_notes_on_top());
        self.refresh_toggles();
    }

    /// Whether the stats timer is running.
    pub fn is_refreshing(&self) -> bool {
        self.timer.running()
    }

    /// General is (or is no longer) on screen: start or stop the stats timer.
    pub fn set_showing(&self, showing: bool) {
        if !showing {
            self.set_confirming(false); // leaving General cancels the confirm step
        }
        if showing == self.timer.running() {
            return;
        }
        if showing {
            self.refresh_toggles(); // the right-click menu may have changed Start with Windows
            self.refresh_stats(); // at once, not after a second
            let me = self.me.clone();
            self.timer.start(slint::TimerMode::Repeated, REFRESH, move || {
                if let Some(g) = me.upgrade() {
                    g.refresh_stats();
                }
            });
        } else {
            self.timer.stop();
            *self.sampler.borrow_mut() = CpuSampler::new();
        }
    }

    fn refresh_stats(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let s = &self.deps.env.stats;
        let cpu = self.sampler.borrow_mut().sample((self.now)(), s.cpu_time(), s.processors());
        ui.set_stat_memory(fmt_bytes(s.private_working_set()).into());
        ui.set_stat_cpu(fmt_cpu(cpu).into());
        ui.set_stat_db(fmt_bytes(db_size(&self.deps.env.data_dir)).into());
        ui.set_stat_notes((self.deps.open_note_count)().to_string().into());
    }
}
