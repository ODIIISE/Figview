//! GPU texture management for images and glyph atlases.

use std::collections::HashMap;

/// Manages GPU textures for image fills and glyph atlases.
pub struct TextureManager {
    textures: HashMap<String, GpuTexture>,
    sampler: wgpu::Sampler,
}

struct GpuTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
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

    /// Upload raw image bytes as an RGBA8 texture.
    /// `hash` is the image's Figma hash string, used for caching.
    /// `width` and `height` are the decoded image dimensions.
    /// `rgba_pixels` is the raw RGBA pixel data.
    pub fn upload(
        &mut self,
        hash: &str,
        width: u32,
        height: u32,
        rgba_pixels: &[u8],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<&wgpu::TextureView> {
        if self.textures.contains_key(hash) {
            return Some(&self.textures[hash].view);
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

        self.textures.insert(
            hash.to_string(),
            GpuTexture { texture, view },
        );

        Some(&self.textures[hash].view)
    }

    /// Get the global sampler.
    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
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