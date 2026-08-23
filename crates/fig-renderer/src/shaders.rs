//! WGSL shader source code for 2D scene rendering.
//!
//! Layout notes:
//! - `Uniforms` (scene) is mirrored by `SceneUniforms` in `wgpu_renderer.rs`:
//!   view_projection [0..64], node_transform [64..128], opacity [128..132],
//!   pad to 144, paint_color [144..160]. Total 160 bytes.
//! - `Gradient` is mirrored by `gradients::GradientUniforms`, one 256-byte
//!   slot per gradient, addressed with dynamic bind-group offsets.

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
    _pad0: vec3<f32>,
    paint_color: vec4<f32>,
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
    out.color = uniforms.paint_color * uniforms.opacity;
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

struct Gradient {
    inv_transform: mat4x4<f32>,      //   0.. 64
    stops_rgb: array<vec4<f32>, 8>,  //  64..192  [offset, r, g, b]
    alphas: array<vec4<f32>, 2>,     // 192..224  stop alphas, 4 packed
    params: vec4<u32>,               // 224..240  [num_stops, paint_type, 0, 0]
};

@group(1) @binding(0)
var<uniform> gradient: Gradient;

fn stop_color(i: u32) -> vec4<f32> {
    let rgb = gradient.stops_rgb[i];
    return vec4<f32>(rgb.y, rgb.z, rgb.w, gradient.alphas[i / 4u][i % 4u]);
}

fn mix_stops(t: f32, num: u32) -> vec4<f32> {
    var prev_offset: f32 = gradient.stops_rgb[0u].x;
    var prev_color: vec4<f32> = stop_color(0u);

    for (var i: u32 = 1u; i < num; i = i + 1u) {
        let offset = gradient.stops_rgb[i].x;
        let d = offset - prev_offset;
        if t <= offset {
            if abs(d) < 1e-6 {
                return prev_color;
            }
            return mix(prev_color, stop_color(i), clamp((t - prev_offset) / d, 0.0, 1.0));
        }
        prev_offset = offset;
        prev_color = stop_color(i);
    }
    return prev_color;
}

@fragment
fn main(in: VertexOutput) -> @location(0) vec4<f32> {
    let paint_type = gradient.params.y;
    let num_stops = gradient.params.x;

    if (paint_type == 0u || num_stops < 2u) {
        // Solid fallback: use the vertex color.
        return in.color;
    }

    var t: f32;
    let p = gradient.inv_transform * vec4<f32>(in.world_position, 0.0, 1.0);
    if (paint_type == 1u) {
        t = p.x;
    } else if (paint_type == 2u) {
        t = length(p.xy);
    } else {
        t = atan2(p.y, p.x) / (2.0 * 3.14159265) + 0.5;
    }

    return mix_stops(clamp(t, 0.0, 1.0), num_stops);
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
