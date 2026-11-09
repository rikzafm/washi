//! End-to-end functional and visual fidelity snapshot tests for the All Notes ("Notes") window.

use snap::Harness;
use std::path::Path;
use std::time::Duration;

const ENDFIELD_CONTENT: &str = "# Endfield Note\n## Thermite Cutter : POG & Akekuri\nCurrently POG no Weapon : Ideal for him are Never Rest or White Night Nova. The last can be get in a box";

/// Set up a `Harness` with the exact 6 notes from the reference mockups and open `AllNotesWindow`
/// with `"Endfield Note"` selected.
fn setup_mockup_all_notes() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();

    // Create 4 closed notes from oldest to newest so `updated_at DESC` orders them:
    // 1. Meeting ideas, 2. Birthday gifts, 3. Reading list, 4. Trip to Kyoto
    let closed_titles = [
        "# Trip to Kyoto",
        "# Reading list",
        "# Birthday gifts",
        "# Meeting ideas",
    ];

    // Note 0 starts as the first closed note
    h.set_note_text(0, closed_titles[0]);
    h.advance(Duration::from_secs(1));

    for title in &closed_titles[1..] {
        h.right_click(0, Some(&["New note"]));
        let last = h.open_note_count() - 1;
        h.set_note_text(last, title);
        h.advance(Duration::from_secs(1));
    }

    // Create the 2 open notes: "Grocery run" and "Endfield Note"
    h.right_click(0, Some(&["New note"]));
    let grocery_idx = h.open_note_count() - 1;
    h.set_note_text(grocery_idx, "# Grocery run\nmilk");
    let grocery_id = h.note_id(grocery_idx);
    h.advance(Duration::from_secs(1));

    h.right_click(0, Some(&["New note"]));
    let endfield_idx = h.open_note_count() - 1;
    h.set_note_text(endfield_idx, ENDFIELD_CONTENT);
    let endfield_id = h.note_id(endfield_idx);
    h.advance(Duration::from_secs(1));

    // Close the first 4 notes (always closing index 0 four times)
    for _ in 0..4 {
        h.right_click(0, Some(&["Close"]));
    }
    assert_eq!(h.open_note_count(), 2);

    // Order open notes so Endfield Note is on top of the stack (listed 1st under OPEN)
    h.set_stacking(vec![grocery_id, endfield_id]);

    // Open All Notes from Endfield Note's dropdown menu
    let endfield_pos = (0..h.open_note_count())
        .find(|&i| h.note_id(i) == endfield_id)
        .unwrap();
    h.toggle_dropdown(endfield_pos);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());
    assert_eq!(h.all_notes_selected_id(), Some(endfield_id));

    h
}

fn load_png_rgb(path: &Path) -> (u32, u32, Vec<u8>) {
    let file = std::fs::File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().expect("read png info");
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("decode png frame");
    let (w, h) = (info.width, info.height);
    let rgb = match info.color_type {
        png::ColorType::Rgb => buf[..(w * h * 3) as usize].to_vec(),
        png::ColorType::Rgba => {
            let mut out = Vec::with_capacity((w * h * 3) as usize);
            for chunk in buf[..(w * h * 4) as usize].chunks_exact(4) {
                out.extend_from_slice(&[chunk[0], chunk[1], chunk[2]]);
            }
            out
        }
        other => panic!("unsupported png color type: {other:?}"),
    };
    (w, h, rgb)
}

fn write_png_rgb(path: &Path, width: u32, height: u32, rgb: &[u8]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("png header");
        w.write_image_data(rgb).expect("png data");
    }
    std::fs::write(path, out).expect("write png file");
}

