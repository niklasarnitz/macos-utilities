// build.rs – converts all SVGs in the plugin's imgs/ folder to PNG files.
// This fixes the Stream Deck hardware: it only displays PNG images, not SVGs.
// Runs automatically as part of `cargo build`.

use std::path::PathBuf;

fn main() {
    let imgs = PathBuf::from("com.niklasarnitz.macos-utilities.sdPlugin/imgs");

    println!("cargo:rerun-if-changed={}", imgs.display());

    let entries: Vec<_> = std::fs::read_dir(&imgs)
        .expect("imgs dir not found")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("svg"))
        .collect();

    for entry in entries {
        let svg_path = entry.path();
        println!("cargo:rerun-if-changed={}", svg_path.display());

        let svg_data = std::fs::read_to_string(&svg_path).expect("read svg");

        let opt = resvg::usvg::Options::default();
        let tree =
            resvg::usvg::Tree::from_str(&svg_data, &opt).expect("parse svg");

        // Render at 1× (72×72) and 2× (144×144)
        for (size, suffix) in [(72u32, ""), (144u32, "@2x")] {
            let svg_size = tree.size();
            let scale = size as f32 / svg_size.width().max(svg_size.height());
            let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);

            let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)
                .expect("create pixmap");
            resvg::render(&tree, transform, &mut pixmap.as_mut());

            let stem = svg_path.file_stem().unwrap().to_str().unwrap();
            let png_name = format!("{}{}.png", stem, suffix);
            let png_path = imgs.join(png_name);
            pixmap.save_png(&png_path).expect("save png");
        }
    }
}
