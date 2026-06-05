//! 文档生成器模块
//!
//! 将带样式的 AST（Layer 2）转换为布局后的 Document（Layer 3）。
//! 负责分页、文本排版、图片处理等。

pub mod constants;
pub mod types;
pub mod text;
mod table;

pub use constants::*;
pub use types::*;

use std::path::{Path, PathBuf};
use vello_cpu::kurbo::{Point, Rect};
use crate::ast::{self, Node, NodeKind, Style};
use crate::text::{FONT_CONTEXT, LAYOUT_CONTEXT, TextAlign, TextStyle, layout_text_with_contexts};
use crate::visual::{Color, VisualElement, FillStrokeStyle, StrokeStyle};
use crate::generator::text::{collect_inline_segments, estimate_children_height, annotate_runs_with_urls};

use image::ImageDecoder;

/// 文档生成器
pub struct DocumentGenerator {
    pub page_context: PageContext,
    pub base_dir: Option<PathBuf>,
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
            base_dir: None,
        }
    }

    pub fn with_base_dir(mut self, base_dir: PathBuf) -> Self {
        self.base_dir = Some(base_dir);
        self
    }

    pub fn finish(self) -> Document {
        let settings = self.page_context.settings.clone();
        let has_header_footer = settings.header.is_some()
            || settings.footer.is_some();
        let mut document = self.page_context.finish();
        if has_header_footer {
            inject_header_footer_into_document(&mut document, &settings);
        }
        document
    }

    // ─── 公开布局入口 ───────────────────────────────────────

    pub fn layout_node(&mut self, node: &Node) {
        let style = &node.style;
        match &node.kind {
            NodeKind::Paragraph { children } => {
                self.layout_paragraph_with_indent(children, style, 0.0);
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
            NodeKind::List { ordered, children, start } => {
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
            NodeKind::Span { children } => {
                // Span 作为顶级块级子节点时，作为段落处理
                self.layout_paragraph_with_indent(children, style, 0.0);
            }
            _ => {}
        }
    }

    // ─── 通用文本行放置 ─────────────────────────────────────

    /// 放置文本行到页面上，处理自动分页。
    ///
    /// # 参数
    /// - `x_offset`: 水平偏移量
    /// - `margin_bottom`: 底部边距
    /// - `splittable`: 是否允许跨页分割（段落可分割，标题不可）
    fn place_text_lines(
        &mut self,
        lines: Vec<crate::text::TextLine>,
        x_offset: f32,
        margin_bottom: f32,
        splittable: bool,
    ) {
        let settings = self.page_context.settings.clone();
        let content_height = settings.content_height();
        let content_x = settings.content_x();
        let content_y = settings.content_y();
        let start_y = self.page_context.current_y;
        let mut current_page_y = start_y;
        let mut last_page_start_y = start_y;

        for line in lines {
            let line_bottom_rel = current_page_y + line.line_height;

            if splittable && line_bottom_rel > content_height && !self.page_context.is_empty() {
                self.page_context.finalize_current_page();
                self.page_context.start_new_page();
                current_page_y = 0.0;
                last_page_start_y = 0.0;
            }

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

        let consumed_height = current_page_y - last_page_start_y + margin_bottom;
        self.page_context.consume_height(consumed_height);
    }

    // ─── 标题布局 ─────────────────────────────────────────────

    fn layout_heading(&mut self, _level: u8, children: &[Node], style: &Style) {
        let margin_bottom = style.margin_bottom_pt;
        let segments = collect_inline_segments(children);
        if segments.is_empty() {
            return;
        }

        let align = match style.text_align {
            crate::ast::TextAlign::Left => TextAlign::Left,
            crate::ast::TextAlign::Center => TextAlign::Center,
            crate::ast::TextAlign::Right => TextAlign::Right,
            crate::ast::TextAlign::Justify => TextAlign::Left,
        };

        let total_text: String = segments.iter().map(|(t, _)| t.as_str()).collect();
        let combined: Vec<(&str, &TextStyle)> =
            segments.iter().map(|(t, s)| (t.as_str(), s)).collect();
        let available_width = self.page_context.settings.content_width();

        FONT_CONTEXT.with(|font_cx| {
            LAYOUT_CONTEXT.with(|layout_cx| {
                let mut fcx = font_cx.borrow_mut();
                let mut lcx = layout_cx.borrow_mut();

                let layout = layout_text_with_contexts(
                    &combined,
                    Some(available_width as f64),
                    align,
                    &mut fcx,
                    &mut lcx,
                );

                let mut lines = layout.lines;
                annotate_runs_with_urls(&mut lines, &total_text, &segments);
                let total_line_height: f32 = lines.iter().map(|l| l.line_height).sum();
                let total_height = total_line_height + margin_bottom;

                if total_height > self.page_context.settings.content_height() {
                    panic!(
                        "标题 '{}' 高度 ({:.1}pt) 超过页面内容区高度 ({:.1}pt)，无法渲染",
                        total_text.chars().take(50).collect::<String>(),
                        total_height,
                        self.page_context.settings.content_height()
                    );
                }

                if total_height > self.page_context.remaining_height()
                    && !self.page_context.is_empty()
                {
                    self.page_context.start_new_page();
                }

                self.place_text_lines(lines, 0.0, margin_bottom, false);
            })
        })
    }

    // ─── 代码块布局 ─────────────────────────────────────────────

    fn layout_code(&mut self, code: &str, style: &Style) {
        if code.is_empty() {
            return;
        }

        let content_width = self.page_context.settings.content_width();
        let content_x = self.page_context.settings.content_x();
        let content_y = self.page_context.settings.content_y();
        let margin_bottom = style.margin_bottom_pt;

        let code_style = ast::computed_style_to_text_style(style);

        FONT_CONTEXT.with(|font_cx| {
            LAYOUT_CONTEXT.with(|layout_cx| {
                let mut fcx = font_cx.borrow_mut();
                let mut lcx = layout_cx.borrow_mut();

                let layout = layout_text_with_contexts(
                    &[(code, &code_style)],
                    Some(content_width as f64),
                    TextAlign::Left,
                    &mut fcx,
                    &mut lcx,
                );

                let code_x = 8.0;
                let lines = layout.lines;

                // 使用实际渲染行高计算总高度
                let actual_total_height: f32 = lines.iter().map(|l| l.line_height).sum();
                let total_height = actual_total_height + margin_bottom;

                if total_height > self.page_context.remaining_height()
                    && !self.page_context.is_empty()
                {
                    self.page_context.start_new_page();
                }

                let settings = self.page_context.settings.clone();
                let content_height = settings.content_height();

                // 第一步：将行分组到各页面
                let mut page_groups: Vec<Vec<usize>> = Vec::new();
                let mut current_lines: Vec<usize> = Vec::new();
                let mut cursor_y = self.page_context.current_y;

                for (i, line) in lines.iter().enumerate() {
                    let line_bottom_rel = cursor_y + line.line_height;

                    if line_bottom_rel > content_height && !current_lines.is_empty() {
                        page_groups.push(std::mem::take(&mut current_lines));
                        cursor_y = 0.0;
                    }

                    current_lines.push(i);
                    cursor_y += line.line_height;
                }

                if !current_lines.is_empty() {
                    page_groups.push(current_lines);
                }

                // 第二步：逐页放置背景和文本
                for (group_idx, line_indices) in page_groups.iter().enumerate() {
                    let is_last = group_idx == page_groups.len() - 1;

                    // 计算本页代码组的高度
                    let group_height: f32 =
                        line_indices.iter().map(|&i| lines[i].line_height).sum();

                    let page_current_y = self.page_context.current_y;
                    let bg_top = content_y + page_current_y;
                    let bg_bottom = bg_top + group_height;

                    // 先添加背景矩形
                    self.page_context.add_element(VisualElement::Rect {
                        rect: Rect::new(
                            content_x as f64,
                            bg_top as f64,
                            (content_x + content_width) as f64,
                            bg_bottom as f64,
                        ),
                        style: FillStrokeStyle {
                            fill: Some(Color::new(245, 245, 245)),
                            stroke: None,
                        },
                    });

                    // 再添加文本行
                    let mut line_y = page_current_y;
                    for &line_idx in line_indices {
                        let line = &lines[line_idx];
                        let line_abs_left = content_x + code_x + line.bounds.x0 as f32;
                        let line_abs_top = content_y + line_y;
                        let line_width = line.bounds.width() as f32;

                        let bounds = Rect::new(
                            line_abs_left as f64,
                            line_abs_top as f64,
                            (line_abs_left + line_width) as f64,
                            (line_abs_top + line.line_height) as f64,
                        );

                        self.page_context.add_element(VisualElement::TextLine {
                            runs: line.runs.clone(),
                            bounds,
                            line_height: line.line_height,
                        });

                        line_y += line.line_height;
                    }

                    if is_last {
                        // 最后一页：消耗实际使用的高度
                        let last_group_height: f32 = line_indices
                            .iter()
                            .map(|&i| lines[i].line_height)
                            .sum();
                        self.page_context.consume_height(last_group_height + margin_bottom);
                    } else {
                        // 非最后一页：结束当前页，移至下一页
                        self.page_context.start_new_page();
                    }
                }
            })
        })
    }

    // ─── 图片布局 ─────────────────────────────────────────────

    fn layout_image(&mut self, src: &str, alt: &str, style: &Style) {
        let content_width = self.page_context.settings.content_width();
        let content_x = self.page_context.settings.content_x();
        let content_y = self.page_context.settings.content_y();
        let margin_bottom = style.margin_bottom_pt;
        let image_result = self.load_image(src);

        const PDF_DPI: f32 = 72.0;
        const DEFAULT_IMAGE_DPI: f32 = 96.0;

        let (pixel_width, pixel_height, image_data, image_format, image_dpi) = match &image_result {
            Some(result) => {
                let format = format_to_string(result.format);
                let dpi = result.dpi.unwrap_or((DEFAULT_IMAGE_DPI, DEFAULT_IMAGE_DPI));
                (result.width, result.height, Some(result.data.clone()), format, dpi)
            }
            None => {
                let pw = (content_width * DEFAULT_IMAGE_DPI / PDF_DPI) as u32;
                let ph = (content_width * 0.75 * DEFAULT_IMAGE_DPI / PDF_DPI) as u32;
                (pw, ph, None, "jpeg".to_string(), (DEFAULT_IMAGE_DPI, DEFAULT_IMAGE_DPI))
            }
        };

        let dpi_x = image_dpi.0;

        let native_width = pixel_width as f32 * PDF_DPI / dpi_x;
        let density_at_content_width = pixel_width as f32 / content_width * PDF_DPI;

        let (display_width, display_height) = if density_at_content_width >= 96.0 && native_width < content_width {
            (content_width, pixel_height as f32 * content_width / pixel_width as f32)
        } else if native_width > content_width {
            (content_width, pixel_height as f32 * content_width / pixel_width as f32)
        } else {
            (native_width, pixel_height as f32 * native_width / pixel_width as f32)
        };

        let caption_style = crate::text::TextStyle {
            color: crate::visual::Color::new(102, 102, 102),
            font_family: style.font_family.clone(),
            font_size: 9.0,
            font_weight: "normal".to_string(),
            font_style: "normal".to_string(),
            align: crate::text::TextAlign::Center,
            url: None,
            decoration: crate::text::TextDecoration::None,
        };

        let label_height = if !alt.is_empty() {
            FONT_CONTEXT.with(|font_cx| {
                LAYOUT_CONTEXT.with(|layout_cx| {
                    let mut fcx = font_cx.borrow_mut();
                    let mut lcx = layout_cx.borrow_mut();

                    let layout = layout_text_with_contexts(
                        &[(alt, &caption_style)],
                        Some(display_width as f64),
                        TextAlign::Center,
                        &mut fcx,
                        &mut lcx,
                    );

                    layout.height as f32 + 4.0
                })
            })
        } else {
            0.0
        };

        let total_height_needed = display_height + label_height + margin_bottom;
        let remaining = self.page_context.remaining_height();
        let fits_on_current = display_width <= content_width
            && total_height_needed <= remaining;

        let (target_width, target_height) = if fits_on_current {
            (display_width, display_height)
        } else {
            let scale_w = content_width / display_width;
            let scaled_h = display_height * scale_w;
            let scaled_total = scaled_h + label_height + margin_bottom;
            if scaled_total <= remaining {
                (content_width, scaled_h)
            } else {
                if !self.page_context.is_empty() {
                    self.page_context.start_new_page();
                }
                let remaining_h = self.page_context.remaining_height();
                let total_on_new_page = display_height + label_height + margin_bottom;
                if total_on_new_page <= remaining_h {
                    (display_width, display_height)
                } else {
                    let scale_w = content_width / display_width;
                    let scale_h = (remaining_h - label_height - margin_bottom) / display_height;
                    let scale = scale_w.min(scale_h.max(0.1));
                    (display_width * scale, display_height * scale)
                }
            }
        };

        let top = content_y + self.page_context.current_y;
        let left = content_x + (content_width - target_width) / 2.0;

        self.page_context.add_element(VisualElement::Image {
            position: Point::new(left as f64, top as f64),
            size: vello_cpu::kurbo::Vec2::new(target_width as f64, target_height as f64),
            pixel_size: (pixel_width, pixel_height),
            data: image_data.unwrap_or_default(),
            format: image_format,
            alt: alt.to_string(),
        });

        let image_bottom = top + target_height;

        if !alt.is_empty() {
            FONT_CONTEXT.with(|font_cx| {
                LAYOUT_CONTEXT.with(|layout_cx| {
                    let mut fcx = font_cx.borrow_mut();
                    let mut lcx = layout_cx.borrow_mut();

                    let layout = layout_text_with_contexts(
                        &[(alt, &caption_style)],
                        Some(target_width as f64),
                        TextAlign::Center,
                        &mut fcx,
                        &mut lcx,
                    );

                    let label_top = image_bottom + 4.0;

                    for line in layout.lines.iter() {
                        let line_y = label_top + line.bounds.y0 as f32;
                        let line_width = line.bounds.width() as f32;
                        let line_x0 = line.bounds.x0 as f32;

                        let bounds = Rect::new(
                            (left + line_x0) as f64,
                            line_y as f64,
                            (left + line_x0 + line_width) as f64,
                            (line_y + line.line_height) as f64,
                        );

                        self.page_context.add_element(VisualElement::TextLine {
                            runs: line.runs.clone(),
                            bounds,
                            line_height: line.line_height,
                        });
                    }

                    let actual_label_height = layout.height as f32;
                    self.page_context.consume_height(target_height + 4.0 + actual_label_height + margin_bottom);
                })
            });
        } else {
            self.page_context.consume_height(target_height + margin_bottom);
        }
    }

    // ─── 段落布局 ─────────────────────────────────────────────

    fn layout_paragraph_with_indent(&mut self, children: &[Node], style: &Style, indent: f32) {
        // 先处理非文本子节点（如图片）
        for child in children {
            if let NodeKind::Image { src, alt, title: _ } = &child.kind {
                self.layout_image(src, alt, &child.style);
            }
        }

        let segments = collect_inline_segments(children);
        if segments.is_empty() {
            return;
        }

        let total_text: String = segments.iter().map(|(t, _)| t.as_str()).collect();
        let combined: Vec<(&str, &TextStyle)> =
            segments.iter().map(|(t, s)| (t.as_str(), s)).collect();
        let available_width = self.page_context.settings.content_width() - indent;
        let margin_bottom = style.margin_bottom_pt;

        FONT_CONTEXT.with(|font_cx| {
            LAYOUT_CONTEXT.with(|layout_cx| {
                let mut fcx = font_cx.borrow_mut();
                let mut lcx = layout_cx.borrow_mut();

                let layout = layout_text_with_contexts(
                    &combined,
                    Some(available_width as f64),
                    TextAlign::Left,
                    &mut fcx,
                    &mut lcx,
                );

                let mut lines = layout.lines;
                annotate_runs_with_urls(&mut lines, &total_text, &segments);
                self.place_text_lines(lines, indent, margin_bottom, true);
            })
        })
    }

    // ─── 列表布局 ─────────────────────────────────────────────

    fn layout_list(&mut self, children: &[Node], ordered: bool, start: u32, style: &Style) {
        let indent = style
            .list_indent_pt
            .unwrap_or_else(|| ast::calculate_list_indent(style.font_size_pt));
        self.layout_list_with_indent(children, ordered, start, indent, style);
    }

    fn layout_list_with_indent(
        &mut self,
        children: &[Node],
        ordered: bool,
        start: u32,
        indent: f32,
        style: &Style,
    ) {
        // 计算标记区域宽度
        let marker_area = if ordered {
            // 有序列表：根据最大编号宽度计算
            self.calculate_ordered_marker_width(children, start)
        } else {
            // 无序列表：固定宽度
            10.0
        };
        let marker_gap = 6.0; // 标记与内容之间的固定间距
        let content_indent = indent + marker_area + marker_gap; // 内容实际缩进

        for (index, item) in children.iter().enumerate() {
            let (item_children, is_task, checked) = match &item.kind {
                NodeKind::ListItem { children } => (children, false, false),
                NodeKind::TaskListItem { checked, children } => (children, true, *checked),
                _ => continue,
            };

            if item_children.is_empty() {
                continue;
            }

            // 生成列表标记
            let marker = if is_task {
                if checked { "☑".to_string() } else { "⬜".to_string() }
            } else if ordered {
                format!("{}.", start + index as u32)
            } else {
                "•".to_string()
            };

                // 布局列表项内容，同时处理标记
                let mut is_first = true;
                for grandchild in item_children {
                    if is_first {
                        // 第一个子节点：放置标记和内容的第一行
                        self.layout_list_item_first_child(
                            grandchild,
                            content_indent,
                            &marker,
                            indent,
                            marker_area,
                            ordered,
                            start,
                            style,
                        );
                        is_first = false;
                    } else {
                        self.layout_node_with_indent(grandchild, content_indent, ordered, start, style);
                    }
                }
            }
    }

    /// 计算有序列表的最大标记宽度
    fn calculate_ordered_marker_width(&self, children: &[Node], start: u32) -> f32 {
        use crate::text::create_text_layout;

        let item_count = children.len() as u32;
        let max_number = start + item_count - 1;

        // 生成最大编号的标记（如 "99."）
        let max_marker = format!("{}.", max_number);

        // 使用标记样式计算宽度
        let marker_style = ast::list_marker_style();
        let text_style = ast::computed_style_to_text_style(&marker_style);
        let layout = create_text_layout(&max_marker, &text_style, None);

        // 返回布局宽度 + 少量边距
        let width = layout.width as f32;
        (width + 4.0).max(12.0).min(30.0) // 最小12pt，最大30pt
    }

    fn layout_list_item_first_child(
        &mut self,
        node: &Node,
        content_indent: f32,
        marker: &str,
        marker_x: f32,
        marker_area: f32,
        ordered: bool,
        start: u32,
        style: &Style,
    ) {
        use crate::text::{create_text_layout, layout_text_with_contexts, TextAlign};

        let content_x = self.page_context.settings.content_x();
        let content_y = self.page_context.settings.content_y();
        let marker_style = ast::list_marker_style();
        let text_style = ast::computed_style_to_text_style(&marker_style);

        // 创建标记布局
        let marker_layout = create_text_layout(marker, &text_style, None);
        let marker_line = marker_layout.lines.first();
        let marker_advance = marker_line
            .and_then(|line| line.runs.first().map(|r| r.advance))
            .unwrap_or(0.0);

        match &node.kind {
            NodeKind::Paragraph { children } => {
                // 处理段落中的图片
                for child in children {
                    if let NodeKind::Image { src, alt, title: _ } = &child.kind {
                        self.layout_image(src, alt, &child.style);
                    }
                }

                let segments = collect_inline_segments(children);
                if segments.is_empty() {
                    // 没有文本内容，只放置标记
                    if let Some(m_line) = marker_line {
                        let line_y = content_y + self.page_context.current_y;
                        let marker_left = content_x + marker_x + marker_area - marker_advance;

                        let bounds = vello_cpu::kurbo::Rect::new(
                            marker_left as f64,
                            line_y as f64,
                            (marker_left + marker_advance) as f64,
                            (line_y + m_line.line_height) as f64,
                        );

                        self.page_context.add_element(crate::visual::VisualElement::TextLine {
                            runs: m_line.runs.clone(),
                            bounds,
                            line_height: m_line.line_height,
                        });
                        self.page_context.consume_height(m_line.line_height);
                    }
                    return;
                }

                let total_text: String = segments.iter().map(|(t, _)| t.as_str()).collect();
                let combined: Vec<(&str, &TextStyle)> =
                    segments.iter().map(|(t, s)| (t.as_str(), s)).collect();
                let available_width = self.page_context.settings.content_width() - content_indent;
                let margin_bottom = node.style.margin_bottom_pt;

                FONT_CONTEXT.with(|font_cx| {
                    LAYOUT_CONTEXT.with(|layout_cx| {
                        let mut fcx = font_cx.borrow_mut();
                        let mut lcx = layout_cx.borrow_mut();

                        let layout = layout_text_with_contexts(
                            &combined,
                            Some(available_width as f64),
                            TextAlign::Left,
                            &mut fcx,
                            &mut lcx,
                        );

                        let mut lines = layout.lines;
                        annotate_runs_with_urls(&mut lines, &total_text, &segments);

                        // 放置标记和内容的第一行
                        if let (Some(m_line), Some(first_line)) = (marker_line, lines.first()) {
                            let line_y = content_y + self.page_context.current_y;
                            let marker_left = content_x + marker_x + marker_area - marker_advance;

                            // 计算标记和内容第一行的基线偏移，使两者基线对齐
                            let marker_baseline = m_line.runs.first().map(|r| r.baseline_y).unwrap_or(0.0);
                            let content_baseline = first_line.runs.first().map(|r| r.baseline_y).unwrap_or(0.0);
                            let baseline_offset = content_baseline - marker_baseline;

                            // 放置标记（垂直偏移使基线对齐）
                            let marker_y = line_y + baseline_offset;
                            let marker_bounds = vello_cpu::kurbo::Rect::new(
                                marker_left as f64,
                                marker_y as f64,
                                (marker_left + marker_advance) as f64,
                                (marker_y + m_line.line_height) as f64,
                            );

                            self.page_context.add_element(crate::visual::VisualElement::TextLine {
                                runs: m_line.runs.clone(),
                                bounds: marker_bounds,
                                line_height: m_line.line_height,
                            });

                            // 放置内容的第一行
                            let content_left = content_x + content_indent + first_line.bounds.x0 as f32;
                            let content_bounds = vello_cpu::kurbo::Rect::new(
                                content_left as f64,
                                line_y as f64,
                                (content_left + first_line.bounds.width() as f32) as f64,
                                (line_y + first_line.line_height) as f64,
                            );

                            self.page_context.add_element(crate::visual::VisualElement::TextLine {
                                runs: first_line.runs.clone(),
                                bounds: content_bounds,
                                line_height: first_line.line_height,
                            });

                            // 消费第一行的高度
                            let first_line_height = first_line.line_height.max(m_line.line_height + baseline_offset);
                            self.page_context.consume_height(first_line_height);

                            // 放置剩余的行
                            let settings = self.page_context.settings.clone();
                            let content_height = settings.content_height();
                            let mut current_page_y = self.page_context.current_y;

                            for line in lines.iter().skip(1) {
                                let line_bottom_rel = current_page_y + line.line_height;

                                if line_bottom_rel > content_height && !self.page_context.is_empty() {
                                    self.page_context.finalize_current_page();
                                    self.page_context.start_new_page();
                                    current_page_y = 0.0;
                                }

                                let line_abs_left = content_x + content_indent + line.bounds.x0 as f32;
                                let line_abs_top = content_y + current_page_y;

                                let bounds = vello_cpu::kurbo::Rect::new(
                                    line_abs_left as f64,
                                    line_abs_top as f64,
                                    (line_abs_left + line.bounds.width() as f32) as f64,
                                    (line_abs_top + line.line_height) as f64,
                                );

                                self.page_context.add_element(crate::visual::VisualElement::TextLine {
                                    runs: line.runs.clone(),
                                    bounds,
                                    line_height: line.line_height,
                                });

                                current_page_y += line.line_height;
                            }

                            // 消费剩余高度和底部边距
                            let consumed = current_page_y - self.page_context.current_y + margin_bottom;
                            self.page_context.consume_height(consumed);
                        }
                    })
                });
            }
            _ => {
                // 非段落节点：先放置标记，再布局节点
                if let Some(m_line) = marker_line {
                    let line_y = content_y + self.page_context.current_y;
                    let marker_left = content_x + marker_x + marker_area - marker_advance;

                    let bounds = vello_cpu::kurbo::Rect::new(
                        marker_left as f64,
                        line_y as f64,
                        (marker_left + marker_advance) as f64,
                        (line_y + m_line.line_height) as f64,
                    );

                    self.page_context.add_element(crate::visual::VisualElement::TextLine {
                        runs: m_line.runs.clone(),
                        bounds,
                        line_height: m_line.line_height,
                    });
                }
                self.layout_node_with_indent(node, content_indent, ordered, start, style);
            }
        }
    }

    fn layout_node_with_indent(
        &mut self,
        node: &Node,
        indent: f32,
        _ordered: bool,
        _start: u32,
        style: &Style,
    ) {
        match &node.kind {
            NodeKind::Paragraph { children } => {
                self.layout_paragraph_with_indent(children, &node.style, indent);
            }
            NodeKind::List {
                ordered: child_ordered,
                children,
                start: child_start,
            } => {
                self.layout_list_with_indent(
                    children,
                    *child_ordered,
                    child_start.unwrap_or(1),
                    indent + ast::calculate_list_indent(style.font_size_pt),
                    style,
                );
            }
            _ => {
                self.layout_node(node);
            }
        }
    }

    // ─── 引用块布局 ─────────────────────────────────────────────

    fn layout_blockquote(&mut self, children: &[Node], style: &Style) {
        let content_x = self.page_context.settings.content_x();
        let content_y = self.page_context.settings.content_y();
        let estimated_height = estimate_children_height(children) + 16.0;

        if estimated_height > self.page_context.remaining_height() && !self.page_context.is_empty() {
            self.page_context.start_new_page();
        }

        let top = content_y + self.page_context.current_y;

        let border = VisualElement::Rect {
            rect: Rect::new(
                content_x as f64,
                top as f64,
                (content_x + 4.0) as f64,
                (top + estimated_height - 8.0) as f64,
            ),
            style: FillStrokeStyle {
                fill: Some(Color::new(200, 200, 200)),
                stroke: None,
            },
        };
        self.page_context.add_element(border);

        self.page_context.consume_height(8.0);

        for child in children {
            self.layout_blockquote_child(child, style);
        }

        self.page_context.consume_height(8.0);
    }

    fn layout_blockquote_child(&mut self, node: &Node, style: &Style) {
        match &node.kind {
            NodeKind::Paragraph { children } => {
                self.layout_paragraph_with_indent(children, &node.style, 24.0);
            }
            NodeKind::List { ordered, children, start } => {
                self.layout_list_with_indent(children, *ordered, start.unwrap_or(1), 24.0, style);
            }
            _ => {
                self.layout_node(node);
            }
        }
    }

    // ─── 容器布局 (div / center) ─────────────────────────────

    /// 布局块级容器（div、center），递归布局子节点。
    ///
    /// # 参数
    /// - `children`: 子节点列表
    /// - `style`: 容器的样式
    /// - `centered`: 是否居中对齐子节点
    fn layout_container(&mut self, children: &[Node], style: &Style, centered: bool) {
        // 先估算高度判断是否需要分页
        let estimated_height = estimate_children_height(children);
        if estimated_height > self.page_context.remaining_height() && !self.page_context.is_empty() {
            self.page_context.start_new_page();
        }

        self.page_context.consume_height(style.margin_top_pt);

        for child in children {
            if centered {
                // 强制居中的容器：将子节点的 text-align 覆盖为 center
                let mut child = child.clone();
                child.style.text_align = crate::ast::TextAlign::Center;
                self.layout_node(&child);
            } else {
                self.layout_node(child);
            }
        }

        self.page_context.consume_height(style.margin_bottom_pt);
    }

    fn layout_thematic_break(&mut self, style: &Style) {
        let content_x = self.page_context.settings.content_x();
        let content_width = self.page_context.settings.content_width();
        let content_y = self.page_context.settings.content_y();
        let margin = style.margin_top_pt;
        let total_height = 1.0 + margin;

        if total_height > self.page_context.remaining_height() && !self.page_context.is_empty() {
            self.page_context.start_new_page();
        }

        let top = content_y + self.page_context.current_y + margin / 2.0;

        let line = VisualElement::Line {
            start: Point::new(content_x as f64, top as f64),
            end: Point::new(
                (content_x + content_width) as f64,
                top as f64,
            ),
            style: StrokeStyle {
                color: Color::new(200, 200, 200),
                width: 1.0,
            },
        };
        self.page_context.add_element(line);

        self.page_context.consume_height(total_height);
    }

    // ─── 表格布局 ─────────────────────────────────────────────

    fn layout_table(&mut self, node: &Node) {
        use crate::generator::table::{compute_layout_info, generate_rows};

        let content_width = self.page_context.settings.content_width();
        let content_x = self.page_context.settings.content_x();
        let content_y = self.page_context.settings.content_y();
        let margin_bottom = node.style.margin_bottom_pt;

        // 1. 计算表格布局（列宽、行高），不生成视觉元素
        let layout = compute_layout_info(node, content_width);
        if layout.num_rows == 0 || layout.num_cols == 0 {
            return;
        }

        // 2. 按行逐页放置
        let mut row_idx = 0;
        let mut total_consumed = 0.0_f32;

        while row_idx < layout.num_rows {
            let remaining = self.page_context.remaining_height();

            // 找到能放在当前页的最大行区间
            let mut chunk_height = 0.0_f32;
            let mut end_idx = row_idx;
            while end_idx < layout.num_rows
                && chunk_height + layout.row_heights[end_idx] <= remaining
            {
                chunk_height += layout.row_heights[end_idx];
                end_idx += 1;
            }

            // 如果一行都放不下，强制换页
            if end_idx == row_idx {
                if !self.page_context.is_empty() {
                    self.page_context.start_new_page();
                    continue;
                }
                // 强制放至少一行（会溢出页面，但这是极限情况）
                end_idx = row_idx + 1;
                chunk_height = layout.row_heights[row_idx];
            }

            // 3. 为该行区间生成视觉元素（相对坐标，从 0 开始）
            let page_y = content_y + self.page_context.current_y;
            let elements = generate_rows(node, &layout, row_idx, end_idx, &node.style);

            // 4. 平移到页面绝对坐标
            for element in elements {
                let shifted = shift_element(element, content_x as f64, page_y as f64);
                self.page_context.add_element(shifted);
            }

            // 5. 消耗高度
            self.page_context.consume_height(chunk_height);
            total_consumed += chunk_height;
            row_idx = end_idx;

            // 6. 如果还有剩余行，换页
            if row_idx < layout.num_rows {
                self.page_context.start_new_page();
            }
        }

        // 最后加上底部边距
        if total_consumed > 0.0 {
            self.page_context.consume_height(margin_bottom);
        }
    }

    // ─── 图片加载 ─────────────────────────────────────────────

    fn load_image(&self, url: &str) -> Option<ImageLoadResult> {
        let path = match &self.base_dir {
            Some(base) => {
                let p = Path::new(url);
                if p.is_relative() {
                    base.join(p)
                } else {
                    p.to_path_buf()
                }
            }
            None => Path::new(url).to_path_buf(),
        };
        if !path.exists() {
            return None;
        }

        // 使用 ImageReader 读取图片，支持格式检测和 EXIF 读取
        let file = std::fs::File::open(&path).ok()?;
        let reader = image::ImageReader::new(std::io::BufReader::new(file));
        let reader = reader.with_guessed_format().ok()?;

        let format = reader.format()?;

        // 获取 decoder 读取 EXIF 和尺寸
        let mut decoder = reader.into_decoder().ok()?;
        let (width, height) = decoder.dimensions();

        // 读取 EXIF 元数据中的 DPI
        let dpi = read_dpi_from_decoder(&mut decoder);

        // 对于 PDF 支持的格式，读取原始文件数据；否则转换为 PNG
        let (output_data, output_format) = if is_pdf_supported_format(format) {
            // 重新读取原始文件数据
            let data = std::fs::read(&path).ok()?;
            (data, format)
        } else {
            // 转换为 PNG
            let img = image::DynamicImage::from_decoder(decoder).ok()?;
            let mut png_data = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut png_data), image::ImageFormat::Png)
                .ok()?;
            (png_data, image::ImageFormat::Png)
        };

        Some(ImageLoadResult {
            width,
            height,
            data: output_data,
            dpi,
            format: output_format,
        })
    }
}

