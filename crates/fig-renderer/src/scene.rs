//! Scene graph construction from a parsed FigDocument.
//!
//! Converts the flat FigDocument (Vec of nodes + children_map) into a
//! hierarchical RenderTree with pre-computed world transforms and bounds.

use fig_parser::types::*;
use crate::transforms::{self, Matrix, Rect};

/// A node in the pre-computed render tree.
#[derive(Debug, Clone)]
pub struct RenderNode {
    pub id: NodeId,
    pub node_type: NodeType,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: String,
    pub world_transform: Matrix,
    pub clips_content: bool,
    pub bounds: Option<Rect>,

    // Geometry
    pub fill_paints: Vec<Paint>,
    pub background_paints: Vec<Paint>,
    pub stroke_paints: Vec<Paint>,
    pub stroke_weight: f32,
    pub stroke_align: StrokeAlign,
    pub fill_geometry: Vec<GeometryPath>,
    pub stroke_geometry: Vec<GeometryPath>,
    pub vector_geometry: Option<VectorGeometry>,
    pub text_data: Option<TextData>,
    pub corner_radii: Option<CornerRadii>,
    pub corner_radius: Option<f32>,
    pub size: Option<Vec2>,
    pub effects: Vec<Effect>,

    // Hierarchy (indices into the flat node list)
    pub children: Vec<usize>,
    pub depth: usize,
}

/// A flat, pre-computed render tree for a single page.
#[derive(Debug, Clone)]
pub struct RenderTree {
    pub page_id: NodeId,
    pub page_name: String,
    pub nodes: Vec<RenderNode>,
    /// Root-level node indices (top-level children of the page).
    pub root_indices: Vec<usize>,
    /// Bounding box of all visible content.
    pub content_bounds: Rect,
}

/// The full scene graph for a document.
#[derive(Debug, Clone)]
pub struct SceneGraph {
    pub document_name: String,
    pub document_id: Option<String>,
    pub pages: Vec<(NodeId, String)>,
    pub trees: Vec<RenderTree>,
    pub image_hashes: Vec<String>,
}

/// Build a SceneGraph from a parsed FigDocument.
pub fn build_scene_graph(doc: &FigDocument) -> SceneGraph {
    let pages: Vec<(NodeId, String)> = doc
        .pages
        .iter()
        .map(|p| (p.id.clone(), p.name.clone()))
        .collect();

    let trees: Vec<RenderTree> = pages
        .iter()
        .filter_map(|(page_id, page_name)| {
            build_render_tree(doc, page_id, page_name.clone())
        })
        .collect();

    SceneGraph {
        document_name: doc.file_name.clone(),
        document_id: doc.document_id.clone(),
        pages,
        trees,
        image_hashes: doc.image_hashes.clone(),
    }
}

/// Build a RenderTree for a single page.
fn build_render_tree(doc: &FigDocument, page_id: &NodeId, page_name: String) -> Option<RenderTree> {
    let page_key = format!("{}:{}", page_id.session_id, page_id.local_id);
    let page_children = doc.children_map.get(&page_key)?;

    // Build a node ID → index lookup
    let node_index: std::collections::HashMap<String, usize> = doc
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.guid.as_ref().map(|g| (g.to_string(), i)))
        .collect();

    let mut flat_nodes: Vec<RenderNode> = Vec::new();
    let mut content_bounds = Rect::ZERO;
    let root_indices: Vec<usize> = page_children
        .iter()
        .filter_map(|child_id| {
            build_node_subtree(
                doc,
                child_id,
                &node_index,
                &transforms::IDENTITY,
                0,
                &mut flat_nodes,
                &mut content_bounds,
            )
        })
        .collect();

    if flat_nodes.is_empty() {
        return None;
    }

    Some(RenderTree {
        page_id: page_id.clone(),
        page_name,
        nodes: flat_nodes,
        root_indices,
        content_bounds,
    })
}

