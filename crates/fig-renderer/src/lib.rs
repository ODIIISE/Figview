//! Native GPU rendering crate for Figma `.fig` documents.
//!
//! Architecture:
//!   FigDocument → SceneGraph → RenderTree → RenderNode list
//!                                     ↓
//!                               WgpuRenderer → GPU

pub mod camera;
pub mod gradients;
pub mod path_tess;
pub mod pipelines;
pub mod renderer;
pub mod scene;
pub mod shaders;
pub mod shapes;
pub mod text;
pub mod textures;
pub mod transforms;
pub mod wgpu_renderer;

pub use camera::Camera;
pub use renderer::{RenderCommand, Renderer};
pub use scene::{SceneGraph, build_scene_graph};
pub use transforms::{Matrix, Point, Rect};
pub use wgpu_renderer::WgpuRenderer;