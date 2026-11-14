//! Adversarial Cross-Module Integration Stress Tests for Milestone 4 (Final Verification & Hardening)
//!
//! Evaluates complex cross-module workflows across deepened seams:
//! 1. Concurrency and interleaving:
//!    - All Notes open while desktop notes are being edited, parked, unparked, and resized.
//!    - Interleaved typing in active desktop notes with rapid row switching in All Notes.
//!    - Multiple active Image Viewers while All Notes is open and editing is ongoing.
//! 2. Cross-module data flows:
//!    - Interleaved clipboard operations between Image Viewer copy (`on_copy_picture`),
//!      Note editor text paste, and Note editor image paste.
//!    - Cascade parking edge cases: park multiple notes, switch notes in All Notes,
//!      delete notes from All Notes while parked, unpark remaining notes.
//!    - Geometry persistence across complex sequences: ensuring desktop note geometry
//!      saved to SQLite is never lost, overwritten, or corrupted by All Notes parking cycles.
//! 3. High-stress interleaved operations matrix.

use slint::platform::{Key, WindowAdapter, WindowEvent};
use snap::Harness;
use std::time::Duration;
use washi::images::{encode_png, Rgba};
use washi::platform::NoteId;
use washi::storage::Geometry;

const COLOR_RED: [u8; 3] = [210, 35, 55];
const COLOR_BLUE: [u8; 3] = [35, 75, 215];
const COLOR_GREEN: [u8; 3] = [40, 185, 65];

fn sample_rgba(w: u32, h: u32, rgb: [u8; 3]) -> Rgba {
    let data = (0..w * h).flat_map(|_| [rgb[0], rgb[1], rgb[2], 255]).collect();
    Rgba { width: w, height: h, data }
}

fn write_png(dir: &std::path::Path, name: &str, img: &Rgba) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, encode_png(img).unwrap()).unwrap();
    path
}

fn placed_behind(h: &Harness, id: i64) -> bool {
    h.control.log.borrow().contains(&snap::WindowCommand::PlaceBehind(id))
}

fn open_viewer_for_note(h: &mut Harness, note_idx: usize, target_rgb: [u8; 3]) {
    let (w, _, px) = h.render(note_idx);
    let spots: Vec<(u32, u32)> = px
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            (p.r as i32 - target_rgb[0] as i32).abs() <= 15
                && (p.g as i32 - target_rgb[1] as i32).abs() <= 15
                && (p.b as i32 - target_rgb[2] as i32).abs() <= 15
        })
        .map(|(i, _)| (i as u32 % w, i as u32 / w))
        .collect();
    assert!(
        !spots.is_empty(),
        "picture matching color {target_rgb:?} not found in rendered note {note_idx}"
    );
    let n = spots.len() as f32;
    let scale = h.note_drawing_scale(note_idx);
    let x = spots.iter().map(|s| s.0 as f32).sum::<f32>() / n / scale;
    let y = spots.iter().map(|s| s.1 as f32).sum::<f32>() / n / scale;
    h.click(note_idx, x, y);
    h.click(note_idx, x, y);
}

fn dispatch_ctrl_c(window: &slint::Window) {
    let ctrl: slint::SharedString = Key::Control.into();
    window.dispatch_event(WindowEvent::KeyPressed { text: ctrl.clone() });
    window.dispatch_event(WindowEvent::KeyPressed { text: "c".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: "c".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: ctrl });
}

// =========================================================================
// SECTION 1: CONCURRENCY AND INTERLEAVING
// =========================================================================

