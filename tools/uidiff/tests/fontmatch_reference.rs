//! Reference extraction: undo the paper tilt on a region, then threshold to an ink mask.

mod common;

use common::*;
use uidiff::fontmatch::{derotate, ink_bitmap};

#[test]
fn derotating_a_tilted_bar_gives_an_axis_aligned_bar_of_the_same_size() {
    // a 200 x 20 ink bar whose left end is at (50, 100), tilted 1.45 deg clockwise about that end
    let deg = 1.45f64;
    let (s, c) = deg.to_radians().sin_cos();
    let layer: Layer = Box::new(move |x, y| {
        let (dx, dy) = (x - 50.0, y - 100.0);
        let (u, v) = (dx * c + dy * s, -dx * s + dy * c); // into the bar's frame
        (u >= 0.0 && u < 200.0 && v >= 0.0 && v < 20.0).then_some(INK)
    });
    let img = render(300, 200, PAPER, &[layer]);
    // region whose top-left is the pivot: the bar starts at its (0, 0)
    let flat = derotate(&img, [50, 100, 260, 130], deg);
    let mask = ink_bitmap(&flat, PAPER, INK);
    let (x0, y0, x1, y1) = mask.bbox().expect("ink survives");
    assert_eq!((x0, x1 - x0), (0, 200), "x extent");
    assert_eq!((y0, y1 - y0), (0, 20), "y extent");
}
