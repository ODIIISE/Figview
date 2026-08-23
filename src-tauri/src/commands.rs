use crate::state::{AppState, DocumentEntry, DocumentSource, TabInfo};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

#[tauri::command]
pub async fn take_startup_path(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.pending_path.lock().unwrap().take())
}

#[tauri::command]
pub async fn open_file(
    path: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let doc = fig_parser::parse_file(&path).map_err(|e| e.to_string())?;
    let id = uuid();
    let name = doc.file_name.clone();
    let mut json = serde_json::to_value(&doc).map_err(|e| e.to_string())?;
    json.as_object_mut()
        .ok_or_else(|| "Serialized document was not an object".to_string())?
        .insert("document_id".into(), serde_json::Value::String(id.clone()));
    state.tabs.lock().unwrap().push(TabInfo {
        document_id: id.clone(),
        path: path.clone(),
        name,
    });
    *state.active_document.lock().unwrap() = Some(id.clone());
    state.documents.lock().unwrap().insert(
        id,
        DocumentEntry {
            source: DocumentSource::Path(path),
        },
    );
    Ok(json)
}

#[tauri::command]
pub async fn open_file_bytes(
    data: Vec<u8>,
    name: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let doc = fig_parser::parse_bytes(&data).map_err(|e| e.to_string())?;
    let id = uuid();
    let display_name = doc.file_name.clone();
    let mut json = serde_json::to_value(&doc).map_err(|e| e.to_string())?;
    json.as_object_mut()
        .ok_or_else(|| "Serialized document was not an object".to_string())?
        .insert("document_id".into(), serde_json::Value::String(id.clone()));
    state.tabs.lock().unwrap().push(TabInfo {
        document_id: id.clone(),
        path: name,
        name: display_name,
    });
    *state.active_document.lock().unwrap() = Some(id.clone());
    state.documents.lock().unwrap().insert(
        id,
        DocumentEntry {
            source: DocumentSource::Bytes(Arc::new(data)),
        },
    );
    Ok(json)
}

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

#[tauri::command]
pub async fn get_documents(state: State<'_, AppState>) -> Result<Vec<TabInfo>, String> {
    Ok(state.tabs.lock().unwrap().clone())
}

fn uuid() -> String {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("doc-{:x}-{:x}", t.as_secs(), t.subsec_nanos())
}
