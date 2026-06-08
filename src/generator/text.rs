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
            | NodeKind::Delete { children: inner } => {
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
                let (seg_text, seg_style) = &segments[seg_idx];
                let glyph_count = run.text_range.len();

                let mut run_text = String::with_capacity(glyph_count);
                let mut chars_taken = 0_usize;
                for (ci, ch) in seg_text.chars().enumerate() {
                    if ci < seg_char_consumed {
                        continue;
                    }
                    if chars_taken >= glyph_count {
                        break;
                    }
                    run_text.push(ch);
                    chars_taken += 1;
                }
                run.text = run_text;
                run.url = seg_style.url.clone();
                seg_char_consumed += chars_taken;
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
