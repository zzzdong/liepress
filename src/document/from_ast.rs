//! AST → Skeleton 转换（方案 S1）。
//!
//! 将 [`crate::ast::Node`] 树转换为 [`crate::document::skeleton::DocumentSkeleton`]。
//! 文本行（[`crate::text::TextLine`]）在此阶段通过复用 text/generator 的排版函数
//! 进行排版，并投影为 [`crate::document::types::DocTextLine`]。
//!
//! 本阶段不做分页与绝对坐标（S2）。
//!
//! ## 与方案/旧管线不一致（记录于 docs/refactor-log.md）
//! - 行内语义节点（Strong/Emphasis/Delete/Sub/Super）不生成独立 SkeletonBlock，
//!   其语义由排版后的 [`crate::document::types::DocTextRun`] 样式承载（与旧管线一致）。
//! - `<center>`/`<div>`/`<span>` 统一映射到 `BlockKind::Container`（center 的居中
//!   alignment 体现在该块 `ResolvedStyle.text_align`，未额外标记）。
//! - 列表项 marker（有序序号 / 无序圆点 / 任务复选框）在 S1 已注入（`BlockKind::ListItem` /
//!   `TaskListItem` 的 `marker` 字段，方案 §3.6），由 `from_ast` 在 `List` 边界计算。
//! - 块级 `Image` 在 S1 不加载字节（data 为空、pixel_size=(0,0)）：AST `Image` 仅含 `src`
//!   路径而无字节，真实字节由渲染后端（PDF/DOCX/HTML）按 `src` 加载（方案 §3.5.1）。
//! - 可用宽度 S1 取整页内容宽度 `PageSettings::content_width()`，box model 缩进/边框
//!   占用在 S2 处理。

use crate::ast::{Node, NodeKind, computed_style_to_text_style};
use crate::document::skeleton::{BlockKind, DocumentSkeleton, SkeletonBlock, TableCell, TableRow};
use crate::document::types::page::PageSettings;
use crate::document::types::{DocImage, DocTextLine, ObjectFit, ResolvedStyle, TextAlign as DocTextAlign};
use crate::text::{FONT_CONTEXT, LAYOUT_CONTEXT, TextStyle, TextLine};

/// 将带样式的 AST 根节点转换为文档 Skeleton（不分页的源 IR）。
///
/// `settings` 用于计算内联文本排版的可用宽度（S1 取整页内容宽度）。
/// 分页与绝对坐标由 S2 的 `paginate` 完成。
pub fn ast_to_skeleton(root: &Node, settings: &PageSettings) -> DocumentSkeleton {
    let block = convert_node(root, settings);
    let mut skeleton = DocumentSkeleton::default();
    // 根节点（Document）的所有子块直接作为源 IR 的顶层 blocks（不分页）。
    if let BlockKind::Document { children } = &block.kind {
        skeleton.blocks = children.clone();
    } else {
        skeleton.blocks = vec![block];
    }
    skeleton
}

