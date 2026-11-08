//! Washi core library. The `washi` binary is a thin shell over this; the `snap` harness
//! drives the same `App` on a stand-in platform.

pub mod app;
pub mod clipboard_html;
pub mod document;
pub mod dropdown;
pub mod history;
pub mod icon;
pub mod image_viewer;
pub mod images;
pub mod export;
pub mod note_images;
pub mod note_window;
pub mod occlusion;
pub mod placement;
pub mod platform;
pub mod textflow;
pub mod storage;
pub mod suspend;
pub mod trace;
pub mod win;

pub use app::{App, ClosedCandidate, Env, StartError, EXIT_WINDOW, NEW_NOTE_OFFSET};
