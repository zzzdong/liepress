//! Debug 渲染器 - 用于测试验证
#![allow(dead_code)]

use liepress::PageRenderer;
use liepress::text::TextRun;
use liepress::visual::{FillStrokeStyle, GradientDef, Stroke, StrokeStyle, Transform};
use vello_cpu::kurbo::{BezPath, Point, Rect};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum DebugElement {
    Rect {
        rect: Rect,
        fill: Option<(u8, u8, u8, u8)>,
        stroke: Option<(f64, (u8, u8, u8, u8))>,
    },
    Circle {
        center: (f64, f64),
        radius: f64,
        fill: Option<(u8, u8, u8, u8)>,
        stroke: Option<(f64, (u8, u8, u8, u8))>,
    },
    Line {
        start: (f64, f64),
        end: (f64, f64),
        width: f64,
        color: (u8, u8, u8, u8),
    },
    Polyline {
        points: Vec<(f64, f64)>,
        width: f64,
        color: (u8, u8, u8, u8),
    },
    Path {
        elements: Vec<(f64, f64)>,
        fill: Option<(u8, u8, u8, u8)>,
        stroke: Option<(f64, (u8, u8, u8, u8))>,
    },
    Text {
        text: String,
        position: (f64, f64),
        font_size: f32,
        color: (u8, u8, u8, u8),
    },
    Image {
        position: (f64, f64),
        size: (f64, f64),
        format: String,
        data_len: usize,
    },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DebugPage {
    pub width: f32,
    pub height: f32,
    pub elements: Vec<DebugElement>,
}

impl DebugPage {
    pub fn texts(&self) -> Vec<&DebugElement> {
        self.elements
            .iter()
            .filter(|e| matches!(e, DebugElement::Text { .. }))
            .collect()
    }

    pub fn lines(&self) -> Vec<&DebugElement> {
        self.elements
            .iter()
            .filter(|e| matches!(e, DebugElement::Line { .. }))
            .collect()
    }

    pub fn rects(&self) -> Vec<&DebugElement> {
        self.elements
            .iter()
            .filter(|e| matches!(e, DebugElement::Rect { .. }))
            .collect()
    }
}

pub struct DebugRenderer {
    pages: Vec<DebugPage>,
    current_page: Option<DebugPage>,
    width: f32,
    height: f32,
}

impl DebugRenderer {
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            current_page: None,
            width: 0.0,
            height: 0.0,
        }
    }

    pub fn begin_page(&mut self, w: f32, h: f32) {
        if let Some(page) = self.current_page.take()
            && !page.elements.is_empty()
        {
            self.pages.push(page);
        }
        self.width = w;
        self.height = h;
    }

    pub fn finalize(mut self) -> Vec<DebugPage> {
        if let Some(page) = self.current_page.take()
            && !page.elements.is_empty()
        {
            self.pages.push(page);
        }
        self.pages
    }

    fn current_page(&mut self) -> &mut DebugPage {
        self.current_page.get_or_insert_with(|| DebugPage {
            width: self.width,
            height: self.height,
            elements: vec![],
        })
    }
}

impl PageRenderer for DebugRenderer {
    fn draw_rect(&mut self, rect: Rect, style: &FillStrokeStyle) {
        let fill = style.fill.map(|c| (c.r, c.g, c.b, c.a));
        let stroke = style
            .stroke
            .as_ref()
            .map(|s| (s.width, (s.color.r, s.color.g, s.color.b, s.color.a)));
        self.current_page().elements.push(DebugElement::Rect {
            rect: Rect::new(rect.x0, rect.y0, rect.x1, rect.y1),
            fill,
            stroke,
        });
    }

    fn draw_rounded_rect(
        &mut self,
        rect: Rect,
        _radii: (f64, f64, f64, f64),
        style: &FillStrokeStyle,
    ) {
        // 简化为普通矩形记录
        let fill = style.fill.map(|c| (c.r, c.g, c.b, c.a));
        let stroke = style
            .stroke
            .as_ref()
            .map(|s| (s.width, (s.color.r, s.color.g, s.color.b, s.color.a)));
        self.current_page().elements.push(DebugElement::Rect {
            rect: Rect::new(rect.x0, rect.y0, rect.x1, rect.y1),
            fill,
            stroke,
        });
    }

