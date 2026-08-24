//! Extractors for typed fields from decoded Kiwi values.
//!
//! These navigate the lean `FastVal` trees produced by `fastkiwi` directly —
//! no serde_json intermediate — which removes an entire allocation-heavy
//! conversion pass.

use crate::fastkiwi::FastVal;
use crate::types::*;

/// Object field lookup on a kiwi value.
pub fn get<'s, 'm>(v: &'s FastVal<'s, 'm>, name: &str) -> Option<&'s FastVal<'s, 'm>> {
    v.get(name)
}

/// Field lookup returning a string-ish value (owned string or enum variant).
pub fn as_str_of<'s, 'm>(obj: &'s FastVal<'s, 'm>, name: &str) -> Option<&'s str> {
    as_str_v(get(obj, name)?)
}

pub fn as_str_v<'s, 'm>(v: &'s FastVal<'s, 'm>) -> Option<&'s str> {
    v.as_str()
}

pub fn as_f64_v(v: &FastVal) -> Option<f64> {
    v.as_f64()
}

pub fn as_bool_v(v: &FastVal) -> Option<bool> {
    v.as_bool()
}

pub fn as_array_ref<'s, 'm>(v: &'s FastVal<'s, 'm>) -> Option<&'s Vec<FastVal<'s, 'm>>> {
    v.as_array()
}

fn as_f64(v: &FastVal) -> Option<f64> {
    v.as_f64()
}

fn as_bool(v: &FastVal) -> Option<bool> {
    v.as_bool()
}

/// String-ish access: owned strings and enum variants both surface as `&str`.
fn as_str<'s, 'm>(v: &'s FastVal<'s, 'm>) -> Option<&'s str> {
    v.as_str()
}

pub fn get_guid(obj: &FastVal, name: &str) -> Option<NodeId> {
    let g = get(obj, name)?;
    let sid = get(g, "sessionID")?;
    let lid = get(g, "localID")?;
    Some(NodeId::new(sid.as_u64()? as u32, lid.as_u64()? as u32))
}

pub fn get_vector(obj: &FastVal, name: &str) -> Option<Vec2> {
    let v = get(obj, name)?;
    let x = get(v, "x")?;
    let y = get(v, "y")?;
    Some(Vec2::new(x.as_f64()? as f32, y.as_f64()? as f32))
}

pub fn get_color(obj: &FastVal, name: &str) -> Option<Color> {
    let c = get(obj, name)?;
    let num = |n: &str| get(c, n).and_then(as_f64);
    Some(Color {
        r: num("r")? as f32,
        g: num("g")? as f32,
        b: num("b")? as f32,
        a: num("a").unwrap_or(1.0) as f32,
    })
}

pub fn get_matrix(obj: &FastVal, name: &str) -> Option<Matrix> {
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

pub fn get_node_type(obj: &FastVal) -> NodeType {
    as_str_of(obj, "type")
        .map(NodeType::from_str)
        .unwrap_or(NodeType::Unknown)
}

pub fn get_corner_radii(obj: &FastVal) -> Option<CornerRadii> {
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

pub fn get_clips_content(obj: &FastVal) -> bool {
    // Mirrors the original serde_json semantics exactly: a FRAME whose size
    // hugs its children and paints nothing behaves as a group, not a mask.
    let is_group_frame = as_str_of(obj, "type") == Some("FRAME")
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

pub fn get_parent_index(obj: &FastVal) -> Option<(NodeId, String)> {
    let pi = get(obj, "parentIndex")?;
    let guid = get(pi, "guid")?;
    let pos = get(pi, "position").and_then(as_str).unwrap_or("");
    let sid = get(guid, "sessionID")?;
    let lid = get(guid, "localID")?;
    Some((
        NodeId::new(sid.as_u64()? as u32, lid.as_u64()? as u32),
        pos.to_string(),
    ))
}

/// Byte arrays arrive as contiguous borrowed slices; hash fields render as
/// lowercase hex strings.
fn bytes_as_hex(v: &FastVal) -> Option<String> {
    let bytes = v.as_bytes()?;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    Some(out)
}

pub fn get_paints(obj: &FastVal, name: &str) -> Vec<Paint> {
    let arr = match get(obj, name).and_then(as_array_ref) {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .map(|item| {
            let image_hash = get(item, "image")
                .and_then(|img| get(img, "hash"))
                .and_then(bytes_as_hex);
            let stops = get(item, "stops")
                .and_then(as_array_ref)
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
                paint_type: match item
                    .as_str()
                    .or_else(|| as_str_of(item, "type"))
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

pub fn get_text_data(obj: &FastVal) -> Option<TextData> {
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

pub fn get_effects(obj: &FastVal) -> Vec<Effect> {
    let arr = match get(obj, "effects").and_then(as_array_ref) {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .map(|item| Effect {
            effect_type: match item
                .as_str()
                .or_else(|| as_str_of(item, "type"))
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

pub fn get_geometry_paths(value: Option<&FastVal>, blobs: &[Vec<u8>]) -> Vec<GeometryPath> {
    let Some(arr) = value.and_then(as_array_ref) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|entry| {
            let blob_id = get(entry, "commandsBlob").and_then(|v| v.as_u64())? as usize;
            let commands = crate::geometry::decode_commands_blob(blobs.get(blob_id)?)?;
            Some(GeometryPath {
                commands,
                winding_rule: crate::geometry::winding_rule(
                    get(entry, "windingRule").and_then(|v| v.as_str()),
                ),
                style_id: get(entry, "styleID").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            })
        })
        .collect()
}

pub fn get_vector_geometry(value: Option<&FastVal>, blobs: &[Vec<u8>]) -> Option<VectorGeometry> {
    let data = value?;
    let blob_id = get(data, "vectorNetworkBlob").and_then(|v| v.as_u64())? as usize;
    let paths = crate::geometry::decode_vector_blob(blobs.get(blob_id)?)?;
    let normalized_size = get(data, "normalizedSize").and(get_vector(data, "normalizedSize"));
    Some(VectorGeometry {
        paths,
        normalized_size,
    })
}
