//! Dropping files on the Settings window to install them as fonts (ticket 08).
//!
//! The window system only says that a file is over the window, left it, or was let go of
//! (winit's `HoveredFile`, `HoveredFileCancelled`, `DroppedFile`: ADR 0003, S3). What that means is
//! decided here, so it can be tested without a real drag: a drop counts only while the Font manager
//! is open, each dropped file is installed on its own (a drop of several files arrives as one event
//! per file), and the install area lights up while a file is held over the window.

use std::path::PathBuf;

/// What the window system reported about a file being dragged over the Settings window.
#[derive(Clone, Debug, PartialEq)]
pub enum FileDrag {
    /// A file is being held over the window.
    Hover,
    /// It was taken away again without being let go.
    Cancel,
    /// It was let go of over the window.
    Drop(PathBuf),
}

/// What to do about a [`FileDrag`].
#[derive(Debug, Default, PartialEq)]
pub struct Reaction {
    /// The install area shows that it is being dropped on.
    pub hover: bool,
    /// A file to install, exactly as the "Install a font" picker would.
    pub install: Option<PathBuf>,
}

/// The reaction to `drag` while the Font manager is `manager_open` (or not).
pub fn react(manager_open: bool, drag: FileDrag) -> Reaction {
    match drag {
        FileDrag::Hover => Reaction { hover: manager_open, install: None },
        FileDrag::Cancel => Reaction::default(),
        FileDrag::Drop(path) => Reaction { hover: false, install: manager_open.then_some(path) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_dropped_while_the_manager_is_open_is_installed_and_the_hover_ends() {
        let r = react(true, FileDrag::Drop("Open_Sans.zip".into()));
        assert_eq!(r, Reaction { hover: false, install: Some("Open_Sans.zip".into()) });
    }

    #[test]
    fn a_file_dropped_while_the_manager_is_closed_does_nothing() {
        assert_eq!(react(false, FileDrag::Drop("Open_Sans.zip".into())), Reaction::default());
    }

    #[test]
    fn the_install_area_lights_up_only_while_a_file_is_over_an_open_manager() {
        assert!(react(true, FileDrag::Hover).hover);
        assert!(!react(false, FileDrag::Hover).hover);
        assert!(!react(true, FileDrag::Cancel).hover);
    }
}
