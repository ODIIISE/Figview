use crate::state::{
    self, AppState, DocumentEntry, DocumentMetadata, DocumentSource, LayerTreeNode, NodeProperties,
    PageMeta, TabInfo,
};
use fig_renderer::renderer::{RenderCommand, Renderer};
use fig_renderer::scene::build_scene_graph;
use fig_renderer::WgpuRenderer;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

/// Open a .fig file from disk. Returns lightweight metadata only.
#[tauri::command]
pub async fn open_file(
    path: String,
    state: State<'_, AppState>,
) -> Result<DocumentMetadata, String> {
    let doc = fig_parser::parse_file(&path).map_err(|e| e.to_string())?;
    let id = uuid();
    let name = doc.file_name.clone();
    let image_count = doc.image_hashes.len();
    let node_count = doc.nodes.len();
    let version = doc.header.version;
    let schema_def_count = doc.header.schema_def_count;

    let pages: Vec<PageMeta> = doc
        .pages
        .iter()
        .map(|p| PageMeta {
            id: format!("{}:{}", p.id.session_id, p.id.local_id),
            name: p.name.clone(),
        })
        .collect();

    // Build scene graph
    let scene_graph = build_scene_graph(&doc);

    // Initialize renderer with scene
    {
        let mut renderer = state.renderer.lock().unwrap();
        if let Some(ref mut r) = *renderer {
            let _ = r.handle_command(RenderCommand::LoadScene(scene_graph.clone()));
        }
    }

    // Store document and scene graph
    state.documents.lock().unwrap().insert(
        id.clone(),
        DocumentEntry {
            source: DocumentSource::Path(path.clone()),
            document: doc,
            scene_graph,
        },
    );

    state.tabs.lock().unwrap().push(TabInfo {
        document_id: id.clone(),
        path: path.clone(),
        name: name.clone(),
    });
    *state.active_document.lock().unwrap() = Some(id.clone());

    Ok(DocumentMetadata {
        document_id: id,
        file_name: name,
        pages,
        node_count,
        image_count,
        version,
        schema_def_count,
    })
}

/// Open a .fig file from bytes (drag-and-drop). Returns lightweight metadata only.
#[tauri::command]
pub async fn open_file_bytes(
    data: Vec<u8>,
    name: String,
    state: State<'_, AppState>,
) -> Result<DocumentMetadata, String> {
    let doc = fig_parser::parse_bytes(&data).map_err(|e| e.to_string())?;
    let id = uuid();
    let display_name = doc.file_name.clone();
    let image_count = doc.image_hashes.len();
    let node_count = doc.nodes.len();
    let version = doc.header.version;
    let schema_def_count = doc.header.schema_def_count;

    let pages: Vec<PageMeta> = doc
        .pages
        .iter()
        .map(|p| PageMeta {
            id: format!("{}:{}", p.id.session_id, p.id.local_id),
            name: p.name.clone(),
        })
        .collect();

    let scene_graph = build_scene_graph(&doc);

    {
        let mut renderer = state.renderer.lock().unwrap();
        if let Some(ref mut r) = *renderer {
            let _ = r.handle_command(RenderCommand::LoadScene(scene_graph.clone()));
        }
    }

    state.documents.lock().unwrap().insert(
        id.clone(),
        DocumentEntry {
            source: DocumentSource::Bytes(Arc::new(data)),
            document: doc,
            scene_graph,
        },
    );

    state.tabs.lock().unwrap().push(TabInfo {
        document_id: id.clone(),
        path: name.clone(),
        name: display_name.clone(),
    });
    *state.active_document.lock().unwrap() = Some(id.clone());

    Ok(DocumentMetadata {
        document_id: id,
        file_name: display_name,
        pages,
        node_count,
        image_count,
        version,
        schema_def_count,
    })
}

/// Close a document.
#[tauri::command]
pub async fn close_file(document_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.documents.lock().unwrap().remove(&document_id);
    let mut tabs = state.tabs.lock().unwrap();
    tabs.retain(|t| t.document_id != document_id);
    let mut active = state.active_document.lock().unwrap();
    if *active == Some(document_id) {
        *active = tabs.first().map(|t| t.document_id.clone());
    }
    Ok(())
}

