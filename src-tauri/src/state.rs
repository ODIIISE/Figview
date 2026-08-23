use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum DocumentSource {
    Path(String),
    Bytes(Arc<Vec<u8>>),
}

pub struct DocumentEntry {
    pub source: DocumentSource,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TabInfo {
    pub document_id: String,
    pub path: String,
    pub name: String,
}

pub struct AppState {
    pub documents: Mutex<HashMap<String, DocumentEntry>>,
    pub tabs: Mutex<Vec<TabInfo>>,
    pub active_document: Mutex<Option<String>>,
    pub pending_path: Mutex<Option<String>>,
}

impl AppState {
    pub fn new(pending_path: Option<String>) -> Self {
        Self {
            documents: Mutex::new(HashMap::new()),
            tabs: Mutex::new(Vec::new()),
            active_document: Mutex::new(None),
            pending_path: Mutex::new(pending_path),
        }
    }
}
