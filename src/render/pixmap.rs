//! Pixmap 渲染器 - 使用 vello_cpu 渲染为位图

use crate::error::Result;
use crate::generator::Page;
use crate::render::PageRenderer;
use crate::text::{TextLayout, TextStyle, layout_text};
use crate::visual::{Color, FillStrokeStyle, GradientDef, Stroke, StrokeStyle, Transform};
use std::sync::Arc;
use vello_cpu::RenderContext;
use vello_cpu::kurbo::{Affine, BezPath, Circle, Point, Rect, Shape, Stroke as KurboStroke};
use vello_cpu::peniko::color::AlphaColor;
use vello_cpu::peniko::{Extend, ImageQuality, ImageSampler};
use vello_cpu::{Image, ImageSource, Pixmap, Resources};

pub struct PixmapDocumentGenerator {
    name: String,
}

impl PixmapDocumentGenerator {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

impl PixmapDocumentGenerator {
    fn render_page(&mut self, page: &Page) -> Result<()> {
        PixmapRenderer::new(page.width, page.height, DEFAULT_DPI);

        Ok(())
    }
}

/// 默认 DPI - 72 DPI 是 PostScript 的标准
pub const DEFAULT_DPI: f32 = 72.0;

/// Vello CPU 渲染器，输出 Pixmap
///
/// 实现 Renderer trait 和 PageRenderer trait，将 VisualElement 渲染为位图
///
/// # 坐标系统
/// - Document 使用 pt (point) 作为单位
/// - Pixmap 使用像素作为单位
/// - 通过 DPI 进行转换: pixels = pt * dpi / 72
pub struct PixmapRenderer {
    ctx: RenderContext,
    resources: Resources,
    width: u32,
    height: u32,
    /// DPI (dots per inch)，默认 72
    dpi: f32,
    /// 缩放因子 = dpi / 72.0
    scale: f32,
    /// 当前页面的变换矩阵（用于整体缩放）
    transform: Affine,
}

impl PixmapRenderer {
    /// 创建新的 PixmapRenderer
    ///
    /// # 参数
    /// - `width`: 页面宽度（pt）
    /// - `height`: 页面高度（pt）
    /// - `dpi`: 分辨率，默认 72
    pub fn new(width: f32, height: f32, dpi: f32) -> Self {
        let scale = dpi / DEFAULT_DPI;
        let pixel_width = (width * scale) as u32;
        let pixel_height = (height * scale) as u32;

        Self {
            ctx: RenderContext::new(pixel_width as u16, pixel_height as u16),
            resources: Resources::new(),
            width: pixel_width,
            height: pixel_height,
            dpi,
            scale,
            transform: Affine::scale(scale as f64),
        }
    }

    /// 使用默认 DPI (72) 创建渲染器
    pub fn new_default_dpi(width: f32, height: f32) -> Self {
        Self::new(width, height, DEFAULT_DPI)
    }

    pub fn render_to_png(&mut self) -> Result<Vec<u8>> {
        let mut pixmap = Pixmap::new(self.width as u16, self.height as u16);
        self.ctx.render_to_pixmap(&mut self.resources, &mut pixmap);

        let png = pixmap
            .into_png()
            .map_err(|e| crate::error::Error::VisualElementError(e.to_string()))?;

        Ok(png)
    }

    fn color_to_vello(color: &Color) -> AlphaColor<vello_cpu::color::Srgb> {
        AlphaColor::from_rgba8(color.r, color.g, color.b, color.a)
    }

    /// 将 Stroke 转换为 KurboStroke 并设置到渲染上下文
    fn set_stroke_style(&mut self, stroke: &Stroke) {
        // 线宽也需要缩放
        let scaled_width = stroke.width * self.scale as f64;
        let kurbo_stroke = KurboStroke::new(scaled_width);
        self.ctx.set_stroke(kurbo_stroke);
    }

    /// 将 pt 坐标转换为像素坐标
    fn pt_to_px(&self, pt: f64) -> f64 {
        pt * self.scale as f64
    }

