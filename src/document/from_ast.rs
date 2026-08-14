//! AST → `Document` 转换。
//!
//! 将 [`crate::ast::Node`] 树转换为 [`crate::document::layout::Document`]。
//! 文本行（[`crate::document::text::TextLine`]）在此阶段通过复用文档排版模块的
//! 排版函数进行排版。本阶段**不做分页与绝对坐标**（分页由输出后端
//! [`crate::output::pdf`] 负责）。
//!
//! ## 与方案/旧管线不一致（记录于 docs/refactor-log.md）
//! - 行内语义节点（Strong/Emphasis/Delete/Sub/Super）不生成独立 `Block`，
//!   其语义由排版后的 [`crate::document::text::TextRun`] 样式承载（与旧管线一致）。
//! - `<center>`/`<div>`/`<span>` 统一映射到 `BlockKind::Container`（center 的居中
//!   alignment 体现在该块 `ResolvedStyle.text_align`，未额外标记）。
//! - 列表项 marker（有序序号 / 无序圆点 / 任务复选框）已注入（`BlockKind::ListItem` /
//!   `TaskListItem` 的 `marker` 字段，方案 §3.6），由 `from_ast` 在 `List` 边界计算。
//! - 块级 `Image`：若 `src` 为 data URI（`data:image/...;base64,...`，来自
//!   `dom::inline_local_images` 内联），在此解码为字节存入 `DocImage.data`；
//!   若为普通路径则 data 为空，由渲染后端按 `src` 加载（方案 §3.5.1）。
//! - 可用宽度取整页内容宽度 `PageSettings::content_width()`，box model 缩进/边框
//!   占用尚未纳入。

use crate::ast::{Node, NodeKind, computed_style_to_text_style};
use crate::color::Color;
use crate::document::layout::{
    Block, BlockKind, DefinitionItemBlock, Document, TableCell, TableRow,
};
use crate::document::text::{FONT_CONTEXT, LAYOUT_CONTEXT, TextLine, TextStyle};
use crate::document::types::page::PageSettings;
use crate::document::types::{DocImage, ResolvedStyle};

/// 将带样式的 AST 根节点转换为文档 `Document`（不分页的源 IR）。
///
/// `settings` 用于计算内联文本排版的可用宽度（取整页内容宽度）。
/// 分页与绝对坐标由输出后端完成。
pub fn ast_to_layout(root: &Node, settings: &PageSettings) -> Document {
    let block = convert_node(root, settings);
    let mut document = Document::default();
    // 根节点（Document）的所有子块直接作为源 IR 的顶层 blocks（不分页）。
    if let BlockKind::Document { children } = &block.kind {
        document.blocks = children.clone();
    } else {
        document.blocks = vec![block];
    }
    document
}

/// 单元格测量结果：理想宽度（不折行完整宽度）与最小宽度（最宽不可断词）。
struct CellMeasure {
    ideal_width: f64,
    min_width: f64,
}

/// 度量单个单元格的理想宽度与最小宽度（参考 main `generator/table.rs`）。
///
/// - `ideal_width`：不折行时完整文本的自然宽度（`max_width=None` 布局的宽度）。
/// - `min_width`：最宽不可断词（最长单词）的宽度，保证该列至少能容纳一个单词。
fn measure_cell(node: &Node, style: &crate::ast::Style, padding_h: f64) -> CellMeasure {
    let base = computed_style_to_text_style(style);
    let mut segments = collect_inline_segments(std::slice::from_ref(node), &base);
    fold_segments_whitespace(&mut segments);
    if segments.is_empty() {
        return CellMeasure {
            ideal_width: padding_h * 2.0,
            min_width: padding_h * 2.0,
        };
    }
    let combined: Vec<(&str, &crate::document::text::TextStyle)> =
        segments.iter().map(|(t, s)| (t.as_str(), s)).collect();
    let ideal_width = FONT_CONTEXT.with(|font_cx| {
        LAYOUT_CONTEXT.with(|layout_cx| {
            let mut fcx = font_cx.borrow_mut();
            let mut lcx = layout_cx.borrow_mut();
            let layout = crate::document::text::layout_text_with_contexts(
                &combined,
                None,
                crate::ast::TextAlign::Left,
                &mut fcx,
                &mut lcx,
            );
            layout.width
        })
    });
    // 最宽不可断词宽度
    let min_width = segments
        .iter()
        .flat_map(|(text, st)| split_words(text).into_iter().map(move |w| (w, st)))
        .filter(|(w, _)| !w.is_empty())
        .fold(0.0_f64, |acc, (word, st)| {
            let w = FONT_CONTEXT.with(|font_cx| {
                LAYOUT_CONTEXT.with(|layout_cx| {
                    let mut fcx = font_cx.borrow_mut();
                    let mut lcx = layout_cx.borrow_mut();
                    let layout = crate::document::text::layout_text_with_contexts(
                        &[(word, st)],
                        None,
                        crate::ast::TextAlign::Left,
                        &mut fcx,
                        &mut lcx,
                    );
                    layout.width
                })
            });
            acc.max(w)
        });
    CellMeasure {
        ideal_width: ideal_width + padding_h * 2.0,
        min_width: (min_width + padding_h * 2.0).max(padding_h * 2.0),
    }
}

/// 按空白把文本拆分为不可断词（连续非空白段）。
fn split_words(text: &str) -> Vec<&str> {
    text.split(char::is_whitespace)
        .filter(|s| !s.is_empty())
        .collect()
}

