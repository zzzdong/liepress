//! 表格布局引擎
//!
//! 将表格 AST 节点转换为 VisualElement 列表。
//! 核心算法：启发式列宽分配 + 基于换行的行高计算。
//!
//! 设计分两阶段：
//! 1. compute_layout_info — 纯计算，返回列宽、行高等布局数据
//! 2. generate_rows — 按行区间生成视觉元素，支持跨页分割

use crate::ast::{Node, NodeKind, TextAlign, Style, computed_style_to_text_style};
use crate::generator::text::{collect_inline_segments, build_text_lines_rel};
use crate::text::{
    FONT_CONTEXT, LAYOUT_CONTEXT,
    layout_text_with_contexts, TextStyle, TextAlign as TextAlign2,
};
use crate::visual::{VisualElement, StrokeStyle, FillStrokeStyle};
use vello_cpu::kurbo::{Point, Rect};

// ─── 内部数据结构 ───

/// 单元格测量结果
struct CellMeasure {
    /// 不换行时的完整文本宽度
    ideal_width: f32,
    /// 最宽不可断片段的宽度（最小宽度）
    min_width: f32,
}

// ─── 公开数据结构 ───

/// 预计算的表格布局数据（不含视觉元素）
pub struct TableLayoutInfo {
    /// 行数
    pub num_rows: usize,
    /// 列数
    pub num_cols: usize,
    /// 每列宽度
    pub col_widths: Vec<f32>,
    /// 每行高度
    pub row_heights: Vec<f32>,
    /// 每列对齐方式
    pub row_align: Vec<TextAlign>,
}

// ─── 阶段 1：计算布局信息 ───

/// 计算表格布局信息（列宽 + 行高），不生成视觉元素
pub fn compute_layout_info(node: &Node, content_width: f32) -> TableLayoutInfo {
    // 1. 解析表格结构
    let rows = collect_rows(node);
    let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if num_cols == 0 || rows.is_empty() {
        return TableLayoutInfo {
            num_rows: 0,
            num_cols: 0,
            col_widths: vec![],
            row_heights: vec![],
            row_align: vec![],
        };
    }

    // 2. 获取列对齐信息
    let align = build_alignment(node, num_cols);

    // 3. 测量单元格
    let cell_measures = measure_cells(&rows, num_cols);

    // 4. 计算列宽
    let col_widths = calculate_column_widths(&cell_measures, num_cols, content_width);

    // 5. 计算行高
    let row_heights = calculate_row_heights(&rows, &col_widths, num_cols);

    TableLayoutInfo {
        num_rows: rows.len(),
        num_cols,
        col_widths,
        row_heights,
        row_align: align,
    }
}

// ─── 阶段 2：按行区间生成视觉元素 ───

/// 为 [row_start..row_end) 范围内的行生成视觉元素
///
/// 所有元素的坐标相对于表格内容区左上角（即左缩进 + content_x 后的偏移由调用方处理）。
/// 此函数负责单元格文本、背景填充以及该区间内的边框线。
pub fn generate_rows(
    node: &Node,
    layout: &TableLayoutInfo,
    row_start: usize,
    row_end: usize,
    style: &Style,
) -> Vec<VisualElement> {
    if row_start >= row_end || row_start >= layout.num_rows {
        return vec![];
    }
    let end = row_end.min(layout.num_rows);

    let rows = collect_rows(node);

    // 计算列起始 X 坐标
    let col_starts = compute_column_starts(&layout.col_widths);

    // 计算该行区间内的起始 Y 坐标（相对值，从 0 开始）
    let row_starts = compute_row_starts_range(&layout.row_heights, row_start, end);

    let mut elements = Vec::new();

    // 确定是否包含表头行（第一行为表头）
    let has_header = !rows.is_empty();

    // 背景填充（表头 + 交替行）
    let chunk_table_width: f32 = col_starts.last().copied().unwrap_or(0.0)
        + layout.col_widths.last().copied().unwrap_or(0.0);

    for (chunk_idx, abs_row_idx) in (row_start..end).enumerate() {
        let row_y = row_starts[chunk_idx];
        let row_h = layout.row_heights[abs_row_idx];

        // 表头背景
        if has_header && abs_row_idx == 0 {
            if let Some(bg) = style.table_header_bg {
                elements.push(VisualElement::Rect {
                    rect: Rect::new(0.0, row_y as f64, chunk_table_width as f64, (row_y + row_h) as f64),
                    style: FillStrokeStyle {
                        fill: Some(bg),
                        stroke: None,
                    },
                });
            }
        } else if abs_row_idx > 0 && abs_row_idx % 2 == 0 {
            // 偶数行（非表头）使用交替行背景
            if let Some(bg) = style.table_alt_row_bg {
                elements.push(VisualElement::Rect {
                    rect: Rect::new(0.0, row_y as f64, chunk_table_width as f64, (row_y + row_h) as f64),
                    style: FillStrokeStyle {
                        fill: Some(bg),
                        stroke: None,
                    },
                });
            }
        }
    }

    // 生成单元格文本
    layout_cell_texts(
        &rows,
        &col_starts,
        &layout.col_widths,
        &row_starts,
        &layout.row_heights,
        &layout.row_align,
        layout.num_cols,
        row_start,
        end,
        style,
        &mut elements,
    );

    // 绘制该行区间内的边框
    draw_table_borders_range(
        &col_starts,
        &layout.col_widths,
        &row_starts,
        &layout.row_heights,
        row_start,
        end,
        style,
        &mut elements,
    );

    elements
}