    /// 缩放矩形
    fn scale_rect(&self, rect: &Rect) -> Rect {
        Rect::new(
            rect.x0 * self.scale as f64,
            rect.y0 * self.scale as f64,
            rect.x1 * self.scale as f64,
            rect.y1 * self.scale as f64,
        )
    }

    /// 缩放点
    fn scale_point(&self, point: &Point) -> Point {
        Point::new(point.x * self.scale as f64, point.y * self.scale as f64)
    }

    /// 缩放路径
    fn scale_path(&self, path: &BezPath) -> BezPath {
        let mut new_path = BezPath::new();
        for el in path.elements() {
            match el {
                vello_cpu::kurbo::PathEl::MoveTo(p) => {
                    new_path.move_to(self.scale_point(p));
                }
                vello_cpu::kurbo::PathEl::LineTo(p) => {
                    new_path.line_to(self.scale_point(p));
                }
                vello_cpu::kurbo::PathEl::QuadTo(p1, p2) => {
                    new_path.quad_to(self.scale_point(p1), self.scale_point(p2));
                }
                vello_cpu::kurbo::PathEl::CurveTo(p1, p2, p3) => {
                    new_path.curve_to(
                        self.scale_point(p1),
                        self.scale_point(p2),
                        self.scale_point(p3),
                    );
                }
                vello_cpu::kurbo::PathEl::ClosePath => {
                    new_path.close_path();
                }
            }
        }
        new_path
    }

    /// 创建缩放后的文本布局
    ///
    /// 当 DPI 不为 72 时，需要重新计算文本布局以使用缩放后的字体大小
    fn create_scaled_text_layout(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
    ) -> Option<TextLayout> {
        if (self.scale - 1.0).abs() < f32::EPSILON {
            // 如果 scale 接近 1，使用原始布局
            return None;
        }

        // 创建缩放后的样式
        let scaled_style = TextStyle {
            font_family: style.font_family.clone(),
            font_size: style.font_size * self.scale as f64,
            font_weight: style.font_weight.clone(),
            font_style: style.font_style.clone(),
            color: style.color,
            align: style.align,
            url: style.url.clone(),
            decoration: style.decoration,
        };

        // 缩放最大宽度
        let scaled_max_width = max_width.map(|w| w * self.scale as f64);

        // 重新布局文本
        Some(layout_text(
            &[(text, &scaled_style)],
            scaled_max_width,
            style.align,
        ))
    }
}

impl PageRenderer for PixmapRenderer {
    fn draw_rect(&mut self, rect: Rect, style: &FillStrokeStyle) {
        let scaled_rect = self.scale_rect(&rect);

        if let Some(fill) = &style.fill {
            let color = Self::color_to_vello(fill);
            self.ctx.set_paint(color);
            self.ctx.fill_rect(&scaled_rect);
        }

        if let Some(stroke) = &style.stroke {
            let color = Self::color_to_vello(&stroke.color);
            self.ctx.set_paint(color);
            self.set_stroke_style(stroke);
            self.ctx.stroke_rect(&scaled_rect);
        }
    }

    fn draw_circle(&mut self, center: Point, radius: f64, style: &FillStrokeStyle) {
        let scaled_center = self.scale_point(&center);
        let scaled_radius = radius * self.scale as f64;
        let circle = Circle::new(scaled_center, scaled_radius);
        let path = circle.to_path(0.1);

        if let Some(fill) = &style.fill {
            let color = Self::color_to_vello(fill);
            self.ctx.set_paint(color);
            self.ctx.fill_path(&path);
        }

        if let Some(stroke) = &style.stroke {
            let color = Self::color_to_vello(&stroke.color);
            self.ctx.set_paint(color);
            self.set_stroke_style(stroke);
            self.ctx.stroke_path(&path);
        }
    }

    fn draw_line(&mut self, start: Point, end: Point, style: &StrokeStyle) {
        let scaled_start = self.scale_point(&start);
        let scaled_end = self.scale_point(&end);

        let color = Self::color_to_vello(&style.color);
        self.ctx.set_paint(color);
        // 线宽也需要缩放
        self.ctx
            .set_stroke(KurboStroke::new(style.width * self.scale as f64));

        let mut path = BezPath::new();
        path.move_to(scaled_start);
        path.line_to(scaled_end);
        self.ctx.stroke_path(&path);
    }

