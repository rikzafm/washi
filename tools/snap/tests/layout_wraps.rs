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
fn wraps_per_operation() {
    // The editor owns one cached `Layout` (ticket 03): an edit or a resize builds it once, which is one
    // `wrap` per text block (mockup.md 3, long.md 15, generated 50), and caret motion and the pointer
    // are answered from it without wrapping. Before the cache (992e711) a keystroke cost 12 whole-note
    // wraps, Down 8, a pointer move over a selected image 4 and a resize step 3 (see docs/PERF.md).
    use Op::*;
    let expected: &[(&str, Op, usize)] = &[
        ("mockup.md", Keystroke, 3),
        ("mockup.md", Down, 0),
        ("mockup.md", PointerOverImage, 0),
        ("mockup.md", ResizeStep, 3),
        ("long.md", Keystroke, 15),
        ("long.md", Down, 0),
        ("long.md", PointerOverImage, 0),
        ("long.md", ResizeStep, 15),
        ("50 paragraphs", Keystroke, 50),
        ("50 paragraphs", Down, 0),
        ("50 paragraphs", PointerOverImage, 0),
        ("50 paragraphs", ResizeStep, 50),
    ];
    for (name, op, median, max) in counts() {
        let want = expected.iter().find(|e| e.0 == name && e.1 == op).unwrap_or_else(|| panic!("no pinned count for {name} / {op:?}: {median}")).2;
        assert_eq!((median, max), (want, want), "{name} / {}", op.name());
    }
}

#[test]
fn layout_builds_per_operation_on_the_three_block_mockup() {
    // mockup.md is a 3-block note. A typed character builds the layout exactly once, a resize step once,
    // and the Down arrow and the pointer move none: they are answered from the cached layout.
    let f = fixtures().into_iter().find(|f| f.name == "mockup.md").unwrap();
    for (op, want) in [(Op::Keystroke, 1), (Op::Down, 0), (Op::PointerOverImage, 0), (Op::ResizeStep, 1)] {
        let s = run(&f.text, op, 5);
        assert_eq!((s.min_builds(), s.max_builds()), (want, want), "{}", op.name());
    }
}
