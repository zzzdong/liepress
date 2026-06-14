//! 文档生成器模块
//!
//! 将带样式的 AST（Layer 2）转换为布局后的 Document（Layer 3）。
//! 负责分页、文本排版、图片处理等。

mod block;
pub mod box_model;
pub mod constants;
pub mod context;
pub mod header_footer;
pub mod image;
mod table;
pub mod text;
pub mod types;

pub use constants::*;
pub use context::*;
pub use types::*;

use crate::ast::{Node, NodeKind};
use crate::visual::{FillStrokeStyle, VisualElement};
use vello_cpu::kurbo::{Point, Rect};

/// 文档生成器
///
/// 核心布局引擎，将 Styled Node 树转换为 `DocumentLayout`。
///
/// ## 架构说明
///
/// - **布局方法**：通过与模块同文件中的 `impl DocumentGenerator` 扩展（`block.rs`、`table.rs`）
/// - **Box Model**：独立在 `box_model` 模块中，以函数方式操作 `PageContext`
/// - **图片加载**：独立在 `image` 模块中
/// - **页眉页脚**：独立在 `header_footer` 模块中，在 `finish()` 时注入
pub struct DocumentGenerator {
    pub page_context: PageContext,
    pub layout_ctx: LayoutContext,
}

impl Default for DocumentGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentGenerator {
    pub fn new() -> Self {
        Self::with_settings(PageSettings::default())
    }

    pub fn with_settings(settings: PageSettings) -> Self {
        Self {
            page_context: PageContext::new(settings),
            layout_ctx: LayoutContext::new(),
        }
    }

    pub fn finish(self) -> DocumentLayout {
        let outline = self.layout_ctx.outline().to_vec();
        let settings = self.page_context.settings.clone();
        let has_header_footer = settings.header.is_some() || settings.footer.is_some();
        let mut layout = self.page_context.finish();
        layout.outline = outline;
        if has_header_footer {
            header_footer::inject_header_footer(&mut layout, &settings);
        }
        layout
    }

    // ─── 公开布局入口 ───────────────────────────────────────

    pub fn layout_node(&mut self, node: &Node) {
        let style = &node.style;
        match &node.kind {
            NodeKind::Paragraph { children } => {
                self.layout_paragraph(children, style);
            }
            NodeKind::Heading { level, children } => {
                self.layout_heading(*level, children, style);
            }
            NodeKind::CodeBlock { code, .. } => {
                self.layout_code(code, style);
            }
            NodeKind::Image { src, alt, title: _ } => {
                self.layout_image(src, alt, style);
            }
            NodeKind::List {
                ordered,
                children,
                start,
            } => {
                self.layout_list(children, *ordered, start.unwrap_or(1), style);
            }
            NodeKind::Blockquote { children } => {
                self.layout_blockquote(children, style);
            }
            NodeKind::ThematicBreak => {
                self.layout_thematic_break(style);
            }
            NodeKind::Table { .. } => {
                self.layout_table(node);
            }
            NodeKind::Center { children } => {
                self.layout_container(children, style, true);
            }
            NodeKind::Container { children } => {
                self.layout_container(children, style, false);
            }
            NodeKind::Span { children } => {
                // Span 作为顶级块级子节点时，作为段落处理
                self.layout_paragraph(children, style);
            }
            _ => {}
        }
    }

    // ─── 缩进上下文辅助方法 ────────────────────────────────

    /// 在指定缩进上下文中执行操作，执行完毕后恢复之前的缩进。
    fn with_indent<R>(&mut self, indent: f32, f: impl FnOnce(&mut Self) -> R) -> R {
        let prev = self.layout_ctx.current_indent;
        self.layout_ctx.current_indent = indent;
        let result = f(self);
        self.layout_ctx.current_indent = prev;
        result
    }