/// Recursively build a subtree starting from a node ID.
/// Returns the index of the node in the flat list.
fn build_node_subtree(
    doc: &FigDocument,
    node_id: &str,
    node_index: &std::collections::HashMap<String, usize>,
    parent_transform: &Matrix,
    depth: usize,
    flat: &mut Vec<RenderNode>,
    content_bounds: &mut Rect,
) -> Option<usize> {
    let original_idx = *node_index.get(node_id)?;
    let node = &doc.nodes[original_idx];

    // Skip removed or invisible nodes (but still process children for visibility inheritance)
    if matches!(node.phase, Some(NodePhase::Removed)) {
        return None;
    }

    let world_transform = transforms::compose(Some(parent_transform), node.transform.as_ref())
        .unwrap_or(transforms::IDENTITY);

    let size = node.size;
    let bounds = size.map(|s| {
        Rect::from_origin_size(0.0, 0.0, s.x, s.y).transform(&world_transform)
    });

    // Build children first
    let mut child_indices: Vec<usize> = Vec::new();
    if let Some(children) = doc.children_map.get(node_id) {
        for child_id in children {
            if let Some(idx) = build_node_subtree(
                doc,
                child_id,
                node_index,
                &world_transform,
                depth + 1,
                flat,
                content_bounds,
            ) {
                child_indices.push(idx);
            }
        }
    }

    let visible = node.visible && node.opacity > 0.0;

    // Update content bounds
    if visible {
        if let Some(ref b) = bounds {
            if !b.is_empty() {
                *content_bounds = content_bounds.union(b);
            }
        }
        // For groups/frames without explicit size, use union of children
        if bounds.is_none() || bounds.as_ref().map(|b| b.is_empty()).unwrap_or(true) {
            let child_bounds = child_indices.iter().fold(Rect::ZERO, |acc, &ci| {
                if let Some(cn) = flat.get(ci) {
                    acc.union(&cn.bounds.unwrap_or(Rect::ZERO))
                } else {
                    acc
                }
            });
            if !child_bounds.is_empty() {
                *content_bounds = content_bounds.union(&child_bounds);
            }
        }
    }

    let result_idx = flat.len();
    flat.push(RenderNode {
        id: node.guid.clone().unwrap_or(NodeId::new(0, 0)),
        node_type: node.node_type,
        name: node.name.clone(),
        visible,
        opacity: node.opacity,
        blend_mode: node.blend_mode.clone(),
        world_transform,
        clips_content: node.clips_content,
        bounds,
        fill_paints: node.fill_paints.clone(),
        background_paints: node.background_paints.clone(),
        stroke_paints: node.stroke_paints.clone(),
        stroke_weight: node.stroke_weight,
        stroke_align: node.stroke_align,
        fill_geometry: node.fill_geometry.clone(),
        stroke_geometry: node.stroke_geometry.clone(),
        vector_geometry: node.vector_geometry.clone(),
        text_data: node.text_data.clone(),
        corner_radii: node.corner_radii,
        corner_radius: node.corner_radius,
        size: node.size,
        effects: node.effects.clone(),
        children: child_indices,
        depth,
    });

    Some(result_idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_doc() -> FigDocument {
        FigDocument {
            header: FigHeader {
                prelude: "fig-kiwi".into(),
                version: 35,
                schema_def_count: 1,
            },
            file_name: "test".into(),
            document_id: None,
            pages: vec![Page { id: NodeId::new(0, 1), name: "Page 1".into() }],
            nodes: vec![
                FigNode {
                    guid: Some(NodeId::new(1, 1)),
                    node_type: NodeType::Frame,
                    name: "Frame 1".into(),
                    visible: true,
                    opacity: 1.0,
                    size: Some(Vec2::new(100.0, 100.0)),
                    transform: Some(Matrix { m00: 1.0, m01: 0.0, m02: 10.0, m10: 0.0, m11: 1.0, m12: 20.0 }),
                    fill_paints: vec![Paint {
                        paint_type: PaintType::Solid,
                        color: Some(Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
                        opacity: 1.0, visible: true,
                        stops: vec![], transform: None, image_hash: None, gradient_handles: vec![],
                    }],
                    // ...default rest
                    phase: None, parent_id: None, position: None, locked: false,
                    corner_radius: None, corner_radii: None, clips_content: false,
                    blend_mode: "PASS_THROUGH".into(),
                    background_paints: vec![], stroke_paints: vec![],
                    stroke_weight: 0.0, stroke_align: StrokeAlign::Center,
                    effects: vec![], fill_geometry: vec![], stroke_geometry: vec![],
                    vector_geometry: None, text_data: None,
                },
            ],
            children_map: {
                let mut m = std::collections::HashMap::new();
                m.insert("0:1".into(), vec!["1:1".into()]);
                m
            },
            image_hashes: vec![],
            thumbnail: vec![],
        }
    }

    #[test]
    fn builds_scene_graph() {
        let doc = make_test_doc();
        let sg = build_scene_graph(&doc);
        assert_eq!(sg.pages.len(), 1);
        assert_eq!(sg.trees.len(), 1);
        assert_eq!(sg.trees[0].nodes.len(), 1);
        assert_eq!(sg.trees[0].nodes[0].name, "Frame 1");
        assert!((sg.trees[0].nodes[0].world_transform.m02 - 10.0).abs() < 0.01);
    }
}