    fn draw_polyline(&mut self, points: &[Point], style: &StrokeStyle) {
        if points.len() < 2 {
            return;
        }

        let mut path = BezPath::new();
        let scaled_start = self.scale_point(&points[0]);
        path.move_to(scaled_start);
        for point in &points[1..] {
            let scaled_point = self.scale_point(point);
            path.line_to(scaled_point);
        }

        let color = Self::color_to_vello(&style.color);
        self.ctx.set_paint(color);
        // 线宽也需要缩放
        self.ctx
            .set_stroke(KurboStroke::new(style.width * self.scale as f64));
        self.ctx.stroke_path(&path);
    }

    fn draw_path(&mut self, path: &BezPath, style: &FillStrokeStyle) {
        let scaled_path = self.scale_path(path);

        if let Some(fill) = &style.fill {
            let color = Self::color_to_vello(fill);
            self.ctx.set_paint(color);
            self.ctx.fill_path(&scaled_path);
        }

        if let Some(stroke) = &style.stroke {
            let color = Self::color_to_vello(&stroke.color);
            self.ctx.set_paint(color);
            self.set_stroke_style(stroke);
            self.ctx.stroke_path(&scaled_path);
        }
    }

    fn draw_gradient_path(
        &mut self,
        path: &BezPath,
        gradient: &GradientDef,
        stroke: Option<&Stroke>,
    ) {
        use vello_cpu::kurbo::Point as KurboPoint;
        use vello_cpu::peniko::color::{ColorSpaceTag, DynamicColor, HueDirection};
        use vello_cpu::peniko::{
            ColorStop, ColorStops, GradientKind, InterpolationAlphaSpace, LinearGradientPosition,
        };
        use vello_cpu::peniko::{Extend, Gradient};

        let scaled_path = self.scale_path(path);

        let stops: Vec<ColorStop> = gradient
            .stops
            .iter()
            .map(|(offset, color)| ColorStop {
                offset: *offset as f32,
                color: DynamicColor::from_alpha_color(AlphaColor::from_rgba8(
                    color.r, color.g, color.b, color.a,
                )),
            })
            .collect();

        // 使用缩放后路径的包围盒来确定渐变坐标
        let bounds = scaled_path.bounding_box();
        let peniko_gradient = Gradient {
            kind: GradientKind::Linear(LinearGradientPosition {
                start: KurboPoint::new(bounds.x0, bounds.y0),
                end: KurboPoint::new(bounds.x1, bounds.y0),
            }),
            extend: Extend::Pad,
            interpolation_cs: ColorSpaceTag::Srgb,
            hue_direction: HueDirection::default(),
            interpolation_alpha_space: InterpolationAlphaSpace::Premultiplied,
            stops: ColorStops::from(stops.as_slice()),
        };

        self.ctx.set_paint(peniko_gradient);
        self.ctx.fill_path(&scaled_path);

        if let Some(stroke) = stroke {
            let color = Self::color_to_vello(&stroke.color);
            self.ctx.set_paint(color);
            self.set_stroke_style(stroke);
            self.ctx.stroke_path(&scaled_path);
        }
    }

