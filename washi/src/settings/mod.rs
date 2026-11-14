//! The Settings page.
//!
//! Pressing Settings in a note's dropdown turns that note into one global Settings window: the window
//! shows at once at the host's Paper (identical blank paper) and grows left by the Sidebar's strip and
//! to the Settings size (about 190 ms) while the host's own window hides. Closing plays it in reverse,
//! puts the host back exactly where it was and fades its text in (40 ms). Same pattern as All notes: the
//! Settings window is a separate window, the note stays alive underneath.
//!
//! The page's bounds are set by an 8 ms timer from a wall-clock tween (so a slow frame skips ahead
//! instead of stretching the animation, and the harness can sample any instant). The timer holds the
//! system's 1 ms timer resolution while it runs: at the default ~15.6 ms grid a 16 ms timer fires every
//! ~31 ms.
//!
//! The native window is not resized every frame (that is what made the transitions choppy: each resize
//! is a round trip through the window manager and a new framebuffer). At the start of a transition the
//! window becomes a transparent canvas covering every pose the transition passes through, the page
//! moves and grows inside it, and once the page rests the window is trimmed to it.

pub mod anim;
pub mod appearance;
pub mod fonts;
pub mod shortcuts;
pub mod general;
pub mod stats;
pub mod style;
pub mod tape;

use crate::note_window::{app_icon, SettingsWindow};
use crate::platform::{NoteId, ScreenRect};
use crate::storage::Geometry;
use anim::{contains, fit_on_screen, screen_limit, grow_pose, logical_areas, paper_of_note, tab_settle, union, window_bounds, lerp_rect, Ease, Pose, RectF, Tween, GROW, TAB_SWITCH, MARGIN, NOTE_FADE, PAPER_MIN_H, PAPER_W, SIDEBAR_STRIP, MANAGER_STRIP, TICK, frame_step};
use slint::ComponentHandle;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

/// What the view needs from the app.
#[derive(Clone)]
pub struct Hooks {
    /// Show or hide the host note's desktop window.
    pub desktop_visible: Rc<dyn Fn(NoteId, bool)>,
    /// Put the host note back at this window geometry (logical px).
    pub restore_host: Rc<dyn Fn(NoteId, Geometry)>,
    /// Fade the host note's own content (1 is fully shown).
    pub host_content: Rc<dyn Fn(NoteId, f32)>,
    /// The view has closed: the app drops it.
    pub closed: Rc<dyn Fn()>,
    pub work_areas: Rc<dyn Fn() -> Vec<ScreenRect>>,
    pub begin_move: Rc<dyn Fn(Option<isize>)>,
    /// Monotonic time (the app's clock).
    pub now: Rc<dyn Fn() -> Duration>,
    /// Move and resize a native window in one call (physical px); false if the platform cannot.
    pub set_bounds: Rc<dyn Fn(Option<isize>, i32, i32, i32, i32) -> bool>,
    /// Ask for a fine system timer resolution (true) while frames run, and give it back (false).
    pub fine_timer: Rc<dyn Fn(bool)>,
    /// What the General tab reads and changes.
    pub general: general::GeneralDeps,
    /// The Sidebars' Height or Anchor changed (Settings › Appearance): an open All notes takes it.
    pub sidebar_layout_changed: Rc<dyn Fn()>,
    /// A Sidebar Theme was changed on the Appearance Tab: redraw the open All notes Sidebar.
    pub sidebar_themes_changed: Rc<dyn Fn()>,
    /// The UI size changed (already saved): redraw every window at it, this page included.
    pub ui_scale_changed: Rc<dyn Fn(f32)>,
    /// The Letter spacing changed (already saved), to this many em: every note lays its text out again.
    pub letter_spacing_changed: Rc<dyn Fn(f32)>,
    /// The Font size changed (already saved): every note lays its text out at the new size.
    pub font_size_changed: Rc<dyn Fn(u32)>,
}

#[derive(Clone, Debug)]
enum Phase {
    /// Both windows are on screen (identical blank paper) for one frame; then hide one and go on.
    Swap { then: Box<Phase>, closing: bool },
    /// The note (e = 0) and the Settings page (e = 1).
    Morph { tween: Tween, closing: bool },
    /// The Paper's height tweens to the tab's, and the new tab's content (shown at once) settles in.
    Tab { h: Tween },
    /// The Font manager slides out (or in): its strip on the page's right goes from `from_right` to
    /// `to_right` px while the Paper moves from `tab_from` to `tab_to` (it shifts left when the
    /// screen has no room for the slip).
    Manager { h: Tween, from_right: f32, to_right: f32 },
    /// At rest (the tick timer is stopped).
    Open,
    /// Settings hidden again; the host fades its text back in.
    NoteFadeIn { tween: Tween },
    Done,
}

