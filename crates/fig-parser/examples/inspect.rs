//! Quick test binary: parses a .fig file and prints its structure.
//!
//! All lookups are index-based (HashMap) — no linear scans — so timing
//! reflects the real pipeline rather than instrumentation overhead.

use std::collections::HashMap;
use std::time::Instant;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("Usage: inspect <path/to/file.fig>");
    println!("Parsing: {}", path);

    let t_parse = Instant::now();
    let doc = match fig_parser::parse_file(&path) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    println!("  parsed in {:.2?}", t_parse.elapsed());

    // GUID -> node index for O(1) lookups.
    let _index: HashMap<String, usize> = doc
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.guid.as_ref().map(|g| (g.to_string(), i)))
        .collect();

    println!("  Prelude: {}", doc.header.prelude);
    println!("  Version: {}", doc.header.version);
    println!("  Schema defs: {}", doc.header.schema_def_count);
    println!("  File: {}", doc.file_name);
    println!("  Nodes: {}", doc.nodes.len());
    println!("  Images: {}", doc.image_hashes.len());
    println!("  Thumbnail: {} bytes", doc.thumbnail.len());

    // Cycle detection: report self-references and nodes claimed by
    // multiple parents, so hangs can never hide in the graph again.
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut cycles = 0usize;
    let mut duplicates = 0usize;
    for (id, children) in &doc.children_map {
        for c in children {
            if c == id {
                cycles += 1;
                eprintln!("  CYCLE: node {} is its own child", c);
            }
            if !seen.insert(c.as_str()) {
                duplicates += 1;
            }
        }
    }
    if cycles > 0 || duplicates > 0 {
        println!(
            "  GRAPH WARNINGS: {} self-cycles, {} nodes with multiple parents",
            cycles, duplicates
        );
    }

    println!("  Pages: {}", doc.pages.len());
    for page in &doc.pages {
        let key = page.id.to_string();
        let empty = vec![];
        let children = doc.children_map.get(&key).unwrap_or(&empty);

        // Iterative DFS with a visited set: cycle-proof by construction.
        let mut reachable = 0usize;
        let mut max_depth = 0usize;
        let mut stack: Vec<(&String, usize)> = children.iter().map(|c| (c, 1usize)).collect();
        let mut visited: std::collections::HashSet<&String> = std::collections::HashSet::new();
        while let Some((cid, depth)) = stack.pop() {
            if !visited.insert(cid) {
                continue;
            }
            reachable += 1;
            max_depth = max_depth.max(depth);
            if let Some(kids) = doc.children_map.get(cid.as_str()) {
                for k in kids {
                    stack.push((k, depth + 1));
                }
            }
        }
        println!(
            "    Page {:?} ({}): {} direct children, {} reachable, depth {}",
            page.name,
            key,
            children.len(),
            reachable,
            max_depth
        );
    }

    // Orphan check: nodes whose key claims children but is neither a page
    // nor reachable as some node's child (unreachable roots).
    let page_keys: std::collections::HashSet<String> =
        doc.pages.iter().map(|p| p.id.to_string()).collect();
    let mut orphaned = 0usize;
    for n in &doc.nodes {
        if let Some(guid) = &n.guid {
            let key = guid.to_string();
            if doc.children_map.contains_key(&key)
                && !page_keys.contains(&key)
                && !seen.contains(key.as_str())
            {
                orphaned += 1;
            }
        }
    }
    if orphaned > 0 {
        println!("  Orphaned subtrees (unreachable roots): {}", orphaned);
    }
}
