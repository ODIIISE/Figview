//! Kiwi binary format decoding via the `kiwi-schema` crate.
//!
//! The decoded message borrows field names from the schema, so extraction
//! into owned types happens inside a single scoped call — no intermediate
//! JSON representation is ever built.

use crate::error::ParseError;
use kiwi_schema::{Schema, Value};

/// Decode the schema and message, then hand the root value to `extract`
/// while both are alive. Returns the schema definition count plus whatever
/// `extract` produces.
pub fn decode_and_extract<T>(
    schema_bytes: &[u8],
    message_bytes: &[u8],
    extract: impl FnOnce(usize, &Value) -> Result<T, ParseError>,
) -> Result<T, ParseError> {
    let timed = std::env::var("FIG_PARSE_TIMING").is_ok();
    let t = std::time::Instant::now();

    let schema = Schema::decode(schema_bytes)?;
    let schema_def_count = schema.defs.len();
    let root_id = if let Some(def) = schema.def("Message") {
        def.index
    } else {
        *schema
            .defs
            .first()
            .map(|d| &d.index)
            .ok_or_else(|| ParseError::SchemaDecode("No root type found in schema".into()))?
    };
    let value = Value::decode(&schema, root_id, message_bytes)?;
    if timed {
        eprintln!("    [kiwi] Value::decode      {:>8.2?}", t.elapsed());
    }

    extract(schema_def_count, &value)
}