impl Phase {
    fn restarted(self, now: Duration) -> Phase {
        match self {
            Phase::Morph { tween, closing } => Phase::Morph { tween: tween.restarted(now), closing },
            Phase::NoteFadeIn { tween } => Phase::NoteFadeIn { tween: tween.restarted(now) },
            other => other,
        }
    }
}

pub struct SettingsView {
    ui: SettingsWindow,
    hooks: Hooks,
    host: NoteId,
    /// The host note's window geometry when Settings opened: what closing restores.
    host_window: Cell<Geometry>,
    /// Where the Settings window was last put (to tell a drag by the user from where we placed it).
    placed: Cell<Geometry>,
    /// The window as it is now (logical px): where we put it, moved by any drag since.
    canvas: Cell<Geometry>,
    /// How many times the native window has been moved or resized by the view (for the tests).
    resizes: Cell<u32>,
    phase: RefCell<Phase>,
    tick: slint::Timer,
    /// Hides the scrollbar a moment after the last scroll.
    scroll_hold: slint::Timer,
    /// The same for the Font manager's list.
    manager_hold: slint::Timer,
    /// The Font manager is open, or opening (false: closed, or closing).
    manager_open: Cell<bool>,
    /// The Font manager's strip last applied (logical px, 0 to `MANAGER_STRIP`).
    right: Cell<f32>,
    /// Whether the fine (1 ms) system timer resolution is held for the frame timer.
    fine_timer: Cell<bool>,
    me: Weak<SettingsView>,
    /// The Paper at e = 0 and e = 1 of the current morph.
    morph_from: Cell<RectF>,
    morph_to: Cell<RectF>,
    /// Where the host goes back to when the close morph ends.
    close_target: Cell<Geometry>,
    /// The Sidebar strip last applied (logical px).
    strip: Cell<f32>,
    /// The Paper last drawn, and the ends of the current tab tween.
    paper: Cell<RectF>,
    tab_from: Cell<RectF>,
    tab_to: Cell<RectF>,
    /// The tab the page is showing or heading to.
    tab: Cell<i32>,
    general: Rc<general::General>,
    _appearance: Rc<appearance::Appearance>,
    /// The window's scale: the monitor's times the UI size.
    scale: crate::ui_scale::WindowScale,
    /// The animation clock the tweens run on (see `anim_now`), and the real time it was last read.
    anim: Cell<Duration>,
    anim_read: Cell<Duration>,
    /// The next frame is the first one of the open morph: it follows the page's first paint.
    first_frame: Cell<bool>,
}

