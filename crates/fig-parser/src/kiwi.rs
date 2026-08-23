//! Kiwi binary format decoding via the `kiwi-schema` crate.

use crate::error::ParseError;
use kiwi_schema::{Schema, Value};

pub struct KiwiMessage {
    pub schema_def_count: usize,
    pub root: KiwiRoot,
}

pub struct KiwiRoot {
    pub node_changes: Vec<serde_json::Value>,
    pub blobs: Vec<Vec<u8>>,
}

pub fn decode_schema_and_message(
    schema_bytes: &[u8],
    message_bytes: &[u8],
) -> Result<KiwiMessage, ParseError> {
    let schema = Schema::decode(schema_bytes)?;
    let schema_def_count = schema.defs.len();
    let root_id = find_root_type(&schema)?;
    let value = Value::decode(&schema, root_id, message_bytes)?;
    let root = extract_root(&value);
    Ok(KiwiMessage {
        schema_def_count,
        root,
    })
}

fn find_root_type(schema: &Schema) -> Result<i32, ParseError> {
    if let Some(def) = schema.def("Message") {
        return Ok(def.index);
    }
    if !schema.defs.is_empty() {
        return Ok(schema.defs[0].index);
    }
    Err(ParseError::SchemaDecode(
        "No root type found in schema".into(),
    ))
}

fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Byte(b) => serde_json::Value::Number((*b).into()),
        Value::Int(i) => serde_json::Value::Number((*i).into()),
        Value::UInt(u) => serde_json::Value::Number((*u).into()),
        Value::Float(f) => {
            // serde_json Number::from_f64 preserves all f32 values losslessly
            serde_json::Number::from_f64(*f as f64)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Number(0.into()))
        }
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Int64(i) => serde_json::Value::Number((*i).into()),
        Value::UInt64(u) => serde_json::Value::Number((*u).into()),
        Value::Array(arr) => serde_json::Value::Array(arr.iter().map(value_to_json).collect()),
        Value::Enum(_, variant) => serde_json::Value::String(variant.to_string()),
        Value::Object(_, fields) => {
            let mut map = serde_json::Map::new();
            for (k, v) in fields {
                map.insert(k.to_string(), value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
    }
}

fn extract_root(value: &Value) -> KiwiRoot {
    let node_changes: Vec<serde_json::Value> = match value {
        Value::Object(_, fields) => fields
            .get("nodeChanges")
            .and_then(|v| match v {
                Value::Array(arr) => Some(arr.iter().map(value_to_json).collect()),
                _ => None,
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    let blobs: Vec<Vec<u8>> = match value {
        Value::Object(_, fields) => fields
            .get("blobs")
            .and_then(|v| match v {
                Value::Array(arr) => Some(arr),
                _ => None,
            })
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| match v {
                        Value::Object(_, fields) => fields.get("bytes").and_then(|b| match b {
                            Value::Array(bytes) => Some(
                                bytes
                                    .iter()
                                    .filter_map(|x| match x {
                                        Value::Byte(y) => Some(*y),
                                        _ => None,
                                    })
                                    .collect(),
                            ),
                            _ => None,
                        }),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    KiwiRoot {
        node_changes,
        blobs,
    }
}