#[test]
fn test_concurrent_all_notes_open_while_desktop_notes_edited_parked_unparked_resized() {
    let mut h = Harness::new();
    h.start().unwrap();

    // 1. Create 4 notes on desktop at defined geometries (>= min size 444x444)
    h.set_note_text(0, "# Note 0 (Host)\nInitial host body.");
    let g0 = Geometry { x: 300, y: 80, w: 460, h: 480 };
    h.place_note(0, g0);

    for i in 1..4 {
        h.new_note_from(0);
        let idx = h.open_note_count() - 1;
        h.set_note_text(idx, &format!("# Note {i}\nInitial body {i}."));
        h.place_note(
            idx,
            Geometry {
                x: 320 + (i as i32 * 60),
                y: 100 + (i as i32 * 40),
                w: 460,
                h: 480,
            },
        );
        h.advance(Duration::from_millis(50));
    }
    assert_eq!(h.open_note_count(), 4);
    let ids: Vec<NoteId> = (0..4).map(|i| h.note_id(i)).collect();
    let initial_geom_2 = h.note_geometry(2);

    // 2. Open All Notes from Note 0
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());
    h.advance(Duration::from_millis(100));

    // Sequentially switch rows to park previous notes
    h.invoke_all_notes_select_note(ids[1]);
    h.advance(Duration::from_millis(50));
    assert!(placed_behind(&h, ids[0]), "Note 0 should be parked behind");

    h.invoke_all_notes_select_note(ids[2]);
    h.advance(Duration::from_millis(50));
    assert!(placed_behind(&h, ids[1]), "Note 1 should be parked behind");

    h.invoke_all_notes_select_note(ids[3]);
    h.advance(Duration::from_millis(50));
    assert!(placed_behind(&h, ids[2]), "Note 2 should be parked behind");

    // 3. Edit parked desktop Note 1 while All Notes is open
    h.type_text(1, "\nConcurrent edit on parked Note 1.");
    h.advance(Duration::from_millis(100));

    // 4. Manually resize/move desktop Note 1 while parked
    let resized_g1 = Geometry { x: 850, y: 220, w: 480, h: 500 };
    h.place_note(1, resized_g1);
    h.advance(Duration::from_millis(50));

    // 5. Switch row in All Notes to Note 1
    h.invoke_all_notes_select_note(ids[1]);
    h.advance(Duration::from_millis(100));
    assert_eq!(h.all_notes_selected_id(), Some(ids[1]));
    let editor_text = h.all_notes_editor_text().expect("editor text");
    assert!(
        editor_text.contains("Concurrent edit on parked Note 1."),
        "All Notes editor must immediately reflect desktop edit on parked note: {editor_text}"
    );

    // 6. Edit Note 1 directly in All Notes editor
    h.set_all_notes_editor_text("# Note 1\nEdited inside All Notes.");
    h.advance(Duration::from_millis(600)); // allow debounced autosave

    // 7. Verify live sync from All Notes editor back to desktop Note 1
    assert_eq!(h.note_text(1), "# Note 1\nEdited inside All Notes.");

    // 8. Open/focus Note 2 individually from All Notes list
    h.invoke_all_notes_open_note(ids[2]);
    h.advance(Duration::from_millis(100));

    // 9. Type more into Note 2 on desktop
    h.type_text(2, " Extra text on unparked Note 2.");
    h.advance(Duration::from_millis(100));

    // 10. Dismiss All Notes -> unpark all parked notes
    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open());
    h.advance(Duration::from_millis(100));

    // 11. Verify all notes are unparked cleanly:
    // - Note 1 was manually moved/resized while parked -> must retain user-specified resized_g1!
    // - Note 2 unparks and restores to initial_geom_2!
    assert_eq!(h.note_geometry(1), resized_g1, "Note 1 must retain manually moved geometry");
    assert_eq!(h.note_geometry(2), initial_geom_2, "Note 2 must restore to initial home geometry");
    assert_eq!(h.note_text(1), "# Note 1\nEdited inside All Notes.");
    assert!(h.note_text(2).contains("Extra text on unparked Note 2."));
}