impl SettingsView {
    /// Open Settings over the note `host` (its window at `host_window`, logical px).
    pub fn open(host: NoteId, host_window: Geometry, pinned: bool, hooks: Hooks) -> Result<Rc<Self>, slint::PlatformError> {
        let ui = SettingsWindow::new()?;
        crate::fonts::current().apply(&ui.global::<crate::note_window::Theme>());
        ui.set_app_icon(app_icon());
        ui.set_pinned(pinned);
        ui.set_shortcuts(shortcuts::model());
        tape::set_preview_tapes(&ui);
        let now = (hooks.now)();
        let general = general::General::new(&ui, hooks.general.clone(), hooks.now.clone());
        let appearance = appearance::Appearance::new(
            &ui,
            hooks.general.db.clone(),
            hooks.sidebar_layout_changed.clone(),
            hooks.sidebar_themes_changed.clone(),
            hooks.ui_scale_changed.clone(),
        );
        style::wire(&ui, hooks.general.env.clone(), hooks.general.db.clone(), hooks.letter_spacing_changed.clone(), hooks.font_size_changed.clone());
        fonts::wire(&ui);
        let scale = crate::ui_scale::WindowScale::attach(ui.window());
        let view = Rc::new_cyclic(|me| SettingsView {
            ui,
            hooks,
            host,
            host_window: Cell::new(host_window),
            placed: Cell::new(host_window),
            canvas: Cell::new(host_window),
            resizes: Cell::new(0),
            phase: RefCell::new(Phase::Swap {
                then: Box::new(Phase::Morph { tween: Tween::new(0.0, 1.0, Duration::ZERO, GROW, Ease::OutCubic), closing: false }),
                closing: false,
            }),
            tick: slint::Timer::default(),
            scroll_hold: slint::Timer::default(),
            manager_hold: slint::Timer::default(),
            manager_open: Cell::new(false),
            right: Cell::new(0.0),
            fine_timer: Cell::new(false),
            me: me.clone(),
            morph_from: Cell::new(paper_of_note(host_window)),
            morph_to: Cell::new(RectF::default()),
            close_target: Cell::new(host_window),
            strip: Cell::new(0.0),
            paper: Cell::new(RectF::default()),
            tab_from: Cell::new(RectF::default()),
            tab_to: Cell::new(RectF::default()),
            tab: Cell::new(0),
            general,
            _appearance: appearance,
            scale,
            anim: Cell::new(Duration::ZERO),
            anim_read: Cell::new(now),
            first_frame: Cell::new(false),
        });
        view.wire(Rc::downgrade(&view));
        view.morph_to.set(view.open_paper());
        // Settings shows at once, exactly on the note's Paper (blank, as the note's text is gone beneath
        // it); the grow starts on the next frame, when the host's window is hidden. The window already
        // covers the whole grow, so it is not resized while it plays.
        view.reserve(window_bounds(view.morph_from.get(), 0.0, 0.0), window_bounds(view.morph_to.get(), SIDEBAR_STRIP, 0.0));
        view.apply(grow_pose(view.morph_from.get(), view.morph_to.get(), 0.0));
        let _ = view.ui.show();
        crate::win::set_window_icon_when_ready(&view.ui);
        view.scale.resync(view.ui.window(), true);
        view.start_ticking();
        Ok(view)
    }

    /// The Paper at the Settings size, anchored at the host Paper's top-left and shifted only as far
    /// as keeping the window (with the Sidebar's strip out) on screen needs.
    fn open_paper(&self) -> RectF {
        let note = paper_of_note(self.host_window.get());
        let areas = self.areas();
        self.limit_to_screen(note, &areas);
        let want = RectF { x: note.x, y: note.y, w: PAPER_W, h: self.ui.invoke_fitted_height_for(self.tab.get()).max(PAPER_MIN_H) };
        fit_on_screen(want, SIDEBAR_STRIP, 0.0, &areas)
    }

    /// Draw `pose` inside the window. The window only changes if the pose falls outside it (a
    /// transition that did not reserve room for it).
    fn apply(&self, pose: Pose) {
        self.strip.set(pose.strip);
        self.right.set(pose.right);
        self.ui.set_strip(pose.strip);
        self.ui.set_manager_strip(pose.right);
        self.ui.set_manager_progress(pose.right / MANAGER_STRIP);
        self.paper.set(pose.paper);
        self.ui.set_content_opacity(pose.content);
        let page = window_bounds(pose.paper, pose.strip, pose.right);
        if !contains(self.canvas.get(), page) {
            self.place(union(self.canvas.get(), page));
        }
        // The page is drawn where the pose says, not on whole px: rounding its left edge while the
        // strip slides out (a fraction of a px each frame) made the Paper wobble sideways by a pixel.
        let c = self.canvas.get();
        self.ui.set_page_x(pose.paper.x - MARGIN - pose.strip - c.x as f32);
        self.ui.set_page_y(pose.paper.y - MARGIN - c.y as f32);
        self.ui.set_page_w(pose.paper.w + 2.0 * MARGIN + pose.strip + pose.right);
        self.ui.set_page_h(pose.paper.h + 2.0 * MARGIN);
    }

    /// Put the page (Sidebar strip, margin and Paper) at `page`, screen logical px, in the window.
    fn show_page(&self, page: Geometry) {
        let c = self.canvas.get();
        self.ui.set_page_x((page.x - c.x) as f32);
        self.ui.set_page_y((page.y - c.y) as f32);
        self.ui.set_page_w(page.w as f32);
        self.ui.set_page_h(page.h as f32);
    }

