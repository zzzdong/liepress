//! 输出层公共辅助：各后端共享的块测量与样式工具。
//!
//! 从 `pdf.rs` 提取，供 PDF / SVG / PNG 等后端复用，避免重复实现
//! 块高度计算、样式映射等逻辑。

use crate::color::Color;
use crate::document::layout::{Block, BlockKind, TableRow};
use crate::document::text::{TextDecoration, TextLine, TextStyle};
use crate::document::types::page::PageSettings;
use crate::document::types::{ResolvedStyle, TextAlign as LayoutAlign};

/// 引用块左侧竖条宽度（pt）。
pub const BQ_BAR_WIDTH: f64 = 2.0;
/// 引用块左内边距（pt），竖条与文本之间的水平留白。
pub const BQ_PAD_X: f64 = 8.0;
/// 引用块上下内边距（pt）。文本在引用块内垂直居中，此值即上下均分留白的一半。
pub const BQ_PAD_Y: f64 = 6.0;

/// 块高度（含上下外边距）。
///
/// 使不同元素之间产生垂直间距。容器块（List/Blockquote/Document 等）先累加
/// 子块高度（子块已含自身 margin），再加上容器自身的 margin。
pub fn block_height(block: &Block, settings: &PageSettings, x: f64) -> f64 {
    let style = &block.style;
    let base = match &block.kind {
        BlockKind::Heading { children, .. } => {
            children.iter().map(|c| block_height(c, settings, x)).sum()
        }
        BlockKind::Paragraph { lines } => (lines.len().max(1) as f64) * style.line_height_pt as f64,
        BlockKind::CodeBlock { lines, .. } => {
            (lines.len().max(1) as f64) * style.line_height_pt as f64 + 8.0
        }
        BlockKind::ThematicBreak => 4.0,
        BlockKind::Image(img) => {
            if img.size.1 > 0.0 {
                img.size.1
            } else {
                120.0
            }
        }
        BlockKind::Blockquote { children } => {
            blockquote_content_height(children, settings, x + BQ_BAR_WIDTH + BQ_PAD_X)
                + 2.0 * BQ_PAD_Y
        }
        BlockKind::List { children, .. } => {
            children.iter().map(|c| block_height(c, settings, x)).sum()
        }
        BlockKind::ListItem { marker, children } => children
            .iter()
            .map(|c| block_height(c, settings, x + list_item_indent(marker, style)))
            .sum::<f64>()
            .max(style.line_height_pt as f64),
        BlockKind::TaskListItem {
            marker, children, ..
        } => children
            .iter()
            .map(|c| block_height(c, settings, x + list_item_indent(marker, style)))
            .sum::<f64>()
            .max(style.line_height_pt as f64),
        BlockKind::Container { children, .. } => {
            children.iter().map(|c| block_height(c, settings, x)).sum()
        }
        BlockKind::DefinitionList { items } => items
            .iter()
            .map(|item| {
                item.term
                    .iter()
                    .map(|c| block_height(c, settings, x))
                    .sum::<f64>()
                    + item
                        .definition
                        .iter()
                        .map(|c| block_height(c, settings, x))
                        .sum::<f64>()
            })
            .sum::<f64>(),
        BlockKind::FootnoteDef { children, .. } => {
            children.iter().map(|c| block_height(c, settings, x)).sum()
        }
        BlockKind::Table { row_heights, .. } => {
            if row_heights.is_empty() {
                (style.line_height_pt as f64).max(8.0)
            } else {
                row_heights.iter().sum()
            }
        }
        BlockKind::TableRow { cells } => cells
            .iter()
            .map(|c| measure_block_recursive(&c.children, settings, x))
            .sum::<f64>()
            .max(18.0),
        BlockKind::TableCell { children } => {
            children.iter().map(|c| block_height(c, settings, x)).sum()
        }
        BlockKind::Text { .. } => style.line_height_pt as f64,
        BlockKind::InlineCode { .. } => style.line_height_pt as f64,
        BlockKind::Link { children, .. } => {
            if children.is_empty() {
                style.line_height_pt as f64
            } else {
                children.iter().map(|c| block_height(c, settings, x)).sum()
            }
        }
        BlockKind::LineBreak => style.line_height_pt as f64 / 2.0,
        BlockKind::Document { children, .. } => {
            children.iter().map(|c| block_height(c, settings, x)).sum()
        }
    };
    base + style.margin_top as f64 + style.margin_bottom as f64
}

