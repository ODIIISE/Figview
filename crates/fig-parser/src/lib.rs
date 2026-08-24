//! Parser for Figma `.fig` binary files.

pub mod archive;
pub mod binary;
pub mod document;
pub mod error;
pub mod fastkiwi;
pub mod geometry;
pub mod kiwi;
pub mod nodes;
pub mod types;

use types::FigDocument;

fn parse_archive(archive: archive::FigArchive) -> Result<FigDocument, error::ParseError> {
    let t = std::time::Instant::now();
    let mut stage = |name: &str| {
        let elapsed = t.elapsed();
        if std::env::var("FIG_PARSE_TIMING").is_ok() {
            eprintln!("  [stage] {:<22} {:>8.2?}", name, elapsed);
        }
    };

    let meta: serde_json::Value = if archive.meta_json.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&archive.meta_json)?
    };
    stage("meta.json");

    let decompressed = binary::parse_canvas_fig(&archive.canvas_fig)?;
    stage(&format!("inflate (v{})", decompressed.version));

    let doc = kiwi::decode_and_extract(
        &decompressed.schema_bytes,
        &decompressed.message_bytes,
        |schema_def_count, root| {
            document::build_document(
                root,
                schema_def_count,
                &meta,
                &archive.thumbnail,
                &archive.images,
                &decompressed.prelude,
                decompressed.version,
            )
        },
    )?;
    stage("decode + build");

    Ok(doc)
}

pub fn parse_file(path: &str) -> Result<FigDocument, error::ParseError> {
    parse_archive(archive::open_archive(path)?)
}

pub fn parse_bytes(data: &[u8]) -> Result<FigDocument, error::ParseError> {
    use std::io::Cursor;
    parse_archive(archive::read_archive(Cursor::new(data))?)
}
