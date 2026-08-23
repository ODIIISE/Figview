//! Decoders for Figma's baked path and vector-network blobs.

use crate::types::{GeometryPath, PathCommand, Vec2, VectorGeometry, WindingRule};
use serde_json::Value;

fn f32_at(bytes: &[u8], offset: &mut usize) -> Option<f32> {
    let end = offset.checked_add(4)?;
    let value = f32::from_le_bytes(bytes.get(*offset..end)?.try_into().ok()?);
    *offset = end;
    value.is_finite().then_some(value)
}

fn u32_at(bytes: &[u8], offset: &mut usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let value = u32::from_le_bytes(bytes.get(*offset..end)?.try_into().ok()?);
    *offset = end;
    Some(value)
}

fn winding_rule(value: Option<&Value>) -> WindingRule {
    match value.and_then(Value::as_str) {
        Some("EVENODD") | Some("ODD") => WindingRule::EvenOdd,
        _ => WindingRule::NonZero,
    }
}

/// Decode a pre-baked Figma path stream.
///
/// The command bytes are: 0 separator, 1 move, 2 line, 3 close, 4 cubic.
pub fn decode_commands_blob(bytes: &[u8]) -> Option<Vec<PathCommand>> {
    let mut offset = 0;
    let mut commands = Vec::new();
    while offset < bytes.len() {
        let opcode = *bytes.get(offset)?;
        offset += 1;
        match opcode {
            0 => {}
            1 => commands.push(PathCommand::MoveTo {
                x: f32_at(bytes, &mut offset)?,
                y: f32_at(bytes, &mut offset)?,
            }),
            2 => commands.push(PathCommand::LineTo {
                x: f32_at(bytes, &mut offset)?,
                y: f32_at(bytes, &mut offset)?,
            }),
            3 => commands.push(PathCommand::Close),
            4 => commands.push(PathCommand::CubicTo {
                x1: f32_at(bytes, &mut offset)?,
                y1: f32_at(bytes, &mut offset)?,
                x2: f32_at(bytes, &mut offset)?,
                y2: f32_at(bytes, &mut offset)?,
                x: f32_at(bytes, &mut offset)?,
                y: f32_at(bytes, &mut offset)?,
            }),
            _ => return None,
        }
    }
    (!commands.is_empty()).then_some(commands)
}

pub fn decode_geometry_paths(value: Option<&Value>, blobs: &[Vec<u8>]) -> Vec<GeometryPath> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let blob_id = entry.get("commandsBlob")?.as_u64()? as usize;
            let commands = decode_commands_blob(blobs.get(blob_id)?)?;
            Some(GeometryPath {
                commands,
                winding_rule: winding_rule(entry.get("windingRule")),
                style_id: entry.get("styleID").and_then(Value::as_u64).unwrap_or(0) as u32,
            })
        })
        .collect()
}

#[derive(Clone, Copy)]
struct Vertex {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Segment {
    start: u32,
    start_dx: f32,
    start_dy: f32,
    end: u32,
    end_dx: f32,
    end_dy: f32,
}

fn decode_vector_blob(bytes: &[u8]) -> Option<Vec<GeometryPath>> {
    let mut offset = 0;
    let vertex_count = u32_at(bytes, &mut offset)? as usize;
    let segment_count = u32_at(bytes, &mut offset)? as usize;
    let region_count = u32_at(bytes, &mut offset)? as usize;
    let remaining = bytes.len().saturating_sub(offset);
    if vertex_count > remaining / 12 {
        return None;
    }
    let remaining_after_vertices = remaining - vertex_count * 12;
    if segment_count > remaining_after_vertices / 28 {
        return None;
    }
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        let _style = u32_at(bytes, &mut offset)?;
        vertices.push(Vertex {
            x: f32_at(bytes, &mut offset)?,
            y: f32_at(bytes, &mut offset)?,
        });
    }
    let mut segments = Vec::with_capacity(segment_count);
    for _ in 0..segment_count {
        let _style = u32_at(bytes, &mut offset)?;
        segments.push(Segment {
            start: u32_at(bytes, &mut offset)?,
            start_dx: f32_at(bytes, &mut offset)?,
            start_dy: f32_at(bytes, &mut offset)?,
            end: u32_at(bytes, &mut offset)?,
            end_dx: f32_at(bytes, &mut offset)?,
            end_dy: f32_at(bytes, &mut offset)?,
        });
    }

