//! winit + Slint software renderer spike.
//! usage: winit_win [--notes N] [--bare] [--x X --y Y] [--minimize-after S] [--hide-after S] [--quit-after S]
use slint::{ComponentHandle, PhysicalPosition};
use washi_spikes::{Bare, Note};

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}
fn flag(name: &str) -> bool { std::env::args().any(|x| x == name) }

fn main() {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .select()
        .unwrap();
    let n: i32 = arg("--notes").and_then(|s| s.parse().ok()).unwrap_or(1);
    let x0: i32 = arg("--x").and_then(|s| s.parse().ok()).unwrap_or(100);
    let y0: i32 = arg("--y").and_then(|s| s.parse().ok()).unwrap_or(100);
    let bare = flag("--bare");
    let mut keep: Vec<Box<dyn std::any::Any>> = vec![];
    let mut wins: Vec<slint::Weak<Note>> = vec![];
    let mut bare_wins: Vec<slint::Weak<Bare>> = vec![];
    for i in 0..n {
        let pos = PhysicalPosition::new(x0 + i * 480, y0);
        if bare {
            let w = Bare::new().unwrap();
            w.window().set_position(pos);
            w.show().unwrap();
            bare_wins.push(w.as_weak());
            keep.push(Box::new(w));
        } else {
            let w = Note::new().unwrap();
            w.set_probe(!flag("--no-probe"));
            w.window().set_position(pos);
            w.show().unwrap();
            wins.push(w.as_weak());
            keep.push(Box::new(w));
        }
    }
    // log hover transitions (hover-through-children test)
    let t = slint::Timer::default();
    if let Some(w) = wins.first().cloned() {
        let last = std::cell::Cell::new(false);
        t.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(100), move || {
            if let Some(w) = w.upgrade() {
                let h = w.get_hovered();
                if h != last.get() { last.set(h); println!("HOVER {h}"); }
            }
        });
    }
    let schedule = |name: &str, f: Box<dyn Fn()>| {
        if let Some(s) = arg(name).and_then(|s| s.parse::<u64>().ok()) {
            let tt = slint::Timer::default();
            tt.start(slint::TimerMode::SingleShot, std::time::Duration::from_secs(s), move || f());
            std::mem::forget(tt);
        }
    };
    let ws = wins.clone();
    let bs = bare_wins.clone();
    schedule("--minimize-after", Box::new(move || {
        for w in ws.iter().filter_map(|w| w.upgrade()) { w.window().set_minimized(true); }
        for w in bs.iter().filter_map(|w| w.upgrade()) { w.window().set_minimized(true); }
    }));
    let ws = wins.clone();
    schedule("--hide-after", Box::new(move || {
        for w in ws.iter().filter_map(|w| w.upgrade()) { w.window().hide().unwrap(); }
    }));
    schedule("--quit-after", Box::new(|| { slint::quit_event_loop().unwrap(); }));
    slint::run_event_loop_until_quit().unwrap();
}
