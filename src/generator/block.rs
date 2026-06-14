//! 块级元素布局（Block-level Layout）
//!
//! 包含 `DocumentGenerator` 上所有块级元素的布局方法：
//! 段落、标题、代码块、列表、引用块、分割线、容器、表格等。
//!
//! 这些方法负责内容面积计算、分页决策、视觉元素生成。
//! 共享的 Box Model 基础设施在父模块 `mod.rs` 中定义。

use crate::ast::{ListStyleType, Node, NodeKind};
use crate::generator::text::{
    annotate_runs_with_urls, collect_inline_segments, estimate_children_height,
};
use crate::text::{
    FONT_CONTEXT, LAYOUT_CONTEXT, TextAlign, TextStyle, create_text_layout,
    layout_text_with_contexts,
};
use crate::visual::{Color, StrokeStyle, VisualElement};
use vello_cpu::kurbo::{Point, Rect};

use super::DocumentGenerator;
use crate::generator::box_model::BgStyle;

// ═══════════════════════════════════════════════════════════
// Block Layout Methods
// ═══════════════════════════════════════════════════════════

impl DocumentGenerator {
    // ─── 段落布局 ─────────────────────────────────────────────

    pub(crate) fn layout_paragraph(&mut self, children: &[Node], style: &crate::ast::Style) {
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

        // 通过 box model 处理 margin.top + padding.top，计算内容区缩进和宽度
        let (indent, available_width, content_start_y, _prev_indent) =
            crate::generator::box_model::begin_box(
                &mut self.page_context,
                self.layout_ctx.current_indent,
                style,
            );

        // 构建背景样式（用于跨页分段绘制）
        let bg = style.background_color.map(|color| BgStyle {
            color,
            padding: style.padding,
            margin: style.margin,
        });

        let align = match style.text_align {
            crate::ast::TextAlign::Left => TextAlign::Left,
            crate::ast::TextAlign::Center => TextAlign::Center,
            crate::ast::TextAlign::Right => TextAlign::Right,
            crate::ast::TextAlign::Justify => TextAlign::Left,
        };

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
                // 使用 box model 计算出的 indent 作为 x_offset
                let prev_indent = self.layout_ctx.current_indent;
                self.layout_ctx.current_indent = indent;
                self.place_text_lines(
                    lines,
                    true,
                    bg.as_ref(),
                    style.text_indent_em * style.font_size_pt,
                );
                self.layout_ctx.current_indent = prev_indent;
            })
        });

        // 绘制边框（使用 begin_box 记录的起始位置）
        crate::generator::box_model::draw_box_border(
            &mut self.page_context,
            style,
            content_start_y,
            indent,
            available_width,
        );

        // 收尾：padding.bottom + margin.bottom
        crate::generator::box_model::end_box_content(&mut self.page_context, style);
    }

    // ─── 标题布局 ─────────────────────────────────────────────

    pub(crate) fn layout_heading(
        &mut self,
        level: u8,
        children: &[Node],
        style: &crate::ast::Style,
    ) {
        let segments = collect_inline_segments(children);
        if segments.is_empty() {
            return;
        }

        let total_text: String = segments.iter().map(|(t, _)| t.as_str()).collect();

        // 先布局文本获取高度，用于分页判断
        let combined: Vec<(&str, &TextStyle)> =
            segments.iter().map(|(t, s)| (t.as_str(), s)).collect();

        let align = match style.text_align {
            crate::ast::TextAlign::Left => TextAlign::Left,
            crate::ast::TextAlign::Center => TextAlign::Center,
            crate::ast::TextAlign::Right => TextAlign::Right,
            crate::ast::TextAlign::Justify => TextAlign::Left,
        };

        // 预布局获取文本行和总行高（在 FONT_CONTEXT 外完成标注）
        let available_width = self.page_context.settings.content_width()
            - self.layout_ctx.current_indent
            - style.margin.left
            - style.padding.left
            - style.margin.right
            - style.padding.right;

        let (mut lines, total_line_height) = FONT_CONTEXT.with(|font_cx| {
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

                let total: f32 = layout.lines.iter().map(|l| l.line_height).sum();
                (layout.lines, total)
            })
        });
        annotate_runs_with_urls(&mut lines, &total_text, &segments);

        // 计算总高度用于分页判断
        let total_height = total_line_height
            + style.margin.top
            + style.padding.top
            + style.padding.bottom
            + style.margin.bottom;

        if total_height > self.page_context.settings.content_height() {
            panic!(
                "标题 '{}' 高度 ({:.1}pt) 超过页面内容区高度 ({:.1}pt)，无法渲染",
                total_text.chars().take(50).collect::<String>(),
                total_height,
                self.page_context.settings.content_height()
            );
        }

        // 页面破裂检查 — 在消耗任何高度之前
        if total_height > self.page_context.remaining_height() && !self.page_context.is_empty() {
            self.page_context.start_new_page();
        }

        // 通过 box model 处理 margin.top + padding.top，计算内容区缩进和宽度
        let (indent, _available_width, content_start_y, prev_indent) =
            crate::generator::box_model::begin_box(
                &mut self.page_context,
                self.layout_ctx.current_indent,
                style,
            );

        // 构建背景样式（标题不分页，但 place_text_lines 的 BgStyle 仍然兼容单页情况）
        let bg = style.background_color.map(|color| BgStyle {
            color,
            padding: style.padding,
            margin: style.margin,
        });

        let content_x = self.page_context.settings.content_x();
        let content_y = self.page_context.settings.content_y();
        let x_position = content_x + indent;

        // 记录大纲位置
        let page_number = self.page_context.current_page.index + 1;
        let y_position = content_y + self.page_context.current_y;
        self.layout_ctx.record_heading(
            level,
            total_text.clone(),
            page_number,
            x_position,
            y_position,
        );

        // 放置文本行
        let prev = self.layout_ctx.current_indent;
        self.layout_ctx.current_indent = indent;
        self.place_text_lines(lines, false, bg.as_ref(), 0.0);
        self.layout_ctx.current_indent = prev;

        // 收尾：绘制背景+边框，消耗 padding.bottom + margin.bottom
        crate::generator::box_model::end_box(
            &mut self.page_context,
            style,
            content_start_y,
            prev_indent,
        );
    }

    // ─── 代码块布局 ─────────────────────────────────────────────

    pub(crate) fn layout_code(&mut self, code: &str, style: &crate::ast::Style) {
        if code.is_empty() {
            return;
        }

        let code_style = crate::ast::computed_style_to_text_style(style);

        // 计算可用宽度（与 begin_box 中的逻辑一致），预布局获取文本行
        let available_width = self.page_context.settings.content_width()
            - self.layout_ctx.current_indent
            - style.margin.left
            - style.padding.left
            - style.margin.right
            - style.padding.right;

        let (lines, total_line_height) = FONT_CONTEXT.with(|font_cx| {
            LAYOUT_CONTEXT.with(|layout_cx| {
                let mut fcx = font_cx.borrow_mut();
                let mut lcx = layout_cx.borrow_mut();

                // white-space: pre 时：不自动换行，保留原始换行
                let max_width = if style.white_space == crate::ast::WhiteSpace::Pre {
                    None
                } else {
                    Some(available_width as f64)
                };

                let layout = layout_text_with_contexts(
                    &[(code, &code_style)],
                    max_width,
                    TextAlign::Left,
                    &mut fcx,
                    &mut lcx,
                );

                let total: f32 = layout.lines.iter().map(|l| l.line_height).sum();
                (layout.lines, total)
            })
        });

        // 计算总高度用于分页判断
        let total_height = total_line_height
            + style.margin.top
            + style.padding.top
            + style.padding.bottom
            + style.margin.bottom;

        if total_height > self.page_context.settings.content_height() {
            panic!(
                "代码块高度 ({:.1}pt) 超过页面内容区高度 ({:.1}pt)，无法渲染",
                total_height,
                self.page_context.settings.content_height()
            );
        }

        // 页面破裂检查 — 在消耗任何高度之前
        if total_height > self.page_context.remaining_height() && !self.page_context.is_empty() {
            self.page_context.start_new_page();
        }

        // 通过 box model 处理 margin.top + padding.top
        let (indent, _available_width, _content_start_y, _prev_indent) =
            crate::generator::box_model::begin_box(
                &mut self.page_context,
                self.layout_ctx.current_indent,
                style,
            );

        let content_x = self.page_context.settings.content_x();
        let content_y = self.page_context.settings.content_y();
        let content_width = self.page_context.settings.content_width();
        let code_x = 8.0; // 代码块内部缩进

        // 将行分组到各页面
        let settings = self.page_context.settings.clone();
        let content_height = settings.content_height();

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

        // 逐页放置背景和文本
        for (group_idx, line_indices) in page_groups.iter().enumerate() {
            let is_last = group_idx == page_groups.len() - 1;

            // 计算本页代码组的高度
            let group_height: f32 = line_indices.iter().map(|&i| lines[i].line_height).sum();

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
                style: crate::visual::FillStrokeStyle {
                    fill: Some(crate::visual::Color::new(245, 245, 245)),
                    stroke: None,
                },
            });

            // 再添加文本行
            let mut line_y = page_current_y;
            for &line_idx in line_indices {
                let line = &lines[line_idx];
                // 使用 begin_box 返回的 indent（含 margin.left + padding.left）+ code_x
                let line_abs_left = content_x + indent + code_x + line.bounds.x0 as f32;
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
                // 最后一页：消耗文本行高度（margin/padding 由 end_box_content 处理）
                let last_group_height: f32 =
                    line_indices.iter().map(|&i| lines[i].line_height).sum();
                self.page_context.consume_height(last_group_height);
            } else {
                // 非最后一页：结束当前页，移至下一页
                self.page_context.start_new_page();
            }
        }

        // 收尾：padding.bottom + margin.bottom
        crate::generator::box_model::end_box_content(&mut self.page_context, style);
    }

    // ─── 图片布局 ─────────────────────────────────────────────

    pub(crate) fn layout_image(&mut self, src: &str, alt: &str, style: &crate::ast::Style) {
        let content_width = self.page_context.settings.content_width();
        let content_x = self.page_context.settings.content_x();
        let content_y = self.page_context.settings.content_y();
        let image_result = crate::generator::image::load_image(src);

        const PDF_DPI: f32 = 72.0;
        const DEFAULT_IMAGE_DPI: f32 = 96.0;

        let (pixel_width, pixel_height, image_data, image_format, image_dpi) = match &image_result {
            Some(result) => {
                let format = crate::generator::image::format_to_string(result.format);
                let dpi = result.dpi.unwrap_or((DEFAULT_IMAGE_DPI, DEFAULT_IMAGE_DPI));
                (
                    result.width,
                    result.height,
                    Some(result.data.clone()),
                    format,
                    dpi,
                )
            }
            None => {
                let pw = (content_width * DEFAULT_IMAGE_DPI / PDF_DPI) as u32;
                let ph = (content_width * 0.75 * DEFAULT_IMAGE_DPI / PDF_DPI) as u32;
                (
                    pw,
                    ph,
                    None,
                    "jpeg".to_string(),
                    (DEFAULT_IMAGE_DPI, DEFAULT_IMAGE_DPI),
                )
            }
        };

        let dpi_x = image_dpi.0;
        let native_width = pixel_width as f32 * PDF_DPI / dpi_x;
        let density_at_content_width = pixel_width as f32 / content_width * PDF_DPI;

        let (display_width, display_height) =
            if density_at_content_width >= 96.0 && native_width < content_width {
                (
                    content_width,
                    pixel_height as f32 * content_width / pixel_width as f32,
                )
            } else if native_width > content_width {
                (
                    content_width,
                    pixel_height as f32 * content_width / pixel_width as f32,
                )
            } else {
                (
                    native_width,
                    pixel_height as f32 * native_width / pixel_width as f32,
                )
            };

        let caption_style = TextStyle {
            color: crate::visual::Color::new(102, 102, 102),
            font_family: style.font_family.clone(),
            font_size: 9.0,
            font_weight: "normal".to_string(),
            font_style: "normal".to_string(),
            align: TextAlign::Center,
            url: None,
            decoration: crate::text::TextDecoration::None,
            baseline_shift: 0.0,
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

        // 通过 box model 处理 margin.top + padding.top
        let (_indent, _available_width, content_start_y, prev_indent) =
            crate::generator::box_model::begin_box(
                &mut self.page_context,
                self.layout_ctx.current_indent,
                style,
            );

        // 分页判断和缩放（此时 remaining_height 已扣除 margin.top + padding.top）
        let total_content_height = display_height + label_height;
        let total_height_needed = total_content_height + style.padding.bottom + style.margin.bottom;
        let remaining = self.page_context.remaining_height();
        let fits_on_current = display_width <= content_width && total_height_needed <= remaining;

        let (target_width, target_height) = if fits_on_current {
            (display_width, display_height)
        } else {
            let scale_w = content_width / display_width;
            let scaled_h = display_height * scale_w;
            let scaled_total = scaled_h + label_height + style.padding.bottom + style.margin.bottom;
            if scaled_total <= remaining {
                (content_width, scaled_h)
            } else {
                if !self.page_context.is_empty() {
                    self.page_context.start_new_page();
                }
                let remaining_h = self.page_context.remaining_height();
                let total_on_new_page =
                    display_height + label_height + style.padding.bottom + style.margin.bottom;
                if total_on_new_page <= remaining_h {
                    (display_width, display_height)
                } else {
                    let scale_w = content_width / display_width;
                    let scale_h =
                        (remaining_h - label_height - style.padding.bottom - style.margin.bottom)
                            / display_height;
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
                    // 只消耗内容高度，margin/padding 由 end_box 处理
                    self.page_context
                        .consume_height(target_height + 4.0 + actual_label_height);
                })
            });
        } else {
            // 只消耗内容高度，margin/padding 由 end_box 处理
            self.page_context.consume_height(target_height);
        }

        // 收尾：绘制背景+边框，消耗 padding.bottom + margin.bottom
        crate::generator::box_model::end_box(
            &mut self.page_context,
            style,
            content_start_y,
            prev_indent,
        );
    }

    // ─── 列表布局 ─────────────────────────────────────────────

    pub(crate) fn layout_list(
        &mut self,
        children: &[Node],
        ordered: bool,
        start: u32,
        style: &crate::ast::Style,
    ) {
        let list_indent = style
            .list_indent_pt
            .unwrap_or_else(|| crate::ast::calculate_list_indent(style.font_size_pt));
        self.with_additional_indent(list_indent, |s| {
            s.layout_list_with_indent(children, ordered, start, style);
        });
    }

    fn layout_list_with_indent(
        &mut self,
        children: &[Node],
        ordered: bool,
        start: u32,
        style: &crate::ast::Style,
    ) {
        const MARKER_GAP: f32 = 6.0; // 标记与内容之间的固定间距

        // 计算标记区域宽度
        let marker_area = if ordered {
            self.calculate_ordered_marker_width(children, start, style.list_style_type)
        } else {
            10.0
        };
        let marker_base = self.layout_ctx.current_indent; // 标记基准位置
        let content_indent = marker_base + marker_area + MARKER_GAP; // 内容实际缩进

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
                if checked {
                    "☑".to_string()
                } else {
                    "⬜".to_string()
                }
            } else if ordered {
                ordered_marker(style.list_style_type, start + index as u32)
            } else {
                unordered_marker(style.list_style_type)
            };

            // 在内容缩进上下文中布局列表项
            self.with_indent(content_indent, |s| {
                let mut is_first = true;
                for grandchild in item_children {
                    if is_first {
                        s.layout_list_item_first_child(grandchild, &marker);
                        is_first = false;
                    } else {
                        s.layout_node_with_indent(grandchild, style);
                    }
                }
            });
        }
    }

    /// 计算有序列表的最大标记宽度
    fn calculate_ordered_marker_width(
        &self,
        children: &[Node],
        start: u32,
        list_style: ListStyleType,
    ) -> f32 {
        let item_count = children.len() as u32;
        let max_number = start + item_count - 1;

        // 使用 ordered_marker 生成最大标记（支持罗马数字、字母等）
        let max_marker = ordered_marker(list_style, max_number);

        // 使用标记样式计算宽度
        let marker_style = crate::ast::list_marker_style();
        let text_style = crate::ast::computed_style_to_text_style(&marker_style);
        let layout = create_text_layout(&max_marker, &text_style, None);

        // 返回布局宽度 + 少量边距
        let width = layout.width as f32;
        (width + 4.0).max(12.0).min(30.0) // 最小12pt，最大30pt
    }

    fn layout_list_item_first_child(&mut self, node: &Node, marker: &str) {
        const MARKER_GAP: f32 = 6.0;

        let content_x = self.page_context.settings.content_x();
        let content_y = self.page_context.settings.content_y();
        let marker_style = crate::ast::list_marker_style();
        let text_style = crate::ast::computed_style_to_text_style(&marker_style);

        // 创建标记布局
        let marker_layout = create_text_layout(marker, &text_style, None);
        let marker_line = marker_layout.lines.first();
        let marker_advance = marker_line
            .and_then(|line| line.runs.first().map(|r| r.advance))
            .unwrap_or(0.0);

        // 标记位置 = 内容缩进 - 间距 - 标记宽度（右对齐）
        let content_indent = self.layout_ctx.current_indent;
        let marker_left_base = content_x + content_indent - MARKER_GAP - marker_advance;

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
                        let bounds = Rect::new(
                            marker_left_base as f64,
                            line_y as f64,
                            (marker_left_base + marker_advance) as f64,
                            (line_y + m_line.line_height) as f64,
                        );

                        self.page_context.add_element(VisualElement::TextLine {
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
                let margin_bottom = node.style.margin.bottom;

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

                        // 检查第一行是否有足够空间，不够则分页
                        if let Some(first_line) = lines.first() {
                            let first_height = first_line
                                .line_height
                                .max(marker_line.as_ref().map(|m| m.line_height).unwrap_or(0.0));
                            if first_height > self.page_context.remaining_height()
                                && !self.page_context.is_empty()
                            {
                                self.page_context.start_new_page();
                            }
                        }

                        // 放置标记和内容的第一行
                        if let (Some(m_line), Some(first_line)) = (marker_line, lines.first()) {
                            let line_y = content_y + self.page_context.current_y;

                            // 计算标记和内容第一行的基线偏移，使两者基线对齐
                            let marker_baseline =
                                m_line.runs.first().map(|r| r.baseline_y).unwrap_or(0.0);
                            let content_baseline =
                                first_line.runs.first().map(|r| r.baseline_y).unwrap_or(0.0);
                            let baseline_offset = content_baseline - marker_baseline;

                            // 放置标记（垂直偏移使基线对齐）
                            let marker_y = line_y + baseline_offset;
                            let marker_bounds = Rect::new(
                                marker_left_base as f64,
                                marker_y as f64,
                                (marker_left_base + marker_advance) as f64,
                                (marker_y + m_line.line_height) as f64,
                            );

                            self.page_context.add_element(VisualElement::TextLine {
                                runs: m_line.runs.clone(),
                                bounds: marker_bounds,
                                line_height: m_line.line_height,
                            });

                            // 放置内容的第一行
                            let content_left =
                                content_x + content_indent + first_line.bounds.x0 as f32;
                            let content_bounds = Rect::new(
                                content_left as f64,
                                line_y as f64,
                                (content_left + first_line.bounds.width() as f32) as f64,
                                (line_y + first_line.line_height) as f64,
                            );

                            self.page_context.add_element(VisualElement::TextLine {
                                runs: first_line.runs.clone(),
                                bounds: content_bounds,
                                line_height: first_line.line_height,
                            });

                            // 消费第一行的高度
                            let first_line_height = first_line
                                .line_height
                                .max(m_line.line_height + baseline_offset);
                            self.page_context.consume_height(first_line_height);

                            // 放置剩余的行
                            let settings = self.page_context.settings.clone();
                            let content_height = settings.content_height();
                            let mut current_page_y = self.page_context.current_y;

                            for line in lines.iter().skip(1) {
                                let line_bottom_rel = current_page_y + line.line_height;

                                if line_bottom_rel > content_height && !self.page_context.is_empty()
                                {
                                    self.page_context.finalize_current_page();
                                    self.page_context.start_new_page();
                                    current_page_y = 0.0;
                                }

                                let line_abs_left =
                                    content_x + content_indent + line.bounds.x0 as f32;
                                let line_abs_top = content_y + current_page_y;

                                let bounds = Rect::new(
                                    line_abs_left as f64,
                                    line_abs_top as f64,
                                    (line_abs_left + line.bounds.width() as f32) as f64,
                                    (line_abs_top + line.line_height) as f64,
                                );

                                self.page_context.add_element(VisualElement::TextLine {
                                    runs: line.runs.clone(),
                                    bounds,
                                    line_height: line.line_height,
                                });

                                current_page_y += line.line_height;
                            }

                            // 消费剩余高度和底部边距
                            let consumed =
                                current_page_y - self.page_context.current_y + margin_bottom;
                            self.page_context.consume_height(consumed);
                        }
                    })
                });
            }
            // 内联节点（Text, Strong, Emphasis 等）作为段落处理
            NodeKind::Text { .. }
            | NodeKind::Strong { .. }
            | NodeKind::Emphasis { .. }
            | NodeKind::Link { .. }
            | NodeKind::InlineCode { .. } => {
                // 将单个内联节点包装为段落处理
                let margin_bottom = 0.0;
                let fake_children = vec![node.clone()];
                let segments = collect_inline_segments(&fake_children);
                if segments.is_empty() {
                    if let Some(m_line) = marker_line {
                        let line_y = content_y + self.page_context.current_y;
                        let bounds = Rect::new(
                            marker_left_base as f64,
                            line_y as f64,
                            (marker_left_base + marker_advance) as f64,
                            (line_y + m_line.line_height) as f64,
                        );
                        self.page_context.add_element(VisualElement::TextLine {
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

                        if let (Some(m_line), Some(first_line)) = (marker_line, lines.first()) {
                            let first_height = first_line.line_height.max(m_line.line_height);
                            if first_height > self.page_context.remaining_height()
                                && !self.page_context.is_empty()
                            {
                                self.page_context.start_new_page();
                            }

                            let line_y = content_y + self.page_context.current_y;

                            let marker_baseline =
                                m_line.runs.first().map(|r| r.baseline_y).unwrap_or(0.0);
                            let content_baseline =
                                first_line.runs.first().map(|r| r.baseline_y).unwrap_or(0.0);
                            let baseline_offset = content_baseline - marker_baseline;

                            let marker_y = line_y + baseline_offset;
                            let marker_bounds = Rect::new(
                                marker_left_base as f64,
                                marker_y as f64,
                                (marker_left_base + marker_advance) as f64,
                                (marker_y + m_line.line_height) as f64,
                            );

                            self.page_context.add_element(VisualElement::TextLine {
                                runs: m_line.runs.clone(),
                                bounds: marker_bounds,
                                line_height: m_line.line_height,
                            });

                            let content_left =
                                content_x + content_indent + first_line.bounds.x0 as f32;
                            let content_bounds = Rect::new(
                                content_left as f64,
                                line_y as f64,
                                (content_left + first_line.bounds.width() as f32) as f64,
                                (line_y + first_line.line_height) as f64,
                            );

                            self.page_context.add_element(VisualElement::TextLine {
                                runs: first_line.runs.clone(),
                                bounds: content_bounds,
                                line_height: first_line.line_height,
                            });

                            let first_line_height = first_line
                                .line_height
                                .max(m_line.line_height + baseline_offset);
                            self.page_context.consume_height(first_line_height);

                            let settings = self.page_context.settings.clone();
                            let content_height = settings.content_height();
                            let mut current_page_y = self.page_context.current_y;

                            for line in lines.iter().skip(1) {
                                let line_bottom_rel = current_page_y + line.line_height;

                                if line_bottom_rel > content_height && !self.page_context.is_empty()
                                {
                                    self.page_context.finalize_current_page();
                                    self.page_context.start_new_page();
                                    current_page_y = 0.0;
                                }

                                let line_abs_left =
                                    content_x + content_indent + line.bounds.x0 as f32;
                                let line_abs_top = content_y + current_page_y;

                                let bounds = Rect::new(
                                    line_abs_left as f64,
                                    line_abs_top as f64,
                                    (line_abs_left + line.bounds.width() as f32) as f64,
                                    (line_abs_top + line.line_height) as f64,
                                );

                                self.page_context.add_element(VisualElement::TextLine {
                                    runs: line.runs.clone(),
                                    bounds,
                                    line_height: line.line_height,
                                });

                                current_page_y += line.line_height;
                            }

                            let consumed =
                                current_page_y - self.page_context.current_y + margin_bottom;
                            self.page_context.consume_height(consumed);
                        }
                    })
                });
            }
            _ => {
                // 其他类型节点直接布局
                self.layout_node(node);
            }
        }
    }

    // ─── 带缩进的节点布局 ────────────────────────────────────

    pub(crate) fn layout_node_with_indent(&mut self, node: &Node, style: &crate::ast::Style) {
        match &node.kind {
            NodeKind::Paragraph { children } => {
                self.layout_paragraph(children, &node.style);
            }
            NodeKind::List {
                ordered: child_ordered,
                children,
                start: child_start,
            } => {
                let list_indent = crate::ast::calculate_list_indent(style.font_size_pt);
                self.with_additional_indent(list_indent, |s| {
                    s.layout_list_with_indent(
                        children,
                        *child_ordered,
                        child_start.unwrap_or(1),
                        style,
                    );
                });
            }
            _ => {
                self.layout_node(node);
            }
        }
    }

    // ─── 引用块布局 ─────────────────────────────────────────────

    pub(crate) fn layout_blockquote(&mut self, children: &[Node], style: &crate::ast::Style) {
        let content_x = self.page_context.settings.content_x();
        let content_y = self.page_context.settings.content_y();
        let estimated_height = estimate_children_height(children)
            + 16.0
            + style.margin.top
            + style.padding.top
            + style.padding.bottom
            + style.margin.bottom;

        // 页面破裂检查
        if estimated_height > self.page_context.remaining_height() && !self.page_context.is_empty()
        {
            self.page_context.start_new_page();
        }

        // 通过 box model 处理 margin.top + padding.top
        let (_indent, _available_width, content_start_y, prev_indent) =
            crate::generator::box_model::begin_box(
                &mut self.page_context,
                self.layout_ctx.current_indent,
                style,
            );

        let top = content_y + content_start_y;

        // 引用块左边框
        let border_color = if style.border.left.is_visible() {
            style.border.left.color
        } else {
            Color::new(200, 200, 200)
        };
        // 左边框高度 = estimated_height - margin.top - padding.top - padding.bottom - margin.bottom - 8.0(底部间距)
        let border_height = estimated_height
            - style.margin.top
            - style.padding.top
            - style.padding.bottom
            - style.margin.bottom
            - 8.0;
        let border = VisualElement::Rect {
            rect: Rect::new(
                content_x as f64,
                top as f64,
                (content_x + 4.0) as f64,
                (top + border_height) as f64,
            ),
            style: crate::visual::FillStrokeStyle {
                fill: Some(border_color),
                stroke: None,
            },
        };
        self.page_context.add_element(border);

        // 顶部 8pt 间距
        self.page_context.consume_height(8.0);

        // 引用块内容缩进 24pt
        self.with_indent(24.0, |s| {
            for child in children {
                s.layout_node(child);
            }
        });

        // 底部 8pt 间距
        self.page_context.consume_height(8.0);

        // 收尾：绘制背景+边框，消耗 padding.bottom + margin.bottom
        crate::generator::box_model::end_box(
            &mut self.page_context,
            style,
            content_start_y,
            prev_indent,
        );
    }

    // ─── 容器布局 (div / center) ─────────────────────────────

    /// 布局块级容器（div、center），递归布局子节点。
    pub(crate) fn layout_container(
        &mut self,
        children: &[Node],
        style: &crate::ast::Style,
        centered: bool,
    ) {
        // 先估算高度判断是否需要分页
        let estimated_height = estimate_children_height(children);
        if estimated_height > self.page_context.remaining_height() && !self.page_context.is_empty()
        {
            self.page_context.start_new_page();
        }

        let (_, _, content_start_y, prev_indent) = crate::generator::box_model::begin_box(
            &mut self.page_context,
            self.layout_ctx.current_indent,
            style,
        );

        for child in children {
            if centered {
                let mut child = child.clone();
                child.style.text_align = crate::ast::TextAlign::Center;
                self.layout_node(&child);
            } else {
                self.layout_node(child);
            }
        }

        crate::generator::box_model::end_box(
            &mut self.page_context,
            style,
            content_start_y,
            prev_indent,
        );
    }

    // ─── 分割线布局 ─────────────────────────────────────────────

    pub(crate) fn layout_thematic_break(&mut self, style: &crate::ast::Style) {
        let content_x = self.page_context.settings.content_x();
        let content_width = self.page_context.settings.content_width();
        let content_y = self.page_context.settings.content_y();

        // 计算总高度用于分页判断
        let total_height =
            1.0 + style.margin.top + style.padding.top + style.padding.bottom + style.margin.bottom;

        if total_height > self.page_context.remaining_height() && !self.page_context.is_empty() {
            self.page_context.start_new_page();
        }

        // 通过 box model 处理 margin.top + padding.top
        let (_indent, _available_width, content_start_y, prev_indent) =
            crate::generator::box_model::begin_box(
                &mut self.page_context,
                self.layout_ctx.current_indent,
                style,
            );

        // 绘制分割线（位于当前 y 位置，即 margin.top + padding.top 之后）
        let line_y = content_y + self.page_context.current_y;
        let line = VisualElement::Line {
            start: Point::new(content_x as f64, line_y as f64),
            end: Point::new((content_x + content_width) as f64, line_y as f64),
            style: StrokeStyle {
                color: Color::new(200, 200, 200),
                width: 1.0,
            },
        };
        self.page_context.add_element(line);

        // 消耗分割线自身高度
        self.page_context.consume_height(1.0);

        // 收尾：绘制背景+边框，消耗 padding.bottom + margin.bottom
        crate::generator::box_model::end_box(
            &mut self.page_context,
            style,
            content_start_y,
            prev_indent,
        );
    }

    // ─── 表格布局 ─────────────────────────────────────────────

    pub(crate) fn layout_table(&mut self, node: &Node) {
        use crate::generator::table::{compute_layout_info, generate_rows};

        let content_width = self.page_context.settings.content_width();
        let content_x = self.page_context.settings.content_x();
        let content_y = self.page_context.settings.content_y();

        // 1. 计算表格布局（列宽、行高），不生成视觉元素
        let layout = compute_layout_info(node, content_width);
        if layout.num_rows == 0 || layout.num_cols == 0 {
            return;
        }

        // 通过 box model 处理 margin.top + padding.top
        let (_indent, _available_width, _content_start_y, _prev_indent) =
            crate::generator::box_model::begin_box(
                &mut self.page_context,
                self.layout_ctx.current_indent,
                &node.style,
            );

        // 2. 按行逐页放置
        let mut row_idx = 0;

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
                let shifted =
                    crate::generator::shift_element(element, content_x as f64, page_y as f64);
                self.page_context.add_element(shifted);
            }

            // 5. 消耗高度
            self.page_context.consume_height(chunk_height);
            row_idx = end_idx;

            // 6. 如果还有剩余行，换页
            if row_idx < layout.num_rows {
                self.page_context.start_new_page();
            }
        }

        // 收尾：padding.bottom + margin.bottom
        crate::generator::box_model::end_box_content(&mut self.page_context, &node.style);
    }
}

