//! wgpu-based GPU renderer for Figma documents.
//!
//! Renders to an offscreen texture, then copies the result to a
//! mappable buffer for readback to the frontend.

use std::collections::HashMap;

use fig_parser::types::{NodeType, PaintType};
use wgpu::util::DeviceExt;

use crate::camera::Camera;
use crate::gradients;
use crate::pipelines::RenderPipelines;
use crate::renderer::{RenderCommand, RenderOutput, Renderer};
use crate::scene::{RenderNode, RenderTree, SceneGraph};
use crate::shapes;
use crate::textures::TextureManager;
use crate::transforms;

/// Uniform buffer data mirroring the WGSL `Uniforms` struct.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct SceneUniforms {
    view_projection: [f32; 16],
    node_transform: [f32; 16],
    opacity: f32,
    _padding: [f32; 3],
}

impl SceneUniforms {
    fn new(view_projection: [f32; 16], node_transform: [f32; 16], opacity: f32) -> Self {
        Self {
            view_projection,
            node_transform,
            opacity,
            _padding: [0.0; 3],
        }
    }
}

/// Cached GPU geometry for a node.
struct GpuGeometry {
    vertex_buffer: wgpu::Buffer,
    index_buffer: Option<wgpu::Buffer>,
    vertex_count: u32,
    index_count: u32,
}

/// The main wgpu renderer.
pub struct WgpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipelines: Option<RenderPipelines>,

    // Render target
    render_texture: Option<wgpu::Texture>,
    render_texture_view: Option<wgpu::TextureView>,
    readback_buffer: Option<wgpu::Buffer>,

    // Uniforms
    scene_uniform_buffer: wgpu::Buffer,
    scene_bind_group: Option<wgpu::BindGroup>,
    scene_bind_group_layout: Option<wgpu::BindGroupLayout>,
    gradient_uniform_buffer: wgpu::Buffer,
    gradient_bind_group_layout: Option<wgpu::BindGroupLayout>,

    // Scene state
    scene_graph: Option<SceneGraph>,
    current_page: usize,
    selected_node: Option<String>,
    camera: Camera,

    // Cached GPU geometry (node ID → geometry)
    geometry_cache: HashMap<String, GpuGeometry>,

    // Texture cache
    texture_manager: TextureManager,

    // Dimensions
    width: u32,
    height: u32,
    dpr: f32,
}

