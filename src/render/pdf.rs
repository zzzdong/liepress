//! PDF 渲染器 - 使用 krilla 0.7.0

use std::collections::HashMap;

use krilla::color::Color as KrillaColor;
use krilla::color::rgb::Color as RgbColor;
use krilla::document::Document;
use krilla::geom::{PathBuilder, Point as KrillaPoint, Rect as KrillaRect, Size, Transform as KrillaTransform};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{
    Fill, FillRule, LineCap, LineJoin, LinearGradient, Paint, SpreadMethod, Stop,
    Stroke as KrillaStroke,
};
use krilla::surface::Surface;
use krilla::text::Font;
use krilla::annotation::{Annotation, LinkAnnotation, Target};
use krilla::action::{Action, LinkAction};
use vello_cpu::kurbo::{BezPath, PathEl, Point, Rect, Shape};

use crate::error::Result;
use crate::generator::Page;
use crate::render::PageRenderer;
use crate::visual::{Color, FillStrokeStyle, GradientDef, Stroke, StrokeStyle, Transform};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FontCacheKey {
    data_ptr: *const u8,
    data_len: usize,
    index: u32,
}

impl FontCacheKey {
    fn from_font_data(font_data: &parley::FontData) -> Self {
        let data = font_data.data.as_ref();
        Self {
            data_ptr: data.as_ptr(),
            data_len: data.len(),
            index: font_data.index,
        }
    }
}

pub struct PdfDocumentGenerator {
    name: String,
    document: Document,
}

impl PdfDocumentGenerator {
    pub fn new(name: String) -> Self {
        Self {
            name,
            document: Document::new(),
        }
    }

    pub fn render_page(&mut self, page: &Page) -> Result<()> {
        let size = Size::from_wh(page.width, page.height)
            .ok_or_else(|| crate::error::Error::VisualElementError("invalid page size".into()))?;
        let mut krilla_page = self.document.start_page_with(PageSettings::new(size));
        let mut surface = krilla_page.surface();

        // 先绘制白色背景
        {
            let mut pb = PathBuilder::new();
            pb.move_to(0.0, 0.0);
            pb.line_to(page.width, 0.0);
            pb.line_to(page.width, page.height);
            pb.line_to(0.0, page.height);
            pb.close();
            if let Some(path) = pb.finish() {
                let fill = Fill {
                    paint: Paint::from(RgbColor::new(255, 255, 255)),
                    opacity: NormalizedF32::ONE,
                    rule: FillRule::NonZero,
                };
                surface.set_fill(Some(fill));
                surface.draw_path(&path);
            }
        }

        let links = {
            let mut renderer = PdfRenderer::new(&mut surface, page.height);
            renderer.render_elements(&page.elements);
            renderer.take_links()
        };

        surface.finish();

        // 添加超链接注释
        for (x, y, w, h, url) in links {
            if let Some(rect) = KrillaRect::from_xywh(x, y, w, h) {
                let action = Action::from(LinkAction::new(url.clone()));
                let link_annotation = LinkAnnotation::new(rect, Target::Action(action));
                krilla_page.add_annotation(Annotation::new_link(link_annotation, None));
            }
        }

        krilla_page.finish();
        Ok(())
    }

    pub fn finalize(self) -> Result<Vec<u8>> {
        self.document
            .finish()
            .map_err(|e| crate::error::Error::VisualElementError(format!("{:?}", e)))
    }
}

pub struct PdfRenderer<'a, 's> {
    surface: &'s mut Surface<'a>,
    page_height: f32,
    font_cache: HashMap<FontCacheKey, Font>,
    push_count: usize,
    links: Vec<(f32, f32, f32, f32, String)>,
}

impl<'a, 's> PdfRenderer<'a, 's> {
    fn new(surface: &'s mut Surface<'a>, page_height: f32) -> Self {
        Self {
            surface,
            page_height,
            font_cache: HashMap::new(),
            push_count: 0,
            links: Vec::new(),
        }
    }

    fn take_links(&mut self) -> Vec<(f32, f32, f32, f32, String)> {
        std::mem::take(&mut self.links)
    }

    fn color_to_krilla(color: &Color) -> RgbColor {
        RgbColor::new(color.r, color.g, color.b)
    }

    fn opacity_from_alpha(alpha: u8) -> NormalizedF32 {
        NormalizedF32::new(alpha as f32 / 255.0).unwrap_or(NormalizedF32::ONE)
    }

