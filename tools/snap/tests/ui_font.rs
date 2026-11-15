//! The Font in use across the whole UI: Settings, All notes, the Dropdown menu, the right-click menu
//! and the Delete dialog draw their text in it, scaled by the UI fit factor so a wide font takes the
//! room the Default font took. The factor is exactly 1 for the Default font, and note text is never
//! scaled by it.

use snap::{tab, Harness};
use std::path::{Path, PathBuf};
use std::time::Duration;
use washi::fonts::{bake, read_package, ActiveFont, AxisValues};
use washi::storage::Geometry;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fonts").join(name)
}

fn started() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    h
}

fn open_settings(h: &mut Harness) {
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
}

/// Make the wide fixture font ("Test Sans") the Font in use through the Font manager.
fn use_the_wide_font(h: &mut Harness) {
    open_settings(h);
    h.settings_click_tab(tab::STYLE);
    h.advance_frames(ms(600));
    h.settings_style_click(0);
    h.advance_frames(ms(600));
    h.choose_font_file(Some(&fixture("Test_Sans.zip")));
    h.settings_manager_click_install();
    h.wait_for_font_installs();
    h.settings_manager_click_row(1);
    h.advance_frames(ms(100));
}

fn wide_fit() -> f32 {
    let wide = bake(&read_package(&std::fs::read(fixture("Test_Sans.zip")).unwrap()).unwrap(), AxisValues::default()).unwrap();
    let default = ActiveFont::default_font();
    (washi::fonts::avg_advance_em(default.faces.face(false, false)) / wide.metrics.avg_advance_em).clamp(0.70, 1.0)
}

#[test]
fn the_default_font_has_a_ui_fit_of_exactly_one_on_every_surface() {
    let mut h = started();
    open_settings(&mut h);
    h.press_dropdown_escape();
    h.right_click(0, None);
    let surfaces = h.ui_surfaces();
    assert!(surfaces.len() >= 2, "settings and the right-click menu are open: {surfaces:?}");
    for (name, family, fit) in surfaces {
        assert_eq!(fit, 1.0, "{name}");
        assert_eq!(family, "Shadows Into Light", "{name}");
    }
}

#[test]
fn the_wide_font_shrinks_ui_text_but_never_below_seventy_percent() {
    let fit = wide_fit();
    assert!(fit < 1.0, "the wide fixture really is wider than the Default font: {fit}");
    assert!((0.70..1.0).contains(&fit));
    // the wide font at that fit takes no more room than the Default font did (unless the floor held)
    let wide = bake(&read_package(&std::fs::read(fixture("Test_Sans.zip")).unwrap()).unwrap(), AxisValues::default()).unwrap();
    let default = washi::fonts::avg_advance_em(ActiveFont::default_font().faces.face(false, false));
    assert!(fit == 0.70 || wide.metrics.avg_advance_em * fit <= default * 1.0001);
}

#[test]
fn every_open_surface_takes_the_chosen_font_and_fit_live_and_popups_opened_later_do_too() {
    let mut h = started();
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    h.advance_frames(ms(600));
    assert!(h.is_all_notes_open());
    // Settings and All notes are open together while the font changes
    use_the_wide_font(&mut h);
    let family = h.note_font_family(0);
    assert_ne!(family, "Shadows Into Light");
    let fit = wide_fit();
    let live = h.ui_surfaces();
    assert!(live.iter().any(|s| s.0 == "settings") && live.iter().any(|s| s.0 == "all notes"), "{live:?}");
    for (name, f, k) in &live {
        assert_eq!(f, &family, "{name} follows the font live");
        assert_eq!(*k, fit, "{name} follows the fit live");
    }

}

#[test]
fn a_popup_that_is_open_when_the_font_changes_follows_it() {
    let mut h = started();
    h.toggle_dropdown(0);
    assert_eq!(h.ui_surfaces()[0].1, "Shadows Into Light");
    h.app().set_active_font(font_with_fit());
    h.advance(ms(20));
    let s = h.ui_surfaces();
    assert_eq!(s[0].0, "dropdown");
    assert!(s[0].2 < 1.0, "{s:?}");
}

fn font_with_fit() -> ActiveFont {
    let set = bake(&read_package(&std::fs::read(fixture("Test_Sans.zip")).unwrap()).unwrap(), AxisValues::default()).unwrap();
    washi::fonts::registry::activate(&set);
    ActiveFont::from_face_set(&set).unwrap()
}