/// 引用块内容高度：子块高度之和，但**扣掉子块自身的上下外边距**，
/// 使子段落的 margin 不外溢撑开引用块（否则竖条会比文本高出一个段落 margin）。
pub fn blockquote_content_height(children: &[Block], settings: &PageSettings, x: f64) -> f64 {
    let mut h = 0.0_f64;
    for c in children {
        h += block_height(c, settings, x);
        h -= c.style.margin_top as f64;
        h -= c.style.margin_bottom as f64;
    }
    h.max(0.0)
}

/// 递归测量子块总高度（供 TableRow 等使用）。
fn measure_block_recursive(children: &[Block], settings: &PageSettings, x: f64) -> f64 {
    children.iter().map(|c| block_height(c, settings, x)).sum()
}

/// 一条表格边框线段（起终点 + 颜色 + 宽度），供各后端用自身图元绘制。
///
/// 坐标为文档内容区坐标系（pt），与块绘制坐标一致。
#[derive(Clone, Copy)]
pub struct TableBorderSegment {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub color: Color,
    pub width: f64,
}

/// 计算一张表格需要绘制的所有边框线（外框 + 列分隔竖线 + 行分隔横线）。
///
/// 这是 PDF / SVG / PNG 共用的几何逻辑，三个后端据此用各自图元画线，
/// 避免重复实现导致（如 PNG 缺列分隔线）的不一致。
///
/// - 外框：整表四边
/// - 列分隔：每列交界处竖线（`ci + 1 < ncols`），贯穿整表高度
/// - 行分隔：表头底边 + 每 body 行之间横线
///
/// `rows`/`col_widths`/`row_heights` 与 [`BlockKind::Table`] 布局数据一致。
/// `y` 为表格顶边。返回线段已按「先外框、再横线、后竖线」排序，便于后端
/// 决定绘制顺序（竖线覆盖横线，横线覆盖外框）。
pub fn table_border_segments(
    rows: &[TableRow],
    col_widths: &[f64],
    row_heights: &[f64],
    style: &ResolvedStyle,
    x: f64,
    y: f64,
) -> Vec<TableBorderSegment> {
    let mut segs = Vec::new();
    if rows.is_empty() || col_widths.is_empty() {
        return segs;
    }
    let ncols = col_widths.len();
    let content_w: f64 = col_widths.iter().sum();
    let border = style.table_border_color;
    let border_w = style.table_border_width_pt as f64;
    let row_h_at = |i: usize| -> f64 {
        row_heights
            .get(i)
            .copied()
            .unwrap_or((style.line_height_pt as f64).max(8.0))
            .max(8.0)
    };
    let total_h: f64 = (0..rows.len()).map(row_h_at).sum();
    let top = y;
    let bottom = y + total_h;

    // 外框四边
    segs.push(TableBorderSegment {
        x1: x,
        y1: top,
        x2: x + content_w,
        y2: top,
        color: border,
        width: border_w,
    });
    segs.push(TableBorderSegment {
        x1: x,
        y1: bottom,
        x2: x + content_w,
        y2: bottom,
        color: border,
        width: border_w,
    });
    segs.push(TableBorderSegment {
        x1: x,
        y1: top,
        x2: x,
        y2: bottom,
        color: border,
        width: border_w,
    });
    segs.push(TableBorderSegment {
        x1: x + content_w,
        y1: top,
        x2: x + content_w,
        y2: bottom,
        color: border,
        width: border_w,
    });

    // 列分隔竖线（每列交界处，贯穿整表高度）
    let mut sep = x;
    for (ci, cw) in col_widths.iter().enumerate() {
        sep += cw;
        if ci + 1 < ncols {
            segs.push(TableBorderSegment {
                x1: sep,
                y1: top,
                x2: sep,
                y2: bottom,
                color: border,
                width: border_w,
            });
        }
    }

    // 行分隔横线：表头底边 + 每 body 行之间
    let mut ry = y;
    for (ri, _row) in rows.iter().enumerate() {
        ry += row_h_at(ri);
        if ri + 1 < rows.len() {
            // 表头底边（ri==0）与 body 行分隔统一处理
            segs.push(TableBorderSegment {
                x1: x,
                y1: ry,
                x2: x + content_w,
                y2: ry,
                color: border,
                width: border_w,
            });
        }
    }

    // 排序：外框(0) → 横线(1) → 竖线(2)，让竖线压住横线/外框交点
    segs.sort_by_key(|s| {
        let is_horizontal = (s.y1 - s.y2).abs() < 1e-9;
        let is_vertical = (s.x1 - s.x2).abs() < 1e-9;
        if !is_horizontal && !is_vertical {
            0 // 外框（正常都水平/垂直，兜底）
        } else if is_vertical {
            2
        } else {
            1
        }
    });
    segs
}

