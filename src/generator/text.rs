//! 文本排版辅助函数
//!
//! 提供文本布局、行构建、内联段收集等功能。

use crate::ast::{Node, computed_style_to_text_style};
use crate::text::{Glyph, TextLayout, TextRun, TextStyle};
use crate::visual::Color;

// ─── 中间结构：相对段落原点的行数据 ────────────────────────

/// 中间结构：相对段落原点的行数据
pub struct TextLineRel {
    pub runs: Vec<TextRun>,
    pub min_x: f32,
    pub width: f32,
    pub line_height: f32,
    pub row_top_rel: f32,
}

/// 字形原始数据（相对 layout 原点的坐标）
pub struct GlyphRaw {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub advance: f32,
}

// ─── 文本处理函数 ─────────────────────────────────────────

/// 收集内联子节点的文本段
pub fn collect_inline_segments(children: &[Node]) -> Vec<(String, TextStyle)> {
    let mut segments = Vec::new();
    for child in children {
        let text = child.kind.text_content();
        if !text.is_empty() {
            segments.push((text, computed_style_to_text_style(&child.style)));
        }
    }
    segments
}

/// 从 segments 生成文本范围到 URL 的映射，并标注到 TextLineRel 的 runs 上。
///
/// 通过匹配 run 的颜色和位置查找对应 segment 的 URL 和文本，将其设置到 run 的 `url` 字段。
/// 用于在文本布局完成后，将超链接元数据回填到渲染单元中。
/// 
/// 使用 run.text_range 来从 total_text 中提取文本，并匹配对应的 segment 获取 URL。
pub fn annotate_runs_with_urls(
    lines: &mut [TextLineRel],
    total_text: &str,
    segments: &[(String, TextStyle)],
) {
    // 构建 segment 的累积范围映射
    // segment_ranges: (start_byte, end_byte, segment_index)
    let mut segment_ranges: Vec<(usize, usize, usize)> = Vec::new();
    let mut current_pos = 0_usize;
    for (i, (text, _)) in segments.iter().enumerate() {
        let start = current_pos;
        let end = current_pos + text.len();
        segment_ranges.push((start, end, i));
        current_pos = end;
    }

    // 为每个 run 填充文本和 URL
    for line in lines.iter_mut() {
        for run in line.runs.iter_mut() {
            let range = &run.text_range;
            
            // 从 total_text 中提取该 run 的文本
            if range.start < total_text.len() && range.end <= total_text.len() {
                run.text = total_text[range.clone()].to_string();
            }

            // 找到包含该 run 的 segment
            // 使用 run 的范围中点来匹配 segment
            let mid_point = (range.start + range.end) / 2;
            for (seg_start, seg_end, seg_idx) in &segment_ranges {
                if mid_point >= *seg_start && mid_point < *seg_end {
                    // 找到匹配的 segment
                    let (_, style) = &segments[*seg_idx];
                    run.url = style.url.clone();
                    break;
                }
            }
        }
    }
}

/// 估算子节点的总高度
pub fn estimate_children_height(children: &[Node]) -> f32 {
    let mut height = 0.0;
    for child in children {
        height += child.style.line_height_pt;
    }
    height
}