    let mut paths = Vec::new();
    for _ in 0..region_count {
        let style_rule = u32_at(bytes, &mut offset)?;
        let style_id = style_rule >> 1;
        let winding = if style_rule & 1 == 1 {
            WindingRule::NonZero
        } else {
            WindingRule::EvenOdd
        };
        let loop_count = u32_at(bytes, &mut offset)? as usize;
        for _ in 0..loop_count {
            let segment_indices = u32_at(bytes, &mut offset)? as usize;
            let mut commands = Vec::new();
            for index in 0..segment_indices {
                let raw_index = u32_at(bytes, &mut offset)?;
                let reversed = raw_index & 0x8000_0000 != 0;
                let segment_index = (raw_index & 0x7fff_ffff) as usize;
                let segment = *segments.get(segment_index)?;
                let (start_index, end_index, start_dx, start_dy, end_dx, end_dy) = if reversed {
                    (
                        segment.end,
                        segment.start,
                        segment.end_dx,
                        segment.end_dy,
                        segment.start_dx,
                        segment.start_dy,
                    )
                } else {
                    (
                        segment.start,
                        segment.end,
                        segment.start_dx,
                        segment.start_dy,
                        segment.end_dx,
                        segment.end_dy,
                    )
                };
                let start = *vertices.get(start_index as usize)?;
                let end = *vertices.get(end_index as usize)?;
                if index == 0 {
                    commands.push(PathCommand::MoveTo {
                        x: start.x,
                        y: start.y,
                    });
                }
                let control1 = (start.x + start_dx, start.y + start_dy);
                let control2 = (end.x + end_dx, end.y + end_dy);
                if start_dx == 0.0 && start_dy == 0.0 && end_dx == 0.0 && end_dy == 0.0 {
                    commands.push(PathCommand::LineTo { x: end.x, y: end.y });
                } else {
                    commands.push(PathCommand::CubicTo {
                        x1: control1.0,
                        y1: control1.1,
                        x2: control2.0,
                        y2: control2.1,
                        x: end.x,
                        y: end.y,
                    });
                }
            }
            if !commands.is_empty() {
                commands.push(PathCommand::Close);
                paths.push(GeometryPath {
                    commands,
                    winding_rule: winding,
                    style_id,
                });
            }
        }
    }
    Some(paths)
}

pub fn decode_vector_geometry(value: Option<&Value>, blobs: &[Vec<u8>]) -> Option<VectorGeometry> {
    let data = value?;
    let blob_id = data.get("vectorNetworkBlob")?.as_u64()? as usize;
    let paths = decode_vector_blob(blobs.get(blob_id)?)?;
    let normalized_size = data.get("normalizedSize").and_then(|_| {
        Some(Vec2 {
            x: data.get("normalizedSize")?.get("x")?.as_f64()? as f32,
            y: data.get("normalizedSize")?.get("y")?.as_f64()? as f32,
        })
    });
    Some(VectorGeometry {
        paths,
        normalized_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_oversized_vector_counts() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        assert!(decode_vector_blob(&bytes).is_none());
    }

    #[test]
    fn decodes_commands() {
        let mut bytes = vec![1];
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.push(2);
        bytes.extend_from_slice(&2.0f32.to_le_bytes());
        bytes.extend_from_slice(&3.0f32.to_le_bytes());
        bytes.push(4);
        for value in [2.0, 3.0, 4.0, 5.0, 6.0, 7.0] {
            bytes.extend_from_slice(&(value as f32).to_le_bytes());
        }
        bytes.push(3);
        assert_eq!(decode_commands_blob(&bytes).unwrap().len(), 4);
    }
}
