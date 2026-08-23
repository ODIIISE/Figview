use crate::state::{AppState, TabInfo};
use tauri::State;
use std::time::{SystemTime, UNIX_EPOCH};

#[tauri::command]
pub async fn open_file(path: String, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let doc = fig_parser::parse_file(&path).map_err(|e| e.to_string())?;
    let id = uuid();
    let name = doc.file_name.clone();
    let json = serde_json::to_value(&doc).map_err(|e| e.to_string())?;
    state.tabs.lock().unwrap().push(TabInfo { document_id: id.clone(), path: path.clone(), name });
    *state.active_document.lock().unwrap() = Some(id.clone());
    state.documents.lock().unwrap().insert(id, (doc, path));
    Ok(json)
}

#[tauri::command]
pub async fn open_file_bytes(data: Vec<u8>, name: String, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let doc = fig_parser::parse_bytes(&data).map_err(|e| e.to_string())?;
    let id = uuid();
    let display_name = doc.file_name.clone();
    let json = serde_json::to_value(&doc).map_err(|e| e.to_string())?;
    state.tabs.lock().unwrap().push(TabInfo { document_id: id.clone(), path: name.clone(), name: display_name });
    *state.active_document.lock().unwrap() = Some(id.clone());
    state.documents.lock().unwrap().insert(id, (doc, name));
    Ok(json)
}

#[tauri::command]
pub async fn close_file(document_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.documents.lock().unwrap().remove(&document_id);
    let mut tabs = state.tabs.lock().unwrap();
    tabs.retain(|t| t.document_id != document_id);
    let mut active = state.active_document.lock().unwrap();
    if *active == Some(document_id.clone()) { *active = tabs.first().map(|t| t.document_id.clone()); }
    Ok(())
}

#[tauri::command]
pub async fn get_image(document_id: String, hash: String, state: State<'_, AppState>) -> Result<Vec<u8>, String> {
    let docs = state.documents.lock().unwrap();
    let path = docs.get(&document_id).ok_or("Not found")?.1.clone();
    drop(docs);
    fig_parser::archive::open_archive(&path).map_err(|e| e.to_string())?
        .images.get(&hash).cloned().ok_or("Image not found".into())
}

#[tauri::command]
pub async fn get_documents(state: State<'_, AppState>) -> Result<Vec<TabInfo>, String> {
    Ok(state.tabs.lock().unwrap().clone())
}

fn uuid() -> String {
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("doc-{:x}-{:x}", t.as_secs(), t.subsec_nanos())
}