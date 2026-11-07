//! `uidiff --font-match`: pick fonts by measurement. See docs/design/fonts.md.

mod matcher;
mod reference;
mod render;
mod score;

pub use matcher::{run, PASS_IOU};
pub use reference::{derotate, ink_bitmap};

pub use score::{align_iou, align_iou_ignoring, Bitmap};

pub use render::{load_fonts, Coverage, FontFace, TextRenderer};
