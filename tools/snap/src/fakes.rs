//! Test fakes for every `washi::platform` interface.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use washi::platform::*;

/// Reversible fake of DPAPI: XOR plus a checksum byte, so any altered byte fails to unwrap.
pub struct FakeProtector {
    seed: u8,
}

impl FakeProtector {
    pub fn new(seed: u8) -> Self {
        FakeProtector { seed }
    }
}

impl KeyProtector for FakeProtector {
    fn generate_key(&self) -> [u8; 32] {
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = self.seed.wrapping_mul(31).wrapping_add((i as u8).wrapping_mul(7)).wrapping_add(1);
        }
        k
    }

    fn protect(&self, raw: &[u8]) -> Result<Vec<u8>, ProtectError> {
        let sum = raw.iter().fold(0u8, |a, b| a.wrapping_add(*b));
        let mut out: Vec<u8> = raw.iter().map(|b| b ^ 0xA5).collect();
        out.push(sum);
        Ok(out)
    }

    fn unprotect(&self, wrapped: &[u8]) -> Result<Vec<u8>, ProtectError> {
        let (&sum, body) = wrapped.split_last().ok_or_else(|| ProtectError("empty blob".into()))?;
        let raw: Vec<u8> = body.iter().map(|b| b ^ 0xA5).collect();
        if raw.iter().fold(0u8, |a, b| a.wrapping_add(*b)) != sum {
            return Err(ProtectError("blob was altered".into()));
        }
        Ok(raw)
    }
}

/// Fake clock: wall time = fixed epoch + fake monotonic time.
pub struct FakeClock {
    mono: Rc<Cell<Duration>>,
}

impl FakeClock {
    pub fn new(mono: Rc<Cell<Duration>>) -> Self {
        FakeClock { mono }
    }
}

impl Clock for FakeClock {
    fn unix_millis(&self) -> i64 {
        1_700_000_000_000 + self.mono.get().as_millis() as i64
    }
    fn monotonic(&self) -> Duration {
        self.mono.get()
    }
}

/// Window-state signals that tests set directly.
#[derive(Default)]
pub struct FakeSignals {
    pub states: RefCell<HashMap<NoteId, WindowState>>,
    pub background: Cell<bool>,
}

impl WindowSignals for FakeSignals {
    fn window_state(&self, note: NoteId) -> WindowState {
        self.states.borrow().get(&note).copied().unwrap_or_default()
    }
    fn app_in_background(&self) -> bool {
        self.background.get()
    }
}

/// Records efficiency-mode and memory-release calls.
#[derive(Default)]
pub struct FakePower {
    pub efficiency: Cell<bool>,
    pub releases: Cell<u32>,
    /// Every call in order: `release_memory`, `efficiency_on`, `efficiency_off`.
    pub log: RefCell<Vec<String>>,
}

impl ProcessPower for FakePower {
    fn set_efficiency_mode(&self, on: bool) {
        self.efficiency.set(on);
        self.log.borrow_mut().push(if on { "efficiency_on" } else { "efficiency_off" }.into());
    }
    fn release_memory(&self) {
        self.releases.set(self.releases.get() + 1);
        self.log.borrow_mut().push("release_memory".into());
    }
}

/// Records the menus shown and answers with a queued choice: either a raw item id, or a path
/// of labels (e.g. `["Open note", "shopping list"]`) looked up in the menu actually shown.
#[derive(Default)]
pub struct FakeMenu {
    pub shown: RefCell<Vec<Vec<MenuItem>>>,
    pub next_choice: Cell<Option<u32>>,
    pub next_path: RefCell<Option<Vec<String>>>,
}

fn find<'a>(items: &'a [MenuItem], path: &[String]) -> Option<&'a MenuItem> {
    let (first, rest) = path.split_first()?;
    let it = items.iter().find(|i| &i.label == first)?;
    if rest.is_empty() {
        Some(it)
    } else {
        find(&it.children, rest)
    }
}

impl ContextMenu for FakeMenu {
    fn show(&self, items: &[MenuItem], _at: (i32, i32)) -> Option<u32> {
        self.shown.borrow_mut().push(items.to_vec());
        if let Some(path) = self.next_path.borrow_mut().take() {
            let it = find(items, &path).unwrap_or_else(|| panic!("no menu item {path:?} in {items:?}"));
            assert!(it.enabled, "menu item {path:?} is disabled");
            return Some(it.id);
        }
        self.next_choice.take()
    }
}

/// Scripted dialogs.
#[derive(Default)]
pub struct FakeDialogs {
    pub confirm_answer: Cell<bool>,
    pub errors: RefCell<Vec<String>>,
    pub open_path: RefCell<Option<PathBuf>>,
    pub save_path: RefCell<Option<PathBuf>>,
}

impl Dialogs for FakeDialogs {
    fn confirm(&self, _message: &str) -> bool {
        self.confirm_answer.get()
    }
    fn error(&self, message: &str) {
        self.errors.borrow_mut().push(message.to_string());
    }
    fn pick_open_file(&self, _filter: &str) -> Option<PathBuf> {
        self.open_path.borrow().clone()
    }
    fn pick_save_file(&self, _suggested_name: &str, _filter: &str) -> Option<PathBuf> {
        self.save_path.borrow().clone()
    }
}