#[test]
fn all_notes_end_to_end_workflow() {
    let mut h = setup_mockup_all_notes();

    // 1. Counts and sections match the 6 fixture notes
    assert_eq!(h.all_notes_open_count(), Some(2));
    assert_eq!(h.all_notes_closed_count(), Some(4));
    let open_items = h.all_notes_open_items();
    let closed_items = h.all_notes_closed_items();
    assert_eq!(
        open_items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
        vec!["Endfield Note", "Grocery run"]
    );
    assert_eq!(
        closed_items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
        vec!["Meeting ideas", "Birthday gifts", "Reading list", "Trip to Kyoto"]
    );
    let endfield_id = open_items[0].id as i64;
    let grocery_id = open_items[1].id as i64;
    let meeting_id = closed_items[0].id as i64;

    // 2. Selecting a closed note swaps the editor to it
    h.invoke_all_notes_select_note(meeting_id);
    assert_eq!(h.all_notes_selected_id(), Some(meeting_id));
    assert_eq!(h.all_notes_editor_note_id(), Some(meeting_id));
    assert_eq!(h.all_notes_status_text().as_deref(), Some("Closed · edited today"));

    // 3. Open/Focus reopens the closed note onto the desktop
    h.invoke_all_notes_open_or_focus();
    assert_eq!(h.open_note_count(), 3);
    assert_eq!(h.all_notes_open_count(), Some(3));
    assert_eq!(h.all_notes_closed_count(), Some(3));
    assert_eq!(
        h.all_notes_status_text().as_deref(),
        Some("Open on your desktop · edited today")
    );

    // 4. Close closes the note's desktop window and moves it back to CLOSED
    h.invoke_all_notes_close_note();
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.all_notes_open_count(), Some(2));
    assert_eq!(h.all_notes_closed_count(), Some(4));
    assert_eq!(h.all_notes_status_text().as_deref(), Some("Closed · edited today"));

    // 5. Open/Focus on an open note brings it to front
    h.invoke_all_notes_select_note(endfield_id);
    let log_before = h.control.log.borrow().len();
    h.invoke_all_notes_open_or_focus();
    assert!(
        h.control.log.borrow()[log_before..].contains(&snap::WindowCommand::BringToFront(endfield_id)),
        "Open/Focus on an open note brings it to front"
    );

    // 6. Export writes the selected note as Markdown
    let export_dir = tempfile::tempdir().unwrap();
    let md_path = export_dir.path().join("endfield.md");
    *h.dialogs.save_path.borrow_mut() = Some(md_path.clone());
    h.invoke_all_notes_export_note();
    let exported = std::fs::read_to_string(&md_path).expect("exported markdown file");
    assert!(exported.contains("# Endfield Note"));

    // 7-9. Delete asks first; Keep and Escape cancel
    assert_eq!(h.all_notes_confirming_delete(), Some(false));
    h.invoke_all_notes_request_delete();
    assert_eq!(h.all_notes_confirming_delete(), Some(true));
    h.invoke_all_notes_cancel_delete();
    assert_eq!(h.all_notes_confirming_delete(), Some(false));
    assert!(!h.is_deleted(endfield_id));
    h.invoke_all_notes_request_delete();
    h.press_all_notes_escape();
    assert_eq!(h.all_notes_confirming_delete(), Some(false));
    assert!(h.is_all_notes_open());

    // 10. Confirm deletes for good and selects the next note
    h.invoke_all_notes_request_delete();
    h.invoke_all_notes_confirm_delete();
    assert!(h.is_deleted(endfield_id));
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.all_notes_open_count(), Some(1));
    assert_eq!(h.all_notes_selected_id(), Some(grocery_id));

    // 11. "+ New note" creates a note and selects it
    h.invoke_all_notes_new_note();
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.all_notes_open_count(), Some(2));

    // 12. Escape closes All Notes
    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open());
}

#[test]
fn external_desktop_note_lifecycle_updates_open_all_notes_and_exits_when_all_closed() {
    let mut h = setup_mockup_all_notes();
    assert_eq!(h.all_notes_open_count(), Some(2));
    assert_eq!(h.all_notes_closed_count(), Some(4));
    let endfield_id = h.all_notes_open_items()[0].id as i64;
    assert_eq!(h.all_notes_selected_id(), Some(endfield_id));

    // Editing another open note from the desktop updates the list titles
    let grocery_idx = (0..h.open_note_count()).find(|&i| h.note_id(i) != endfield_id).unwrap();
    h.set_note_text(grocery_idx, "# Updated Grocery\nNew body line");
    assert!(h.all_notes_open_items().iter().any(|i| i.title.as_str() == "Updated Grocery"));

    // Creating a new note from a desktop window updates the count and keeps the selection
    h.right_click(grocery_idx, Some(&["New note"]));
    assert_eq!(h.all_notes_open_count(), Some(3));
    assert_eq!(h.all_notes_selected_id(), Some(endfield_id));

    // Closing every desktop note keeps the app alive while All Notes is open
    while h.open_note_count() > 0 {
        h.close_note(0);
    }
    assert_eq!(h.all_notes_open_count(), Some(0));
    assert!(!h.has_quit(), "app stays alive while All Notes is open");

    // Closing All Notes after that quits the app
    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open());
    assert!(h.has_quit());
}

#[test]
fn the_window_grows_left_for_the_list_and_the_paper_stays_put() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, "# Solo\nbody");
    let g = h.note_geometry(0);
    h.place_note(0, washi::storage::Geometry { x: 600, y: 100, ..g });
    let host = h.note_geometry(0);
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());

    let (x, y, w, hh) = h.all_notes_window_geometry().unwrap();
    assert_eq!(x, host.x - 282);
    assert_eq!(y, host.y);
    assert_eq!(w, host.w + 282);
    assert_eq!(hh, host.h);

    // closing gives the strip back and the note keeps its place
    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open());
    assert_eq!(h.note_geometry(0), host);
}

