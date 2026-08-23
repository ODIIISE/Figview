//! wgpu-based GPU renderer for Figma documents.
//!
//! Renders to an offscreen texture, then copies the result to a
//! mappable buffer for readback to the frontend. The readback uses a
//! 256-byte-aligned row stride as required by `wgpu` copy operations;
//! padding bytes are stripped before returning pixels.

use std::collections::HashMap;

use fig_parser::types::{NodeType, PaintType};
use wgpu::util::DeviceExt;

use crate::camera::Camera;
use crate::gradients::{encode_paint, GRADIENT_SLOT_SIZE};
use crate::path_tess::{tessellate_geometry_path, vector_scale};
use crate::pipelines::RenderPipelines;
use crate::renderer::{DecodedImage, RenderCommand, RenderOutput, Renderer};
use crate::scene::{RenderNode, RenderTree, SceneGraph};
use crate::shapes;
use crate::textures::TextureManager;
use crate::transforms;

/// Uniform buffer data mirroring the WGSL `Uniforms` struct (160 bytes).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct SceneUniforms {
    view_projection: [f32; 16],
    node_transform: [f32; 16],
    opacity: f32,
    _padding: [f32; 3],
    paint_color: [f32; 4],
}

impl SceneUniforms {
    fn new(
        view_projection: [f32; 16],
        node_transform: [f32; 16],
        opacity: f32,
        paint_color: [f32; 4],
    ) -> Self {
        Self {
            view_projection,
            node_transform,
            opacity,
            _padding: [0.0; 3],
            paint_color,
        }
    }
}

/// Vertex/index buffers for one geometry mesh.
struct GpuGeom {
    vertex_buffer: wgpu::Buffer,
    index_buffer: Option<wgpu::Buffer>,
    vertex_count: u32,
    index_count: u32,
}

impl GpuGeom {
    fn from_vertices(device: &wgpu::Device, vertices: &[shapes::RenderVertex]) -> Option<Self> {
        if vertices.is_empty() {
            return None;
        }
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vbo"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        Some(GpuGeom {
            vertex_buffer,
            index_buffer: None,
            vertex_count: vertices.len() as u32,
            index_count: 0,
        })
    }

    fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        if let Some(ref ib) = self.index_buffer {
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.index_count, 0, 0..1);
        } else {
            pass.draw(0..self.vertex_count, 0..1);
        }
    }
}

/// How to color one paint on a node.
#[derive(Debug, Clone)]
enum PaintDraw {
    /// Flat RGBA color.
    Solid([f32; 4]),
    /// Index into the gradient uniform arena (one 256-byte slot each).
    Gradient(u32),
    /// Image fill keyed by Figma image hash.
    Image(String),
}

/// Cached GPU data for a render node.
struct GpuNode {
    fill_geometry: Option<GpuGeom>,
    stroke_geometry: Option<GpuGeom>,
    /// Visible paints in draw order (bottom first).
    fills: Vec<PaintDraw>,
    strokes: Vec<PaintDraw>,
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
    readback_stride: u64,

    // Uniforms
    scene_uniform_buffer: wgpu::Buffer,
    scene_bind_group: Option<wgpu::BindGroup>,
    gradient_arena_buffer: Option<wgpu::Buffer>,
    gradient_bind_group: Option<wgpu::BindGroup>,
    image_bind_group_layout: Option<wgpu::BindGroupLayout>,

    // Scene state
    scene_graph: Option<SceneGraph>,
    current_page: usize,
    camera: Camera,

    // Cached GPU geometry (node key → geometry + paint plan)
    geometry_cache: HashMap<String, GpuNode>,

    // Decoded images waiting for upload / already uploaded
    decoded_images: HashMap<String, DecodedImage>,

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

        let texture_manager = TextureManager::new(&device);

