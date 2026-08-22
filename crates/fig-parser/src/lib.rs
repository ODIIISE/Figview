//! Parser for Figma `.fig` binary files.

pub mod archive;
pub mod binary;
pub mod document;
pub mod error;
pub mod kiwi;
pub mod nodes;
pub mod types;

use types::FigDocument;

fn parse_archive(archive: archive::FigArchive) -> Result<FigDocument, error::ParseError> {
    let meta: serde_json::Value = if archive.meta_json.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&archive.meta_json)?
    };
    let decompressed = binary::parse_canvas_fig(&archive.canvas_fig)?;
    let msg = kiwi::decode_schema_and_message(&decompressed.schema_bytes, &decompressed.message_bytes)?;
    document::build_document(msg, &meta, &archive.thumbnail, &archive.images, &decompressed.prelude, decompressed.version)
}

pub fn parse_file(path: &str) -> Result<FigDocument, error::ParseError> {
    parse_archive(archive::open_archive(path)?)
}

pub fn parse_bytes(data: &[u8]) -> Result<FigDocument, error::ParseError> {
    use std::io::Cursor;
    parse_archive(archive::read_archive(Cursor::new(data))?)
}