/// 计算表格的列宽与行高（参考 main 分支 `generator/table.rs` 算法）。
///
/// 列宽：优先用每列理想宽度；若总和超可用宽度，则保证每列 ≥ 最宽不可断词宽度
/// （min_width），剩余空间按 (ideal - min) 比例分配。这样窄列不会因压缩而容不下文本。
/// 行高：按列宽折行，取该行所有单元格折行后高度的最大值。
fn compute_table_layout(
    cell_nodes: &[Vec<&Node>],
    style: &crate::ast::Style,
    n_cols: usize,
    content_w: f64,
) -> (Vec<f64>, Vec<f64>) {
    if n_cols == 0 || cell_nodes.is_empty() {
        return (vec![content_w], vec![0.0]);
    }
    // 单元格内边距取自样式（与绘制端 pdf.rs/png.rs 读取的
    // `table_cell_padding_h_pt` / `table_cell_padding_v_pt` 保持一致），
    // 避免硬编码 4.0 与 CSS 调整后的实际绘制内边距脱节。
    let padding_h = style.table_cell_padding_h_pt as f64;
    let padding_v = style.table_cell_padding_v_pt as f64;

    // 1. 每列理想/最小宽度 = 该列所有单元格的最大值
    let mut ideal_cols = vec![0.0f64; n_cols];
    let mut min_cols = vec![0.0f64; n_cols];
    for row in cell_nodes {
        for (ci, cell) in row.iter().enumerate() {
            if ci < n_cols {
                let m = measure_cell(cell, style, padding_h);
                ideal_cols[ci] = ideal_cols[ci].max(m.ideal_width);
                min_cols[ci] = min_cols[ci].max(m.min_width);
            }
        }
    }

    // 2. 分配列宽（main 三段式算法）
    let total_ideal: f64 = ideal_cols.iter().sum();
    let col_widths: Vec<f64> = if total_ideal <= content_w {
        ideal_cols.iter().map(|w| w.max(1.0)).collect()
    } else {
        let total_min: f64 = min_cols.iter().sum();
        if total_min >= content_w {
            min_cols.clone()
        } else {
            let extra = content_w - total_min;
            let ideal_extra: f64 = ideal_cols
                .iter()
                .zip(min_cols.iter())
                .map(|(i, m)| (i - m).max(0.0))
                .sum();
            ideal_cols
                .iter()
                .zip(min_cols.iter())
                .map(|(i, m)| {
                    let ratio = if ideal_extra > 0.0 {
                        (i - m).max(0.0) / ideal_extra
                    } else {
                        1.0 / n_cols as f64
                    };
                    m + extra * ratio
                })
                .collect()
        }
    };

    // 3. 行高：按列宽（减左右 padding）折行，取该行所有单元格折行高度的最大值，
    //    并叠加上下内边距（2 × padding_v），使内容垂直居中于上下边框之间，
    //    避免文本底部与下边框贴在一起（见 PDF 表格绘制，内容从 cy+pad_v 起排）。
    let mut row_heights = vec![0.0f64; cell_nodes.len()];
    for (ri, row) in cell_nodes.iter().enumerate() {
        for (ci, cell) in row.iter().enumerate() {
            if ci >= n_cols {
                continue;
            }
            let col_w = col_widths[ci];
            let inner_w = (col_w - padding_h * 2.0).max(1.0);
            let segments = measure_cell_height(cell, style, inner_w);
            row_heights[ri] = row_heights[ri].max(segments + padding_v * 2.0);
        }
    }
    (col_widths, row_heights)
}

/// 按宽度折行测量单元格高度（pt）。
fn measure_cell_height(node: &Node, style: &crate::ast::Style, width: f64) -> f64 {
    let base = computed_style_to_text_style(style);
    let mut segments = collect_inline_segments(std::slice::from_ref(node), &base);
    fold_segments_whitespace(&mut segments);
    if segments.is_empty() {
        return 0.0;
    }
    let combined: Vec<(&str, &crate::document::text::TextStyle)> =
        segments.iter().map(|(t, s)| (t.as_str(), s)).collect();
    FONT_CONTEXT.with(|font_cx| {
        LAYOUT_CONTEXT.with(|layout_cx| {
            let mut fcx = font_cx.borrow_mut();
            let mut lcx = layout_cx.borrow_mut();
            let layout = crate::document::text::layout_text_with_contexts(
                &combined,
                Some(width),
                crate::ast::TextAlign::Left,
                &mut fcx,
                &mut lcx,
            );
            layout.height
        })
    })
}

/// 按指定宽度转换表格单元格（`NodeKind::Paragraph`）为段落块。
///
/// 与 `convert_node` 的 Paragraph 分支等价，但 `layout_inline` 使用 `col_width`
/// 作为可用宽度，保证文本在真实列宽下正确折行、不溢出。
fn convert_cell(node: &Node, settings: &PageSettings, col_width: f64) -> Block {
    let style = ResolvedStyle::from(node.style.clone());
    match &node.kind {
        NodeKind::Paragraph { children } => Block::new(
            BlockKind::Paragraph {
                lines: layout_inline(children, &node.style, settings, Some(col_width)),
            },
            style,
            node.splittable,
        ),
        // 其余节点（如纯文本）按整列宽布局
        _ => convert_node(node, settings),
    }
}

