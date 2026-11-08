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
    h.right_click(0, Some(&["New note"]));
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
    h.right_click(0, Some(&["New note"]));
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
    h.right_click(0, Some(&["New note"]));
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
    h.right_click(0, Some(&["New note"]));
    assert_eq!(h.open_note_count(), 2);
    let id1 = h.note_id(1);
    h.set_note_text(1, "Closed Note\nWill be reopened");

    h.right_click(1, Some(&["Close"]));
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
    h.advance(ms(60));
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
    h.advance(ms(120));
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
    h.right_click(0, Some(&["New note"]));
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