/// 从 TextLayout 提取行列表（相对坐标）
///
/// 坐标系设计：
/// - parley 的 `positioned_glyphs()` 返回的坐标相对 layout 原点（左上角）
/// - 本函数将字形坐标转换为相对**行左上角**的偏移量：
///   - `glyph.x -= min_x`（`min_x` 为该行所有字形的最小 x）
///   - `glyph.y -= row_top_rel`（`row_top_rel` 通过累加前序行的 `line_height` 得到）
///   - `baseline_y -= row_top_rel`（变为相对行顶的偏移）
/// - 行的垂直位置完全由累加 `line_height` 决定，不依赖 baseline/ascent
pub fn build_text_lines_rel(layout: &TextLayout, full_text: &str) -> Vec<TextLineRel> {
    let mut lines = Vec::new();
    let mut row_top_rel = 0.0_f32;

    for line in layout.lines() {
        let metrics = line.metrics();
        let line_height = metrics.line_height;
        let baseline_y = metrics.baseline - row_top_rel; // 行基线 Y 坐标（相对行顶的偏移）

        // 收集该行所有字形及所属 run 索引
        let mut glyph_data: Vec<(GlyphRaw, usize)> = Vec::new();
        // 收集当前行的所有 glyph 和 run 信息
        // run_infos: (color, font_data, font_size, run, first_glyph_x, text_range)
        let mut run_infos: Vec<(
            Color,
            parley::FontData,
            f32,
            parley::layout::Run<'_, Color>,
            f32,
            std::ops::Range<usize>,
        )> = Vec::new();

        let mut next_run_idx = 0;
        for item in line.items() {
            if let parley::layout::PositionedLayoutItem::GlyphRun(glyph_run) = item {
                let run = glyph_run.run();
                let color = glyph_run.style().brush;
                let font_data = run.font().clone();
                let font_size = run.font_size();

                // 使用 run.text_range() 获取该 run 的文本范围
                let text_range = run.text_range();

                // 获取第一个字形的绝对 x（后续会减去 min_x 转换为相对偏移）
                let first_glyph_x = glyph_run
                    .positioned_glyphs()
                    .next()
                    .map(|g| g.x)
                    .unwrap_or(0.0);

                run_infos.push((color, font_data, font_size, *run, first_glyph_x, text_range));

                for g in glyph_run.positioned_glyphs() {
                    glyph_data.push((
                        GlyphRaw {
                            id: g.id,
                            x: g.x,
                            y: g.y,
                            advance: g.advance,
                        },
                        next_run_idx,
                    ));
                }
                next_run_idx += 1;
            }
            // InlineBox 忽略
        }

        if glyph_data.is_empty() {
            row_top_rel += line_height;
            continue;
        }

        // 计算行内最小 x 和最大 x（用于相对坐标转换）
        let min_x = glyph_data
            .iter()
            .map(|(g, _)| g.x)
            .fold(f32::INFINITY, f32::min);
        let max_x = glyph_data
            .iter()
            .map(|(g, _)| g.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let width = max_x - min_x;

        // 按 run 分组，构建 TextRun
        let mut runs = Vec::new();

        // 收集每个 run 的 glyph 索引范围
        let mut run_glyph_ranges: Vec<(usize, usize)> = Vec::new(); // (start_idx, end_idx)
        let mut current_start = 0;
        let mut last_run_idx = glyph_data[0].1;

        for (i, (_, run_idx)) in glyph_data.iter().enumerate() {
            if *run_idx != last_run_idx {
                run_glyph_ranges.push((current_start, i));
                current_start = i;
                last_run_idx = *run_idx;
            }
        }
        run_glyph_ranges.push((current_start, glyph_data.len()));

        // 按 run 分组构建 TextRun

        for (start_idx, end_idx) in run_glyph_ranges.iter() {
            let run_idx = glyph_data[*start_idx].1;
            let (color, font_data, font_size, _run, first_glyph_x, text_range) =
                &run_infos[run_idx];

            // 提取当前行的 glyphs 并转换为相对坐标
            let relative_glyphs: Vec<Glyph> = glyph_data[*start_idx..*end_idx]
                .iter()
                .map(|(g, _)| Glyph {
                    id: g.id,
                    x: g.x - min_x,
                    y: g.y - row_top_rel,
                    advance: g.advance,
                })
                .collect();

            // 注意：run_text 暂时留空，由 annotate_runs_with_urls 根据颜色匹配填充
            // 这是为了避免使用错误的 text_range 截取 Unicode 字符串导致 panic
            let advance = relative_glyphs.iter().map(|g| g.advance).sum();
            // baseline_x 转换为相对行最左侧的偏移
            let baseline_x = *first_glyph_x - min_x;

            runs.push(TextRun {
                text: String::new(), // 暂时留空
                text_range: text_range.clone(),
                font_data: font_data.clone(),
                font_size: *font_size,
                color: *color,
                advance,
                glyphs: relative_glyphs,
                is_rtl: false,
                baseline_x,
                baseline_y,
                url: None,
            });
        }

        lines.push(TextLineRel {
            runs,
            min_x,
            width,
            line_height,
            row_top_rel,
        });

        row_top_rel += line_height;
    }

    lines
}