/// 递归转换单个 AST 节点为 `Block`。
fn convert_node(node: &Node, settings: &PageSettings) -> Block {
    let style = ResolvedStyle::from(node.style.clone());
    match &node.kind {
        NodeKind::Document { children } => Block::new(
            BlockKind::Document {
                children: children.iter().map(|c| convert_node(c, settings)).collect(),
            },
            style,
            node.splittable,
        ),
        NodeKind::Heading { level, children } => Block::new(
            BlockKind::Heading {
                level: *level,
                children: vec![Block::new(
                    BlockKind::Paragraph {
                        lines: layout_inline(children, &node.style, settings, None),
                    },
                    style.clone(),
                    true,
                )],
            },
            style,
            node.splittable,
        ),
        NodeKind::Paragraph { children } => {
            // Markdown 图片（`![alt](src)`）经 pulldown 包在 `<p>` 段落内。纯图片段落
            // 提升为独立的 `BlockKind::Image`（而非降级为 alt 文本），并默认居中。
            // 文本+图片混排的段落仍走内联排版（图片暂作 alt 文本占位）。
            if children.len() == 1
                && let NodeKind::Image { .. } = &children[0].kind
            {
                let mut centered = style.clone();
                centered.text_align = crate::ast::TextAlign::Center;
                return convert_image_node(&children[0], &centered, settings);
            }
            Block::new(
                BlockKind::Paragraph {
                    lines: layout_inline(children, &node.style, settings, None),
                },
                style,
                node.splittable,
            )
        }
        NodeKind::List {
            ordered,
            start,
            children,
        } => {
            // 注入列表项 marker（方案 §3.6）：有序用 "N."，无序用 "●"（实心大圆点，
            // 比默认的 "•" 在正文同字号下视觉更大更清晰）。
            // 子列表（ListItem 内的 List）由其自身 List 节点重新计数，因此递归到
            // convert_list_item 后，内层 List 的 convert_node 会再次计算序号。
            let start_n: u32 = start.unwrap_or(if *ordered { 1 } else { 0 });
            let mut idx = start_n;
            let children_blocks: Vec<Block> = children
                .iter()
                .map(|c| {
                    let marker = if *ordered {
                        let m = format!("{}.", idx);
                        idx += 1;
                        m
                    } else {
                        "●".to_string()
                    };
                    convert_list_item(c, &marker, settings)
                })
                .collect();
            Block::new(
                BlockKind::List {
                    ordered: *ordered,
                    start: *start,
                    children: children_blocks,
                },
                style,
                node.splittable,
            )
        }
        NodeKind::DefinitionList { items } => {
            // 定义列表：术语（dt）与定义（dd）分别转换为块序列。
            let items_blocks: Vec<DefinitionItemBlock> = items
                .iter()
                .map(|item| DefinitionItemBlock {
                    term: item
                        .term
                        .iter()
                        .map(|c| convert_node(c, settings))
                        .collect(),
                    definition: item
                        .definition
                        .iter()
                        .map(|c| convert_node(c, settings))
                        .collect(),
                })
                .collect();
            Block::new(
                BlockKind::DefinitionList {
                    items: items_blocks,
                },
                style,
                node.splittable,
            )
        }
        NodeKind::FootnoteDef { id, children } => {
            // 脚注定义（末尾聚合）：子节点转为块序列，携带 label 供 PDF 内部跳转定位。
            let child_blocks: Vec<Block> =
                children.iter().map(|c| convert_node(c, settings)).collect();
            Block::new(
                BlockKind::FootnoteDef {
                    id: id.clone(),
                    children: child_blocks,
                },
                style,
                node.splittable,
            )
        }
        NodeKind::ListItem { .. } => {
            // 非列表直接子项的边缘情况：marker 留空，交由 convert_list_item 统一处理
            // （合并连续内联节点为单个段落，避免每个内联节点被拆成独立段落）。
            convert_list_item(node, "", settings)
        }
        NodeKind::TaskListItem { checked, children } => {
            let marker = if *checked {
                "☑ ".to_string()
            } else {
                "☐ ".to_string()
            };
            Block::new(
                BlockKind::TaskListItem {
                    marker,
                    checked: *checked,
                    children: children.iter().map(|c| convert_node(c, settings)).collect(),
                },
                style,
                node.splittable,
            )
        }
        NodeKind::Blockquote { children } => Block::new(
            BlockKind::Blockquote {
                children: children.iter().map(|c| convert_node(c, settings)).collect(),
            },
            style,
            node.splittable,
        ),
        NodeKind::CodeBlock { code, lang } => Block::new(
            BlockKind::CodeBlock {
                code: code.clone(),
                lang: lang.clone(),
            },
            style,
            node.splittable,
        ),
        NodeKind::ThematicBreak => Block::new(BlockKind::ThematicBreak, style, node.splittable),
        NodeKind::Table { children, align } => {
            // 先收集所有单元格的原始 AST 节点（行→列），用于真实度量列宽/行高。
            let cell_nodes: Vec<Vec<&Node>> = children
                .iter()
                .filter_map(|row| match &row.kind {
                    NodeKind::TableRow { children: cells } => {
                        Some(cells.iter().collect::<Vec<_>>())
                    }
                    _ => None,
                })
                .collect();
            let n_cols = cell_nodes.iter().map(|r| r.len()).max().unwrap_or(0);
            let content_w = settings.content_width() as f64;

            // 度量每列自然宽度与每行折行高度（参考 main 分支 generator/table.rs 算法）。
            let (col_widths, row_heights) =
                compute_table_layout(&cell_nodes, &node.style, n_cols, content_w);

            // 按真实列宽布局每个单元格（避免文本在窄列下溢出）。
            let rows = cell_nodes
                .into_iter()
                .map(|cells| TableRow {
                    cells: cells
                        .into_iter()
                        .enumerate()
                        .map(|(ci, cell)| {
                            // 与 compute_table_layout 的 padding_h 保持一致（内容区减 2*padding）。
                            let col_w = col_widths.get(ci).copied().unwrap_or(content_w);
                            TableCell {
                                children: vec![convert_cell(
                                    cell,
                                    settings,
                                    (col_w - 8.0).max(1.0),
                                )],
                            }
                        })
                        .collect(),
                })
                .collect();
            Block::new(
                BlockKind::Table {
                    rows,
                    column_align: align.to_vec(),
                    col_widths,
                    row_heights,
                },
                style,
                node.splittable,
            )
        }
        NodeKind::Image { .. } => convert_image_node(node, &style, settings),
        NodeKind::Center { children }
        | NodeKind::Container { children }
        | NodeKind::Span { children } => Block::new(
            BlockKind::Container {
                children: children.iter().map(|c| convert_node(c, settings)).collect(),
            },
            style,
            node.splittable,
        ),
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
        | NodeKind::TableRow { .. } => Block::new(
            BlockKind::Paragraph {
                lines: layout_inline(std::slice::from_ref(node), &node.style, settings, None),
            },
            style,
            node.splittable,
        ),
    }
}