/// 递归转换单个 AST 节点为 SkeletonBlock。
fn convert_node(node: &Node, settings: &PageSettings) -> SkeletonBlock {
    let style = ResolvedStyle::from(node.style.clone());
    match &node.kind {
        NodeKind::Document { children } => SkeletonBlock::new(
            BlockKind::Document {
                children: children.iter().map(|c| convert_node(c, settings)).collect(),
            },
            style,
            node.splittable,
        ),
        NodeKind::Heading { level, children } => SkeletonBlock::new(
            BlockKind::Heading {
                level: *level,
                children: vec![SkeletonBlock::new(
                    BlockKind::Paragraph {
                        lines: layout_inline(children, &node.style, settings),
                    },
                    style.clone(),
                    true,
                )],
            },
            style,
            node.splittable,
        ),
        NodeKind::Paragraph { children } => SkeletonBlock::new(
            BlockKind::Paragraph {
                lines: layout_inline(children, &node.style, settings),
            },
            style,
            node.splittable,
        ),
        NodeKind::List {
            ordered,
            start,
            children,
        } => {
            // 注入列表项 marker（方案 §3.6）：有序用 "N."，无序用 "•"。
            // 子列表（ListItem 内的 List）由其自身 List 节点重新计数，因此递归到
            // convert_list_item 后，内层 List 的 convert_node 会再次计算序号。
            let start_n: u32 = start.unwrap_or(if *ordered { 1 } else { 0 });
            let mut idx = start_n;
            let children_blocks: Vec<SkeletonBlock> = children
                .iter()
                .map(|c| {
                    let marker = if *ordered {
                        let m = format!("{}.", idx);
                        idx += 1;
                        m
                    } else {
                        "•".to_string()
                    };
                    convert_list_item(c, &marker, settings)
                })
                .collect();
            SkeletonBlock::new(
                BlockKind::List {
                    ordered: *ordered,
                    start: *start,
                    children: children_blocks,
                },
                style,
                node.splittable,
            )
        }
        NodeKind::ListItem { children } => {
            // 非列表直接子项的边缘情况：marker 留空，交由外层 List 注入。
            SkeletonBlock::new(
                BlockKind::ListItem {
                    marker: String::new(),
                    children: children.iter().map(|c| convert_node(c, settings)).collect(),
                },
                style,
                node.splittable,
            )
        }
        NodeKind::TaskListItem { checked, children } => {
            let marker = if *checked {
                "☑ ".to_string()
            } else {
                "☐ ".to_string()
            };
            SkeletonBlock::new(
                BlockKind::TaskListItem {
                    marker,
                    checked: *checked,
                    children: children.iter().map(|c| convert_node(c, settings)).collect(),
                },
                style,
                node.splittable,
            )
        }
        NodeKind::Blockquote { children } => SkeletonBlock::new(
            BlockKind::Blockquote {
                children: children.iter().map(|c| convert_node(c, settings)).collect(),
            },
            style,
            node.splittable,
        ),
        NodeKind::CodeBlock { code, lang } => SkeletonBlock::new(
            BlockKind::CodeBlock {
                code: code.clone(),
                lang: lang.clone(),
            },
            style,
            node.splittable,
        ),
        NodeKind::ThematicBreak => {
            SkeletonBlock::new(BlockKind::ThematicBreak, style, node.splittable)
        }
        NodeKind::Table { children, align } => {
            let rows = children
                .iter()
                .filter_map(|row| match &row.kind {
                    NodeKind::TableRow { children: cells } => Some(TableRow {
                        cells: cells
                            .iter()
                            .map(|cell| TableCell {
                                children: vec![convert_node(cell, settings)],
                            })
                            .collect(),
                    }),
                    _ => None,
                })
                .collect();
            SkeletonBlock::new(
                BlockKind::Table {
                    rows,
                    column_align: align.iter().map(|a| DocTextAlign::from(*a)).collect(),
                },
                style,
                node.splittable,
            )
        }
        NodeKind::Image { alt, .. } => {
            // S1：不加载字节，仅占位；S3 投影阶段加载。
            let size = match (style.width, style.height) {
                (Some(w), Some(h)) => (w as f64, h as f64),
                _ => (100.0, 100.0),
            };
            SkeletonBlock::new(
                BlockKind::Image(DocImage {
                    position: (0.0, 0.0),
                    size,
                    pixel_size: (0, 0),
                    data: Vec::new(),
                    format: "png".to_string(),
                    alt: alt.clone(),
                    object_fit: ObjectFit::from(node.style.object_fit),
                    background: None,
                }),
                style,
                node.splittable,
            )
        }
        NodeKind::Center { children } | NodeKind::Container { children } | NodeKind::Span { children } => {
            SkeletonBlock::new(
                BlockKind::Container {
                    children: children.iter().map(|c| convert_node(c, settings)).collect(),
                },
                style,
                node.splittable,
            )
        }
        // 内联节点不应作为块级顶层出现；若遇到，包裹为段落。
        NodeKind::Text { .. }
        | NodeKind::Strong { .. }
        | NodeKind::Emphasis { .. }
        | NodeKind::InlineCode { .. }
        | NodeKind::Link { .. }
        | NodeKind::Delete { .. }
        | NodeKind::Subscript { .. }
        | NodeKind::Superscript { .. }
        | NodeKind::LineBreak
        | NodeKind::TableRow { .. } => SkeletonBlock::new(
            BlockKind::Paragraph {
                lines: layout_inline(std::slice::from_ref(node), &node.style, settings),
            },
            style,
            node.splittable,
        ),
    }
}

/// 转换列表项节点并注入预生成的 `marker`。
///
/// 仅对 `ListItem` / `TaskListItem` 填充 `marker`；其余类型（如嵌套的 `Paragraph`、
/// 内联节点）透传给 [`convert_node`]，保持原有行为。
fn convert_list_item(node: &Node, marker: &str, settings: &PageSettings) -> SkeletonBlock {
    match &node.kind {
        NodeKind::ListItem { children } => SkeletonBlock::new(
            BlockKind::ListItem {
                marker: marker.to_string(),
                children: children.iter().map(|c| convert_node(c, settings)).collect(),
            },
            ResolvedStyle::from(node.style.clone()),
            node.splittable,
        ),
        NodeKind::TaskListItem { checked, children } => SkeletonBlock::new(
            BlockKind::TaskListItem {
                marker: marker.to_string(),
                checked: *checked,
                children: children.iter().map(|c| convert_node(c, settings)).collect(),
            },
            ResolvedStyle::from(node.style.clone()),
            node.splittable,
        ),
        _ => convert_node(node, settings),
    }
}

