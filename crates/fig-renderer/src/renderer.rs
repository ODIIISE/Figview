//! Abstract renderer trait and render commands.

use crate::camera::Camera;
use crate::scene::SceneGraph;

/// Commands sent from the frontend to control rendering.
#[derive(Debug, Clone)]
pub enum RenderCommand {
    /// Load a new document scene.
    LoadScene(SceneGraph),
    /// Switch to a specific page (by index into scene_graph.pages).
    SetPage(usize),
    /// Set the current zoom level.
    SetZoom(f32),
    /// Zoom centered on a screen point.
    ZoomAt { screen_x: f32, screen_y: f32, zoom: f32 },
    /// Pan by a screen-space delta.
    Pan { dx: f32, dy: f32 },
    /// Fit the current page content in the viewport.
    FitPage { padding: f32 },
    /// Fit a specific node's bounds (by node ID string like "1:234").
    FitNode(String),
    /// Center on a specific node.
    CenterOnNode(String),
    /// Select a node for highlight rendering.
    SelectNode(Option<String>),
    /// Resize the viewport.
    Resize { width: u32, height: u32, dpr: f32 },
    /// Request a render frame.
    Render,
}

/// Result of a render operation: pixels ready for display.
#[derive(Debug, Clone)]
pub struct RenderOutput {
    /// RGBA pixel data, row-major, width * height * 4 bytes.
    pub pixels: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Trait for Figma document renderers.
pub trait Renderer {
    /// Initialize the renderer with GPU device and configuration.
    fn initialize(&mut self, width: u32, height: u32) -> Result<(), String>;

    /// Process a render command.
    fn handle_command(&mut self, command: RenderCommand) -> Result<(), String>;

    /// Render the current frame.
    /// Returns the rendered pixel data for display in the frontend canvas.
    fn render(&mut self) -> Result<RenderOutput, String>;

    /// Resize the render target.
    fn resize(&mut self, width: u32, height: u32, dpr: f32);

    /// Get the current camera state (for debug display).
    fn camera(&self) -> &Camera;

    /// Shut down and release GPU resources.
    fn dispose(&mut self);
}