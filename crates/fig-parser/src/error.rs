//! Error types for the `.fig` parsing pipeline.

/// Errors that can occur while parsing a `.fig` file.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Failed to open ZIP archive: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Schema decode failed: {0}")]
    SchemaDecode(String),

    #[error("Message decode failed: {0}")]
    MessageDecode(String),

    #[error("Invalid or missing prelude: expected 'fig-kiwi', got '{0}'")]
    InvalidPrelude(String),

    #[error("Zstandard decompression failed: {0}")]
    Zstd(String),

    #[error("Deflate decompression failed: {0}")]
    Deflate(String),

    #[error("Missing required ZIP entry: {0}")]
    MissingEntry(String),

    #[error("JSON parse error in meta.json: {0}")]
    MetaJson(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

// kiwi_schema uses () for all errors
impl From<()> for ParseError {
    fn from(_: ()) -> Self {
        ParseError::SchemaDecode("kiwi-schema returned an error".to_string())
    }
}