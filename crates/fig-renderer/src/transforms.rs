//! 2D transform and geometry helpers for the renderer.

use fig_parser::types;

/// Re-export fig-parser's core geometry types for renderer convenience.
pub type Matrix = types::Matrix;
pub type Vec2 = types::Vec2;

/// 2D point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl From<Vec2> for Point {
    fn from(v: Vec2) -> Self {
        Self { x: v.x, y: v.y }
    }
}

/// Axis-aligned bounding rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl Rect {
    pub const ZERO: Self = Self {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 0.0,
        max_y: 0.0,
    };

    pub fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub fn from_origin_size(origin_x: f32, origin_y: f32, width: f32, height: f32) -> Self {
        Self {
            min_x: origin_x,
            min_y: origin_y,
            max_x: origin_x + width,
            max_y: origin_y + height,
        }
    }

    pub fn width(&self) -> f32 {
        self.max_x - self.min_x
    }

    pub fn height(&self) -> f32 {
        self.max_y - self.min_y
    }

    pub fn is_empty(&self) -> bool {
        self.max_x <= self.min_x || self.max_y <= self.min_y
    }

    pub fn union(&self, other: &Rect) -> Rect {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        Rect {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    pub fn transform(&self, m: &Matrix) -> Rect {
        let corners = [
            transform_point(m, self.min_x, self.min_y),
            transform_point(m, self.max_x, self.min_y),
            transform_point(m, self.min_x, self.max_y),
            transform_point(m, self.max_x, self.max_y),
        ];
        let xs: Vec<f32> = corners.iter().map(|p| p.x).collect();
        let ys: Vec<f32> = corners.iter().map(|p| p.y).collect();
        Rect {
            min_x: xs.iter().cloned().fold(f32::INFINITY, f32::min),
            min_y: ys.iter().cloned().fold(f32::INFINITY, f32::min),
            max_x: xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
            max_y: ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
        }
    }
}

/// Identity matrix.
pub const IDENTITY: Matrix = Matrix {
    m00: 1.0,
    m01: 0.0,
    m02: 0.0,
    m10: 0.0,
    m11: 1.0,
    m12: 0.0,
};

/// Multiply two affine matrices: a * b.
pub fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    Matrix {
        m00: a.m00 * b.m00 + a.m01 * b.m10,
        m01: a.m00 * b.m01 + a.m01 * b.m11,
        m02: a.m00 * b.m02 + a.m01 * b.m12 + a.m02,
        m10: a.m10 * b.m00 + a.m11 * b.m10,
        m11: a.m10 * b.m01 + a.m11 * b.m11,
        m12: a.m10 * b.m02 + a.m11 * b.m12 + a.m12,
    }
}

/// Multiply optional parent matrix with optional local matrix.
pub fn compose(parent: Option<&Matrix>, local: Option<&Matrix>) -> Option<Matrix> {
    match (parent, local) {
        (Some(p), Some(l)) => Some(multiply(p, l)),
        (Some(p), None) => Some(*p),
        (None, Some(l)) => Some(*l),
        (None, None) => None,
    }
}

/// Transform a point by a matrix.
pub fn transform_point(m: &Matrix, x: f32, y: f32) -> Point {
    Point {
        x: m.m00 * x + m.m01 * y + m.m02,
        y: m.m10 * x + m.m11 * y + m.m12,
    }
}

/// Identity 4x4 column-major matrix.
pub const IDENTITY_4X4: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// Convert a Matrix to a column-major [f32; 16] (4x4) suitable for GPU uniforms.
/// The Figma matrix is a 2D affine transform:
///   [m00  m01  m02]
///   [m10  m11  m12]
///   [0    0    1  ]
/// We convert to a 4x4 column-major:
///   [m00, m10, 0, 0, m01, m11, 0, 0, 0, 0, 1, 0, m02, m12, 0, 1]
pub fn to_column_major_4x4(m: &Matrix) -> [f32; 16] {
    [
        m.m00, m.m10, 0.0, 0.0, m.m01, m.m11, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, m.m02, m.m12, 0.0, 1.0,
    ]
}

/// Y-axis flip matrix (Figma uses Y-down, GPU often uses Y-up).
pub const FLIP_Y: Matrix = Matrix {
    m00: 1.0,
    m01: 0.0,
    m02: 0.0,
    m10: 0.0,
    m11: -1.0,
    m12: 0.0,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_multiply() {
        let result = multiply(&IDENTITY, &IDENTITY);
        assert!((result.m00 - 1.0).abs() < 1e-6);
        assert!((result.m11 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn compose_none_none() {
        assert!(compose(None, None).is_none());
    }

    #[test]
    fn compose_identity() {
        let result = compose(Some(&IDENTITY), Some(&IDENTITY));
        assert!(result.is_some());
    }

    #[test]
    fn rect_union() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 15.0, 15.0);
        let u = a.union(&b);
        assert!((u.min_x - 0.0).abs() < 1e-6);
        assert!((u.max_x - 15.0).abs() < 1e-6);
    }
}