    /// Where the page is on screen (whole logical px: each edge rounded, as `window_bounds` does).
    pub fn page_geometry(&self) -> Geometry {
        let c = self.canvas.get();
        let ui = &self.ui;
        let (left, top) = (ui.get_page_x().round() as i32, ui.get_page_y().round() as i32);
        let right = (ui.get_page_x() + ui.get_page_w()).round() as i32;
        let bottom = (ui.get_page_y() + ui.get_page_h()).round() as i32;
        Geometry { x: c.x + left, y: c.y + top, w: (right - left).max(1) as u32, h: (bottom - top).max(1) as u32 }
    }

    /// The UI size changed while the page is open: draw it at `ui` at once. The page keeps its top
    /// left on screen and its logical size, so it shrinks or grows on screen. Whatever the view holds
    /// in logical px (the host's window, where it put the page) is carried into the new logical px by
    /// `k` (old UI size / new), so closing still puts the host back where it was on screen.
    pub fn set_ui_scale(&self, ui: f32, k: f32) {
        let rect = |r: RectF| RectF { x: r.x * k, y: r.y * k, w: r.w * k, h: r.h * k };
        let page = self.page_geometry().scaled(k);
        let paper = rect(self.paper.get());
        for g in [&self.host_window, &self.placed, &self.canvas, &self.close_target] {
            g.set(g.get().scaled(k));
        }
        for r in [&self.morph_to, &self.tab_from, &self.tab_to] {
            r.set(rect(r.get()));
        }
        self.paper.set(paper);
        self.scale.set_ui(self.ui.window(), ui);
        self.show_page(page);
        self.morph_from.set(paper_of_note(self.host_window.get()));
        // the screen holds more logical px now: the cap follows, then the Paper
        let areas = self.areas();
        self.limit_to_screen(paper, &areas);
        let h = self.ui.invoke_fitted_height_for(self.tab.get()).max(PAPER_MIN_H);
        let right = self.right_target();
        let fit = fit_on_screen(RectF { x: paper.x, y: paper.y, w: PAPER_W, h }, SIDEBAR_STRIP, right, &areas);
        let phase = self.phase.borrow().clone();
        match phase {
            Phase::Open | Phase::Tab { .. } | Phase::Manager { .. } => {
                *self.phase.borrow_mut() = Phase::Open;
                self.stop_ticking();
                self.ui.set_content_dy(0.0);
                self.morph_to.set(fit);
                self.apply(Pose { paper: fit, strip: SIDEBAR_STRIP, right, content: 1.0 });
                self.fit();
            }
            Phase::Morph { closing: false, .. } => self.morph_to.set(fit),
            _ => {}
        }
    }

    /// The window's scale changed. When the platform did it (the window moved to a monitor with
    /// another DPI, and the platform resized it by the DPIs' ratio), put the UI size back on top:
    /// the page keeps its logical size and so grows or shrinks on screen with the monitor. Where
    /// the window now is in logical px follows from its place on screen at the new scale.
    fn scale_factor_changed(&self) {
        if self.scale.resync(self.ui.window(), false) {
            self.sync_canvas();
        }
    }

    /// How many times the view has moved or resized the native window so far.
    pub fn window_resizes(&self) -> u32 {
        self.resizes.get()
    }

    /// The user may have dragged the window since we placed it: follow it.
    fn sync_canvas(&self) {
        let w = self.ui.window();
        let s = w.scale_factor();
        let p = w.position();
        let c = self.canvas.get();
        self.canvas.set(Geometry { x: (p.x as f32 / s).round() as i32, y: (p.y as f32 / s).round() as i32, ..c });
    }

    /// Before a transition from page bounds `a` to `b`: make the window cover both (with a pixel of
    /// slack for rounding) in one move, and keep the page where it is on screen.
    fn reserve(&self, a: Geometry, b: Geometry) {
        let u = union(a, b);
        if contains(self.canvas.get(), u) {
            return;
        }
        let want = Geometry { x: u.x - 1, y: u.y - 1, w: u.w + 2, h: u.h + 2 };
        {
            let page = self.page_geometry();
            self.place(want);
            self.show_page(page);
        }
    }

    /// The page rests: trim the window to it.
    fn fit(&self) {
        let page = self.page_geometry();
        if self.canvas.get() != page {
            self.place(page);
            self.show_page(page);
        }
    }