/// Get the list of open tabs.
#[tauri::command]
pub async fn get_documents(state: State<'_, AppState>) -> Result<Vec<TabInfo>, String> {
    Ok(state.tabs.lock().unwrap().clone())
}

/// Get the startup file path (if any).
#[tauri::command]
pub async fn take_startup_path(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.pending_path.lock().unwrap().take())
}

/// Get layer tree for a specific page.
#[tauri::command]
pub async fn get_layer_tree(
    document_id: String,
    page_index: usize,
    state: State<'_, AppState>,
) -> Result<Vec<LayerTreeNode>, String> {
    let docs = state.documents.lock().unwrap();
    let entry = docs.get(&document_id).ok_or("Document not found")?;
    Ok(state::build_layer_tree(&entry.scene_graph, page_index))
}

/// Get properties for a specific node.
#[tauri::command]
pub async fn get_node_properties(
    document_id: String,
    node_id: String,
    state: State<'_, AppState>,
) -> Result<NodeProperties, String> {
    let docs = state.documents.lock().unwrap();
    let entry = docs.get(&document_id).ok_or("Document not found")?;

    // Find the node in the flat list
    let node = entry
        .document
        .nodes
        .iter()
        .find(|n| {
            if let Some(ref guid) = n.guid {
                format!("{}:{}", guid.session_id, guid.local_id) == node_id
            } else {
                false
            }
        })
        .ok_or("Node not found")?;

    Ok(NodeProperties {
        id: node_id,
        name: node.name.clone(),
        node_type: format!("{:?}", node.node_type),
        width: node.size.map(|s| s.x),
        height: node.size.map(|s| s.y),
        x: node.transform.map(|t| t.m02),
        y: node.transform.map(|t| t.m12),
        opacity: node.opacity,
        visible: node.visible,
        fill_count: node.fill_paints.len(),
        stroke_weight: node.stroke_weight,
        corner_radius: node.corner_radius,
        font_family: node
            .text_data
            .as_ref()
            .and_then(|td| td.font_family.clone()),
        font_size: node.text_data.as_ref().map(|td| td.font_size),
        font_weight: node
            .text_data
            .as_ref()
            .map(|td| td.font_weight)
            .unwrap_or(400.0),
        text_characters: node.text_data.as_ref().map(|td| td.characters.clone()),
    })
}

// ── Renderer control commands ──

/// Initialize the GPU renderer.
#[tauri::command]
pub async fn init_renderer(
    width: u32,
    height: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let needs_init = {
        let renderer = state.renderer.lock().unwrap();
        renderer.is_none()
    };

    if needs_init {
        let wgpu_renderer = WgpuRenderer::new()
            .await
            .map_err(|e| format!("Failed to create renderer: {}", e))?;
        let mut renderer = state.renderer.lock().unwrap();
        *renderer = Some(wgpu_renderer);
    }

    let mut renderer = state.renderer.lock().unwrap();
    if let Some(ref mut r) = *renderer {
        r.initialize(width, height)?;
    }
    Ok(())
}

/// Render a frame and return the pixel data as a base64-encoded PNG or raw bytes.
#[tauri::command]
pub async fn render_frame(state: State<'_, AppState>) -> Result<Vec<u8>, String> {
    let mut renderer = state.renderer.lock().unwrap();
    let r = renderer.as_mut().ok_or("Renderer not initialized")?;
    let output = r.render()?;
    Ok(output.pixels)
}

/// Set the active page.
#[tauri::command]
pub async fn set_page(page_index: usize, state: State<'_, AppState>) -> Result<(), String> {
    let mut renderer = state.renderer.lock().unwrap();
    let r = renderer.as_mut().ok_or("Renderer not initialized")?;
    r.handle_command(RenderCommand::SetPage(page_index))?;
    Ok(())
}

/// Set zoom level.
#[tauri::command]
pub async fn set_zoom(zoom: f32, state: State<'_, AppState>) -> Result<(), String> {
    let mut renderer = state.renderer.lock().unwrap();
    let r = renderer.as_mut().ok_or("Renderer not initialized")?;
    r.handle_command(RenderCommand::SetZoom(zoom))?;
    Ok(())
}