#[test]
fn the_editor_shows_the_selected_note_and_stays_in_sync_with_its_desktop_window() {
    let mut h = setup_mockup_all_notes();
    let open_items = h.all_notes_open_items();
    let endfield_id = open_items[0].id as i64;
    let grocery_id = open_items[1].id as i64;

    assert_eq!(h.all_notes_editor_note_id(), Some(endfield_id));
    assert!(h.all_notes_editor_text().unwrap().contains("Thermite Cutter"));

    // typing in the editor reaches the desktop window of the same note
    h.invoke_all_notes_select_note(grocery_id);
    assert_eq!(h.all_notes_editor_note_id(), Some(grocery_id));
    h.set_all_notes_editor_text("# Grocery run\nmilk and eggs");
    let grocery_idx = (0..h.open_note_count()).find(|&i| h.note_id(i) == grocery_id).unwrap();
    assert!(h.note_text(grocery_idx).contains("milk and eggs"));

    // and an edit on the desktop reaches the editor
    h.set_note_text(grocery_idx, "# Grocery run\noat milk");
    assert!(h.all_notes_editor_text().unwrap().contains("oat milk"));
}

#[test]
fn double_clicking_a_closed_row_opens_it_and_an_open_row_is_brought_forward() {
    let mut h = setup_mockup_all_notes();
    let closed_id = h.all_notes_closed_items()[0].id as i64;
    assert_eq!(h.open_note_count(), 2);

    h.invoke_all_notes_open_note(closed_id);
    assert_eq!(h.open_note_count(), 3);
    assert_eq!(h.all_notes_open_count(), Some(3));
    assert_eq!(h.all_notes_closed_count(), Some(3));
    assert!(h.all_notes_open_items().iter().any(|i| i.id as i64 == closed_id));

    let log_before = h.control.log.borrow().len();
    h.invoke_all_notes_open_note(closed_id);
    assert_eq!(h.open_note_count(), 3, "an open note is not opened twice");
    assert!(h.control.log.borrow()[log_before..].contains(&snap::WindowCommand::BringToFront(closed_id)));
}

/// With many notes the list outgrows the panel: "+ New note" stays at the panel's bottom instead of
/// following the last row.
#[test]
fn a_long_list_pins_new_note_to_the_bottom_and_a_short_one_lets_it_follow() {
    let dark_button_bottom = |h: &Harness| -> Option<usize> {
        let (w, hh, px) = h.render_all_notes().unwrap();
        // the button is #2B2F36: the lowest row with a long run of such pixels in the sidebar strip
        // (a single blended edge pixel of the panel is not the button)
        (0..hh as usize).rev().find(|&y| {
            (0..282usize.min(w as usize))
                .filter(|&x| {
                    let p = px[y * w as usize + x];
                    (p.r as i32 - 43).abs() < 8 && (p.g as i32 - 47).abs() < 8 && (p.b as i32 - 54).abs() < 8
                })
                .count()
                > 25
        })
    };
    let short = {
        let mut solo = Harness::new();
        solo.start().unwrap();
        solo.set_note_text(0, "# Solo");
        solo.toggle_dropdown(0);
        solo.invoke_dropdown_all_notes();
        dark_button_bottom(&solo).expect("button drawn")
    };
    let mut h = setup_mockup_all_notes();

    for i in 0..12 {
        h.right_click(0, Some(&["New note"]));
        let last = h.open_note_count() - 1;
        h.set_note_text(last, &format!("# Extra {i}"));
        h.right_click(last, Some(&["Close"]));
    }
    assert!(h.all_notes_closed_count().unwrap() >= 16);
    let long = dark_button_bottom(&h).expect("button still drawn");
    let (_, window_h, _) = h.render_all_notes().unwrap();
    assert!(long > short, "pinned at the bottom, lower than when it followed the rows ({short} -> {long})");
    assert!(long < window_h as usize - 30, "inside the panel, above the window's margin ({long})");

    // adding more notes does not move it again
    h.right_click(0, Some(&["New note"]));
    let last = h.open_note_count() - 1;
    h.set_note_text(last, "# One more");
    h.right_click(last, Some(&["Close"]));
    assert_eq!(dark_button_bottom(&h), Some(long));
}

/// Dumps the editor window next to its sidebar for looking at: `WASHI_SNAP_OUT_DIR=… cargo test`.
#[test]
fn render_all_notes_for_a_look() {
    let h = setup_mockup_all_notes();
    let shot = h.render_all_notes_canvas((760, 560)).expect("render");
    assert_eq!((shot.width, shot.height), (760, 560));
    if let Some(dir) = std::env::var_os("WASHI_SNAP_OUT_DIR") {
        write_png_rgb(&Path::new(&dir).join("all_notes.png"), 760, 560, shot.rgb());
    }
}


