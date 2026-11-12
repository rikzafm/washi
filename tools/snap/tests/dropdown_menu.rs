//! Tests for dropdown menu actions and state synchronization through the app harness.

use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn dropdown_pin_toggle_reflects_and_updates_note_always_on_top() {
    let mut h = Harness::new();
    h.start().unwrap();

    // Note 0 starts unpinned
    assert!(!h.always_on_top(0));

    // Open dropdown for note 0
    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());
    assert_eq!(h.dropdown_pinned(), Some(false));

    // Toggle pin from dropdown
    h.invoke_dropdown_toggle_pin();
    assert!(h.always_on_top(0));
    assert_eq!(h.dropdown_pinned(), Some(true));

    // Toggle pin again
    h.invoke_dropdown_toggle_pin();
    assert!(!h.always_on_top(0));
    assert_eq!(h.dropdown_pinned(), Some(false));
}

#[test]
fn dropdown_delete_note_dismisses_and_deletes_current_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.new_note_from(0);
    assert_eq!(h.open_note_count(), 2);

    let id1 = h.note_id(1);

    // Open dropdown on note 1
    h.toggle_dropdown(1);
    assert!(h.is_dropdown_open());
    assert_eq!(h.dropdown_note_id(), Some(id1));

    // Delete note from dropdown
    h.invoke_dropdown_delete_note();

    // Dropdown should be dismissed and delete confirmation dialog open
    assert!(!h.is_dropdown_open());
    assert!(h.is_delete_dialog_open());

    // Confirm deletion
    h.invoke_confirm_delete();
    assert_eq!(h.open_note_count(), 1);

    // Verify deleted note is gone from recent notes list in DB
    h.toggle_dropdown(0);
    let items = h.dropdown_notes();
    assert!(!items.iter().any(|item| item.id == id1 as i32));
}

#[test]
fn dropdown_close_note_dismisses_and_closes_current_note_without_deleting_it() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.new_note_from(0);
    assert_eq!(h.open_note_count(), 2);
    let id1 = h.note_id(1);
    h.set_note_text(1, "# Keep me");

    h.toggle_dropdown(1);
    assert!(h.is_dropdown_open());
    h.invoke_dropdown_close_note();

    assert!(!h.is_dropdown_open());
    assert!(!h.is_delete_dialog_open());
    assert_eq!(h.open_note_count(), 1);

    // the closed note is kept: it is listed in the other note's menu
    h.toggle_dropdown(0);
    assert!(h.dropdown_notes().iter().any(|item| item.id == id1 as i32));
}

#[test]
fn dropdown_new_note_dismisses_and_creates_new_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    assert_eq!(h.open_note_count(), 1);

    // Open dropdown on note 0
    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());

    // Click new note in dropdown
    h.invoke_dropdown_new_note();

    // Dropdown should be dismissed and a new note opened
    assert!(!h.is_dropdown_open());
    assert_eq!(h.open_note_count(), 2);
}

#[test]
fn dropdown_populates_notes_list_excluding_active_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.new_note_from(0);
    assert_eq!(h.open_note_count(), 2);

    let id1 = h.note_id(1);

    h.set_note_text(1, "Second Note Title\nPreview line for note 1");

    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());

    let items = h.dropdown_notes();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, id1 as i32);
    assert_eq!(items[0].title.as_str(), "Second Note Title");
    // Long snippets are cut at 22 characters, like the design.
    assert_eq!(items[0].preview.as_str(), "Preview line for note…");
    assert_eq!(items[0].timestamp.as_str(), "· today");
    // Note 1 has a window on the desktop, so its row carries the tape marker.
    assert!(items[0].has_tape);
}

#[test]
fn dropdown_select_open_note_raises_to_front_and_dismisses() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.new_note_from(0);
    let id1 = h.note_id(1);

    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());

    h.control.log.borrow_mut().clear();

    h.invoke_dropdown_select_note(id1);

    assert!(!h.is_dropdown_open());

    let logs = h.control.log.borrow().clone();
    assert!(logs.contains(&snap::WindowCommand::BringToFront(id1)));
}

