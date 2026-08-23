//! Gradient uniform encoding for GPU shaders.
//!
//! A single gradient is packed into an exactly-256-byte block so that many
//! gradients can live in one uniform buffer addressed via dynamic offsets
//! (which must be multiples of 256).
//!
//! WGSL mirror (all members are 16-byte aligned, so the uniform-space layout
//! matches the byte layout exactly):
//!
//! ```wgsl
//! struct Gradient {
//!     inv_transform: mat4x4<f32>,      //  0.. 64
//!     stops_rgb: array<vec4<f32>, 8>,  // 64..192  [offset, r, g, b]
//!     alphas: array<vec4<f32>, 2>,     // 192..224 8 stop alphas packed
//!     params: vec4<u32>,               // 224..240 [num_stops, paint_type, 0, 0]
//! }
//! ```

use crate::transforms;
use fig_parser::types::{Matrix, Paint, PaintType};

/// Maximum number of gradient stops supported in the shader.
pub const MAX_STOPS: usize = 8;

/// Size in bytes of one gradient slot (dynamic-offset alignment).
pub const GRADIENT_SLOT_SIZE: usize = 256;

/// Uniform data for a single gradient.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GradientUniforms {
    /// Inverse of the gradient transform: maps world space → gradient space.
    pub inv_transform: [f32; 16],
    /// Per-stop `[offset, r, g, b]`.
    pub stops_rgb: [[f32; 4]; MAX_STOPS],
    /// Stop alphas packed 4-per-vector.
    pub alphas: [[f32; 4]; 2],
    /// `[num_stops, paint_type, 0, 0]` — 0=solid, 1=linear, 2=radial, 3=angular.
    pub params: [u32; 4],
}

impl Default for GradientUniforms {
    fn default() -> Self {
        Self {
            inv_transform: transforms::IDENTITY_4X4,
            stops_rgb: [[0.0; 4]; MAX_STOPS],
            alphas: [[0.0; 4]; 2],
            params: [0; 4],
        }
    }
}

impl GradientUniforms {
    /// Serialize into one aligned 256-byte slot (zero padded).
    pub fn to_slot_bytes(&self) -> [u8; GRADIENT_SLOT_SIZE] {
        let mut out = [0u8; GRADIENT_SLOT_SIZE];
        let bytes: &[u8] = bytemuck::cast_slice(std::slice::from_ref(self));
        out[..bytes.len()].copy_from_slice(bytes);
        out
    }
}

/// Encode a Paint into GradientUniforms for the shader.
///
/// For solid paints, num_stops is set to 0 (falls through to vertex color).
pub fn encode_paint(paint: &Paint) -> GradientUniforms {
    let mut u = GradientUniforms::default();

    let paint_type_code: u32 = match paint.paint_type {
        PaintType::GradientLinear => 1,
        PaintType::GradientRadial | PaintType::GradientDiamond => 2,
        PaintType::GradientAngular => 3,
        _ => return u,
    };

    if paint.stops.len() < 2 {
        return u;
    }

    u.inv_transform = match &paint.transform {
        // The paint transform maps normalized gradient space (0..1) into the
        // object's local coordinate space; the shader needs the inverse.
        Some(t) => invert_gradient_matrix(t).unwrap_or(transforms::IDENTITY_4X4),
        None => transforms::IDENTITY_4X4,
    };

    let count = paint.stops.len().min(MAX_STOPS);
    for (i, stop) in paint.stops.iter().take(count).enumerate() {
        u.stops_rgb[i] = [
            stop.position.clamp(0.0, 1.0),
            stop.color.r,
            stop.color.g,
            stop.color.b,
        ];
        u.alphas[i / 4][i % 4] = stop.color.a;
    }

    u.params[0] = count as u32;
    u.params[1] = paint_type_code;
    u
}

/// Invert a Figma gradient transform matrix (2D affine).
fn invert_gradient_matrix(m: &Matrix) -> Option<[f32; 16]> {
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

    // Column-major 4x4
    Some([
        inv_m00, inv_m10, 0.0, 0.0, inv_m01, inv_m11, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, inv_m02,
        inv_m12, 0.0, 1.0,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use fig_parser::types::{Color, ColorStop};

    fn gradient_paint() -> Paint {
        Paint {
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
                        a: 0.5,
                    },
                    position: 1.0,
                },
            ],
            transform: None,
            image_hash: None,
            gradient_handles: vec![],
        }
    }

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
        let u = encode_paint(&paint);
        assert_eq!(u.params[0], 0);
    }

    #[test]
    fn linear_gradient_encodes_stops() {
        let u = encode_paint(&gradient_paint());
        assert_eq!(u.params[0], 2);
        assert_eq!(u.params[1], 1);
        assert!((u.stops_rgb[0][0] - 0.0).abs() < 0.01);
        assert!((u.stops_rgb[1][0] - 1.0).abs() < 0.01);
        assert!((u.alphas[0][0] - 1.0).abs() < 0.01);
        assert!((u.alphas[0][1] - 0.5).abs() < 0.01);
    }

    #[test]
    fn slot_bytes_are_aligned_size() {
        let u = encode_paint(&gradient_paint());
        let bytes = u.to_slot_bytes();
        assert_eq!(bytes.len(), GRADIENT_SLOT_SIZE);
        assert_eq!(GRADIENT_SLOT_SIZE % 256, 0);
    }
}