        Ok(Self {
            device,
            queue,
            pipelines: None,
            render_texture: None,
            render_texture_view: None,
            readback_buffer: None,
            readback_stride: 0,
            scene_uniform_buffer,
            scene_bind_group: None,
            gradient_arena_buffer: None,
            gradient_bind_group: None,
            image_bind_group_layout: None,
            scene_graph: None,
            current_page: 0,
            camera: Camera::default(),
            geometry_cache: HashMap::new(),
            decoded_images: HashMap::new(),
            texture_manager,
            width: 1,
            height: 1,
            dpr: 1.0,
        })
    }

    /// Build bind groups/layouts shared by all draws.
    fn build_bind_groups(&mut self) {
        let scene_layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("scene uniform layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
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
        self.scene_bind_group = Some(scene_bind_group);

        let image_layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("image bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        self.image_bind_group_layout = Some(image_layout);

        let gradient_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("gradient uniform layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });
        // Keep the layout alive implicitly via the pipeline layouts; store it
        // inside the bind group creation below using an empty arena for now.
        let empty_arena = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("gradient arena (empty)"),
                contents: &[0u8; GRADIENT_SLOT_SIZE],
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let gradient_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gradient bind group"),
            layout: &gradient_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &empty_arena,
                    offset: 0,
                    size: Some(wgpu::BufferSize::new(GRADIENT_SLOT_SIZE as u64).unwrap()),
                }),
            }],
        });
        self.gradient_bind_group = Some(gradient_bind_group);
    }

    /// Create or recreate render target textures with an aligned readback stride.
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

        // wgpu requires COPY_BYTES_PER_ROW_ALIGNMENT (256) alignment.
        let bytes_per_row = (4 * w) as u64;
        let stride = bytes_per_row.div_ceil(256) * 256;
        let buffer_size = stride * (h - 1) as u64 + bytes_per_row.max(256);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        self.render_texture = Some(texture);
        self.render_texture_view = Some(view);
        self.readback_buffer = Some(readback);
        self.readback_stride = stride;
    }

    /// Upload uniform data for one draw.
    fn update_scene_uniforms(
        &self,
        node_transform: &transforms::Matrix,
        opacity: f32,
        paint_color: [f32; 4],
    ) {
        let vp = self.camera.view_projection_matrix();
        let nt = transforms::to_column_major_4x4(node_transform);
        let uniforms = SceneUniforms::new(vp, nt, opacity, paint_color);
        self.queue.write_buffer(
            &self.scene_uniform_buffer,
            0,
            bytemuck::cast_slice(&[uniforms]),
        );
    }

    /// Pure pass: builds geometry + paint plan and encodes gradient slots.
    fn build_cache_data(&self, sg: &SceneGraph) -> (HashMap<String, GpuNode>, Vec<u8>) {
        let mut cache: HashMap<String, GpuNode> = HashMap::new();

        let mut encoded: Vec<u8> = Vec::new();
        let mut next_slot: u32 = 0;

        for tree in &sg.trees {
            for node in &tree.nodes {
                let key = node_key(&node.id.session_id, &node.id.local_id);

                let fill_geometry = self.build_fill_geometry(node);
                let stroke_geometry = self.build_stroke_geometry(node);
                if fill_geometry.is_none() && stroke_geometry.is_none() {
                    continue;
                }

                let fills = visible_paints(&node.fill_paints)
                    .into_iter()
                    .filter_map(|p| plan_paint(p, &mut encoded, &mut next_slot))
                    .collect();
                let strokes = visible_paints(&node.stroke_paints)
                    .into_iter()
                    .filter_map(|p| plan_paint(p, &mut encoded, &mut next_slot))
                    .collect();

                cache.insert(
                    key,
                    GpuNode {
                        fill_geometry,
                        stroke_geometry,
                        fills,
                        strokes,
                    },
                );
            }
        }

        (cache, encoded)
    }

    /// Tessellate fill geometry: baked paths first, then shape primitives,
    /// then the vector network as a last resort.
    fn build_fill_geometry(&self, node: &RenderNode) -> Option<GpuGeom> {
        // Baked fill geometry from the parser (local units).
        if !node.fill_geometry.is_empty() {
            let mut vertices = Vec::new();
            for path in &node.fill_geometry {
                vertices.extend(tessellate_geometry_path(path, 1.0, 1.0));
            }
            if !vertices.is_empty() {
                return GpuGeom::from_vertices(&self.device, &vertices);
            }
        }

        // Vector network fallback (may be normalized).
        if let Some(vg) = &node.vector_geometry {
            if !vg.paths.is_empty() {
                let (sx, sy) = vector_scale(node.size.as_ref(), vg.normalized_size.as_ref());
                let mut vertices = Vec::new();
                for path in &vg.paths {
                    vertices.extend(tessellate_geometry_path(path, sx, sy));
                }
                if !vertices.is_empty() {
                    return GpuGeom::from_vertices(&self.device, &vertices);
                }
            }
        }

        self.build_primitive_geometry(node)
    }

    /// Stroke geometry is pre-expanded by Figma into fill regions, so it is
    /// tessellated with the fill tessellator too.
    fn build_stroke_geometry(&self, node: &RenderNode) -> Option<GpuGeom> {
        if node.stroke_geometry.is_empty() || node.stroke_weight <= 0.0 {
            return None;
        }
        let mut vertices = Vec::new();
        for path in &node.stroke_geometry {
            vertices.extend(tessellate_geometry_path(path, 1.0, 1.0));
        }
        GpuGeom::from_vertices(&self.device, &vertices)
    }

    /// Primitive fallback geometry (rect / rounded rect / ellipse).
    fn build_primitive_geometry(&self, node: &RenderNode) -> Option<GpuGeom> {
        let width = node.size.map(|s| s.x).unwrap_or(0.0);
        let height = node.size.map(|s| s.y).unwrap_or(0.0);

        if width <= 0.0 && height <= 0.0 {
            return None;
        }

        let vertices = match node.node_type {
            NodeType::Ellipse => shapes::generate_ellipse(width, height),
            NodeType::RoundedRectangle
            | NodeType::Rectangle
            | NodeType::Frame
            | NodeType::Component
            | NodeType::Section
            | NodeType::ComponentSet
            | NodeType::Instance => {
                let r = node.corner_radius.unwrap_or(0.0);
                if r > 0.0 || node.corner_radii.is_some() {
                    let radii = effective_corner_radii(node, r);
                    shapes::generate_rounded_rect(
                        width,
                        height,
                        radii.top_left,
                        radii.top_right,
                        radii.bottom_right,
                        radii.bottom_left,
                    )
                } else {
                    shapes::generate_rect(width, height)
                }
            }
            NodeType::Line => shapes::generate_rect(width.max(1.0), height.max(1.0)),
            _ => return None,
        };

        GpuGeom::from_vertices(&self.device, &vertices)
    }

    /// Make sure every referenced image has a GPU texture.
    fn ensure_textures(&mut self) {
        // Split-borrow distinct fields so textures can be uploaded while
        // reading device/queue/layout immutably.
        let WgpuRenderer {
            device,
            queue,
            scene_uniform_buffer,
            image_bind_group_layout,
            decoded_images,
            texture_manager,
            ..
        } = self;

        let layout = match image_bind_group_layout.as_ref() {
            Some(l) => l,
            None => return,
        };

        let pending: Vec<String> = decoded_images
            .keys()
            .filter(|h| !texture_manager.contains(h))
            .cloned()
            .collect();
        for hash in pending {
            if let Some(img) = decoded_images.get(&hash) {
                texture_manager.upload(
                    &hash,
                    img.width,
                    img.height,
                    &img.rgba,
                    device,
                    queue,
                    layout,
                    scene_uniform_buffer,
                );
            }
        }
    }

    /// Collect all visible nodes in draw order with accumulated opacity.
    fn collect_draw_list<'a>(&self, tree: &'a RenderTree) -> Vec<(&'a RenderNode, f32)> {
        let mut list = Vec::new();
        for &root_idx in &tree.root_indices {
            self.collect_nodes(tree, root_idx, 1.0, &mut list);
        }
        list
    }

    fn collect_nodes<'a>(
        &self,
        tree: &'a RenderTree,
        idx: usize,
        parent_opacity: f32,
        list: &mut Vec<(&'a RenderNode, f32)>,
    ) {
        if let Some(node) = tree.nodes.get(idx) {
            if node.visible {
                let opacity = parent_opacity * node.opacity.clamp(0.0, 1.0);
                list.push((node, opacity));
                for &child_idx in &node.children {
                    self.collect_nodes(tree, child_idx, opacity, list);
                }
            }
        }
    }

    /// Draw one node: all fills bottom-up, then strokes.
    fn draw_node<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        pipelines: &'a RenderPipelines,
        node: &RenderNode,
        opacity: f32,
    ) {
        let gpu = match self
            .geometry_cache
            .get(&node_key(&node.id.session_id, &node.id.local_id))
        {
            Some(g) => g,
            None => return,
        };

        let scene_bg = match self.scene_bind_group {
            Some(ref bg) => bg,
            None => return,
        };
        let gradient_bg = self.gradient_bind_group.as_ref();

        // Fills
        if let Some(ref geom) = gpu.fill_geometry {
            for paint in gpu.fills.iter() {
                match paint {
                    PaintDraw::Solid(color) => {
                        self.update_scene_uniforms(&node.world_transform, opacity, *color);
                        pass.set_pipeline(&pipelines.solid_fill);
                        pass.set_bind_group(0, scene_bg, &[]);
                    }
                    PaintDraw::Gradient(slot) => {
                        self.update_scene_uniforms(
                            &node.world_transform,
                            opacity,
                            [1.0, 1.0, 1.0, 1.0],
                        );
                        pass.set_pipeline(&pipelines.gradient_fill);
                        pass.set_bind_group(0, scene_bg, &[]);
                        if let Some(bg) = gradient_bg {
                            pass.set_bind_group(
                                1,
                                bg,
                                &[(*slot as u32) * (GRADIENT_SLOT_SIZE as u32)],
                            );
                        } else {
                            continue;
                        }
                    }
                    PaintDraw::Image(hash) => {
                        self.update_scene_uniforms(
                            &node.world_transform,
                            opacity,
                            [1.0, 1.0, 1.0, 1.0],
                        );
                        let binding = self.texture_manager.get(hash);
                        match binding {
                            Some(b) => {
                                pass.set_pipeline(&pipelines.image_fill);
                                pass.set_bind_group(0, &b.bind_group, &[]);
                            }
                            None => continue,
                        }
                    }
                }
                geom.draw(pass);
            }
        }

        // Strokes (drawn over fills)
        if let Some(ref geom) = gpu.stroke_geometry {
            for paint in gpu.strokes.iter() {
                match paint {
                    PaintDraw::Solid(color) => {
                        self.update_scene_uniforms(&node.world_transform, opacity, *color);
                        pass.set_pipeline(&pipelines.solid_fill);
                        pass.set_bind_group(0, scene_bg, &[]);
                    }
                    PaintDraw::Gradient(slot) => {
                        self.update_scene_uniforms(
                            &node.world_transform,
                            opacity,
                            [1.0, 1.0, 1.0, 1.0],
                        );
                        pass.set_pipeline(&pipelines.gradient_fill);
                        pass.set_bind_group(0, scene_bg, &[]);
                        if let Some(bg) = gradient_bg {
                            pass.set_bind_group(
                                1,
                                bg,
                                &[(*slot as u32) * (GRADIENT_SLOT_SIZE as u32)],
                            );
                        } else {
                            continue;
                        }
                    }
                    PaintDraw::Image(_) => continue,
                }
                geom.draw(pass);
            }
        }
    }
}