#[test]
fn dropdown_select_closed_note_reopens_and_dismisses() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.new_note_from(0);
    assert_eq!(h.open_note_count(), 2);
    let id1 = h.note_id(1);
    h.set_note_text(1, "Closed Note\nWill be reopened");

    h.close_note(1);
    assert_eq!(h.open_note_count(), 1);

    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());

    let items = h.dropdown_notes();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, id1 as i32);
    assert_eq!(items[0].title.as_str(), "Closed Note");

    h.invoke_dropdown_select_note(id1);

    assert!(!h.is_dropdown_open());
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.note_id(1), id1);
}

#[test]
fn menu_button_hover_fade_and_persists_while_dropdown_open() {
    let mut h = Harness::new();
    h.start().unwrap();

    let (bx, by, bw, bh) = h.menu_button_rect(0);
    assert_eq!((bw, bh), (28.0, 24.0));
    let (nw, nh) = h.note_size(0);
    // Button is located in top-right corner of note paper
    assert!(bx > nw as f32 / 2.0, "menu button x should be on right half: {bx} vs {nw}");
    assert!(by < nh as f32 / 2.0, "menu button y should be on top half: {by} vs {nh}");

    // Initially hidden
    assert_eq!(h.menu_button_opacity(0), 0.0);
    assert!(!h.menu_button_shown(0));

    // Pointer enters note paper
    h.pointer_move(0, 5.0, 5.0);
    h.advance(ms(0));
    h.advance(ms(60));
    let half = h.menu_button_opacity(0);
    assert!(half > 0.0 && half < 1.0, "halfway through fade: {half}");
    assert!(h.menu_button_shown(0));
    h.advance(ms(60));
    assert_eq!(h.menu_button_opacity(0), 1.0);

    // Pointer leaves note paper -> button fades out
    h.pointer_exit(0);
    h.advance(ms(0));
    h.advance(ms(60));
    assert!(h.menu_button_opacity(0) < 1.0);
    h.advance(ms(110));
    assert_eq!(h.menu_button_opacity(0), 0.0);
    assert!(!h.menu_button_shown(0));

    // Open dropdown for note 0
    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());

    // Pointer is outside note, but button remains visible because dropdown is open
    h.pointer_exit(0);
    h.advance(ms(300));
    assert_eq!(h.menu_button_opacity(0), 1.0, "button must remain visible while dropdown is open");
    assert!(h.menu_button_shown(0));

    // When dropdown closes and pointer is outside, button fades out
    h.invoke_dropdown_dismiss();
    assert!(!h.is_dropdown_open());
    h.advance(ms(0));
    h.advance(ms(170));
    assert_eq!(h.menu_button_opacity(0), 0.0);
    assert!(!h.menu_button_shown(0));
}

#[test]
fn menu_button_click_toggles_dropdown_open_and_close() {
    let mut h = Harness::new();
    h.start().unwrap();

    assert!(!h.is_dropdown_open());

    // Hover to reveal button, then click it
    h.click_menu_button(0);
    assert!(h.is_dropdown_open(), "clicking menu button opens dropdown");

    // Click again to toggle close
    h.click_menu_button(0);
    assert!(!h.is_dropdown_open(), "clicking menu button again closes dropdown");
}