    fn set_fill_color(&mut self, color: &Color) {
        let krilla_color = Self::color_to_krilla(color);
        let fill = Fill {
            paint: Paint::from(krilla_color),
            opacity: Self::opacity_from_alpha(color.a),
            rule: FillRule::NonZero,
        };
        self.surface.set_fill(Some(fill));
    }

    fn set_stroke_color(&mut self, color: &Color, width: f64) {
        let krilla_color = Self::color_to_krilla(color);
        let stroke = KrillaStroke {
            paint: Paint::from(krilla_color),
            opacity: Self::opacity_from_alpha(color.a),
            width: width as f32,
            miter_limit: 4.0,
            line_cap: LineCap::default(),
            line_join: LineJoin::default(),
            dash: None,
        };
        self.surface.set_stroke(Some(stroke));
    }

    fn clear_stroke(&mut self) {
        self.surface.set_stroke(None);
    }

    fn clear_fill(&mut self) {
        self.surface.set_fill(None);
    }

    fn apply_fill_stroke(&mut self, style: &FillStrokeStyle) {
        if let Some(fill) = &style.fill {
            self.set_fill_color(fill);
        } else {
            self.clear_fill();
        }

        if let Some(stroke) = &style.stroke {
            self.set_stroke_color(&stroke.color, stroke.width);
        } else {
            self.clear_stroke();
        }
    }

    fn bezpath_to_krilla_path(path: &BezPath) -> Option<krilla::geom::Path> {
        let mut pb = PathBuilder::new();
        for el in path.elements() {
            match el {
                PathEl::MoveTo(p) => {
                    pb.move_to(p.x as f32, p.y as f32);
                }
                PathEl::LineTo(p) => {
                    pb.line_to(p.x as f32, p.y as f32);
                }
                PathEl::QuadTo(p1, p2) => {
                    pb.quad_to(p1.x as f32, p1.y as f32, p2.x as f32, p2.y as f32);
                }
                PathEl::CurveTo(p1, p2, p3) => {
                    pb.cubic_to(
                        p1.x as f32,
                        p1.y as f32,
                        p2.x as f32,
                        p2.y as f32,
                        p3.x as f32,
                        p3.y as f32,
                    );
                }
                PathEl::ClosePath => {
                    pb.close();
                }
            }
        }
        pb.finish()
    }
}

impl PageRenderer for PdfRenderer<'_, '_> {
    fn draw_rect(&mut self, rect: Rect, style: &FillStrokeStyle) {
        let mut pb = PathBuilder::new();
        pb.move_to(rect.x0 as f32, rect.y0 as f32);
        pb.line_to(rect.x1 as f32, rect.y0 as f32);
        pb.line_to(rect.x1 as f32, rect.y1 as f32);
        pb.line_to(rect.x0 as f32, rect.y1 as f32);
        pb.close();
        if let Some(path) = pb.finish() {
            self.apply_fill_stroke(style);
            self.surface.draw_path(&path);
        }
    }

    fn draw_circle(&mut self, center: Point, radius: f64, style: &FillStrokeStyle) {
        let k = 0.552_284_8;
        let r = radius as f32;
        let cx = center.x as f32;
        let cy = center.y as f32;
        let kr = r * k;

        let mut pb = PathBuilder::new();
        pb.move_to(cx, cy - r);
        pb.cubic_to(cx + kr, cy - r, cx + r, cy - kr, cx + r, cy);
        pb.cubic_to(cx + r, cy + kr, cx + kr, cy + r, cx, cy + r);
        pb.cubic_to(cx - kr, cy + r, cx - r, cy + kr, cx - r, cy);
        pb.cubic_to(cx - r, cy - kr, cx - kr, cy - r, cx, cy - r);
        pb.close();

        if let Some(path) = pb.finish() {
            self.apply_fill_stroke(style);
            self.surface.draw_path(&path);
        }
    }

    fn draw_line(&mut self, start: Point, end: Point, style: &StrokeStyle) {
        let mut pb = PathBuilder::new();
        pb.move_to(start.x as f32, start.y as f32);
        pb.line_to(end.x as f32, end.y as f32);

        if let Some(path) = pb.finish() {
            self.clear_fill();
            self.set_stroke_color(&style.color, style.width);
            self.surface.draw_path(&path);
        }
    }

