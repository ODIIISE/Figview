//! Core types for the Figma document model — only what the parser populates or the frontend consumes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId {
    pub session_id: u32,
    pub local_id: u32,
}
impl NodeId {
    pub fn new(session_id: u32, local_id: u32) -> Self {
        Self {
            session_id,
            local_id,
        }
    }
}
impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.session_id, self.local_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}
impl Vec2 {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Matrix {
    pub m00: f32,
    pub m01: f32,
    pub m02: f32,
    pub m10: f32,
    pub m11: f32,
    pub m12: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PathCommand {
    MoveTo {
        x: f32,
        y: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    CubicTo {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x: f32,
        y: f32,
    },
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindingRule {
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryPath {
    pub commands: Vec<PathCommand>,
    pub winding_rule: WindingRule,
    pub style_id: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorGeometry {
    pub paths: Vec<GeometryPath>,
    pub normalized_size: Option<Vec2>,
}

// ── Enums ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    Document,
    Canvas,
    Frame,
    Group,
    Text,
    Rectangle,
    Ellipse,
    Line,
    Polygon,
    Star,
    Vector,
    BooleanGroup,
    Component,
    ComponentSet,
    Instance,
    Section,
    RoundedRectangle,
    Table,
    Widget,
    Stamp,
    Unknown,
}
impl NodeType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "DOCUMENT" => Self::Document,
            "CANVAS" => Self::Canvas,
            "FRAME" => Self::Frame,
            "GROUP" => Self::Group,
            "TEXT" => Self::Text,
            "RECTANGLE" => Self::Rectangle,
            "ELLIPSE" => Self::Ellipse,
            "LINE" => Self::Line,
            "POLYGON" => Self::Polygon,
            "STAR" => Self::Star,
            "VECTOR" => Self::Vector,
            "BOOLEAN_GROUP" => Self::BooleanGroup,
            "COMPONENT" => Self::Component,
            "COMPONENT_SET" => Self::ComponentSet,
            "INSTANCE" => Self::Instance,
            "SECTION" => Self::Section,
            "ROUNDED_RECTANGLE" => Self::RoundedRectangle,
            "TABLE" => Self::Table,
            "WIDGET" => Self::Widget,
            "STAMP" => Self::Stamp,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodePhase {
    Created,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaintType {
    Solid,
    GradientLinear,
    GradientRadial,
    GradientAngular,
    GradientDiamond,
    Image,
    Emoji,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeAlign {
    Center,
    Inside,
    Outside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectType {
    InnerShadow,
    DropShadow,
    ForegroundBlur,
    BackgroundBlur,
}

// ── Structs ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paint {
    pub paint_type: PaintType,
    pub color: Option<Color>,
    pub opacity: f32,
    pub visible: bool,
    pub stops: Vec<ColorStop>,
    pub transform: Option<Matrix>,
    pub image_hash: Option<String>,
    pub gradient_handles: Vec<Vec2>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColorStop {
    pub color: Color,
    pub position: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub effect_type: EffectType,
    pub color: Option<Color>,
    pub offset: Vec2,
    pub radius: f32,
    pub visible: bool,
    pub spread: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CornerRadii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextData {
    pub characters: String,
    pub font_family: Option<String>,
    pub font_weight: f32,
    pub font_size: f32,
    pub letter_spacing: f32,
    pub line_height: Option<f32>,
    pub fills: Vec<Paint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FigNode {
    pub guid: Option<NodeId>,
    pub node_type: NodeType,
    pub name: String,
    pub phase: Option<NodePhase>,
    pub parent_id: Option<NodeId>,
    pub position: Option<String>,
    pub visible: bool,
    pub opacity: f32,
    pub locked: bool,
    pub size: Option<Vec2>,
    pub transform: Option<Matrix>,
    pub corner_radius: Option<f32>,
    pub corner_radii: Option<CornerRadii>,
    pub clips_content: bool,
    pub blend_mode: String,
    pub fill_paints: Vec<Paint>,
    pub background_paints: Vec<Paint>,
    pub stroke_paints: Vec<Paint>,
    pub stroke_weight: f32,
    pub stroke_align: StrokeAlign,
    pub effects: Vec<Effect>,
    pub fill_geometry: Vec<GeometryPath>,
    pub stroke_geometry: Vec<GeometryPath>,
    pub vector_geometry: Option<VectorGeometry>,
    pub text_data: Option<TextData>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: NodeId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FigHeader {
    pub prelude: String,
    pub version: u32,
    pub schema_def_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FigDocument {
    pub header: FigHeader,
    pub file_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    pub pages: Vec<Page>,
    pub nodes: Vec<FigNode>,
    pub children_map: HashMap<String, Vec<String>>,
    pub image_hashes: Vec<String>,
    pub thumbnail: Vec<u8>,
}
