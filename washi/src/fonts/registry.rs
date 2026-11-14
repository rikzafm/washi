//! Registering faces with Slint's shared font collection (ADR 0003).
//!
//! This is the only place that touches `slint::fontique_011` (an unstable Slint feature, pinned by
//! the lock file; recheck `spikes/fonts` after a Slint upgrade). Two rules from the ADR:
//!
//! - register first, then set the family on the windows: a window does not notice a font that
//!   shows up after its family string was set;
//! - every Face set has a fresh private family, so no stale glyph cache survives a change.
//!
//! Slint's collection is shared by every window of the process; calls come from the UI thread.

use super::FaceSet;
use slint::fontique_011::{fontique, shared_collection};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use fontique::{Blob, FontInfoOverride, FontStyle, FontWeight, FontWidth};

/// One face to register: its bytes, the family and the weight and style Slint asks for it by.
fn register(bytes: &[u8], family: &str, weight: f32, italic: bool) {
    let style = if italic { FontStyle::Italic } else { FontStyle::Normal };
    let over = FontInfoOverride { family_name: Some(family), weight: Some(FontWeight::new(weight)), style: Some(style), ..Default::default() };
    shared_collection().register_fonts(Blob::new(Arc::new(bytes.to_vec())), Some(over));
}

fn unregister(family: &str, weight: f32, italic: bool) {
    let style = if italic { FontStyle::Italic } else { FontStyle::Normal };
    let mut c = shared_collection();
    if let Some(id) = c.family_id(family) {
        c.unregister_font(id, FontWidth::NORMAL, style, FontWeight::new(weight));
    }
}

const TEXT_FACES: [(f32, bool); 4] = [(400.0, false), (700.0, false), (400.0, true), (700.0, true)];

thread_local! {
    /// The families of the Face set in use (text, caret), to take out once it is replaced.
    static IN_USE: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
    /// Preview faces by font id, to draw each font's name in its own face.
    static PREVIEWS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// Register the five faces of `set`. Windows must be given its families only after this returns.
/// The Face set before it is unregistered on the next turn of the event loop, when every window
/// has switched. (A fresh process starts with the bundled Default font, which is not registered
/// here: it is embedded.)
pub fn activate(set: &FaceSet) {
    let faces = [&set.regular, &set.bold, &set.italic, &set.bold_italic];
    for (bytes, (weight, italic)) in faces.into_iter().zip(TEXT_FACES) {
        register(bytes, &set.text_family, weight, italic);
    }
    register(&set.caret, &set.caret_family, 400.0, false);
    let old = IN_USE.with(|u| u.borrow_mut().replace((set.text_family.clone(), set.caret_family.clone())));
    retire(old);
}

/// The Default font is back (embedded, nothing to register): take the Face set in use out.
pub fn deactivate() {
    let old = IN_USE.with(|u| u.borrow_mut().take());
    retire(old);
}

fn retire(old: Option<(String, String)>) {
    let Some((text, caret)) = old else { return };
    slint::Timer::single_shot(std::time::Duration::ZERO, move || {
        for (weight, italic) in TEXT_FACES {
            unregister(&text, weight, italic);
        }
        unregister(&caret, 400.0, false);
    });
}

/// The preview family of font `id`, if it was registered.
pub fn preview(id: &str) -> Option<String> {
    PREVIEWS.with(|p| p.borrow().get(id).cloned())
}

/// The family a font's name is drawn in on the Font manager's list: its source file, registered
/// once under a private name.
pub fn preview_family(id: &str, source: &[u8]) -> String {
    PREVIEWS.with(|p| {
        p.borrow_mut()
            .entry(id.to_string())
            .or_insert_with(|| {
                let family = format!("Washi Preview {id}");
                register(source, &family, 400.0, false);
                family
            })
            .clone()
    })
}

/// Take a font's preview face out (the font was removed or replaced).
pub fn forget_preview(id: &str) {
    if let Some(family) = PREVIEWS.with(|p| p.borrow_mut().remove(id)) {
        unregister(&family, 400.0, false);
    }
}
