//! Binding the editor to a note window: every Slint callback the editor answers.

use super::*;

impl Editor {
    /// Bind this editor engine to a Slint NoteWindow component, wiring all models and callbacks.
    pub fn bind_ui(
        self: &Rc<Self>,
        ui: &NoteWindow,
        on_changed: Rc<dyn Fn(Option<Caret>)>,
        on_pointer_down: Option<Rc<dyn Fn()>>,
        on_open_image: Option<Rc<dyn Fn(usize)>>,
        on_paste_image: Option<Rc<dyn Fn() -> bool>>,
    ) {
        *self.ui.borrow_mut() = Some(ui.as_weak());
        *self.on_changed.borrow_mut() = Some(on_changed.clone());

        // Initialize PickerPalette entries
        let entries: Vec<PaletteEntry> = PALETTE
            .iter()
            .map(|s| PaletteEntry {
                name: s.name.into(),
                ink: rgb(s.ink),
                marker: swatch_marker(s.marker),
            })
            .collect();
        ui.global::<PickerPalette>().set_entries(slint::ModelRc::from(Rc::new(VecModel::from(entries))));

        // Attach Slint VecModels
        ui.set_blocks(slint::ModelRc::from(self.model.clone()));
        ui.set_runs(slint::ModelRc::from(self.runs.clone()));
        ui.set_rules(slint::ModelRc::from(self.rules.clone()));
        ui.set_checks(slint::ModelRc::from(self.checks.clone()));
        ui.set_sel_rects(slint::ModelRc::from(self.sel_rects.clone()));
        ui.set_image_handles(slint::ModelRc::from(self.handles.clone()));
        ui.set_markers(slint::ModelRc::from(self.marker_data.clone()));
        ui.set_marker_wash(slint::ModelRc::from(self.marker_wash.clone()));

        self.tilted.set(self.db.tilted_editing());
        ui.set_tilted_editing(self.tilted.get());
        ui.global::<Theme>().set_letter_spacing_em(self.db.letter_spacing().em());
        self.relayout(ui);
        self.show_formats(ui);
        ui.set_text(self.text().as_str().into());

        // Helper closure to trigger change
        let editor_weak = Rc::downgrade(self);
        let changed = {
            let weak_ui = ui.as_weak();
            let editor_weak = editor_weak.clone();
            let on_changed = on_changed.clone();
            Rc::new(move |focus: Option<Caret>| {
                if let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) {
                    editor.apply_change(&ui, focus);
                }
                on_changed(focus);
            })
        };

        // Wire on_block_edited
        {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            ui.on_block_edited(move |i, block_text, cursor| {
                let Some(editor) = editor_weak.upgrade() else { return };
                let caret = editor.edit(i.max(0) as usize, block_text.as_str(), cursor.max(0) as usize);
                if caret.is_none() {
                    // typing: snapshot when it pauses
                    let weak_editor = editor_weak.clone();
                    editor.history_timer.start(TimerMode::SingleShot, HISTORY_PAUSE, move || {
                        if let Some(ed) = weak_editor.upgrade() {
                            ed.checkpoint();
                        }
                    });
                }
                changed(caret);
            });
        }