/// PDF 内置支持的图片格式
const PDF_SUPPORTED_FORMATS: &[image::ImageFormat] = &[
    image::ImageFormat::Png,
    image::ImageFormat::Jpeg,
    image::ImageFormat::Gif,
];

fn is_pdf_supported_format(format: image::ImageFormat) -> bool {
    PDF_SUPPORTED_FORMATS.contains(&format)
}

/// 将 image::ImageFormat 转换为渲染器使用的格式字符串
fn format_to_string(format: image::ImageFormat) -> String {
    match format {
        image::ImageFormat::Png => "png".to_string(),
        image::ImageFormat::Jpeg => "jpeg".to_string(),
        image::ImageFormat::Gif => "gif".to_string(),
        _ => "png".to_string(), // 其他格式已转换为 PNG
    }
}

struct ImageLoadResult {
    width: u32,
    height: u32,
    data: Vec<u8>,
    dpi: Option<(f32, f32)>,
    format: image::ImageFormat,
}

/// 通过 ImageDecoder 读取 EXIF 元数据中的 DPI
fn read_dpi_from_decoder(decoder: &mut dyn image::ImageDecoder) -> Option<(f32, f32)> {
    let exif_data = decoder.exif_metadata().ok()??;
    parse_exif_dpi(&exif_data)
}

