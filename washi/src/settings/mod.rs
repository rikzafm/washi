//! The Settings page.
//!
//! Pressing Settings in a note's dropdown turns that note into one global Settings window: the host's
//! text fades out, the window shows at the host's Paper (identical blank paper) and then grows left by
//! the Sidebar's strip and to the Settings size while the host's own window hides. Closing plays it in
//! reverse and puts the host back exactly where it was. Same pattern as All notes: the Settings window
//! is a separate window, the note stays alive underneath.
//!
//! The window's bounds are set by a ~16 ms timer from a wall-clock tween (so a slow frame skips ahead
//! instead of stretching the animation, and the harness can sample any instant).

pub mod anim;
pub mod general;
pub mod stats;

use crate::note_window::{app_icon, SettingsWindow};
use crate::platform::{NoteId, ScreenRect};
use crate::storage::Geometry;
use anim::{fit_on_screen, grow_pose, logical_areas, paper_of_note, window_bounds, lerp_rect, tab_fade, tab_fade_resume, Ease, Pose, RectF, Tween, GROW, TAB_SWITCH, MARGIN, NOTE_FADE, PAPER_MIN_H, PAPER_W, SIDEBAR_STRIP, TICK};
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
    /// What the General tab reads and changes.
    pub general: general::GeneralDeps,
}

#[derive(Clone, Debug)]
enum Phase {
    /// The host fades its text out; Settings is not shown yet.
    NoteFadeOut { tween: Tween },
    /// Both windows are on screen (identical blank paper) for one frame; then hide one and go on.
    Swap { then: Box<Phase>, closing: bool },
    /// The note (e = 0) and the Settings page (e = 1).
    Morph { tween: Tween, closing: bool },
    /// The Paper's height tweens to the tab's, and the content cross-fades. `fade_start` is when the fade
    /// began (earlier than the tween when a switch retargets mid-fade, so the opacity stays continuous).
    Tab { h: Tween, fade_start: Duration, to_tab: i32, swapped: bool },
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
    host_window: Geometry,
    /// Where the Settings window was last put (to tell a drag by the user from where we placed it).
    placed: Cell<Geometry>,
    phase: RefCell<Phase>,
    tick: slint::Timer,
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
}

impl SettingsView {
    /// Open Settings over the note `host` (its window at `host_window`, logical px).
    pub fn open(host: NoteId, host_window: Geometry, pinned: bool, hooks: Hooks) -> Result<Rc<Self>, slint::PlatformError> {
        let ui = SettingsWindow::new()?;
        ui.set_app_icon(app_icon());
        ui.set_pinned(pinned);
        let now = (hooks.now)();
        let general = general::General::new(&ui, hooks.general.clone(), hooks.now.clone());
        let view = Rc::new_cyclic(|me| SettingsView {
            ui,
            hooks,
            host,
            host_window,
            placed: Cell::new(host_window),
            phase: RefCell::new(Phase::NoteFadeOut { tween: Tween::new(1.0, 0.0, now, NOTE_FADE, Ease::OutCubic) }),
            tick: slint::Timer::default(),
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
        });
        view.wire(Rc::downgrade(&view));
        view.morph_to.set(view.open_paper());
        // The window waits, unshown, exactly on the note's Paper while the note's text fades out.
        view.apply(grow_pose(view.morph_from.get(), view.morph_to.get(), 0.0));
        view.start_ticking();
        Ok(view)
    }

    /// The Paper at the Settings size, anchored at the host Paper's top-left and shifted only as far
    /// as keeping the window (with the Sidebar's strip out) on screen needs.
    fn open_paper(&self) -> RectF {
        let note = paper_of_note(self.host_window);
        let want = RectF { x: note.x, y: note.y, w: PAPER_W, h: PAPER_MIN_H };
        let areas = logical_areas(&(self.hooks.work_areas)(), self.ui.window().scale_factor());
        fit_on_screen(want, SIDEBAR_STRIP, &areas)
    }

    /// Draw `pose`: strip and content first, then the window's bounds in one step.
    fn apply(&self, pose: Pose) {
        self.strip.set(pose.strip);
        self.ui.set_strip(pose.strip);
        self.paper.set(pose.paper);
        self.ui.set_content_opacity(pose.content);
        self.place(window_bounds(pose.paper, pose.strip));
    }