#[test]
fn test_interleave_typing_active_desktop_note_with_row_switching_all_notes() {
    let mut h = Harness::new();
    h.start().unwrap();

    // Create 3 notes: Alpha, Beta, Gamma
    h.set_note_text(0, "# Alpha\nAlpha body.");
    let g0 = Geometry { x: 100, y: 100, w: 460, h: 460 };
    h.place_note(0, g0);

    h.new_note_from(0);
    h.set_note_text(1, "# Beta\nBeta body.");
    h.place_note(1, Geometry { x: 580, y: 100, w: 460, h: 460 });

    h.new_note_from(0);
    h.set_note_text(2, "# Gamma\nGamma body.");
    h.place_note(2, Geometry { x: 1060, y: 100, w: 460, h: 460 });

    let id0 = h.note_id(0);
    let id1 = h.note_id(1);
    let id2 = h.note_id(2);
    let ids = [id0, id1, id2];

    // Open All Notes
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());
    h.advance(Duration::from_millis(50));

    // Stress loop: 40 iterations of interleaved typing and row switching
    for step in 0..40 {
        let target_note_idx = step % 3;
        let selected_id = ids[target_note_idx];

        // Switch row in All Notes
        h.invoke_all_notes_select_note(selected_id);
        h.advance(Duration::from_millis(5));

        // Interleave typing in the desktop note
        let char_to_type = format!("k{}", step % 10);
        h.type_text(target_note_idx, &char_to_type);

        // Interleave typing into All Notes editor
        if step % 5 == 0 {
            let cur_editor_text = h.all_notes_editor_text().unwrap_or_default();
            h.set_all_notes_editor_text(&format!("{cur_editor_text}+"));
        }

        h.advance(Duration::from_millis(15));
    }

    // Settle debounced autosave
    h.advance(Duration::from_millis(700));

    // Verify consistency:
    // Desktop notes and SQLite saved text must be in sync
    for i in 0..3 {
        let desktop_text = h.note_text(i);
        let saved_text = h.saved_text(i);
        assert_eq!(desktop_text, saved_text, "Desktop text must match SQLite saved text for note {i}");
    }

    // Dismiss All Notes cleanly
    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open());
}

#[test]
fn test_multiple_image_viewers_while_all_notes_active_and_editing() {
    let dir = tempfile::tempdir().unwrap();
    let img_red = write_png(dir.path(), "red.png", &sample_rgba(300, 200, COLOR_RED));
    let img_blue = write_png(dir.path(), "blue.png", &sample_rgba(400, 250, COLOR_BLUE));

    let mut h = Harness::new();
    h.start().unwrap();

    // Note 0 gets red image
    h.set_note_text(0, "# Red Note");
    h.choose_picture(&img_red);
    h.click_image_button(0);
    h.wait_for_images();

    // Note 1 gets blue image
    h.new_note_from(0);
    h.set_note_text(1, "# Blue Note");
    h.choose_picture(&img_blue);
    h.click_image_button(1);
    h.wait_for_images();

    assert_eq!(h.viewer_window_sizes().len(), 0);

    // Open ImageViewer 0 from Note 0
    open_viewer_for_note(&mut h, 0, COLOR_RED);
    assert_eq!(h.viewer_window_sizes().len(), 1, "First image viewer must open");

    // Open All Notes from Note 0
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());
    h.advance(Duration::from_millis(50));

    // Open ImageViewer 1 from Note 1 while All Notes is open!
    open_viewer_for_note(&mut h, 1, COLOR_BLUE);
    assert_eq!(h.viewer_window_sizes().len(), 2, "Both image viewers must be open simultaneously");

    // While both viewers and All Notes are active:
    // 1. Edit text in Note 1 on desktop
    h.type_text(1, "\nEditing Note 1 with 2 viewers and All Notes active.");
    h.advance(Duration::from_millis(50));

    // 2. Switch row in All Notes to Note 1
    let id1 = h.note_id(1);
    h.invoke_all_notes_select_note(id1);
    h.advance(Duration::from_millis(50));

    // 3. Edit in All Notes editor
    h.set_all_notes_editor_text("# Blue Note\nEdited in All Notes paper.");
    h.advance(Duration::from_millis(600));

    // 4. Close first viewer window
    let v0_size = h.viewer_window_sizes()[0];
    let wins = snap::platform::windows();
    let viewer_win_0 = wins
        .iter()
        .find(|w| {
            let s = w.window().size();
            (s.width, s.height) == v0_size
        })
        .expect("viewer window 0 exists");
    viewer_win_0.window().dispatch_event(WindowEvent::CloseRequested);
    h.advance(Duration::from_millis(10));

    assert_eq!(h.viewer_window_sizes().len(), 1, "Only one viewer must remain after closing viewer 0");

    // 5. Dismiss All Notes
    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open());

    // 6. Close remaining viewer
    let remaining_size = h.viewer_window_sizes()[0];
    let remaining_wins = snap::platform::windows();
    let viewer_win_1 = remaining_wins
        .iter()
        .find(|w| {
            let s = w.window().size();
            (s.width, s.height) == remaining_size
        })
        .expect("viewer window 1 exists");
    viewer_win_1.window().dispatch_event(WindowEvent::CloseRequested);
    h.advance(Duration::from_millis(10));

    assert_eq!(h.viewer_window_sizes().len(), 0, "All viewers closed");

    // Verify persisted text in SQLite
    h.advance(Duration::from_millis(600));
    assert_eq!(h.saved_text(1), "# Blue Note\nEdited in All Notes paper.");
}

