//! `layout_bench [--iterations 200]`
//!
//! What one input costs in layout work, headless (stand-in platform), on `docs/design/fixtures/mockup.md`,
//! `long.md` and a generated 50 paragraph note: the median and p95 wall time and the `wrap` count of a
//! typed character, the Down arrow, a pointer move over a selected picture and a 10 px paper resize.
//! Run it on a release build for numbers worth logging (`cargo run --release -p snap --bin layout_bench`).
//! Prints a Markdown table.

use snap::layout_bench::{build_time, fixtures, run, Op};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let iterations: usize = args.iter().position(|a| a == "--iterations").and_then(|i| args.get(i + 1)).map_or(200, |s| s.parse().expect("--iterations <n>"));
    println!("| note | operation | median ms | p95 ms | wraps (median) |");
    println!("|---|---|---|---|---|");
    for f in fixtures() {
        for op in Op::ALL {
            let s = run(&f.text, op, iterations);
            let ms = |d: std::time::Duration| format!("{:.3}", d.as_secs_f64() * 1000.0);
            println!("| {} | {} | {} | {} | {} |", f.name, op.name(), ms(s.median_time()), ms(s.p95_time()), s.median_wraps());
        }
    }
    println!();
    println!("| note | Layout::build median ms | typed character median ms | share |");
    println!("|---|---|---|---|");
    for f in fixtures() {
        let build = build_time(&f.text, iterations).as_secs_f64() * 1000.0;
        let key = run(&f.text, Op::Keystroke, iterations).median_time().as_secs_f64() * 1000.0;
        println!("| {} | {:.3} | {:.3} | {:.0}% |", f.name, build, key, build / key * 100.0);
    }
}