// ─── 解析结构 ───

/// 从表格节点中提取二维单元格节点数组
fn collect_rows(node: &Node) -> Vec<Vec<&Node>> {
    let mut rows = Vec::new();
    if let NodeKind::Table { children, .. } = &node.kind {
        for child in children {
            if let NodeKind::TableRow { children: cells } = &child.kind {
                let row: Vec<&Node> = cells.iter().collect();
                rows.push(row);
            }
        }
    }
    rows
}

/// 构建列对齐数组
fn build_alignment(node: &Node, num_cols: usize) -> Vec<TextAlign> {
    let mut a = match &node.kind {
        NodeKind::Table { align, .. } => align.clone(),
        _ => vec![],
    };
    while a.len() < num_cols {
        a.push(TextAlign::Left);
    }
    a.truncate(num_cols);
    a
}

// ─── 测量单元格 ───

/// 测量所有单元格，返回每行每列的测量结果
fn measure_cells(rows: &[Vec<&Node>], num_cols: usize) -> Vec<Vec<CellMeasure>> {
    let padding_h = 4.0; // 用固定值而非样式，测量阶段尚不知样式
    let mut measures = Vec::with_capacity(rows.len());
    for row in rows {
        let mut row_meas = Vec::with_capacity(num_cols);
        for cell in row.iter().take(num_cols) {
            row_meas.push(measure_cell(cell, padding_h));
        }
        while row_meas.len() < num_cols {
            row_meas.push(CellMeasure {
                ideal_width: padding_h * 2.0,
                min_width: padding_h * 2.0,
            });
        }
        measures.push(row_meas);
    }
    measures
}

/// 测量单个单元格的理想宽度和最小宽度
fn measure_cell(cell: &Node, padding_h: f32) -> CellMeasure {
    let segments = collect_inline_segments_from_cell(cell);
    if segments.is_empty() {
        return CellMeasure {
            ideal_width: padding_h * 2.0,
            min_width: padding_h * 2.0,
        };
    }

    let texts: Vec<(&str, &TextStyle)> = segments
        .iter()
        .map(|(t, s)| (t.as_str(), s))
        .collect();

    let ideal_width = FONT_CONTEXT.with(|font_cx| {
        LAYOUT_CONTEXT.with(|layout_cx| {
            let mut fcx = font_cx.borrow_mut();
            let mut lcx = layout_cx.borrow_mut();
            let layout = layout_text_with_contexts(
                &texts,
                None,
                TextAlign2::Left,
                &mut fcx,
                &mut lcx,
            );
            layout.width() as f32
        })
    });

    let min_width = segments
        .iter()
        .flat_map(|(text, style)| {
            let words = split_words(text);
            words.into_iter().map(move |w| (w, style))
        })
        .filter(|(w, _)| !w.is_empty())
        .map(|(word, style)| {
            FONT_CONTEXT.with(|font_cx| {
                LAYOUT_CONTEXT.with(|layout_cx| {
                    let mut fcx = font_cx.borrow_mut();
                    let mut lcx = layout_cx.borrow_mut();
                    let layout = layout_text_with_contexts(
                        &[(word, style)],
                        None,
                        TextAlign2::Left,
                        &mut fcx,
                        &mut lcx,
                    );
                    layout.width() as f32
                })
            })
        })
        .fold(0.0_f32, f32::max);

    CellMeasure {
        ideal_width: ideal_width + padding_h * 2.0,
        min_width: (min_width + padding_h * 2.0).max(padding_h * 2.0),
    }
}

/// 从单元格节点收集内联段
fn collect_inline_segments_from_cell(cell: &Node) -> Vec<(String, TextStyle)> {
    match &cell.kind {
        NodeKind::Paragraph { children } => collect_inline_segments(children),
        _ => {
            let text = cell.text_content();
            if text.is_empty() {
                vec![]
            } else {
                vec![(text, computed_style_to_text_style(&cell.style))]
            }
        }
    }
}

