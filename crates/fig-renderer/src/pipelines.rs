//! Pipeline creation and management for the wgpu renderer.

use crate::shaders;
use crate::shapes;

/// All render pipelines managed by the renderer.
pub struct RenderPipelines {
    /// Solid color fill pipeline.
    pub solid_fill: wgpu::RenderPipeline,
    /// Gradient fill pipeline.
    pub gradient_fill: wgpu::RenderPipeline,
    /// Image fill pipeline.
    pub image_fill: wgpu::RenderPipeline,
}

impl RenderPipelines {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        msaa_samples: u32,
    ) -> Self {
        let solid_fill = create_solid_pipeline(device, surface_format, msaa_samples);
        let gradient_fill = create_gradient_pipeline(device, surface_format, msaa_samples);
        let image_fill = create_image_pipeline(device, surface_format, msaa_samples);

        Self {
            solid_fill,
            gradient_fill,
            image_fill,
        }
    }
}

fn base_primitive_state() -> wgpu::PrimitiveState {
    wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        strip_index_format: None,
        front_face: wgpu::FrontFace::Cw,
        cull_mode: None,
        polygon_mode: wgpu::PolygonMode::Fill,
        unclipped_depth: false,
        conservative: false,
    }
}

fn base_multisample(msaa_samples: u32) -> wgpu::MultisampleState {
    wgpu::MultisampleState {
        count: msaa_samples,
        mask: !0,
        alpha_to_coverage_enabled: false,
    }
}

fn base_blend_target() -> Option<wgpu::ColorTargetState> {
    Some(wgpu::ColorTargetState {
        format: wgpu::TextureFormat::Rgba8UnormSrgb, // replaced by caller format below
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
    })
}

fn create_solid_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    msaa_samples: u32,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("solid shader"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(&format!(
            "{}\n{}",
            shaders::VS_SCENE,
            shaders::FS_SOLID
        ))),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("scene uniforms"),
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

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("solid pipeline layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let mut target = base_blend_target();
    if let Some(t) = &mut target {
        t.format = format;
    }

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("solid fill pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "main",
            buffers: &[shapes::vertex_buffer_layout()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "main",
            targets: &[target],
            compilation_options: Default::default(),
        }),
        primitive: base_primitive_state(),
        depth_stencil: None,
        multisample: base_multisample(msaa_samples),
        multiview: None,
    })
}

fn create_gradient_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    msaa_samples: u32,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("gradient shader"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(&format!(
            "{}\n{}",
            shaders::VS_SCENE,
            shaders::FS_GRADIENT
        ))),
    });

    let scene_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene uniforms"),
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

    let gradient_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gradient uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    // One 256-byte slot per gradient, selected per draw.
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("gradient pipeline layout"),
        bind_group_layouts: &[&scene_bind_group_layout, &gradient_bind_group_layout],
        push_constant_ranges: &[],
    });

    let mut target = base_blend_target();
    if let Some(t) = &mut target {
        t.format = format;
    }

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("gradient fill pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "main",
            buffers: &[shapes::vertex_buffer_layout()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "main",
            targets: &[target],
            compilation_options: Default::default(),
        }),
        primitive: base_primitive_state(),
        depth_stencil: None,
        multisample: base_multisample(msaa_samples),
        multiview: None,
    })
}

fn create_image_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    msaa_samples: u32,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("image shader"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(&format!(
            "{}\n{}",
            shaders::VS_SCENE,
            shaders::FS_IMAGE
        ))),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("image bind group"),
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

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("image pipeline layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let mut target = base_blend_target();
    if let Some(t) = &mut target {
        t.format = format;
    }

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("image fill pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "main",
            buffers: &[shapes::vertex_buffer_layout()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "main",
            targets: &[target],
            compilation_options: Default::default(),
        }),
        primitive: base_primitive_state(),
        depth_stencil: None,
        multisample: base_multisample(msaa_samples),
        multiview: None,
    })
}