/// 使用 kamadak-exif 解析 EXIF 数据中的 DPI
fn parse_exif_dpi(exif_data: &[u8]) -> Option<(f32, f32)> {
    use exif::{In, Tag, Value};

    let mut cursor = std::io::Cursor::new(exif_data);
    let reader = exif::Reader::new();
    let exif = reader.read_from_container(&mut cursor).ok()?;

    let xres = exif.get_field(Tag::XResolution, In::PRIMARY)?;
    let yres = exif.get_field(Tag::YResolution, In::PRIMARY)?;

    let dpi_x = match &xres.value {
        Value::Rational(v) if !v.is_empty() => v[0].to_f64() as f32,
        _ => return None,
    };
    let dpi_y = match &yres.value {
        Value::Rational(v) if !v.is_empty() => v[0].to_f64() as f32,
        _ => return None,
    };

    if dpi_x <= 0.0 || dpi_y <= 0.0 {
        return None;
    }

    let unit = exif
        .get_field(Tag::ResolutionUnit, In::PRIMARY)
        .and_then(|f| f.value.get_uint(0));

    match unit {
        Some(3) => Some((dpi_x * 2.54, dpi_y * 2.54)),
        _ => Some((dpi_x, dpi_y)),
    }
}

/// 从 PNG 数据中读取 DPI（pHYs 块）- 作为回退方案
#[allow(dead_code)]
fn read_png_dpi(data: &[u8]) -> Option<(f32, f32)> {
    if data.len() < 8 || &data[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }

    let mut pos = 8;
    while pos + 12 <= data.len() {
        let chunk_len =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let chunk_type = &data[pos + 4..pos + 8];

        if chunk_type == b"pHYs" && chunk_len >= 9 && pos + 12 + chunk_len <= data.len() {
            let ppu_x = u32::from_be_bytes([
                data[pos + 8],
                data[pos + 9],
                data[pos + 10],
                data[pos + 11],
            ]);
            let ppu_y = u32::from_be_bytes([
                data[pos + 12],
                data[pos + 13],
                data[pos + 14],
                data[pos + 15],
            ]);
            let unit = data[pos + 16];
            if unit == 1 && ppu_x > 0 && ppu_y > 0 {
                let dpi_x = ppu_x as f32 * 0.0254;
                let dpi_y = ppu_y as f32 * 0.0254;
                return Some((dpi_x, dpi_y));
            }
        }

        if chunk_type == b"IEND" {
            break;
        }

        pos += 12 + chunk_len;
    }
    None
}

