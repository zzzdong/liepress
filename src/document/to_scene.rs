//! 将 liepress 的 `Document`（已排版盒子树）转换为 lievisual 的 `Scene`（图元 IR）。
//!
//! 这是 PNG / SVG 输出后端共用的统一转换层：两个后端都先调用 [`document_to_scene`]
//! 构造 `lievisual::Scene`，再分别委托给 lievisual 的 `VelloPixmapRenderer` / `SvgRenderer`，
//! 从而移除原先手写的 vello_cpu / resvg 绘制逻辑。
//!
//! 坐标约定：转换全程在 pt 坐标系下进行，最后统一乘 `scale = dpi / 72` 归一到 px；
//! 这样 PNG 后端按 `scale` 设置画布像素尺寸，SVG 后端通过 `Scene.scale` 让 `viewBox`
//! 回到 pt 单位，二者共用同一份场景数据。

use lievisual::geometry::Color as LColor;
use lievisual::text::{FontStyle, TextAlign, TextBaseline, TextStyle as LTextStyle};
use lievisual::{
    Element, Fill, FillStrokeStyle, LineCap, LineJoin, Point, Rect as LRect, RichSpan, Scene,
    SceneImage, SceneNode, Stroke,
};

use crate::color::Color as PColor;
use crate::document::layout::{Block, BlockKind, Document, TableRow};
use crate::document::text::{Glyph, TextLine};
use crate::document::types::page::PageSettings;
use crate::document::types::ObjectFit as LayoutObjectFit;
use crate::document::types::ResolvedStyle;
use crate::output::common::{
    block_height, list_item_indent, table_border_segments, table_row_height, BQ_BAR_WIDTH,
    BQ_PAD_X, BQ_PAD_Y,
};

/// 代码块内边距（pt）。
const CODE_PADDING: f64 = 4.0;
/// 默认 DPI（与旧 PNG 后端保持一致）。
pub const DEFAULT_DPI: f64 = 144.0;

/// 颜色：liepress `Color`(u8) → lievisual `Color`(u8)。
#[inline]
fn lc(c: PColor) -> LColor {
    LColor::rgba(c.r, c.g, c.b, c.a)
}

/// 文档 → lievisual 场景。
///
/// `scale = dpi / 72`（`1pt = 1px @ 72dpi`）。所有几何坐标在 pt 下计算后乘 `scale`。
/// 返回的 `Scene.width/height` 已是缩放后的像素尺寸，`Scene.scale = scale` 供 SVG
/// 后端还原 `viewBox` 到 pt 单位。
pub fn document_to_scene(document: &Document, settings: &PageSettings, dpi: f64) -> Scene {
    let scale = dpi / 72.0;
    let mut b = SceneBuilder::new(scale);

    let page_w = settings.width_pt as f64;
    let content_w = settings.content_width() as f64;

    // 总高度 = 各顶层块高度之和（含 margin），与旧 png/svg 后端一致。
    let mut y = settings.margin_top_pt as f64;
    let x0 = settings.margin_left_pt as f64;

    for block in &document.blocks {
        b.draw_block(block, settings, x0, y, content_w);
        y += block_height(block, settings, x0);
    }

    let total_h = y + settings.margin_bottom_pt as f64;

    let mut scene = Scene::new(page_w * scale, total_h * scale);
    scene.scale = scale;
    scene.background = LColor::WHITE;
    scene.nodes = b.nodes;
    scene
}

/// 场景构建器：累积 `SceneNode`，所有坐标经 `s()` 缩放。
struct SceneBuilder {
    nodes: Vec<SceneNode>,
    scale: f64,
}

impl SceneBuilder {
    fn new(scale: f64) -> Self {
        Self {
            nodes: Vec::new(),
            scale,
        }
    }

    #[inline]
    fn s(&self, v: f64) -> f64 {
        v * self.scale
    }

    fn push(&mut self, el: Element, z: i32) {
        self.nodes.push(SceneNode {
            element: el,
            z_index: z,
            transform: None,
            opacity: 1.0,
            visible: true,
            clip: None,
            name: None,
        });
    }