    fn place(&self, g: Geometry) {
        self.placed.set(g);
        self.canvas.set(g);
        self.resizes.set(self.resizes.get() + 1);
        let w = self.ui.window();
        let s = w.scale_factor();
        let phys = |v: i32| (v as f32 * s).round() as i32;
        if (self.hooks.set_bounds)(self.hwnd(), phys(g.x), phys(g.y), phys(g.w as i32), phys(g.h as i32)) {
            return;
        }
        w.set_size(slint::PhysicalSize::new(phys(g.w as i32) as u32, phys(g.h as i32) as u32));
        w.set_position(slint::PhysicalPosition::new(phys(g.x), phys(g.y)));
    }

    fn wire(&self, me: Weak<SettingsView>) {
        {
            let me = me.clone();
            self.ui.on_close_clicked(move || {
                if let Some(v) = me.upgrade() {
                    v.close();
                }
            });
        }
        {
            let me = me.clone();
            self.ui.window().on_close_requested(move || {
                if let Some(v) = me.upgrade() {
                    v.close();
                }
                slint::CloseRequestResponse::KeepWindowShown
            });
        }
        {
            let me = me.clone();
            self.ui.on_start_move(move || {
                if let Some(v) = me.upgrade() {
                    (v.hooks.begin_move)(v.hwnd());
                }
            });
        }
        {
            let me = me.clone();
            self.ui.on_paper_height_changed(move || {
                if let Some(v) = me.upgrade() {
                    v.refit_height();
                }
            });
        }
        {
            let me = me.clone();
            self.ui.on_scale_factor_changed(move || {
                if let Some(v) = me.upgrade() {
                    v.scale_factor_changed();
                }
            });
        }
        {
            let me = me.clone();
            self.ui.on_scroll_activity(move || {
                let Some(v) = me.upgrade() else { return };
                let ui = v.ui.as_weak();
                v.scroll_hold.start(slint::TimerMode::SingleShot, crate::note_window::SCROLLBAR_HOLD, move || {
                    if let Some(ui) = ui.upgrade() {
                        ui.set_content_scrolling(false);
                    }
                });
            });
        }
        {
            let me = me.clone();
            self.ui.on_manage_clicked(move || {
                if let Some(v) = me.upgrade() {
                    v.toggle_manager();
                }
            });
        }
        {
            let me = me.clone();
            self.ui.on_manager_scroll_activity(move || {
                let Some(v) = me.upgrade() else { return };
                let ui = v.ui.as_weak();
                v.manager_hold.start(slint::TimerMode::SingleShot, crate::note_window::SCROLLBAR_HOLD, move || {
                    if let Some(ui) = ui.upgrade() {
                        ui.set_manager_scrolling(false);
                    }
                });
            });
        }
        self.ui.on_tab_clicked(move |to| {
            if let Some(v) = me.upgrade() {
                v.select_tab(to);
            }
        });
    }

    /// The shown tab's content changed height while the page rests (the preview note grew with the
    /// text size): resize the Paper to fit, keeping its top left. Mid-transition the tween is already
    /// heading for the tab's height.
    fn refit_height(&self) {
        if !matches!(*self.phase.borrow(), Phase::Open) {
            return;
        }
        let now = self.actual_paper();
        let areas = self.areas();
        self.limit_to_screen(now, &areas);
        let h = self.ui.invoke_fitted_height_for(self.tab.get()).max(PAPER_MIN_H);
        if (h - now.h).abs() < 0.5 {
            return;
        }
        let fit = fit_on_screen(RectF { h, ..now }, SIDEBAR_STRIP, self.right.get(), &areas);
        self.apply(Pose { paper: fit, strip: SIDEBAR_STRIP, right: self.right.get(), content: 1.0 });
        self.fit();
    }

    fn areas(&self) -> Vec<RectF> {
        logical_areas(&(self.hooks.work_areas)(), self.ui.window().scale_factor())
    }

    /// Tell the page how tall the screen under `paper` lets it grow (part of the Settings height cap).
    fn limit_to_screen(&self, paper: RectF, areas: &[RectF]) {
        self.ui.set_screen_limit(screen_limit(paper, areas).unwrap_or(100_000.0));
    }