/// 收集内联子节点的文本段
///
/// 递归展开容器节点（Span、Strong、Emphasis、Link、Delete），
/// 使得每个 Text 片段使用自己的样式。
fn collect_inline_segments(children: &[Node]) -> Vec<(String, TextStyle)> {
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

/// 对展平后的文本段序列做 CSS 空白折叠与合并（`white-space: normal`）。
///
/// 规则：
/// 1. 每个段内部连续空白折叠为单个空格，但**保留边界单空格**
///    （来自 `collapse_whitespace`），用于跨段分词。
/// 2. **跨段合并**：若前一段以空格结尾、后一段以空格开头，则去掉后一段
///    的开头空格，避免 `Hello ` + ` world ` 产生双空格。
/// 3. **块级流边界去空**：去掉首段的开头空格、末段的结尾空格
///    （CSS 行首/行尾孤立空白不渲染）。
/// 4. 折叠后为空的段被移除。
///
/// `\n`（`LineBreak`）段视为硬换行，作为分隔符处理，不被折叠或删空。
fn fold_segments_whitespace(segments: &mut Vec<(String, TextStyle)>) {
    if segments.is_empty() {
        return;
    }

    // 1) 折叠每个段的内部空白（保留边界单空格）；空段标记删除。
    let mut retained: Vec<(String, TextStyle)> = Vec::with_capacity(segments.len());
    for (text, style) in segments.drain(..) {
        if text == "\n" {
            retained.push((text, style));
            continue;
        }
        let folded = crate::html::ast::collapse_whitespace(&text);
        if !folded.is_empty() {
            retained.push((folded, style));
        }
    }

    // 2) 跨段合并：前段结尾空格 + 后段开头空格 → 只保留一个。
    let mut i = 1;
    while i < retained.len() {
        let prev_ends_space = retained[i - 1].0.ends_with(' ');
        let cur_starts_space = retained[i].0.starts_with(' ');
        if prev_ends_space && cur_starts_space && retained[i].0 != "\n" {
            let cur = &mut retained[i].0;
            *cur = cur.trim_start().to_string();
        }
        i += 1;
    }

    // 3) 块级流边界去空：首段开头、末段结尾。
    if let Some(first) = retained.first_mut()
        && first.0 != "\n"
    {
        first.0 = first.0.trim_start().to_string();
    }
    if let Some(last) = retained.last_mut()
        && last.0 != "\n"
    {
        last.0 = last.0.trim_end().to_string();
    }

    // 4) 移除因去空而变空的段。
    retained.retain(|(text, _)| !text.is_empty());

    *segments = retained;
}

/// 从 segments 生成 URL 映射，并标注到 TextLine 的 runs 上。
///
/// 使用顺序匹配：runs 在行中的顺序与 segments 一致。通过跟踪每个 segment
/// 已消费的 Unicode 字符数，正确处理多字节字符（如 emoji）和自动换行场景。
fn annotate_runs_with_urls(
    lines: &mut [TextLine],
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
                let (_seg_text, seg_style) = &segments[seg_idx];
                run.url = seg_style.url.clone();
                run.decoration = seg_style.decoration;
                run.background_color = seg_style.background_color;
                seg_char_consumed += run.text.chars().count();
            }
        }
    }
}

