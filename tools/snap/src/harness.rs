use crate::fakes::*;
use crate::platform;
use slint::platform::{Key, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, PhysicalSize};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use washi::platform::{KeyProtector, NoteId, ScreenRect, WindowState};
use washi::{App, Env, StartError};

/// One app instance's world: a temporary data folder, fakes for Windows, and the running app.
pub struct Harness {
    dir: tempfile::TempDir,
    env: Env,
    pub signals: Rc<FakeSignals>,
    pub power: Rc<FakePower>,
    pub menu: Rc<FakeMenu>,
    pub dialogs: Rc<FakeDialogs>,
    pub startup: Rc<FakeStartup>,
    pub instance: Rc<FakeInstance>,
    pub control: Rc<FakeWindowControl>,
    app: Option<App>,
    last_ids: Vec<NoteId>,
}

impl Default for Harness {
    fn default() -> Self {
        Self::new()
    }
}

impl Harness {
    pub fn new() -> Harness {
        Self::with_key_seed(1)
    }

    /// `seed` picks the (deterministic) test key.
    pub fn with_key_seed(seed: u8) -> Harness {
        platform::install();
        let dir = tempfile::tempdir().expect("temp data folder");
        let signals = Rc::new(FakeSignals::default());
        let power = Rc::new(FakePower::default());
        let menu = Rc::new(FakeMenu::default());
        let dialogs = Rc::new(FakeDialogs::default());
        let startup = Rc::new(FakeStartup::default());
        let instance = Rc::new(FakeInstance::default());
        let control = Rc::new(FakeWindowControl::default());
        let env = Env {
            window_control: control.clone(),
            data_dir: dir.path().to_path_buf(),
            protector: Rc::new(FakeProtector::new(seed)),
            clock: Rc::new(FakeClock::new(platform::time_handle())),
            signals: signals.clone(),
            power: power.clone(),
            menu: menu.clone(),
            dialogs: dialogs.clone(),
            startup: startup.clone(),
            instance: instance.clone(),
        };
        Harness { dir, env, signals, power, menu, dialogs, startup, instance, control, app: None, last_ids: Vec::new() }
    }

    fn app(&self) -> &App {
        self.app.as_ref().expect("app is not running")
    }

    fn id(&self, note: usize) -> NoteId {
        self.app().note_ids()[note]
    }

    // ---- lifecycle ----

    pub fn start(&mut self) -> Result<(), StartError> {
        self.app = None;
        self.instance.process_exited(); // a fresh process: nothing else is running
        let app = App::launch(self.env.clone())?.expect("the first launch cannot hand off");
        // Give every window the focus a real one would have.
        for id in app.note_ids() {
            app.with_note(id, |n| n.ui().window().dispatch_event(WindowEvent::WindowActiveChanged(true)));
        }
        self.app = Some(app);
        Ok(())
    }

    /// Quit (saving), drop the app entirely, and start again on the same data folder.
    pub fn restart(&mut self) -> Result<(), StartError> {
        self.quit();
        self.app = None;
        self.start()
    }

    /// Launch Washi again while it runs. True if that launch handed off and exited.
    pub fn launch_again(&mut self) -> bool {
        App::launch(self.env.clone()).expect("second launch").is_none()
    }

    pub fn quit(&mut self) {
        if let Some(app) = &self.app {
            self.last_ids = app.note_ids();
            app.quit();
        }
    }

    pub fn has_quit(&self) -> bool {
        self.app().has_quit()
    }

    // ---- act ----