// ─── 计算列宽 ───

fn calculate_column_widths(
    cell_measures: &[Vec<CellMeasure>],
    num_cols: usize,
    available_width: f32,
) -> Vec<f32> {
    let mut ideal_w = vec![0.0_f32; num_cols];
    let mut min_w = vec![0.0_f32; num_cols];

    for row in cell_measures {
        for (c, m) in row.iter().enumerate().take(num_cols) {
            if m.ideal_width > ideal_w[c] {
                ideal_w[c] = m.ideal_width;
            }
            if m.min_width > min_w[c] {
                min_w[c] = m.min_width;
            }
        }
    }

    let total_ideal: f32 = ideal_w.iter().sum();
    if total_ideal <= available_width {
        return ideal_w;
    }

    let total_min: f32 = min_w.iter().sum();
    if total_min >= available_width {
        return min_w;
    }

    let extra = available_width - total_min;
    let ideal_extra: f32 = ideal_w.iter().zip(min_w.iter()).map(|(i, m)| i - m).sum();

    let mut final_w = Vec::with_capacity(num_cols);
    for c in 0..num_cols {
        let ratio = if ideal_extra > 0.0 {
            (ideal_w[c] - min_w[c]) / ideal_extra
        } else {
            1.0 / num_cols as f32
        };
        final_w.push(min_w[c] + extra * ratio);
    }

    final_w
}

// ─── 计算行高 ───

fn calculate_row_heights(
    rows: &[Vec<&Node>],
    col_widths: &[f32],
    num_cols: usize,
) -> Vec<f32> {
    let padding_h = 4.0;
    let padding_v = 2.0;
    let mut heights = Vec::with_capacity(rows.len());

    for row in rows {
        let mut max_height = 0.0_f32;

        for (c, cell) in row.iter().enumerate().take(num_cols) {
            let cell_width = col_widths[c] - padding_h * 2.0;
            if cell_width <= 0.0 {
                continue;
            }

            let segments = collect_inline_segments_from_cell(cell);
            if segments.is_empty() {
                max_height = max_height.max(padding_v * 2.0);
                continue;
            }

            let texts: Vec<(&str, &TextStyle)> = segments
                .iter()
                .map(|(t, s)| (t.as_str(), s))
                .collect();

            let height = FONT_CONTEXT.with(|font_cx| {
                LAYOUT_CONTEXT.with(|layout_cx| {
                    let mut fcx = font_cx.borrow_mut();
                    let mut lcx = layout_cx.borrow_mut();
                    let layout = layout_text_with_contexts(
                        &texts,
                        Some(cell_width as f64),
                        TextAlign2::Left,
                        &mut fcx,
                        &mut lcx,
                    );

                    let mut h = 0.0_f32;
                    for line in layout.lines() {
                        h += line.metrics().line_height;
                    }
                    h
                })
            });

            max_height = max_height.max(height + padding_v * 2.0);
        }

        heights.push(max_height.max(padding_v * 2.0));
    }

    heights
}

// ─── 累积坐标 ───

/// 计算每列起始 X 坐标
fn compute_column_starts(col_widths: &[f32]) -> Vec<f32> {
    let mut starts = Vec::with_capacity(col_widths.len());
    let mut x = 0.0_f32;
    for w in col_widths {
        starts.push(x);
        x += w;
    }
    starts
}

/// 计算 [row_start..row_end) 范围内的行起始 Y 坐标（相对值）
fn compute_row_starts_range(
    row_heights: &[f32],
    row_start: usize,
    row_end: usize,
) -> Vec<f32> {
    let mut starts = Vec::with_capacity(row_end - row_start);
    let mut y = 0.0_f32;
    for i in row_start..row_end {
        starts.push(y);
        y += row_heights[i];
    }
    starts
}

// ─── 生成单元格文本 ───

