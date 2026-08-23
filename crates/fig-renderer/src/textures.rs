//! GPU texture management for image fills.

use std::collections::HashMap;

/// A GPU texture plus the bind group needed to draw it with the image
/// pipeline (scene uniform at binding 0, texture at 1, sampler at 2).
pub struct ImageBinding {
    pub bind_group: wgpu::BindGroup,
}

/// Manages GPU textures for image fills.
pub struct TextureManager {
    textures: HashMap<String, ImageBinding>,
    sampler: wgpu::Sampler,
}

impl TextureManager {
    pub fn new(device: &wgpu::Device) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("texture sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            textures: HashMap::new(),
            sampler,
        }
    }

    /// Upload decoded RGBA8 pixels as a texture and create its bind group.
    /// `hash` is the image's Figma hash string, used for caching.
    #[allow(clippy::too_many_arguments)]
    pub fn upload(
        &mut self,
        hash: &str,
        width: u32,
        height: u32,
        rgba_pixels: &[u8],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image_bind_group_layout: &wgpu::BindGroupLayout,
        scene_uniform_buffer: &wgpu::Buffer,
    ) -> Option<&ImageBinding> {
        if rgba_pixels.is_empty() || width == 0 || height == 0 {
            return None;
        }
        if self.textures.contains_key(hash) {
            return self.textures.get(hash);
        }

        let texture_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("image_{}", hash)),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba_pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            texture_size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("image_bind_group_{}", hash)),
            layout: image_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: scene_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        self.textures
            .insert(hash.to_string(), ImageBinding { bind_group });
        self.textures.get(hash)
    }

    /// Get the bind group for an uploaded texture, if present.
    pub fn get(&self, hash: &str) -> Option<&ImageBinding> {
        self.textures.get(hash)
    }

    /// Check if a texture is cached.
    pub fn contains(&self, hash: &str) -> bool {
        self.textures.contains_key(hash)
    }

    /// Remove all cached textures.
    pub fn clear(&mut self) {
        self.textures.clear();
    }
}