// =========================================================================
// SECTION 2: CROSS-MODULE DATA FLOWS
// =========================================================================

#[test]
fn test_clipboard_interleaved_viewer_copy_note_text_paste_note_image_paste() {
    let dir = tempfile::tempdir().unwrap();
    let img_path = write_png(dir.path(), "green.png", &sample_rgba(320, 180, COLOR_GREEN));

    let mut h = Harness::new();
    h.start().unwrap();

    // Note 0: Text note typed via type_text so caret is at the end
    h.type_text(0, "Clipboard Hub");

    // Step A: Plain text paste into Note 0
    h.paste(0, " Added plain text.");
    assert!(h.note_text(0).contains("Clipboard Hub Added plain text."));
    assert_eq!(h.block_kinds(0), ["title"]);

    // Note 1: Image container note (made as before: a right click on note 0, then New)
    h.click_right(0, 5.0, 5.0);
    h.dismiss_menu();
    h.new_note_from(0);
    h.type_text(1, "Image Source");
    h.choose_picture(&img_path);
    h.click_image_button(1);
    h.wait_for_images();

    // Step B: Open ImageViewer for the green picture in Note 1
    open_viewer_for_note(&mut h, 1, COLOR_GREEN);
    assert_eq!(h.viewer_window_sizes().len(), 1);

    // Step C: Trigger copy_picture in ImageViewer via Ctrl+C
    let viewer_size = h.viewer_window_sizes()[0];
    let viewer_win = snap::platform::windows()
        .into_iter()
        .find(|w| {
            let s = w.window().size();
            (s.width, s.height) == viewer_size
        })
        .expect("viewer window")
        .clone();
    dispatch_ctrl_c(viewer_win.window());
    h.advance(Duration::from_millis(20));

    // Verify clipboard now holds the RGBA image
    let cb_img = h.clipboard_image().expect("clipboard must contain copied image");
    assert_eq!(cb_img.width, 320);
    assert_eq!(cb_img.height, 180);

    // Step D: Interleave paste into Note 0 -> because clipboard holds an image,
    // paste routes to paste_image and inserts an image block!
    h.press_enter(0);
    h.press_ctrl(0, 'v');
    h.wait_for_images();

    assert_eq!(
        h.block_kinds(0),
        ["title", "image", "body"],
        "Pasting while clipboard has image must insert an image block"
    );

    // Step E: Select all in Note 0 and copy text range
    // Ctrl+A activates range selection wash; Ctrl+C invokes copy_range which sets rich text and clears image
    h.press_ctrl(0, 'a');
    h.press_ctrl(0, 'c');
    h.advance(Duration::from_millis(20));

    // Verify clipboard image was cleared / replaced with text
    assert!(
        h.clipboard_image().is_none(),
        "Copying text selection must clear clipboard image"
    );
    assert!(h.clipboard_text().is_some(), "Clipboard must now hold copied text");

    // Step F: Paste in Note 0 -> should paste TEXT, not an image!
    let prev_blocks = h.block_kinds(0);
    h.press_ctrl(0, 'v');
    h.advance(Duration::from_millis(50));
    assert_eq!(
        h.block_kinds(0),
        prev_blocks,
        "Pasting text must NOT insert additional image blocks"
    );

    // Step G: Copy picture again from Viewer and paste into Note 1
    dispatch_ctrl_c(viewer_win.window());
    h.advance(Duration::from_millis(20));
    assert!(h.clipboard_image().is_some());

    h.press_enter(1);
    h.press_ctrl(1, 'v');
    h.wait_for_images();
    assert_eq!(
        h.image_blocks(1).len(),
        2,
        "Note 1 must now contain 2 images after image paste"
    );

    // Close viewer cleanly
    viewer_win.window().dispatch_event(WindowEvent::CloseRequested);
    h.advance(Duration::from_millis(10));
    assert_eq!(h.viewer_window_sizes().len(), 0);
}