#[test]
fn dropdown_window_geometry_positioning_and_rendering() {
    let mut h = Harness::new();
    h.start().unwrap();

    // Open dropdown
    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());

    // Check dropdown window size (card-width + 2*shadow-margin, card-height + 2*shadow-margin)
    let size = h.dropdown_size().expect("dropdown size");
    assert_eq!(size, (300.0, 352.0));

    // Check dropdown window position relative to note's menu button
    let (bx, by, _bw, bh) = h.menu_button_rect(0);
    let (win_x, win_y) = h.dropdown_position().expect("dropdown position");

    // The card sits at (win_x + shadow_margin, win_y + shadow_margin).
    // In our positioning logic matching mockup:
    // card left starts ~14px to left of button: card_x = btn_screen_x - 14.0
    // And card top is below button by gap: card_y = btn_screen_y + bh + gap
    let note_geom = h.note_geometry(0);
    let btn_screen_x = note_geom.x as f32 + bx;
    let btn_screen_y = note_geom.y as f32 + by;

    let metrics = washi::dropdown::DropdownMetrics::default();
    let card_x = win_x + metrics.shadow_margin;
    let card_y = win_y + metrics.shadow_margin;

    assert!((card_x - (btn_screen_x - 14.0)).abs() <= 2.0, "card left is offset ~14px from button left");
    assert!((card_y - (btn_screen_y + bh + metrics.gap)).abs() <= 2.0, "card top is gap below button");

    // Verify software rendering of dropdown window
    let (w, h_px, pixels) = h.render_dropdown().expect("render dropdown");
    assert_eq!((w, h_px), (300, 352));

    // Corner (0,0) in the shadow margin is transparent/empty default
    assert_eq!(pixels[0], slint::Rgb8Pixel::default());

    // Inside the card body (e.g. x=150, y=40), pixels are predominantly white (#FFFFFF)
    let card_pixel = pixels[(40 * w + 150) as usize];
    assert!(card_pixel.r >= 250 && card_pixel.g >= 250 && card_pixel.b >= 250, "card background is white: {:?}", card_pixel);

    // Header and icons contribute non-white pixels
    let non_white_count = pixels.iter().filter(|p| p.r < 220 || p.g < 220 || p.b < 220).count();
    assert!(non_white_count > 100, "rendered dropdown card contains header and action icons");
}

#[test]
fn dropdown_dismissal_on_escape_key() {
    let mut h = Harness::new();
    h.start().unwrap();

    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());

    // Press Escape key
    h.press_dropdown_escape();

    // Dropdown should be dismissed and note state synchronized
    assert!(!h.is_dropdown_open());
    assert!(!h.is_note_dropdown_open(0));
}

#[test]
fn dropdown_dismissal_on_outside_click() {
    let mut h = Harness::new();
    h.start().unwrap();

    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());

    // Click outside card in the shadow margin (e.g. 5, 5)
    h.click_dropdown_outside();

    // Dropdown should be dismissed
    assert!(!h.is_dropdown_open());
    assert!(!h.is_note_dropdown_open(0));
}

#[test]
fn dropdown_dismissal_on_parent_move_and_resize() {
    let mut h = Harness::new();
    h.start().unwrap();

    // 1. Dismiss on parent move
    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());

    // Parent note geometry changes (user moves the note)
    h.notify_geometry_changed(0);
    assert!(!h.is_dropdown_open(), "moving parent note dismisses dropdown");

    // 2. Dismiss on parent resize
    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());

    // Parent note geometry changes (user resizes the note)
    h.notify_geometry_changed(0);
    assert!(!h.is_dropdown_open(), "resizing parent note dismisses dropdown");
}

#[test]
fn snap_render_verifies_menu_button_appearance_on_hover() {
    let mut h = Harness::new();
    h.start().unwrap();

    let (bx, by, bw, bh) = h.menu_button_rect(0);
    let (x0, y0) = (bx as usize, by as usize);
    let (x1, y1) = ((bx + bw) as usize, (by + bh) as usize);

    // Initial render without hover
    let (w, _, idle) = h.render(0);

    // Move pointer over menu button and allow fade animation to complete
    h.pointer_move(0, bx + bw / 2.0, by + bh / 2.0);
    h.advance(ms(300));
    let (_, _, hover) = h.render(0);

    // Collect changed pixels within the menu button bounds
    let changed_button_pixels = (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| y * w as usize + x))
        .filter(|&idx| idle[idx] != hover[idx])
        .count();

    assert!(changed_button_pixels > 0, "hovering menu button draws button graphics (three dots)");
}