    /// A Sidebar tab was pressed: light it up and show its content at once, then tween the Paper's
    /// height to the tab's while the content settles in. Starting from wherever the last tween is, so
    /// a retarget is smooth.
    pub fn select_tab(&self, to: i32) {
        if !matches!(*self.phase.borrow(), Phase::Open | Phase::Tab { .. } | Phase::Manager { .. }) || to == self.tab.get() {
            return;
        }
        // leaving the Font & Style Tab (or any switch) closes the Font manager
        self.shut_manager_now();
        let now = self.anim_now();
        self.ui.set_selected_tab(to);
        self.ui.set_shown_tab(to);
        // every Tab starts at its top
        self.ui.set_content_scroll(0.0);
        let resting = matches!(*self.phase.borrow(), Phase::Open);
        // At rest the Paper is where the user left it; mid-tween it is where the last frame put it.
        let from = if resting { self.actual_paper() } else { self.paper.get() };
        let areas = self.areas();
        self.limit_to_screen(from, &areas);
        let want = RectF { h: self.ui.invoke_fitted_height_for(to).max(PAPER_MIN_H), ..from };
        let target = fit_on_screen(want, SIDEBAR_STRIP, 0.0, &areas);
        self.tab_from.set(from);
        self.tab_to.set(target);
        self.tab.set(to);
        self.reserve(window_bounds(from, SIDEBAR_STRIP, 0.0), window_bounds(target, SIDEBAR_STRIP, 0.0));
        let (content, dy) = tab_settle(0.0);
        self.apply(Pose { paper: from, strip: SIDEBAR_STRIP, right: 0.0, content });
        self.ui.set_content_dy(dy);
        *self.phase.borrow_mut() = Phase::Tab { h: Tween::new(0.0, 1.0, now, TAB_SWITCH, Ease::OutCubic) };
        self.start_ticking();
        self.sync_general();
    }

    /// The Font manager's strip when its tween has finished: all of it while it is open.
    fn right_target(&self) -> f32 {
        if self.manager_open.get() {
            MANAGER_STRIP
        } else {
            0.0
        }
    }

    /// The manage button or Esc: open the Font manager if it is closed, close it if it is open. The
    /// slip slides out beside the Paper (190 ms on the Settings clock) while the window's strip on
    /// the right grows to hold it; the Paper moves left only as far as the screen's right edge needs.
    pub fn toggle_manager(&self) {
        let resting = match *self.phase.borrow() {
            Phase::Open => true,
            Phase::Manager { .. } => false,
            _ => return,
        };
        let open = !self.manager_open.get();
        let now = self.anim_now();
        let from = if resting { self.actual_paper() } else { self.paper.get() };
        let areas = self.areas();
        self.limit_to_screen(from, &areas);
        let target = if open { fit_on_screen(from, SIDEBAR_STRIP, MANAGER_STRIP, &areas) } else { from };
        let (from_right, to_right) = (self.right.get(), if open { MANAGER_STRIP } else { 0.0 });
        self.manager_open.set(open);
        self.ui.set_manager_open(open);
        self.tab_from.set(from);
        self.tab_to.set(target);
        self.reserve(window_bounds(from, SIDEBAR_STRIP, from_right), window_bounds(target, SIDEBAR_STRIP, from_right.max(to_right)));
        *self.phase.borrow_mut() = Phase::Manager { h: Tween::new(0.0, 1.0, now, GROW, Ease::OutCubic), from_right, to_right };
        self.start_ticking();
    }

    /// Close the Font manager at once, wherever its tween is (a Tab switch or closing Settings):
    /// the Paper stays where it is.
    fn shut_manager_now(&self) {
        if !self.manager_open.get() && self.right.get() == 0.0 {
            return;
        }
        let resting = matches!(*self.phase.borrow(), Phase::Open);
        let paper = if resting { self.actual_paper() } else { self.paper.get() };
        self.manager_open.set(false);
        self.ui.set_manager_open(false);
        self.apply(Pose { paper, strip: SIDEBAR_STRIP, right: 0.0, content: 1.0 });
        if matches!(*self.phase.borrow(), Phase::Manager { .. }) {
            *self.phase.borrow_mut() = Phase::Open;
        }
    }

    /// Whether the Font manager is open (or opening).
    pub fn is_manager_open(&self) -> bool {
        self.manager_open.get()
    }

    /// Whether the stats refresh is running (only while General is showing).
    pub fn is_refreshing_stats(&self) -> bool {
        self.general.is_refreshing()
    }

    /// Resource use refreshes only while the page is at rest or switching tabs and General is the
    /// tab the Sidebar lights: not mid-open, not once closing has begun, not on another tab.
    fn sync_general(&self) {
        let page_up = matches!(*self.phase.borrow(), Phase::Open | Phase::Tab { .. } | Phase::Manager { .. });
        self.general.set_showing(page_up && self.tab.get() == 0);
    }

