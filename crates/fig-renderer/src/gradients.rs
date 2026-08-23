//! Gradient uniform buffer encoding for GPU shaders.
//!
//! Encodes Figma gradient data into a fixed-size uniform buffer
//! that the gradient fragment shader can consume directly.

use crate::transforms;
use fig_parser::types::{Matrix, Paint, PaintType};

/// Maximum number of gradient stops supported in the shader.
pub const MAX_STOPS: usize = 8;

/// Uniform data for a single gradient. We manually implement Pod/Zeroable
/// since arrays larger than 32 don't auto-derive those traits.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct GradientUniforms {
    /// 4x4 column-major: inverse of the gradient transform mapping world→gradient space.
    pub gradient_transform: [f32; 16],
    /// Number of active stops.
    pub num_stops: u32,
    /// The paint type code: 0=solid, 1=linear, 2=radial, 3=angular.
    pub paint_type: u32,
    /// Padding.
    pub _pad0: u32,
    pub _pad1: u32,
    /// Interleaved stop data: [offset0, r, g, b, a, offset1, r, g, b, a, ...]
    pub stops: [f32; MAX_STOPS * 5],
}

unsafe impl bytemuck::Pod for GradientUniforms {}
unsafe impl bytemuck::Zeroable for GradientUniforms {}

impl Default for GradientUniforms {
    fn default() -> Self {
        Self {
            gradient_transform: transforms::IDENTITY_4X4,
            num_stops: 0,
            paint_type: 0,
            _pad0: 0,
            _pad1: 0,
            stops: [0.0; MAX_STOPS * 5],
        }
    }
}

/// Identity 4x4 matrix. Used as a convenience.
use crate::transforms::IDENTITY_4X4;

/// Encode a Paint into GradientUniforms for the shader.
///
/// For solid paints, num_stops is set to 0 (falls through to vertex color).
pub fn encode_paint(paint: &Paint) -> GradientUniforms {
    if paint.paint_type == PaintType::Solid {
        return GradientUniforms::default();
    }

    if paint.stops.len() < 2 {
        return GradientUniforms::default();
    }

    let paint_type_code: u32 = match paint.paint_type {
        PaintType::GradientLinear => 1,
        PaintType::GradientRadial => 2,
        PaintType::GradientAngular => 3,
        PaintType::GradientDiamond => 2, // approximate as radial
        _ => 0,
    };

    let gradient_transform = match &paint.transform {
        Some(t) => {
            // The paint transform maps from a normalized gradient space (0..1)
            // into the object's local coordinate space.
            // We need the inverse: world-space position → normalized gradient coordinate.
            if let Some(inv) = invert_gradient_matrix(t) {
                inv
            } else {
                IDENTITY_4X4
            }
        }
        None => IDENTITY_4X4,
    };

    let num_stops = paint.stops.len().min(MAX_STOPS) as u32;
    let mut stops = [0.0f32; MAX_STOPS * 5];

    for (i, stop) in paint.stops.iter().take(MAX_STOPS).enumerate() {
        let base = i * 5;
        stops[base] = stop.position.clamp(0.0, 1.0);
        stops[base + 1] = stop.color.r;
        stops[base + 2] = stop.color.g;
        stops[base + 3] = stop.color.b;
        stops[base + 4] = stop.color.a;
    }

    GradientUniforms {
        gradient_transform,
        num_stops,
        paint_type: paint_type_code,
        _pad0: 0,
        _pad1: 0,
        stops,
    }
}

/// Invert a Figma gradient transform matrix.
/// The gradient transform maps normalized coords to object coords;
/// we invert it so the shader can go from world coords → gradient coords.
fn invert_gradient_matrix(m: &Matrix) -> Option<[f32; 16]> {
    // Convert 2x3 affine to 3x3, invert, convert back.
    let det = m.m00 * m.m11 - m.m01 * m.m10;
    if det.abs() < 1e-10 {
        return None;
    }

    let inv_det = 1.0 / det;
    let inv_m00 = m.m11 * inv_det;
    let inv_m01 = -m.m01 * inv_det;
    let inv_m02 = (m.m01 * m.m12 - m.m11 * m.m02) * inv_det;
    let inv_m10 = -m.m10 * inv_det;
    let inv_m11 = m.m00 * inv_det;
    let inv_m12 = (m.m10 * m.m02 - m.m00 * m.m12) * inv_det;

    // Return 4x4 column-major
    Some([
        inv_m00, inv_m10, 0.0, 0.0, inv_m01, inv_m11, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, inv_m02,
        inv_m12, 0.0, 1.0,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use fig_parser::types::{Color, ColorStop, Vec2};

    #[test]
    fn solid_paint_has_zero_stops() {
        let paint = Paint {
            paint_type: PaintType::Solid,
            color: Some(Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }),
            opacity: 1.0,
            visible: true,
            stops: vec![],
            transform: None,
            image_hash: None,
            gradient_handles: vec![],
        };
        let uniforms = encode_paint(&paint);
        assert_eq!(uniforms.num_stops, 0);
    }

    #[test]
    fn linear_gradient_encodes_stops() {
        let paint = Paint {
            paint_type: PaintType::GradientLinear,
            color: None,
            opacity: 1.0,
            visible: true,
            stops: vec![
                ColorStop {
                    color: Color {
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    },
                    position: 0.0,
                },
                ColorStop {
                    color: Color {
                        r: 0.0,
                        g: 0.0,
                        b: 1.0,
                        a: 1.0,
                    },
                    position: 1.0,
                },
            ],
            transform: None,
            image_hash: None,
            gradient_handles: vec![],
        };
        let uniforms = encode_paint(&paint);
        assert_eq!(uniforms.num_stops, 2);
        assert!((uniforms.stops[0] - 0.0).abs() < 0.01);
        assert!((uniforms.stops[5] - 1.0).abs() < 0.01);
    }
}
