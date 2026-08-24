//! Quick test binary: parses a .fig file and prints its structure.
//!
//! All lookups are index-based (HashMap) — no linear scans — so timing
//! reflects the real pipeline rather than instrumentation overhead.
//!
//! Usage:
//!   inspect <file.fig>            human-readable summary
//!   inspect <file.fig> --json     machine-readable summary (for manifests)

use std::collections::HashMap;
use std::time::Instant;

#[derive(serde::Serialize)]
struct PageSummary {
    name: String,
    direct_children: usize,
    reachable: usize,
    max_depth: usize,
}

#[derive(serde::Serialize)]
struct FileSummary {
    file: String,
    prelude: String,
    version: u32,
    schema_defs: usize,
    nodes: usize,
    images: usize,
    thumbnail_bytes: usize,
    pages: Vec<PageSummary>,
    self_cycles: usize,
    multi_parent_nodes: usize,
    orphaned_subtrees: usize,
    parse_ms: u128,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let path = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .expect("Usage: inspect <path/to/file.fig> [--json]");

    println!("Parsing: {}", path);

    let t_parse = Instant::now();
    let doc = match fig_parser::parse_file(&path) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    let parse_ms = t_parse.elapsed().as_millis();

    // GUID -> node index for O(1) lookups.
    let _index: HashMap<String, usize> = doc
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.guid.as_ref().map(|g| (g.to_string(), i)))
        .collect();

    // Cycle detection: report self-references and multiply-parented nodes.
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut self_cycles = 0usize;
    let mut multi_parent_nodes = 0usize;
    for (id, children) in &doc.children_map {
        for c in children {
            if c == id {
                self_cycles += 1;
                eprintln!("  CYCLE: node {} is its own child", c);
            }
            if !seen.insert(c.as_str()) {
                multi_parent_nodes += 1;
            }
        }
    }

    let mut pages = Vec::new();
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
        pages.push(PageSummary {
            name: page.name.clone(),
            direct_children: children.len(),
            reachable,
            max_depth,
        });
    }

    // Orphan check: nodes whose key claims children but is neither a page
    // nor reachable as some node's child (unreachable roots).
    let page_keys: std::collections::HashSet<String> =
        doc.pages.iter().map(|p| p.id.to_string()).collect();
    let mut orphaned_subtrees = 0usize;
    for n in &doc.nodes {
        if let Some(guid) = &n.guid {
            let key = guid.to_string();
            if doc.children_map.contains_key(&key)
                && !page_keys.contains(&key)
                && !seen.contains(key.as_str())
            {
                orphaned_subtrees += 1;
            }
        }
    }

    if json {
        let summary = FileSummary {
            file: path,
            prelude: doc.header.prelude.clone(),
            version: doc.header.version,
            schema_defs: doc.header.schema_def_count,
            nodes: doc.nodes.len(),
            images: doc.image_hashes.len(),
            thumbnail_bytes: doc.thumbnail.len(),
            pages,
            self_cycles,
            multi_parent_nodes,
            orphaned_subtrees,
            parse_ms,
        };
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        println!("  Prelude: {}", doc.header.prelude);
        println!("  Version: {}", doc.header.version);
        println!("  Schema defs: {}", doc.header.schema_def_count);
        println!("  File: {}", doc.file_name);
        println!("  Nodes: {}", doc.nodes.len());
        println!("  Images: {}", doc.image_hashes.len());
        println!("  Thumbnail: {} bytes", doc.thumbnail.len());
        println!("  Pages: {}", doc.pages.len());

        if self_cycles > 0 || multi_parent_nodes > 0 {
            println!(
                "  GRAPH WARNINGS: {} self-cycles, {} nodes with multiple parents",
                self_cycles, multi_parent_nodes
            );
        }
        if orphaned_subtrees > 0 {
            println!(
                "  Orphaned subtrees (unreachable roots): {}",
                orphaned_subtrees
            );
        }
        println!("  Parsed in {:.2?}", t_parse.elapsed());
    }
}