#[test]
fn dropdown_dismissal_on_note_canvas_interaction() {
    let mut h = Harness::new();
    h.start().unwrap();

    // Open dropdown on note 0
    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());

    // Click on note canvas (e.g. at 100, 100)
    h.click(0, 100.0, 100.0);

    // Dropdown should be dismissed
    assert!(!h.is_dropdown_open(), "clicking note canvas dismisses dropdown");
    assert!(!h.is_note_dropdown_open(0));
}

#[test]
fn dropdown_new_note_spawns_near_origin_note_with_multiple_notes() {
    let mut h = Harness::new();
    h.start().unwrap();

    // Create a second note offset from note 0
    h.new_note_from(0);
    assert_eq!(h.open_note_count(), 2);

    let g0 = h.note_geometry(0);
    let g1 = h.note_geometry(1);
    assert_ne!((g0.x, g0.y), (g1.x, g1.y));

    // Open dropdown on note 0 (which is not the last note in notes list)
    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());
    assert_eq!(h.dropdown_note_id(), Some(h.note_id(0)));

    // Click "+ New" from dropdown
    h.invoke_dropdown_new_note();

    // Dropdown should be dismissed and note 2 created
    assert!(!h.is_dropdown_open());
    assert_eq!(h.open_note_count(), 3);

    let g2 = h.note_geometry(2);
    // Note 2 should be offset from note 0, not note 1
    assert_eq!(g2.x, g0.x + washi::NEW_NOTE_OFFSET);
    assert_eq!(g2.y, g0.y + washi::NEW_NOTE_OFFSET);
}

#[test]
fn dropdown_all_notes_dismisses_dropdown_opens_all_notes_and_closes_on_escape() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, "First Note\nBody 1");
    h.new_note_from(0);
    h.set_note_text(1, "Second Note\nBody 2");
    h.new_note_from(1);
    h.set_note_text(2, "Third Note\nBody 3");
    h.close_note(2);

    assert_eq!(h.open_note_count(), 2);
    let id1 = h.note_id(1);

    h.toggle_dropdown(1);
    assert!(h.is_dropdown_open());
    assert!(!h.is_all_notes_open());

    h.invoke_dropdown_all_notes();

    assert!(!h.is_dropdown_open(), "clicking All notes dismisses dropdown");
    assert!(h.is_all_notes_open(), "clicking All notes opens AllNotesWindow");
    assert_eq!(h.all_notes_selected_id(), Some(id1));
    assert_eq!(h.all_notes_open_count(), Some(2));
    assert_eq!(h.all_notes_closed_count(), Some(1));

    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open(), "pressing Escape closes AllNotesWindow");
}

