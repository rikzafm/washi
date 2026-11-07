//! uidiff: measurements of the Washi reference mockups (and, later, of snapshots).

mod cc;
mod empty;
mod icons;
mod image;
mod paper;
mod spec;
mod tape;
mod text;

pub use image::Image;
pub use spec::{Mockup, Rect, Region, Spec};

use serde_json::{Map, Value};

/// Measure one region of an image. Returns a JSON object whose shape depends on the region kind.
pub fn measure_region(img: &Image, region: &Region) -> Value {
    match region {
        Region::Paper { color, tol } => paper::measure(img, color, *tol),
        Region::Empty { rect, color, tol } => empty::measure(img, rect, color, *tol),
        Region::Icons { rect, cells, paper, fill, ink_lum_max } => icons::measure(img, rect, *cells, paper, fill.as_ref(), *ink_lum_max),
        Region::Tape { rect, mask } => tape::measure(img, rect.as_ref(), mask),
        Region::Text { rect, paper, ink_lum_max } => text::measure(img, rect, paper, *ink_lum_max),
    }
}

/// Measure every region in `regions` (in the given order) after painting `masks` with the paper color.
pub fn measure_regions<'a>(img: &Image, spec: &Spec, masks: &[Rect], names: impl IntoIterator<Item = &'a String>) -> Result<Value, String> {
    let mut img = img.clone();
    let paper = image::parse_hex(&spec.paper_color)?;
    for m in masks {
        let (x0, y0, x1, y1) = empty::clamp_rect(&img, m);
        for y in y0..y1 {
            for x in x0..x1 {
                img.set(x, y, paper);
            }
        }
    }
    let mut out = Map::new();
    for name in names {
        let region = spec.regions.get(name).ok_or_else(|| format!("region '{name}' is not defined in the spec"))?;
        out.insert(name.clone(), measure_region(&img, region));
    }
    Ok(Value::Object(out))
}

/// Measure the regions a named mockup checks, from its PNG (path relative to `spec_dir`).
pub fn measure_mockup(spec: &Spec, name: &str, spec_dir: &std::path::Path) -> Result<Value, String> {
    let m = spec.mockups.get(name).ok_or_else(|| format!("mockup '{name}' is not in the spec"))?;
    let img = Image::load_png(&spec_dir.join(&m.file))?;
    measure_regions(&img, spec, &m.mask, &m.check)
}
