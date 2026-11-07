//! Washi core library. The `washi` binary is a thin shell over this; the `snap` harness
//! drives the same `App` on a stand-in platform.

pub mod app;
pub mod document;
pub mod icon;
pub mod images;
pub mod note_window;
pub mod occlusion;
pub mod placement;
pub mod platform;
pub mod textflow;
pub mod storage;
pub mod suspend;
pub mod win;

pub use app::{App, Env, StartError};
