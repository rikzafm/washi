//! Washi core library. The `washi` binary is a thin shell over this; the `snap` harness
//! drives the same `App` on a stand-in platform.

pub mod all_notes;
pub mod caret_motion;
pub mod app;
pub mod clipboard_html;
pub mod context_menu;
pub mod delete_dialog;
pub mod document;
pub mod dropdown;
pub mod editor;
pub mod export;
pub mod fonts;
pub mod history;
pub mod icon;
pub mod image_viewer;
pub mod images;
pub mod note_images;
pub mod note_window;
pub mod occlusion;
pub mod placement;
pub mod platform;
pub mod popup;
pub mod popups;
pub mod settings;
pub mod sidebar_panel;
pub mod textflow;
pub mod storage;
pub mod suspend;
pub mod trace;
pub mod ui_scale;
pub mod win;

pub use app::{App, ClosedCandidate, Env, StartError, EXIT_WINDOW, NEW_NOTE_OFFSET};
