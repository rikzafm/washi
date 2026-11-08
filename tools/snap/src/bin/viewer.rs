//! `viewer --image <png|jpg> [--caption <text>] [--scale 1.5] [--desk RRGGBB] --out <png>`
//!
//! Renders the picture viewer window headlessly (stand-in platform) over a desk colour, for
//! comparing it with the design.

use slint::platform::software_renderer::PremultipliedRgbaColor;
use slint::platform::WindowAdapter;
use slint::PhysicalSize;
use std::rc::Rc;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let image = get("--image").expect("--image <file>");
    let caption = get("--caption").unwrap_or_else(|| "cart + chisa quickswap".into());
    let scale: f32 = get("--scale").map(|s| s.parse().unwrap()).unwrap_or(1.5);
    let out = get("--out").unwrap_or_else(|| "viewer.png".into());
    let desk = u32::from_str_radix(&get("--desk").unwrap_or_else(|| "2B3340".into()), 16).unwrap();
    let desk = [(desk >> 16) as u8, (desk >> 8) as u8, desk as u8];

    snap::platform::install();
    let px = washi::images::decode(&std::fs::read(&image).unwrap()).unwrap();
    let size = (px.width, px.height);
    let pic = washi::note_images::to_slint(&px);
    let _viewer = washi::image_viewer::ImageViewer::open(pic, size, &caption, None, slint::Image::default(), Rc::new(|_| {})).unwrap();
    let adapter = snap::platform::windows().pop().unwrap();
    let win = adapter.window();
    win.dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor: scale });
    let (lw, lh) = washi::image_viewer::window_size(size, None);
    let (pw, ph) = ((lw * scale).round() as u32, (lh * scale).round() as u32);
    win.set_size(PhysicalSize::new(pw, ph));
    let mut buf = vec![PremultipliedRgbaColor::default(); (pw * ph) as usize];
    for _ in 0..2 {
        win.request_redraw();
        snap::platform::advance(std::time::Duration::from_millis(50));
        adapter.draw_if_needed(|r| {
            r.render(&mut buf, pw as usize);
        });
    }
    let mut rgb = Vec::with_capacity((pw * ph * 3) as usize);
    for p in &buf {
        let inv = 255 - p.alpha as u32;
        for (s, d) in [(p.red, desk[0]), (p.green, desk[1]), (p.blue, desk[2])] {
            rgb.push((s as u32 + (d as u32 * inv + 127) / 255).min(255) as u8);
        }
    }
    let mut enc = png::Encoder::new(std::fs::File::create(&out).unwrap(), pw, ph);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgb).unwrap();
    println!("{out} {pw}x{ph}");
}
