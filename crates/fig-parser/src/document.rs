//! Document tree construction from decoded Kiwi message.

use crate::error::ParseError;
use crate::fastkiwi::FastVal;
use crate::nodes;
use crate::types::*;
use rayon::prelude::*;
use std::collections::HashMap;

/// Build the document directly from the decoded kiwi root value —
/// no intermediate JSON representation, nodes extracted in parallel.
pub fn build_document(
    root: &FastVal,
    schema_def_count: usize,
    meta: &serde_json::Value,
    thumbnail: &[u8],
    images: &HashMap<String, Vec<u8>>,
    prelude: &str,
    version: u32,
) -> Result<FigDocument, ParseError> {
    let file_name = meta
        .get("file_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled")
        .to_string();
    let document_id = meta
        .get("document_id")
        .or_else(|| meta.get("file_id"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let node_changes: &[FastVal] =
        match nodes::get(root, "nodeChanges").and_then(nodes::as_array_ref) {
            Some(a) => a,
            None => &[],
        };
    let blobs = extract_blobs(root);

    // Extract every node in parallel; each change is independent.
    let extracted: Vec<(FigNode, Option<(NodeId, String)>)> = node_changes
        .par_iter()
        .map(|raw| {
            let node = extract_node(raw, &blobs);
            let parent = nodes::get_parent_index(raw);
            (node, parent)
        })
        .collect();

    let mut nodes: Vec<FigNode> = Vec::with_capacity(extracted.len());
    let mut parent_refs: Vec<(usize, NodeId, String)> = Vec::new();
    for (idx, (node, parent)) in extracted.into_iter().enumerate() {
        if let Some((pid, pos)) = parent {
            parent_refs.push((idx, pid, pos));
        }
        nodes.push(node);
    }

    let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
    for (idx, pid, pos) in &parent_refs {
        if let Some(n) = nodes.get_mut(*idx) {
            n.parent_id = Some(pid.clone());
            n.position = Some(pos.clone());
        }
        if let Some(ref guid) = nodes[*idx].guid {
            children_map
                .entry(pid.to_string())
                .or_default()
                .push(guid.to_string());
        }
    }

    // Sort children by position. Figma position keys are numeric strings
    // ("0", "1", "10", …, "END") — a lexicographic sort would scramble
    // z-order past 9 siblings, so parse numerically and pin non-numeric
    // keys (like "END") to the end, preserving their relative order.
    let pos_lookup: HashMap<String, (u64, usize)> = {
        let mut m: HashMap<String, (u64, usize)> = HashMap::new();
        let mut seq = 0usize;
        for n in nodes.iter() {
            if let (Some(guid), Some(pos)) = (&n.guid, &n.position) {
                let key = match pos.parse::<u64>() {
                    Ok(v) => v,
                    Err(_) => u64::MAX,
                };
                m.insert(guid.to_string(), (key, seq));
                seq += 1;
            }
        }
        m
    };
    for ids in children_map.values_mut() {
        ids.sort_by(|a, b| {
            let pa = pos_lookup.get(a).map(|s| s.0).unwrap_or(u64::MAX);
            let pb = pos_lookup.get(b).map(|s| s.0).unwrap_or(u64::MAX);
            pa.cmp(&pb).then_with(|| {
                let sa = pos_lookup.get(a).map(|s| s.1).unwrap_or(usize::MAX);
                let sb = pos_lookup.get(b).map(|s| s.1).unwrap_or(usize::MAX);
                sa.cmp(&sb)
            })
        });
    }

    let pages: Vec<Page> = nodes
        .iter()
        .filter(|n| {
            matches!(n.parent_id.as_ref(), Some(p) if p.to_string() == "0:0")
                && matches!(n.node_type, NodeType::Canvas)
                && n.name != "Internal Only Canvas"
        })
        .map(|n| Page {
            id: n.guid.clone().unwrap_or(NodeId::new(0, 0)),
            name: n.name.clone(),
        })
        .collect();

    Ok(FigDocument {
        header: FigHeader {
            prelude: prelude.to_string(),
            version,
            schema_def_count,
        },
        file_name,
        document_id,
        pages,
        nodes,
        children_map,
        image_hashes: images.keys().cloned().collect(),
        thumbnail: thumbnail.to_vec(),
    })
}

fn extract_node(raw: &FastVal, blobs: &[Vec<u8>]) -> FigNode {
    let type_str = || nodes::as_str_of(raw, "type");
    let str_field = |name: &str| nodes::get(raw, name).and_then(|v| v.as_str());
    let num_field = |name: &str| nodes::get(raw, name).and_then(|v| v.as_f64());
    let bool_field = |name: &str| nodes::get(raw, name).and_then(|v| v.as_bool());

    FigNode {
        guid: nodes::get_guid(raw, "guid"),
        node_type: type_str()
            .map(NodeType::from_str)
            .unwrap_or(NodeType::Unknown),
        name: str_field("name").unwrap_or("").to_string(),
        phase: str_field("phase").and_then(|s| match s {
            "CREATED" => Some(NodePhase::Created),
            "REMOVED" => Some(NodePhase::Removed),
            _ => None,
        }),
        parent_id: None,
        position: None,
        visible: bool_field("visible").unwrap_or(true),
        opacity: num_field("opacity").unwrap_or(1.0) as f32,
        locked: bool_field("locked").unwrap_or(false),
        size: nodes::get_vector(raw, "size"),
        transform: nodes::get_matrix(raw, "transform"),
        corner_radius: num_field("cornerRadius").map(|v| v as f32),
        corner_radii: nodes::get_corner_radii(raw),
        clips_content: nodes::get_clips_content(raw),
        blend_mode: str_field("blendMode").unwrap_or("PASS_THROUGH").to_string(),
        fill_paints: nodes::get_paints(raw, "fillPaints"),
        background_paints: nodes::get_paints(raw, "backgroundPaints"),
        stroke_paints: nodes::get_paints(raw, "strokePaints"),
        stroke_weight: num_field("strokeWeight").unwrap_or(0.0) as f32,
        stroke_align: str_field("strokeAlign")
            .map(|s| match s {
                "INSIDE" => StrokeAlign::Inside,
                "OUTSIDE" => StrokeAlign::Outside,
                _ => StrokeAlign::Center,
            })
            .unwrap_or(StrokeAlign::Center),
        effects: nodes::get_effects(raw),
        fill_geometry: nodes::get_geometry_paths(nodes::get(raw, "fillGeometry"), blobs),
        stroke_geometry: nodes::get_geometry_paths(nodes::get(raw, "strokeGeometry"), blobs),
        vector_geometry: nodes::get_vector_geometry(nodes::get(raw, "vectorData"), blobs),
        text_data: nodes::get_text_data(raw),
    }
}

/// Pull the raw `blobs` array (geometry byte streams) out of the root value.
fn extract_blobs(root: &FastVal) -> Vec<Vec<u8>> {
    let Some(arr) = nodes::get(root, "blobs").and_then(nodes::as_array_ref) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|entry| {
            let bytes = nodes::get(entry, "bytes")?.as_bytes()?;
            Some(bytes.to_vec())
        })
        .collect()
}
