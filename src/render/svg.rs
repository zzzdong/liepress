//! SVG 渲染器 - 将 VisualElement 渲染为 SVG 字符串

use crate::error::Result;
use crate::generator::Page;
use crate::render::PageRenderer;
use crate::visual::{Color, FillStrokeStyle, GradientDef, Stroke, StrokeStyle, Transform};
use std::collections::HashMap;
use vello_cpu::kurbo::{BezPath, PathSeg, Point, Rect};

pub struct SvgDocumentGenerator {
    name: String,
}

impl SvgDocumentGenerator {
    pub fn new(name: String) -> Self {
        Self { name }
    }

    pub fn render_page(&mut self, page: &Page) -> Result<()> {
        let mut renderer = SvgRenderer::new(page.width, page.height);
        renderer.render_elements(&page.elements);
        let svg = renderer.finalize();
        let filename = format!("page_{}.svg", page.index);
        std::fs::write(filename, svg)?;
        Ok(())
    }
}

pub struct SvgRenderer {
    svg_content: String,
    gradient_map: HashMap<*const GradientDef, usize>,
    width: f32,
    height: f32,
    defs: String,
}

impl SvgRenderer {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            svg_content: String::new(),
            gradient_map: HashMap::new(),
            width,
            height,
            defs: String::new(),
        }
    }

    pub fn finalize(&self) -> String {
        let mut svg = String::new();
        svg.push_str(&format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#,
            self.width, self.height, self.width, self.height,
        ));
        svg.push('\n');

        // 添加白色背景
        svg.push_str(&format!(
            r#"<rect x="0" y="0" width="{}" height="{}" fill="white" />"#,
            self.width, self.height,
        ));
        svg.push('\n');

        if !self.defs.is_empty() {
            svg.push_str(&format!("<defs>\n{}</defs>\n", self.defs));
        }
        svg.push_str(&self.svg_content);
        svg.push_str("</svg>\n");
        svg
    }

    fn color_to_css(color: &Color) -> String {
        if color.a == 255 {
            format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
        } else {
            format!(
                "rgba({}, {}, {}, {})",
                color.r,
                color.g,
                color.b,
                color.a as f64 / 255.0
            )
        }
    }

    fn escape_xml(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    fn bezpath_to_svg_d(path: &BezPath, close: bool) -> String {
        let mut d = String::new();
        for seg in path.segments() {
            match seg {
                PathSeg::Line(line) => {
                    if d.is_empty() {
                        d.push_str(&format!("M {} {}", line.p0.x, line.p0.y));
                    }
                    d.push_str(&format!(" L {} {}", line.p1.x, line.p1.y));
                }
                PathSeg::Quad(quad) => {
                    if d.is_empty() {
                        d.push_str(&format!("M {} {}", quad.p0.x, quad.p0.y));
                    }
                    d.push_str(&format!(
                        " Q {} {} {} {}",
                        quad.p1.x, quad.p1.y, quad.p2.x, quad.p2.y
                    ));
                }
                PathSeg::Cubic(cubic) => {
                    if d.is_empty() {
                        d.push_str(&format!("M {} {}", cubic.p0.x, cubic.p0.y));
                    }
                    d.push_str(&format!(
                        " C {} {} {} {} {} {}",
                        cubic.p1.x, cubic.p1.y, cubic.p2.x, cubic.p2.y, cubic.p3.x, cubic.p3.y
                    ));
                }
            }
        }
        if close && !d.is_empty() {
            d.push_str(" Z");
        }
        d
    }
}