// ── Helpers ──

fn node_key(session: &u32, local: &u32) -> String {
    format!("{}:{}", session, local)
}

enum PaintClass {
    Solid([f32; 4]),
    Gradient,
    Image,
}

fn classify_paint(paint: &fig_parser::types::Paint) -> PaintClass {
    match paint.paint_type {
        PaintType::Solid => PaintClass::Solid(paint_color(paint)),
        PaintType::GradientLinear
        | PaintType::GradientRadial
        | PaintType::GradientAngular
        | PaintType::GradientDiamond => PaintClass::Gradient,
        PaintType::Image => PaintClass::Image,
        _ => PaintClass::Solid([0.0, 0.0, 0.0, 0.0]),
    }
}

fn paint_color(paint: &fig_parser::types::Paint) -> [f32; 4] {
    match paint.color {
        Some(c) => [c.r, c.g, c.b, c.a * paint.opacity.clamp(0.0, 1.0)],
        None => [0.0, 0.0, 0.0, 0.0],
    }
}

fn visible_paints(paints: &[fig_parser::types::Paint]) -> Vec<&fig_parser::types::Paint> {
    paints.iter().filter(|p| p.visible).collect()
}

/// Classify a paint and, for gradients, append its encoded slot to the arena.
fn plan_paint(
    paint: &fig_parser::types::Paint,
    encoded: &mut Vec<u8>,
    next_slot: &mut u32,
) -> Option<PaintDraw> {
    match classify_paint(paint) {
        PaintClass::Solid(color) => Some(PaintDraw::Solid(color)),
        PaintClass::Gradient => {
            let slot = *next_slot;
            *next_slot += 1;
            encoded.extend_from_slice(&encode_paint(paint).to_slot_bytes());
            Some(PaintDraw::Gradient(slot))
        }
        PaintClass::Image => paint
            .image_hash
            .clone()
            .map(PaintDraw::Image)
            .or(Some(PaintDraw::Solid([0.8, 0.8, 0.8, 1.0]))),
    }
}