#[test]
fn all_notes_sidebar_selection_and_new_note_button() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, "Endfield Note\nThermite Cutter");
    h.new_note_from(0);
    h.set_note_text(1, "Grocery run\nMilk");
    h.new_note_from(1);
    h.set_note_text(2, "Meeting ideas\nRoadmap");
    let closed_id = h.note_id(2);
    h.close_note(2);

    let id0 = h.note_id(0);
    let id1 = h.note_id(1);
    h.set_stacking(vec![id1, id0]);

    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());
    assert_eq!(h.all_notes_selected_id(), Some(id0));
    assert_eq!(
        h.all_notes_status_text().as_deref(),
        Some("Open on your desktop · edited today")
    );
    let open_preview = h.all_notes_editor_text().unwrap();
    assert!(open_preview.contains("Endfield Note"));
    assert!(open_preview.contains("Thermite Cutter"));

    let open_items = h.all_notes_open_items();
    let closed_items = h.all_notes_closed_items();
    assert_eq!(open_items.len(), 2);
    assert_eq!(open_items[0].title.as_str(), "Endfield Note");
    assert_eq!(open_items[1].title.as_str(), "Grocery run");
    assert_eq!(closed_items.len(), 1);
    assert_eq!(closed_items[0].title.as_str(), "Meeting ideas");

    // Selecting a closed note updates selection, status text, preview runs, and resets confirming-delete
    h.set_all_notes_confirming_delete(true);
    assert_eq!(h.all_notes_confirming_delete(), Some(true));
    h.invoke_all_notes_select_note(closed_id);
    assert_eq!(h.all_notes_selected_id(), Some(closed_id));
    assert_eq!(h.all_notes_confirming_delete(), Some(false));
    assert_eq!(
        h.all_notes_status_text().as_deref(),
        Some("Closed · edited today")
    );
    let closed_preview = h.all_notes_editor_text().unwrap();
    assert!(closed_preview.contains("Meeting ideas"));
    assert!(closed_preview.contains("Roadmap"));

    // Clicking "+ New note" creates a new note on the desktop and refreshes All Notes
    h.set_all_notes_confirming_delete(true);
    h.invoke_all_notes_new_note();
    assert_eq!(h.open_note_count(), 3);
    assert_eq!(h.all_notes_open_count(), Some(3));
    let new_id = h.note_id(2);
    assert_eq!(h.all_notes_selected_id(), Some(new_id));
    assert_eq!(h.all_notes_confirming_delete(), Some(false));

    // The editor window is the note's window plus the sidebar's strip on the left (once it has slid in)
    h.advance(ms(300));
    let (w, h_px, pixels) = h.render_all_notes().expect("render all notes");
    assert_eq!((w, h_px), (447 + 282, 468));
    // the sidebar's slate backing at the middle of its left edge area
    let backing = pixels[(240 * w + 14) as usize];
    assert_eq!((backing.r, backing.g, backing.b), (214, 219, 226));
    // the paper, right of the strip: the middle of the paper is #FFFFFD
    let paper = pixels[(250 * w + 282 + 200) as usize];
    assert!(paper.r >= 252 && paper.g >= 252 && paper.b >= 250);
}