    /// 在当前缩进基础上增加偏移量，执行操作后恢复。
    fn with_additional_indent<R>(&mut self, additional: f32, f: impl FnOnce(&mut Self) -> R) -> R {
        let prev = self.layout_ctx.current_indent;
        self.layout_ctx.current_indent += additional;
        let result = f(self);
        self.layout_ctx.current_indent = prev;
        result
    }

    // ─── 通用文本行放置 ─────────────────────────────────────

    /// 放置文本行到页面上，处理自动分页。
    ///
    /// 水平偏移量从 `layout_ctx.current_indent` 获取。
    /// 此方法只消耗文本行的垂直高度，不处理 margin/padding。
    /// 调用方需在之后调用 `end_box` 或 `end_box_content`。
    ///
    /// # 参数
    /// - `splittable`: 是否允许跨页分割
    /// - `bg`: 可选背景样式（分页时自动绘制背景覆盖 padding 区域）
    /// - `first_line_indent`: 首行缩进量（仅第一行生效，不因分页重置）
    fn place_text_lines(
        &mut self,
        lines: Vec<crate::text::TextLine>,
        splittable: bool,
        bg: Option<&box_model::BgStyle>,
        first_line_indent: f32,
    ) {
        let base_x_offset = self.layout_ctx.current_indent;
        let settings = self.page_context.settings.clone();
        let content_height = settings.content_height();
        let content_x = settings.content_x();
        let content_y = settings.content_y();
        let mut current_page_y = self.page_context.current_y;
        let mut page_start_y = current_page_y;
        let mut is_first_line = true;

        for line in lines {
            let line_bottom_rel = current_page_y + line.line_height;

            if splittable && line_bottom_rel > content_height && !self.page_context.is_empty() {
                // 分页前，为本页绘制背景
                if let Some(bg) = bg {
                    let (bg_left, bg_right) =
                        self.bg_horizontal_bounds(base_x_offset, bg, &settings);
                    let bg_top = content_y + page_start_y - bg.padding.top;
                    let bg_bottom = content_y + current_page_y + bg.padding.bottom;
                    self.page_context
                        .add_element_before_text(VisualElement::Rect {
                            rect: Rect::new(
                                bg_left as f64,
                                bg_top as f64,
                                bg_right as f64,
                                bg_bottom as f64,
                            ),
                            style: FillStrokeStyle {
                                fill: Some(bg.color),
                                stroke: None,
                            },
                        });
                }
                self.page_context.finalize_current_page();
                self.page_context.start_new_page();
                current_page_y = 0.0;
                page_start_y = 0.0;
            }

            let x_offset = if is_first_line {
                is_first_line = false;
                base_x_offset + first_line_indent
            } else {
                base_x_offset
            };

            let line_abs_left = content_x + x_offset + line.bounds.x0 as f32;
            let line_abs_top = content_y + current_page_y;
            let line_width = line.bounds.width() as f32;

            let bounds = Rect::new(
                line_abs_left as f64,
                line_abs_top as f64,
                (line_abs_left + line_width) as f64,
                (line_abs_top + line.line_height) as f64,
            );

            self.page_context.add_element(VisualElement::TextLine {
                runs: line.runs,
                bounds,
                line_height: line.line_height,
            });

            current_page_y += line.line_height;
        }

        // 最后一页的背景（在文本行之后添加，确保背景在文本下方）
        if let Some(bg) = bg {
            let (bg_left, bg_right) = self.bg_horizontal_bounds(base_x_offset, bg, &settings);
            let bg_top = content_y + page_start_y - bg.padding.top;
            let bg_bottom = content_y + current_page_y + bg.padding.bottom;
            self.page_context
                .add_element_before_text(VisualElement::Rect {
                    rect: Rect::new(
                        bg_left as f64,
                        bg_top as f64,
                        bg_right as f64,
                        bg_bottom as f64,
                    ),
                    style: FillStrokeStyle {
                        fill: Some(bg.color),
                        stroke: None,
                    },
                });
        }

        // 只消耗文本行高度，margin/padding 由调用方通过 end_box/end_box_content 处理
        let consumed_height = current_page_y - page_start_y;
        self.page_context.consume_height(consumed_height);
    }