    /// 矩形（填充 + 可选描边）。
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, fill: Option<LColor>, stroke: Option<Stroke>) {
        let fill = fill.unwrap_or_else(|| LColor::rgba(0, 0, 0, 0));
        let style = FillStrokeStyle {
            fill: Some(Fill::Solid(fill)),
            stroke,
        };
        self.push(Element::Rect { rect: LRect::new(self.s(x), self.s(y), self.s(x + w), self.s(y + h)), style }, 0);
    }

    /// 直线段。
    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: LColor, width: f64) {
        let stroke = Stroke {
            color,
            width: self.s(width),
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            dash_array: Vec::new(),
            dash_offset: 0.0,
            miter_limit: 4.0,
        };
        self.push(Element::Line { start: Point::new(self.s(x1), self.s(y1)), end: Point::new(self.s(x2), self.s(y2)), style: stroke }, 0);
    }

    /// 实心/描边圆。
    fn circle(&mut self, cx: f64, cy: f64, r: f64, fill: Option<LColor>, stroke: Option<Stroke>) {
        let fill = fill.unwrap_or_else(|| LColor::rgba(0, 0, 0, 0));
        let style = FillStrokeStyle {
            fill: Some(Fill::Solid(fill)),
            stroke,
        };
        self.push(Element::Circle { center: Point::new(self.s(cx), self.s(cy)), radius: self.s(r), style }, 0);
    }

    /// 单行文本：先绘制行内背景（如有），再绘制 `Element::Text`。
    ///
    /// 坐标：`position = (x + bounds.x0, y + bounds.y0)`，即行左上角（lievisual 的
    /// `baseline = Top` 语义）。`family` 为块级字体族（run 不携带族名）。
    fn text_line(&mut self, line: &TextLine, x: f64, y: f64, family: &[String]) {
        if line.runs.is_empty() {
            return;
        }
        let line_left = x + line.bounds.x0;
        let line_top = y + line.bounds.y0;
        let default_color = lc(line.runs[0].color);
        let default_fs = line.runs[0].font_size as f64;
        let family_str = family.join(", ");

        // 行内背景：逐 run 测量宽度并绘制矩形（顺序累积 x 偏移）。
        let mut cursor = 0.0_f64;
        for run in &line.runs {
            let fs = run.font_size as f64;
            if let Some(bg) = run.background_color {
                let w = measure_width(&run.text, &family_str, fs, default_color);
                let pad = fs * 0.1;
                self.rect(
                    line_left + cursor - pad,
                    line_top - pad,
                    w + 2.0 * pad,
                    fs * 1.25,
                    Some(lc(bg)),
                    None,
                );
            }
            cursor += measure_width(&run.text, &family_str, fs, default_color);
        }

        // 富文本片段。
        let mut spans = Vec::with_capacity(line.runs.len());
        for run in &line.runs {
            let fs = run.font_size as f64;
            let style = text_style(&family_str, fs, run.color, run.font_weight_bold, run.font_style_italic, run.decoration, TextBaseline::Alphabetic, run.baseline_shift as f64, default_color);
            spans.push(RichSpan::new(run.text.clone(), style));
        }

        let style = text_style(&family_str, default_fs, line.runs[0].color, false, false, crate::ast::TextDecoration::None, TextBaseline::Top, 0.0, default_color);

        self.push(
            Element::Text {
                spans,
                position: Point::new(self.s(line_left), self.s(line_top)),
                style,
                layout: None,
            },
            1,
        );
    }

    /// 绘制一个块（递归）。`x`/`y` 为该块在文档内容区的左上角（pt）。
    fn draw_block(&mut self, block: &Block, settings: &PageSettings, x: f64, y: f64, content_w: f64) {
        let style = &block.style;
        let bg = style.background_color;

        match &block.kind {
            BlockKind::Heading { children, .. } => {
                if let Some(bg) = bg {
                    self.rect(x, y, content_w, block_height(block, settings, x), Some(lc(bg)), None);
                }
                for c in children {
                    self.draw_block(c, settings, x, y, content_w);
                }
            }
            BlockKind::Paragraph { lines } => {
                if let Some(bg) = bg {
                    self.rect(x, y, content_w, block_height(block, settings, x), Some(lc(bg)), None);
                }
                let family = &style.font_family;
                let mut ly = y;
                for line in lines {
                    self.text_line(line, x, ly, family);
                    ly += style.line_height_pt as f64;
                }
            }
            BlockKind::CodeBlock { lines, .. } => {
                let h = block_height(block, settings, x);
                let code_bg = style.background_color.unwrap_or_else(|| PColor::new(245, 245, 245));
                self.rect(x, y, content_w, h, Some(lc(code_bg)), None);
                let family = &style.font_family;
                let mut ly = y + CODE_PADDING;
                for line in lines {
                    self.text_line(line, x + CODE_PADDING, ly, family);
                    ly += style.line_height_pt as f64;
                }
            }
            BlockKind::ThematicBreak => {
                let yy = y + 2.0;
                self.line(x, yy, x + content_w, yy, lc(style.color), 1.0);
            }
            BlockKind::Image(img) => {
                let (w, h) = img.size;
                let ix = x + img.position.0;
                let iy = y + img.position.1;
                let frame = LRect::new(self.s(ix), self.s(iy), self.s(ix + w), self.s(iy + h));
                if img.data.is_empty() {
                    self.rect(ix, iy, w, h, Some(LColor::rgba(230, 230, 230, 255)), Some(self.border_stroke(style)));
                } else {
                    let fit = match img.object_fit {
                        LayoutObjectFit::Contain => lievisual::ObjectFit::Contain,
                        LayoutObjectFit::Cover => lievisual::ObjectFit::Cover,
                        LayoutObjectFit::Fill => lievisual::ObjectFit::Fill,
                        LayoutObjectFit::None => lievisual::ObjectFit::None,
                    };
                    let scene_img = SceneImage {
                        data: img.data.clone(),
                        format: img.format.clone(),
                        width: img.pixel_size.0,
                        height: img.pixel_size.1,
                        object_fit: fit,
                    };
                    self.push(Element::Image { image: scene_img, frame, opacity: 1.0 }, 1);
                }
            }
            BlockKind::Blockquote { children } => {
                let bq_h = block_height(block, settings, x);
                self.rect(x, y, BQ_BAR_WIDTH, bq_h, Some(lc(PColor::new(200, 200, 200))), None);
                let inner_x = x + BQ_BAR_WIDTH + BQ_PAD_X;
                let inner_w = content_w - BQ_BAR_WIDTH - BQ_PAD_X;
                let mut iy = y + BQ_PAD_Y;
                for c in children {
                    self.draw_block(c, settings, inner_x, iy, inner_w);
                    iy += block_height(c, settings, inner_x)
                        - c.style.margin_top as f64
                        - c.style.margin_bottom as f64;
                }
            }
            BlockKind::List { children, .. } | BlockKind::Document { children } => {
                let mut iy = y;
                for c in children {
                    self.draw_block(c, settings, x, iy, content_w);
                    iy += block_height(c, settings, x);
                }
            }
            BlockKind::ListItem { marker, children } => {
                let indent = list_item_indent(marker, style);
                let inner_x = x + indent;
                self.draw_list_marker(marker, x, y, style);
                let mut iy = y;
                for c in children {
                    self.draw_block(c, settings, inner_x, iy, content_w - indent);
                    iy += block_height(c, settings, inner_x);
                }
            }
            BlockKind::TaskListItem { marker, checked, children } => {
                let indent = list_item_indent(marker, style);
                let inner_x = x + indent;
                self.draw_task_marker(x, y, *checked, style);
                let mut iy = y;
                for c in children {
                    self.draw_block(c, settings, inner_x, iy, content_w - indent);
                    iy += block_height(c, settings, inner_x);
                }
            }
            BlockKind::Container { children, .. } => {
                if let Some(bg) = bg {
                    self.rect(x, y, content_w, block_height(block, settings, x), Some(lc(bg)), None);
                }
                let mut iy = y;
                for c in children {
                    self.draw_block(c, settings, x, iy, content_w);
                    iy += block_height(c, settings, x);
                }
            }
            BlockKind::Table { rows, col_widths, row_heights, .. } => {
                self.draw_table(block, rows, col_widths, row_heights, settings, x, y);
            }
            BlockKind::TableRow { .. } | BlockKind::TableCell { .. } => {
                // 表格内部由 `draw_table` 处理，顶层不应出现。
            }
            BlockKind::DefinitionList { items } => {
                let mut iy = y;
                for item in items {
                    for c in &item.term {
                        self.draw_block(c, settings, x, iy, content_w);
                        iy += block_height(c, settings, x);
                    }
                    for c in &item.definition {
                        self.draw_block(c, settings, x + 20.0, iy, content_w - 20.0);
                        iy += block_height(c, settings, x + 20.0);
                    }
                }
            }
            BlockKind::FootnoteDef { children, .. } => {
                let mut iy = y;
                for c in children {
                    self.draw_block(c, settings, x, iy, content_w);
                    iy += block_height(c, settings, x);
                }
            }
            BlockKind::Text { .. }
            | BlockKind::InlineCode { .. }
            | BlockKind::Link { .. }
            | BlockKind::LineBreak => {
                // 纯文本叶节点不在顶层独立绘制（已在 Paragraph / 容器文本行中呈现）。
            }
        }
    }

    /// 列表项普通 marker（无序圆点 / 有序数字）。
    fn draw_list_marker(&mut self, marker: &str, x: f64, y: f64, style: &ResolvedStyle) {
        let fs = style.font_size_pt as f64;
        let marker_left = x + 2.0;
        let text_center_y = y + fs * 0.75;

        if marker.trim() == "•" {
            self.circle(marker_left + fs * 0.35, text_center_y, fs * 0.18, Some(lc(style.color)), None);
        } else {
            let line = make_text_line(marker, style);
            self.text_line(&line, marker_left, y, &style.font_family);
        }
    }

    /// 任务列表复选框 marker。
    fn draw_task_marker(&mut self, x: f64, y: f64, checked: bool, style: &ResolvedStyle) {
        let fs = style.font_size_pt as f64;
        let box_x = x + 2.0;
        let box_size = fs * 1.1;
        let box_y = y + fs * 0.2;
        let stroke = self.border_stroke(style);
        self.rect(box_x, box_y, box_size, box_size, None, Some(stroke));
        if checked {
            let r = box_size;
            let cx = box_x;
            let cy = box_y;
            let x1 = cx + r * 0.2;
            let y1 = cy + r * 0.55;
            let x2 = cx + r * 0.42;
            let y2 = cy + r * 0.78;
            let x3 = cx + r * 0.8;
            let y3 = cy + r * 0.32;
            let check_color = LColor::rgba(26, 128, 230, 255);
            self.line(x1, y1, x2, y2, check_color, 1.5);
            self.line(x2, y2, x3, y3, check_color, 1.5);
        }
    }

    /// 表格：背景、单元格文本、边框。
    fn draw_table(
        &mut self,
        block: &Block,
        rows: &[TableRow],
        col_widths: &[f64],
        row_heights: &[f64],
        settings: &PageSettings,
        x: f64,
        y: f64,
    ) {
        let style = &block.style;
        if rows.is_empty() || col_widths.is_empty() {
            return;
        }
        let row_h_at = |i: usize| -> f64 { table_row_height(style, row_heights, i) };

        // 单元格背景：表头底纹 + 隔行底纹。
        let mut ry = y;
        for (ri, row) in rows.iter().enumerate() {
            let rh = row_h_at(ri);
            let mut cx = x;
            for (_ci, cell) in row.cells.iter().enumerate() {
                let cw = *col_widths.get(_ci).unwrap_or(&0.0);
                let bg = if ri == 0 {
                    Some(LColor::rgba(237, 237, 242, 255))
                } else if ri % 2 == 0 {
                    Some(LColor::rgba(247, 247, 247, 255))
                } else {
                    None
                };
                if let Some(bg) = bg {
                    self.rect(cx, ry, cw, rh, Some(bg), None);
                }
                // 单元格内容（带内边距）。
                let pad = 4.0;
                let mut cy = ry + pad;
                for child in &cell.children {
                    self.draw_block(child, settings, cx + pad, cy, cw - 2.0 * pad);
                    cy += block_height(child, settings, cx + pad);
                }
                cx += cw;
            }
            ry += rh;
        }

        // 边框。
        for seg in table_border_segments(rows, col_widths, row_heights, style, x, y) {
            self.line(seg.x1, seg.y1, seg.x2, seg.y2, lc(seg.color), seg.width);
        }
    }

    /// 由 ResolvedStyle 构造描边（用于边框 / 复选框）。
    fn border_stroke(&self, style: &ResolvedStyle) -> Stroke {
        Stroke {
            color: lc(style.color),
            width: self.s(1.0),
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            dash_array: Vec::new(),
            dash_offset: 0.0,
            miter_limit: 4.0,
        }
    }
}