impl PageRenderer for SvgRenderer {
    fn draw_rect(&mut self, rect: Rect, style: &FillStrokeStyle) {
        let mut attrs = format!(
            r#"x="{}" y="{}" width="{}" height="{}""#,
            rect.x0,
            rect.y0,
            rect.width(),
            rect.height()
        );

        if let Some(fill) = &style.fill {
            attrs.push_str(&format!(r#" fill="{}""#, Self::color_to_css(fill)));
        } else {
            attrs.push_str(r#" fill="none""#);
        }

        if let Some(stroke) = &style.stroke {
            attrs.push_str(&format!(
                r#" stroke="{}" stroke-width="{}""#,
                Self::color_to_css(&stroke.color),
                stroke.width
            ));
        }

        self.svg_content.push_str(&format!("<rect {} />\n", attrs));
    }

    fn draw_circle(&mut self, center: Point, radius: f64, style: &FillStrokeStyle) {
        let mut attrs = format!(r#"cx="{}" cy="{}" r="{}""#, center.x, center.y, radius);

        if let Some(fill) = &style.fill {
            attrs.push_str(&format!(r#" fill="{}""#, Self::color_to_css(fill)));
        } else {
            attrs.push_str(r#" fill="none""#);
        }

        if let Some(stroke) = &style.stroke {
            attrs.push_str(&format!(
                r#" stroke="{}" stroke-width="{}""#,
                Self::color_to_css(&stroke.color),
                stroke.width
            ));
        }

        self.svg_content
            .push_str(&format!("<circle {} />\n", attrs));
    }

    fn draw_line(&mut self, start: Point, end: Point, style: &StrokeStyle) {
        let line = format!(
            r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}" />"#,
            start.x,
            start.y,
            end.x,
            end.y,
            Self::color_to_css(&style.color),
            style.width
        );
        self.svg_content.push_str(&line);
        self.svg_content.push('\n');
    }

    fn draw_polyline(&mut self, points: &[Point], style: &StrokeStyle) {
        if points.len() < 2 {
            return;
        }

        let points_str: Vec<String> = points.iter().map(|p| format!("{}, {}", p.x, p.y)).collect();

        let polyline = format!(
            r#"<polyline points="{}" fill="none" stroke="{}" stroke-width="{}" />"#,
            points_str.join(" "),
            Self::color_to_css(&style.color),
            style.width
        );
        self.svg_content.push_str(&polyline);
        self.svg_content.push('\n');
    }

    fn draw_path(&mut self, path: &BezPath, style: &FillStrokeStyle) {
        let close = style.fill.is_some();
        let d = Self::bezpath_to_svg_d(path, close);
        let mut attrs = format!(r#"d="{}""#, d);

        if let Some(fill) = &style.fill {
            attrs.push_str(&format!(r#" fill="{}""#, Self::color_to_css(fill)));
        } else {
            attrs.push_str(r#" fill="none""#);
        }

        if let Some(stroke) = &style.stroke {
            attrs.push_str(&format!(
                r#" stroke="{}" stroke-width="{}""#,
                Self::color_to_css(&stroke.color),
                stroke.width
            ));
        }

        self.svg_content.push_str(&format!("<path {} />\n", attrs));
    }

    fn draw_gradient_path(
        &mut self,
        path: &BezPath,
        gradient: &GradientDef,
        stroke: Option<&Stroke>,
    ) {
        let d = Self::bezpath_to_svg_d(path, true);
        let mut attrs = format!(r#"d="{}""#, d);

        let ptr: *const GradientDef = gradient as *const GradientDef;
        let gradient_id = if let Some(&id) = self.gradient_map.get(&ptr) {
            id
        } else {
            let id = self.gradient_map.len();
            self.gradient_map.insert(ptr, id);
            let gradient_def = format!(
                r#"<linearGradient id="g{}" x1="0%" y1="0%" x2="100%" y2="0%">"#,
                id
            );
            self.defs.push_str(&gradient_def);
            self.defs.push('\n');
            for (offset, color) in &gradient.stops {
                self.defs.push_str(&format!(
                    r#"<stop offset="{}%" stop-color="{}" />"#,
                    (offset * 100.0) as i32,
                    Self::color_to_css(color)
                ));
                self.defs.push('\n');
            }
            self.defs.push_str("</linearGradient>\n");
            id
        };
        attrs.push_str(&format!(r#" fill="url(#g{})""#, gradient_id));

        if let Some(stroke) = stroke {
            attrs.push_str(&format!(
                r#" stroke="{}" stroke-width="{}""#,
                Self::color_to_css(&stroke.color),
                stroke.width
            ));
        }

        self.svg_content.push_str(&format!("<path {} />\n", attrs));
    }

    fn draw_text_run(&mut self, run: &crate::text::TextRun, position: Point) {
        if run.text.is_empty() || run.glyphs.is_empty() {
            return;
        }

        // 使用第一个 glyph 的位置来定位文本
        // position 是 TextLine.bounds.origin（行左上角绝对坐标）
        // glyph.x/y 是相对行左上角的偏移
        let first_glyph = &run.glyphs[0];
        let x = position.x + first_glyph.x as f64;
        let y = position.y + first_glyph.y as f64;
        let color = Self::color_to_css(&run.color);
        let font_size = run.font_size;
        let escaped_text = Self::escape_xml(&run.text);

        let text_svg = format!(
            r#"<text fill="{}" font-size="{}" x="{:.2}" y="{:.2}">{}</text>"#,
            color, font_size, x, y, escaped_text
        );
        self.svg_content.push_str(&text_svg);
        self.svg_content.push('\n');
    }

    fn begin_group(&mut self, transform: Option<&Transform>) {
        let mut attrs = String::new();

        if let Some(transform) = transform {
            let mut transforms = Vec::new();

            if transform.translate.x != 0.0 || transform.translate.y != 0.0 {
                transforms.push(format!(
                    "translate({}, {})",
                    transform.translate.x, transform.translate.y
                ));
            }

            if transform.rotate != 0.0 {
                transforms.push(format!("rotate({})", transform.rotate.to_degrees()));
            }

            if transform.scale.x != 1.0 || transform.scale.y != 1.0 {
                transforms.push(format!(
                    "scale({}, {})",
                    transform.scale.x, transform.scale.y
                ));
            }

            if !transforms.is_empty() {
                attrs.push_str(&format!(r#" transform="{}""#, transforms.join(" ")));
            }
        }

        self.svg_content.push_str(&format!("<g{}>\n", attrs));
    }

    fn end_group(&mut self) {
        self.svg_content.push_str("</g>\n");
    }

    fn draw_image(&mut self, data: &[u8], format: &str, position: Point, size: (f64, f64)) {
        if data.is_empty() {
            // 图片数据为空，绘制占位符矩形
            let rect = Rect::new(
                position.x,
                position.y,
                position.x + size.0,
                position.y + size.1,
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

        // 根据格式确定 MIME 类型
        let mime_type = match format.to_lowercase().as_str() {
            "png" => "image/png",
            "jpeg" | "jpg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            _ => "image/png", // 默认
        };

        // 将图片数据编码为 Base64
        let base64_data = base64::encode(data);
        let data_uri = format!("data:{};base64,{}", mime_type, base64_data);

        // 创建 <image> 元素
        let image_svg = format!(
            r#"<image x="{}" y="{}" width="{}" height="{}" href="{}" />"#,
            position.x, position.y, size.0, size.1, data_uri
        );

        self.svg_content.push_str(&image_svg);
        self.svg_content.push('\n');
    }
}

impl Default for SvgRenderer {
    fn default() -> Self {
        Self::new(0.0, 0.0)
    }
}
