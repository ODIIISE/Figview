//! GPU geometry generators for basic shapes.
//!
//! Each function returns a list of vertices forming a triangle list.

use bytemuck::{Pod, Zeroable};

/// Vertex format for 2D rendering.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct RenderVertex {
    pub position: [f32; 2],
    pub tex_coord: [f32; 2],
    pub color: [f32; 4],
}

impl RenderVertex {
    pub fn new(x: f32, y: f32, u: f32, v: f32, r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            position: [x, y],
            tex_coord: [u, v],
            color: [r, g, b, a],
        }
    }
}

pub fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<RenderVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    }
}

/// Generate a rectangle (two triangles) from origin at (0,0).
pub fn generate_rect(width: f32, height: f32) -> Vec<RenderVertex> {
    let hw = width;
    let hh = height;
    vec![
        // Triangle 1
        RenderVertex::new(0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0),
        RenderVertex::new(hw, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0),
        RenderVertex::new(hw, hh, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0),
        // Triangle 2
        RenderVertex::new(0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0),
        RenderVertex::new(hw, hh, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0),
        RenderVertex::new(0.0, hh, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0),
    ]
}

/// Generate vertices for a rounded rectangle.
pub fn generate_rounded_rect(
    width: f32,
    height: f32,
    tl: f32,
    tr: f32,
    br: f32,
    bl: f32,
) -> Vec<RenderVertex> {
    use lyon_path::math::point;
    use lyon_path::path::Builder;
    use lyon_tessellation::{
        BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers,
    };

    if width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }

    let max_r = (width / 2.0).min(height / 2.0);
    let tl = tl.min(max_r).max(0.0);
    let tr = tr.min(max_r).max(0.0);
    let br = br.min(max_r).max(0.0);
    let bl = bl.min(max_r).max(0.0);

    let mut builder = Builder::new();

    // Build as a simple outline: rounded corners approximated via cubic beziers
    let mut segments: Vec<(f32, f32)> = Vec::new();

    // Helper: add a rounded corner as cubic bezier
    fn add_arc_points(
        out: &mut Vec<(f32, f32)>,
        cx: f32,
        cy: f32,
        r: f32,
        start_angle: f32,
        end_angle: f32,
    ) {
        use std::f32::consts::PI;

        let angle_range = end_angle - start_angle;
        let segments_count = ((angle_range.abs() / (PI / 4.0)).ceil() as usize)
            .max(1)
            .min(4);
        let k = 4.0 / 3.0 * ((angle_range / segments_count as f32) / 4.0).tan();

        let mut a = start_angle;
        for _ in 0..segments_count {
            let next_a = a + angle_range / segments_count as f32;
            let cos_a = a.cos();
            let sin_a = a.sin();
            let cos_na = next_a.cos();
            let sin_na = next_a.sin();

            let x1 = cx + r * cos_a + r * k * (-sin_a);
            let y1 = cy + r * sin_a + r * k * cos_a;
            let x2 = cx + r * cos_na - r * k * (-sin_na);
            let y2 = cy + r * sin_na - r * k * cos_na;
            let xn = cx + r * cos_na;
            let yn = cy + r * sin_na;

            out.push((x1, y1));
            out.push((x2, y2));
            out.push((xn, yn));
            a = next_a;
        }
    }

    // Top edge: left to right
    segments.push((tl, 0.0)); // starting point after top-left corner

    // Top-right corner
    if tr > 0.0 {
        add_arc_points(
            &mut segments,
            width - tr,
            tr,
            tr,
            -std::f32::consts::FRAC_PI_2,
            0.0,
        );
    } else {
        segments.push((width, 0.0));
    }

    // Right edge
    segments.push((width, height - br));

    // Bottom-right corner
    if br > 0.0 {
        add_arc_points(
            &mut segments,
            width - br,
            height - br,
            br,
            0.0,
            std::f32::consts::FRAC_PI_2,
        );
    } else {
        segments.push((width, height));
    }

    // Bottom edge
    segments.push((bl, height));

    // Bottom-left corner
    if bl > 0.0 {
        add_arc_points(
            &mut segments,
            bl,
            height - bl,
            bl,
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
        );
    } else {
        segments.push((0.0, height));
    }

    // Left edge
    segments.push((0.0, tl));

    // Top-left corner
    if tl > 0.0 {
        add_arc_points(
            &mut segments,
            tl,
            tl,
            tl,
            std::f32::consts::PI,
            3.0 * std::f32::consts::FRAC_PI_2,
        );
    } else {
        segments.push((0.0, 0.0));
    }

    // Now build path from segments
    if let Some(&(sx, sy)) = segments.first() {
        let _ = builder.begin(point(sx, sy));
        for i in 1..segments.len() {
            let (x, y) = segments[i];
            let _ = builder.line_to(point(x, y));
        }
        let _ = builder.close();
    }

    let path = builder.build();

    let mut geometry: VertexBuffers<RenderVertex, u32> = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();
    let _ = tessellator.tessellate_path(
        &path,
        &FillOptions::default().with_tolerance(0.5),
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
            RenderVertex::new(
                vertex.position().x,
                vertex.position().y,
                0.0,
                0.0,
                1.0,
                1.0,
                1.0,
                1.0,
            )
        }),
    );

    geometry.vertices
}

/// Generate vertices for an ellipse.
pub fn generate_ellipse(width: f32, height: f32) -> Vec<RenderVertex> {
    use lyon_path::math::point;
    use lyon_path::path::Builder;
    use lyon_tessellation::{
        BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers,
    };

    if width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }

    let rx = width / 2.0;
    let ry = height / 2.0;
    let cx = rx;
    let cy = ry;

    // Build an ellipse as 4 cubic Bezier segments (approximation)
    let k = 0.5522847498; // magic constant for circle approximation
    let mut builder = Builder::new();
    let _ = builder.begin(point(cx, cy - ry));

    // Top-right
    let _ = builder.cubic_bezier_to(
        point(cx + rx * k, cy - ry),
        point(cx + rx, cy - ry * k),
        point(cx + rx, cy),
    );
    // Bottom-right
    let _ = builder.cubic_bezier_to(
        point(cx + rx, cy + ry * k),
        point(cx + rx * k, cy + ry),
        point(cx, cy + ry),
    );
    // Bottom-left
    let _ = builder.cubic_bezier_to(
        point(cx - rx * k, cy + ry),
        point(cx - rx, cy + ry * k),
        point(cx - rx, cy),
    );
    // Top-left
    let _ = builder.cubic_bezier_to(
        point(cx - rx, cy - ry * k),
        point(cx - rx * k, cy - ry),
        point(cx, cy - ry),
    );

    let _ = builder.close();
    let path = builder.build();

    let mut geometry: VertexBuffers<RenderVertex, u32> = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();
    let _ = tessellator.tessellate_path(
        &path,
        &FillOptions::default().with_tolerance(0.5),
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
            RenderVertex::new(
                vertex.position().x,
                vertex.position().y,
                0.0,
                0.0,
                1.0,
                1.0,
                1.0,
                1.0,
            )
        }),
    );

    geometry.vertices
}

// Re-exports for use in other modules
pub use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_has_six_vertices() {
        let verts = generate_rect(100.0, 50.0);
        assert_eq!(verts.len(), 6);
    }

    #[test]
    fn ellipse_not_empty() {
        let verts = generate_ellipse(100.0, 100.0);
        assert!(!verts.is_empty());
    }

    #[test]
    fn rounded_rect_not_empty() {
        let verts = generate_rounded_rect(100.0, 50.0, 10.0, 10.0, 10.0, 10.0);
        assert!(!verts.is_empty());
    }
}
