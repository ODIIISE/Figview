//! SVG emitter: walks a `RenderTree` and produces plain, inspectable SVG.
//!
//! Design notes:
//! - Every node's `world_transform` is already absolute, so shapes are emitted
//!   flat-in-draw-order, each carrying its own `transform` matrix.
//! - Clip containers (`clips_content`) are the only place nesting is required.
//! - Text is emitted as real `<text>` (approximate metrics, flagged).
//! - Images are embedded as base64 data URIs when bytes are available.

use fig_parser::types::{GeometryPath, Paint, PaintType, PathCommand};
use fig_renderer::scene::{RenderNode, RenderTree};
use std::collections::HashMap;

type ImageMap = HashMap<String, Vec<u8>>;

/// Extract raw image bytes referenced by the document (for embedding).
pub fn extract_images(path: &str) -> Result<ImageMap, String> {
    let archive =
        fig_parser::archive::open_archive(path).map_err(|e| format!("archive: {}", e))?;
    Ok(archive.images)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn color_css(c: &fig_parser::types::Color) -> String {
    format!(
        "rgb({},{},{})",
        (c.r.clamp(0.0, 1.0) * 255.0).round() as u32,
        (c.g.clamp(0.0, 1.0) * 255.0).round() as u32,
        (c.b.clamp(0.0, 1.0) * 255.0).round() as u32
    )
}

struct Emitter<'a> {
    tree: &'a RenderTree,
    images: &'a ImageMap,
    defs: String,
    body: String,
    id_counter: usize,
}

impl<'a> Emitter<'a> {
    fn next_id(&mut self, kind: &str) -> String {
        self.id_counter += 1;
        format!("{}{}", kind, self.id_counter)
    }

    /// Path `d` attribute from baked geometry paths.
    fn paths_to_d(paths: &[&GeometryPath], sx: f32, sy: f32) -> String {
        let mut d = String::new();
        for p in paths {
            for cmd in &p.commands {
                match cmd {
                    PathCommand::MoveTo { x, y } => {
                        d.push_str(&format!("M {:.2} {:.2} ", x * sx, y * sy))
                    }
                    PathCommand::LineTo { x, y } => {
                        d.push_str(&format!("L {:.2} {:.2} ", x * sx, y * sy))
                    }
                    PathCommand::CubicTo { x1, y1, x2, y2, x, y } => d.push_str(&format!(
                        "C {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} ",
                        x1 * sx,
                        y1 * sy,
                        x2 * sx,
                        y2 * sy,
                        x * sx,
                        y * sy
                    )),
                    PathCommand::Close => d.push_str("Z "),
                }
            }
        }
        d.trim_end().to_string()
    }

    fn matrix_attr(m: &fig_renderer::transforms::Matrix) -> String {
        // SVG matrix(a b c d e f) == column pairs of the affine.
        format!(
            "matrix({:.4} {:.4} {:.4} {:.4} {:.4} {:.4})",
            m.m00, m.m10, m.m01, m.m11, m.m02, m.m12
        )
    }

    fn register_gradient(&mut self, paint: &Paint, node_size: (f32, f32)) -> Option<String> {
        let stops = &paint.stops;
        if stops.len() < 2 {
            return None;
        }
        let id = self.next_id("grad");
        let kind_code = match paint.paint_type {
            PaintType::GradientLinear => "linear",
            _ => "radial",
        };
        let mut def = match kind_code {
            "linear" => format!(
                "<linearGradient id=\"{}\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"0\"",
                id
            ),
            _ => format!(
                "<radialGradient id=\"{}\" cx=\"0.5\" cy=\"0.5\" r=\"0.5\"",
                id
            ),
        };
        if let Some(t) = &paint.transform {
            def.push_str(&format!(
                " gradientTransform=\"matrix({:.5} {:.5} {:.5} {:.5} {:.5} {:.5})\"",
                t.m00, t.m10, t.m01, t.m11, t.m02, t.m12
            ));
        }
        def.push('>');
        for s in stops {
            def.push_str(&format!(
                "<stop offset=\"{:.3}\" stop-color=\"{}\" stop-opacity=\"{:.3}\"/>",
                s.position.clamp(0.0, 1.0),
                color_css(&s.color),
                s.color.a
            ));
        }
        def.push_str(if kind_code == "linear" {
            "</linearGradient>"
        } else {
            "</radialGradient>"
        });
        let _ = node_size;
        self.defs.push_str(&def);
        Some(id)
    }

