//! How many whole-block wraps one input costs today, per operation and note. These are the numbers
//! the layout cache work (.scratch/layout-cache) brings down; each ticket lowers them in this file.
//! Wall times are in `bin/layout_bench` (timing is too noisy to assert).

use snap::layout_bench::{fixtures, run, Op};

/// `(fixture, op) -> wraps` for the median input, and the most any input needed.
fn counts() -> Vec<(String, Op, usize, usize)> {
    let mut out = vec![];
    for f in fixtures() {
        for op in Op::ALL {
            let s = run(&f.text, op, 5);
            out.push((f.name.to_string(), op, s.median_wraps(), s.max_wraps()));
        }
    }
    out
}

#[test]
fn wraps_per_operation_today() {
    // Measured at 992e711 (before any layout cache work). One whole-note wrap is one `wrap` per
    // text block, so divide by the block count (mockup.md 3, long.md 13, generated 50; the pointer
    // scenario also has the pasted picture and the paragraph below it).
    use Op::*;
    let expected: &[(&str, Op, usize)] = &[
        ("mockup.md", Keystroke, 36),
        ("mockup.md", Down, 24),
        ("mockup.md", PointerOverImage, 12),
        ("mockup.md", ResizeStep, 10),
        ("long.md", Keystroke, 156),
        ("long.md", Down, 96),
        ("long.md", PointerOverImage, 48),
        ("long.md", ResizeStep, 46),
        ("50 paragraphs", Keystroke, 506),
        ("50 paragraphs", Down, 306),
        ("50 paragraphs", PointerOverImage, 153),
        ("50 paragraphs", ResizeStep, 151),
    ];
    for (name, op, median, max) in counts() {
        let want = expected.iter().find(|e| e.0 == name && e.1 == op).unwrap_or_else(|| panic!("no pinned count for {name} / {op:?}: {median}")).2;
        assert_eq!((median, max), (want, want), "{name} / {}", op.name());
    }
}