    /// The time the tweens run on. It follows the real clock, but one frame never moves it by more
    /// than `MAX_FRAME`, and the first frame of the open morph by more than a tick: a frame that
    /// took long to draw (the page's first paint above all) then plays late instead of being
    /// skipped, which showed as the Paper jumping most of the way at once.
    fn anim_now(&self) -> Duration {
        let real = (self.hooks.now)();
        let dt = real.saturating_sub(self.anim_read.replace(real));
        let t = self.anim.get() + frame_step(dt, self.first_frame.replace(false));
        self.anim.set(t);
        t
    }

    /// Stop the frame timer and give back the fine timer resolution.
    fn stop_ticking(&self) {
        self.tick.stop();
        if self.fine_timer.replace(false) {
            (self.hooks.fine_timer)(false);
        }
    }

    fn start_ticking(&self) {
        if self.tick.running() {
            return;
        }
        // Windows timers fire on a ~15.6 ms grid unless asked otherwise: a 16 ms frame timer then
        // lands every ~31 ms (about 30 fps), which is what made the transition look choppy.
        if !self.fine_timer.replace(true) {
            (self.hooks.fine_timer)(true);
        }
        let me = self.me.clone();
        self.tick.start(slint::TimerMode::Repeated, TICK, move || {
            if let Some(v) = me.upgrade() {
                crate::trace::mark("settings-frame");
                v.step();
            }
        });
    }

    /// Whether the frame timer is running (it is stopped at rest).
    pub fn is_ticking(&self) -> bool {
        self.tick.running()
    }

    /// Whether the Settings window is on screen.
    pub fn is_shown(&self) -> bool {
        self.ui.window().is_visible()
    }

    fn step(&self) {
        let now = self.anim_now();
        let phase = self.phase.borrow().clone();
        let next: Option<Phase> = match phase {
            Phase::Swap { then, closing } => {
                // The window that goes away does so once the one taking over has drawn its first
                // frame: hiding it at once left a frame with neither on screen (a flicker).
                if closing {
                    let ui = self.ui.as_weak();
                    slint::Timer::single_shot(crate::all_notes::SWAP_OVERLAP, move || {
                        if let Some(ui) = ui.upgrade() {
                            let _ = ui.hide();
                        }
                    });
                } else {
                    // the host's paper goes blank at once, the same blank paper Settings starts as
                    (self.hooks.host_content)(self.host, 0.0);
                    // the page's first paint comes before the next frame: the grow starts after it
                    self.first_frame.set(true);
                    let (hide, host) = (self.hooks.desktop_visible.clone(), self.host);
                    let me = self.me.clone();
                    slint::Timer::single_shot(crate::all_notes::SWAP_OVERLAP, move || {
                        // not if Settings already closed back onto the host meanwhile
                        if me.upgrade().is_some_and(|v| !matches!(*v.phase.borrow(), Phase::Done | Phase::NoteFadeIn { .. } | Phase::Swap { closing: true, .. })) {
                            hide(host, false);
                        }
                    });
                }
                Some(then.restarted(now))
            }
            Phase::Morph { tween, closing } => {
                let (e, done) = tween.sample(now);
                self.apply(grow_pose(self.morph_from.get(), self.morph_to.get(), e));
                match (done, closing) {
                    (false, _) => None,
                    (true, false) => Some(Phase::Open),
                    (true, true) => {
                        (self.hooks.restore_host)(self.host, self.close_target.get());
                        (self.hooks.host_content)(self.host, 0.0);
                        (self.hooks.desktop_visible)(self.host, true);
                        Some(Phase::Swap { then: Box::new(Phase::NoteFadeIn { tween: Tween::new(0.0, 1.0, now, NOTE_FADE, Ease::OutCubic) }), closing: true })
                    }
                }
            }
            Phase::NoteFadeIn { tween } => {
                let (o, done) = tween.sample(now);
                (self.hooks.host_content)(self.host, o);
                if done {
                    (self.hooks.closed)();
                    Some(Phase::Done)
                } else {
                    None
                }
            }
            Phase::Tab { h } => {
                let (e, done) = h.sample(now);
                let (content, dy) = tab_settle(h.progress(now));
                self.apply(Pose { paper: lerp_rect(self.tab_from.get(), self.tab_to.get(), e), strip: SIDEBAR_STRIP, right: 0.0, content });
                self.ui.set_content_dy(dy);
                done.then_some(Phase::Open)
            }
            Phase::Manager { h, from_right, to_right } => {
                let (e, done) = h.sample(now);
                let right = from_right + (to_right - from_right) * e;
                self.apply(Pose { paper: lerp_rect(self.tab_from.get(), self.tab_to.get(), e), strip: SIDEBAR_STRIP, right, content: 1.0 });
                done.then_some(Phase::Open)
            }
            Phase::Open | Phase::Done => {
                self.stop_ticking();
                None
            }
        };
        if let Some(p) = next {
            let rest = matches!(p, Phase::Open | Phase::Done);
            let open = matches!(p, Phase::Open);
            *self.phase.borrow_mut() = p;
            if rest {
                self.stop_ticking();
            }
            if open {
                self.fit();
            }
        }
        self.sync_general();
    }

