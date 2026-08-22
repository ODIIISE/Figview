use fig_parser::types::FigDocument;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TabInfo { pub document_id: String, pub path: String, pub name: String }

pub struct AppState {
    pub documents: Mutex<HashMap<String, (FigDocument, String)>>,
    pub tabs: Mutex<Vec<TabInfo>>,
    pub active_document: Mutex<Option<String>>,
}
impl AppState {
    pub fn new() -> Self {
        Self { documents: Mutex::new(HashMap::new()), tabs: Mutex::new(Vec::new()), active_document: Mutex::new(None) }
    }
}