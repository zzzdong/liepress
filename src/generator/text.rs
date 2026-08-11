//! 文本排版辅助函数
//!
//! 提供文本布局、行构建、内联段收集等功能。

use crate::ast::{Node, NodeKind, computed_style_to_text_style};
use crate::text::TextStyle;

// ─── 文本处理函数 ─────────────────────────────────────────

/// 收集内联子节点的文本段
///
/// 递归展开容器节点（Span、Strong、Emphasis、Link、Delete），
/// 使得每个 Text 片段使用自己的样式。
pub fn collect_inline_segments(children: &[Node]) -> Vec<(String, TextStyle)> {
    let mut segments = Vec::new();
    for child in children {
        match &child.kind {
            NodeKind::Span { children: inner }
            | NodeKind::Strong { children: inner }
            | NodeKind::Emphasis { children: inner }
            | NodeKind::Link {
                children: inner, ..
            }
            | NodeKind::Delete { children: inner }
            | NodeKind::Subscript { children: inner }
            | NodeKind::Superscript { children: inner } => {
                // 递归展开容器节点
                segments.extend(collect_inline_segments(inner));
            }
            NodeKind::Text { text } => {
                if !text.is_empty() {
                    segments.push((text.clone(), computed_style_to_text_style(&child.style)));
                }
            }
            NodeKind::InlineCode { code } => {
                if !code.is_empty() {
                    segments.push((code.clone(), computed_style_to_text_style(&child.style)));
                }
            }
            NodeKind::LineBreak => {
                // <br> 插入换行符，让 parley 在排版时换行
                segments.push(("\n".to_string(), computed_style_to_text_style(&child.style)));
            }
            _ => {
                let text = child.kind.text_content();
                if !text.is_empty() {
                    segments.push((text, computed_style_to_text_style(&child.style)));
                }
            }
        }
    }
    segments
}

/// 从 segments 生成 URL 映射，并标注到 TextLine 的 runs 上。
///
/// 使用顺序匹配：runs 在行中的顺序与 segments 一致。通过跟踪每个 segment
/// 已消费的 Unicode 字符数，正确处理多字节字符（如 emoji）和自动换行场景。
pub fn annotate_runs_with_urls(
    lines: &mut [crate::text::TextLine],
    _total_text: &str,
    segments: &[(String, TextStyle)],
) {
    // 现在 run.text 已经在 extract_lines_from_parley 中正确设置
    // 这个函数只负责添加 URL 和 decoration
    let mut seg_idx = 0;
    let mut seg_char_consumed = 0_usize;
    let seg_char_counts: Vec<usize> = segments.iter().map(|(s, _)| s.chars().count()).collect();

    for line in lines.iter_mut() {
        for run in line.runs.iter_mut() {
            while seg_idx < seg_char_counts.len() && seg_char_consumed >= seg_char_counts[seg_idx] {
                seg_idx += 1;
                seg_char_consumed = 0;
            }

            if seg_idx < segments.len() {
                let (_seg_text, seg_style) = &segments[seg_idx];
                // 只添加 URL、decoration、背景色，不覆盖文本
                run.url = seg_style.url.clone();
                run.decoration = seg_style.decoration;
                run.background_color = seg_style.background_color;
                seg_char_consumed += run.text.chars().count();
            }
        }
    }
}

/// 估算子节点的总高度
pub fn estimate_children_height(children: &[Node]) -> f32 {
    let mut height = 0.0;
    for child in children {
        match &child.kind {
            NodeKind::List {
                children: items, ..
            } => {
                // 列表：每个列表项一行高度 + 列表本身行高作为间距
                let item_count = items.len();
                height += item_count as f32 * child.style.line_height_pt;
            }
            NodeKind::Blockquote { children: inner } => {
                // 引用块：递归估算内容高度（padding 由调用方负责）
                height += estimate_children_height(inner);
            }
            _ => {
                height += child.style.line_height_pt;
            }
        }
    }
    height
}