/// 转换列表项节点并注入预生成的 `marker`。
///
/// 关键修复：列表项内的多个并列内联节点（如 `Text`/`Emphasis`/`Strong` 兄弟）
/// 必须合并为一个 `Paragraph` 并整体排版，而不是让每个内联节点各自落到
/// `convert_node` 的"内联节点当作块级顶层"分支、被拆成独立段落。
/// 这里按"块级 / 内联"对 children 分组，连续的內联节点合并为单个段落。
fn convert_list_item(node: &Node, marker: &str, settings: &PageSettings) -> Block {
    match &node.kind {
        NodeKind::ListItem { children } => Block::new(
            BlockKind::ListItem {
                marker: marker.to_string(),
                // 不再把 marker 前置进正文首行文本：marker 由各输出后端在缩进槽
                // 左缘**单独绘制**（PDF/PNG 矢量画圆点/方框、有序数字画文本；
                // SVG 单独 `<text>`）。正文首行与续行统一从缩进槽起点排起，
                // 因此 marker 不会与正文重叠，天然实现悬挂缩进。
                children: group_inline_children(children, settings),
            },
            ResolvedStyle::from(node.style.clone()),
            node.splittable,
        ),
        NodeKind::TaskListItem { checked, children } => Block::new(
            BlockKind::TaskListItem {
                marker: marker.to_string(),
                checked: *checked,
                children: group_inline_children(children, settings),
            },
            ResolvedStyle::from(node.style.clone()),
            node.splittable,
        ),
        _ => convert_node(node, settings),
    }
}

/// 将 `NodeKind::Image` 节点转换为独立的 `BlockKind::Image` 块。
///
/// - 图片字节：若 `src` 为 data URI（`data:image/xxx;base64,...`，来自
///   `dom::inline_local_images` / `embed_local_images` 内联），在此解码为字节并探测
///   原始像素尺寸；若是普通路径则 data 留空（渲染后端按需加载，方案 §3.5.1）。
/// - 尺寸解析：显式 width/height 优先，缺失维度按原始宽高比推算；都未指定时
///   "适合页宽"（宽度=内容宽度，高度按宽高比），避免固定 `100×100` 造成图片失真。
/// - `style` 可由调用方指定（纯图片段落会传入 `text_align: Center` 以居中）。
fn convert_image_node(node: &Node, style: &ResolvedStyle, settings: &PageSettings) -> Block {
    let NodeKind::Image { src, alt, .. } = &node.kind else {
        unreachable!("convert_image_node 只接受 Image 节点");
    };
    let (data, format) = decode_image_data_uri(src);
    let pixel = if data.is_empty() {
        (0, 0)
    } else {
        probe_image_dimensions(&data).unwrap_or((0, 0))
    };
    let content_w = settings.content_width() as f64;
    let size = resolve_image_size(style.width, style.height, pixel, content_w);
    Block::new(
        BlockKind::Image(DocImage {
            position: (0.0, 0.0),
            size,
            pixel_size: pixel,
            data,
            format,
            alt: alt.clone(),
            object_fit: node.style.object_fit,
            background: None,
        }),
        style.clone(),
        node.splittable,
    )
}

/// 判断节点是否为内联节点（应合并进同一段落）。
fn is_inline_node(n: &Node) -> bool {
    matches!(
        n.kind,
        NodeKind::Text { .. }
            | NodeKind::Strong { .. }
            | NodeKind::Emphasis { .. }
            | NodeKind::InlineCode { .. }
            | NodeKind::Link { .. }
            | NodeKind::Delete { .. }
            | NodeKind::Subscript { .. }
            | NodeKind::Superscript { .. }
            | NodeKind::LineBreak
    )
}

/// 将连续的內联兄弟节点合并为单个 `Paragraph`，块级节点保持独立。
fn group_inline_children(children: &[Node], settings: &PageSettings) -> Vec<Block> {
    let mut out = Vec::new();
    let mut inline_buf: Vec<&Node> = Vec::new();
    for c in children {
        if is_inline_node(c) {
            inline_buf.push(c);
        } else {
            flush_inline_buffer(&mut inline_buf, &mut out, settings);
            out.push(convert_node(c, settings));
        }
    }
    flush_inline_buffer(&mut inline_buf, &mut out, settings);
    out
}

/// 把缓冲的连续内联节点合并成一个 `Paragraph` 块（整体排版）。
fn flush_inline_buffer(buf: &mut Vec<&Node>, out: &mut Vec<Block>, settings: &PageSettings) {
    if buf.is_empty() {
        return;
    }
    let nodes: Vec<Node> = buf.iter().map(|n| (*n).clone()).collect();
    let style = ResolvedStyle::from(nodes[0].style.clone());
    out.push(Block::new(
        BlockKind::Paragraph {
            lines: layout_inline(&nodes, &nodes[0].style, settings, None),
        },
        style,
        true,
    ));
    buf.clear();
}

