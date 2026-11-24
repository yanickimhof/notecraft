//! Renders the application icon from `assets/icon.svg` (workspace root) into
//! PNG (window icon) and ICNS (macOS bundle) files in OUT_DIR.

use std::path::PathBuf;

fn render(tree: &resvg::usvg::Tree, size: u32) -> Vec<u8> {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).expect("pixmap");
    let s = size as f32 / tree.size().width();
    resvg::render(tree, resvg::tiny_skia::Transform::from_scale(s, s), &mut pixmap.as_mut());
    pixmap.encode_png().expect("png")
}

fn main() {
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=NOTECRAFT_COMMIT={commit}");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let svg_path = manifest.join("../../assets/icon.svg");
    println!("cargo:rerun-if-changed={}", svg_path.display());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let data = std::fs::read(&svg_path).expect("assets/icon.svg is missing");
    let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).expect("valid svg");
    std::fs::write(out.join("icon-256.png"), render(&tree, 256)).unwrap();
    let mut family = icns::IconFamily::new();
    for size in [16u32, 32, 64, 128, 256, 512, 1024] {
        let png = render(&tree, size);
        let image = icns::Image::read_png(std::io::Cursor::new(png)).expect("decode png");
        family.add_icon(&image).expect("add icon");
    }
    let mut f = std::fs::File::create(out.join("NoteCraft.icns")).unwrap();
    family.write(&mut f).unwrap();
}
