//! GPU text rendering.
//!
//! FUTURE: font loading (fontdb), text shaping (rustybuzz), glyph atlas upload.
//! For now: text nodes are passed through to the renderer but rendered as
//! a basic colored rectangle placeholder until text shaping is integrated.

use fig_parser::types::TextData;

/// Placeholder for text rendering state.
/// Will be replaced with glyph atlas and shaping pipeline.
#[derive(Debug, Clone)]
pub struct TextRenderer {
    /// Whether text rendering is enabled (always false for now).
    pub enabled: bool,
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self { enabled: false }
    }
}

impl TextRenderer {
    /// Stub: returns None, indicating text cannot be rendered yet.
    /// When implemented, this will return tessellated glyph quads.
    pub fn layout_text(&self, _data: &TextData, _max_width: f32) -> Option<TextLayout> {
        None
    }
}

/// Layout result for a text node (future).
#[derive(Debug, Clone)]
pub struct TextLayout {
    /// Per-glyph quads: position (x, y) and texcoords (u0, v0, u1, v1).
    pub glyphs: Vec<GlyphQuad>,
    /// Total width of the laid-out text.
    pub width: f32,
    /// Total height.
    pub height: f32,
}

/// A single glyph quad for GPU rendering.
#[derive(Debug, Clone, Copy)]
pub struct GlyphQuad {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}