use crate::state::{
    self, AppState, DocumentEntry, DocumentMetadata, DocumentSource, TabInfo, TextItem,
};
use fig_renderer::renderer::{DecodedImage, RenderCommand, Renderer};
use fig_renderer::scene::build_scene_graph;
use fig_renderer::WgpuRenderer;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use tauri::State;

static DOC_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_doc_id() -> String {
    format!("doc-{}", DOC_COUNTER.fetch_add(1, Ordering::Relaxed) + 1)
}

/// Decode raw image bytes into RGBA pixels for GPU upload.
fn decode_images(raw: &HashMap<String, Vec<u8>>) -> HashMap<String, DecodedImage> {
    let mut out = HashMap::new();
    for (hash, bytes) in raw {
        // Skip obviously wrong payloads.
        if bytes.len() < 8 || bytes.len() > 64 * 1024 * 1024 {
            continue;
        }
        if let Ok(img) = image::load_from_memory(bytes) {
            let rgba = img.to_rgba8();
            out.insert(
                hash.clone(),
                DecodedImage {
                    width: rgba.width(),
                    height: rgba.height(),
                    rgba: rgba.into_raw(),
                },
            );
        }
    }
    out
}

/// Extract raw image bytes referenced by the document from its archive.
fn extract_image_bytes(entry_source: &DocumentSource) -> HashMap<String, Vec<u8>> {
    let open = || -> Result<fig_parser::archive::FigArchive, String> {
        match entry_source {
            DocumentSource::Path(path) => {
                fig_parser::archive::open_archive(path).map_err(|e| e.to_string())
            }
            DocumentSource::Bytes(data) => {
                fig_parser::archive::read_archive(std::io::Cursor::new(data.as_ref()))
                    .map_err(|e| e.to_string())
            }
        }
    };
    match open() {
        Ok(archive) => archive.images,
        Err(_) => HashMap::new(),
    }
}

