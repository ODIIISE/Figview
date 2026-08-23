//! Path tessellation: converts Figma path commands into GPU triangle meshes.

use fig_parser::types::{GeometryPath, PathCommand, WindingRule};
use crate::shapes::RenderVertex;
use crate::shapes::{VertexBuffers, BuffersBuilder, FillVertex, FillTessellator, FillOptions};
use lyon_path::math::point;
use lyon_path::Builder;

/// Tessellate a Figma GeometryPath into a triangle mesh.
pub fn tessellate_geometry_path(
    geo_path: &GeometryPath,
    scale_x: f32,
    scale_y: f32,
) -> Vec<RenderVertex> {
    if geo_path.commands.is_empty() {
        return Vec::new();
    }

    let mut builder = Builder::new();
    let mut has_open = false;

    for cmd in &geo_path.commands {
        match cmd {
            PathCommand::MoveTo { x, y } => {
                if has_open {
                    builder.end(false);
                }
                builder.begin(point(x * scale_x, y * scale_y));
                has_open = true;
            }
            PathCommand::LineTo { x, y } => {
                if !has_open {
                    builder.begin(point(0.0, 0.0));
                    has_open = true;
                }
                builder.line_to(point(x * scale_x, y * scale_y));
            }
            PathCommand::CubicTo { x1, y1, x2, y2, x, y } => {
                if !has_open {
                    builder.begin(point(0.0, 0.0));
                    has_open = true;
                }
                builder.cubic_bezier_to(
                    point(x1 * scale_x, y1 * scale_y),
                    point(x2 * scale_x, y2 * scale_y),
                    point(x * scale_x, y * scale_y),
                );
            }
            PathCommand::Close => {
                if has_open {
                    builder.close();
                    has_open = false; // close() = end(true)
                }
            }
        }
    }

    if has_open {
        builder.end(false);
    }

    let path = builder.build();

    let mut geometry: VertexBuffers<RenderVertex, u32> = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();
    let options = FillOptions::default().with_tolerance(0.5)
        .with_fill_rule(match geo_path.winding_rule {
            WindingRule::EvenOdd => lyon_tessellation::FillRule::EvenOdd,
            WindingRule::NonZero => lyon_tessellation::FillRule::NonZero,
        });

    let _ = tessellator.tessellate_path(
        &path,
        &options,
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
            RenderVertex::new(
                vertex.position().x,
                vertex.position().y,
                0.0, 0.0,
                1.0, 1.0, 1.0, 1.0,
            )
        }),
    );

    geometry.vertices
}

/// Compute the scale factors for a vector node.
pub fn vector_scale(
    vector_node_size: Option<&fig_parser::types::Vec2>,
    normalized_size: Option<&fig_parser::types::Vec2>,
) -> (f32, f32) {
    match (vector_node_size, normalized_size) {
        (Some(size), Some(norm)) => {
            let sx = if norm.x > 0.0 { size.x / norm.x } else { 1.0 };
            let sy = if norm.y > 0.0 { size.y / norm.y } else { 1.0 };
            (sx, sy)
        }
        _ => (1.0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tessellates_simple_rect() {
        let path = GeometryPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 100.0, y: 0.0 },
                PathCommand::LineTo { x: 100.0, y: 50.0 },
                PathCommand::LineTo { x: 0.0, y: 50.0 },
                PathCommand::Close,
            ],
            winding_rule: WindingRule::NonZero,
            style_id: 0,
        };
        let verts = tessellate_geometry_path(&path, 1.0, 1.0);
        assert!(!verts.is_empty(), "Tessellated rect should produce vertices: got {}", verts.len());
    }

    #[test]
    fn empty_path_returns_empty() {
        let path = GeometryPath {
            commands: vec![],
            winding_rule: WindingRule::NonZero,
            style_id: 0,
        };
        let verts = tessellate_geometry_path(&path, 1.0, 1.0);
        assert!(verts.is_empty());
    }
}