fn layout_cell_texts(
    rows: &[Vec<&Node>],
    col_starts: &[f32],
    col_widths: &[f32],
    row_starts: &[f32],
    _row_heights: &[f32],
    align: &[TextAlign],
    num_cols: usize,
    row_start: usize,
    row_end: usize,
    style: &Style,
    elements: &mut Vec<VisualElement>,
) {
    let padding_h = style.table_cell_padding_h_pt;
    let padding_v = style.table_cell_padding_v_pt;

    for (chunk_idx, abs_row_idx) in (row_start..row_end).enumerate() {
        if abs_row_idx >= rows.len() {
            break;
        }
        let row = &rows[abs_row_idx];
        let cell_y = row_starts[chunk_idx];

        for (c, cell) in row.iter().enumerate().take(num_cols) {
            let cell_x = col_starts[c];
            let cell_w = col_widths[c];

            let segments = collect_inline_segments_from_cell(cell);
            if segments.is_empty() {
                continue;
            }

            let text_align = map_text_align(align.get(c).copied().unwrap_or(TextAlign::Left));
            let cell_text_width = cell_w - padding_h * 2.0;
            if cell_text_width <= 0.0 {
                continue;
            }

            let texts: Vec<(&str, &TextStyle)> = segments
                .iter()
                .map(|(t, s)| (t.as_str(), s))
                .collect();

            FONT_CONTEXT.with(|font_cx| {
                LAYOUT_CONTEXT.with(|layout_cx| {
                    let mut fcx = font_cx.borrow_mut();
                    let mut lcx = layout_cx.borrow_mut();

                    let total_text: String = segments.iter().map(|(t, _)| t.as_str()).collect();
                    let layout = layout_text_with_contexts(
                        &texts,
                        Some(cell_text_width as f64),
                        text_align,
                        &mut fcx,
                        &mut lcx,
                    );

                    let lines_rel = build_text_lines_rel(&layout, &total_text);
                    for line_rel in &lines_rel {
                        let line_abs_x = cell_x + padding_h + line_rel.min_x;
                        let line_abs_y = cell_y + padding_v + line_rel.row_top_rel;
                        let bounds = Rect::new(
                            line_abs_x as f64,
                            line_abs_y as f64,
                            (line_abs_x + line_rel.width) as f64,
                            (line_abs_y + line_rel.line_height) as f64,
                        );

                        elements.push(VisualElement::TextLine {
                            runs: line_rel.runs.clone(),
                            bounds,
                            line_height: line_rel.line_height,
                        });
                    }
                })
            });
        }
    }
}

// ─── 绘制区间边框 ───

/// 绘制 [row_start..row_end) 行区间内的边框
fn draw_table_borders_range(
    col_starts: &[f32],
    col_widths: &[f32],
    row_starts: &[f32],
    row_heights: &[f32],
    row_start: usize,
    row_end: usize,
    style: &Style,
    elements: &mut Vec<VisualElement>,
) {
    let border = StrokeStyle {
        color: style.table_border_color,
        width: style.table_border_width_pt as f64,
    };

    // 该区间的总宽度
    let table_width: f32 = col_starts.last().copied().unwrap_or(0.0)
        + col_widths.last().copied().unwrap_or(0.0);

    // 该区间的高度范围
    let chunk_first_y = row_starts.first().copied().unwrap_or(0.0);
    let chunk_last_y = row_starts.last().copied().unwrap_or(0.0)
        + if let (Some(&last_h), Some(_last_start)) = (row_heights.get(row_end - 1), row_starts.last()) {
            last_h
        } else {
            0.0
        };

    // 顶部边框（仅在 row_start == 0 时或该区间第一行的顶部）
    elements.push(VisualElement::Line {
        start: Point::new(0.0, chunk_first_y as f64),
        end: Point::new(table_width as f64, chunk_first_y as f64),
        style: border.clone(),
    });

    // 每行底部边框
    for (chunk_idx, abs_row_idx) in (row_start..row_end).enumerate() {
        let y = row_starts[chunk_idx] + row_heights[abs_row_idx];
        elements.push(VisualElement::Line {
            start: Point::new(0.0, y as f64),
            end: Point::new(table_width as f64, y as f64),
            style: border.clone(),
        });
    }

    // 垂直边框（每列左右）
    for c in 0..=col_widths.len() {
        let x = if c == 0 {
            0.0
        } else if c < col_starts.len() {
            col_starts[c]
        } else {
            col_starts.last().copied().unwrap_or(0.0)
                + col_widths.last().copied().unwrap_or(0.0)
        };

        elements.push(VisualElement::Line {
            start: Point::new(x as f64, chunk_first_y as f64),
            end: Point::new(x as f64, chunk_last_y as f64),
            style: border.clone(),
        });
    }
}

// ─── 辅助函数 ───

fn map_text_align(a: TextAlign) -> TextAlign2 {
    match a {
        TextAlign::Left => TextAlign2::Left,
        TextAlign::Center => TextAlign2::Center,
        TextAlign::Right => TextAlign2::Right,
        TextAlign::Justify => TextAlign2::Left,
    }
}

fn split_words(text: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut start = 0;
    let mut in_word = false;

    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if in_word {
                words.push(&text[start..i]);
                in_word = false;
            }
        } else if is_cjk(c) {
            if in_word {
                words.push(&text[start..i]);
                in_word = false;
            }
            words.push(&text[i..i + c.len_utf8()]);
        } else {
            if !in_word {
                start = i;
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(&text[start..]);
    }
    words
}

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}'
        | '\u{3400}'..='\u{4DBF}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{2F800}'..='\u{2FA1F}'
    )
}