    fn shape_elements(&mut self, node: &RenderNode) -> Vec<String> {
        let mut out = Vec::new();
        let width = node.size.map(|s| s.x).unwrap_or(0.0);
        let height = node.size.map(|s| s.y).unwrap_or(0.0);

        // Fill geometry sources, best first.
        let geo_paths: Vec<&GeometryPath> = if !node.fill_geometry.is_empty() {
            node.fill_geometry.iter().collect()
        } else if let Some(vg) = &node.vector_geometry {
            vg.paths.iter().collect()
        } else {
            Vec::new()
        };

        let visible_paints: Vec<&Paint> = node
            .fill_paints
            .iter()
            .chain(node.background_paints.iter())
            .filter(|p| p.visible)
            .collect();

        // Emit one drawable per visible paint (bottom-up).
        for paint in &visible_paints {
            let mut attrs = String::new();
            match paint.paint_type {
                PaintType::Solid => {
                    let Some(c) = paint.color else { continue };
                    attrs.push_str(&format!(" fill=\"{}\"", color_css(&c)));
                    let a = c.a * paint.opacity;
                    if a < 1.0 {
                        attrs.push_str(&format!(" fill-opacity=\"{:.3}\"", a));
                    }
                }
                PaintType::GradientLinear
                | PaintType::GradientRadial
                | PaintType::GradientDiamond => {
                    if let Some(id) = self.register_gradient(paint, (width, height)) {
                        attrs.push_str(&format!(" fill=\"url(#{})\"", id));
                    } else if let Some(first) = paint.stops.first() {
                        attrs.push_str(&format!(" fill=\"{}\"", color_css(&first.color)));
                    } else {
                        continue;
                    }
                }
                PaintType::Image => {
                    if let Some(hash) = &paint.image_hash {
                        if let Some(bytes) = self.images.get(hash) {
                            let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
                                "image/png"
                            } else if bytes.starts_with(&[0xFF, 0xD8]) {
                                "image/jpeg"
                            } else {
                                "application/octet-stream"
                            };
                            let b64 = base64_encode(bytes);
                            out.push(format!(
                                "<image x=\"0\" y=\"0\" width=\"{:.2}\" height=\"{:.2}\" preserveAspectRatio=\"none\" href=\"data:{};base64,{}\"/>",
                                width.max(0.001),
                                height.max(0.001),
                                mime,
                                b64
                            ));
                            continue;
                        }
                    }
                    // Missing image: neutral placeholder so layout stays visible.
                    attrs.push_str(" fill=\"#cccccc\"");
                }
                _ => continue,
            }

            if !geo_paths.is_empty() {
                let d = Self::paths_to_d(&geo_paths, 1.0, 1.0);
                if !d.is_empty() {
                    out.push(format!("<path d=\"{}\"{}/>", d, attrs));
                }
            } else {
                match node.node_type {
                    fig_parser::types::NodeType::Ellipse => {
                        out.push(format!(
                            "<ellipse cx=\"{:.2}\" cy=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\"{}/>",
                            width / 2.0,
                            height / 2.0,
                            width / 2.0,
                            height / 2.0,
                            attrs
                        ));
                    }
                    _ => {
                        let r = node.corner_radius.unwrap_or(0.0);
                        let radii = node.corner_radii.unwrap_or(fig_parser::types::CornerRadii {
                            top_left: r,
                            top_right: r,
                            bottom_right: r,
                            bottom_left: r,
                        });
                        let rx = radii.top_left.max(radii.top_right);
                        out.push(format!(
                            "<rect x=\"0\" y=\"0\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\"{}/>",
                            width, height, rx.min(width / 2.0), radii.top_left.min(height / 2.0), attrs
                        ));
                    }
                }
            }
        }

