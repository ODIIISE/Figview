//! fig-golden — golden-reference renderer and CLI toolkit.
//!
//! Subcommands:
//!   inspect <file.fig> [--json]   stage-timed parse summary (delegates to parser)
//!   render  <file.fig> --out DIR  emit one SVG + PNG per page
//!
//! The renderer walks the same scene graph the live viewer uses, then emits
//! plain SVG so `resvg` can rasterize a deterministic reference image.

mod svg;

use fig_renderer::scene::build_scene_graph;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("inspect") => inspect_command(&args[1..]),
        Some("render") => render_command(&args[1..]),
        _ => {
            eprintln!(
                "Usage:\n  fig-golden inspect <file.fig> [--json]\n  fig-golden render <file.fig> --out <dir> [--page N] [--scale S]"
            );
            std::process::exit(2);
        }
    }
}

fn inspect_command(args: &[String]) {
    let json = args.iter().any(|a| a == "--json");
    let path = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .expect("missing file path");
    // Reuse the parser example's logic by invoking it inline: simplest is to
    // shell out to keep one implementation, but for a single binary we just
    // re-implement the tiny summary here via parse_file.
    let t = std::time::Instant::now();
    let doc = match fig_parser::parse_file(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    if json {
        println!(
            "{}",
            serde_json::json!({
                "file": path,
                "prelude": doc.header.prelude,
                "version": doc.header.version,
                "schema_defs": doc.header.schema_def_count,
                "nodes": doc.nodes.len(),
                "images": doc.image_hashes.len(),
                "pages": doc.pages.len(),
                "parse_ms": t.elapsed().as_millis(),
            })
        );
    } else {
        println!(
            "file={} version={} nodes={} pages={} images={} in {:.2?}",
            path,
            doc.header.version,
            doc.nodes.len(),
            doc.pages.len(),
            doc.image_hashes.len(),
            t.elapsed()
        );
    }
}

fn render_command(args: &[String]) {
    let mut file = None;
    let mut out_dir = PathBuf::from("golden-out");
    let mut page_filter: Option<usize> = None;
    let mut scale: f32 = 1.0;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                i += 1;
                out_dir = PathBuf::from(args.get(i).cloned().unwrap_or_default());
            }
            "--page" => {
                i += 1;
                page_filter = args.get(i).and_then(|v| v.parse().ok());
            }
            "--scale" => {
                i += 1;
                scale = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(1.0);
            }
            other => file = Some(other.to_string()),
        }
        i += 1;
    }

    let Some(file) = file else {
        eprintln!("render: missing <file.fig>");
        std::process::exit(2);
    };
    std::fs::create_dir_all(&out_dir).expect("create out dir");

    println!("Parsing {} ...", file);
    let doc = match fig_parser::parse_file(&file) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    println!("Building scene graph ...");
    let scene = build_scene_graph(&doc);

    // Extract embedded images once; keyed by hash for SVG embedding.
    let images = crate::svg::extract_images(&file).unwrap_or_default();

    let stem = std::path::Path::new(&file)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "document".into());

    for (page_idx, tree) in scene.trees.iter().enumerate() {
        if let Some(want) = page_filter {
            if want != page_idx {
                continue;
            }
        }
        if tree.content_bounds.is_empty() {
            continue;
        }
        let safe_name: String = tree
            .page_name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' })
            .collect();
        let base = format!(
            "{}/p{:02}_{}",
            out_dir.display(),
            page_idx,
            safe_name.trim()
        );
        let svg_path = PathBuf::from(format!("{}.svg", base));
        let png_path = PathBuf::from(format!("{}.png", base));

        print!("  page {:2} {:?}: ", page_idx, tree.page_name);
        let svg = svg::emit_page_svg(tree, &images);

        if let Some(parent) = svg_path.parent() {
            std::fs::create_dir_all(parent).expect("create page parent dir");
        }
        println!("writing {}", svg_path.display());
        if let Err(e) = std::fs::write(&svg_path, &svg) {
            eprintln!("write failed: {} -> {:?}", svg_path.display(), e);
            std::process::exit(1);
        }

        // Rasterize to PNG at the requested scale.
        let opt = resvg::usvg::Options::default();
        let tree = resvg::usvg::Tree::from_str(&svg, &opt)
            .map_err(|e| format!("usvg parse: {:?}", e));
        match tree {
            Ok(usvg_tree) => {
                let size = usvg_tree.size();
                let w = (size.width() * scale).round().max(1.0) as u32;
                let h = (size.height() * scale).round().max(1.0) as u32;
                let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).expect("pixmap");
                resvg::render(
                    &usvg_tree,
                    resvg::tiny_skia::Transform::from_scale(scale, scale),
                    &mut pixmap.as_mut(),
                );
                match pixmap.save_png(&png_path) {
                    Ok(_) => println!("{}x{} -> {}", w, h, png_path.display()),
                    Err(e) => println!("PNG save failed: {:?}", e),
                }
            }
            Err(e) => println!("SVG written but rasterization failed: {}", e),
        }
    }
}
