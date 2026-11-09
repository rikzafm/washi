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
    pub clipboard: Rc<FakeClipboard>,
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
        Self::with_protector(Rc::new(FakeProtector::new(seed)))
    }

    /// A harness whose key protector is `protector` (for example one with a broken random generator).
    pub fn with_protector(protector: Rc<dyn KeyProtector>) -> Harness {
        platform::install();
        let dir = tempfile::tempdir().expect("temp data folder");
        let signals = Rc::new(FakeSignals::default());
        let power = Rc::new(FakePower::default());
        let menu = Rc::new(FakeMenu::default());
        let dialogs = Rc::new(FakeDialogs::default());
        let clipboard = Rc::new(FakeClipboard::default());
        let startup = Rc::new(FakeStartup::default());
        let instance = Rc::new(FakeInstance::default());
        let control = Rc::new(FakeWindowControl::default());
        let env = Env {
            window_control: control.clone(),
            data_dir: dir.path().to_path_buf(),
            protector,
            clock: Rc::new(FakeClock::new(platform::time_handle())),
            signals: signals.clone(),
            power: power.clone(),
            menu: menu.clone(),
            dialogs: dialogs.clone(),
            clipboard: clipboard.clone(),
            startup: startup.clone(),
            instance: instance.clone(),
        };
        Harness { dir, env, signals, power, menu, dialogs, clipboard, startup, instance, control, app: None, last_ids: Vec::new() }
    }

    pub(crate) fn app(&self) -> &App {
        self.app.as_ref().expect("app is not running")
    }

    pub(crate) fn id(&self, note: usize) -> NoteId {
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

    /// Windows is ending the session: everything must be saved before this returns.
    pub fn end_session(&mut self) {
        self.signals.end_session();
    }

    /// The notes' windows, bottom to top, as the desktop will report them from now on.
    pub fn set_stacking(&mut self, order: Vec<NoteId>) {
        *self.control.stacking.borrow_mut() = order;
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

    /// Press one special key with Shift held, several times (extends the selection).
    pub fn press_shift(&mut self, note: usize, key: Key, times: usize) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let w = n.ui().window();
            let shift: slint::SharedString = Key::Shift.into();
            w.dispatch_event(WindowEvent::KeyPressed { text: shift.clone() });
            for _ in 0..times {
                w.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
                w.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
            }
            w.dispatch_event(WindowEvent::KeyReleased { text: shift });
        });
        self.settle();
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

    /// The text on the clipboard (what Ctrl+C or Ctrl+X put there).
    pub fn clipboard_text(&self) -> Option<String> {
        self.clipboard.text.borrow().clone().or_else(platform::clipboard)
    }

    /// The image on the clipboard, if any.
    pub fn clipboard_image(&self) -> Option<washi::images::Rgba> {
        self.clipboard.image.borrow().clone()
    }

    /// The rich HTML on the clipboard, if any.
    pub fn clipboard_html(&self) -> Option<String> {
        self.clipboard.html.borrow().clone()
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

    /// Like `advance`, but in steps of at most one frame (16 ms), so a repeating frame timer (the
    /// Settings transition) ticks as often as it would in real time.
    pub fn advance_frames(&mut self, by: Duration) {
        let frame = Duration::from_millis(16);
        let mut left = by;
        while left > frame {
            platform::advance(frame);
            left -= frame;
        }
        platform::advance(left);
    }

    pub fn close_note(&mut self, note: usize) {
        self.last_ids = self.app().note_ids();
        let id = self.last_ids[note];
        self.app().close_note(id);
        self.settle();
    }

    /// Right-click on the paper. The menu answers with `choose` (a label path) or is dismissed.
    pub fn right_click(&mut self, note: usize, choose: Option<&[&str]>) {
        self.right_click_at(note, 5.0, 5.0, choose);
    }

    /// Right-click at `(x, y)`. The menu answers with `choose` (a label path) or is dismissed.
    pub fn right_click_at(&mut self, note: usize, x: f32, y: f32, choose: Option<&[&str]>) {
        *self.menu.next_path.borrow_mut() = choose.map(|p| p.iter().map(|s| s.to_string()).collect());
        self.menu.next_choice.set(None);
        self.click_right(note, x, y);
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

    /// Press Ctrl plus a key that reports the control code Windows sends for Ctrl+letter (U+0001
    /// for A, U+0002 for B ...) instead of the letter, as keyboard layouts without Latin letters do.
    pub fn press_ctrl_code(&mut self, note: usize, letter: char) {
        let code = ((letter.to_ascii_lowercase() as u8) - b'a' + 1) as char;
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let w = n.ui().window();
            let ctrl: slint::SharedString = Key::Control.into();
            w.dispatch_event(WindowEvent::KeyPressed { text: ctrl.clone() });
            w.dispatch_event(WindowEvent::KeyPressed { text: code.to_string().into() });
            w.dispatch_event(WindowEvent::KeyReleased { text: code.to_string().into() });
            w.dispatch_event(WindowEvent::KeyReleased { text: ctrl });
        });
        self.settle();
    }

    /// Ask a block to take the caret (at its start).
    pub fn place_caret(&mut self, note: usize, block: usize) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let ui = n.ui();
            ui.set_focus_block(block as i32);
            ui.set_focus_anchor(-1);
            ui.set_focus_offset(0);
            ui.set_focus_serial(ui.get_focus_serial() + 1);
        });
    }

    /// The note's selection as `((block, offset), (block, offset))`: where it started and where it ends.
    pub fn selection(&self, note: usize) -> ((usize, usize), (usize, usize)) {
        let r = self.app().with_note(self.id(note), |n| n.selection()).unwrap();
        ((r.anchor.block, r.anchor.offset), (r.focus.block, r.focus.offset))
    }

    /// Whether the fields draw their own selection (false while the note draws one across blocks).
    pub fn wash(&self, note: usize) -> bool {
        self.app().with_note(self.id(note), |n| n.ui().get_wash()).unwrap()
    }

    /// The text field with the keyboard focus (-1 none).
    pub fn text_focused_block(&self, note: usize) -> i32 {
        self.app().with_note(self.id(note), |n| n.ui().get_text_focused_block()).unwrap()
    }

    /// Press the left button with Shift held (the release is the caller's).
    pub fn press_shift_button(&mut self, note: usize, x: f32, y: f32) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let w = n.ui().window();
            let shift: slint::SharedString = Key::Shift.into();
            w.dispatch_event(WindowEvent::KeyPressed { text: shift.clone() });
            w.dispatch_event(WindowEvent::PointerPressed {
                position: slint::LogicalPosition::new(x, y),
                button: slint::platform::PointerEventButton::Left,
            });
            w.dispatch_event(WindowEvent::KeyReleased { text: shift });
        });
    }

    /// Press `Ctrl+Shift+<letter>` in the note.
    pub fn press_ctrl_shift(&mut self, note: usize, letter: char) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let w = n.ui().window();
            let (ctrl, shift): (slint::SharedString, slint::SharedString) = (Key::Control.into(), Key::Shift.into());
            let letter = letter.to_ascii_uppercase().to_string();
            w.dispatch_event(WindowEvent::KeyPressed { text: ctrl.clone() });
            w.dispatch_event(WindowEvent::KeyPressed { text: shift.clone() });
            w.dispatch_event(WindowEvent::KeyPressed { text: letter.clone().into() });
            w.dispatch_event(WindowEvent::KeyReleased { text: letter.into() });
            w.dispatch_event(WindowEvent::KeyReleased { text: shift });
            w.dispatch_event(WindowEvent::KeyReleased { text: ctrl });
        });
        self.settle();
    }

    /// Press `Ctrl+Shift+<special key>` (Home, End ...) in the note.
    pub fn press_ctrl_shift_key(&mut self, note: usize, key: Key) {
        let id = self.id(note);
        self.app().with_note(id, |n| {
            let w = n.ui().window();
            let (ctrl, shift): (slint::SharedString, slint::SharedString) = (Key::Control.into(), Key::Shift.into());
            w.dispatch_event(WindowEvent::KeyPressed { text: ctrl.clone() });
            w.dispatch_event(WindowEvent::KeyPressed { text: shift.clone() });
            w.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
            w.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
            w.dispatch_event(WindowEvent::KeyReleased { text: shift });
            w.dispatch_event(WindowEvent::KeyReleased { text: ctrl });
        });
        self.settle();
    }

    /// Click (press and release, left button) at a window position, moving there first.
    pub fn click(&mut self, note: usize, x: f32, y: f32) {
        self.pointer_move(note, x, y);
        self.press_button(note, x, y, false);
        self.release_button(note, x, y, false);
        self.settle();
    }

    /// The center of toolbar button `k` (0 B, 1 I, 2 U, 3 ab, 4 A, 5 list, 6 image) in window
    /// coordinates: the button row sits in the paper's frame, turned about the paper's center.
    pub fn toolbar_button(&self, note: usize, k: usize) -> (f32, f32) {
        let du = 23.2 + 16.0 + k as f32 * 42.667;
        self.paper_point(note, du, -40.0 + 16.0 + self.paper_size(note).1)
    }

    /// A point inside the color popover's body, in window coordinates (the caption row, clear of
    /// the swatches and of Clear): 22 px from the paper's left edge plus and 30 px above the band.
    pub fn picker_point(&self, note: usize) -> (f32, f32) {
        self.paper_point(note, 22.0 + 110.0, self.paper_size(note).1 - 38.0 - 10.0 - 20.0)
    }

    /// The center of an ink dot (`row` 0) or marker chip (`row` 1) `i` of the open popover, in
    /// window coordinates.
    pub fn swatch_point(&self, note: usize, row: usize, i: usize) -> (f32, f32) {
        let top = self.paper_size(note).1 - 38.0 - 10.0 - 116.0;
        let v = top + if row == 0 { 14.0 } else { 47.0 } + 10.5;
        self.paper_point(note, 22.0 + 67.0 + 10.5 + i as f32 * 26.0, v)
    }

    /// The center of the popover's Clear link, in window coordinates.
    pub fn clear_point(&self, note: usize) -> (f32, f32) {
        let top = self.paper_size(note).1 - 38.0 - 10.0 - 116.0;
        self.paper_point(note, 22.0 + 330.0 - 14.0 - 20.0, top + 80.0 + 13.0)
    }

    /// The ink and marker index the toolbar and popover show (-1: mixed).
    pub fn shown_colors(&self, note: usize) -> (i32, i32) {
        self.app().with_note(self.id(note), |n| (n.ui().get_fmt_ink(), n.ui().get_fmt_marker())).unwrap()
    }

    /// The caption the open popover shows.
    pub fn picker_caption(&self, note: usize) -> String {
        self.app().with_note(self.id(note), |n| n.ui().get_picker_caption().to_string()).unwrap()
    }

    fn paper_size(&self, note: usize) -> (f32, f32) {
        let (w, h) = self.note_size(note);
        let margin = self.app().with_note(self.id(note), |n| n.ui().get_window_margin()).unwrap();
        (w as f32 - 2.0 * margin, h as f32 - 2.0 * margin)
    }

    /// A point `(u, v)` px from the paper's top-left corner (untilted frame), in window coordinates.
    fn paper_point(&self, note: usize, u: f32, v: f32) -> (f32, f32) {
        let (w, h) = self.note_size(note);
        let (pw, ph) = self.paper_size(note);
        let (du, dv) = (u - pw / 2.0, v - ph / 2.0);
        let (s, c) = 1.5f32.to_radians().sin_cos();
        (w as f32 / 2.0 + du * c - dv * s, h as f32 / 2.0 + du * s + dv * c)
    }

    /// The swatch the keyboard has reached in the open popover as (row, index), `None` before any
    /// arrow key (row 0 ink, row 1 marker).
    pub fn picker_focus(&self, note: usize) -> Option<(usize, usize)> {
        let f = self.app().with_note(self.id(note), |n| n.ui().get_picker_focus()).unwrap();
        (f >= 0).then(|| ((f / 10) as usize, (f % 10) as usize))
    }

    /// Whether the color popover is open.
    pub fn picker_open(&self, note: usize) -> bool {
        self.app().with_note(self.id(note), |n| n.ui().get_picker_open()).unwrap()
    }

    /// Which toolbar buttons show as active: B, I, U, ab, list.
    pub fn toolbar_active(&self, note: usize) -> [bool; 5] {
        self.app()
            .with_note(self.id(note), |n| {
                let ui = n.ui();
                [ui.get_fmt_bold(), ui.get_fmt_italic(), ui.get_fmt_underline(), ui.get_fmt_strike(), ui.get_fmt_list()]
            })
            .unwrap()
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
                WindowCommand::BringToFront(_) => return None,
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

    /// Put a note's window somewhere (logical px), as if the user had moved it.
    pub fn place_note(&mut self, note: usize, g: washi::storage::Geometry) {
        self.app().with_note(self.id(note), |n| n.place(g));
    }

    /// Geometry of a note's window.
    pub fn note_geometry(&self, note: usize) -> washi::storage::Geometry {
        self.app().with_note(self.id(note), |n| n.geometry()).unwrap()
    }

    /// Whether the note window's dropdown-open property is set.
    pub fn is_note_dropdown_open(&self, note: usize) -> bool {
        self.app().with_note(self.id(note), |n| n.is_dropdown_open()).unwrap()
    }

    /// Report geometry changed (e.g. after moving or resizing the note).
    pub fn notify_geometry_changed(&mut self, note: usize) {
        let id = self.note_id(note);
        self.app().geometry_changed(id);
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

    /// Center of the scrollbar thumb in window coordinates (logical px).
    pub fn scrollbar_thumb(&self, note: usize) -> (f32, f32) {
        self.app()
            .with_note(self.id(note), |n| (n.ui().get_scrollbar_thumb_x(), n.ui().get_scrollbar_thumb_y()))
            .unwrap()
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

    /// The ink of each resting word, as `0xRRGGBB`, in the order of [`Harness::text_runs`].
    pub fn run_inks(&self, note: usize) -> Vec<u32> {
        use slint::Model;
        let rgb = |c: slint::Color| (c.red() as u32) << 16 | (c.green() as u32) << 8 | c.blue() as u32;
        self.app().with_note(self.id(note), |n| n.ui().get_runs().iter().map(|r| rgb(r.ink)).collect()).unwrap()
    }

    /// The markers behind the text: `(x, y, width, height, 0xRRGGBB)`, level, in the paper's frame.
    pub fn markers(&self, note: usize) -> Vec<(f32, f32, f32, f32, u32)> {
        use slint::Model;
        let rgb = |c: slint::Color| (c.red() as u32) << 16 | (c.green() as u32) << 8 | c.blue() as u32;
        self.app()
            .with_note(self.id(note), |n| n.ui().get_markers().iter().map(|m| (m.x, m.y, m.width, m.height, rgb(m.color))).collect())
            .unwrap()
    }

    /// The boxes of the selection wash `(x, y, width, height)`, level, in the paper's frame.
    pub fn sel_rects(&self, note: usize) -> Vec<(f32, f32, f32, f32)> {
        use slint::Model;
        self.app()
            .with_note(self.id(note), |n| n.ui().get_sel_rects().iter().map(|r| (r.x, r.y, r.width, r.height)).collect())
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

    pub fn exit_candidates(&self) -> Vec<washi::ClosedCandidate> {
        self.app().exit_candidates()
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
        let wrapped = other.protect(&*other.generate_key().unwrap()).unwrap();
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

    pub fn is_dropdown_open(&self) -> bool {
        self.app.as_ref().map(|a| a.is_dropdown_open()).unwrap_or(false)
    }

    pub fn dropdown_note_id(&self) -> Option<NoteId> {
        self.app.as_ref().and_then(|a| a.dropdown_note_id())
    }

    pub fn dropdown_pinned(&self) -> Option<bool> {
        self.app.as_ref().and_then(|a| a.dropdown_pinned())
    }

    pub fn dropdown_position(&self) -> Option<(f32, f32)> {
        self.app.as_ref().and_then(|a| a.dropdown_position())
    }

    pub fn dropdown_size(&self) -> Option<(f32, f32)> {
        self.app.as_ref().and_then(|a| a.dropdown_size())
    }

    pub fn menu_button_rect(&self, note: usize) -> (f32, f32, f32, f32) {
        let id = self.note_id(note);
        self.app().menu_button_rect(id).unwrap()
    }

    pub fn menu_button_opacity(&self, note: usize) -> f32 {
        let id = self.note_id(note);
        self.app().menu_button_opacity(id).unwrap()
    }

    pub fn menu_button_shown(&self, note: usize) -> bool {
        let id = self.note_id(note);
        self.app().menu_button_shown(id).unwrap()
    }

    pub fn click_menu_button(&mut self, note: usize) {
        let (x, y, w, h) = self.menu_button_rect(note);
        self.click(note, x + w / 2.0, y + h / 2.0);
    }

    pub fn toggle_dropdown(&mut self, note: usize) {
        let id = self.note_id(note);
        if let Some(app) = &self.app {
            app.toggle_dropdown(id);
        }
    }

    pub fn invoke_dropdown_toggle_pin(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_dropdown_toggle_pin();
        }
    }

    pub fn invoke_dropdown_delete_note(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_dropdown_delete_note();
        }
    }

    pub fn invoke_dropdown_close_note(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_dropdown_close_note();
        }
    }

    pub fn invoke_dropdown_new_note(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_dropdown_new_note();
        }
    }

    pub fn invoke_dropdown_all_notes(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_dropdown_all_notes();
        }
        self.settle();
    }

    /// Press Settings in the open dropdown menu.
    pub fn invoke_dropdown_settings(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_dropdown_settings();
        }
        self.settle();
    }

    /// Whether the open dropdown menu shows its Settings button (`None` without a dropdown).
    pub fn dropdown_settings_visible(&self) -> Option<bool> {
        self.app.as_ref().and_then(|a| a.dropdown_settings_visible())
    }

    pub fn is_settings_open(&self) -> bool {
        self.app.as_ref().is_some_and(|a| a.is_settings_open())
    }

    /// The Settings window's place and size, logical px (physical at scale 1).
    pub fn settings_window_geometry(&self) -> Option<(i32, i32, u32, u32)> {
        let s = self.app.as_ref()?.settings()?;
        let w = s.ui().window();
        let (p, sz) = (w.position(), w.size());
        Some((p.x, p.y, sz.width, sz.height))
    }

    /// Whether the Settings page's frame timer is running (it stops at rest).
    pub fn settings_ticking(&self) -> bool {
        self.app.as_ref().and_then(|a| a.settings()).is_some_and(|s| s.is_ticking())
    }

    /// Whether the Settings window is on screen.
    pub fn settings_shown(&self) -> bool {
        self.app.as_ref().and_then(|a| a.settings()).is_some_and(|s| s.is_shown())
    }

    /// How visible a note's own content is (1 at rest; fades while it turns into Settings).
    pub fn note_content_opacity(&self, note: usize) -> f32 {
        self.app().with_note(self.id(note), |n| n.ui().get_content_opacity()).unwrap()
    }

    /// Drag the Settings window to `(x, y)` (as the user moving the page would).
    pub fn move_settings_window(&mut self, x: i32, y: i32) {
        if let Some(s) = self.app.as_ref().and_then(|a| a.settings()) {
            s.ui().window().set_position(slint::LogicalPosition::new(x as f32, y as f32));
        }
    }

    /// Press the Sidebar's X.
    pub fn settings_close(&mut self) {
        if let Some(s) = self.app.as_ref().and_then(|a| a.settings()) {
            s.ui().invoke_close_clicked();
        }
        self.settle();
    }

    pub fn settings_press_escape(&mut self) {
        if let Some(app) = &self.app {
            app.dispatch_settings_event(WindowEvent::KeyPressed { text: Key::Escape.into() });
            app.dispatch_settings_event(WindowEvent::KeyReleased { text: Key::Escape.into() });
        }
        self.settle();
    }

    /// Whether a note's desktop window is shown.
    pub fn note_visible(&self, note: usize) -> bool {
        self.app().with_note(self.id(note), |n| n.ui().window().is_visible()).unwrap()
    }

    pub fn is_all_notes_open(&self) -> bool {
        self.app.as_ref().is_some_and(|a| a.is_all_notes_open())
    }

    pub fn all_notes_selected_id(&self) -> Option<NoteId> {
        self.app.as_ref().and_then(|a| a.all_notes_selected_id())
    }

    pub fn all_notes_open_count(&self) -> Option<usize> {
        self.app.as_ref().and_then(|a| a.all_notes_open_count())
    }

    pub fn all_notes_closed_count(&self) -> Option<usize> {
        self.app.as_ref().and_then(|a| a.all_notes_closed_count())
    }

    pub fn all_notes_open_items(&self) -> Vec<washi::note_window::AllNoteItem> {
        self.app.as_ref().map(|a| a.all_notes_open_items()).unwrap_or_default()
    }

    pub fn all_notes_closed_items(&self) -> Vec<washi::note_window::AllNoteItem> {
        self.app.as_ref().map(|a| a.all_notes_closed_items()).unwrap_or_default()
    }

    pub fn all_notes_status_text(&self) -> Option<String> {
        self.app.as_ref().and_then(|a| a.all_notes_status_text())
    }

    /// The note the All Notes editor shows (it may differ from the selected row only briefly).
    pub fn all_notes_editor_note_id(&self) -> Option<NoteId> {
        self.app.as_ref().and_then(|a| a.all_notes()).map(|v| v.editor_note_id())
    }

    pub fn all_notes_editor_text(&self) -> Option<String> {
        self.app.as_ref().and_then(|a| a.all_notes()).map(|v| v.editor_text())
    }

    /// Type into the All Notes editor (replace its text without typing it).
    pub fn set_all_notes_editor_text(&mut self, text: &str) {
        if let Some(v) = self.app.as_ref().and_then(|a| a.all_notes()) {
            v.set_editor_text(text);
        }
        self.settle();
    }

    /// Press the All Notes editor's menu button.
    pub fn invoke_editor_menu(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_editor_menu();
        }
        self.settle();
    }

    pub fn editor_menu_button_rect(&self) -> Option<(f32, f32, f32, f32)> {
        self.app.as_ref().and_then(|a| a.editor_menu_button_rect())
    }

    /// Double click a row of the list.
    pub fn invoke_all_notes_open_note(&mut self, id: NoteId) {
        if let Some(app) = &self.app {
            app.invoke_all_notes_open_note(id);
        }
        self.settle();
    }

    /// The editor window's place and size, logical px.
    pub fn all_notes_window_geometry(&self) -> Option<(i32, i32, u32, u32)> {
        let v = self.app.as_ref().and_then(|a| a.all_notes())?;
        let ui = v.ui();
        let w = ui.window();
        let (p, sz) = (w.position(), w.size());
        Some((p.x, p.y, sz.width, sz.height))
    }

    pub fn all_notes_confirming_delete(&self) -> Option<bool> {
        self.app.as_ref().and_then(|a| a.all_notes_confirming_delete())
    }

    pub fn set_all_notes_confirming_delete(&mut self, confirming: bool) {
        if let Some(app) = &self.app {
            app.set_all_notes_confirming_delete(confirming);
        }
    }

    pub fn invoke_all_notes_select_note(&mut self, id: NoteId) {
        if let Some(app) = &self.app {
            app.invoke_all_notes_select_note(id);
        }
        self.settle();
    }

    pub fn invoke_all_notes_new_note(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_all_notes_new_note();
        }
        self.settle();
    }

    pub fn invoke_all_notes_open_or_focus(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_all_notes_open_or_focus();
        }
        self.settle();
    }

    pub fn invoke_all_notes_close_note(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_all_notes_close_note();
        }
        self.settle();
    }

    pub fn invoke_all_notes_export_note(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_all_notes_export_note();
        }
        self.settle();
    }

    pub fn invoke_all_notes_request_delete(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_all_notes_request_delete();
        }
        self.settle();
    }

    pub fn invoke_all_notes_cancel_delete(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_all_notes_cancel_delete();
        }
        self.settle();
    }

    pub fn invoke_all_notes_confirm_delete(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_all_notes_confirm_delete();
        }
        self.settle();
    }

    pub fn click_all_notes(&mut self, x: f32, y: f32) {
        if let Some(app) = &self.app {
            let position = slint::LogicalPosition::new(x, y);
            let button = slint::platform::PointerEventButton::Left;
            app.dispatch_all_notes_event(slint::platform::WindowEvent::PointerMoved { position });
            app.dispatch_all_notes_event(slint::platform::WindowEvent::PointerPressed { position, button });
            app.dispatch_all_notes_event(slint::platform::WindowEvent::PointerReleased { position, button });
        }
        self.settle();
    }

    /// The editor window of the open All Notes view: the newest window as wide as a note plus
    /// the sidebar's strip.
    fn all_notes_window(&self) -> Option<std::rc::Rc<crate::platform::StandInWindow>> {
        if !self.is_all_notes_open() {
            return None;
        }
        let (_, _, w, h) = self.all_notes_window_geometry()?;
        platform::windows().into_iter().rev().find(|win| {
            let s = win.window().size();
            s.width == w && s.height == h
        })
    }

    /// Turn the mouse wheel over the All Notes editor window at (x, y).
    pub fn scroll_all_notes(&mut self, x: f32, y: f32, dy: f32) {
        if let Some(app) = &self.app {
            let position = slint::LogicalPosition::new(x, y);
            app.dispatch_all_notes_event(slint::platform::WindowEvent::PointerMoved { position });
            app.dispatch_all_notes_event(slint::platform::WindowEvent::PointerScrolled { position, delta_x: 0.0, delta_y: dy });
        }
        self.settle();
    }

    pub fn render_all_notes(&self) -> Option<(u32, u32, Vec<slint::Rgb8Pixel>)> {
        let w = self.all_notes_window()?;
        w.window().request_redraw();
        let s = w.window().size();
        let (pw, ph) = (s.width, s.height);
        let mut buf = vec![slint::Rgb8Pixel::default(); (pw * ph) as usize];
        w.draw_if_needed(|r| {
            r.render(&mut buf, pw as usize);
        });
        Some((pw, ph, buf))
    }

    /// Render the open All Notes editor window onto a `#D6DBE2` desk canvas of `canvas` physical pixels.
    pub fn render_all_notes_canvas(&self, canvas: (u32, u32)) -> Option<crate::shot::Snapshot> {
        let (pw, ph, buf) = self.render_all_notes()?;
        let desk = [0xD6u8, 0xDB, 0xE2];
        let (cw, ch) = canvas;
        let (ox, oy) = ((cw as i64 - pw as i64) / 2, (ch as i64 - ph as i64) / 2);
        let mut rgb = Vec::with_capacity((cw * ch * 3) as usize);
        for y in 0..ch as i64 {
            for x in 0..cw as i64 {
                let (sx, sy) = (x - ox, y - oy);
                if sx >= 0 && sy >= 0 && sx < pw as i64 && sy < ph as i64 {
                    let p = buf[(sy as u32 * pw + sx as u32) as usize];
                    rgb.extend_from_slice(&[p.r, p.g, p.b]);
                } else {
                    rgb.extend_from_slice(&desk);
                }
            }
        }
        Some(crate::shot::Snapshot::from_rgb(cw, ch, rgb))
    }

    pub fn press_all_notes_escape(&mut self) {
        if let Some(app) = &self.app {
            app.dispatch_all_notes_event(slint::platform::WindowEvent::KeyPressed {
                text: slint::platform::Key::Escape.into(),
            });
            app.dispatch_all_notes_event(slint::platform::WindowEvent::KeyReleased {
                text: slint::platform::Key::Escape.into(),
            });
        }
        self.settle();
    }

    pub fn invoke_dropdown_select_note(&mut self, id: NoteId) {
        if let Some(app) = &self.app {
            app.invoke_dropdown_select_note(id);
        }
    }

    pub fn invoke_dropdown_dismiss(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_dropdown_dismiss();
        }
    }

    pub fn press_dropdown_escape(&mut self) {
        if let Some(app) = &self.app {
            app.dispatch_dropdown_event(slint::platform::WindowEvent::KeyPressed {
                text: slint::platform::Key::Escape.into(),
            });
            app.dispatch_dropdown_event(slint::platform::WindowEvent::KeyReleased {
                text: slint::platform::Key::Escape.into(),
            });
        }
    }

    pub fn is_delete_dialog_open(&self) -> bool {
        self.app.as_ref().map_or(false, |a| a.is_delete_dialog_open())
    }

    pub fn delete_dialog_note_id(&self) -> Option<NoteId> {
        self.app.as_ref().and_then(|a| a.delete_dialog_note_id())
    }

    pub fn invoke_confirm_delete(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_confirm_delete();
        }
        self.settle();
    }

    pub fn invoke_keep_note(&mut self) {
        if let Some(app) = &self.app {
            app.invoke_keep_note();
        }
        self.settle();
    }

    pub fn press_delete_dialog_escape(&mut self) {
        if let Some(app) = &self.app {
            app.dispatch_delete_dialog_event(slint::platform::WindowEvent::KeyPressed {
                text: slint::platform::Key::Escape.into(),
            });
            app.dispatch_delete_dialog_event(slint::platform::WindowEvent::KeyReleased {
                text: slint::platform::Key::Escape.into(),
            });
        }
        self.settle();
    }

    pub fn click_dropdown_outside(&mut self) {
        if let Some(app) = &self.app {
            let position = slint::LogicalPosition::new(5.0, 5.0);
            let button = slint::platform::PointerEventButton::Left;
            app.dispatch_dropdown_event(slint::platform::WindowEvent::PointerMoved { position });
            app.dispatch_dropdown_event(slint::platform::WindowEvent::PointerPressed { position, button });
            app.dispatch_dropdown_event(slint::platform::WindowEvent::PointerReleased { position, button });
        }
    }

    pub fn render_dropdown(&self) -> Option<(u32, u32, Vec<slint::Rgb8Pixel>)> {
        if !self.is_dropdown_open() {
            return None;
        }
        let wins = platform::windows();
        let w = wins.into_iter().rev().find(|w| {
            let s = w.window().size();
            s.width == 300 && s.height == 352
        })?;
        w.window().request_redraw();
        let s = w.window().size();
        let (pw, ph) = (s.width, s.height);
        let mut buf = vec![slint::Rgb8Pixel::default(); (pw * ph) as usize];
        w.draw_if_needed(|r| {
            r.render(&mut buf, pw as usize);
        });
        Some((pw, ph, buf))
    }

    pub fn dropdown_notes(&self) -> Vec<washi::note_window::DropdownNoteItem> {
        self.app.as_ref().map(|a| a.dropdown_notes()).unwrap_or_default()
    }

    pub(crate) fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
}
