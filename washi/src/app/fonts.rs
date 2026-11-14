//! Installing, picking and removing fonts, and loading the Font in use at startup.
//!
//! The Font manager (`settings/fonts.rs`) only draws rows and reports clicks. Here a click becomes a
//! change: the Face set of the picked font is baked and registered with Slint first, then every
//! window is given its families (`App::set_active_font`), then the choice is saved (ADR 0003).
//! A package is read, validated and written off the UI thread; the database and the windows are only
//! touched on it.

use super::{App, Env};
use crate::fonts::registry;
use crate::fonts::store::FontStore;
use crate::fonts::{
    apply_regular_preview, bake, bake_regular, default_package, read_package, ActiveFont, AxisRanges, AxisValues, BakeError, FontPackage, ThemeMetrics,
    DEFAULT_TEXT_FAMILY,
};
use crate::settings::fonts::{self as manager, RowSpec};
use crate::storage::{Db, FontRow};
use slint::ComponentHandle;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Mutex;
use std::time::Duration;

/// A font being read and written on its worker thread: `(id, family)` when done, or the reason.
pub(super) type InstallResult = Result<(String, String), String>;

pub(super) struct FontInstall {
    result: Receiver<InstallResult>,
}

/// How often the worker threads' results are looked for while a font is being installed.
const INSTALL_POLL: Duration = Duration::from_millis(20);

/// One install writes at a time, so two installs of one family never share a temporary folder.
static WRITING: Mutex<()> = Mutex::new(());

/// Largest file looked at (the archive cap of the engine).
const MAX_FILE: u64 = crate::fonts::MAX_ARCHIVE_BYTES as u64;

