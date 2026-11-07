//! `snap` in library mode: the whole Washi app on a stand-in platform, driven from `cargo test`.
//!
//! Tests act from the outside (type, advance the fake clock, close, restart on the same data
//! folder) and check from the outside (note text, rendered pixels, raw database bytes).

pub mod fakes;
mod harness;
mod images;
pub mod platform;
pub mod shot;

pub use fakes::WindowCommand;
pub use harness::Harness;