fn effective_corner_radii(node: &RenderNode, default: f32) -> fig_parser::types::CornerRadii {
    node.corner_radii.unwrap_or(fig_parser::types::CornerRadii {
        top_left: default,
        top_right: default,
        bottom_right: default,
        bottom_left: default,
    })
}

impl Renderer for WgpuRenderer {
    fn initialize(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.width = width.max(1);
        self.height = height.max(1);
        self.dpr = 1.0;

        let surface_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        self.pipelines = Some(RenderPipelines::new(&self.device, surface_format, 1));
        self.texture_manager = TextureManager::new(&self.device);
        self.build_bind_groups();
        self.create_render_targets();
        self.camera.resize(width as f32, height as f32, 1.0);
        self.ensure_textures();

        Ok(())
    }

    fn handle_command(&mut self, command: RenderCommand) -> Result<(), String> {
        match command {
            RenderCommand::LoadScene(sg) => {
                // Build GPU caches from the scene before storing it.
                let (cache, encoded) = self.build_cache_data(&sg);
                self.geometry_cache = cache;

                if !encoded.is_empty() {
                    let arena = self
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("gradient arena"),
                            contents: &encoded,
                            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                        });

                    let gradient_layout =
                        self.device
                            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                                label: Some("gradient uniform layout"),
                                entries: &[wgpu::BindGroupLayoutEntry {
                                    binding: 0,
                                    visibility: wgpu::ShaderStages::FRAGMENT,
                                    ty: wgpu::BindingType::Buffer {
                                        ty: wgpu::BufferBindingType::Uniform,
                                        has_dynamic_offset: true,
                                        min_binding_size: None,
                                    },
                                    count: None,
                                }],
                            });