    fn draw_circle(&mut self, center: Point, radius: f64, style: &FillStrokeStyle) {
        let fill = style.fill.map(|c| (c.r, c.g, c.b, c.a));
        let stroke = style
            .stroke
            .as_ref()
            .map(|s| (s.width, (s.color.r, s.color.g, s.color.b, s.color.a)));
        self.current_page().elements.push(DebugElement::Circle {
            center: (center.x, center.y),
            radius,
            fill,
            stroke,
        });
    }

    fn draw_line(&mut self, start: Point, end: Point, style: &StrokeStyle) {
        self.current_page().elements.push(DebugElement::Line {
            start: (start.x, start.y),
            end: (end.x, end.y),
            width: style.width,
            color: (style.color.r, style.color.g, style.color.b, style.color.a),
        });
    }

    fn draw_polyline(&mut self, points: &[Point], style: &StrokeStyle) {
        let pts: Vec<_> = points.iter().map(|p| (p.x, p.y)).collect();
        self.current_page().elements.push(DebugElement::Polyline {
            points: pts,
            width: style.width,
            color: (style.color.r, style.color.g, style.color.b, style.color.a),
        });
    }

    fn draw_path(&mut self, path: &BezPath, style: &FillStrokeStyle) {
        let elements: Vec<_> = path
            .elements()
            .iter()
            .map(|el| match el {
                vello_cpu::kurbo::PathEl::MoveTo(p) => (p.x, p.y),
                vello_cpu::kurbo::PathEl::LineTo(p) => (p.x, p.y),
                vello_cpu::kurbo::PathEl::QuadTo(p1, _) => (p1.x, p1.y),
                vello_cpu::kurbo::PathEl::CurveTo(p1, _, _) => (p1.x, p1.y),
                vello_cpu::kurbo::PathEl::ClosePath => (f64::NAN, f64::NAN),
            })
            .collect();
        let fill = style.fill.map(|c| (c.r, c.g, c.b, c.a));
        let stroke = style
            .stroke
            .as_ref()
            .map(|s| (s.width, (s.color.r, s.color.g, s.color.b, s.color.a)));
        self.current_page().elements.push(DebugElement::Path {
            elements,
            fill,
            stroke,
        });
    }

    fn draw_gradient_path(&mut self, path: &BezPath, _: &GradientDef, stroke: Option<&Stroke>) {
        let elements: Vec<_> = path
            .elements()
            .iter()
            .map(|el| match el {
                vello_cpu::kurbo::PathEl::MoveTo(p) => (p.x, p.y),
                vello_cpu::kurbo::PathEl::LineTo(p) => (p.x, p.y),
                vello_cpu::kurbo::PathEl::QuadTo(p1, _) => (p1.x, p1.y),
                vello_cpu::kurbo::PathEl::CurveTo(p1, _, _) => (p1.x, p1.y),
                vello_cpu::kurbo::PathEl::ClosePath => (f64::NAN, f64::NAN),
            })
            .collect();
        let stroke_data = stroke.map(|s| (s.width, (s.color.r, s.color.g, s.color.b, s.color.a)));
        self.current_page().elements.push(DebugElement::Path {
            elements,
            fill: None,
            stroke: stroke_data,
        });
    }

    fn draw_text_run(&mut self, run: &TextRun, position: Point) {
        self.current_page().elements.push(DebugElement::Text {
            text: run.text.clone(),
            position: (position.x, position.y),
            font_size: run.font_size,
            color: (run.color.r, run.color.g, run.color.b, run.color.a),
        });
    }

    fn draw_image(&mut self, data: &[u8], format: &str, position: Point, size: (f64, f64)) {
        self.current_page().elements.push(DebugElement::Image {
            position: (position.x, position.y),
            size,
            format: format.to_string(),
            data_len: data.len(),
        });
    }

    fn begin_group(&mut self, _transform: Option<&Transform>) {
        // Debug renderer doesn't need to track groups
    }

    fn end_group(&mut self) {
        // Debug renderer doesn't need to track groups
    }
}