/// Read, validate and store the font file at `path`: everything `Install a font` does below the
/// file picker. Runs on a worker thread.
fn prepare(path: &std::path::Path, store: &FontStore) -> InstallResult {
    let meta = std::fs::metadata(path).map_err(|e| format!("Washi could not read that file: {e}"))?;
    if !meta.is_file() {
        return Err(crate::fonts::PackageError::NotAFont.to_string());
    }
    if meta.len() > MAX_FILE {
        return Err(crate::fonts::PackageError::TooLarge("over 64 MB").to_string());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Washi could not read that file: {e}"))?;
    let pkg = read_package(&bytes).map_err(|e| e.to_string())?;
    let _one_at_a_time = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let id = store.install(&pkg).map_err(|e| e.to_string())?;
    Ok((id, pkg.family))
}

/// While a slider of the Axis panel is dragged, Regular is remade at most this often.
const DRAG_EVERY: Duration = Duration::from_millis(30);

/// The font an Axis slider is changing, held from the first move of a drag to its release.
struct DragTarget {
    /// `"default"` or the installed font's id.
    key: String,
    pkg: FontPackage,
    axes: AxisRanges,
    /// The values as saved, and as dragged to so far.
    saved: AxisValues,
    values: AxisValues,
    /// A Regular-only preview is showing in the Settings window.
    previewed: bool,
}

/// A slider drag in progress, and when Regular was last remade for it.
#[derive(Default)]
pub(super) struct AxisDrag {
    target: Option<DragTarget>,
    last_bake: Option<Duration>,
    later: slint::Timer,
}

/// The Face set of the Default font at its saved slant: the bundled faces themselves when it is
/// upright, else Shadows Into Light baked and sheared.
fn make_default(db: &Db) -> Result<ActiveFont, String> {
    let Some(slant) = db.default_font_slant().filter(|s| *s > 0.0) else {
        registry::deactivate();
        return Ok(ActiveFont::default_font());
    };
    let set = bake(&default_package(), AxisValues { slant: Some(slant), ..Default::default() }).map_err(|e: BakeError| e.to_string())?;
    registry::activate(&set);
    ActiveFont::from_face_set(&set).map_err(|_| "The font could not be prepared.".to_string())
}

/// Make the Face set of an installed font and register it with Slint.
fn make_font(store: &FontStore, row: &FontRow) -> Result<ActiveFont, String> {
    let pkg = store.load(&row.id).map_err(|e| e.to_string())?;
    let set = bake(&pkg, row.axes).map_err(|e: BakeError| e.to_string())?;
    registry::activate(&set);
    ActiveFont::from_face_set(&set).map_err(|_| "The font could not be prepared.".to_string())
}

/// Startup, before the first window: drop the rows whose folder is gone, and load the Font in use. A
/// font that is missing or broken falls back to the Default font and is reported once (the saved
/// choice goes back to the Default font, so the next start does not report it again).
pub(super) fn load_startup(env: &Env, db: &Db) {
    let store = FontStore::new(&env.data_dir);
    let rows = db.fonts();
    let active = db.font_active();
    let mut report = None;
    for row in &rows {
        if !store.exists(&row.id) {
            let _ = db.remove_font(&row.id);
            if row.id == active {
                report = Some(format!("The font \"{}\" could not be found, so Washi is using {DEFAULT_TEXT_FAMILY}.", row.family));
            }
        }
    }
    if active != "default" && report.is_none() {
        match rows.iter().find(|r| r.id == active) {
            Some(row) => match make_font(&store, row) {
                Ok(font) => {
                    crate::fonts::set_current(font);
                    return;
                }
                Err(e) => report = Some(format!("The font \"{}\" could not be loaded ({e}), so Washi is using {DEFAULT_TEXT_FAMILY}.", row.family)),
            },
            None => report = Some(format!("The font in use could not be found, so Washi is using {DEFAULT_TEXT_FAMILY}.")),
        }
    }
    // the Default font, at its saved slant
    if db.default_font_slant().is_some_and(|s| s > 0.0) {
        if let Ok(font) = make_default(db) {
            crate::fonts::set_current(font);
        }
    } else if crate::fonts::current().text_family != DEFAULT_TEXT_FAMILY {
        crate::fonts::set_current(ActiveFont::default_font());
        registry::deactivate();
    }
    if let Some(message) = report {
        let _ = db.set_font_active("default");
        env.dialogs.error(&message);
    }
}

impl App {
    fn font_store(&self) -> FontStore {
        FontStore::new(&self.0.env.data_dir)
    }

    /// Fonts being installed right now.
    pub fn pending_font_installs(&self) -> usize {
        self.0.font_installs.borrow().len()
    }

    /// "Install a font": ask for a `.zip` or `.ttf`, then [`App::install_font_file`].
    pub(super) fn pick_font_to_install(&self) {
        let Some(path) = self.0.env.dialogs.pick_open_file("Fonts|*.zip;*.ttf|All files|*.*") else { return };
        self.install_font_file(path);
    }

    /// Install the font package at `path` (a Google Fonts `.zip` or a `.ttf`): it is read and
    /// validated on a worker thread, kept in the fonts folder, and its row appears in the Font
    /// manager and scrolls into view. It does not become the Font in use. A refusal is shown in the
    /// error dialog with its reason. Dropping a file on the Font manager calls this too.
    pub fn install_font_file(&self, path: PathBuf) {
        let store = self.font_store();
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("font-install".into()).spawn(move || {
            let _ = tx.send(prepare(&path, &store));
        });
        if let Err(e) = spawned {
            return self.0.env.dialogs.error(&format!("Washi could not start installing the font: {e}"));
        }
        self.0.font_installs.borrow_mut().push(FontInstall { result: rx });
        if !self.0.font_timer.running() {
            let weak = Rc::downgrade(&self.0);
            self.0.font_timer.start(slint::TimerMode::Repeated, INSTALL_POLL, move || {
                if let Some(inner) = weak.upgrade() {
                    App(inner).collect_font_installs();
                }
            });
        }
    }

    /// Take in the fonts whose threads are done; stop polling when none are left.
    fn collect_font_installs(&self) {
        let mut done = Vec::new();
        self.0.font_installs.borrow_mut().retain(|i| match i.result.try_recv() {
            Ok(r) => {
                done.push(r);
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => {
                done.push(Err("The font could not be installed.".into()));
                false
            }
        });
        if self.0.font_installs.borrow().is_empty() {
            self.0.font_timer.stop();
        }
        for result in done {
            match result {
                Ok((id, family)) => self.font_installed(&id, &family),
                Err(message) => self.0.env.dialogs.error(&message),
            }
        }
    }

    fn font_installed(&self, id: &str, family: &str) {
        let now = self.0.env.clock.unix_millis();
        if let Err(e) = self.0.db.add_font(id, family, now) {
            // the files are there but nothing lists them: do not leave them behind
            let _ = self.font_store().remove(id);
            return self.0.env.dialogs.error(&format!("Washi could not save the font: {e}"));
        }
        // a replaced font: its old preview face and measured axes are stale
        registry::forget_preview(id);
        self.0.font_axes.borrow_mut().remove(id);
        if self.0.db.font_active() == id {
            // the Font in use was reinstalled: make its faces from the new files
            self.activate_installed(id);
        }
        self.refresh_font_list(Some(id));
    }

    /// Row `row` of the Font manager's list was pressed: make that font the Font in use.
    pub(super) fn pick_font(&self, row: usize) {
        if row == 0 {
            return self.activate_default();
        }
        let Some(font) = self.0.db.fonts().into_iter().nth(row - 1) else { return };
        if self.0.db.font_active() == font.id {
            return;
        }
        self.activate_installed(&font.id);
    }

    /// Make the installed font `id` the Font in use, in every window, and save the choice. A font
    /// that cannot be made is reported and nothing changes.
    fn activate_installed(&self, id: &str) {
        let Some(row) = self.0.db.fonts().into_iter().find(|f| f.id == id) else { return };
        let font = match make_font(&self.font_store(), &row) {
            Ok(font) => font,
            Err(e) => return self.0.env.dialogs.error(&format!("Washi could not use \"{}\": {e}", row.family)),
        };
        self.set_active_font(font);
        if let Err(e) = self.0.db.set_font_active(id) {
            self.0.env.dialogs.error(&format!("Washi could not save that setting: {e}"));
        }
        self.refresh_font_list(None);
    }

    /// Put the Default font (Shadows Into Light, bundled) back in use, in every window, and save it.
    fn activate_default(&self) {
        let already = self.0.db.font_active() == "default";
        let leaning = self.0.db.default_font_slant().is_some_and(|s| s > 0.0);
        if !already || (!leaning && crate::fonts::current().text_family != DEFAULT_TEXT_FAMILY) {
            let font = make_default(&self.0.db).unwrap_or_else(|_| {
                registry::deactivate();
                ActiveFont::default_font()
            });
            self.set_active_font(font);
        }
        if let Err(e) = self.0.db.set_font_active("default") {
            self.0.env.dialogs.error(&format!("Washi could not save that setting: {e}"));
        }
        self.refresh_font_list(None);
    }

    /// "Remove" on row `row`: delete the font's row and files. Removing the Font in use switches to
    /// the Default font first. The Default font cannot be removed.
    pub(super) fn remove_font(&self, row: usize) {
        if row == 0 {
            return;
        }
        let Some(font) = self.0.db.fonts().into_iter().nth(row - 1) else { return };
        if self.0.db.font_active() == font.id {
            self.activate_default();
        }
        if let Err(e) = self.0.db.remove_font(&font.id) {
            return self.0.env.dialogs.error(&format!("Washi could not remove the font: {e}"));
        }
        if let Err(e) = self.font_store().remove(&font.id) {
            self.0.env.dialogs.error(&format!("Washi could not delete the font's files: {e}"));
        }
        registry::forget_preview(&font.id);
        self.0.font_axes.borrow_mut().remove(&font.id);
        self.refresh_font_list(None);
    }

    /// The axes of an installed font, read once and remembered.
    fn axes_of(&self, id: &str) -> AxisRanges {
        if let Some(a) = self.0.font_axes.borrow().get(id) {
            return *a;
        }
        let axes = self.font_store().load(id).map(|p| p.axes).unwrap_or_default();
        self.0.font_axes.borrow_mut().insert(id.to_string(), axes);
        axes
    }

    /// The rows of the Font manager: the Default font, then the installed fonts by install time.
    fn font_rows(&self) -> (Vec<RowSpec>, String, String) {
        let fonts = self.0.db.fonts();
        let active = self.0.db.font_active();
        let active = if active == "default" || fonts.iter().any(|f| f.id == active) { active } else { "default".into() };
        let store = self.font_store();
        let mut rows = vec![manager::default_row(active == "default")];
        let (mut name, mut summary) = (rows[0].name.clone(), rows[0].subtitle.clone());
        if active == "default" {
            let values = AxisValues { slant: self.0.db.default_font_slant(), ..Default::default() };
            summary = manager::summary(&AxisRanges::default(), values);
            rows[0].axes = manager::axis_panel(&AxisRanges::default(), values);
            rows[0].extra = manager::PANEL_H;
        }
        for f in &fonts {
            let axes = self.axes_of(&f.id);
            let family = registry::preview(&f.id)
                .or_else(|| store.regular_bytes(&f.id).ok().map(|b| registry::preview_family(&f.id, &b)))
                .unwrap_or_else(|| DEFAULT_TEXT_FAMILY.into());
            let is_active = f.id == active;
            let row = RowSpec {
                name: f.family.clone(),
                family,
                subtitle: manager::subtitle(&axes),
                active: is_active,
                locked: false,
                tag: "Remove".into(),
                extra: if is_active { manager::PANEL_H } else { 0.0 },
                axes: if is_active { manager::axis_panel(&axes, f.axes) } else { Vec::new() },
            };
            if is_active {
                name = f.family.clone();
                summary = manager::summary(&axes, f.axes);
                rows[0].active = false;
            }
            rows.push(row);
        }
        (rows, name, summary)
    }

    /// Redraw the Font manager's list and the Font row from the database, and scroll the list so
    /// that the font `show` (if any) is in view. Does nothing while Settings is closed.
    pub(super) fn refresh_font_list(&self, show: Option<&str>) {
        let Some(settings) = self.settings() else { return };
        let (rows, name, summary) = self.font_rows();
        let shown = show.and_then(|id| self.0.db.fonts().iter().position(|f| f.id == id)).map(|i| i + 1);
        manager::show(settings.ui(), &rows, &name, &summary, shown);
    }

    /// The font the Axis sliders change: the Font in use, its package, ranges and saved values.
    fn drag_target(&self) -> Option<DragTarget> {
        let active = self.0.db.font_active();
        let (pkg, saved) = if active == "default" {
            (default_package(), AxisValues { slant: self.0.db.default_font_slant(), ..Default::default() })
        } else {
            let row = self.0.db.fonts().into_iter().find(|f| f.id == active)?;
            (self.font_store().load(&row.id).ok()?, row.axes)
        };
        Some(DragTarget { key: active, axes: pkg.axes, pkg, saved, values: saved, previewed: false })
    }

    /// Put the font's Axis values on the Font row's summary.
    fn show_summary(&self, axes: &AxisRanges, values: AxisValues) {
        if let Some(settings) = self.settings() {
            settings.ui().set_font_summary(manager::summary(axes, values).into());
        }
    }

    /// An Axis slider is being dragged to `value` (`axis`: 0 Weight, 1 Width, 2 Slant). Only Regular
    /// is remade, at most once per 30 ms, and only the Settings window follows; the Font in use
    /// and every other window wait for the release (`axis_committed`).
    pub(super) fn axis_moved(&self, axis: i32, value: i32) {
        let now = self.0.env.clock.monotonic();
        let due = {
            let mut drag = self.0.axis_drag.borrow_mut();
            if drag.target.as_ref().is_none_or(|t| t.key != self.0.db.font_active()) {
                drag.target = self.drag_target();
            }
            let AxisDrag { target, last_bake, later } = &mut *drag;
            let Some(t) = target.as_mut() else { return };
            let Some(values) = manager::with_axis(&t.axes, t.values, axis, value) else { return };
            t.values = values;
            self.show_summary(&t.axes, values);
            let wait = last_bake.map_or(Duration::ZERO, |last| DRAG_EVERY.saturating_sub(now.saturating_sub(last)));
            if wait.is_zero() {
                later.stop();
                true
            } else {
                if !later.running() {
                    let weak = Rc::downgrade(&self.0);
                    later.start(slint::TimerMode::SingleShot, wait, move || {
                        if let Some(inner) = weak.upgrade() {
                            App(inner).drag_preview();
                        }
                    });
                }
                false
            }
        };
        if due {
            self.drag_preview();
        }
    }

    /// Remake Regular at the values dragged to, and show it in the Settings window alone.
    fn drag_preview(&self) {
        let now = self.0.env.clock.monotonic();
        let (family, metrics) = {
            let mut drag = self.0.axis_drag.borrow_mut();
            let Some(t) = drag.target.as_mut() else { return };
            let Ok(face) = bake_regular(&t.pkg, t.values) else { return };
            t.previewed = true;
            let family = registry::drag_face(&face);
            drag.last_bake = Some(now);
            (family, ThemeMetrics::from_engine(&face.metrics))
        };
        if let Some(settings) = self.settings() {
            apply_regular_preview(&settings.ui().global::<crate::note_window::Theme>(), &family, &metrics);
        }
    }

    /// An Axis slider was let go at `value` (or a number was typed in its box): clamp it into the
    /// font's range, save it for that font, make the full Face set and update every window.
    pub(super) fn axis_committed(&self, axis: i32, value: i32) {
        let (key, axes, saved, values, previewed) = {
            let mut drag = self.0.axis_drag.borrow_mut();
            drag.later.stop();
            if drag.target.as_ref().is_none_or(|t| t.key != self.0.db.font_active()) {
                drag.target = self.drag_target();
            }
            let Some(t) = drag.target.as_mut() else { return };
            if let Some(values) = manager::with_axis(&t.axes, t.values, axis, value) {
                t.values = values;
            }
            (t.key.clone(), t.axes, t.saved, t.values, t.previewed)
        };
        let same = manager::axis_panel(&axes, saved) == manager::axis_panel(&axes, values);
        if same && !previewed {
            self.0.axis_drag.borrow_mut().target = None;
            return;
        }
        let saving = if key == "default" {
            self.0.db.set_default_font_slant(values.slant.filter(|s| *s > 0.0))
        } else {
            self.0.db.set_font_axes(&key, values)
        };
        if let Err(e) = saving {
            self.0.env.dialogs.error(&format!("Washi could not save that setting: {e}"));
        }
        let made = if key == "default" {
            make_default(&self.0.db)
        } else {
            match self.0.db.fonts().into_iter().find(|f| f.id == key) {
                Some(row) => make_font(&self.font_store(), &row),
                None => Err("The font is gone.".into()),
            }
        };
        *self.0.axis_drag.borrow_mut() = AxisDrag::default();
        registry::end_drag();
        match made {
            Ok(font) => self.set_active_font(font),
            Err(e) => self.0.env.dialogs.error(&format!("Washi could not use that font: {e}")),
        }
        self.show_summary(&axes, values);
    }

    /// The axes ranges of the Font in use, if it is an installed font.
    pub fn font_in_use(&self) -> Option<(FontRow, AxisRanges)> {
        let id = self.0.db.font_active();
        let row = self.0.db.fonts().into_iter().find(|f| f.id == id)?;
        let axes = self.axes_of(&row.id);
        Some((row, axes))
    }
}

/// The axes of every installed font read so far, by id.
pub(super) type AxesCache = HashMap<String, AxisRanges>;