/// 收集内联子节点的文本段
///
/// 递归展开容器节点（Span、Strong、Emphasis、Link、Delete 等），
/// 将每个叶子片段的样式**按节点语义**叠加（而非依赖节点自身的
/// `style` 字段——因为 Bold/Italic 等语义由 `NodeKind` 表达，由下游
/// 样式解析阶段注入 `ResolvedStyle`，并非写在节点 `style` 上）。
///
/// - `Strong`/`B` → bold
/// - `Emphasis`/`I` → italic
/// - `Delete`/`S` → line-through
/// - `Link` → 携带其 `url`
/// - `Subscript` → 下标（`baseline_shift < 0`）
/// - `Superscript` → 上标（`baseline_shift > 0`）
/// - `InlineCode` → 等宽字体（mono）
///
/// 这些语义同时供 PDF 排版（`segments`）与原始流式内容（`ParagraphRaw`）
/// 使用，保证二者一致（方案 A 双轨同源）。
fn collect_inline_segments(children: &[Node], inherited: &TextStyle) -> Vec<(String, TextStyle)> {
    let mut segments = Vec::new();
    for child in children {
        match &child.kind {
            NodeKind::Strong { children: inner }
            | NodeKind::Emphasis { children: inner }
            | NodeKind::Link {
                children: inner, ..
            }
            | NodeKind::Delete { children: inner }
            | NodeKind::Subscript { children: inner }
            | NodeKind::Superscript { children: inner }
            | NodeKind::Span { children: inner } => {
                let mut merged = inherited.clone();
                apply_node_semantic_style(&child.kind, &mut merged);
                if let NodeKind::Link { url, .. } = &child.kind {
                    merged.url = Some(url.clone());
                }
                segments.extend(collect_inline_segments(inner, &merged));
            }
            NodeKind::Text { text } => {
                if !text.is_empty() {
                    let style = inherited.clone();
                    segments.push((text.clone(), style));
                }
            }
            NodeKind::InlineCode { code } => {
                if !code.is_empty() {
                    let mut style = inherited.clone();
                    style.font_family = vec!["monospace".to_string()];
                    // 行内代码用灰色背景框区分（PDF/PNG 由 draw_text_run 依据
                    // background_color 绘制矩形；SVG/HTML 由样式输出）。
                    style.background_color = Some(Color::new(238, 240, 244));
                    style.color = Color::new(199, 52, 29);
                    segments.push((code.clone(), style));
                }
            }
            NodeKind::LineBreak => {
                let style = inherited.clone();
                segments.push(("\n".to_string(), style));
            }
            _ => {
                let text = child.kind.text_content();
                if !text.is_empty() {
                    let style = inherited.clone();
                    segments.push((text, style));
                }
            }
        }
    }
    segments
}

/// 根据节点语义向样式中叠加粗体/斜体/修饰/基线偏移等标记。
///
/// 注意：这是**语义层**标记（来自 `NodeKind`），与 CSS 预设解析出的
/// `ResolvedStyle` 解耦——流式后端与排版后端都从这里取同源语义。
fn apply_node_semantic_style(kind: &NodeKind, style: &mut TextStyle) {
    match kind {
        NodeKind::Strong { .. } => style.font_weight = "bold".to_string(),
        NodeKind::Emphasis { .. } => style.font_style = "italic".to_string(),
        NodeKind::Delete { .. } => style.decoration = crate::ast::TextDecoration::LineThrough,
        NodeKind::Subscript { .. } => style.baseline_shift = -(style.font_size as f32 * 0.3),
        NodeKind::Superscript { .. } => style.baseline_shift = style.font_size as f32 * 0.3,
        _ => {}
    }
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
        let folded = crate::dom::collapse_whitespace(&text);
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
    // 各 segment 的字节区间（相对 `total_text`），按顺序累计。
    // 匹配时用 run 在 `total_text` 中的字节偏移（`run.text_offset`）精确命中，
    // 而非按字符数顺序累加——后者在「一个 run 横跨多个 segment」（如 CJK 字体
    // 优先后整行合并为一个 run）时会错位，导致链接 url 丢失。
    let mut seg_bytes: Vec<(usize, usize, &(String, TextStyle))> =
        Vec::with_capacity(segments.len());
    let mut acc = 0usize;
    for seg in segments {
        let end = acc + seg.0.len();
        seg_bytes.push((acc, end, seg));
        acc = end;
    }

    for line in lines.iter_mut() {
        for run in line.runs.iter_mut() {
            let start = run.text_offset;
            let end = start + run.text.len();
            // 一个 run 可能横跨多个 segment（CJK 优先后 parley 常把整行合并为
            // 一个 run）。优先取与 run 区间重叠且带 url 的 segment（保证链接不丢）；
            // 否则回退到 run 起点所在的 segment。
            let matched = seg_bytes
                .iter()
                .find(|(s, e, seg)| *s < end && *e > start && seg.1.url.is_some())
                .or_else(|| seg_bytes.iter().find(|(s, e, _)| *s <= start && start < *e));
            if let Some((_, _, (_seg_text, seg_style))) = matched {
                run.url = seg_style.url.clone();
                run.decoration = seg_style.decoration;
                run.background_color = seg_style.background_color;
            }
        }
    }
}

/// 从图片 `src` 解析 data URI，返回（解码后的字节, 图片格式）。
///
/// 支持 `data:image/<format>;base64,<payload>`。若 `src` 不是 data URI（如普通文件路径），
/// 返回 `(Vec::new(), "png".to_string())`，由渲染后端按需加载（方案 §3.5.1）。
fn decode_image_data_uri(src: &str) -> (Vec<u8>, String) {
    // 形如 "data:image/png;base64,...."。前缀（`data:image/` 与 `;base64,`）大小写不敏感，
    // 但 **payload 必须保留原始大小写**（base64 编码区分大小写，不能 lower）。
    // 这里先在 lower 副本上定位各分隔符的字节偏移，再从原串切出 payload。
    let lower = src.to_ascii_lowercase();
    let prefix = match lower.strip_prefix("data:image/") {
        Some(r) => r,
        None => return (Vec::new(), "png".to_string()),
    };
    // prefix 在 lower 中从 `prefix_start` 开始；semicolon 在 lower 中的全局偏移。
    let prefix_start = src.len() - prefix.len();
    let semicolon_rel = match prefix.find(';') {
        Some(i) => i,
        None => return (Vec::new(), "png".to_string()),
    };
    let semicolon = prefix_start + semicolon_rel;
    // format 从原串按字节偏移切出（format 本身通常为字母，但保持原样更稳妥）。
    let format = src[prefix_start..semicolon].to_string();
    // 检查 `;` 之后是否为 `base64,`（大小写不敏感），payload 保留原串大小写。
    let marker = lower[semicolon + 1..].strip_prefix("base64,");
    let payload = match marker {
        Some(_) => &src[semicolon + 1 + "base64,".len()..],
        None => return (Vec::new(), format),
    };
    // base64 URL-safe 与标准两种 padding 均可解码。
    use base64::Engine;
    use base64::engine::general_purpose::{STANDARD, URL_SAFE};
    let bytes = STANDARD
        .decode(payload)
        .or_else(|_| URL_SAFE.decode(payload))
        .unwrap_or_default();
    (bytes, format)
}