    /// X or Esc: play the transition backwards, from wherever it is, and put the host back.
    pub fn close(&self) {
        // the Font manager closes with Settings
        self.shut_manager_now();
        let now = self.anim_now();
        let phase = self.phase.borrow().clone();
        let next = match phase {
            Phase::Morph { closing: true, .. } | Phase::Swap { closing: true, .. } | Phase::NoteFadeIn { .. } | Phase::Done => return,
            Phase::Morph { tween, closing: false } => {
                let (e, _) = tween.sample(now);
                self.freeze_close(false);
                Phase::Morph { tween: Tween::new(e, 0.0, now, GROW, Ease::OutCubic), closing: true }
            }
            Phase::Swap { .. } => {
                // Settings has just shown and the morph has not started: nothing to undo but the swap
                self.freeze_close(false);
                Phase::Morph { tween: Tween::new(0.0, 0.0, now, TICK, Ease::Linear), closing: true }
            }
            Phase::Open | Phase::Tab { .. } | Phase::Manager { .. } => {
                // Mid tab switch the page is where the last frame put it; the close morph takes over.
                self.freeze_close(true);
                Phase::Morph { tween: Tween::new(1.0, 0.0, now, GROW, Ease::OutCubic), closing: true }
            }
        };
        // the close morph plays inside one window too
        self.ui.set_content_dy(0.0);
        self.reserve(self.page_geometry(), window_bounds(self.morph_from.get(), 0.0, 0.0));
        *self.phase.borrow_mut() = next;
        self.start_ticking();
        self.sync_general();
    }

    /// Fix both ends of the close morph. The note end follows where the user dragged the page; the
    /// page end is where Settings is now when it is at rest (it may have been dragged), otherwise
    /// where the open morph was heading, so a reversal continues from the current frame.
    fn freeze_close(&self, at_rest: bool) {
        let back = self.host_after_moves();
        self.morph_from.set(paper_of_note(back));
        self.close_target.set(back);
        if at_rest {
            self.morph_to.set(self.actual_paper());
        }
    }

    /// The Paper where the page really is now (it may have been dragged).
    fn actual_paper(&self) -> RectF {
        self.sync_canvas();
        let page = self.page_geometry();
        let (strip, right) = (self.strip.get(), self.right.get());
        RectF {
            x: page.x as f32 + strip + MARGIN,
            y: page.y as f32 + MARGIN,
            w: page.w as f32 - strip - right - 2.0 * MARGIN,
            h: page.h as f32 - 2.0 * MARGIN,
        }
    }

    /// If the user dragged the Settings page, the host comes back moved by the same amount. A
    /// shift made only to keep Settings on screen is undone.
    fn host_after_moves(&self) -> Geometry {
        let w = self.ui.window();
        let s = w.scale_factor();
        let at = w.position();
        let placed = self.placed.get();
        let (dx, dy) = ((at.x as f32 / s).round() as i32 - placed.x, (at.y as f32 / s).round() as i32 - placed.y);
        let g = self.host_window.get();
        Geometry { x: g.x + dx, y: g.y + dy, ..g }
    }

    pub fn host(&self) -> NoteId {
        self.host
    }

    pub fn ui(&self) -> &SettingsWindow {
        &self.ui
    }

    pub fn hwnd(&self) -> Option<isize> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let window_handle = self.ui.window().window_handle();
        let handle = window_handle.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
            _ => None,
        }
    }
}