    pub fn type_text(&mut self, note: usize, text: &str) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let w = n.ui().window();
            for ch in text.chars() {
                w.dispatch_event(WindowEvent::KeyPressed { text: ch.to_string().into() });
                w.dispatch_event(WindowEvent::KeyReleased { text: ch.to_string().into() });
            }
        });
    }

    /// Put text into the note without typing it (fixtures).
    pub fn set_note_text(&mut self, note: usize, text: &str) {
        let id = self.id(note);
        self.app().with_note(id, |n| n.set_text(text));
    }

    pub fn press_enter(&mut self, note: usize) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let w = n.ui().window();
            w.dispatch_event(WindowEvent::KeyPressed { text: Key::Return.into() });
            w.dispatch_event(WindowEvent::KeyReleased { text: Key::Return.into() });
        });
    }

    /// Press and release one special key (arrows, Home, End, Delete ...).
    pub fn press(&mut self, note: usize, key: Key) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let w = n.ui().window();
            w.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
            w.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
        });
    }

    /// Press one key several times.
    pub fn press_n(&mut self, note: usize, key: Key, times: usize) {
        for _ in 0..times {
            self.press(note, key);
        }
    }

    /// Paste text (line breaks included): it goes on the clipboard, then Ctrl+V.
    pub fn paste(&mut self, note: usize, text: &str) {
        platform::set_clipboard(text);
        self.press_ctrl(note, 'v');
    }

    /// The pieces of the empty-note hint currently shown, joined (empty when not shown).
    pub fn hint_shown(&self, note: usize) -> String {
        use slint::Model;
        self.app()
            .with_note(self.id(note), |n| n.ui().get_runs().iter().filter(|r| r.hint).map(|r| r.text.to_string()).collect::<Vec<_>>().join(""))
            .unwrap()
    }

    pub fn press_backspace(&mut self, note: usize) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let w = n.ui().window();
            w.dispatch_event(WindowEvent::KeyPressed { text: Key::Backspace.into() });
            w.dispatch_event(WindowEvent::KeyReleased { text: Key::Backspace.into() });
        });
    }

    pub fn advance(&mut self, by: Duration) {
        platform::advance(by);
    }

    pub fn close_note(&mut self, note: usize) {
        self.last_ids = self.app().note_ids();
        let id = self.last_ids[note];
        self.app().close_note(id);
        self.settle();
    }

    /// Right-click on the paper. The menu answers with `choose` (a label path) or is dismissed.
    pub fn right_click(&mut self, note: usize, choose: Option<&[&str]>) {
        *self.menu.next_path.borrow_mut() = choose.map(|p| p.iter().map(|s| s.to_string()).collect());
        self.menu.next_choice.set(None);
        self.click_right(note, 5.0, 5.0);
        self.settle();
    }

    /// Right-click at a window position without queuing any menu answer.
    pub fn click_right(&mut self, note: usize, x: f32, y: f32) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let w = n.ui().window();
            let position = slint::LogicalPosition::new(x, y);
            w.dispatch_event(WindowEvent::PointerMoved { position });
            let button = slint::platform::PointerEventButton::Right;
            w.dispatch_event(WindowEvent::PointerPressed { position, button });
            w.dispatch_event(WindowEvent::PointerReleased { position, button });
        });
    }

    /// Press `Ctrl+<letter>` in the note.
    pub fn press_ctrl(&mut self, note: usize, letter: char) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let w = n.ui().window();
            let ctrl: slint::SharedString = Key::Control.into();
            w.dispatch_event(WindowEvent::KeyPressed { text: ctrl.clone() });
            w.dispatch_event(WindowEvent::KeyPressed { text: letter.to_string().into() });
            w.dispatch_event(WindowEvent::KeyReleased { text: letter.to_string().into() });
            w.dispatch_event(WindowEvent::KeyReleased { text: ctrl });
        });
        self.settle();
    }

    /// Let deferred work (releasing closed windows) run.
    pub fn settle(&mut self) {
        platform::advance(Duration::ZERO);
    }

    /// Resize a note's window (logical pixels).
    pub fn resize_note(&mut self, note: usize, w: u32, h: u32) {
        let id = self.id(note);
        self.app().with_note(id, |n| n.ui().window().set_size(slint::LogicalSize::new(w as f32, h as f32)));
    }

    /// Position of a note's window on the desktop (physical = logical pixels at scale 1).
    pub fn window_position(&self, note: usize) -> (i32, i32) {
        let p = platform::windows()[note].window().position();
        (p.x, p.y)
    }

    /// The monitors' work areas from now on (primary first).
    pub fn set_work_areas(&mut self, areas: Vec<ScreenRect>) {
        *self.control.areas.borrow_mut() = areas;
    }

    /// Press at a window position and drag the pointer by `by`. If the press grabbed the note, the
    /// platform is asked to move or resize it, and then does so like Windows would: the window
    /// follows the pointer and never gets smaller than its minimum. Returns what was grabbed.
    pub fn drag(&mut self, note: usize, from: (f32, f32), by: (f32, f32)) -> Option<WindowCommand> {
        let id = self.id(note);
        let seen = self.control.log.borrow().len();
        self.pointer_move(note, from.0, from.1);
        self.press_button(note, from.0, from.1, false);
        let grabbed = self.control.log.borrow().get(seen).copied();
        if let Some(cmd) = grabbed {
            let w = platform::windows().remove(note);
            let scale = w.window().scale_factor();
            let (dx, dy) = ((by.0 * scale).round() as i32, (by.1 * scale).round() as i32);
            let (p, s, min) = (w.window().position(), w.window().size(), w.min_size());
            let (mut left, mut top) = (p.x, p.y);
            let (mut right, mut bottom) = (p.x + s.width as i32, p.y + s.height as i32);
            match cmd {
                WindowCommand::Move(_) => {
                    left += dx;
                    right += dx;
                    top += dy;
                    bottom += dy;
                }
                WindowCommand::Resize(_, edge) => {
                    if edge.moves_left() {
                        left = (left + dx).min(right - min.width as i32);
                    }
                    if edge.moves_right() {
                        right = (right + dx).max(left + min.width as i32);
                    }
                    if edge.moves_top() {
                        top = (top + dy).min(bottom - min.height as i32);
                    }
                    if edge.moves_bottom() {
                        bottom = (bottom + dy).max(top + min.height as i32);
                    }
                }
            }
            w.os_place(
                slint::PhysicalPosition::new(left, top),
                PhysicalSize::new((right - left) as u32, (bottom - top) as u32),
            );
            // what the platform's window procedure reports after a move or resize
            self.app().geometry_changed(id);
        }
        self.release_button(note, from.0, from.1, false);
        grabbed
    }

    /// Size of a note's window (logical pixels).
    pub fn note_size(&self, note: usize) -> (u32, u32) {
        let g = self.app().with_note(self.id(note), |n| n.geometry()).unwrap();
        (g.w, g.h)
    }

    /// The items of the most recently shown menu.
    pub fn last_menu(&self) -> Vec<washi::platform::MenuItem> {
        self.menu.shown.borrow().last().cloned().expect("no menu was shown")
    }

    /// Windows that still exist (open notes, plus any not yet released).
    pub fn window_count(&self) -> usize {
        platform::windows().len()
    }

    /// Id of an open note.
    pub fn note_id(&self, note: usize) -> NoteId {
        self.id(note)
    }

    /// Whether a note is gone from the database.
    pub fn is_deleted(&self, id: NoteId) -> bool {
        self.app().saved_text(id).is_none()
    }

    // ---- window state (suspension) ----

    fn set_state(&mut self, note: usize, change: impl FnOnce(&mut WindowState)) {
        let id = self.id(note);
        change(self.signals.states.borrow_mut().entry(id).or_default());
        self.app().window_state_changed();
    }

    pub fn minimize(&mut self, note: usize) {
        // What Windows does: the window goes inactive and is never painted while minimized.
        let id = self.id(note);
        self.app().with_note(id, |n| n.ui().window().dispatch_event(WindowEvent::WindowActiveChanged(false)));
        self.repaint_pending(note);
        self.set_state(note, |s| {
            s.minimized = true;
            s.focused = false;
        });
    }

    pub fn restore(&mut self, note: usize) {
        self.set_state(note, |s| s.minimized = false);
    }

    /// Other windows now hide the whole note (only counts while Washi is in the background).
    pub fn cover(&mut self, note: usize) {
        self.set_state(note, |s| s.covered = true);
    }

    pub fn uncover(&mut self, note: usize) {
        self.set_state(note, |s| s.covered = false);
    }

    /// Another app takes focus (Alt-Tab away): no note is focused any more.
    pub fn switch_away(&mut self) {
        self.signals.background.set(true);
        for n in 0..self.open_note_count() {
            let id = self.id(n);
            self.signals.states.borrow_mut().entry(id).or_default().focused = false;
            // What Windows does: every window goes inactive, so no caret blinks.
            self.app().with_note(id, |n| n.ui().window().dispatch_event(WindowEvent::WindowActiveChanged(false)));
        }
        self.app().window_state_changed();
    }

    /// Back to Washi (Alt-Tab back): the first note takes focus.
    pub fn switch_back(&mut self) {
        self.signals.background.set(false);
        self.focus(0);
    }

    pub fn focus(&mut self, note: usize) {
        self.set_state(note, |s| s.focused = true);
    }

    /// Move the mouse over a note's margin.
    pub fn hover(&mut self, note: usize) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let position = slint::LogicalPosition::new(5.0, 5.0);
            n.ui().window().dispatch_event(WindowEvent::PointerMoved { position });
        });
        self.settle();
    }

    /// Move the pointer to a window position (logical px).
    pub fn pointer_move(&mut self, note: usize, x: f32, y: f32) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            n.ui().window().dispatch_event(WindowEvent::PointerMoved { position: slint::LogicalPosition::new(x, y) });
        });
    }

    /// The pointer leaves the note's window.
    pub fn pointer_exit(&mut self, note: usize) {
        let id = self.id(note);
        self.app().with_note(id, |n| n.ui().window().dispatch_event(WindowEvent::PointerExited));
    }

    /// Press (without moving first) with the left or right button.
    pub fn press_button(&mut self, note: usize, x: f32, y: f32, right: bool) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let position = slint::LogicalPosition::new(x, y);
            let button = if right { slint::platform::PointerEventButton::Right } else { slint::platform::PointerEventButton::Left };
            n.ui().window().dispatch_event(WindowEvent::PointerPressed { position, button });
        });
    }

    pub fn release_button(&mut self, note: usize, x: f32, y: f32, right: bool) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let position = slint::LogicalPosition::new(x, y);
            let button = if right { slint::platform::PointerEventButton::Right } else { slint::platform::PointerEventButton::Left };
            n.ui().window().dispatch_event(WindowEvent::PointerReleased { position, button });
        });
    }

    /// Turn the mouse wheel at a window position (logical px); positive `dy` is up.
    pub fn wheel(&mut self, note: usize, x: f32, y: f32, dy: f32) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            n.ui().window().dispatch_event(WindowEvent::PointerScrolled { position: slint::LogicalPosition::new(x, y), delta_x: 0.0, delta_y: dy });
        });
    }

    /// How far the text is scrolled up, in logical px (0 at the top).
    pub fn scroll_offset(&self, note: usize) -> f32 {
        self.app().with_note(self.id(note), |n| n.ui().get_scroll_y()).unwrap()
    }

    /// Height of the text area in logical px.
    pub fn text_view_height(&self, note: usize) -> f32 {
        self.app().with_note(self.id(note), |n| n.ui().get_text_view_height()).unwrap()
    }

    /// Opacity of the scrollbar (0 hidden, 1 shown).
    pub fn scrollbar_opacity(&self, note: usize) -> f32 {
        self.app().with_note(self.id(note), |n| n.ui().get_scrollbar_opacity()).unwrap()
    }

    /// How much of the resting tilt the text still carries: 1 at rest, 0 while editing.
    pub fn text_tilt_scale(&self, note: usize) -> f32 {
        self.app().with_note(self.id(note), |n| n.ui().get_text_tilt_scale()).unwrap()
    }

    /// The resting words: (text, level x, level y, dx, dy, width) in logical px of the paper's frame.
    pub fn text_runs(&self, note: usize) -> Vec<(String, f32, f32, f32, f32, f32)> {
        use slint::Model;
        self.app()
            .with_note(self.id(note), |n| n.ui().get_runs().iter().map(|r| (r.text.to_string(), r.x, r.y, r.dx, r.dy, r.width)).collect())
            .unwrap()
    }

    /// The window loses (or regains) the system focus: no field has the keyboard focus then.
    pub fn set_window_active(&mut self, note: usize, active: bool) {
        self.app().with_note(self.id(note), |n| n.ui().window().dispatch_event(WindowEvent::WindowActiveChanged(active)));
    }

    /// Opacity of the toolbar (0 hidden, 1 shown).
    pub fn toolbar_opacity(&self, note: usize) -> f32 {
        self.app().with_note(self.id(note), |n| n.ui().get_toolbar_opacity()).unwrap()
    }

    /// Whether the toolbar takes part in drawing and hit testing at all.
    pub fn toolbar_visible(&self, note: usize) -> bool {
        self.app().with_note(self.id(note), |n| n.ui().get_toolbar_visible()).unwrap()
    }

    /// Number of context menus shown so far.
    pub fn menus_shown(&self) -> usize {
        self.menu.shown.borrow().len()
    }

    /// Give a note's window keyboard focus without moving the pointer.
    pub fn activate_window(&mut self, note: usize) {
        let id = self.id(note);
        self.app().with_note(id, |n| n.ui().window().dispatch_event(WindowEvent::WindowActiveChanged(true)));
    }

    /// Windows asks a note to paint (it was uncovered).
    pub fn paint_request(&mut self, note: usize) {
        let id = self.id(note);
        self.app().paint_requested(id);
    }

    pub fn is_suspended(&self) -> bool {
        self.app().is_suspended()
    }

    /// Time until the next Slint timer is due; `None` when none is scheduled.
    pub fn next_timer_due(&self) -> Option<Duration> {
        slint::platform::duration_until_next_timer_update()
    }

    /// Draw a note if it asked for a repaint; says whether it did (so a second call is `false`).
    pub fn repaint_pending(&self, note: usize) -> bool {
        let w = platform::windows().remove(note);
        let s = w.window().size();
        let (pw, ph) = (s.width, s.height);
        let mut buf = vec![slint::Rgb8Pixel::default(); (pw * ph) as usize];
        w.draw_if_needed(|r| {
            r.render(&mut buf, pw as usize);
        })
    }

    pub fn draw_pending(&self, note: usize) -> bool {
        self.repaint_pending(note)
    }

    /// Position and size last written to the database for a note.
    pub fn saved_geometry(&self, id: NoteId) -> Option<washi::storage::Geometry> {
        self.app().saved_geometry(id)
    }

    // ---- check ----

    /// The title a note's window (and taskbar button) shows.
    pub fn window_title(&self, note: usize) -> String {
        self.app().with_note(self.id(note), |n| n.title()).unwrap()
    }

    pub fn always_on_top(&self, note: usize) -> bool {
        self.app().with_note(self.id(note), |n| n.always_on_top()).unwrap()
    }

    /// Whether "Start with Windows" is registered.
    pub fn startup_registered(&self) -> bool {
        self.startup.enabled.get()
    }

    pub fn open_note_count(&self) -> usize {
        self.app().note_ids().len()
    }

    /// Text currently in the note's editor.
    pub fn note_text(&self, note: usize) -> String {
        self.app().with_note(self.id(note), |n| n.text()).unwrap()
    }

    /// Text last written to the database for the note.
    pub fn saved_text(&self, note: usize) -> String {
        self.app().saved_text(self.id(note)).unwrap()
    }

    /// For use after the last note was closed.
    pub fn saved_text_of_only_note(&self) -> String {
        assert_eq!(self.last_ids.len(), 1);
        self.app().saved_text(self.last_ids[0]).unwrap()
    }

    pub fn schema_version(&self) -> u32 {
        self.app().schema_version()
    }

    pub fn pragma(&self, name: &str) -> String {
        self.app().pragma(name)
    }

    /// Render a note's window to RGB bytes (width, height, pixels).
    pub fn render(&self, note: usize) -> (u32, u32, Vec<slint::Rgb8Pixel>) {
        let w = platform::windows().remove(note);
        let s = w.window().size();
        let (pw, ph) = (s.width, s.height);
        let mut buf = vec![slint::Rgb8Pixel::default(); (pw * ph) as usize];
        w.draw_if_needed(|r| {
            r.render(&mut buf, pw as usize);
        });
        (pw, ph, buf)
    }

    /// Raw bytes of the database file and its `-wal` (empty if there is none).
    pub fn raw_db_bytes(&self) -> (Vec<u8>, Vec<u8>) {
        let db = std::fs::read(self.path("notes.db")).unwrap_or_default();
        let wal = std::fs::read(self.path("notes.db-wal")).unwrap_or_default();
        (db, wal)
    }

    // ---- sabotage ----

    pub fn corrupt_key_file(&self) {
        let p = self.path("key.bin");
        let mut b = std::fs::read(&p).unwrap();
        b[3] ^= 1;
        std::fs::write(p, b).unwrap();
    }

    pub fn replace_key_with_another_valid_key(&self) {
        let other = FakeProtector::new(99);
        let wrapped = other.protect(&other.generate_key()).unwrap();
        std::fs::write(self.path("key.bin"), wrapped).unwrap();
    }

    pub fn damage_database_header(&mut self) {
        // Close the app first: closing the database checkpoints the WAL back over the header.
        self.app = None;
        let p = self.path("notes.db");
        let mut b = std::fs::read(&p).unwrap();
        for byte in b.iter_mut().take(32) {
            *byte ^= 0xFF;
        }
        std::fs::write(p, b).unwrap();
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
}
