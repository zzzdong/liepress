//! 视觉元素模块 - 纯数据描述，与渲染后端解耦

use crate::text::TextRun;
use vello_cpu::kurbo::{BezPath, Point, Rect, Vec2};

/// 视觉元素枚举 - 纯数据描述，可被任何渲染后端解释
///
/// 设计原则：
/// - 纯数据，不包含任何渲染逻辑
/// - 使用标准几何类型（来自 kurbo）
/// - 支持嵌套组合（Group）
/// - 支持自定义扩展（Custom）
pub enum VisualElement {
    // ---- 基础图形 ----
    Rect {
        rect: Rect,
        style: FillStrokeStyle,
    },
    /// 圆角矩形
    RoundedRect {
        rect: Rect,
        radii: (f64, f64, f64, f64), // (top-left, top-right, bottom-right, bottom-left)
        style: FillStrokeStyle,
    },
    Circle {
        center: Point,
        radius: f64,
        style: FillStrokeStyle,
    },
    Line {
        start: Point,
        end: Point,
        style: StrokeStyle,
    },
    Polyline {
        points: Vec<Point>,
        style: StrokeStyle,
    },
    Path {
        path: BezPath,
        style: FillStrokeStyle,
    },

    // ---- 渐变路径 ----
    GradientPath {
        path: BezPath,
        gradient: GradientDef,
        stroke: Option<Stroke>,
    },

    // ---- 文本 ----
    /// 文本行 - 包含一行中的所有 Run
    ///
    /// 分页以 TextLine 为单位，确保行不会被截断。
    /// 渲染时遍历 runs，每个 Run 对应 parley 的一个 GlyphRun
    TextLine {
        /// 该行的所有 Run
        runs: Vec<TextRun>,
        /// 行的边界框（页面绝对坐标）
        bounds: Rect,
        /// 该行的高度（来自 LineMetrics.line_height）
        line_height: f32,
    },

    // ---- 图片 ----
    Image {
        /// 图片位置（左上角）
        position: Point,
        /// 图片显示尺寸（点）
        size: Vec2,
        /// 图片原始像素尺寸（宽, 高）
        pixel_size: (u32, u32),
        /// 图片数据（字节）
        data: Vec<u8>,
        /// 图片格式（如 "png", "jpeg"）
        format: String,
        /// 替代文本
        alt: String,
    },

    // ---- 变换组合 ----
    Group {
        children: Vec<VisualElement>,
        transform: Option<Transform>,
    },

    // ---- 层级分组（z-index） ----
    /// 具有 z-index 的分组
    ///
    /// 渲染时所有同级 ZGroup 按 z_index 升序渲染（低值在下层，高值在上层）。
    /// 非 ZGroup 的元素视为 z_index = 0。
    ZGroup {
        z_index: i32,
        children: Vec<VisualElement>,
    },
}

impl Clone for VisualElement {
    fn clone(&self) -> Self {
        match self {
            VisualElement::Rect { rect, style } => VisualElement::Rect {
                rect: *rect,
                style: style.clone(),
            },
            VisualElement::RoundedRect { rect, radii, style } => VisualElement::RoundedRect {
                rect: *rect,
                radii: *radii,
                style: style.clone(),
            },
            VisualElement::Circle {
                center,
                radius,
                style,
            } => VisualElement::Circle {
                center: *center,
                radius: *radius,
                style: style.clone(),
            },
            VisualElement::Line { start, end, style } => VisualElement::Line {
                start: *start,
                end: *end,
                style: style.clone(),
            },
            VisualElement::Polyline { points, style } => VisualElement::Polyline {
                points: points.clone(),
                style: style.clone(),
            },
            VisualElement::Path { path, style } => VisualElement::Path {
                path: path.clone(),
                style: style.clone(),
            },
            VisualElement::GradientPath {
                path,
                gradient,
                stroke,
            } => VisualElement::GradientPath {
                path: path.clone(),
                gradient: gradient.clone(),
                stroke: stroke.clone(),
            },
            VisualElement::TextLine {
                runs,
                bounds,
                line_height,
            } => VisualElement::TextLine {
                runs: runs.clone(),
                bounds: *bounds,
                line_height: *line_height,
            },
            VisualElement::Group {
                children,
                transform,
            } => VisualElement::Group {
                children: children.clone(),
                transform: *transform,
            },
            VisualElement::Image {
                position,
                size,
                pixel_size,
                data,
                format,
                alt,
            } => VisualElement::Image {
                position: *position,
                size: *size,
                pixel_size: *pixel_size,
                data: data.clone(),
                format: format.clone(),
                alt: alt.clone(),
            },
            VisualElement::ZGroup { z_index, children } => VisualElement::ZGroup {
                z_index: *z_index,
                children: children.clone(),
            },
        }
    }
}

