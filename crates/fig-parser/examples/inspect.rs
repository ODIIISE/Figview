//! Quick test binary: parses a .fig file and prints its structure.

fn main() {
    let path = std::env::args().nth(1).expect("Usage: inspect <path/to/file.fig>");
    println!("Parsing: {}", path);

    match fig_parser::parse_file(&path) {
        Ok(doc) => {
            println!("  Prelude: {}", doc.header.prelude);
            println!("  Version: {}", doc.header.version);
            println!("  Schema defs: {}", doc.header.schema_def_count);
            println!("  File: {}", doc.file_name);
            println!("  Pages: {}", doc.pages.len());
            for page in &doc.pages {
                println!("    Page: {} ({}:{})", page.name, page.id.session_id, page.id.local_id);
                let key = format!("{}:{}", page.id.session_id, page.id.local_id);
                let empty = vec![];
                let children = doc.children_map.get(&key).unwrap_or(&empty);
                println!("      Children: {}", children.len());
                for cid in children.iter().take(10) {
                    if let Some(n) = doc.nodes.iter().find(|n| n.guid.as_ref().map(|g| g.to_string()) == Some(cid.clone())) {
                        println!("        {}: {} ({:?})", cid, n.name, n.node_type);
                    }
                }
                if children.len() > 10 {
                    println!("        ... and {} more", children.len() - 10);
                }
            }
            println!("  Total nodes: {}", doc.nodes.len());
            println!("  Images: {}", doc.image_hashes.len());
            println!("  Thumbnail: {} bytes", doc.thumbnail.len());

            // Print top-level tree summary
            let root_children = doc.children_map.get("0:0");
            println!("\n  Document tree:");
            if let Some(rc) = root_children {
                println!("    Root (0:0) has {} children", rc.len());
                for cid in rc {
                    if let Some(n) = doc.nodes.iter().find(|n| n.guid.as_ref().map(|g| g.to_string()) == Some(cid.clone())) {
                        println!("      {}: {} ({:?})", cid, n.name, n.node_type);
                        let depth1 = doc.children_map.get(cid);
                        if let Some(d1) = depth1 {
                            for d1id in d1.iter().take(5) {
                                if let Some(d1n) = doc.nodes.iter().find(|n| n.guid.as_ref().map(|g| g.to_string()) == Some(d1id.clone())) {
                                    println!("        {}: {} ({:?})", d1id, d1n.name, d1n.node_type);
                                }
                            }
                            if d1.len() > 5 { println!("        ... and {} more", d1.len() - 5); }
                        }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}