    /// 计算背景矩形的水平边界
    fn bg_horizontal_bounds(
        &self,
        x_offset: f32,
        bg: &box_model::BgStyle,
        settings: &PageSettings,
    ) -> (f32, f32) {
        let content_x = settings.content_x();
        let content_width = settings.content_width();
        let bg_left = content_x + x_offset - bg.padding.left;
        let bg_right = content_x + content_width - bg.margin.right;
        (bg_left, bg_right)
    }
}

// ─── 辅助函数 ───────────────────────────────────────────────────

/// 递归平移 VisualElement 中的所有坐标
fn shift_element(element: VisualElement, dx: f64, dy: f64) -> VisualElement {
    match element {
        VisualElement::Rect { rect, style } => VisualElement::Rect {
            rect: Rect::new(rect.x0 + dx, rect.y0 + dy, rect.x1 + dx, rect.y1 + dy),
            style,
        },
        VisualElement::RoundedRect { rect, radii, style } => VisualElement::RoundedRect {
            rect: Rect::new(rect.x0 + dx, rect.y0 + dy, rect.x1 + dx, rect.y1 + dy),
            radii,
            style,
        },
        VisualElement::Circle {
            center,
            radius,
            style,
        } => VisualElement::Circle {
            center: Point::new(center.x + dx, center.y + dy),
            radius,
            style,
        },
        VisualElement::Line { start, end, style } => VisualElement::Line {
            start: Point::new(start.x + dx, start.y + dy),
            end: Point::new(end.x + dx, end.y + dy),
            style,
        },
        VisualElement::Polyline { points, style } => VisualElement::Polyline {
            points: points
                .into_iter()
                .map(|p| Point::new(p.x + dx, p.y + dy))
                .collect(),
            style,
        },
        VisualElement::Path { path, style } => VisualElement::Path { path, style },
        VisualElement::GradientPath {
            path,
            gradient,
            stroke,
        } => VisualElement::GradientPath {
            path,
            gradient,
            stroke,
        },
        VisualElement::TextLine {
            runs,
            bounds,
            line_height,
        } => VisualElement::TextLine {
            runs,
            bounds: Rect::new(
                bounds.x0 + dx,
                bounds.y0 + dy,
                bounds.x1 + dx,
                bounds.y1 + dy,
            ),
            line_height,
        },
        VisualElement::Image {
            position,
            size,
            pixel_size,
            data,
            format,
            alt,
        } => VisualElement::Image {
            position: Point::new(position.x + dx, position.y + dy),
            size,
            pixel_size,
            data,
            format,
            alt,
        },
        VisualElement::Group {
            children,
            transform,
        } => VisualElement::Group {
            children: children
                .into_iter()
                .map(|c| shift_element(c, dx, dy))
                .collect(),
            transform,
        },
        VisualElement::ZGroup { z_index, children } => VisualElement::ZGroup {
            z_index,
            children: children
                .into_iter()
                .map(|c| shift_element(c, dx, dy))
                .collect(),
        },
    }
}

// ─── 旧管线入口 ───────────────────────────────────────────────

/// 使用新管线将 Markdown 转换为 Document
///
/// 管线：Markdown → HTML → HtmlDocument → Styled Node → Document
pub fn markdown_to_document(markdown: &str) -> Document {
    let html_str = crate::html::markdown_to_html(markdown);
    let user_css = crate::ast::presets::DEFAULT_CSS.to_string();
    crate::html_to_document(&html_str, &user_css, false, None).unwrap_or_else(|_| Document {
        pages: vec![],
        page_width: 0.0,
        page_height: 0.0,
        outline: vec![],
    })
}