/// Zoom centered on a point.
#[tauri::command]
pub async fn zoom_at(
    screen_x: f32,
    screen_y: f32,
    zoom: f32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut renderer = state.renderer.lock().unwrap();
    let r = renderer.as_mut().ok_or("Renderer not initialized")?;
    r.handle_command(RenderCommand::ZoomAt {
        screen_x,
        screen_y,
        zoom,
    })?;
    Ok(())
}

/// Pan by a delta.
#[tauri::command]
pub async fn pan_camera(dx: f32, dy: f32, state: State<'_, AppState>) -> Result<(), String> {
    let mut renderer = state.renderer.lock().unwrap();
    let r = renderer.as_mut().ok_or("Renderer not initialized")?;
    r.handle_command(RenderCommand::Pan { dx, dy })?;
    Ok(())
}

/// Fit the page in viewport.
#[tauri::command]
pub async fn fit_page(padding: f32, state: State<'_, AppState>) -> Result<(), String> {
    let mut renderer = state.renderer.lock().unwrap();
    let r = renderer.as_mut().ok_or("Renderer not initialized")?;
    r.handle_command(RenderCommand::FitPage { padding })?;
    Ok(())
}

/// Select a node.
#[tauri::command]
pub async fn select_node(
    node_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut renderer = state.renderer.lock().unwrap();
    let r = renderer.as_mut().ok_or("Renderer not initialized")?;
    r.handle_command(RenderCommand::SelectNode(node_id))?;
    Ok(())
}

/// Fit a node in viewport.
#[tauri::command]
pub async fn fit_node(node_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut renderer = state.renderer.lock().unwrap();
    let r = renderer.as_mut().ok_or("Renderer not initialized")?;
    r.handle_command(RenderCommand::FitNode(node_id))?;
    Ok(())
}

/// Resize the viewport.
#[tauri::command]
pub async fn resize_viewport(
    width: u32,
    height: u32,
    dpr: f32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut renderer = state.renderer.lock().unwrap();
    let r = renderer.as_mut().ok_or("Renderer not initialized")?;
    r.handle_command(RenderCommand::Resize { width, height, dpr })?;
    Ok(())
}

/// Get the current zoom level.
#[tauri::command]
pub async fn get_zoom(state: State<'_, AppState>) -> Result<f32, String> {
    let renderer = state.renderer.lock().unwrap();
    let r = renderer.as_ref().ok_or("Renderer not initialized")?;
    Ok(r.camera().zoom)
}

/// Get the current camera pan position.
#[tauri::command]
pub async fn get_camera_state(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let renderer = state.renderer.lock().unwrap();
    let r = renderer.as_ref().ok_or("Renderer not initialized")?;
    let cam = r.camera();
    Ok(serde_json::json!({
        "zoom": cam.zoom,
        "pan_x": cam.pan_x,
        "pan_y": cam.pan_y,
        "width": cam.viewport_width,
        "height": cam.viewport_height,
    }))
}

/// Get image bytes for a specific hash (needed for embedded images).
#[tauri::command]
pub async fn get_image(
    document_id: String,
    hash: String,
    state: State<'_, AppState>,
) -> Result<Vec<u8>, String> {
    let source = {
        let documents = state.documents.lock().unwrap();
        documents
            .get(&document_id)
            .map(|entry| entry.source.clone())
    }
    .ok_or_else(|| "Document not found".to_string())?;

    let archive = match source {
        DocumentSource::Path(path) => fig_parser::archive::open_archive(&path),
        DocumentSource::Bytes(data) => {
            fig_parser::archive::read_archive(std::io::Cursor::new(data.as_ref()))
        }
    }
    .map_err(|e| e.to_string())?;

    archive
        .images
        .get(&hash)
        .cloned()
        .ok_or_else(|| "Image not found".to_string())
}

fn uuid() -> String {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("doc-{:x}-{:x}", t.as_secs(), t.subsec_nanos())
}