impl WgpuRenderer {
    pub async fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "No suitable GPU adapter found".to_string())?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("figview device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    ..Default::default()
                },
                None,
            )
            .await
            .map_err(|e| format!("Failed to create device: {}", e))?;

        let scene_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scene uniforms"),
            contents: &[0u8; std::mem::size_of::<SceneUniforms>()],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let gradient_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gradient uniforms"),
            contents: &[0u8; std::mem::size_of::<gradients::GradientUniforms>()],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let texture_manager = TextureManager::new(&device);

        Ok(Self {
            device,
            queue,
            pipelines: None,
            render_texture: None,
            render_texture_view: None,
            readback_buffer: None,
            scene_uniform_buffer,
            scene_bind_group: None,
            scene_bind_group_layout: None,
            gradient_uniform_buffer,
            gradient_bind_group_layout: None,
            scene_graph: None,
            current_page: 0,
            selected_node: None,
            camera: Camera::default(),
            geometry_cache: HashMap::new(),
            texture_manager,
            width: 1,
            height: 1,
            dpr: 1.0,
        })
    }

    /// Build bind group layouts for the scene and gradient uniforms.
    fn build_bind_group_layouts(&mut self) {
        let scene_layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let scene_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene bind group"),
            layout: &scene_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.scene_uniform_buffer.as_entire_binding(),
            }],
        });

        let gradient_layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gradient uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        self.scene_bind_group_layout = Some(scene_layout);
        self.scene_bind_group = Some(scene_bind_group);
        self.gradient_bind_group_layout = Some(gradient_layout);
    }

    /// Create or recreate render target textures.
    fn create_render_targets(&mut self) {
        let w = (self.width as f32 * self.dpr) as u32;
        let h = (self.height as f32 * self.dpr) as u32;

        if w == 0 || h == 0 {
            return;
        }

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render target"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let buffer_size = (w * h * 4) as u64;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        self.render_texture = Some(texture);
        self.render_texture_view = Some(view);
        self.readback_buffer = Some(readback);
    }

    /// Upload uniform data for the current node.
    fn update_scene_uniforms(&self, node_transform: &transforms::Matrix, opacity: f32) {
        let vp = self.camera.view_projection_matrix();
        let nt = transforms::to_column_major_4x4(node_transform);
        let uniforms = SceneUniforms::new(vp, nt, opacity);
        self.queue.write_buffer(
            &self.scene_uniform_buffer,
            0,
            bytemuck::cast_slice(&[uniforms]),
        );
    }

    /// Upload gradient uniform data.
    fn update_gradient_uniforms(&self, gradient_data: &gradients::GradientUniforms) {
        self.queue.write_buffer(
            &self.gradient_uniform_buffer,
            0,
            bytemuck::cast_slice(&[*gradient_data]),
        );
    }

    /// Cache or retrieve GPU geometry for a node.
    fn ensure_geometry(&mut self, node: &RenderNode) -> Option<()> {
        let key = format!("{}:{}", node.id.session_id, node.id.local_id);

        if self.geometry_cache.contains_key(&key) {
            return Some(());
        }

        let (vertices, indices) = Self::build_shape_geometry(node);

        if vertices.is_empty() {
            return None;
        }

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("vbo_{}", key)),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = indices.as_ref().map(|idx| {
            self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("ibo_{}", key)),
                contents: bytemuck::cast_slice(idx),
                usage: wgpu::BufferUsages::INDEX,
            })
        });

        let geo = GpuGeometry {
            vertex_buffer,
            index_buffer,
            vertex_count: vertices.len() as u32,
            index_count: indices.as_ref().map(|i| i.len() as u32).unwrap_or(0),
        };

        self.geometry_cache.insert(key, geo);
        Some(())
    }

    /// Build triangle geometry for a render node based on its type.
    fn build_shape_geometry(
        node: &RenderNode,
    ) -> (Vec<shapes::RenderVertex>, Option<Vec<u32>>) {
        let width = node.size.map(|s| s.x).unwrap_or(0.0);
        let height = node.size.map(|s| s.y).unwrap_or(0.0);

        if width <= 0.0 && height <= 0.0 {
            return (vec![], None);
        }

        match node.node_type {
            NodeType::Ellipse => {
                let verts = shapes::generate_ellipse(width, height);
                (verts, None)
            }
            NodeType::Line => {
                let verts = shapes::generate_rect(width.max(1.0), height.max(1.0));
                (verts, None)
            }
            NodeType::RoundedRectangle => {
                let r = node.corner_radius.unwrap_or(0.0);
                let radii = node.corner_radii.unwrap_or(fig_parser::types::CornerRadii {
                    top_left: r,
                    top_right: r,
                    bottom_right: r,
                    bottom_left: r,
                });
                let verts = shapes::generate_rounded_rect(
                    width, height,
                    radii.top_left, radii.top_right,
                    radii.bottom_right, radii.bottom_left,
                );
                (verts, None)
            }
            NodeType::Rectangle | NodeType::Frame | NodeType::Component | NodeType::Section => {
                let r = node.corner_radius.unwrap_or(0.0);
                if r > 0.0 || node.corner_radii.is_some() {
                    let radii = node.corner_radii.unwrap_or(fig_parser::types::CornerRadii {
                        top_left: r,
                        top_right: r,
                        bottom_right: r,
                        bottom_left: r,
                    });
                    let verts = shapes::generate_rounded_rect(
                        width, height,
                        radii.top_left, radii.top_right,
                        radii.bottom_right, radii.bottom_left,
                    );
                    (verts, None)
                } else {
                    let verts = shapes::generate_rect(width, height);
                    (verts, None)
                }
            }
            _ => {
                let verts = shapes::generate_rect(width.max(1.0), height.max(1.0));
                (verts, None)
            }
        }
    }

    /// Draw the entire scene into a render pass.
    fn draw_scene<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        pipelines: &'a RenderPipelines,
    ) {
        let sg = match self.scene_graph.as_ref() {
            Some(sg) => sg,
            None => return,
        };

        let tree = match sg.trees.get(self.current_page) {
            Some(t) => t,
            None => return,
        };

        // Collect all visible nodes in draw order (pre-order traversal)
        let draw_list = self.collect_draw_list(tree);

        for node in &draw_list {
            self.draw_node(pass, pipelines, node, tree);
        }
    }

    /// Collect all visible nodes in pre-order.
    fn collect_draw_list<'a>(&self, tree: &'a RenderTree) -> Vec<&'a RenderNode> {
        let mut list = Vec::new();
        for &root_idx in &tree.root_indices {
            self.collect_nodes(tree, root_idx, &mut list);
        }
        list
    }

    fn collect_nodes<'a>(&self, tree: &'a RenderTree, idx: usize, list: &mut Vec<&'a RenderNode>) {
        if let Some(node) = tree.nodes.get(idx) {
            if node.visible {
                list.push(node);
                for &child_idx in &node.children {
                    self.collect_nodes(tree, child_idx, list);
                }
            }
        }
    }

    /// Draw a single node.
    fn draw_node<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        pipelines: &'a RenderPipelines,
        node: &RenderNode,
        _tree: &RenderTree,
    ) {
        let key = format!("{}:{}", node.id.session_id, node.id.local_id);

        // Check if we have geometry for this node
        let geometry = match self.geometry_cache.get(&key) {
            Some(g) => g,
            None => return,
        };

        // Compute accumulated opacity
        let opacity = node.opacity;

        // Update uniforms
        self.update_scene_uniforms(&node.world_transform, opacity);

        // Get the first visible paint
        let paint = node
            .fill_paints
            .iter()
            .chain(node.background_paints.iter())
            .find(|p| p.visible);

        match paint {
            Some(p) if p.paint_type == PaintType::Solid || p.stops.is_empty() => {
                pass.set_pipeline(&pipelines.solid_fill);
                if let Some(ref bg) = self.scene_bind_group {
                    pass.set_bind_group(0, bg, &[]);
                }
            }
            Some(p) if !p.stops.is_empty() => {
                // Gradient fill
                let grad_data = gradients::encode_paint(p);
                self.update_gradient_uniforms(&grad_data);
                pass.set_pipeline(&pipelines.gradient_fill);
                if let Some(ref bg) = self.scene_bind_group {
                    pass.set_bind_group(0, bg, &[]);
                }
                // TODO: set gradient bind group (group 1)
            }
            _ => {
                // Default solid
                pass.set_pipeline(&pipelines.solid_fill);
                if let Some(ref bg) = self.scene_bind_group {
                    pass.set_bind_group(0, bg, &[]);
                }
            }
        }

        pass.set_vertex_buffer(0, geometry.vertex_buffer.slice(..));

        if let Some(ref ib) = geometry.index_buffer {
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..geometry.index_count, 0, 0..1);
        } else {
            pass.draw(0..geometry.vertex_count, 0..1);
        }
    }
}

