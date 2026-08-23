use fig_parser::types::FigDocument;
use fig_renderer::scene::SceneGraph;
use fig_renderer::WgpuRenderer;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum DocumentSource {
    Path(String),
    Bytes(Arc<Vec<u8>>),
}

pub struct DocumentEntry {
    /// Kept so documents can be re-read from their origin later.
    #[allow(dead_code)]
    pub source: DocumentSource,
    pub document: FigDocument,
    pub scene_graph: SceneGraph,
    /// Raw (compressed) image bytes keyed by Figma image hash, extracted
    /// once at open time so images never require re-reading the archive.
    pub image_bytes: Arc<HashMap<String, Vec<u8>>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TabInfo {
    pub document_id: String,
    pub path: String,
    pub name: String,
    pub active_page: usize,
}

/// Lightweight layer tree node sent to the frontend.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LayerTreeNode {
    pub id: String,
    pub name: String,
    pub node_type: String,
    pub visible: bool,
    pub opacity: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<BoundsDto>,
    pub children: Vec<LayerTreeNode>,
}

/// Axis-aligned world-space bounding box.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct BoundsDto {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A text node rendered by the frontend overlay.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TextItem {
    pub id: String,
    pub characters: String,
    pub font_family: Option<String>,
    pub font_size: f32,
    pub font_weight: f32,
    pub line_height: Option<f32>,
    pub color: [f32; 4],
    pub opacity: f32,
    /// World-space position of the first glyph baseline origin box.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Metadata for a loaded document (no scene data).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentMetadata {
    pub document_id: String,
    pub file_name: String,
    pub pages: Vec<PageMeta>,
    pub node_count: usize,
    pub image_count: usize,
    pub version: u32,
    pub schema_def_count: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PageMeta {
    pub id: String,
    pub name: String,
}

/// Properties of a selected node for the properties panel.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeProperties {
    pub id: String,
    pub name: String,
    pub node_type: String,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub opacity: f32,
    pub visible: bool,
    pub fill_count: usize,
    pub stroke_weight: f32,
    pub corner_radius: Option<f32>,
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub font_weight: f32,
    pub text_characters: Option<String>,
}

pub struct AppState {
    pub documents: Mutex<HashMap<String, DocumentEntry>>,
    pub tabs: Mutex<Vec<TabInfo>>,
    pub active_document: Mutex<Option<String>>,
    pub pending_path: Mutex<Option<String>>,
    pub renderer: Mutex<Option<WgpuRenderer>>,
}

impl AppState {
    pub fn new(pending_path: Option<String>) -> Self {
        Self {
            documents: Mutex::new(HashMap::new()),
            tabs: Mutex::new(Vec::new()),
            active_document: Mutex::new(None),
            pending_path: Mutex::new(pending_path),
            renderer: Mutex::new(None),
        }
    }
}

fn bounds_dto(rect: &fig_renderer::transforms::Rect) -> Option<BoundsDto> {
    if rect.is_empty() {
        return None;
    }
    Some(BoundsDto {
        x: rect.min_x,
        y: rect.min_y,
        w: rect.width(),
        h: rect.height(),
    })
}

/// Build a lightweight layer tree from a scene graph page.
pub fn build_layer_tree(scene_graph: &SceneGraph, page_index: usize) -> Vec<LayerTreeNode> {
    let tree = match scene_graph.trees.get(page_index) {
        Some(t) => t,
        None => return Vec::new(),
    };

    tree.root_indices
        .iter()
        .filter_map(|&idx| build_layer_node(tree, idx))
        .collect()
}

fn build_layer_node(
    tree: &fig_renderer::scene::RenderTree,
    node_idx: usize,
) -> Option<LayerTreeNode> {
    let node = tree.nodes.get(node_idx)?;

    let node_id = format!("{}:{}", node.id.session_id, node.id.local_id);
    let children: Vec<LayerTreeNode> = node
        .children
        .iter()
        .filter_map(|&child_idx| build_layer_node(tree, child_idx))
        .collect();

    Some(LayerTreeNode {
        id: node_id,
        name: node.name.clone(),
        node_type: format!("{:?}", node.node_type),
        visible: node.visible,
        opacity: node.opacity,
        bounds: node.bounds.and_then(|b| bounds_dto(&b)),
        children,
    })
}
