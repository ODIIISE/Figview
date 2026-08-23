/// Headless correctness test — drives the real parser on the real .fig file.
use fig_parser::types::*;

const FILE: &str = "/Users/mehrdad/Downloads/Clothing Store App _ Fashion E-Commerce App.fig";

fn load() -> FigDocument {
    fig_parser::parse_file(FILE).expect("parse")
}

#[test]
fn tree_consistency() {
    let doc = load();
    // Every parent_id in a node should have a corresponding entry in children_map
    for n in &doc.nodes {
        if let Some(ref pid) = n.parent_id {
            let key = pid.to_string();
            assert!(
                doc.children_map.contains_key(&key),
                "children_map missing parent {} for node {}", key, n.guid.as_ref().map(|g| g.to_string()).unwrap_or_default()
            );
        }
    }
    // Every child in children_map should reference real nodes
    for ids in doc.children_map.values() {
        for id in ids {
            assert!(
                doc.nodes.iter().any(|n| n.guid.as_ref().map(|g| g.to_string()) == Some(id.clone())),
                "node {} referenced in children_map not found", id
            );
        }
    }
}

#[test]
fn node_count() {
    let doc = load();
    assert_eq!(doc.nodes.len(), 4298, "node count changed");
    assert_eq!(doc.pages.len(), 1, "page count");
    assert_eq!(doc.header.version, 35);
}

#[test]
fn first_page_children() {
    let doc = load();
    let key = "0:1";
    let ids = doc.children_map.get(key).expect("page 0:1 has children");
    assert_eq!(ids.len(), 64, "page 0:1 should have 64 top-level frames");
    // Check the first frame is "Splash"
    let splash = doc.nodes.iter().find(|n| n.guid.as_ref().map(|g| g.to_string()) == Some(ids[0].clone())).expect("first child exists");
    assert_eq!(splash.name, "Splash");
    assert_eq!(splash.node_type, NodeType::Frame);
}

#[test]
fn image_hashes_present() {
    let doc = load();
    assert_eq!(doc.image_hashes.len(), 10, "10 embedded images");
    // All hashes should be 40-char hex strings
    for h in &doc.image_hashes {
        assert_eq!(h.len(), 40, "image hash not 40 hex chars: {}", h);
    }
}

#[test]
fn fills_and_strokes_populated() {
    let doc = load();
    let mut with_fills = 0;
    let mut with_strokes = 0;
    let mut with_gradient = 0;
    let mut with_text = 0;
    for n in &doc.nodes {
        if !n.fill_paints.is_empty() { with_fills += 1; }
        if n.stroke_weight > 0.0 { with_strokes += 1; }
        if n.fill_paints.iter().any(|p| !p.stops.is_empty()) { with_gradient += 1; }
        if n.text_data.is_some() { with_text += 1; }
    }
    assert!(with_fills > 10, "should have nodes with fills, got {}", with_fills);
    assert!(with_strokes > 0, "should have nodes with strokes, got {}", with_strokes);
    assert!(with_gradient > 0, "should have nodes with gradient fills, got {}", with_gradient);
    assert!(with_text > 0, "should have text nodes, got {}", with_text);
}

#[test]
fn gradient_stops_have_valid_positions() {
    let doc = load();
    for n in &doc.nodes {
        for p in &n.fill_paints {
            for s in &p.stops {
                assert!(s.position >= 0.0 && s.position <= 1.0,
                    "gradient stop position out of range: {} in node {}", s.position, n.name);
                assert!(s.color.a >= 0.0 && s.color.a <= 1.0,
                    "gradient stop alpha out of range: {}", s.color.a);
            }
        }
    }
}

#[test]
fn effects_populated() {
    let doc = load();
    let with_effects: Vec<_> = doc.nodes.iter().filter(|n| !n.effects.is_empty()).collect();
    assert!(!with_effects.is_empty(), "should have nodes with effects (shadows/blurs)");
    for n in &with_effects {
        for e in &n.effects {
            assert!(e.radius >= 0.0, "effect radius negative in {}", n.name);
        }
    }
}

#[test]
fn text_nodes_have_size_and_font_data() {
    let doc = load();
    let text_nodes: Vec<_> = doc.nodes.iter().filter(|n| n.node_type == NodeType::Text).collect();
    assert!(!text_nodes.is_empty(), "should have text nodes");
    for n in &text_nodes {
        assert!(n.size.is_some(), "text node {} has no size", n.name);
        assert!(n.text_data.is_some(), "text node {} has no textData", n.name);
        let td = n.text_data.as_ref().unwrap();
        assert!(td.font_size > 0.0, "text node {} has zero font_size", n.name);
    }
}

#[test]
fn no_empty_guid_nodes() {
    let doc = load();
    for n in &doc.nodes {
        assert!(n.guid.is_some(), "node {} has no GUID", n.name);
    }
}