async fn load_document(
    source: DocumentSource,
    display_path: String,
    state: State<'_, AppState>,
) -> Result<DocumentMetadata, String> {
    // Parsing + scene building are CPU-heavy: keep them off the async workers.
    let src = source.clone();
    let (doc, image_bytes) = tokio::task::spawn_blocking(move || {
        let doc = match &src {
            DocumentSource::Path(path) => {
                fig_parser::parse_file(path).map_err(|e| e.to_string())?
            }
            DocumentSource::Bytes(data) => {
                fig_parser::parse_bytes(data.as_ref()).map_err(|e| e.to_string())?
            }
        };
        let image_bytes = extract_image_bytes(&src);
        Ok::<_, String>((doc, image_bytes))
    })
    .await
    .map_err(|e| e.to_string())??;

    let id = next_doc_id();
    let name = doc.file_name.clone();
    let image_count = doc.image_hashes.len();
    let node_count = doc.nodes.len();
    let version = doc.header.version;
    let schema_def_count = doc.header.schema_def_count;

    let pages: Vec<crate::state::PageMeta> = doc
        .pages
        .iter()
        .map(|p| crate::state::PageMeta {
            id: format!("{}:{}", p.id.session_id, p.id.local_id),
            name: p.name.clone(),
        })
        .collect();

    let scene_graph = build_scene_graph(&doc);

    // Hand the scene to the renderer (if initialized) plus decoded textures.
    let decoded = decode_images(&image_bytes);
    {
        let mut renderer = state.renderer.lock().unwrap();
        if let Some(ref mut r) = *renderer {
            r.handle_command(RenderCommand::LoadScene(scene_graph.clone()))?;
            r.handle_command(RenderCommand::LoadImages(decoded))?;
        }
    }

    state.documents.lock().unwrap().insert(
        id.clone(),
        DocumentEntry {
            source,
            document: doc,
            scene_graph,
            image_bytes: std::sync::Arc::new(image_bytes),
        },
    );

    state.tabs.lock().unwrap().push(TabInfo {
        document_id: id.clone(),
        path: display_path,
        name: name.clone(),
        active_page: 0,
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

/// Open a .fig file from disk. Returns lightweight metadata only.
#[tauri::command]
pub async fn open_file(
    path: String,
    state: State<'_, AppState>,
) -> Result<DocumentMetadata, String> {
    load_document(DocumentSource::Path(path.clone()), path, state).await
}

/// Open a .fig file from bytes (fallback for non-path drops). Returns metadata only.
#[tauri::command]
pub async fn open_file_bytes(
    data: Vec<u8>,
    name: String,
    state: State<'_, AppState>,
) -> Result<DocumentMetadata, String> {
    let bytes = std::sync::Arc::new(data);
    load_document(DocumentSource::Bytes(bytes), name, state).await
}

/// Close a document and release all backend resources for it.
#[tauri::command]
pub async fn close_file(document_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.documents.lock().unwrap().remove(&document_id);

    let was_active = { *state.active_document.lock().unwrap() == Some(document_id.clone()) };

    let remaining = {
        let mut tabs = state.tabs.lock().unwrap();
        tabs.retain(|t| t.document_id != document_id);
        tabs.clone()
    };

    if was_active {
        match remaining.first() {
            Some(next) => {
                let next_id = next.document_id.clone();
                switch_to(&state, &next_id)?;
            }
            None => {
                *state.active_document.lock().unwrap() = None;
                let mut renderer = state.renderer.lock().unwrap();
                if let Some(ref mut r) = *renderer {
                    r.handle_command(RenderCommand::ClearScene)?;
                }
            }
        }
    }
    Ok(())
}

/// Switch the active document, reloading its scene into the renderer.
#[tauri::command]
pub async fn switch_document(
    document_id: String,
    page_index: usize,
    state: State<'_, AppState>,
) -> Result<(), String> {
    switch_to(&state, &document_id)?;
    // Restore the requested page without resetting the camera twice.
    let mut renderer = state.renderer.lock().unwrap();
    if let Some(ref mut r) = *renderer {
        r.handle_command(RenderCommand::SetPage(page_index))?;
    }
    Ok(())
}

fn switch_to(state: &State<'_, AppState>, document_id: &str) -> Result<(), String> {
    let documents = state.documents.lock().unwrap();
    let entry = documents.get(document_id).ok_or("Document not found")?;
    let scene_graph = entry.scene_graph.clone();
    let decoded = decode_images(&entry.image_bytes);
    drop(documents);

    let mut renderer = state.renderer.lock().unwrap();
    if let Some(ref mut r) = *renderer {
        r.handle_command(RenderCommand::LoadScene(scene_graph))?;
        r.handle_command(RenderCommand::LoadImages(decoded))?;
    }
    drop(renderer);

    *state.active_document.lock().unwrap() = Some(document_id.to_string());
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
) -> Result<Vec<crate::state::LayerTreeNode>, String> {
    let docs = state.documents.lock().unwrap();
    let entry = docs.get(&document_id).ok_or("Document not found")?;
    Ok(state::build_layer_tree(&entry.scene_graph, page_index))
}

/// Text items for the frontend overlay renderer.
#[tauri::command]
pub async fn get_page_text(
    document_id: String,
    page_index: usize,
    state: State<'_, AppState>,
) -> Result<Vec<TextItem>, String> {
    let docs = state.documents.lock().unwrap();
    let entry = docs.get(&document_id).ok_or("Document not found")?;
    let tree = match entry.scene_graph.trees.get(page_index) {
        Some(t) => t,
        None => return Ok(Vec::new()),
    };

    let mut items = Vec::new();
    collect_text_items(tree, &tree.root_indices, 1.0, &mut items);
    Ok(items)
}

fn collect_text_items(
    tree: &fig_renderer::scene::RenderTree,
    indices: &[usize],
    parent_opacity: f32,
    out: &mut Vec<TextItem>,
) {
    for &idx in indices {
        let Some(node) = tree.nodes.get(idx) else {
            continue;
        };
        if !node.visible {
            continue;
        }
        let opacity = parent_opacity * node.opacity.clamp(0.0, 1.0);

        if let (Some(text), Some(bounds)) = (&node.text_data, node.bounds) {
            if !text.characters.trim().is_empty() && !bounds.is_empty() {
                let color = text_color(node);
                out.push(TextItem {
                    id: format!("{}:{}", node.id.session_id, node.id.local_id),
                    characters: text.characters.clone(),
                    font_family: text.font_family.clone(),
                    font_size: text.font_size.max(1.0),
                    font_weight: text.font_weight,
                    line_height: text.line_height,
                    color,
                    opacity,
                    x: bounds.min_x,
                    y: bounds.min_y,
                    w: bounds.width(),
                    h: bounds.height(),
                });
            }
        }

        collect_text_items(tree, &node.children, opacity, out);
    }
}

fn text_color(node: &fig_renderer::scene::RenderNode) -> [f32; 4] {
    for paint in node.fill_paints.iter().filter(|p| p.visible) {
        if let Some(c) = paint.color {
            return [c.r, c.g, c.b, c.a];
        }
        if let Some(stop) = paint.stops.first() {
            let c = stop.color;
            return [c.r, c.g, c.b, c.a];
        }
    }
    [0.0, 0.0, 0.0, 1.0]
}

/// Hit-test a world-space point against node bounds (front-most wins).
#[tauri::command]
pub async fn hit_test(
    document_id: String,
    page_index: usize,
    world_x: f32,
    world_y: f32,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let docs = state.documents.lock().unwrap();
    let entry = docs.get(&document_id).ok_or("Document not found")?;
    let tree = match entry.scene_graph.trees.get(page_index) {
        Some(t) => t,
        None => return Ok(None),
    };

    // Walk front-most first: reverse pre-order gives later siblings priority.
    let mut stack: Vec<usize> = tree.root_indices.iter().rev().cloned().collect();
    while let Some(idx) = stack.pop() {
        let Some(node) = tree.nodes.get(idx) else {
            continue;
        };
        if !node.visible {
            continue;
        }
        if let Some(b) = node.bounds {
            if world_x >= b.min_x && world_x <= b.max_x && world_y >= b.min_y && world_y <= b.max_y
            {
                // Prefer deepest child containing the point.
                let mut best = format!("{}:{}", node.id.session_id, node.id.local_id);
                let mut child_stack: Vec<usize> = node.children.iter().rev().cloned().collect();
                while let Some(ci) = child_stack.pop() {
                    if let Some(child) = tree.nodes.get(ci) {
                        if !child.visible {
                            continue;
                        }
                        if let Some(cb) = child.bounds {
                            if world_x >= cb.min_x
                                && world_x <= cb.max_x
                                && world_y >= cb.min_y
                                && world_y <= cb.max_y
                            {
                                best = format!("{}:{}", child.id.session_id, child.id.local_id);
                                child_stack.extend(child.children.iter().rev().cloned());
                                continue;
                            }
                        }
                        child_stack.extend(child.children.iter().rev().cloned());
                    }
                }
                return Ok(Some(best));
            }
        }
        stack.extend(node.children.iter().rev().cloned());
    }
    Ok(None)
}

/// Get properties for a specific node.
#[tauri::command]
pub async fn get_node_properties(
    document_id: String,
    node_id: String,
    state: State<'_, AppState>,
) -> Result<crate::state::NodeProperties, String> {
    let docs = state.documents.lock().unwrap();
    let entry = docs.get(&document_id).ok_or("Document not found")?;

    let node = entry
        .document
        .nodes
        .iter()
        .find(|n| {
            n.guid
                .as_ref()
                .map(|g| format!("{}:{}", g.session_id, g.local_id) == node_id)
                .unwrap_or(false)
        })
        .ok_or("Node not found")?;

    Ok(crate::state::NodeProperties {
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

    // If a document was opened before the renderer existed, push it now.
    {
        let active_id = state.active_document.lock().unwrap().clone();
        if let Some(id) = active_id {
            let documents = state.documents.lock().unwrap();
            if let Some(entry) = documents.get(&id) {
                let scene_graph = entry.scene_graph.clone();
                let decoded = decode_images(&entry.image_bytes);
                let mut renderer = state.renderer.lock().unwrap();
                if let Some(ref mut r) = *renderer {
                    r.handle_command(RenderCommand::LoadScene(scene_graph))?;
                    r.handle_command(RenderCommand::LoadImages(decoded))?;
                }
            }
        }
    }

    let mut renderer = state.renderer.lock().unwrap();
    if let Some(ref mut r) = *renderer {
        r.initialize(width.max(1), height.max(1))?;
    }
    Ok(())
}

/// Render a frame and return raw RGBA bytes over the binary IPC channel.
#[tauri::command]
pub async fn render_frame(state: State<'_, AppState>) -> Result<tauri::ipc::Response, String> {
    let output = {
        let mut renderer = state.renderer.lock().unwrap();
        let r = renderer.as_mut().ok_or("Renderer not initialized")?;
        r.render()?
    };
    Ok(tauri::ipc::Response::new(output.pixels))
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

/// Get the current camera state.
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

/// Get image bytes for a specific hash (served from the open-time cache).
#[tauri::command]
pub async fn get_image(
    document_id: String,
    hash: String,
    state: State<'_, AppState>,
) -> Result<Vec<u8>, String> {
    let docs = state.documents.lock().unwrap();
    let entry = docs.get(&document_id).ok_or("Document not found")?;
    entry
        .image_bytes
        .get(&hash)
        .cloned()
        .ok_or_else(|| "Image not found".to_string())
}
