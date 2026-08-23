//! Document tree construction from decoded Kiwi message.

use crate::error::ParseError;
use crate::geometry;
use crate::kiwi::KiwiMessage;
use crate::nodes;
use crate::types::*;
use std::collections::HashMap;

pub fn build_document(
    msg: KiwiMessage,
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
    let schema_def_count = msg.schema_def_count;
    let blobs = msg.root.blobs;

    let mut nodes: Vec<FigNode> = Vec::with_capacity(msg.root.node_changes.len());
    let mut parent_refs: Vec<(usize, NodeId, String)> = Vec::new();

    for raw in &msg.root.node_changes {
        let idx = nodes.len();
        nodes.push(extract_node(raw, &blobs));
        if let Some((pid, pos)) = nodes::get_parent_index(raw) {
            parent_refs.push((idx, pid, pos));
        }
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

    // Sort children by position
    let pos_lookup: HashMap<String, String> = nodes
        .iter()
        .filter_map(|n| {
            Some((
                n.guid.as_ref()?.to_string(),
                n.position.clone().unwrap_or_default(),
            ))
        })
        .collect();
    for ids in children_map.values_mut() {
        ids.sort_by(|a, b| {
            let pa = pos_lookup.get(a).map(|s| s.as_str()).unwrap_or("");
            let pb = pos_lookup.get(b).map(|s| s.as_str()).unwrap_or("");
            pa.cmp(pb)
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

fn extract_node(raw: &serde_json::Value, blobs: &[Vec<u8>]) -> FigNode {
    FigNode {
        guid: nodes::get_guid(raw, "guid"),
        node_type: raw
            .get("type")
            .and_then(|v| v.as_str())
            .map(NodeType::from_str)
            .unwrap_or(NodeType::Unknown),
        name: raw
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        phase: raw
            .get("phase")
            .and_then(|v| v.as_str())
            .and_then(|s| match s {
                "CREATED" => Some(NodePhase::Created),
                "REMOVED" => Some(NodePhase::Removed),
                _ => None,
            }),
        parent_id: None,
        position: None,
        visible: raw.get("visible").and_then(|v| v.as_bool()).unwrap_or(true),
        opacity: raw.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
        locked: raw.get("locked").and_then(|v| v.as_bool()).unwrap_or(false),
        size: nodes::get_vector(raw, "size"),
        transform: nodes::get_matrix(raw, "transform"),
        corner_radius: raw
            .get("cornerRadius")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32),
        corner_radii: nodes::get_corner_radii(raw),
        clips_content: nodes::get_clips_content(raw),
        blend_mode: raw
            .get("blendMode")
            .and_then(|v| v.as_str())
            .unwrap_or("PASS_THROUGH")
            .to_string(),
        fill_paints: nodes::get_paints(raw, "fillPaints"),
        background_paints: nodes::get_paints(raw, "backgroundPaints"),
        stroke_paints: nodes::get_paints(raw, "strokePaints"),
        stroke_weight: raw
            .get("strokeWeight")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
        stroke_align: raw
            .get("strokeAlign")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "INSIDE" => StrokeAlign::Inside,
                "OUTSIDE" => StrokeAlign::Outside,
                _ => StrokeAlign::Center,
            })
            .unwrap_or(StrokeAlign::Center),
        effects: nodes::get_effects(raw),
        fill_geometry: geometry::decode_geometry_paths(raw.get("fillGeometry"), blobs),
        stroke_geometry: geometry::decode_geometry_paths(raw.get("strokeGeometry"), blobs),
        vector_geometry: geometry::decode_vector_geometry(raw.get("vectorData"), blobs),
        text_data: nodes::get_text_data(raw),
    }
}