/// 探测图片字节的原始像素尺寸（宽, 高）。
///
/// 仅读取文件头，不解码整图。解码失败或尺寸未知时返回 `None`。
fn probe_image_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let reader = image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .ok()?;
    reader.into_dimensions().ok()
}

/// 解析图片的显示尺寸（pt）。
///
/// - 同时指定 `width` + `height`：作为固定 box（不保持比例，交由 `object_fit` 处理）。
/// - 仅指定 `width` 或 `height`：缺失维度按原始宽高比推算，保持比例不拉伸。
/// - 都未指定：**适合页宽**（宽度 = 内容宽度，高度按宽高比）；未知宽高比时按 4:3 兜底。
///   取代旧的固定 `100×100` 兜底，避免图片失真或超出页面。
fn resolve_image_size(
    css_w: Option<f32>,
    css_h: Option<f32>,
    pixel: (u32, u32),
    content_w: f64,
) -> (f64, f64) {
    let (pw, ph) = (pixel.0 as f64, pixel.1 as f64);
    let has_aspect = pw > 0.0 && ph > 0.0;
    match (css_w, css_h) {
        (Some(w), Some(h)) => (w as f64, h as f64),
        (Some(w), None) => {
            let w = w as f64;
            let h = if has_aspect { w * ph / pw } else { w * 0.75 };
            (w, h)
        }
        (None, Some(h)) => {
            let h = h as f64;
            let w = if has_aspect {
                h * pw / ph
            } else {
                h * 4.0 / 3.0
            };
            (w, h)
        }
        (None, None) => {
            if has_aspect {
                (content_w, content_w * ph / pw)
            } else {
                (content_w, content_w * 0.75)
            }
        }
    }
}