#[test]
fn the_popups_read_the_font_when_they_open() {
    let mut h = started();
    use_the_wide_font(&mut h);
    h.settings_close();
    h.advance_frames(ms(600));
    let family = h.note_font_family(0);
    let fit = wide_fit();

    h.toggle_dropdown(0);
    assert_eq!(h.ui_surfaces(), vec![("dropdown", family.clone(), fit)]);
    h.invoke_dropdown_dismiss();
    h.right_click(0, None);
    assert_eq!(h.ui_surfaces(), vec![("context menu", family.clone(), fit)]);
}

#[test]
fn the_delete_dialog_reads_the_font_when_it_opens() {
    let mut h = started();
    use_the_wide_font(&mut h);
    h.settings_close();
    h.advance_frames(ms(600));
    h.press_ctrl(0, 'd');
    assert!(h.is_delete_dialog_open());
    assert_eq!(h.ui_surfaces(), vec![("delete dialog", h.note_font_family(0), wide_fit())]);
}

#[test]
fn putting_the_default_font_back_puts_the_ui_back_exactly() {
    let mut h = started();
    use_the_wide_font(&mut h);
    h.settings_manager_click_row(0);
    h.advance_frames(ms(100));
    for (name, family, fit) in h.ui_surfaces() {
        assert_eq!((family.as_str(), fit), ("Shadows Into Light", 1.0), "{name}");
    }
}

#[test]
fn note_text_is_never_scaled_by_the_ui_fit() {
    let mut h = started();
    let before = h.note_body_size(0);
    use_the_wide_font(&mut h);
    assert!(wide_fit() < 1.0);
    assert_eq!(h.note_body_size(0), before);
}

#[test]
fn no_ui_file_hard_codes_the_default_font_family() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../washi/ui");
    let mut found = Vec::new();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().and_then(|x| x.to_str()) != Some("slint") {
            continue;
        }
        for (i, line) in std::fs::read_to_string(&p).unwrap().lines().enumerate() {
            if line.contains("font-family") && line.contains("\"Shadows Into Light\"") {
                found.push(format!("{}:{}", p.file_name().unwrap().to_string_lossy(), i + 1));
            }
        }
    }
    assert!(found.is_empty(), "hard-coded families: {found:?}");
}

#[test]
fn every_ui_font_size_is_multiplied_by_the_ui_fit() {
    // Not UI text: note text, the toolbar glyph buttons, a Font's own-face row name, the Style preview of a note.
    // The color picker draws in its own fixed face (Gelasio), not the Font in use, so the fit has nothing to correct.
    let exempt_files = ["text.slint", "block.slint", "toolbar.slint", "color-picker.slint"];
    let exempt_lines = [
        // PText and TiltLabel only pass on the size their user gave them (the user applies the fit)
        ("settings-widgets.slint", "in property <length> font-size: 14px;"),
        ("settings-widgets.slint", "font-size: root.font-size;"),
        ("settings.slint", "font-size: root.font-size;"),
        ("font-manager.slint", "font-size: 19px + 0px * Theme.text-epoch;"),
        ("style-tab.slint", "font-size: 24px + 0px * Theme.text-epoch;"),
        ("style-tab.slint", "font-size: root.preview-font-size + 0px * Theme.text-epoch;"),
    ];
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../washi/ui");
    let mut missing = Vec::new();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if p.extension().and_then(|x| x.to_str()) != Some("slint") || exempt_files.contains(&name.as_str()) {
            continue;
        }
        for (i, line) in std::fs::read_to_string(&p).unwrap().lines().enumerate() {
            let t = line.trim();
            // a declaration, not a property or callback named *-font-size
            let decl = !t.starts_with("//") && (t.starts_with("font-size:") || t.contains(" font-size:"));
            if decl && !t.contains("ui-fit") && !exempt_lines.iter().any(|(f, l)| *f == name && t == *l) {
                missing.push(format!("{name}:{}: {t}", i + 1));
            }
        }
    }
    assert!(missing.is_empty(), "UI font sizes without the ui-fit factor: {missing:#?}");
}

#[test]
fn the_toolbar_glyph_buttons_keep_their_own_fonts() {
    let src = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../washi/ui/toolbar.slint")).unwrap();
    assert!(src.contains("font-family: \"Gelasio\"") && src.contains("font-family: \"Noto Serif\""));
    assert!(!src.contains("ui-fit") && !src.contains("Theme.handwriting"));
}
