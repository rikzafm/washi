//! Binary-size baseline: a Slint software-rendered window, nothing else (S4 growth = size_full - size_base).
use slint::ComponentHandle;
use washi_font_spikes::*;

fn main() {
    install_headless();
    let rig = Rig::new();
    rig.probe.set_fam("Shadows".into());
    let s = rig.render();
    println!("{}", s.ink_count(0, 200));
}
