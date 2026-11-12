//! `snap` in library mode: the whole Washi app on a stand-in platform, driven from `cargo test`.
//!
//! Tests act from the outside (type, advance the fake clock, close, restart on the same data
//! folder) and check from the outside (note text, rendered pixels, raw database bytes).

pub mod caret;
pub mod fakes;
mod harness;
mod images;
mod settings_appearance;
mod settings_general;
mod settings_height;
mod settings_shots;
mod sidebar_layout;
pub mod platform;
pub mod shot;

pub use fakes::WindowCommand;
pub use harness::{Harness, MenuState, TAPE_COLORS};
pub use settings_appearance::tab;
pub use settings_shots::DESK;
pub use sidebar_layout::sidebar;