#[test]
fn all_notes_bottom_actions_open_close_export_and_delete_confirmation() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, "# Endfield Note\nThermite Cutter");
    h.new_note_from(0);
    h.set_note_text(1, "# Grocery run\nMilk and eggs");
    h.new_note_from(1);
    h.set_note_text(2, "# Meeting ideas\nQ4 roadmap");
    let closed_id = h.note_id(2);
    h.close_note(2);

    let id0 = h.note_id(0);
    let id1 = h.note_id(1);
    h.set_stacking(vec![id1, id0]);

    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());
    assert_eq!(h.all_notes_selected_id(), Some(id0));
    assert_eq!(h.open_note_count(), 2);

    // 1. Open/Focus on an already-open note brings it to front
    let log_len_before = h.control.log.borrow().len();
    h.invoke_all_notes_open_or_focus();
    let log = h.control.log.borrow();
    assert!(
        log[log_len_before..].contains(&snap::WindowCommand::BringToFront(id0)),
        "Open/Focus on open note should bring it to front"
    );
    drop(log);

    // 2. Open/Focus on a closed note reopens it onto the desktop and updates sidebar & status
    h.invoke_all_notes_select_note(closed_id);
    assert_eq!(h.all_notes_status_text().as_deref(), Some("Closed · edited today"));
    h.invoke_all_notes_open_or_focus();
    assert_eq!(h.open_note_count(), 3);
    assert_eq!(h.all_notes_open_count(), Some(3));
    assert_eq!(h.all_notes_closed_count(), Some(0));
    assert_eq!(h.all_notes_selected_id(), Some(closed_id));
    assert_eq!(
        h.all_notes_status_text().as_deref(),
        Some("Open on your desktop · edited today")
    );

    // 3. Close (×) closes the selected note's desktop window and moves it to CLOSED
    h.invoke_all_notes_close_note();
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.all_notes_open_count(), Some(2));
    assert_eq!(h.all_notes_closed_count(), Some(1));
    assert_eq!(h.all_notes_selected_id(), Some(closed_id));
    assert_eq!(
        h.all_notes_status_text().as_deref(),
        Some("Closed · edited today")
    );
    assert!(!h.has_quit());

    // 4. Export (↓) exports both closed and open notes as Markdown via save dialog
    let export_dir = tempfile::tempdir().unwrap();
    let closed_md = export_dir.path().join("closed.md");
    *h.dialogs.save_path.borrow_mut() = Some(closed_md.clone());
    h.invoke_all_notes_export_note();
    let closed_exported = std::fs::read_to_string(&closed_md).expect("closed note exported");
    assert!(closed_exported.contains("# Meeting ideas"));
    assert!(closed_exported.contains("Q4 roadmap"));

    h.invoke_all_notes_select_note(id0);
    let open_md = export_dir.path().join("open.md");
    *h.dialogs.save_path.borrow_mut() = Some(open_md.clone());
    h.invoke_all_notes_export_note();
    let open_exported = std::fs::read_to_string(&open_md).expect("open note exported");
    assert!(open_exported.contains("# Endfield Note"));
    assert!(open_exported.contains("Thermite Cutter"));

    // 5. Delete (🗑) enters confirmation; Keep and Escape cancel without deleting
    assert_eq!(h.all_notes_confirming_delete(), Some(false));
    h.invoke_all_notes_request_delete();
    assert_eq!(h.all_notes_confirming_delete(), Some(true));

    h.invoke_all_notes_cancel_delete();
    assert_eq!(h.all_notes_confirming_delete(), Some(false));
    assert!(!h.is_deleted(id0));

    h.invoke_all_notes_request_delete();
    assert_eq!(h.all_notes_confirming_delete(), Some(true));
    h.press_all_notes_escape();
    assert_eq!(h.all_notes_confirming_delete(), Some(false));
    assert!(h.is_all_notes_open(), "Escape while confirming delete only cancels delete");
    assert!(!h.is_deleted(id0));

    // 6. Confirm Delete permanently deletes the open note and selects the next available note (id1)
    h.invoke_all_notes_request_delete();
    assert_eq!(h.all_notes_confirming_delete(), Some(true));
    h.invoke_all_notes_confirm_delete();
    assert_eq!(h.all_notes_confirming_delete(), Some(false));
    assert!(h.is_deleted(id0), "id0 should be permanently deleted from SQLite");
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.all_notes_open_count(), Some(1));
    assert_eq!(h.all_notes_closed_count(), Some(1));
    assert_eq!(h.all_notes_selected_id(), Some(id1), "next available note id1 should be selected");
}

#[test]
fn test_click_dropdown_settings_button() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.toggle_dropdown(0);
    assert!(h.is_dropdown_open());

    // Window size is 300 x 352
    // card is x: 16, y: 16, w: 268, h: 320
    // action bar is y: 16 + 256 = 272, h: 58. middle y = 301.
    // action bar x: 16 + 6 = 22, w: 256.
    // 3 buttons: New (22..104), All notes (108..190), Settings (194..276).
    // Middle of Settings button: x = 235.0, y = 301.0.
    let position = slint::LogicalPosition::new(235.0, 301.0);
    let button = slint::platform::PointerEventButton::Left;
    h.dispatch_dropdown_event(slint::platform::WindowEvent::PointerMoved { position });
    h.dispatch_dropdown_event(slint::platform::WindowEvent::PointerPressed { position, button });
    h.dispatch_dropdown_event(slint::platform::WindowEvent::PointerReleased { position, button });
    h.advance_frames(ms(600));

    assert!(h.is_settings_open(), "clicking Settings button should open settings!");
}

