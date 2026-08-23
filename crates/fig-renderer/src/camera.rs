//! Camera: viewport, zoom, and view/projection transforms.
//!
//! The camera maps Figma document coordinates (Y-down, origin top-left)
//! to GPU clip space (-1..1, Y-up by default in wgpu/Vulkan/Metal).
//!
//! The view matrix handles panning and zoom.
//! The projection matrix maps from world space to clip space.

/// Manages the viewport transform (pan + zoom).
#[derive(Debug, Clone)]
pub struct Camera {
    /// Pan offset in screen pixels (at current zoom).
    pub pan_x: f32,
    pub pan_y: f32,
    /// Zoom factor: 1.0 = 100%, 2.0 = 200%.
    pub zoom: f32,
    /// Viewport dimensions in logical pixels.
    pub viewport_width: f32,
    pub viewport_height: f32,
    /// Device pixel ratio for physical pixel rendering.
    pub dpr: f32,
    /// Minimum / maximum zoom.
    pub min_zoom: f32,
    pub max_zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            dpr: 1.0,
            min_zoom: 0.02,
            max_zoom: 16.0,
        }
    }
}

impl Camera {
    pub fn new(viewport_width: f32, viewport_height: f32, dpr: f32) -> Self {
        Self {
            viewport_width,
            viewport_height,
            dpr,
            ..Default::default()
        }
    }

    /// Resize the viewport.
    pub fn resize(&mut self, width: f32, height: f32, dpr: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
        self.dpr = dpr;
    }

    /// Set zoom, clamped to valid range.
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(self.min_zoom, self.max_zoom);
    }

    /// Zoom centered on a point in screen coordinates.
    pub fn zoom_at(&mut self, screen_x: f32, screen_y: f32, new_zoom: f32) {
        let world_x = (screen_x - self.pan_x) / self.zoom;
        let world_y = (screen_y - self.pan_y) / self.zoom;
        self.zoom = new_zoom.clamp(self.min_zoom, self.max_zoom);
        self.pan_x = screen_x - world_x * self.zoom;
        self.pan_y = screen_y - world_y * self.zoom;
    }

    /// Zoom centered on the viewport center.
    pub fn zoom_center(&mut self, new_zoom: f32) {
        self.zoom_at(
            self.viewport_width / 2.0,
            self.viewport_height / 2.0,
            new_zoom,
        );
    }

    /// Pan by a screen-space delta.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.pan_x += dx;
        self.pan_y += dy;
    }

    /// Fit a bounding rectangle into the viewport with padding.
    pub fn fit_rect(&mut self, rect: &super::transforms::Rect, padding: f32) {
        if rect.is_empty() {
            return;
        }
        let content_w = rect.width().max(1.0);
        let content_h = rect.height().max(1.0);
        let avail_w = (self.viewport_width - padding * 2.0).max(1.0);
        let avail_h = (self.viewport_height - padding * 2.0).max(1.0);
        self.zoom = (avail_w / content_w).min(avail_h / content_h).clamp(self.min_zoom, self.max_zoom);
        self.pan_x = (self.viewport_width - content_w * self.zoom) / 2.0 - rect.min_x * self.zoom;
        self.pan_y = (self.viewport_height - content_h * self.zoom) / 2.0 - rect.min_y * self.zoom;
    }

    /// Center the viewport on a rectangle.
    pub fn center_on(&mut self, rect: &super::transforms::Rect) {
        if rect.is_empty() {
            return;
        }
        self.pan_x = self.viewport_width / 2.0 - (rect.min_x + rect.width() / 2.0) * self.zoom;
        self.pan_y = self.viewport_height / 2.0 - (rect.min_y + rect.height() / 2.0) * self.zoom;
    }

    /// Convert screen coordinates to world (document) coordinates.
    pub fn screen_to_world(&self, screen_x: f32, screen_y: f32) -> (f32, f32) {
        let x = (screen_x - self.pan_x) / self.zoom;
        let y = (screen_y - self.pan_y) / self.zoom;
        (x, y)
    }

    /// Convert world coordinates to screen coordinates.
    pub fn world_to_screen(&self, world_x: f32, world_y: f32) -> (f32, f32) {
        let x = world_x * self.zoom + self.pan_x;
        let y = world_y * self.zoom + self.pan_y;
        (x, y)
    }

    /// Build the view-projection matrix for the GPU.
    ///
    /// Maps from Figma world coordinates to wgpu clip space (-1..1, Y-up).
    /// Figma uses Y-down (origin at top-left), wgpu uses Y-up (origin at center for
    /// NDC in Vulkan/Metal; DirectX NDC is Y-down, but we flip in projection).
    ///
    /// The view matrix: scale by zoom, translate by pan.
    /// The projection matrix: orthographic from [0, w] x [0, h] to [-1, 1] x [-1, 1].
    ///
    /// Returns column-major 4x4 matrix.
    pub fn view_projection_matrix(&self) -> [f32; 16] {
        let w = self.viewport_width;
        let h = self.viewport_height;

        // Orthographic projection: maps [0, w] x [0, h] → [-1, 1] x [-1, 1]
        // X: [0, w] → [-1, 1], Y: [0, h] → [1, -1] (flip Y for wgpu)
        let proj: [f32; 16] = [
            2.0 / w, 0.0,      0.0, 0.0,
            0.0,     -2.0 / h, 0.0, 0.0,
            0.0,      0.0,     1.0, 0.0,
            -1.0,     1.0,     0.0, 1.0,
        ];

        // View: pan + zoom
        let z = self.zoom;
        let view: [f32; 16] = [
            z,   0.0, 0.0, 0.0,
            0.0, z,   0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            self.pan_x, self.pan_y, 0.0, 1.0,
        ];

        // Multiply: P * V
        multiply_4x4(&proj, &view)
    }
}

/// Multiply two column-major 4x4 matrices.
fn multiply_4x4(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut result = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            result[col * 4 + row] =
                a[0 * 4 + row] * b[col * 4 + 0] +
                a[1 * 4 + row] * b[col * 4 + 1] +
                a[2 * 4 + row] * b[col * 4 + 2] +
                a[3 * 4 + row] * b[col * 4 + 3];
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_to_world_identity() {
        let cam = Camera {
            viewport_width: 800.0,
            viewport_height: 600.0,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            dpr: 1.0,
            min_zoom: 0.01,
            max_zoom: 16.0,
        };
        let (wx, wy) = cam.screen_to_world(400.0, 300.0);
        assert!((wx - 400.0).abs() < 0.01);
        assert!((wy - 300.0).abs() < 0.01);
    }

    #[test]
    fn zoom_at_preserves_point() {
        let mut cam = Camera {
            viewport_width: 800.0,
            viewport_height: 600.0,
            zoom: 1.0,
            pan_x: 100.0,
            pan_y: 50.0,
            dpr: 1.0,
            min_zoom: 0.01,
            max_zoom: 16.0,
        };
        let sx = 400.0;
        let sy = 300.0;
        let (wx1, wy1) = cam.screen_to_world(sx, sy);
        cam.zoom_at(sx, sy, 2.0);
        let (wx2, wy2) = cam.screen_to_world(sx, sy);
        assert!((wx1 - wx2).abs() < 0.01);
        assert!((wy1 - wy2).abs() < 0.01);
    }
}