        // Strokes: Figma bakes them into filled outline regions.
        if !node.stroke_geometry.is_empty() && node.stroke_weight > 0.0 {
            if let Some(sp) = node.stroke_paints.iter().find(|p| p.visible) {
                let color = sp
                    .color
                    .or_else(|| sp.stops.first().map(|s| s.color))
                    .unwrap_or(fig_parser::types::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    });
                let d = Self::paths_to_d(&node.stroke_geometry.iter().collect::<Vec<_>>(), 1.0, 1.0);
                if !d.is_empty() {
                    out.push(format!(
                        "<path d=\"{}\" fill=\"{}\"/>",
                        d,
                        color_css(&color)
                    ));
                }
            }
        }

        // Text approximation: real text runs at bounds origin.
        if let (Some(td), Some(bounds)) = (&node.text_data, node.bounds) {
            if !td.characters.trim().is_empty() {
                let color = node
                    .fill_paints
                    .iter()
                    .filter(|p| p.visible)
                    .find_map(|p| p.color)
                    .unwrap_or(fig_parser::types::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    });
                let family = td.font_family.clone().unwrap_or_else(|| "sans-serif".into());
                // NOTE: emitted in world coordinates (no transform wrapper).
                let lx = bounds.min_x;
                let ly = bounds.min_y + td.font_size;
                let mut text = format!(
                    "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"{}\" font-size=\"{:.2}\" font-weight=\"{}\" fill=\"{}\" xml:space=\"preserve\">",
                    lx,
                    ly,
                    esc(&family),
                    td.font_size,
                    td.font_weight as u32,
                    color_css(&color)
                );
                let lh = td.line_height.unwrap_or(td.font_size * 1.21);
                for (i, line) in td.characters.split('\n').enumerate() {
                    if i > 0 {
                        text.push_str(&format!(
                            "<tspan x=\"{:.2}\" dy=\"{:.2}\">",
                            lx, lh
                        ));
                        text.push_str(&esc(line));
                        text.push_str("</tspan>");
                    } else {
                        text.push_str(&esc(line));
                    }
                }
                text.push_str("</text>");
                out.push(text);
            }
        }

        out
    }

    fn clip_shape_d(&self, node: &RenderNode) -> Option<String> {
        if !node.fill_geometry.is_empty() {
            return Some(Self::paths_to_d(&node.fill_geometry.iter().collect::<Vec<_>>(), 1.0, 1.0));
        }
        if let (Some(w), Some(h)) = (node.size.map(|s| s.x), node.size.map(|s| s.y)) {
            if w > 0.0 && h > 0.0 {
                return Some(format!("M 0 0 H {:.2} V {:.2} H 0 Z", w, h));
            }
        }
        None
    }

    fn walk(&mut self, node: &RenderNode, parent_opacity: f32) {
        if !node.visible {
            return;
        }
        self.body.push_str(&format!(
            "<!-- {} ({:?}) children={} -->",
            esc(&node.name),
            node.node_type,
            node.children.len()
        ));
        let opacity = (parent_opacity * node.opacity.clamp(0.0, 1.0)).min(1.0);

        // Own shapes (text excluded from the transform wrapper because it was
        // emitted in world coordinates above).
        let mut parts = self.shape_elements(node);
        let text_parts: Vec<String> = parts
            .iter()
            .filter(|p| p.starts_with("<text"))
            .cloned()
            .collect();
        parts.retain(|p| !p.starts_with("<text"));

        let has_children = !node.children.is_empty();

        let open_group = node.clips_content
            && has_children
            && self
                .clip_shape_d(node)
                .map(|d| !d.is_empty())
                .unwrap_or(false);

        if open_group {
            let cid = self.next_id("clip");
            let d = self.clip_shape_d(node).unwrap();
            // The clip lives in WORLD space (transform baked into the path)
            // and the referencing group carries NO transform of its own —
            // otherwise the group transform would shift the clip a second
            // time and clip the frame's own background away.
            self.defs.push_str(&format!(
                "<clipPath id=\"{}\"><path transform=\"{}\" d=\"{}\"/></clipPath>",
                cid,
                Self::matrix_attr(&node.world_transform),
                d
            ));
            self.body.push_str(&format!(
                "<g clip-path=\"url(#{})\" opacity=\"{:.3}\">",
                cid, opacity
            ));
            if !parts.is_empty() {
                self.body.push_str(&format!(
                    "<g transform=\"{}\">",
                    Self::matrix_attr(&node.world_transform)
                ));
                for p in parts {
                    self.body.push_str(&p);
                }
                self.body.push_str("</g>");
            }
            for &ci in &node.children {
                if let Some(child) = self.tree.nodes.get(ci) {
                    self.walk(child, opacity);
                }
            }
            self.body.push_str("</g>");
        } else {
            let needs_group = (!parts.is_empty() && opacity < 1.0) || has_children;
            if needs_group {
                self.body.push_str(&format!(
                    "<g transform=\"{}\" opacity=\"{:.3}\">",
                    Self::matrix_attr(&node.world_transform),
                    opacity
                ));
            }
            for p in parts {
                self.body.push_str(&p);
            }
            for &ci in &node.children {
                if let Some(child) = self.tree.nodes.get(ci) {
                    self.walk(child, opacity);
                }
            }
            if needs_group {
                self.body.push_str("</g>");
            }
        }

        for t in text_parts {
            if opacity < 1.0 {
                self.body.push_str(&format!(
                    "<g opacity=\"{:.3}\">{}</g>",
                    opacity, t
                ));
            } else {
                self.body.push_str(&t);
            }
        }
    }
}

/// Emit one complete SVG document for a page.
pub fn emit_page_svg(tree: &RenderTree, images: &ImageMap) -> String {
    let b = &tree.content_bounds;
    let pad = 8.0;
    let vb = format!(
        "{:.1} {:.1} {:.1} {:.1}",
        b.min_x - pad,
        b.min_y - pad,
        (b.max_x - b.min_x) + pad * 2.0,
        (b.max_y - b.min_y) + pad * 2.0
    );

    let mut em = Emitter {
        tree,
        images,
        defs: String::new(),
        body: String::with_capacity(64 * 1024),
        id_counter: 0,
    };

    for &ri in &tree.root_indices {
        if let Some(n) = tree.nodes.get(ri) {
            em.walk(n, 1.0);
        }
    }

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"{}\">\n<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#e5e5e5\"/>\n<defs>{}</defs>\n{}\n</svg>\n",
        vb,
        b.min_x - pad,
        b.min_y - pad,
        (b.max_x - b.min_x) + pad * 2.0,
        (b.max_y - b.min_y) + pad * 2.0,
        em.defs,
        em.body
    )
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}