impl std::fmt::Debug for VisualElement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VisualElement::Rect { rect, style } => f
                .debug_struct("Rect")
                .field("rect", rect)
                .field("style", style)
                .finish(),
            VisualElement::RoundedRect { rect, radii, style } => f
                .debug_struct("RoundedRect")
                .field("rect", rect)
                .field("radii", radii)
                .field("style", style)
                .finish(),
            VisualElement::Circle {
                center,
                radius,
                style,
            } => f
                .debug_struct("Circle")
                .field("center", center)
                .field("radius", radius)
                .field("style", style)
                .finish(),
            VisualElement::Line { start, end, style } => f
                .debug_struct("Line")
                .field("start", start)
                .field("end", end)
                .field("style", style)
                .finish(),
            VisualElement::Polyline { points, style } => f
                .debug_struct("Polyline")
                .field("points", points)
                .field("style", style)
                .finish(),
            VisualElement::Path { path: _, style } => f
                .debug_struct("Path")
                .field("path", &"<BezPath>")
                .field("style", style)
                .finish(),
            VisualElement::GradientPath {
                path: _,
                gradient,
                stroke,
            } => f
                .debug_struct("GradientPath")
                .field("path", &"<BezPath>")
                .field("gradient", gradient)
                .field("stroke", stroke)
                .finish(),
            VisualElement::TextLine {
                runs,
                bounds,
                line_height,
            } => f
                .debug_struct("TextLine")
                .field("runs", runs)
                .field("bounds", bounds)
                .field("line_height", line_height)
                .finish(),
            VisualElement::Group {
                children,
                transform,
            } => f
                .debug_struct("Group")
                .field("children", children)
                .field("transform", transform)
                .finish(),
            VisualElement::Image {
                position,
                size,
                pixel_size,
                data,
                format,
                alt,
            } => f
                .debug_struct("Image")
                .field("position", position)
                .field("size", size)
                .field("pixel_size", pixel_size)
                .field("data", &format!("<{} bytes>", data.len()))
                .field("format", format)
                .field("alt", alt)
                .finish(),
            VisualElement::ZGroup { z_index, children } => f
                .debug_struct("ZGroup")
                .field("z_index", z_index)
                .field("children", children)
                .finish(),
        }
    }
}

/// 2D 变换
#[derive(Clone, Copy, Debug, Default)]
pub struct Transform {
    pub translate: Vec2,
    pub rotate: f64, // 弧度
    pub scale: Vec2,
}

impl Transform {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_translation(x: f64, y: f64) -> Self {
        Self {
            translate: Vec2::new(x, y),
            ..Default::default()
        }
    }

    pub fn with_rotation(angle: f64) -> Self {
        Self {
            rotate: angle,
            ..Default::default()
        }
    }

    pub fn with_scale(x: f64, y: f64) -> Self {
        Self {
            scale: Vec2::new(x, y),
            ..Default::default()
        }
    }
}

/// 颜色
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Self = Self::new(0, 0, 0);
    pub const WHITE: Self = Self::new(255, 255, 255);
    pub const RED: Self = Self::new(255, 0, 0);
    pub const GREEN: Self = Self::new(0, 128, 0);
    pub const BLUE: Self = Self::new(0, 0, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn with_alpha(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn set_alpha(&self, alpha: f64) -> Self {
        Self {
            r: self.r,
            g: self.g,
            b: self.b,
            a: (alpha.clamp(0.0, 1.0) * 255.0) as u8,
        }
    }

    /// 从十六进制字符串解析颜色
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Self::new(r, g, b))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Self::with_alpha(r, g, b, a))
            }
            _ => None,
        }
    }

    pub fn as_vello_color(&self) -> vello_cpu::color::AlphaColor<vello_cpu::color::Srgb> {
        vello_cpu::color::AlphaColor::from_rgba8(self.r, self.g, self.b, self.a)
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::new(0, 0, 0)
    }
}

/// 渐变定义
#[derive(Clone, Debug)]
pub struct GradientDef {
    /// 渐变停止点列表 (offset 0.0~1.0, color)
    pub stops: Vec<(f64, Color)>,
}

impl GradientDef {
    pub fn new(stops: Vec<(f64, Color)>) -> Self {
        Self { stops }
    }
}

/// 描边样式
#[derive(Clone, Debug)]
pub struct Stroke {
    pub color: Color,
    pub width: f64,
}

impl Stroke {
    pub fn new(color: Color, width: f64) -> Self {
        Self { color, width }
    }
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            color: Color::new(0, 0, 0),
            width: 1.0,
        }
    }
}

/// 描边样式（简化版，用于 Line/Polyline）
#[derive(Clone, Debug)]
pub struct StrokeStyle {
    pub color: Color,
    pub width: f64,
}

impl StrokeStyle {
    pub fn new(color: Color, width: f64) -> Self {
        Self { color, width }
    }
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            color: Color::new(0, 0, 0),
            width: 1.0,
        }
    }
}

/// 填充和描边组合样式
#[derive(Clone, Debug, Default)]
pub struct FillStrokeStyle {
    pub fill: Option<Color>,
    pub stroke: Option<Stroke>,
}

impl FillStrokeStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_fill(mut self, color: Color) -> Self {
        self.fill = Some(color);
        self
    }

    pub fn with_stroke(mut self, color: Color, width: f64) -> Self {
        self.stroke = Some(Stroke::new(color, width));
        self
    }
}