    fn place(&self, g: Geometry) {
        self.placed.set(g);
        let w = self.ui.window();
        let s = w.scale_factor();
        let phys = |v: i32| (v as f32 * s).round() as i32;
        if (self.hooks.set_bounds)(self.hwnd(), phys(g.x), phys(g.y), phys(g.w as i32), phys(g.h as i32)) {
            return;
        }
        w.set_size(slint::LogicalSize::new(g.w as f32, g.h as f32));
        w.set_position(slint::LogicalPosition::new(g.x as f32, g.y as f32));
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
            self.ui.on_start_move(move || {
                if let Some(v) = me.upgrade() {
                    (v.hooks.begin_move)(v.hwnd());
                }
            });
        }
        self.ui.on_tab_clicked(move |to| {
            if let Some(v) = me.upgrade() {
                v.select_tab(to);
            }
        });
    }

    fn areas(&self) -> Vec<RectF> {
        logical_areas(&(self.hooks.work_areas)(), self.ui.window().scale_factor())
    }

    /// A Sidebar tab was pressed: light it up at once, tween the Paper's height to the tab's and
    /// cross-fade the content. Starting from wherever the last tween is, so a retarget is smooth.
    pub fn select_tab(&self, to: i32) {
        if !matches!(*self.phase.borrow(), Phase::Open | Phase::Tab { .. }) || to == self.tab.get() {
            return;
        }
        let now = self.now();
        self.ui.set_selected_tab(to);
        let resting = matches!(*self.phase.borrow(), Phase::Open);
        // At rest the Paper is where the user left it; mid-tween it is where the last frame put it.
        let from = if resting { self.actual_paper() } else { self.paper.get() };
        let opacity = self.ui.get_content_opacity();
        let fade_start = now.saturating_sub(TAB_SWITCH.mul_f32(tab_fade_resume(opacity)));
        let want = RectF { h: self.ui.invoke_paper_height_for(to).max(PAPER_MIN_H), ..from };
        self.tab_from.set(from);
        self.tab_to.set(fit_on_screen(want, SIDEBAR_STRIP, &self.areas()));
        self.tab.set(to);
        *self.phase.borrow_mut() = Phase::Tab { h: Tween::new(0.0, 1.0, now, TAB_SWITCH, Ease::InOutCubic), fade_start, to_tab: to, swapped: false };
        self.start_ticking();
        self.sync_general();
    }

    /// Whether the stats refresh is running (only while General is showing).
    pub fn is_refreshing_stats(&self) -> bool {
        self.general.is_refreshing()
    }

    /// Resource use refreshes only while the page is at rest or switching tabs and General is the
    /// tab the Sidebar lights: not mid-open, not once closing has begun, not on another tab.
    fn sync_general(&self) {
        let page_up = matches!(*self.phase.borrow(), Phase::Open | Phase::Tab { .. });
        self.general.set_showing(page_up && self.tab.get() == 0);
    }

    fn now(&self) -> Duration {
        (self.hooks.now)()
    }

    fn start_ticking(&self) {
        if self.tick.running() {
            return;
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
        let now = self.now();
        let phase = self.phase.borrow().clone();
        let next: Option<Phase> = match phase {
            Phase::NoteFadeOut { tween } => {
                let (o, done) = tween.sample(now);
                (self.hooks.host_content)(self.host, o);
                if done {
                    let _ = self.ui.show();
                    Some(Phase::Swap { then: Box::new(self.open_morph(now)), closing: false })
                } else {
                    None
                }
            }
            Phase::Swap { then, closing } => {
                if closing {
                    let _ = self.ui.hide();
                } else {
                    (self.hooks.desktop_visible)(self.host, false);
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
            Phase::Tab { h, fade_start, to_tab, swapped } => {
                let (e, h_done) = h.sample(now);
                let t = now.saturating_sub(fade_start).as_secs_f32() / TAB_SWITCH.as_secs_f32();
                let (content, second_half) = tab_fade(t);
                if second_half && !swapped {
                    self.ui.set_shown_tab(to_tab);
                }
                self.apply(Pose { paper: lerp_rect(self.tab_from.get(), self.tab_to.get(), e), strip: SIDEBAR_STRIP, content });
                if h_done && t >= 1.0 {
                    Some(Phase::Open)
                } else if second_half && !swapped {
                    Some(Phase::Tab { h, fade_start, to_tab, swapped: true })
                } else {
                    None
                }
            }
            Phase::Open | Phase::Done => {
                self.tick.stop();
                None
            }
        };
        if let Some(p) = next {
            let rest = matches!(p, Phase::Open | Phase::Done);
            *self.phase.borrow_mut() = p;
            if rest {
                self.tick.stop();
            }
        }
        self.sync_general();
    }

    fn open_morph(&self, now: Duration) -> Phase {
        Phase::Morph { tween: Tween::new(0.0, 1.0, now, GROW, Ease::OutCubic), closing: false }
    }

    /// X or Esc: play the transition backwards, from wherever it is, and put the host back.
    pub fn close(&self) {
        let now = self.now();
        let phase = self.phase.borrow().clone();
        let next = match phase {
            Phase::NoteFadeOut { tween } => {
                // Settings never showed: the host's text just comes back
                let (o, _) = tween.sample(now);
                Phase::NoteFadeIn { tween: Tween::new(o, 1.0, now, NOTE_FADE, Ease::OutCubic) }
            }
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
            Phase::Open | Phase::Tab { .. } => {
                // Mid tab switch the page is where the last frame put it; the close morph takes over.
                self.freeze_close(true);
                Phase::Morph { tween: Tween::new(1.0, 0.0, now, GROW, Ease::OutCubic), closing: true }
            }
        };
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

    /// The Paper where the Settings window really is now.
    fn actual_paper(&self) -> RectF {
        let w = self.ui.window();
        let s = w.scale_factor();
        let (p, sz) = (w.position(), w.size());
        let strip = self.strip.get();
        RectF {
            x: p.x as f32 / s + strip + MARGIN,
            y: p.y as f32 / s + MARGIN,
            w: sz.width as f32 / s - strip - 2.0 * MARGIN,
            h: sz.height as f32 / s - 2.0 * MARGIN,
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
        let g = self.host_window;
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
