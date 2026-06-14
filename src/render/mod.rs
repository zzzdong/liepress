//! 渲染模块 - 提供多种渲染后端

pub use pdf::{PdfDocumentGenerator, PdfRenderer};
pub use pixmap::{PixmapDocumentGenerator, PixmapRenderer};
pub use svg::{SvgDocumentGenerator, SvgRenderer};
use vello_cpu::kurbo::{BezPath, Point, Rect};

use crate::visual::{FillStrokeStyle, GradientDef, Stroke, StrokeStyle, Transform, VisualElement};

pub mod pdf;
mod pixmap;
mod svg;

pub trait PageRenderer {
    /// 绘制矩形
    fn draw_rect(&mut self, rect: Rect, style: &FillStrokeStyle);
    fn draw_rounded_rect(
        &mut self,
        rect: Rect,
        radii: (f64, f64, f64, f64),
        style: &FillStrokeStyle,
    );

    /// 绘制圆形
    fn draw_circle(&mut self, center: Point, radius: f64, style: &FillStrokeStyle);

    /// 绘制线段
    fn draw_line(&mut self, start: Point, end: Point, style: &StrokeStyle);

    /// 绘制折线
    fn draw_polyline(&mut self, points: &[Point], style: &StrokeStyle);

    /// 绘制路径
    fn draw_path(&mut self, path: &BezPath, style: &FillStrokeStyle);

    /// 绘制渐变填充路径
    fn draw_gradient_path(
        &mut self,
        path: &BezPath,
        gradient: &GradientDef,
        stroke: Option<&Stroke>,
    );

    /// 绘制文本 Run
    ///
    /// 每个 run 对应 parley 的一个 GlyphRun
    ///
    /// 参数：
    /// - `run`: 文本 Run，包含 `start`（左上角偏移）和 `glyphs`（相对偏移的坐标）
    /// - `position`: run 在页面上的位置（绝对坐标 = layout.position + run.start）
    fn draw_text_run(&mut self, run: &crate::text::TextRun, position: Point);

    /// 绘制图片
    ///
    /// # 参数
    /// - `data`: 图片字节数据
    /// - `format`: 图片格式（如 "png", "jpeg"）
    /// - `position`: 图片位置（左上角）
    /// - `size`: 图片显示尺寸（点）
    fn draw_image(&mut self, data: &[u8], format: &str, position: Point, size: (f64, f64));

    /// 开始一个变换组
    fn begin_group(&mut self, transform: Option<&Transform>);

    /// 结束一个变换组
    fn end_group(&mut self);

    /// 渲染视觉元素序列的默认实现
    ///
    /// 遍历元素树并调用对应的原子操作。
    /// 支持 ZGroup 的 z_index 排序：所有顶层 ZGroup 会按 z_index 升序渲染
    /// （低值在下层，高值在上层），非 ZGroup 元素视为 z_index = 0。
    fn render_elements(&mut self, elements: &[VisualElement])
    where
        Self: Sized,
    {
        let mut flat: Vec<(i32, &VisualElement)> = Vec::new();
        for element in elements {
            match element {
                VisualElement::ZGroup { z_index, children } => {
                    for child in children {
                        flat.push((*z_index, child));
                    }
                }
                other => {
                    flat.push((0, other));
                }
            }
        }
        flat.sort_by_key(|(z, _)| *z);
        for (_, element) in flat {
            self.render_element(element);
        }
    }

    /// 渲染单个视觉元素
    fn render_element(&mut self, element: &VisualElement)
    where
        Self: Sized,
    {
        match element {
            VisualElement::Rect { rect, style } => {
                self.draw_rect(*rect, style);
            }
            VisualElement::RoundedRect { rect, radii, style } => {
                self.draw_rounded_rect(*rect, *radii, style);
            }
            VisualElement::Circle {
                center,
                radius,
                style,
            } => {
                self.draw_circle(*center, *radius, style);
            }
            VisualElement::Line { start, end, style } => {
                self.draw_line(*start, *end, style);
            }
            VisualElement::Polyline { points, style } => {
                self.draw_polyline(points, style);
            }
            VisualElement::Path { path, style } => {
                self.draw_path(path, style);
            }
            VisualElement::GradientPath {
                path,
                gradient,
                stroke,
            } => {
                self.draw_gradient_path(path, gradient, stroke.as_ref());
            }
            VisualElement::TextLine {
                runs,
                bounds,
                line_height: _,
            } => {
                // bounds.origin 是行的左上角位置
                let position = bounds.origin();
                for run in runs {
                    self.draw_text_run(run, position);
                }
            }
            VisualElement::Group {
                children,
                transform,
            } => {
                self.begin_group(transform.as_ref());
                self.render_elements(children);
                self.end_group();
            }
            VisualElement::Image {
                position,
                size,
                pixel_size: _,
                data,
                format,
                alt: _,
            } => {
                self.draw_image(data, format, *position, (size.x, size.y));
            }
            VisualElement::ZGroup { children, .. } => {
                self.render_elements(children);
            }
        }
    }
}