        // Wire on_format
        {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            ui.on_format(move |what| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                if editor.format(what) {
                    changed(None);
                    if what == FORMAT_CHECK || what == FORMAT_LIST {
                        // the paragraph's indent changed, so it rewrapped: keep the caret in view
                        editor.reveal_caret(&ui);
                    }
                } else {
                    editor.show_formats(&ui);
                }
            });
        }

        // Wire color popover actions: apply ink / marker
        let apply: Rc<dyn Fn(Option<u8>, Option<u8>)> = {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            Rc::new(move |ink, marker| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                if editor.apply_paint(ink, marker) {
                    changed(None);
                } else {
                    editor.show_formats(&ui);
                }
            })
        };
        {
            let apply = apply.clone();
            ui.on_pick_ink(move |i| apply(Some(i.clamp(0, PALETTE_MAX as i32) as u8), None));
        }
        {
            let apply = apply.clone();
            ui.on_pick_marker(move |i| apply(None, Some(i.clamp(0, PALETTE_MAX as i32) as u8)));
        }
        {
            let apply = apply.clone();
            ui.on_clear_color(move || apply(Some(0), Some(0)));
        }

        // Wire on_focus_entered
        {
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            ui.on_focus_entered(move |i| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                let block = i.max(0) as usize;
                if editor.selection.get().focus.block != block {
                    editor.set_caret(Caret { block, offset: 0 });
                }
                editor.show_formats(&ui);
            });
        }

        // Wire on_block_key
        {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            ui.on_block_key(move |i, key, mods, offset| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return false };
                let (block, offset) = (i.max(0) as usize, offset.max(0) as usize);
                if editor.try_move(&ui, key.as_str(), &mods, block, offset) {
                    return true;
                }
                if mods.control || mods.selecting {
                    // Ctrl+Backspace and the like, or a selection inside the field: the field's own
                    return false;
                }
                let Some(caret) = editor.edit_key(block, key.as_str(), offset) else { return false };
                changed(Some(caret));
                true
            });
        }

        // Wire on_block_history (Ctrl+Z, Ctrl+Y from a field)
        {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            ui.on_block_history(move |redo| {
                let Some(editor) = editor_weak.upgrade() else { return };
                // a step that changed only kinds (a tick, a toggle) leaves the selection alone
                if let Some(caret) = editor.history_step(redo) {
                    changed(caret);
                }
            });
        }

        // Wire on_select_all
        {
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            ui.on_select_all(move || {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                editor.select_all(&ui);
            });
        }

        // Wire on_range_text
        {
            let editor_weak = editor_weak.clone();
            ui.on_range_text(move || {
                editor_weak.upgrade().map(|e| e.range_text().into()).unwrap_or_default()
            });
        }

        // Wire on_copy_range
        {
            let editor_weak = editor_weak.clone();
            ui.on_copy_range(move || {
                let Some(editor) = editor_weak.upgrade() else { return false };
                editor.copy_range(&editor.clipboard)
            });
        }

        // Wire on_cut_range
        {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            ui.on_cut_range(move || {
                let Some(editor) = editor_weak.upgrade() else { return false };
                if let Some(caret) = editor.cut_range(&editor.clipboard) {
                    changed(Some(caret));
                    true
                } else {
                    false
                }
            });
        }

        // Wire on_range_replace
        {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            ui.on_range_replace(move |text| {
                let Some(editor) = editor_weak.upgrade() else { return };
                let caret = editor.replace_range(text.as_str());
                changed(Some(caret));
            });
        }

        // Wire on_range_key
        {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            ui.on_range_key(move |key| {
                let Some(editor) = editor_weak.upgrade() else { return 0 };
                let (taken, caret) = editor.range_key(key.as_str());
                if let Some(c) = caret {
                    changed(Some(c));
                }
                taken
            });
        }

        // Wire on_caret_moved
        {
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            ui.on_caret_moved(move |block, offset, anchor| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                let (block, offset) = (block.max(0) as usize, offset.max(0) as usize);
                editor.caret_moved(&ui, block, offset, anchor.max(0) as usize);
                editor.show_formats(&ui);
                editor.reveal(&ui, block, offset);
            });
        }

        // Wire on_paper_resized
        {
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            let laid_out = Cell::new(None);
            ui.on_paper_resized(move |w, h| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                // A corner drag changes the width and the height at once and each reports the new
                // paper: lay the words out once.
                let paper = (w as f64, h as f64);
                if laid_out.replace(Some(paper)) == Some(paper) && editor.paper.get() == paper {
                    return;
                }
                editor.paper.set(paper);
                editor.relayout(&ui);
                // the words rewrapped and the view changed height: keep the caret in view, but leave
                // a selection being dragged to the drag (it scrolls the note itself)
                if !editor.pointer.get().down {
                    editor.reveal_caret(&ui);
                }
            });
        }

        // Wire on_text_right_click
        {
            let editor_weak = editor_weak.clone();
            ui.on_text_right_click(move |x, y| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.right_click.set(Some((x, y)));
                }
            });
        }

        // Wire on_pointer_down
        {
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            let on_pointer_down = on_pointer_down.clone();
            let on_open_image = on_open_image.clone();
            let changed = changed.clone();
            ui.on_pointer_down(move |x, y, shift| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                if let Some(cb) = on_pointer_down.as_ref() {
                    cb();
                }
                if editor.grab_grip(&ui, (x, y)) {
                    return;
                }
                if editor.toggle_check_at(&ui, (x, y)) {
                    changed(None);
                    return;
                }
                let now = editor.monotonic_time();
                editor.pointer_down(&ui, (x, y), shift, now);
                if editor.pointer.get().clicks == 2 {
                    if let Some(block) = editor.image_at(&ui, (x, y)) {
                        if let Some(cb) = on_open_image.as_ref() {
                            cb(block);
                        }
                    }
                }
            });
        }

        // Wire on_pointer_drag
        {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            let autoscroll = self.autoscroll.clone();
            ui.on_pointer_drag(move |x, y| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                if let Some(resized) = editor.grip_drag(&ui, (x, y)) {
                    if resized {
                        changed(None);
                    }
                    return;
                }
                if editor.drag_to(&ui, (x, y)) {
                    if !autoscroll.running() {
                        let weak_editor = editor_weak.clone();
                        let weak_ui = weak_ui.clone();
                        let timer_weak = Rc::downgrade(&autoscroll);
                        autoscroll.start(TimerMode::Repeated, AUTOSCROLL_STEP, move || {
                            let more = match (weak_editor.upgrade(), weak_ui.upgrade()) {
                                (Some(ed), Some(u)) => ed.autoscroll(&u),
                                _ => false,
                            };
                            if !more {
                                if let Some(t) = timer_weak.upgrade() {
                                    t.stop();
                                }
                            }
                        });
                    }
                } else {
                    autoscroll.stop();
                }
            });
        }

        // Wire on_pointer_up
        {
            let editor_weak = editor_weak.clone();
            let autoscroll = self.autoscroll.clone();
            ui.on_pointer_up(move || {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.release_grip();
                    editor.pointer_up();
                }
                autoscroll.stop();
            });
        }

        // Wire on_pointer_motion
        {
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            ui.on_pointer_motion(move |x, y| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                let kind = editor.cursor_kind_at(&ui, (x, y));
                if ui.get_text_cursor_kind() != kind {
                    ui.set_text_cursor_kind(kind);
                }
            });
        }

        // Paste clipboard handler
        let paste_clipboard_fn = {
            let editor_weak = editor_weak.clone();
            let changed = changed.clone();
            Rc::new(move || -> bool {
                let Some(editor) = editor_weak.upgrade() else { return false };
                if editor.paste_rich_or_image() {
                    return true;
                }
                if let Some(text) = editor.clipboard.text() {
                    if text.contains('\n') || text.contains('\r') || editor.has_range() {
                        let caret = editor.replace_range(&text);
                        changed(Some(caret));
                        return true;
                    }
                }
                false
            })
        };

        ui.on_paste_range({
            let paste_clipboard_fn = paste_clipboard_fn.clone();
            move || paste_clipboard_fn()
        });

        {
            let paste_clipboard_fn = paste_clipboard_fn.clone();
            let on_paste_image = on_paste_image.clone();
            ui.on_paste_image(move || {
                if paste_clipboard_fn() {
                    return true;
                }
                if let Some(cb) = on_paste_image.as_ref() {
                    return cb();
                }
                false
            });
        }

        // Wire on_copy_image
        {
            let editor_weak = editor_weak.clone();
            ui.on_copy_image(move |i| {
                let Some(editor) = editor_weak.upgrade() else { return false };
                editor.copy_image(i.max(0) as usize)
            });
        }

        // Wire on_cut_image
        {
            let editor_weak = editor_weak.clone();
            ui.on_cut_image(move |i| {
                let Some(editor) = editor_weak.upgrade() else { return false };
                editor.cut_image(i.max(0) as usize).is_some()
            });
        }
    }
}