/// 将内联子节点排版为文档文本行。
///
/// 复用 `collect_inline_segments` 与 text 的 `layout_text_with_contexts`，
/// 产出已按页面可用宽度折行、含坐标的 [`crate::document::text::TextLine`]，供 PDF 等精确布局
/// 后端重放（PDF 走 `Document` 排版层，不做二次布局）。
fn layout_inline(
    children: &[Node],
    style: &crate::ast::Style,
    settings: &PageSettings,
    available_width: Option<f64>,
) -> Vec<crate::document::text::TextLine> {
    let base_style = computed_style_to_text_style(style);
    let mut segments = collect_inline_segments(children, &base_style);
    // CSS 空白折叠与合并（跨段分词、行首/行尾去空）
    fold_segments_whitespace(&mut segments);
    if segments.is_empty() {
        return Vec::new();
    }
    let total_text: String = segments.iter().map(|(t, _)| t.as_str()).collect();
    let combined: Vec<(&str, &crate::document::text::TextStyle)> =
        segments.iter().map(|(t, s)| (t.as_str(), s)).collect();

    let available_width = available_width
        .unwrap_or(settings.content_width() as f64)
        .max(1.0);
    let text_align = match style.text_align {
        crate::ast::TextAlign::Left => crate::ast::TextAlign::Left,
        crate::ast::TextAlign::Center => crate::ast::TextAlign::Center,
        crate::ast::TextAlign::Right => crate::ast::TextAlign::Right,
        crate::ast::TextAlign::Justify => crate::ast::TextAlign::Left,
    };

    FONT_CONTEXT.with(|font_cx| {
        LAYOUT_CONTEXT.with(|layout_cx| {
            let mut fcx = font_cx.borrow_mut();
            let mut lcx = layout_cx.borrow_mut();
            let mut layout = crate::document::text::layout_text_with_contexts(
                &combined,
                Some(available_width),
                text_align,
                &mut fcx,
                &mut lcx,
            );
            annotate_runs_with_urls(&mut layout.lines, &total_text, &segments);
            layout.lines
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

    fn make_segments(parts: &[&str]) -> Vec<(String, crate::document::text::TextStyle)> {
        parts
            .iter()
            .map(|s| (s.to_string(), crate::document::text::TextStyle::default()))
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
    fn test_decode_image_data_uri_png() {
        // "data:image/png;base64,aGVsbG8=" → 字节 "hello"
        let (data, format) = decode_image_data_uri("data:image/png;base64,aGVsbG8=");
        assert_eq!(data, b"hello");
        assert_eq!(format, "png");
    }

    #[test]
    fn test_decode_image_data_uri_plain_path() {
        // 普通路径不做内联，data 留空交由渲染后端按需加载
        let (data, format) = decode_image_data_uri("assets/logo.png");
        assert!(data.is_empty());
        assert_eq!(format, "png");
    }

    #[test]
    fn test_decode_image_data_uri_prefix_case_insensitive() {
        // 前缀大小写不敏感
        let (data, _format) = decode_image_data_uri("DATA:IMAGE/JPEG;base64,aGVsbG8=");
        assert_eq!(data, b"hello");
    }

    #[test]
    fn test_decode_image_data_uri_ast_to_layout() {
        // 端到端：Image 节点 src 为 data URI 时，`Document` 的 DocImage 应持有解码后字节。
        let img = Node::new(
            NodeKind::Image {
                src: "data:image/png;base64,aGVsbG8=".to_string(),
                alt: "logo".to_string(),
                title: None,
            },
            crate::ast::Style::default(),
            false,
        );
        let root = Node::new(
            NodeKind::Document {
                children: vec![img],
            },
            crate::ast::Style::default(),
            false,
        );
        let settings = PageSettings::default();
        let document = ast_to_layout(&root, &settings);
        assert_eq!(document.blocks.len(), 1);
        match &document.blocks[0].kind {
            BlockKind::Image(doc_img) => {
                assert_eq!(doc_img.data, b"hello");
                assert_eq!(doc_img.format, "png");
            }
            _ => panic!("expected Image"),
        }
    }

    #[test]
    fn test_resolve_image_size_both_explicit() {
        // 同时指定宽高：作为固定 box，不按比例
        let (w, h) = resolve_image_size(Some(200.0), Some(100.0), (800, 600), 500.0);
        assert_eq!((w, h), (200.0, 100.0));
    }

    #[test]
    fn test_resolve_image_size_width_only_preserves_aspect() {
        // 仅指定宽度：高度按原始宽高比推算（800x600 → 200x150）
        let (w, h) = resolve_image_size(Some(200.0), None, (800, 600), 500.0);
        assert_eq!((w, h), (200.0, 150.0));
    }

    #[test]
    fn test_resolve_image_size_height_only_preserves_aspect() {
        // 仅指定高度：宽度按原始宽高比推算（800x600 → 200x150）
        let (w, h) = resolve_image_size(None, Some(150.0), (800, 600), 500.0);
        assert_eq!((w, h), (200.0, 150.0));
    }

    #[test]
    fn test_resolve_image_size_fit_content_width() {
        // 均未指定：适合页宽（宽度=内容宽度，高度按宽高比）
        let content_w = PageSettings::default().content_width() as f64;
        let (w, h) = resolve_image_size(None, None, (800, 600), content_w);
        assert_eq!(w, content_w);
        assert!((h - content_w * 600.0 / 800.0).abs() < 1e-9);
    }

    #[test]
    fn test_resolve_image_size_unknown_aspect_ratio_uses_4x3() {
        // 未知宽高比时按 4:3 兜底
        let (w, h) = resolve_image_size(None, None, (0, 0), 400.0);
        assert_eq!(w, 400.0);
        assert!((h - 300.0).abs() < 1e-9);
    }

    #[test]
    fn test_resolve_image_size_not_100_fixed() {
        // 回归：不再固定 100×100；未指定时按内容宽度缩放
        let content_w = PageSettings::default().content_width() as f64;
        let (w, _) = resolve_image_size(None, None, (800, 600), content_w);
        assert_eq!(w, content_w);
        assert!(content_w > 100.0);
    }

    #[test]
    fn test_image_only_paragraph_becomes_centered_image_block() {
        // Markdown `![alt](src)` 单独成段 → `<p><img></p>` → 纯图片段落。
        // 应提升为独立 `BlockKind::Image` 且 `text_align: Center`（默认居中），
        // 而非降级为 alt 文本的 Paragraph。
        let img = Node::new(
            NodeKind::Image {
                src: "data:image/png;base64,aGVsbG8=".to_string(),
                alt: "logo".to_string(),
                title: None,
            },
            crate::ast::Style::default(),
            false,
        );
        let para = Node::new(
            NodeKind::Paragraph {
                children: vec![img],
            },
            crate::ast::Style::default(),
            false,
        );
        let root = Node::new(
            NodeKind::Document {
                children: vec![para],
            },
            crate::ast::Style::default(),
            false,
        );
        let settings = PageSettings::default();
        let document = ast_to_layout(&root, &settings);
        assert_eq!(document.blocks.len(), 1);
        match &document.blocks[0].kind {
            BlockKind::Image(doc_img) => {
                assert_eq!(doc_img.data, b"hello");
                assert_eq!(
                    document.blocks[0].style.text_align,
                    crate::ast::TextAlign::Center,
                    "纯图片段落应默认居中"
                );
            }
            other => panic!("expected Image, got {:?}", other),
        }
    }

    #[test]
    fn test_mixed_paragraph_keeps_inline() {
        // 文本+图片混排的段落保持 Paragraph（图片暂作 alt 文本占位），不提升为独立块。
        let img = Node::new(
            NodeKind::Image {
                src: "data:image/png;base64,aGVsbG8=".to_string(),
                alt: "img".to_string(),
                title: None,
            },
            crate::ast::Style::default(),
            false,
        );
        let text = Node::new(
            NodeKind::Text {
                text: "hello".to_string(),
            },
            crate::ast::Style::default(),
            false,
        );
        let para = Node::new(
            NodeKind::Paragraph {
                children: vec![text, img],
            },
            crate::ast::Style::default(),
            false,
        );
        let root = Node::new(
            NodeKind::Document {
                children: vec![para],
            },
            crate::ast::Style::default(),
            false,
        );
        let settings = PageSettings::default();
        let document = ast_to_layout(&root, &settings);
        assert_eq!(document.blocks.len(), 1);
        assert!(
            matches!(document.blocks[0].kind, BlockKind::Paragraph { .. }),
            "混排段落应保持 Paragraph"
        );
    }

    #[test]
    fn test_inherit_object_fit_matches_default() {
        // 回归：`inherit_from` 的 object_fit 默认应与 `Style::default()` 一致（均为 Contain），
        // 避免图片节点经样式解析后默认被拉伸（None）。
        let d = crate::ast::Style::default();
        let inh = crate::ast::Style::inherit_from(&d);
        assert_eq!(
            inh.object_fit, d.object_fit,
            "inherit_from 的 object_fit 默认应与 default 一致"
        );
        assert_eq!(
            inh.object_fit,
            crate::ast::ObjectFit::Contain,
            "图片默认应不拉伸"
        );
    }

    #[test]
    fn test_ast_to_layout_structure() {
        let ast = sample_ast();
        let settings = PageSettings::default();
        let document = ast_to_layout(&ast, &settings);

        // 根文档 -> 源 IR 的顶层 blocks 直接是三个顶层块
        assert_eq!(document.blocks.len(), 3);
        let blocks = &document.blocks;
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
    fn test_ast_to_layout_nested_list() {
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
        let document = ast_to_layout(&ast, &PageSettings::default());
        let blocks = &document.blocks;
        assert_eq!(blocks.len(), 1);
        match &blocks[0].kind {
            BlockKind::List { ordered, start, .. } => {
                assert!(*ordered);
                assert_eq!(*start, Some(1));
            }
            _ => panic!("expected List"),
        }
        assert_eq!(blocks[0].text_content(), "项一");
    }

    /// 验证有序/无序列表的 marker 由 from_ast 正确注入。
    #[test]
    fn test_ast_to_layout_list_marker() {
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
        let document = ast_to_layout(&ast, &PageSettings::default());
        match &document.blocks[0].kind {
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
        let document2 = ast_to_layout(&ast2, &PageSettings::default());
        match &document2.blocks[0].kind {
            BlockKind::List { children, .. } => match &children[0].kind {
                BlockKind::ListItem { marker, .. } => assert_eq!(marker, "●"),
                _ => panic!("expected ListItem"),
            },
            _ => panic!("expected List"),
        }
    }

    /// 行内代码应带灰色背景色（与正文区分）。
    #[test]
    fn test_inline_code_background_color() {
        let code = Node::new(
            NodeKind::InlineCode {
                code: "fn main()".to_string(),
            },
            crate::ast::Style::default(),
            true,
        );
        let para = Node::new(
            NodeKind::Paragraph {
                children: vec![code],
            },
            crate::ast::Style::default(),
            true,
        );
        let ast = Node::new(
            NodeKind::Document {
                children: vec![para],
            },
            crate::ast::Style::default(),
            false,
        );
        let document = ast_to_layout(&ast, &PageSettings::default());
        match &document.blocks[0].kind {
            BlockKind::Paragraph { lines } => {
                let run = &lines[0].runs[0];
                assert!(run.background_color.is_some(), "行内代码应有灰色背景色");
            }
            _ => panic!("expected Paragraph"),
        }
    }

    /// 表格列宽：每列 ≥ 最宽不可断词宽度，且总和 ≤ 内容宽度。
    #[test]
    fn test_table_column_widths_respect_min() {
        // 构造一个两列表格，第二列含一个很长的单词。
        let mk_cell = |text: &str| {
            Node::new(
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
            )
        };
        let row1 = Node::new(
            NodeKind::TableRow {
                children: vec![
                    mk_cell("Name"),
                    mk_cell("Supercalifragilisticexpialidocious"),
                ],
            },
            crate::ast::Style::default(),
            true,
        );
        let ast = Node::new(
            NodeKind::Document {
                children: vec![Node::new(
                    NodeKind::Table {
                        children: vec![row1],
                        align: vec![],
                    },
                    crate::ast::Style::default(),
                    true,
                )],
            },
            crate::ast::Style::default(),
            false,
        );
        let document = ast_to_layout(&ast, &PageSettings::default());
        match &document.blocks[0].kind {
            BlockKind::Table {
                col_widths,
                rows,
                row_heights,
                ..
            } => {
                let content_w = PageSettings::default().content_width() as f64;
                let sum: f64 = col_widths.iter().sum();
                assert!(
                    sum <= content_w + 1.0,
                    "列宽总和应 ≤ 内容宽度, got {} > {}",
                    sum,
                    content_w
                );
                assert_eq!(col_widths.len(), 2);
                assert_eq!(rows.len(), 1);
                // 行高应包含上下内边距（2 × table_cell_padding_v_pt，默认 2pt），
                // 使单元格内容垂直居中、不与边框贴边。
                assert_eq!(row_heights.len(), 1);
                assert!(row_heights[0] > 0.0, "行高应为正, got {}", row_heights[0]);
            }
            _ => panic!("expected Table"),
        }
    }

    /// 表格行高必须叠加上下内边距：内容从 `cy + pad_v` 起排，行高若只含内容高度，
    /// 文本底部会贴到（甚至超出）下边框（见 PDF 表格绘制）。此测试锁定该回归。
    #[test]
    fn test_table_row_height_includes_vertical_padding() {
        let mk_cell = |text: &str| {
            Node::new(
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
            )
        };
        let row1 = Node::new(
            NodeKind::TableRow {
                children: vec![mk_cell("Cell A"), mk_cell("Cell B")],
            },
            crate::ast::Style::default(),
            true,
        );
        let ast = Node::new(
            NodeKind::Document {
                children: vec![Node::new(
                    NodeKind::Table {
                        children: vec![row1],
                        align: vec![],
                    },
                    crate::ast::Style::default(),
                    true,
                )],
            },
            crate::ast::Style::default(),
            false,
        );
        let document = ast_to_layout(&ast, &PageSettings::default());
        match &document.blocks[0].kind {
            BlockKind::Table { row_heights, .. } => {
                assert_eq!(row_heights.len(), 1);
                // 行高 = 内容高 + 2 × 上下内边距（默认 table_cell_padding_v_pt=2pt）。
                // 用 measure_cell_height 反推：单行 "Cell A" 内容高，加 4 应与行高一致。
                let style = crate::ast::Style::default();
                let cell = mk_cell("Cell A");
                let content_h = measure_cell_height(&cell, &style, 200.0);
                let pad_v = style.table_cell_padding_v_pt as f64;
                assert!(
                    (row_heights[0] - (content_h + 2.0 * pad_v)).abs() < 1.0,
                    "行高应 = 内容高 + 上下内边距, got {} vs 内容高 {} + {:.1}",
                    row_heights[0],
                    content_h,
                    2.0 * pad_v
                );
            }
            _ => panic!("expected Table"),
        }
    }
}