#[test]
fn test_cascade_parking_edge_cases_park_switch_delete_unpark() {
    let mut h = Harness::new();
    h.start().unwrap();

    // 1. Create 5 notes on desktop: Note 0 (host) and Notes 1..=4
    h.set_note_text(0, "# Host Note 0");
    h.place_note(0, Geometry { x: 300, y: 100, w: 460, h: 480 });

    for i in 1..5 {
        h.new_note_from(0);
        let idx = h.open_note_count() - 1;
        h.set_note_text(idx, &format!("# Note {i}\nBody {i}"));
        h.place_note(
            idx,
            Geometry {
                x: 340 + (i as i32 * 30),
                y: 120 + (i as i32 * 20),
                w: 460,
                h: 480,
            },
        );
        h.advance(Duration::from_millis(30));
    }
    assert_eq!(h.open_note_count(), 5);

    let id0 = h.note_id(0);
    let id1 = h.note_id(1);
    let id2 = h.note_id(2);
    let id3 = h.note_id(3);
    let id4 = h.note_id(4);
    let initial_geom_1 = h.note_geometry(1);
    let initial_geom_3 = h.note_geometry(3);
    let initial_geom_4 = h.note_geometry(4);

    // 2. Open All Notes from Note 0
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());
    h.advance(Duration::from_millis(100));

    // Sequentially switch to each note to park previous notes
    h.invoke_all_notes_select_note(id1);
    h.advance(Duration::from_millis(40));
    assert!(placed_behind(&h, id0));

    h.invoke_all_notes_select_note(id2);
    h.advance(Duration::from_millis(40));
    assert!(placed_behind(&h, id1));

    h.invoke_all_notes_select_note(id3);
    h.advance(Duration::from_millis(40));
    assert!(placed_behind(&h, id2));

    h.invoke_all_notes_select_note(id4);
    h.advance(Duration::from_millis(40));
    assert!(placed_behind(&h, id3));

    // 3. Switch back to Note 2 in All Notes
    h.invoke_all_notes_select_note(id2);
    h.advance(Duration::from_millis(50));
    assert_eq!(h.all_notes_selected_id(), Some(id2));

    // 4. Delete Note 2 from All Notes while parked!
    h.invoke_all_notes_request_delete();
    assert_eq!(h.all_notes_confirming_delete(), Some(true));
    h.invoke_all_notes_confirm_delete();
    h.advance(Duration::from_millis(100));

    // Note 2 should be gone from open notes
    assert_eq!(h.open_note_count(), 4, "Open note count must decrease to 4 after deleting Note 2");
    let active_ids: Vec<NoteId> = (0..h.open_note_count()).map(|i| h.note_id(i)).collect();
    assert!(
        !active_ids.contains(&id2),
        "Note 2 must be deleted from App active notes"
    );

    // 5. Switch to Note 3, type in All Notes editor
    h.invoke_all_notes_select_note(id3);
    h.advance(Duration::from_millis(50));
    h.set_all_notes_editor_text("# Note 3\nEdited while Note 2 was deleted.");
    h.advance(Duration::from_millis(600));

    // 6. Dismiss All Notes -> unpark remaining notes (Notes 1, 3, 4)
    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open());
    h.advance(Duration::from_millis(100));

    // 7. Verify remaining notes unparked cleanly to their exact home geometries
    let remaining_ids: Vec<NoteId> = (0..h.open_note_count()).map(|i| h.note_id(i)).collect();
    assert_eq!(remaining_ids.len(), 4);
    assert!(!remaining_ids.contains(&id2));

    // Find indices of remaining notes
    let idx_1 = remaining_ids.iter().position(|&id| id == id1).unwrap();
    let idx_3 = remaining_ids.iter().position(|&id| id == id3).unwrap();
    let idx_4 = remaining_ids.iter().position(|&id| id == id4).unwrap();

    assert_eq!(h.note_geometry(idx_1), initial_geom_1, "Note 1 home geometry restored");
    assert_eq!(h.note_geometry(idx_3), initial_geom_3, "Note 3 home geometry restored");
    assert_eq!(h.note_geometry(idx_4), initial_geom_4, "Note 4 home geometry restored");
    assert_eq!(h.note_text(idx_3), "# Note 3\nEdited while Note 2 was deleted.");
}

