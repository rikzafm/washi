#[path = "src/icon.rs"]
#[allow(dead_code)]
mod icon;

fn main() {
    slint_build::compile("ui/note.slint").unwrap();
    // The app icon (drawn from spec values by src/icon.rs) becomes the .exe's icon resource.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let res = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("washi-icon.res");
        std::fs::write(&res, icon::res_file()).unwrap();
        println!("cargo:rerun-if-changed=src/icon.rs");
        println!("cargo:rustc-link-arg-bins={}", res.display());
    }
    // Embed the PerMonitorV2 manifest into washi.exe only (the linker merges it; no rc.exe needed).
    let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("app.manifest");
    println!("cargo:rerun-if-changed=app.manifest");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
    }
}
