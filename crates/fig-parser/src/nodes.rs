//! Extractors for typed fields from decoded Kiwi JSON.

use crate::types::*;

pub fn get_guid(obj: &serde_json::Value, name: &str) -> Option<NodeId> {
    let g = obj.get(name)?;
    Some(NodeId::new(
        g.get("sessionID")?.as_u64()? as u32,
        g.get("localID")?.as_u64()? as u32,
    ))
}

pub fn get_vector(obj: &serde_json::Value, name: &str) -> Option<Vec2> {
    let v = obj.get(name)?;
    Some(Vec2::new(v.get("x")?.as_f64()? as f32, v.get("y")?.as_f64()? as f32))
}

pub fn get_color(obj: &serde_json::Value, name: &str) -> Option<Color> {
    let c = obj.get(name)?;
    Some(Color { r: c.get("r")?.as_f64()? as f32, g: c.get("g")?.as_f64()? as f32, b: c.get("b")?.as_f64()? as f32, a: c.get("a")?.as_f64().unwrap_or(1.0) as f32 })
}

pub fn get_matrix(obj: &serde_json::Value, name: &str) -> Option<Matrix> {
    let m = obj.get(name)?;
    Some(Matrix { m00: m.get("m00")?.as_f64()? as f32, m01: m.get("m01")?.as_f64()? as f32, m02: m.get("m02")?.as_f64()? as f32, m10: m.get("m10")?.as_f64()? as f32, m11: m.get("m11")?.as_f64()? as f32, m12: m.get("m12")?.as_f64()? as f32 })
}

pub fn get_parent_index(obj: &serde_json::Value) -> Option<(NodeId, String)> {
    let pi = obj.get("parentIndex")?;
    let guid = pi.get("guid")?;
    let pos = pi.get("position")?.as_str().unwrap_or("").to_string();
    Some((NodeId::new(guid.get("sessionID")?.as_u64()? as u32, guid.get("localID")?.as_u64()? as u32), pos))
}

pub fn get_paints(obj: &serde_json::Value, name: &str) -> Vec<Paint> {
    let arr = match obj.get(name).and_then(|v| v.as_array()) {
        Some(a) => a, None => return Vec::new(),
    };
    arr.iter().map(|item| {
        let image_hash = item.get("image").and_then(|img| img.get("hash"))
            .and_then(|h| h.as_array())
            .map(|bytes| bytes.iter().filter_map(|b| b.as_u64().map(|n| format!("{:02x}", n))).collect());
        let stops = item.get("stops").and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|s| Some(ColorStop {
                color: get_color(s, "color")?,
                position: s.get("position")?.as_f64()? as f32,
            })).collect())
            .unwrap_or_default();
        Paint {
            paint_type: match item.get("type").and_then(|v| v.as_str()).unwrap_or("SOLID") {
                "SOLID" => PaintType::Solid, "GRADIENT_LINEAR" => PaintType::GradientLinear,
                "GRADIENT_RADIAL" => PaintType::GradientRadial, "GRADIENT_ANGULAR" => PaintType::GradientAngular,
                "GRADIENT_DIAMOND" => PaintType::GradientDiamond, "IMAGE" => PaintType::Image,
                _ => PaintType::Solid,
            },
            color: get_color(item, "color"),
            opacity: item.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
            visible: item.get("visible").and_then(|v| v.as_bool()).unwrap_or(true),
            stops,
            transform: get_matrix(item, "transform"),
            image_hash,
            gradient_handles: Vec::new(),
        }
    }).collect()
}

pub fn get_text_data(obj: &serde_json::Value) -> Option<TextData> {
    let td = obj.get("textData")?;
    Some(TextData {
        characters: td.get("characters").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        font_family: td.get("fontFamily").and_then(|v| v.as_str()).map(|s| s.to_string()),
        font_weight: td.get("fontWeight").and_then(|v| v.as_f64()).unwrap_or(400.0) as f32,
        font_size: td.get("fontSize").and_then(|v| v.as_f64()).unwrap_or(16.0) as f32,
        letter_spacing: td.get("letterSpacing").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        line_height: td.get("lineHeight").and_then(|v| v.as_f64()).map(|v| v as f32),
        fills: get_paints(td, "fills"),
    })
}

pub fn get_effects(obj: &serde_json::Value) -> Vec<Effect> {
    let arr = match obj.get("effects").and_then(|v| v.as_array()) {
        Some(a) => a, None => return Vec::new(),
    };
    arr.iter().map(|item| Effect {
        effect_type: match item.get("type").and_then(|v| v.as_str()).unwrap_or("DROP_SHADOW") {
            "INNER_SHADOW" => EffectType::InnerShadow, "DROP_SHADOW" => EffectType::DropShadow,
            "FOREGROUND_BLUR" => EffectType::ForegroundBlur, "BACKGROUND_BLUR" => EffectType::BackgroundBlur,
            _ => EffectType::DropShadow,
        },
        color: get_color(item, "color"),
        offset: get_vector(item, "offset").unwrap_or(Vec2::new(0.0, 0.0)),
        radius: item.get("radius").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        visible: item.get("visible").and_then(|v| v.as_bool()).unwrap_or(true),
        spread: item.get("spread").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
    }).collect()
}