/// 将内联子节点排版为文档文本行。
///
/// 复用 `collect_inline_segments` 与 text 的 `layout_text_with_contexts`，
/// 再投影为 [`DocTextLine`]。
fn layout_inline(
    children: &[Node],
    style: &crate::ast::Style,
    settings: &PageSettings,
) -> Vec<DocTextLine> {
    let mut segments = collect_inline_segments(children);
    // CSS 空白折叠与合并（跨段分词、行首/行尾去空）
    fold_segments_whitespace(&mut segments);
    if segments.is_empty() {
        return Vec::new();
    }
    let total_text: String = segments.iter().map(|(t, _)| t.as_str()).collect();
    let combined: Vec<(&str, &crate::text::TextStyle)> =
        segments.iter().map(|(t, s)| (t.as_str(), s)).collect();

    let available_width = settings.content_width().max(1.0) as f64;
    let text_align = match style.text_align {
        crate::ast::TextAlign::Left => crate::text::TextAlign::Left,
        crate::ast::TextAlign::Center => crate::text::TextAlign::Center,
        crate::ast::TextAlign::Right => crate::text::TextAlign::Right,
        crate::ast::TextAlign::Justify => crate::text::TextAlign::Left,
    };

    FONT_CONTEXT.with(|font_cx| {
        LAYOUT_CONTEXT.with(|layout_cx| {
            let mut fcx = font_cx.borrow_mut();
            let mut lcx = layout_cx.borrow_mut();
            let mut layout =
                crate::text::layout_text_with_contexts(&combined, Some(available_width), text_align, &mut fcx, &mut lcx);
            annotate_runs_with_urls(&mut layout.lines, &total_text, &segments);
            layout.lines.iter().map(DocTextLine::from).collect()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::NodeKind;
    use crate::document::types::page::PageSettings;

    /// 构造最小 AST 树：Document > [Heading, Paragraph, CodeBlock]
    fn sample_ast() -> Node {
        let heading = Node::new(
            NodeKind::Heading {
                level: 1,
                children: vec![Node::new(
                    NodeKind::Text {
                        text: "标题".to_string(),
                    },
                    crate::ast::Style::default(),
                    true,
                )],
            },
            crate::ast::Style::default(),
            false,
        );
        let paragraph = Node::new(
            NodeKind::Paragraph {
                children: vec![Node::new(
                    NodeKind::Text {
                        text: "正文内容".to_string(),
                    },
                    crate::ast::Style::default(),
                    true,
                )],
            },
            crate::ast::Style::default(),
            true,
        );
        let code = Node::new(
            NodeKind::CodeBlock {
                code: "let x = 1;".to_string(),
                lang: Some("rust".to_string()),
            },
            crate::ast::Style::default(),
            true,
        );
        Node::new(
            NodeKind::Document {
                children: vec![heading, paragraph, code],
            },
            crate::ast::Style::default(),
            false,
        )
    }

    fn make_segments(parts: &[&str]) -> Vec<(String, crate::text::TextStyle)> {
        parts
            .iter()
            .map(|s| (s.to_string(), crate::text::TextStyle::default()))
            .collect()
    }

    #[test]
    fn test_fold_segments_whitespace_cross_style() {
        // `This is a **Markdown** document.`
        // 对应 segment 序列：["This is a ", "Markdown", " document."]
        // → 折叠后应为 "This is a Markdown document."（分词空格保留，无双空格）
        let mut segs = make_segments(&["This is a ", "Markdown", " document."]);
        fold_segments_whitespace(&mut segs);
        let joined: String = segs.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(joined, "This is a Markdown document.");
    }

    #[test]
    fn test_fold_segments_whitespace_boundary() {
        // 块级流起点/终点的孤立空白被丢弃
        let mut segs = make_segments(&["  leading", "text", "trailing  "]);
        fold_segments_whitespace(&mut segs);
        let joined: String = segs.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(joined, "leadingtexttrailing");
    }

    #[test]
    fn test_fold_segments_whitespace_double_space_across_boundary() {
        // 前段结尾空格 + 后段开头空格 → 只保留一个
        let mut segs = make_segments(&["Hello ", " world"]);
        fold_segments_whitespace(&mut segs);
        let joined: String = segs.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(joined, "Hello world");
    }

    #[test]
    fn test_fold_segments_whitespace_internal_collapse() {
        // 段内连续空白折叠为单空格
        let mut segs = make_segments(&["a   b", "c\nd"]);
        fold_segments_whitespace(&mut segs);
        let joined: String = segs.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(joined, "a bc d");
    }

    #[test]
    fn test_fold_segments_whitespace_linebreak_preserved() {
        // `<br>` 产生的 \n 段作为硬换行保留，不删空
        let mut segs = make_segments(&["before", "\n", "after"]);
        fold_segments_whitespace(&mut segs);
        let joined: String = segs.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(joined, "before\nafter");
    }

    #[test]
    fn test_ast_to_skeleton_structure() {
        let ast = sample_ast();
        let settings = PageSettings::default();
        let skeleton = ast_to_skeleton(&ast, &settings);

        // 根文档 -> 源 IR 的顶层 blocks 直接是三个顶层块
        assert_eq!(skeleton.blocks.len(), 3);
        let blocks = &skeleton.blocks;
        assert_eq!(blocks.len(), 3);

        // 1) Heading(level=1) 且内部含一个 Paragraph 子块
        match &blocks[0].kind {
            BlockKind::Heading { level, children } => {
                assert_eq!(*level, 1);
                assert_eq!(children.len(), 1);
                assert!(matches!(children[0].kind, BlockKind::Paragraph { .. }));
                assert_eq!(blocks[0].text_content(), "标题");
            }
            _ => panic!("expected Heading"),
        }

        // 2) Paragraph 文本拼接正确
        assert!(matches!(blocks[1].kind, BlockKind::Paragraph { .. }));
        assert_eq!(blocks[1].text_content(), "正文内容");

        // 3) CodeBlock 保留代码与语言
        match &blocks[2].kind {
            BlockKind::CodeBlock { code, lang } => {
                assert_eq!(code, "let x = 1;");
                assert_eq!(*lang, Some("rust".to_string()));
            }
            _ => panic!("expected CodeBlock"),
        }
    }

    #[test]
    fn test_ast_to_skeleton_nested_list() {
        // Document > List(ordered) > [ListItem > Paragraph]
        let item = Node::new(
            NodeKind::ListItem {
                children: vec![Node::new(
                    NodeKind::Paragraph {
                        children: vec![Node::new(
                            NodeKind::Text {
                                text: "项一".to_string(),
                            },
                            crate::ast::Style::default(),
                            true,
                        )],
                    },
                    crate::ast::Style::default(),
                    true,
                )],
            },
            crate::ast::Style::default(),
            true,
        );
        let list = Node::new(
            NodeKind::List {
                ordered: true,
                start: Some(1),
                children: vec![item],
            },
            crate::ast::Style::default(),
            true,
        );
        let ast = Node::new(
            NodeKind::Document {
                children: vec![list],
            },
            crate::ast::Style::default(),
            false,
        );
        let skeleton = ast_to_skeleton(&ast, &PageSettings::default());
        let blocks = &skeleton.blocks;
        assert_eq!(blocks.len(), 1);
        match &blocks[0].kind {
            BlockKind::List {
                ordered, start, ..
            } => {
                assert!(*ordered);
                assert_eq!(*start, Some(1));
            }
            _ => panic!("expected List"),
        }
        assert_eq!(blocks[0].text_content(), "项一");
    }

    /// 验证有序/无序列表的 marker 由 from_ast 正确注入。
    #[test]
    fn test_ast_to_skeleton_list_marker() {
        // 有序列表 1./2.
        let mk_item = |text: &str| {
            Node::new(
                NodeKind::ListItem {
                    children: vec![Node::new(
                        NodeKind::Paragraph {
                            children: vec![Node::new(
                                NodeKind::Text {
                                    text: text.to_string(),
                                },
                                crate::ast::Style::default(),
                                true,
                            )],
                        },
                        crate::ast::Style::default(),
                        true,
                    )],
                },
                crate::ast::Style::default(),
                true,
            )
        };
        let ordered = Node::new(
            NodeKind::List {
                ordered: true,
                start: Some(1),
                children: vec![mk_item("一"), mk_item("二")],
            },
            crate::ast::Style::default(),
            true,
        );
        let ast = Node::new(
            NodeKind::Document {
                children: vec![ordered],
            },
            crate::ast::Style::default(),
            false,
        );
        let sk = ast_to_skeleton(&ast, &PageSettings::default());
        match &sk.blocks[0].kind {
            BlockKind::List { children, .. } => {
                assert_eq!(children.len(), 2);
                match &children[0].kind {
                    BlockKind::ListItem { marker, .. } => assert_eq!(marker, "1."),
                    _ => panic!("expected ListItem"),
                }
                match &children[1].kind {
                    BlockKind::ListItem { marker, .. } => assert_eq!(marker, "2."),
                    _ => panic!("expected ListItem"),
                }
            }
            _ => panic!("expected List"),
        }

        // 无序列表 "•"
        let unordered = Node::new(
            NodeKind::List {
                ordered: false,
                start: None,
                children: vec![mk_item("甲")],
            },
            crate::ast::Style::default(),
            true,
        );
        let ast2 = Node::new(
            NodeKind::Document {
                children: vec![unordered],
            },
            crate::ast::Style::default(),
            false,
        );
        let sk2 = ast_to_skeleton(&ast2, &PageSettings::default());
        match &sk2.blocks[0].kind {
            BlockKind::List { children, .. } => match &children[0].kind {
                BlockKind::ListItem { marker, .. } => assert_eq!(marker, "•"),
                _ => panic!("expected ListItem"),
            },
            _ => panic!("expected List"),
        }
    }
}