#[test]
fn test_geometry_persistence_across_complex_parking_unparking_cycles() {
    let mut h = Harness::new();
    h.start().unwrap();

    // 1. Setup 3 notes with non-overlapping initial geometries (>= min size 444x444)
    let g0 = Geometry { x: 120, y: 150, w: 460, h: 460 };
    let g1 = Geometry { x: 380, y: 180, w: 470, h: 470 };
    let g2 = Geometry { x: 450, y: 220, w: 480, h: 480 };

    h.set_note_text(0, "# Geometrist 0");
    h.place_note(0, g0);

    h.new_note_from(0);
    h.set_note_text(1, "# Geometrist 1");
    h.place_note(1, g1);

    h.new_note_from(0);
    h.set_note_text(2, "# Geometrist 2");
    h.place_note(2, g2);

    // Wait for debounced geometry flush (GEOMETRY_DELAY = 500ms)
    h.advance(Duration::from_millis(600));

    let id0 = h.note_id(0);
    let id1 = h.note_id(1);
    let id2 = h.note_id(2);

    // Initial check: SQLite geometry
    assert_eq!(h.saved_geometry(id0), Some(g0));
    assert_eq!(h.saved_geometry(id1), Some(g1));
    assert_eq!(h.saved_geometry(id2), Some(g2));

    // 2. Cycle 1: Open All Notes -> Switch to Note 1 to park Note 0, switch to Note 2 to park Note 1
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());
    h.advance(Duration::from_millis(50));

    h.invoke_all_notes_select_note(id1);
    h.advance(Duration::from_millis(50));
    h.invoke_all_notes_select_note(id2);
    h.advance(Duration::from_millis(50));

    // CRITICAL INVARIANT: While parked, SQLite saved_geometry MUST NOT be overwritten
    // with temporary cascade coordinates!
    assert_eq!(
        h.saved_geometry(id1),
        Some(g1),
        "SQLite saved_geometry for Note 1 must NOT be corrupted during parking"
    );

    // 3. User drags Note 1 to a completely new desktop position while parked
    let user_moved_g1 = Geometry { x: 780, y: 550, w: 470, h: 470 };
    h.place_note(1, user_moved_g1);
    h.advance(Duration::from_millis(600)); // debounce flush

    // 4. Close All Notes -> Note 2 unparks to g2, Note 1 retains user_moved_g1
    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open());
    h.advance(Duration::from_millis(600));

    assert_eq!(h.note_geometry(1), user_moved_g1, "Note 1 must retain user-dragged position");
    assert_eq!(h.note_geometry(2), g2, "Note 2 must restore to original home g2");

    assert_eq!(h.saved_geometry(id1), Some(user_moved_g1));
    assert_eq!(h.saved_geometry(id2), Some(g2));

    // 5. Cycle 2: Open All Notes again, switch notes, close again
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    h.advance(Duration::from_millis(50));
    h.invoke_all_notes_select_note(id2);
    h.advance(Duration::from_millis(50));
    h.press_all_notes_escape();
    h.advance(Duration::from_millis(600));

    // 6. App Restart: Verify clean restoration from database without corrupted coordinates
    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 3);
    assert_eq!(h.saved_geometry(id1), Some(user_moved_g1));
    assert_eq!(h.saved_geometry(id2), Some(g2));
}