    fn draw_text_run(&mut self, run: &crate::text::TextRun, position: Point) {
        use vello_cpu::kurbo::Affine;

        // position: run 在页面上的位置（绝对坐标）
        // run.glyphs[].x/y: 相对 run.start 的偏移量
        // 所以，glyph 的实际位置 = position + glyph.x/y

        // 缩放位置
        let scaled_position = self.scale_point(&position);
        let transform = Affine::translate((scaled_position.x, scaled_position.y));

        // 将 glyph 转换为 vello_cpu 的 glyph
        // 注意：vello_cpu 的 glyph.x/y 是相对 transform 的偏移量
        let glyphs: Vec<vello_cpu::Glyph> = run
            .glyphs
            .iter()
            .map(|g| vello_cpu::Glyph {
                id: g.id,
                x: g.x, // 相对 run.start 的偏移
                y: g.y, // 相对 run.start 的偏移
            })
            .collect();

        if glyphs.is_empty() {
            return;
        }

        // 设置颜色
        self.ctx.set_paint(run.color.as_vello_color());

        // 渲染 glyph run
        // transform 将 glyph 的坐标平移到页面位置
        self.ctx
            .glyph_run(&mut self.resources, &run.font_data)
            .font_size(run.font_size)
            .glyph_transform(transform)
            .fill_glyphs(glyphs.into_iter());

        // 绘制删除线
        if run.decoration == crate::text::TextDecoration::LineThrough {
            // 计算删除线位置（基线以上 0.3em）
            let first_glyph = &run.glyphs[0];
            let strike_y = position.y + first_glyph.y as f64 - run.font_size as f64 * 0.3;
            let line_x1 = position.x + first_glyph.x as f64;
            let line_x2 = line_x1 + run.advance as f64;
            let stroke_w = (run.font_size as f64 * 0.06).max(0.5);

            let scaled_start = self.scale_point(&Point::new(line_x1, strike_y));
            let scaled_end = self.scale_point(&Point::new(line_x2, strike_y));

            let color = run.color.as_vello_color();
            self.ctx.set_paint(color);
            self.ctx
                .set_stroke(KurboStroke::new(stroke_w * self.scale as f64));

            let mut path = BezPath::new();
            path.move_to(scaled_start);
            path.line_to(scaled_end);
            self.ctx.stroke_path(&path);
        }
    }

    fn begin_group(&mut self, _transform: Option<&Transform>) {
        // vello_cpu 暂不支持变换组，直接渲染子元素
        // 未来可以在这里保存/恢复渲染状态
    }

    fn end_group(&mut self) {
        // 恢复渲染状态
    }

    fn draw_image(&mut self, data: &[u8], _format: &str, position: Point, size: (f64, f64)) {
        if data.is_empty() {
            let scaled_position = self.scale_point(&position);
            let scaled_size = (size.0 * self.scale as f64, size.1 * self.scale as f64);
            let rect = Rect::new(
                scaled_position.x,
                scaled_position.y,
                scaled_position.x + scaled_size.0,
                scaled_position.y + scaled_size.1,
            );
            let style = crate::visual::FillStrokeStyle {
                fill: Some(crate::visual::Color::new(200, 200, 200)),
                stroke: Some(crate::visual::Stroke {
                    color: crate::visual::Color::new(150, 150, 150),
                    width: 1.0,
                }),
            };
            self.draw_rect(rect, &style);
            return;
        }

        let img = match image::load_from_memory(data) {
            Ok(img) => img,
            Err(_) => return,
        };
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        let width_u16 = width as u16;
        let height_u16 = height as u16;

        let pixels: Vec<vello_cpu::peniko::color::PremulRgba8> = rgba
            .chunks_exact(4)
            .map(|pixel| {
                let alpha = u16::from(pixel[3]);
                let premultiply = |component: u8| ((alpha * u16::from(component)) / 255) as u8;
                vello_cpu::peniko::color::PremulRgba8 {
                    r: premultiply(pixel[0]),
                    g: premultiply(pixel[1]),
                    b: premultiply(pixel[2]),
                    a: pixel[3],
                }
            })
            .collect();

        let pixmap = Pixmap::from_parts(pixels, width_u16, height_u16);
        let arc_pixmap = Arc::new(pixmap);

        let image = Image {
            image: ImageSource::Pixmap(arc_pixmap),
            sampler: ImageSampler {
                x_extend: Extend::Pad,
                y_extend: Extend::Pad,
                quality: ImageQuality::High,
                alpha: 1.0,
            },
        };

        self.ctx.set_paint(image);

        let scaled_position = self.scale_point(&position);
        let scaled_size = (size.0 * self.scale as f64, size.1 * self.scale as f64);
        let rect = Rect::new(
            scaled_position.x,
            scaled_position.y,
            scaled_position.x + scaled_size.0,
            scaled_position.y + scaled_size.1,
        );
        self.ctx.fill_rect(&rect);
    }
}