impl Renderer for WgpuRenderer {
    fn initialize(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.width = width.max(1);
        self.height = height.max(1);
        self.dpr = 1.0;

        let surface_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        self.pipelines = Some(RenderPipelines::new(&self.device, surface_format, 1));
        self.texture_manager = TextureManager::new(&self.device);
        self.build_bind_group_layouts();
        self.create_render_targets();
        self.camera.resize(width as f32, height as f32, 1.0);

        Ok(())
    }

    fn handle_command(&mut self, command: RenderCommand) -> Result<(), String> {
        match command {
            RenderCommand::LoadScene(sg) => {
                self.geometry_cache.clear();
                // Pre-cache geometry for all nodes
                let trees = sg.trees.clone();
                for tree in &trees {
                    for node in &tree.nodes {
                        self.ensure_geometry(node);
                    }
                }
                self.scene_graph = Some(sg);
                self.current_page = 0;
                self.selected_node = None;
                self.camera.zoom = 1.0;
                self.camera.pan_x = 0.0;
                self.camera.pan_y = 0.0;
                if let Some(ref sg) = self.scene_graph {
                    if let Some(tree) = sg.trees.get(self.current_page) {
                        if !tree.content_bounds.is_empty() {
                            self.camera.fit_rect(&tree.content_bounds, 48.0);
                        }
                    }
                }
            }
            RenderCommand::SetPage(idx) => {
                self.current_page = idx;
                self.selected_node = None;
                if let Some(ref sg) = self.scene_graph {
                    if let Some(tree) = sg.trees.get(self.current_page) {
                        if !tree.content_bounds.is_empty() {
                            self.camera.fit_rect(&tree.content_bounds, 48.0);
                        }
                    }
                }
            }
            RenderCommand::SetZoom(z) => {
                self.camera.set_zoom(z);
            }
            RenderCommand::ZoomAt { screen_x, screen_y, zoom } => {
                self.camera.zoom_at(screen_x, screen_y, zoom);
            }
            RenderCommand::Pan { dx, dy } => {
                self.camera.pan(dx, dy);
            }
            RenderCommand::FitPage { padding } => {
                if let Some(ref sg) = self.scene_graph {
                    if let Some(tree) = sg.trees.get(self.current_page) {
                        self.camera.fit_rect(&tree.content_bounds, padding);
                    }
                }
            }
            RenderCommand::FitNode(ref node_id_str) => {
                if let Some(ref sg) = self.scene_graph {
                    if let Some(tree) = sg.trees.get(self.current_page) {
                        for node in &tree.nodes {
                            let nid = format!("{}:{}", node.id.session_id, node.id.local_id);
                            if nid == *node_id_str {
                                if let Some(ref bounds) = node.bounds {
                                    self.camera.fit_rect(bounds, 32.0);
                                }
                                break;
                            }
                        }
                    }
                }
            }
            RenderCommand::CenterOnNode(ref node_id_str) => {
                if let Some(ref sg) = self.scene_graph {
                    if let Some(tree) = sg.trees.get(self.current_page) {
                        for node in &tree.nodes {
                            let nid = format!("{}:{}", node.id.session_id, node.id.local_id);
                            if nid == *node_id_str {
                                if let Some(ref bounds) = node.bounds {
                                    self.camera.center_on(bounds);
                                }
                                break;
                            }
                        }
                    }
                }
            }
            RenderCommand::SelectNode(id) => {
                self.selected_node = id;
            }
            RenderCommand::Resize { width, height, dpr } => {
                self.width = width.max(1);
                self.height = height.max(1);
                self.dpr = dpr;
                self.camera.resize(width as f32, height as f32, dpr);
                self.create_render_targets();
            }
            RenderCommand::Render => {}
        }
        Ok(())
    }

