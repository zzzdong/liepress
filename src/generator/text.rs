//! 文本排版辅助函数
//!
//! 提供文本布局、行构建、内联段收集等功能。

use crate::text::{Glyph, TextRun, TextStyle, TextLayout};
use crate::ast::{Node, computed_style_to_text_style};
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
        // run_infos: (color, font_data, font_size, run, first_glyph_x)
        let mut run_infos: Vec<(Color, parley::FontData, f32, parley::layout::Run<'_, Color>, f32)> =
            Vec::new();

        let mut next_run_idx = 0;
        for item in line.items() {
            if let parley::layout::PositionedLayoutItem::GlyphRun(glyph_run) = item {
                let run = glyph_run.run();
                let color = glyph_run.style().brush;
                let font_data = run.font().clone();
                let font_size = run.font_size();

                // 获取第一个字形的绝对 x（后续会减去 min_x 转换为相对偏移）
                let first_glyph_x = glyph_run
                    .positioned_glyphs()
                    .next()
                    .map(|g| g.x)
                    .unwrap_or(0.0);

                run_infos.push((color, font_data, font_size, *run, first_glyph_x));

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
            let (color, font_data, font_size, run, first_glyph_x) = &run_infos[run_idx];

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

            // 使用 clusters 获取准确的文本范围
            // 获取当前行中该 run 的所有 clusters
            let run_clusters: Vec<_> = run.clusters().collect();

            // 计算当前行包含的文本范围
            // 关键：当前行可能只包含 run 的中间部分（当文本跨多行时）
            // 我们需要根据 glyph 的 x 坐标来匹配对应的 clusters
            let first_glyph_abs_x = glyph_data[*start_idx].0.x;
            let last_glyph_abs_x = glyph_data[*end_idx - 1].0.x;

            let mut current_text_start = None;
            let mut current_text_end = None;
            let mut current_x = 0.0;

            for cluster in run_clusters.iter() {
                let cluster_text_range = cluster.text_range();
                let cluster_advance = cluster.advance();

                // 检查这个 cluster 是否在当前行的 glyph 范围内
                // 通过比较 x 坐标来判断
                let cluster_end_x = current_x + cluster_advance;

                // 如果 cluster 的任意部分与当前行的 glyph 范围重叠
                if cluster_end_x > first_glyph_abs_x - 0.01 && current_x < last_glyph_abs_x + 0.01 {
                    if current_text_start.is_none() {
                        current_text_start = Some(cluster_text_range.start);
                    }
                    current_text_end = Some(cluster_text_range.end);
                }

                current_x += cluster_advance;

                // 如果已经超出当前行的范围，停止
                if current_x > last_glyph_abs_x + 0.01 {
                    break;
                }
            }

            let run_text = match (current_text_start, current_text_end) {
                (Some(start), Some(end)) if start < end => {
                    full_text[start..end].to_string()
                }
                _ => String::new(),
            };

            let text_range = match (current_text_start, current_text_end) {
                (Some(start), Some(end)) => start..end,
                _ => 0..0,
            };

            let advance = relative_glyphs.iter().map(|g| g.advance).sum();
            // baseline_x 转换为相对行最左侧的偏移
            let baseline_x = *first_glyph_x - min_x;

            runs.push(TextRun {
                text: run_text,
                text_range,
                font_data: font_data.clone(),
                font_size: *font_size,
                color: *color,
                advance,
                glyphs: relative_glyphs,
                is_rtl: false,
                baseline_x,
                baseline_y,
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
