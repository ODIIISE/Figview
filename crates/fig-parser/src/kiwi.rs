//! Kiwi binary format decoding via the lean `fastkiwi` decoder.
//!
//! The decoded tree borrows from both the schema (field names) and the
//! message buffer (byte arrays), so extraction into owned types happens
//! inside a single scoped call — nothing intermediate is built.

use crate::error::ParseError;
use crate::fastkiwi;

/// Decode the schema and message, then hand the root value to `extract`
/// while both are alive. Returns the schema definition count plus whatever
/// `extract` produces.
pub fn decode_and_extract<T>(
    schema_bytes: &[u8],
    message_bytes: &[u8],
    extract: impl FnOnce(usize, &fastkiwi::FastVal) -> Result<T, ParseError>,
) -> Result<T, ParseError> {
    let timed = std::env::var("FIG_PARSE_TIMING").is_ok();
    let t = std::time::Instant::now();

    let schema = Schema::decode(schema_bytes)
        .map_err(|_| ParseError::SchemaDecode("failed to decode kiwi schema".into()))?;
    let schema_def_count = schema.defs.len();
    if timed {
        eprintln!("    [kiwi] Schema::decode     {:>8.2?}", t.elapsed());
    }

    fastkiwi::decode_root(&schema, message_bytes).and_then(|root| {
        if timed {
            eprintln!("    [kiwi] decode_root        {:>8.2?}", t.elapsed());
        }
        extract(schema_def_count, &root)
    })
}

use kiwi_schema::Schema;