                    let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("gradient bind group"),
                        layout: &gradient_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &arena,
                                offset: 0,
                                size: Some(
                                    wgpu::BufferSize::new(GRADIENT_SLOT_SIZE as u64).unwrap(),
                                ),
                            }),
                        }],
                    });

                    self.gradient_arena_buffer = Some(arena);
                    self.gradient_bind_group = Some(bind_group);
                } else {
                    self.gradient_arena_buffer = None;
                    self.gradient_bind_group = None;
                }

                self.current_page = 0;
                self.ensure_textures();
                self.camera.zoom = 1.0;
                self.camera.pan_x = 0.0;
                self.camera.pan_y = 0.0;
                if let Some(tree) = sg.trees.get(self.current_page) {
                    if !tree.content_bounds.is_empty() {
                        self.camera.fit_rect(&tree.content_bounds, 48.0);
                    }
                }
                self.scene_graph = Some(sg);
            }
            RenderCommand::LoadImages(images) => {
                for (hash, img) in images {
                    self.decoded_images.insert(hash, img);
                }
                self.ensure_textures();
            }
            RenderCommand::ClearScene => {
                self.geometry_cache.clear();
                self.decoded_images.clear();
                self.texture_manager.clear();
                self.gradient_arena_buffer = None;
                self.scene_graph = None;
            }
            RenderCommand::SetPage(idx) => {
                self.current_page = idx;
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
            RenderCommand::ZoomAt {
                screen_x,
                screen_y,
                zoom,
            } => {
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
                if let Some(bounds) =
                    find_node_bounds(self.scene_graph.as_ref(), self.current_page, node_id_str)
                {
                    self.camera.fit_rect(&bounds, 32.0);
                }
            }
            RenderCommand::CenterOnNode(ref node_id_str) => {
                if let Some(bounds) =
                    find_node_bounds(self.scene_graph.as_ref(), self.current_page, node_id_str)
                {
                    self.camera.center_on(&bounds);
                }
            }
            RenderCommand::SelectNode(_id) => {
                // Selection highlight is drawn by the frontend overlay.
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
        let w = ((self.width as f32 * self.dpr) as u32).max(1);
        let h = ((self.height as f32 * self.dpr) as u32).max(1);

        // If the stored target doesn't match the requested size, rebuild it.
        let expected_stride = (4 * w as u64).div_ceil(256) * 256;
        if self.readback_stride != expected_stride || self.render_texture.is_none() {
            self.create_render_targets();
        }

        let render_view = self
            .render_texture_view
            .as_ref()
            .ok_or_else(|| "Render target not created".to_string())?;
        let readback = self
            .readback_buffer
            .as_ref()
            .ok_or_else(|| "Readback buffer not created".to_string())?;
        let pipelines = self
            .pipelines
            .as_ref()
            .ok_or_else(|| "Pipelines not initialized".to_string())?;

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
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

            if let Some(ref sg) = self.scene_graph {
                if let Some(tree) = sg.trees.get(self.current_page) {
                    for (node, opacity) in self.collect_draw_list(tree) {
                        self.draw_node(&mut pass, pipelines, node, opacity);
                    }
                }
            }
        }

        let bytes_per_row = 4 * w as u64;
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
                    bytes_per_row: Some(self.readback_stride as u32),
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
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::Maintain::Wait);
        let _ = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "Timed out waiting for GPU readback".to_string())?;

        // Strip row padding while copying out.
        let mapped = buffer_slice.get_mapped_range();
        let row_bytes = bytes_per_row as usize;
        let stride_bytes = self.readback_stride as usize;
        let mut pixels = Vec::with_capacity(row_bytes * h as usize);
        if stride_bytes == row_bytes {
            pixels.extend_from_slice(&mapped[..row_bytes * h as usize]);
        } else {
            for row in 0..h as usize {
                let start = row * stride_bytes;
                pixels.extend_from_slice(&mapped[start..start + row_bytes]);
            }
        }
        drop(mapped);
        readback.unmap();

        Ok(RenderOutput {
            pixels,
            width: w,
            height: h,
        })
    }

    fn resize(&mut self, width: u32, height: u32, dpr: f32) {
        self.handle_command(RenderCommand::Resize { width, height, dpr })
            .ok();
    }

    fn camera(&self) -> &Camera {
        &self.camera
    }

    fn dispose(&mut self) {
        self.geometry_cache.clear();
        self.decoded_images.clear();
        self.texture_manager.clear();
        self.gradient_arena_buffer = None;
        self.scene_graph = None;
    }
}

fn find_node_bounds(
    sg: Option<&SceneGraph>,
    page: usize,
    node_id: &str,
) -> Option<transforms::Rect> {
    let sg = sg?;
    let tree = sg.trees.get(page)?;
    for node in &tree.nodes {
        if node.id.to_string() == node_id {
            return node.bounds;
        }
    }
    None
}
