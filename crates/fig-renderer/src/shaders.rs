//! WGSL shader source code strings and pipeline creation helpers.
//!
//! These shaders handle 2D rendering with transforms, solid fills,
//! gradients, textures, and effects.

/// Vertex shader for 2D scene rendering.
pub const VS_SCENE: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coord: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec2<f32>,
    @location(1) tex_coord: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct Uniforms {
    view_projection: mat4x4<f32>,
    node_transform: mat4x4<f32>,
    opacity: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@vertex
fn main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = uniforms.node_transform * vec4<f32>(in.position, 0.0, 1.0);
    out.clip_position = uniforms.view_projection * world_pos;
    out.world_position = world_pos.xy;
    out.tex_coord = in.tex_coord;
    out.color = in.color * uniforms.opacity;
    return out;
}
"#;

/// Fragment shader for solid color fills.
pub const FS_SOLID: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec2<f32>,
    @location(1) tex_coord: vec2<f32>,
    @location(2) color: vec4<f32>,
};

@fragment
fn main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

/// Fragment shader for gradient fills.
pub const FS_GRADIENT: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec2<f32>,
    @location(1) tex_coord: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct GradientUniforms {
    gradient_transform: mat4x4<f32>,
    num_stops: u32,
    // up to 8 stops: [offset, color_r, color_g, color_b, color_a] each
    stops: array<f32, 40>,
    paint_type: u32, // 0=solid, 1=linear, 2=radial, 3=angular
};

@group(1) @binding(0)
var<uniform> gradient: GradientUniforms;

fn mix_linear(t: f32, stops_data: array<f32, 40>, num: u32) -> vec4<f32> {
    var prev_offset: f32 = stops_data[0];
    var prev_color: vec4<f32> = vec4<f32>(stops_data[1], stops_data[2], stops_data[3], stops_data[4]);

    for (var i: u32 = 1u; i < num; i = i + 1u) {
        let idx = i * 5u;
        let offset = stops_data[idx];
        let color = vec4<f32>(stops_data[idx + 1u], stops_data[idx + 2u], stops_data[idx + 3u], stops_data[idx + 4u]);
        if t <= offset {
            let local_t = (t - prev_offset) / (offset - prev_offset);
            return mix(prev_color, color, local_t);
        }
        prev_offset = offset;
        prev_color = color;
    }
    return prev_color;
}

@fragment
fn main(in: VertexOutput) -> @location(0) vec4<f32> {
    var t: f32;

    if (gradient.paint_type == 0u) {
        // Solid: use the vertex color
        return in.color;
    }

    if (gradient.paint_type == 1u) {
        // Linear: project onto the gradient axis
        let p = gradient.gradient_transform * vec4<f32>(in.world_position, 0.0, 1.0);
        t = p.x;
    } elseif (gradient.paint_type == 2u) {
        // Radial: distance from center
        let p = gradient.gradient_transform * vec4<f32>(in.world_position, 0.0, 1.0);
        t = length(p.xy);
    } else {
        // Angular: angle around center
        let p = gradient.gradient_transform * vec4<f32>(in.world_position, 0.0, 1.0);
        t = atan2(p.y, p.x) / (2.0 * 3.14159265) + 0.5;
    }

    return mix_linear(clamp(t, 0.0, 1.0), gradient.stops, gradient.num_stops);
}
"#;

/// Fragment shader for image fills.
pub const FS_IMAGE: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec2<f32>,
    @location(1) tex_coord: vec2<f32>,
    @location(2) color: vec4<f32>,
};

@group(0) @binding(1)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(2)
var s_diffuse: sampler;

@fragment
fn main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, in.tex_coord) * in.color;
}
"#;

/// Vertex shader for selection/highlight overlay.
pub const VS_HIGHLIGHT: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct Uniforms {
    view_projection: mat4x4<f32>,
    node_transform: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@vertex
fn main(in: VertexInput) -> @builtin(position) vec4<f32> {
    let world_pos = uniforms.node_transform * vec4<f32>(in.position, 0.0, 1.0);
    return uniforms.view_projection * world_pos;
}
"#;

/// Fragment shader for selection/highlight overlay.
pub const FS_HIGHLIGHT: &str = r#"
@fragment
fn main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.094, 0.627, 0.984, 0.5);
}
"#;