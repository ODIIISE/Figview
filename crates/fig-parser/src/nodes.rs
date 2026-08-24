//! Extractors for typed fields from decoded Kiwi values.
//!
//! These navigate `kiwi_schema::Value` trees directly — no serde_json
//! intermediate — which removes an entire allocation-heavy conversion pass.

use crate::types::*;
use kiwi_schema::Value;
use std::collections::HashMap;

/// Object field lookup on a kiwi value.
pub fn get<'a>(v: &'a Value, name: &str) -> Option<&'a Value<'a>> {
    match v {
        Value::Object(_, fields) => fields.get(name),
        _ => None,
    }
}

/// Field lookup returning a string-ish value (owned string or enum variant).
pub fn as_str_of<'a>(obj: &'a Value<'a>, name: &str) -> Option<&'a str> {
    as_str_of_v(get(obj, name)?)
}

pub fn as_str_of_v<'a>(v: &'a Value<'a>) -> Option<&'a str> {
    as_str(v)
}

pub fn as_f64_v(v: &Value) -> Option<f64> {
    as_f64(v)
}

pub fn as_bool_v(v: &Value) -> Option<bool> {
    as_bool(v)
}

pub fn as_array_ref<'a>(v: &'a Value<'a>) -> Option<&'a Vec<Value<'a>>> {
    as_array(v)
}

pub fn as_byte(v: &Value) -> Option<u8> {
    match v {
        Value::Byte(b) => Some(*b),
        _ => None,
    }
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i) => Some(*i as f64),
        Value::UInt(u) => Some(*u as f64),
        Value::Float(f) => Some(*f as f64),
        Value::Int64(i) => Some(*i as f64),
        Value::UInt64(u) => Some(*u as f64),
        _ => None,
    }
}

fn as_u64(v: &Value) -> Option<u64> {
    match v {
        Value::Byte(b) => Some(*b as u64),
        Value::Int(i) => (*i >= 0).then_some(*i as u64),
        Value::UInt(u) => Some(*u as u64),
        Value::Int64(i) => (*i >= 0).then_some(*i as u64),
        Value::UInt64(u) => Some(*u),
        _ => None,
    }
}

fn as_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

/// String-ish access: owned strings and enum variants both surface as `&str`.
fn as_str<'a>(v: &'a Value<'a>) -> Option<&'a str> {
    match v {
        Value::String(s) => Some(s.as_str()),
        Value::Enum(_, variant) => Some(variant),
        _ => None,
    }
}

fn as_array<'a>(v: &'a Value<'a>) -> Option<&'a Vec<Value<'a>>> {
    match v {
        Value::Array(a) => Some(a),
        _ => None,
    }
}

pub fn get_guid(obj: &Value, name: &str) -> Option<NodeId> {
    let g = get(obj, name)?;
    let sid = get(g, "sessionID")?;
    let lid = get(g, "localID")?;
    Some(NodeId::new(as_u64(sid)? as u32, as_u64(lid)? as u32))
}

pub fn get_vector(obj: &Value, name: &str) -> Option<Vec2> {
    let v = get(obj, name)?;
    let x = get(v, "x")?;
    let y = get(v, "y")?;
    Some(Vec2::new(as_f64(x)? as f32, as_f64(y)? as f32))
}

pub fn get_color(obj: &Value, name: &str) -> Option<Color> {
    let c = get(obj, name)?;
    let num = |n: &str| get(c, n).and_then(as_f64);
    Some(Color {
        r: num("r")? as f32,
        g: num("g")? as f32,
        b: num("b")? as f32,
        a: num("a").unwrap_or(1.0) as f32,
    })
}

pub fn get_matrix(obj: &Value, name: &str) -> Option<Matrix> {
    let m = get(obj, name)?;
    let num = |n: &str| get(m, n).and_then(as_f64).map(|v| v as f32);
    Some(Matrix {
        m00: num("m00")?,
        m01: num("m01")?,
        m02: num("m02")?,
        m10: num("m10")?,
        m11: num("m11")?,
        m12: num("m12")?,
    })
}

pub fn get_node_type(obj: &Value) -> NodeType {
    get(obj, "type")
        .and_then(as_str)
        .map(NodeType::from_str)
        .unwrap_or(NodeType::Unknown)
}

/// The node's schema type name, if present on the object itself.
fn obj_type_name<'a>(obj: &'a Value<'a>) -> Option<&'a str> {
    match obj {
        Value::Enum(_, variant) => Some(variant),
        Value::Object(type_name, _) => Some(type_name),
        _ => None,
    }
}

pub fn get_corner_radii(obj: &Value) -> Option<CornerRadii> {
    let fallback = get(obj, "cornerRadius").and_then(as_f64).unwrap_or(0.0) as f32;
    let read = |name: &str| get(obj, name).and_then(as_f64).map(|v| v as f32);
    let radii = CornerRadii {
        top_left: read("rectangleTopLeftCornerRadius").unwrap_or(fallback),
        top_right: read("rectangleTopRightCornerRadius").unwrap_or(fallback),
        bottom_right: read("rectangleBottomRightCornerRadius").unwrap_or(fallback),
        bottom_left: read("rectangleBottomLeftCornerRadius").unwrap_or(fallback),
    };
    (radii.top_left > 0.0
        || radii.top_right > 0.0
        || radii.bottom_right > 0.0
        || radii.bottom_left > 0.0)
        .then_some(radii)
}

pub fn get_clips_content(obj: &Value) -> bool {
    let is_group_frame = obj_type_name(obj)
        .map(|t| t.ends_with("FRAME"))
        .unwrap_or(false)
        && get(obj, "resizeToFit").and_then(as_bool).unwrap_or(false)
        && get_paints(obj, "fillPaints").is_empty()
        && get_paints(obj, "strokePaints").is_empty()
        && get_paints(obj, "backgroundPaints").is_empty();
    if is_group_frame {
        return false;
    }
    get(obj, "frameMaskDisabled")
        .and_then(as_bool)
        .map(|disabled| !disabled)
        .unwrap_or(true)
}