/// 构造一个 `lievisual::TextStyle`。
fn text_style(
    family: &str,
    font_size: f64,
    color: PColor,
    bold: bool,
    italic: bool,
    decoration: crate::ast::TextDecoration,
    baseline: TextBaseline,
    baseline_shift: f64,
    _default_color: LColor,
) -> LTextStyle {
    let (underline, strikethrough) = match decoration {
        crate::ast::TextDecoration::None => (false, false),
        crate::ast::TextDecoration::Underline => (true, false),
        crate::ast::TextDecoration::LineThrough => (false, true),
    };
    LTextStyle {
        color: lc(color),
        font_family: family.to_string(),
        font_size,
        font_weight: if bold { 700.0 } else { 400.0 },
        font_style: if italic { FontStyle::Italic } else { FontStyle::Normal },
        font_width: None,
        line_height: None,
        letter_spacing: 0.0,
        underline,
        underline_color: None,
        strikethrough,
        strikethrough_color: None,
        baseline_shift,
        background_color: None,
        rotation: 0.0,
        max_width: None,
        align: TextAlign::Left,
        baseline,
    }
}

/// 估算单行文本宽度（pt）。
fn measure_width(text: &str, family: &str, font_size: f64, color: LColor) -> f64 {
    let style = LTextStyle {
        color,
        font_family: family.to_string(),
        font_size,
        font_weight: 400.0,
        font_style: FontStyle::Normal,
        font_width: None,
        line_height: None,
        letter_spacing: 0.0,
        underline: false,
        underline_color: None,
        strikethrough: false,
        strikethrough_color: None,
        baseline_shift: 0.0,
        background_color: None,
        rotation: 0.0,
        max_width: None,
        align: TextAlign::Left,
        baseline: TextBaseline::Alphabetic,
    };
    let spans = vec![RichSpan::new(text.to_string(), style)];
    lievisual::measure_text(&spans, None).size.width
}

/// 构造单行文本（用于列表有序 marker 等纯文本标记）。
fn make_text_line(text: &str, style: &ResolvedStyle) -> TextLine {
    use crate::document::text::TextRun;
    let run = TextRun {
        text: text.to_string(),
        font_data: std::sync::Arc::new(Vec::new()),
        font_size: style.font_size_pt,
        text_offset: 0,
        font_weight_bold: style.font_weight_bold,
        font_style_italic: style.font_style_italic,
        color: style.color,
        advance: 0.0,
        glyphs: vec![Glyph {
            id: 0,
            x: 0.0,
            y: 0.0,
            advance: style.font_size_pt,
            cluster: 0,
        }],
        is_rtl: false,
        baseline_x: 0.0,
        baseline_y: style.font_size_pt as f32 * 0.8,
        url: None,
        decoration: style.text_decoration,
        baseline_shift: 0.0,
        background_color: None,
    };
    TextLine {
        runs: vec![run],
        bounds: vello_cpu::kurbo::Rect::new(0.0, 0.0, 0.0, style.font_size_pt as f64),
        line_height: style.font_size_pt as f32,
    }
}