    fn render(&mut self) -> Result<RenderOutput, String> {
        let w = (self.width as f32 * self.dpr) as u32;
        let h = (self.height as f32 * self.dpr) as u32;

        let render_view = self.render_texture_view.as_ref()
            .ok_or_else(|| "Render target not created".to_string())?;
        let readback = self.readback_buffer.as_ref()
            .ok_or_else(|| "Readback buffer not created".to_string())?;
        let pipelines = self.pipelines.as_ref()
            .ok_or_else(|| "Pipelines not initialized".to_string())?;

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("render encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: render_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.898,
                            g: 0.898,
                            b: 0.898,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            self.draw_scene(&mut pass, pipelines);
        }

        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: self.render_texture.as_ref().unwrap(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: readback,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * w),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        let buffer_slice = readback.slice(..);
        buffer_slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);

        let pixels = if !buffer_slice.get_mapped_range().is_empty() {
            let data = buffer_slice.get_mapped_range();
            data.to_vec()
        } else {
            vec![0u8; (w * h * 4) as usize]
        };

        readback.unmap();

        Ok(RenderOutput {
            pixels,
            width: w,
            height: h,
        })
    }

    fn resize(&mut self, width: u32, height: u32, dpr: f32) {
        self.width = width.max(1);
        self.height = height.max(1);
        self.dpr = dpr;
        self.camera.resize(width as f32, height as f32, dpr);
        self.create_render_targets();
    }

    fn camera(&self) -> &Camera {
        &self.camera
    }

    fn dispose(&mut self) {
        self.geometry_cache.clear();
        self.texture_manager.clear();
        self.scene_graph = None;
    }
}