pub fn get_parent_index(obj: &Value) -> Option<(NodeId, String)> {
    let pi = get(obj, "parentIndex")?;
    let guid = get(pi, "guid")?;
    let pos = get(pi, "position").and_then(as_str).unwrap_or("");
    let sid = get(guid, "sessionID")?;
    let lid = get(guid, "localID")?;
    Some((
        NodeId::new(as_u64(sid)? as u32, as_u64(lid)? as u32),
        pos.to_string(),
    ))
}

/// Byte arrays in kiwi are `Array[Byte]`; hash fields render as hex strings.
fn bytes_as_hex(v: &Value) -> Option<String> {
    let arr = as_array(v)?;
    let mut out = String::with_capacity(arr.len() * 2);
    for b in arr {
        match b {
            Value::Byte(x) => out.push_str(&format!("{:02x}", x)),
            _ => return None,
        }
    }
    Some(out)
}

pub fn get_paints(obj: &Value, name: &str) -> Vec<Paint> {
    let arr = match get(obj, name).and_then(as_array) {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .map(|item| {
            let image_hash = get(item, "image")
                .and_then(|img| get(img, "hash"))
                .and_then(bytes_as_hex);
            let stops = get(item, "stops")
                .and_then(as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| {
                            Some(ColorStop {
                                color: get_color(s, "color")?,
                                position: get(s, "position").and_then(as_f64)? as f32,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Paint {
                paint_type: match as_str(item)
                    .or_else(|| get(item, "type").and_then(as_str))
                    .unwrap_or("SOLID")
                {
                    "SOLID" => PaintType::Solid,
                    "GRADIENT_LINEAR" => PaintType::GradientLinear,
                    "GRADIENT_RADIAL" => PaintType::GradientRadial,
                    "GRADIENT_ANGULAR" => PaintType::GradientAngular,
                    "GRADIENT_DIAMOND" => PaintType::GradientDiamond,
                    "IMAGE" => PaintType::Image,
                    _ => PaintType::Solid,
                },
                color: get_color(item, "color"),
                opacity: get(item, "opacity").and_then(as_f64).unwrap_or(1.0) as f32,
                visible: get(item, "visible").and_then(as_bool).unwrap_or(true),
                stops,
                transform: get_matrix(item, "transform"),
                image_hash,
                gradient_handles: Vec::new(),
            }
        })
        .collect()
}

pub fn get_text_data(obj: &Value) -> Option<TextData> {
    let td = get(obj, "textData")?;
    let str_of = |n: &str| get(td, n).and_then(as_str);
    let num_of = |n: &str, d: f64| get(td, n).and_then(as_f64).unwrap_or(d) as f32;
    Some(TextData {
        characters: str_of("characters").unwrap_or("").to_string(),
        font_family: str_of("fontFamily").map(|s| s.to_string()),
        font_weight: num_of("fontWeight", 400.0),
        font_size: num_of("fontSize", 16.0),
        letter_spacing: num_of("letterSpacing", 0.0),
        line_height: get(td, "lineHeight").and_then(as_f64).map(|v| v as f32),
        fills: get_paints(td, "fills"),
    })
}

pub fn get_effects(obj: &Value) -> Vec<Effect> {
    let arr = match get(obj, "effects").and_then(as_array) {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .map(|item| Effect {
            effect_type: match as_str(item)
                .or_else(|| get(item, "type").and_then(as_str))
                .unwrap_or("DROP_SHADOW")
            {
                "INNER_SHADOW" => EffectType::InnerShadow,
                "DROP_SHADOW" => EffectType::DropShadow,
                "FOREGROUND_BLUR" => EffectType::ForegroundBlur,
                "BACKGROUND_BLUR" => EffectType::BackgroundBlur,
                _ => EffectType::DropShadow,
            },
            color: get_color(item, "color"),
            offset: get_vector(item, "offset").unwrap_or(Vec2::new(0.0, 0.0)),
            radius: get(item, "radius").and_then(as_f64).unwrap_or(0.0) as f32,
            visible: get(item, "visible").and_then(as_bool).unwrap_or(true),
            spread: get(item, "spread").and_then(as_f64).unwrap_or(0.0) as f32,
        })
        .collect()
}

// ── Geometry decoding over kiwi values ──

pub fn get_geometry_paths(value: Option<&Value>, blobs: &[Vec<u8>]) -> Vec<GeometryPath> {
    let Some(arr) = value.and_then(as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|entry| {
            let blob_id = get(entry, "commandsBlob").and_then(as_u64)? as usize;
            let commands = crate::geometry::decode_commands_blob(blobs.get(blob_id)?)?;
            Some(GeometryPath {
                commands,
                winding_rule: crate::geometry::winding_rule(
                    get(entry, "windingRule").and_then(as_str),
                ),
                style_id: get(entry, "styleID").and_then(as_u64).unwrap_or(0) as u32,
            })
        })
        .collect()
}

pub fn get_vector_geometry(value: Option<&Value>, blobs: &[Vec<u8>]) -> Option<VectorGeometry> {
    let data = value?;
    let blob_id = get(data, "vectorNetworkBlob").and_then(as_u64)? as usize;
    let paths = crate::geometry::decode_vector_blob(blobs.get(blob_id)?)?;
    let normalized_size = get(data, "normalizedSize").and(get_vector(data, "normalizedSize"));
    Some(VectorGeometry {
        paths,
        normalized_size,
    })
}