/// 表格行高（pt）：优先用预计算 `row_heights`，缺省退回行高。
pub fn table_row_height(style: &ResolvedStyle, row_heights: &[f64], idx: usize) -> f64 {
    row_heights
        .get(idx)
        .copied()
        .unwrap_or((style.line_height_pt as f64).max(8.0))
        .max(8.0)
}

/// 标题字号（pt）。
pub fn heading_font_size(level: u8) -> f32 {
    match level {
        1 => 22.0,
        2 => 18.0,
        3 => 15.0,
        4 => 13.0,
        5 => 12.0,
        _ => 11.0,
    }
}

/// 构造 [`crate::document::text::TextStyle`]（用于 layout_text）。
pub fn text_style(color: Color, family: &str, size: f32, weight: &str, style: &str) -> TextStyle {
    TextStyle {
        color,
        font_family: vec![family.to_string()],
        font_size: size as f64,
        font_weight: weight.to_string(),
        font_style: style.to_string(),
        align: LayoutAlign::Left,
        url: None,
        decoration: TextDecoration::None,
        baseline_shift: 0.0,
        background_color: None,
    }
}

/// 从 document 层投影的 [`ResolvedStyle`] 构造排版用的 [`TextStyle`]。
pub fn text_style_from_resolved(style: &ResolvedStyle) -> TextStyle {
    TextStyle {
        color: style.color,
        font_family: style.font_family.clone(),
        font_size: style.font_size_pt as f64,
        font_weight: if style.font_weight_bold {
            "bold".to_string()
        } else {
            "normal".to_string()
        },
        font_style: if style.font_style_italic {
            "italic".to_string()
        } else {
            "normal".to_string()
        },
        align: LayoutAlign::Left,
        url: None,
        decoration: style.text_decoration,
        baseline_shift: 0.0,
        background_color: None,
    }
}

/// 把标题文本行套用标题字号/颜色（from_ast 产出的 Paragraph 行是正文样式）。
pub fn apply_heading_style(lines: &[TextLine], size: f32, color: Color) -> Vec<TextLine> {
    lines
        .iter()
        .map(|line| {
            let mut nl = line.clone();
            for r in nl.runs.iter_mut() {
                r.font_size = size;
                r.color = color;
            }
            nl
        })
        .collect()
}

/// 列表缩进相对字号的步长系数（em）。
///
/// 采用业界通用标准（Typst / LaTeX / Word）：列表缩进使用**绝对间距**，
/// 而非依赖空格个数或 marker 字形测量宽度，保证任何字体、字号下视觉一致，
/// 且多级嵌套每层均匀递增该固定步长。步长 = `font_size × 1.5`（1.5em，
/// 对应 12pt 字号下约 18pt 的通用一级缩进）。
pub const LIST_INDENT_STEP_RATIO: f32 = 1.5;

/// 列表项内容区相对 marker 的缩进宽度（pt）。
///
/// 返回固定步长 `font_size × LIST_INDENT_STEP_RATIO`（1.5em），不随字体字形
/// 波动。marker 由后端在缩进槽**左缘单独绘制**（见各 output 后端），正文从
/// `x + indent` 开始，因此多级列表每层递增固定步长、marker 均顶到各自层级左缘。
pub fn list_item_indent(_marker: &str, style: &ResolvedStyle) -> f64 {
    (style.font_size_pt * LIST_INDENT_STEP_RATIO) as f64
}
