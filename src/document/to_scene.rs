//! 将 liepress 的 `Document`（已排版盒子树）转换为 lievisual 的 `Scene`（图元 IR）。
//!
//! 这是 PNG / SVG 输出后端共用的统一转换层：两个后端都先调用 [`document_to_scene`]
//! 构造 `lievisual::Scene`，再分别委托给 lievisual 的 `VelloPixmapRenderer` / `SvgRenderer`，
//!
//! 坐标约定：转换全程在 pt 坐标系下进行，最后统一乘 `scale = dpi / 72` 归一到 px；
//! 这样 PNG 后端按 `scale` 设置画布像素尺寸，SVG 后端通过 `Scene.scale` 让 `viewBox`
//! 回到 pt 单位，二者共用同一份场景数据。

use lievisual::geometry::Color as LColor;
use lievisual::text::{
    FontStyle, FontWeight, FontWidth, TextAlign, TextBaseline, TextStyle as LTextStyle,
};
use lievisual::{
    Element, Fill, FillStrokeStyle, LineCap, LineJoin, Point, Rect as LRect, RichSpan, Scene,
    SceneImage, SceneNode, Stroke,
};

use crate::document::layout::{Block, BlockKind, Document, TableRow};
use crate::document::text::TextLine;
use crate::document::types::ObjectFit as LayoutObjectFit;
use crate::document::types::ResolvedStyle;
use crate::document::types::page::PageSettings;
use crate::output::common::{
    BQ_BAR_WIDTH, BQ_PAD_X, BQ_PAD_Y, block_height, list_item_indent, table_border_segments,
    table_row_height,
};

