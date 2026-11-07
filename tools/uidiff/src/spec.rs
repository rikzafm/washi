//! Region definitions from `docs/design/spec.json`.

use serde::Deserialize;

/// [x0, y0, x1, y1] in mockup pixels, x1/y1 exclusive.
pub type Rect = [i64; 4];

/// One reference mockup and which regions apply to it.
#[derive(Deserialize, Debug, Clone)]
pub struct Mockup {
    pub file: String,
    pub hover: bool,
    pub check: Vec<String>,
    /// Rects painted over with the paper color before measuring (the idle mockup's leftover caption).
    #[serde(default)]
    pub mask: Vec<Rect>,
}

/// The parts of `spec.json` the measurement tool reads. Everything else in the file
/// (layout rules, expected values, ...) is ignored here.
#[derive(Deserialize, Debug, Clone)]
pub struct Spec {
    pub paper_color: String,
    pub mockups: std::collections::BTreeMap<String, Mockup>,
    pub regions: std::collections::BTreeMap<String, Region>,
}

fn default_ink_lum() -> f64 {
    160.0
}

fn default_icon_lum() -> f64 {
    200.0
}

/// A filled button (e.g. the active toolbar button) drawn behind an icon.
#[derive(Deserialize, Debug, Clone)]
pub struct Fill {
    pub color: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Region {
    Paper {
        color: String,
        tol: u8,
    },
    Empty {
        rect: Rect,
        color: String,
        tol: u8,
    },
    Icons {
        rect: Rect,
        cells: usize,
        paper: String,
        #[serde(default)]
        fill: Option<Fill>,
        #[serde(default = "default_icon_lum")]
        ink_lum_max: f64,
    },
    Tape {
        #[serde(default)]
        rect: Option<Rect>,
        /// e.g. "r-g>=12": channel difference threshold that selects the tape's pixels
        mask: String,
    },
    Text {
        rect: Rect,
        paper: String,
        #[serde(default = "default_ink_lum")]
        ink_lum_max: f64,
    },
}