// ─── 辅助函数 ───────────────────────────────────────────────────

/// 递归平移 VisualElement 中的所有坐标
fn shift_element(element: VisualElement, dx: f64, dy: f64) -> VisualElement {
    match element {
        VisualElement::Rect { rect, style } => VisualElement::Rect {
            rect: Rect::new(rect.x0 + dx, rect.y0 + dy, rect.x1 + dx, rect.y1 + dy),
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

// ─── Markdown 转 Document ───────────────────────────────────────

/// 使用内置默认样式将 Markdown 转换为 Document
pub fn markdown_to_document(markdown: &str) -> Document {
    markdown_to_document_with_base_dir(markdown, None)
}

/// 使用内置默认样式和基础路径将 Markdown 转换为 Document
pub fn markdown_to_document_with_base_dir(markdown: &str, base_dir: Option<PathBuf>) -> Document {
    let styled_root = crate::ast::parse_markdown(markdown)
        .unwrap_or_else(|_| crate::ast::Node::new(
            crate::ast::NodeKind::Document { children: vec![] },
            crate::ast::Style::default(),
            false,
        ));

    let mut generator = DocumentGenerator::new();
    if let Some(dir) = base_dir {
        generator = generator.with_base_dir(dir);
    }

    if let crate::ast::NodeKind::Document { children } = &styled_root.kind {
        for child in children {
            generator.layout_node(child);
        }
    }

    generator.finish()
}

/// 使用自定义 CSS 将 Markdown 转换为 Document
pub fn markdown_to_document_with_css(markdown: &str, user_css: &str) -> Document {
    markdown_to_document_with_css_and_base_dir(markdown, user_css, None)
}

/// 使用自定义 CSS 和基础路径将 Markdown 转换为 Document
pub fn markdown_to_document_with_css_and_base_dir(
    markdown: &str,
    user_css: &str,
    base_dir: Option<PathBuf>,
) -> Document {
    let (styled_root, page_config) = match crate::ast::parse_markdown_with_css(markdown, user_css) {
        Ok((node, config)) => (node, config),
        Err(_) => (
            crate::ast::Node::new(
                crate::ast::NodeKind::Document { children: vec![] },
                crate::ast::Style::default(),
                false,
            ),
            crate::ast::PageConfig::default(),
        ),
    };

    let settings = PageSettings::from(page_config);
    let mut generator = DocumentGenerator::with_settings(settings);
    if let Some(dir) = base_dir {
        generator = generator.with_base_dir(dir);
    }

    if let crate::ast::NodeKind::Document { children } = &styled_root.kind {
        for child in children {
            generator.layout_node(child);
        }
    }

    generator.finish()
}

/// 从 Markdown 生成 Document，使用自定义页面设置
pub fn markdown_to_document_with_settings(markdown: &str, settings: PageSettings) -> Document {
    markdown_to_document_with_settings_and_base_dir(markdown, settings, None)
}

/// 从 Markdown 生成 Document，使用自定义页面设置和基础目录
pub fn markdown_to_document_with_settings_and_base_dir(
    markdown: &str,
    settings: PageSettings,
    base_dir: Option<PathBuf>,
) -> Document {
    let styled_root = crate::ast::parse_markdown(markdown)
        .unwrap_or_else(|_| crate::ast::Node::new(
            crate::ast::NodeKind::Document { children: vec![] },
            crate::ast::Style::default(),
            false,
        ));

    let mut generator = DocumentGenerator::with_settings(settings);
    if let Some(dir) = base_dir {
        generator = generator.with_base_dir(dir);
    }

    if let crate::ast::NodeKind::Document { children } = &styled_root.kind {
        for child in children {
            generator.layout_node(child);
        }
    }

    generator.finish()
}

/// 使用自定义 CSS 将 Markdown 转换为 Document（严格模式）
///
/// 与 `markdown_to_document_with_css` 不同，此函数在 CSS 解析失败时返回错误。
pub fn markdown_to_document_with_css_strict(markdown: &str, user_css: &str) -> Result<Document, String> {
    markdown_to_document_with_css_and_base_dir_strict(markdown, user_css, None)
}

/// 使用自定义 CSS 和基础路径将 Markdown 转换为 Document（严格模式）
pub fn markdown_to_document_with_css_and_base_dir_strict(
    markdown: &str,
    user_css: &str,
    base_dir: Option<PathBuf>,
) -> Result<Document, String> {
    let (styled_root, page_config) = crate::ast::parse_markdown_with_css_strict(markdown, user_css)?;
    let settings = PageSettings::from(page_config);
    let mut generator = DocumentGenerator::with_settings(settings);
    if let Some(dir) = base_dir {
        generator = generator.with_base_dir(dir);
    }

    if let crate::ast::NodeKind::Document { children } = &styled_root.kind {
        for child in children {
            generator.layout_node(child);
        }
    }

    Ok(generator.finish())
}

// ─── 页眉页脚注入 ─────────────────────────────────────────

/// 在所有页面中注入页眉和页脚视觉元素。
///
/// 页眉放置在内容区上方（顶部边距区域），页脚放置在内容区下方（底部边距区域）。
/// 支持 `{page}`（当前页码）和 `{total}`（总页数）模板变量。
fn inject_header_footer_into_document(doc: &mut Document, settings: &PageSettings) {
    let total_pages = doc.pages.len();
    if total_pages == 0 {
        return;
    }

    let content_x = settings.content_x();
    let content_y = settings.content_y();
    let content_width = settings.content_width();
    let content_height = settings.content_height();

    for page in &mut doc.pages {
        let page_num = page.index + 1;

        // ── 页眉 ──
        if let Some(ref header_template) = settings.header {
            let text = header_template
                .replace("{page}", &page_num.to_string())
                .replace("{total}", &total_pages.to_string());

            let header_style = TextStyle {
                color: Color::new(100, 100, 100),
                font_family: vec!["sans-serif".to_string()],
                font_size: settings.header_font_size as f64,
                font_weight: "normal".to_string(),
                font_style: "normal".to_string(),
                align: TextAlign::Center,
                url: None,
                decoration: crate::text::TextDecoration::None,
            };

            let layout = crate::text::layout_text(
                &[(text.as_str(), &header_style)],
                Some(content_width as f64),
                TextAlign::Center,
            );

            let header_total_height: f32 = layout.lines.iter().map(|l| l.line_height).sum();
            let header_y = content_y - header_total_height - 4.0;

            let mut rel_y = 0.0_f32;
            for line in &layout.lines {
                let line_width = line.bounds.width() as f32;
                let x_offset = ((content_width - line_width) / 2.0).max(0.0);

                let abs_left = content_x + x_offset;
                let abs_top = header_y + rel_y;

                let bounds = Rect::new(
                    abs_left as f64,
                    abs_top as f64,
                    (abs_left + line_width) as f64,
                    (abs_top + line.line_height) as f64,
                );

                page.elements.push(VisualElement::TextLine {
                    runs: line.runs.clone(),
                    bounds,
                    line_height: line.line_height,
                });

                rel_y += line.line_height;
            }

            // 页眉分隔线（文字下方 2pt）
            let line_y = header_y + rel_y + 2.0;
            page.elements.push(VisualElement::Line {
                start: Point::new(content_x as f64, line_y as f64),
                end: Point::new((content_x + content_width) as f64, line_y as f64),
                style: StrokeStyle {
                    color: Color::new(200, 200, 200),
                    width: 0.5,
                },
            });
        }

        // ── 页脚 ──
        if let Some(ref footer_template) = settings.footer {
            let text = footer_template
                .replace("{page}", &page_num.to_string())
                .replace("{total}", &total_pages.to_string());

            let footer_style = TextStyle {
                color: Color::new(100, 100, 100),
                font_family: vec!["sans-serif".to_string()],
                font_size: settings.footer_font_size as f64,
                font_weight: "normal".to_string(),
                font_style: "normal".to_string(),
                align: TextAlign::Center,
                url: None,
                decoration: crate::text::TextDecoration::None,
            };

            let layout = crate::text::layout_text(
                &[(text.as_str(), &footer_style)],
                Some(content_width as f64),
                TextAlign::Center,
            );

            let footer_y = content_y + content_height + 4.0;

            let mut rel_y = 0.0_f32;
            for line in &layout.lines {
                let line_width = line.bounds.width() as f32;
                let x_offset = ((content_width - line_width) / 2.0).max(0.0);

                let abs_left = content_x + x_offset;
                let abs_top = footer_y + rel_y;

                let bounds = Rect::new(
                    abs_left as f64,
                    abs_top as f64,
                    (abs_left + line_width) as f64,
                    (abs_top + line.line_height) as f64,
                );

                page.elements.push(VisualElement::TextLine {
                    runs: line.runs.clone(),
                    bounds,
                    line_height: line.line_height,
                });

                rel_y += line.line_height;
            }
        }
    }
}

/// 使用自定义 CSS 和页面配置覆盖将 Markdown 转换为 Document
///
/// `page_config_override` 的优先级最高，会覆盖 CSS `@page` 规则中的同名字段。
/// 如果不需要覆盖，传 `None` 即可。
pub fn markdown_to_document_with_css_and_page_config(
    markdown: &str,
    user_css: &str,
    page_config_override: Option<crate::ast::PageConfig>,
    base_dir: Option<PathBuf>,
    strict: bool,
) -> Result<Document, String> {
    let (styled_root, mut page_config) = if strict {
        crate::ast::parse_markdown_with_css_strict(markdown, user_css)?
    } else {
        match crate::ast::parse_markdown_with_css(markdown, user_css) {
            Ok(result) => result,
            Err(_) => (
                crate::ast::Node::new(
                    crate::ast::NodeKind::Document { children: vec![] },
                    crate::ast::Style::default(),
                    false,
                ),
                crate::ast::PageConfig::default(),
            ),
        }
    };

    // 应用 page_config_override（最高优先级，覆盖 CSS @page 规则）
    if let Some(ref override_config) = page_config_override {
        if let Some(v) = override_config.width {
            page_config.width = Some(v);
        }
        if let Some(v) = override_config.height {
            page_config.height = Some(v);
        }
        if let Some(v) = override_config.margin_top {
            page_config.margin_top = Some(v);
        }
        if let Some(v) = override_config.margin_bottom {
            page_config.margin_bottom = Some(v);
        }
        if let Some(v) = override_config.margin_left {
            page_config.margin_left = Some(v);
        }
        if let Some(v) = override_config.margin_right {
            page_config.margin_right = Some(v);
        }
        if let Some(v) = override_config.header.clone() {
            page_config.header = Some(v);
        }
        if let Some(v) = override_config.footer.clone() {
            page_config.footer = Some(v);
        }
        if let Some(v) = override_config.header_font_size {
            page_config.header_font_size = Some(v);
        }
        if let Some(v) = override_config.footer_font_size {
            page_config.footer_font_size = Some(v);
        }
    }

    let settings = PageSettings::from(page_config);
    let mut generator = DocumentGenerator::with_settings(settings);
    if let Some(dir) = base_dir {
        generator = generator.with_base_dir(dir);
    }

    if let crate::ast::NodeKind::Document { children } = &styled_root.kind {
        for child in children {
            generator.layout_node(child);
        }
    }

    Ok(generator.finish())
}