/// 代码块内边距（pt）。
const CODE_PADDING: f64 = 4.0;
/// 默认 DPI（与旧 PNG 后端保持一致）。
pub const DEFAULT_DPI: f64 = 144.0;

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
    fn rect(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        fill: Option<LColor>,
        stroke: Option<Stroke>,
    ) {
        let fill = fill.unwrap_or_else(|| LColor::rgba(0, 0, 0, 0));
        let style = FillStrokeStyle {
            fill: Some(Fill::Solid(fill)),
            stroke,
        };
        self.push(
            Element::Rect {
                rect: LRect::new(self.s(x), self.s(y), self.s(x + w), self.s(y + h)),
                style,
            },
            0,
        );
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
        self.push(
            Element::Line {
                start: Point::new(self.s(x1), self.s(y1)),
                end: Point::new(self.s(x2), self.s(y2)),
                style: stroke,
            },
            0,
        );
    }

    /// 实心/描边圆。
    fn circle(&mut self, cx: f64, cy: f64, r: f64, fill: Option<LColor>, stroke: Option<Stroke>) {
        let fill = fill.unwrap_or_else(|| LColor::rgba(0, 0, 0, 0));
        let style = FillStrokeStyle {
            fill: Some(Fill::Solid(fill)),
            stroke,
        };
        self.push(
            Element::Circle {
                center: Point::new(self.s(cx), self.s(cy)),
                radius: self.s(r),
                style,
            },
            0,
        );
    }

    /// 单行文本：先绘制行内背景（如有），再绘制 `Element::Text`。
    ///
    /// 坐标：`position = (x + bounds.x0, y + bounds.y0)`，即行左上角（lievisual 的
    /// `baseline = Top` 语义）。`family` 为块级字体族（run 不携带族名）。
    /// `line_height` 为该行所属块的行高（pt，绝对值）；与 `draw_block` 的垂直步进
    /// 同源，保证字形排版与行距一致。
    fn text_line(
        &mut self,
        line: &TextLine,
        x: f64,
        y: f64,
        family: &[String],
        line_height: Option<f64>,
    ) {
        if line.runs.is_empty() {
            return;
        }
        let line_left = x + line.bounds.x0;
        let line_top = y + line.bounds.y0;
        let default_color = line.runs[0].color;
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
                    Some(bg),
                    None,
                );
            }
            cursor += measure_width(&run.text, &family_str, fs, default_color);
        }

        // 富文本片段。
        let mut spans = Vec::with_capacity(line.runs.len());
        for run in &line.runs {
            let fs = run.font_size as f64;
            let style = text_style(
                &family_str,
                fs,
                run.color,
                if run.font_weight_bold {
                    FontWeight::Bold
                } else {
                    FontWeight::Normal
                },
                run.font_style_italic,
                run.decoration,
                TextBaseline::Alphabetic,
                run.baseline_shift as f64,
                default_color,
                line_height,
            );
            spans.push(RichSpan::new(run.text.clone(), style));
        }

        let style = text_style(
            &family_str,
            default_fs,
            line.runs[0].color,
            FontWeight::Normal,
            false,
            lievisual::text::TextDecoration::None,
            TextBaseline::Top,
            0.0,
            default_color,
            line_height,
        );

        // 使用 lievisual 的文本排版功能：预先用 `layout_text` 排好版，作为预排版
        // `layout` 喂给 `Element::Text`，栅格后端按字形级布局精确绘制字形
        // （而非渲染时二次排版），保证与测量 / 尺寸计算一致。
        let prelayout = lievisual::text::layout_text(&spans, None);
        self.push(
            Element::Text {
                spans,
                position: Point::new(self.s(line_left), self.s(line_top)),
                style,
                layout: Some(prelayout),
            },
            1,
        );
    }

    /// 直接用 lievisual 排版并绘制一段纯文本（不经由外部预排版的 `TextLine`）。
    ///
    /// 适用于列表 marker、序号等简单单行文本：直接把 `(文本, 样式)` 交给
    /// `lievisual::text::layout_text` 排版，由其负责字形测量与 `TextLine` 构造，
    /// 得到的结果直接作为 `Element::Text` 的预排版 push 到场景。与 [`Self::text_line`]
    /// 不同，本方法不接收外部 `TextLine`，避免「先排版一次 `TextLine` 再二次重排」。
    fn text(&mut self, text: &str, style: &ResolvedStyle, x: f64, y: f64) {
        let fs = style.font_size_pt as f64;
        let family_str = style.font_family.join(", ");
        let lv_style = text_style(
            &family_str,
            fs,
            style.color,
            if style.font_weight_bold {
                FontWeight::Bold
            } else {
                FontWeight::Normal
            },
            style.font_style_italic,
            lievisual::text::TextDecoration::None,
            TextBaseline::Top,
            0.0,
            style.color,
            resolved_line_height(style),
        );
        let span = RichSpan::new(text.to_string(), lv_style.clone());
        let prelayout = lievisual::text::layout_text(std::slice::from_ref(&span), None);
        let bounds = prelayout.ink_bounds;
        self.push(
            Element::Text {
                spans: vec![span],
                position: Point::new(self.s(x + bounds.x0), self.s(y + bounds.y0)),
                style: lv_style,
                layout: Some(prelayout),
            },
            1,
        );
    }

    /// 绘制一个块（递归）。`x`/`y` 为该块在文档内容区的左上角（pt）。
    fn draw_block(
        &mut self,
        block: &Block,
        settings: &PageSettings,
        x: f64,
        y: f64,
        content_w: f64,
    ) {
        let style = &block.style;
        let bg = style.background_color;

        match &block.kind {
            BlockKind::Heading { children, .. } => {
                if let Some(bg) = bg {
                    self.rect(
                        x,
                        y,
                        content_w,
                        block_height(block, settings, x),
                        Some(bg),
                        None,
                    );
                }
                for c in children {
                    self.draw_block(c, settings, x, y, content_w);
                }
            }
            BlockKind::Paragraph { lines } => {
                if let Some(bg) = bg {
                    self.rect(
                        x,
                        y,
                        content_w,
                        block_height(block, settings, x),
                        Some(bg),
                        None,
                    );
                }
                let family = &style.font_family;
                let mut ly = y;
                for line in lines {
                    self.text_line(line, x, ly, family, resolved_line_height(style));
                    ly += style.line_height_pt as f64;
                }
            }
            BlockKind::CodeBlock { lines, .. } => {
                let h = block_height(block, settings, x);
                let code_bg = style
                    .background_color
                    .unwrap_or_else(|| lievisual::Color::rgb(245, 245, 245));
                self.rect(x, y, content_w, h, Some(code_bg), None);
                let family = &style.font_family;
                let mut ly = y + CODE_PADDING;
                for line in lines {
                    self.text_line(
                        line,
                        x + CODE_PADDING,
                        ly,
                        family,
                        resolved_line_height(style),
                    );
                    ly += style.line_height_pt as f64;
                }
            }
            BlockKind::ThematicBreak => {
                // 与 pdf 后端一致：横线颜色取 border_color（无声明兜底为灰）。
                let yy = y + 2.0;
                let hr_color = style
                    .border_color
                    .unwrap_or(lievisual::Color::rgb(180, 180, 180));
                self.line(x, yy, x + content_w, yy, hr_color, 1.0);
            }
            BlockKind::Image(img) => {
                let (w, h) = img.size;
                let ix = x + img.position.0;
                let iy = y + img.position.1;
                let frame = LRect::new(self.s(ix), self.s(iy), self.s(ix + w), self.s(iy + h));
                if img.data.is_empty() {
                    self.rect(
                        ix,
                        iy,
                        w,
                        h,
                        Some(LColor::rgba(230, 230, 230, 255)),
                        Some(self.border_stroke(style)),
                    );
                } else {
                    let fit = match img.object_fit {
                        LayoutObjectFit::Contain => lievisual::ObjectFit::Contain,
                        LayoutObjectFit::Cover => lievisual::ObjectFit::Cover,
                        LayoutObjectFit::Fill => lievisual::ObjectFit::Fill,
                        LayoutObjectFit::None => lievisual::ObjectFit::None,
                    };
                    // lievisual 的 SceneImage 持有已解码的 RGBA8 位图（Pixmap），
                    // 解码是调用方职责：这里用 `image` 解码原始字节为 RGBA8 再构造。
                    if let Some(scene_img) = decode_scene_image(&img.data, fit) {
                        self.push(
                            Element::Image {
                                image: scene_img,
                                frame,
                                opacity: 1.0,
                            },
                            1,
                        );
                    } else {
                        self.rect(
                            ix,
                            iy,
                            w,
                            h,
                            Some(LColor::rgba(230, 230, 230, 255)),
                            Some(self.border_stroke(style)),
                        );
                    }
                }
            }
            BlockKind::Blockquote { children } => {
                let bq_h = block_height(block, settings, x);
                self.rect(
                    x,
                    y,
                    BQ_BAR_WIDTH,
                    bq_h,
                    Some(lievisual::Color::rgb(200, 200, 200)),
                    None,
                );
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
            BlockKind::TaskListItem {
                marker,
                checked,
                children,
            } => {
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
                    self.rect(
                        x,
                        y,
                        content_w,
                        block_height(block, settings, x),
                        Some(bg),
                        None,
                    );
                }
                let mut iy = y;
                for c in children {
                    self.draw_block(c, settings, x, iy, content_w);
                    iy += block_height(c, settings, x);
                }
            }
            BlockKind::Table {
                rows,
                col_widths,
                row_heights,
                ..
            } => {
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
            self.circle(
                marker_left + fs * 0.35,
                text_center_y,
                fs * 0.18,
                Some(style.color),
                None,
            );
        } else {
            self.text(marker, style, marker_left, y);
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
    #[allow(clippy::too_many_arguments)]
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
            self.line(seg.x1, seg.y1, seg.x2, seg.y2, seg.color, seg.width);
        }
    }

    /// 由 ResolvedStyle 构造描边（用于边框 / 复选框）。
    fn border_stroke(&self, style: &ResolvedStyle) -> Stroke {
        Stroke {
            color: style.color,
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
///
/// `line_height` 为绝对行高（pt）；`None` 表示未声明（字体默认行高）。
/// 必须与 `draw_block` 的垂直步进（`style.line_height_pt`）同源，否则
/// CSS 行高只改变行距、字形盒却保持字体默认，视觉上「行距松了字还挤在一起」。
#[allow(clippy::too_many_arguments)]
fn text_style(
    family: &str,
    font_size: f64,
    color: LColor,
    weight: FontWeight,
    italic: bool,
    decoration: lievisual::text::TextDecoration,
    baseline: TextBaseline,
    baseline_shift: f64,
    _default_color: LColor,
    line_height: Option<f64>,
) -> LTextStyle {
    let (underline, strikethrough) = match decoration {
        lievisual::text::TextDecoration::None => (false, false),
        lievisual::text::TextDecoration::Underline => (true, false),
        lievisual::text::TextDecoration::LineThrough => (false, true),
    };
    LTextStyle {
        color,
        font_family: family.to_string(),
        font_size,
        font_weight: weight,
        font_style: if italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        },
        font_width: FontWidth::Normal,
        line_height,
        letter_spacing: 0.0,
        underline,
        underline_color: None,
        strikethrough,
        strikethrough_color: None,
        baseline_shift,
        background_color: None,
        url: None,
        rotation: 0.0,
        max_width: None,
        align: TextAlign::Left,
        baseline,
    }
}

/// 从投影样式的 `line_height_pt` 取排版行高（0 = 未声明 → 字体默认）。
fn resolved_line_height(style: &ResolvedStyle) -> Option<f64> {
    (style.line_height_pt > 0.0).then_some(style.line_height_pt as f64)
}

/// 估算单行文本宽度（pt）。
fn measure_width(text: &str, family: &str, font_size: f64, color: LColor) -> f64 {
    let style = LTextStyle {
        color,
        font_family: family.to_string(),
        font_size,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        font_width: FontWidth::Normal,
        line_height: None,
        letter_spacing: 0.0,
        underline: false,
        underline_color: None,
        strikethrough: false,
        strikethrough_color: None,
        baseline_shift: 0.0,
        background_color: None,
        url: None,
        rotation: 0.0,
        max_width: None,
        align: TextAlign::Left,
        baseline: TextBaseline::Alphabetic,
    };
    let spans = vec![RichSpan::new(text.to_string(), style)];
    lievisual::measure_text(&spans, None).size.width
}

/// 解码图片原始字节为 lievisual 的 `SceneImage`（RGBA8 位图 + 适应方式）。
///
/// lievisual 不做解码，此处用 `image` crate 解码任意常见格式（png/jpeg/gif/webp…）。
/// 解码失败返回 `None`（调用方回退为占位框）。
fn decode_scene_image(data: &[u8], fit: lievisual::ObjectFit) -> Option<SceneImage> {
    let dyn_img = image::load_from_memory(data).ok()?;
    let rgba = dyn_img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let pixels = rgba.into_raw();
    let pixmap = lievisual::Pixmap::from_rgba8(w, h, pixels)?;
    Some(SceneImage::from_pixmap(pixmap).with_object_fit(fit))
}