    fn draw_polyline(&mut self, points: &[Point], style: &StrokeStyle) {
        if points.len() < 2 {
            return;
        }

        let mut pb = PathBuilder::new();
        pb.move_to(points[0].x as f32, points[0].y as f32);
        for p in &points[1..] {
            pb.line_to(p.x as f32, p.y as f32);
        }

        if let Some(path) = pb.finish() {
            self.clear_fill();
            self.set_stroke_color(&style.color, style.width);
            self.surface.draw_path(&path);
        }
    }

    fn draw_path(&mut self, path: &BezPath, style: &FillStrokeStyle) {
        if let Some(krilla_path) = Self::bezpath_to_krilla_path(path) {
            self.apply_fill_stroke(style);
            self.surface.draw_path(&krilla_path);
        }
    }

    fn draw_gradient_path(
        &mut self,
        path: &BezPath,
        gradient: &GradientDef,
        stroke: Option<&Stroke>,
    ) {
        let Some(krilla_path) = Self::bezpath_to_krilla_path(path) else {
            return;
        };

        let bounds = path.bounding_box();

        let stops: Vec<Stop> = gradient
            .stops
            .iter()
            .map(|(offset, color)| Stop {
                offset: NormalizedF32::new(*offset as f32).unwrap_or(NormalizedF32::ONE),
                color: KrillaColor::from(RgbColor::new(color.r, color.g, color.b)),
                opacity: NormalizedF32::new(color.a as f32 / 255.0).unwrap_or(NormalizedF32::ONE),
            })
            .collect();

        let linear_gradient = LinearGradient {
            x1: bounds.x0 as f32,
            y1: bounds.y0 as f32,
            x2: bounds.x1 as f32,
            y2: bounds.y0 as f32,
            transform: KrillaTransform::default(),
            spread_method: SpreadMethod::Pad,
            stops,
            anti_alias: false,
        };

        self.clear_stroke();
        let fill = Fill {
            paint: Paint::from(linear_gradient),
            opacity: NormalizedF32::ONE,
            rule: FillRule::NonZero,
        };
        self.surface.set_fill(Some(fill));
        self.surface.draw_path(&krilla_path);

        if let Some(stroke) = stroke {
            self.clear_fill();
            self.set_stroke_color(&stroke.color, stroke.width);
            self.surface.draw_path(&krilla_path);
        }
    }

    fn begin_group(&mut self, transform: Option<&Transform>) {
        if let Some(t) = transform {
            if t.translate.x != 0.0 || t.translate.y != 0.0 {
                self.surface
                    .push_transform(&KrillaTransform::from_translate(
                        t.translate.x as f32,
                        t.translate.y as f32,
                    ));
                self.push_count += 1;
            }
            if t.rotate != 0.0 {
                self.surface
                    .push_transform(&KrillaTransform::from_rotate(t.rotate.to_degrees() as f32));
                self.push_count += 1;
            }
            if t.scale.x != 1.0 || t.scale.y != 1.0 {
                self.surface.push_transform(&KrillaTransform::from_scale(
                    t.scale.x as f32,
                    t.scale.y as f32,
                ));
                self.push_count += 1;
            }
        }
    }

    fn end_group(&mut self) {
        for _ in 0..self.push_count {
            self.surface.pop();
        }
        self.push_count = 0;
    }