// =========================================================================
// SECTION 3: STRESS HARNESS MATRIX
// =========================================================================

#[test]
fn test_high_stress_interleaved_matrix() {
    let dir = tempfile::tempdir().unwrap();
    let img_path = write_png(dir.path(), "matrix.png", &sample_rgba(200, 150, COLOR_BLUE));

    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, "# Matrix Note 0\nInitial text.");

    let mut rng = 123456789u64;
    let mut next_rand = || {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (rng >> 32) as u32
    };

    for step in 0..60 {
        let op = next_rand() % 8;
        match op {
            0 => {
                // Open All Notes if closed, or close if open
                if h.is_all_notes_open() {
                    h.press_all_notes_escape();
                } else if h.open_note_count() > 0 {
                    h.toggle_dropdown(0);
                    h.invoke_dropdown_all_notes();
                }
            }
            1 => {
                // Type in desktop note
                let count = h.open_note_count();
                if count > 0 {
                    let n = (next_rand() as usize) % count;
                    h.type_text(n, &format!(" t{step}"));
                }
            }
            2 => {
                // Switch row in All Notes if open
                if h.is_all_notes_open() {
                    let count = h.open_note_count();
                    if count > 0 {
                        let n = (next_rand() as usize) % count;
                        let id = h.note_id(n);
                        h.invoke_all_notes_select_note(id);
                    }
                }
            }
            3 => {
                // Edit text in All Notes editor
                if h.is_all_notes_open() {
                    let cur = h.all_notes_editor_text().unwrap_or_default();
                    h.set_all_notes_editor_text(&format!("{cur}."));
                }
            }
            4 => {
                // Open new note
                if h.open_note_count() < 6 {
                    h.new_note_from(0);
                    let last = h.open_note_count() - 1;
                    h.set_note_text(last, &format!("# Created note {step}"));
                }
            }
            5 => {
                // Open image viewer if an image exists, or insert an image
                let count = h.open_note_count();
                if count > 0 {
                    let n = (next_rand() as usize) % count;
                    if h.image_blocks(n).is_empty() {
                        h.choose_picture(&img_path);
                        h.click_image_button(n);
                        h.wait_for_images();
                    } else if h.viewer_window_sizes().len() < 2 {
                        open_viewer_for_note(&mut h, n, COLOR_BLUE);
                    }
                }
            }
            6 => {
                // Copy or close viewer if open
                let viewer_sizes = h.viewer_window_sizes();
                if !viewer_sizes.is_empty() {
                    let target_size = viewer_sizes[0];
                    let wins = snap::platform::windows();
                    if let Some(v_win) = wins.iter().find(|w| {
                        let s = w.window().size();
                        (s.width, s.height) == target_size
                    }) {
                        if next_rand() % 2 == 0 {
                            dispatch_ctrl_c(v_win.window());
                        } else {
                            v_win.window().dispatch_event(WindowEvent::CloseRequested);
                        }
                    }
                }
            }
            7 => {
                // Settle and advance frames
                h.advance(Duration::from_millis(20));
            }
            _ => unreachable!(),
        }
    }

    // Final cleanup and validation
    if h.is_all_notes_open() {
        h.press_all_notes_escape();
    }
    // Close any remaining image viewers specifically
    while let Some(target_size) = h.viewer_window_sizes().first().copied() {
        if let Some(v_win) = snap::platform::windows().iter().find(|w| {
            let s = w.window().size();
            (s.width, s.height) == target_size
        }) {
            v_win.window().dispatch_event(WindowEvent::CloseRequested);
            h.advance(Duration::from_millis(10));
        } else {
            break;
        }
    }
    h.advance(Duration::from_millis(600));

    assert_eq!(h.viewer_window_sizes().len(), 0);
    assert!(!h.is_all_notes_open());
    assert!(h.open_note_count() >= 1);
    assert_eq!(h.schema_version(), 2);
}