#[test]
fn pressing_the_sidebars_empty_area_drags_the_whole_window_but_a_row_does_not() {
    let mut h = setup_mockup_all_notes();
    let before = h.control.log.borrow().iter().filter(|c| matches!(c, snap::WindowCommand::Move(_))).count();
    // a row (the first one: x in the pill, y at its middle)
    h.click_all_notes(100.0, 172.0);
    let after_row = h.control.log.borrow().iter().filter(|c| matches!(c, snap::WindowCommand::Move(_))).count();
    assert_eq!(after_row, before, "pressing a row selects it, it does not start a drag");

    // the panel's empty lower-left corner
    h.click_all_notes(20.0, 400.0);
    let after_bg = h.control.log.borrow().iter().filter(|c| matches!(c, snap::WindowCommand::Move(_))).count();
    assert_eq!(after_bg, before + 1, "pressing the sidebar's empty area drags the window");
}

#[test]
fn the_list_scrolls_when_it_overflows_and_a_wheel_over_it_moves_the_rows() {
    let mut h = setup_mockup_all_notes();
    // the first row's pill (white) is at the top of the viewport before scrolling
    let pill_y = |h: &Harness| -> Option<usize> {
        let (w, hh, px) = h.render_all_notes().unwrap();
        (0..hh as usize).find(|&y| {
            (60..200usize).filter(|&x| { let p = px[y * w as usize + x]; p.r >= 252 && p.g >= 252 && p.b >= 250 }).count() > 60
        })
    };
    let top = pill_y(&h).expect("selected pill drawn");
    h.scroll_all_notes(100.0, 250.0, -60.0);
    let scrolled = pill_y(&h);
    assert!(scrolled.map_or(true, |y| y < top), "rows moved up (or the pill left the viewport)");
}

#[test]
fn near_the_screen_edge_the_window_is_kept_on_screen_instead_of_growing_off_it() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, "# Solo");
    let g = h.note_geometry(0);
    h.place_note(0, washi::storage::Geometry { x: 40, y: 0, ..g });
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    let (x, _, w, _) = h.all_notes_window_geometry().unwrap();
    assert_eq!(x, 0, "shifted right to the screen's edge, not left of it");
    assert_eq!(w, g.w + 282);
}

#[test]
fn the_editor_has_a_trimmed_dropdown_that_acts_on_the_selected_note() {
    let mut h = setup_mockup_all_notes();
    let endfield_id = h.all_notes_open_items()[0].id as i64;

    h.invoke_editor_menu();
    assert!(h.is_dropdown_open());
    assert_eq!(h.dropdown_note_id(), Some(endfield_id));
    // compact card: header + New/Settings only (128 + 2 * 16 shadow margin)
    assert_eq!(h.dropdown_size(), Some((300.0, 160.0)));
    // the button the card hangs from is inside the editor window, right of the sidebar's strip
    let (bx, _, _, _) = h.editor_menu_button_rect().unwrap();
    assert!(bx > 282.0, "menu button is over the paper: {bx}");

    // pin follows the editor and the note
    h.invoke_dropdown_toggle_pin();
    assert_eq!(h.dropdown_pinned(), Some(true));

    // delete asks inside the action bar, not in a dialog
    h.invoke_dropdown_delete_note();
    assert!(!h.is_dropdown_open());
    assert!(!h.is_delete_dialog_open());
    assert_eq!(h.all_notes_confirming_delete(), Some(true));
    h.invoke_all_notes_cancel_delete();

    // close closes the note's desktop window and keeps it in the list
    h.invoke_editor_menu();
    h.invoke_dropdown_close_note();
    assert!(!h.is_dropdown_open());
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.all_notes_closed_count(), Some(5));

    // New makes a note and selects it
    h.invoke_editor_menu();
    h.invoke_dropdown_new_note();
    assert!(!h.is_dropdown_open());
    assert_eq!(h.open_note_count(), 2);
}

#[test]
fn the_dropdown_opens_on_the_monitor_the_note_is_on() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, "# Far");
    h.control.areas.borrow_mut().push(washi::platform::ScreenRect::new(1920, 0, 1920, 1040));
    let g = h.note_geometry(0);
    h.place_note(0, washi::storage::Geometry { x: 2200, y: 100, ..g });
    h.toggle_dropdown(0);
    let (x, _) = h.dropdown_position().unwrap();
    assert!(x >= 1920.0 - 16.0, "card is on the second monitor, not clamped back to the first: {x}");
}