    fn draw_text_run(&mut self, run: &crate::text::TextRun, position: Point) {
        use krilla::text::GlyphId;

        // position: 文本行左上角在页面上的位置（行顶）
        // run.baseline_x/y: 基线相对行顶的偏移
        //
        // 注意：krilla 的 draw_glyphs 把 origin 当作基线位置，
        // 而 position 是行顶。所以直接用 baseline 值计算基线原点：
        //   - origin = position + (baseline_x, baseline_y)
        //   - glyph 坐标减去 baseline 偏移，变成相对基线的归一化偏移

        if run.glyphs.is_empty() {
            return;
        }

        let font_key = FontCacheKey::from_font_data(&run.font_data);

        if let std::collections::hash_map::Entry::Vacant(e) = self.font_cache.entry(font_key) {
            let data = run.font_data.data.as_ref();
            if let Some(font) = Font::new(krilla::Data::from(data.to_vec()), run.font_data.index) {
                e.insert(font);
            } else {
                return;
            }
        }

        let krilla_font = match self.font_cache.get(&font_key) {
            Some(f) => f.clone(),
            None => return,
        };

        // 基线位置 = 行顶 + baseline 偏移
        let krilla_origin = KrillaPoint::from_xy(
            position.x as f32 + run.baseline_x,
            position.y as f32 + run.baseline_y,
        );

        // 如果 run 有 URL，收集链接区域
        if let Some(ref url) = run.url {
            let link_x = position.x as f32 + run.baseline_x;
            let link_y = position.y as f32;
            let link_w = run.advance;
            let link_h = run.font_size * 1.4;
            self.links.push((link_x, link_y, link_w, link_h, url.clone()));
        }

        let mut krilla_glyphs = Vec::new();
        for g in &run.glyphs {
            if g.id == 0 {
                continue;
            }
            // glyph 坐标转为相对基线的归一化偏移
            krilla_glyphs.push(krilla::text::KrillaGlyph::new(
                GlyphId::new(g.id),
                0.0,
                (g.x - run.baseline_x) / run.font_size,
                (g.y - run.baseline_y) / run.font_size,
                0.0,
                0..0,
                None,
            ));
        }

        if krilla_glyphs.is_empty() {
            return;
        }

        // 确保没有残留的 stroke 状态（否则 krilla 会生成 2 Tr fill+stroke 模式 + 灰色描边）
        self.surface.set_stroke(None);

        let krilla_color = RgbColor::new(run.color.r, run.color.g, run.color.b);
        let paint = Paint::from(krilla_color);
        let opacity = NormalizedF32::new(run.color.a as f32 / 255.0).unwrap_or(NormalizedF32::ONE);
        let fill = Fill {
            paint,
            opacity,
            rule: FillRule::NonZero,
        };

        self.surface.set_fill(Some(fill.clone()));
        self.surface.draw_glyphs(
            krilla_origin,
            &krilla_glyphs,
            krilla_font,
            &run.text,
            run.font_size,
            false,
        );

        // 绘制删除线
        if run.decoration == crate::text::TextDecoration::LineThrough {
            // 删除线位置：基线以上 0.3em
            let strike_y = position.y as f32 + run.baseline_y - run.font_size * 0.3;
            let line_x1 = position.x as f32 + run.baseline_x;
            let line_x2 = line_x1 + run.advance;
            let stroke_w = (run.font_size * 0.06).max(0.5);

            // 保存当前 fill，清除，设置 stroke
            self.surface.set_fill(None);
            let krilla_color = RgbColor::new(run.color.r, run.color.g, run.color.b);
            let stroke = KrillaStroke {
                paint: Paint::from(krilla_color),
                opacity: NormalizedF32::new(run.color.a as f32 / 255.0).unwrap_or(NormalizedF32::ONE),
                width: stroke_w,
                miter_limit: 4.0,
                line_cap: LineCap::default(),
                line_join: LineJoin::default(),
                dash: None,
            };
            self.surface.set_stroke(Some(stroke));

            let mut pb = PathBuilder::new();
            pb.move_to(line_x1, strike_y);
            pb.line_to(line_x2, strike_y);
            if let Some(path) = pb.finish() {
                self.surface.draw_path(&path);
            }

            // 恢复 stroke 状态
            self.surface.set_stroke(None);
            self.surface.set_fill(Some(fill.clone()));
        }
    }

    fn draw_image(&mut self, data: &[u8], format: &str, position: Point, size: (f64, f64)) {
        use krilla::image::Image;

        let image = match format.to_lowercase().as_str() {
            "png" => Image::from_png(krilla::Data::from(data.to_vec()), true),
            "jpeg" | "jpg" => Image::from_jpeg(krilla::Data::from(data.to_vec()), true),
            "gif" => Image::from_gif(krilla::Data::from(data.to_vec()), true),
            "webp" => Image::from_webp(krilla::Data::from(data.to_vec()), true),
            _ => return,
        };

        let image = match image {
            Ok(img) => img,
            Err(_) => return,
        };

        self.surface
            .push_transform(&KrillaTransform::from_translate(
                position.x as f32,
                position.y as f32,
            ));

        let Some(image_size) = Size::from_wh(size.0 as f32, size.1 as f32) else {
            self.surface.pop();
            return;
        };
        self.surface.draw_image(image, image_size);
        self.surface.pop();
    }
}