// ─── 列表标记生成函数 ──────────────────────────────────

/// 生成有序列表标记字符串
fn ordered_marker(style: ListStyleType, number: u32) -> String {
    match style {
        ListStyleType::Decimal => format!("{}.", number),
        ListStyleType::DecimalLeadingZero => format!("{:02}.", number),
        ListStyleType::LowerRoman => format!("{}.", to_roman(number).to_lowercase()),
        ListStyleType::UpperRoman => format!("{}.", to_roman(number)),
        ListStyleType::LowerAlpha => format!("{}.", to_alpha(number).to_lowercase()),
        ListStyleType::UpperAlpha => format!("{}.", to_alpha(number)),
        ListStyleType::None => String::new(),
        _ => format!("{}.", number), // fallback
    }
}

/// 生成无序列表标记字符串
fn unordered_marker(style: ListStyleType) -> String {
    match style {
        ListStyleType::Disc => "•".to_string(),
        ListStyleType::Circle => "○".to_string(),
        ListStyleType::Square => "■".to_string(),
        ListStyleType::None => String::new(),
        _ => "•".to_string(), // fallback
    }
}

/// 将数字转换为罗马数字（1-3999）
fn to_roman(mut num: u32) -> String {
    let values = [1000, 900, 500, 400, 100, 90, 50, 40, 10, 9, 5, 4, 1];
    let numerals = [
        "M", "CM", "D", "CD", "C", "XC", "L", "XL", "X", "IX", "V", "IV", "I",
    ];
    let mut result = String::new();
    for (i, &v) in values.iter().enumerate() {
        while num >= v {
            result.push_str(numerals[i]);
            num -= v;
        }
    }
    result
}

/// 将数字转换为字母序号（1→A, 2→B, ... 26→Z, 27→AA...）
fn to_alpha(num: u32) -> String {
    let mut n = num;
    let mut result = String::new();
    while n > 0 {
        n -= 1;
        result.insert(0, char::from_u32(65 + (n % 26)).unwrap_or('A'));
        n /= 26;
    }
    result
}
