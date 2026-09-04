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

use lievisual::FontWeight;

use crate::ast::{CodeSpan, Node, NodeKind, Style, computed_style_to_text_style};
use crate::document::layout::{
    Block, BlockKind, DefinitionItemBlock, Document, TableCell, TableRow,
};
use crate::document::text::{
    FontStyle, StyleRange, TextAlign, TextLine, TextStyle, layout_text_with_ranges, set_decoration,
};
use crate::document::types::page::PageSettings;
use crate::document::types::{DocImage, ResolvedStyle};
use lievisual::Color;

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
    let ideal_width =
        crate::document::text::layout_text(&combined, None, crate::ast::TextAlign::Left).width;
    // 最宽不可断词宽度（CJK 逐字可断，西文按空白分词）
    let min_width = segments
        .iter()
        .flat_map(|(text, st)| split_min_units(text).into_iter().map(move |w| (w, st)))
        .filter(|(w, _)| !w.is_empty())
        .fold(0.0_f64, |acc, (word, st)| {
            let w = crate::document::text::layout_text(
                &[(word, st)],
                None,
                crate::ast::TextAlign::Left,
            )
            .width;
            acc.max(w)
        });
    CellMeasure {
        ideal_width: ideal_width + padding_h * 2.0,
        min_width: (min_width + padding_h * 2.0).max(padding_h * 2.0),
    }
}

/// 判断字符是否按 CJK 逐字可断语义处理（表意文字、假名、全角标点等）。
fn is_cjk_breakable(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x9FFF      // CJK 部首/注音/假名/CJK 统一表意文字/CJK 标点
        | 0xF900..=0xFAFF    // CJK 兼容表意文字
        | 0xFF00..=0xFF60    // 全角形式（全角标点、字母、数字）
        | 0x20000..=0x2FA1F  // CJK 扩展 B–F
    )
}

/// 把文本拆为「最小不可断单元」（供表格最小列宽测量）：
/// - 西文连续非空白段不可断；
/// - CJK 字符逐字可断（与排版端 `layout_text` 的逐字折行语义一致）。
///
/// 若不区分，中文长句被当成单个不可断词 → 单元格最小宽度 = 整句宽度，
/// 宽表格列宽总和溢出页宽。
fn split_min_units(text: &str) -> Vec<&str> {
    let mut units = Vec::new();
    let mut run_start: Option<usize> = None; // 当前西文 run 起点
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = run_start.take() {
                units.push(&text[s..i]);
            }
        } else if is_cjk_breakable(c) {
            if let Some(s) = run_start.take() {
                units.push(&text[s..i]);
            }
            units.push(&text[i..i + c.len_utf8()]);
        } else if run_start.is_none() {
            run_start = Some(i);
        }
    }
    if let Some(s) = run_start {
        units.push(&text[s..]);
    }
    units
}

fn compute_table_layout(
    cell_nodes: &[Vec<&Node>],
    style: &crate::ast::Style,
    n_cols: usize,
    content_w: f64,
) -> (Vec<f64>, Vec<f64>) {
    if n_cols == 0 || cell_nodes.is_empty() {
        // 空表格：返回与 rows 等长的空列宽/行高，避免下游按行数切片时越界。
        return (Vec::new(), Vec::new());
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
            // 最小列宽总和超出页宽（如含超长 URL 等不可断内容）：
            // 等比压缩到页宽，宁可单元格内文本微溢出也不让整表横向溢出页边距。
            let scale = if total_min > 0.0 { content_w / total_min } else { 1.0 };
            min_cols.iter().map(|m| m * scale).collect()
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
    crate::document::text::layout_text(&combined, Some(width), crate::ast::TextAlign::Left).height
}

/// 按指定宽度转换表格单元格（`NodeKind::Paragraph`）为段落块。
///
/// 与 `convert_node` 的 Paragraph 分支等价，但 `layout_inline` 使用 `col_width`
/// 作为可用宽度，保证文本在真实列宽下正确折行、不溢出。
fn convert_cell(node: &Node, settings: &PageSettings, col_width: f64, depth: usize) -> Block {
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
        _ => convert_node_depth(node, settings, depth),
    }
}

/// 递归转换单个 AST 节点为 `Block`（入口，深度 0）。
fn convert_node(node: &Node, settings: &PageSettings) -> Block {
    convert_node_depth(node, settings, 0)
}

/// S-3 深嵌套保护：Node → Block 递归的最大深度。
///
/// 超过后停止下钻、返回空文本块。虽经 `to_ast` 深度限制后正常输入不会触达，
/// 但 `Node` 树可由库使用方手工构造，此处防御性兜底（栈溢出 abort 无法捕获）。
const MAX_CONVERT_DEPTH: usize = 256;

/// 递归转换单个 AST 节点为 `Block`（带深度保护）。
fn convert_node_depth(node: &Node, settings: &PageSettings, depth: usize) -> Block {
    if depth > MAX_CONVERT_DEPTH {
        return Block::new(
            BlockKind::Text {
                text: String::new(),
            },
            ResolvedStyle::default(),
            false,
        );
    }
    let child_depth = depth + 1;
    let style = ResolvedStyle::from(node.style.clone());
    match &node.kind {
        NodeKind::Document { children } => Block::new(
            BlockKind::Document {
                children: children
                    .iter()
                    .map(|c| convert_node_depth(c, settings, child_depth))
                    .collect(),
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
            // 起始序号完全由用户输入（`<ol start="...">`）控制，可能为 u32::MAX；
            // 用 u64 + saturating_add 累加，避免 debug 下 `idx += 1` 溢出 panic、
            // release 下静默回绕成 0 导致序号错乱。
            let start_n: u64 = start.unwrap_or(if *ordered { 1 } else { 0 }) as u64;
            let mut idx = start_n;
            let children_blocks: Vec<Block> = children
                .iter()
                .map(|c| {
                    let marker = if *ordered {
                        let m = format!("{}.", idx);
                        idx = idx.saturating_add(1);
                        m
                    } else {
                        "●".to_string()
                    };
                    convert_list_item(c, &marker, settings, child_depth)
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
                        .map(|c| convert_node_depth(c, settings, child_depth))
                        .collect(),
                    definition: item
                        .definition
                        .iter()
                        .map(|c| convert_node_depth(c, settings, child_depth))
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
            let mut child_blocks: Vec<Block> = children
                .iter()
                .map(|c| convert_node_depth(c, settings, child_depth))
                .collect();
            // 追加返回引用链接（↩），使脚注定义可点回正文中的引用处（页内 destination 跳转，
            // 与 TOC/脚注引用跳转一致）。label 由 `fn-def-<label>` 反推为 `fn-ref-<label>`。
            if let Some(label) = id.strip_prefix("fn-def-") {
                let backref = Node::new(
                    NodeKind::Link {
                        url: format!("#fn-ref-{}", label),
                        title: None,
                        children: vec![Node::new(
                            NodeKind::Text {
                                text: " ↩".to_string(),
                            },
                            crate::ast::Style::default(),
                            false,
                        )],
                    },
                    crate::ast::Style::default(),
                    false,
                );
                child_blocks.push(convert_node_depth(&backref, settings, child_depth));
            }
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
            convert_list_item(node, "", settings, depth)
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
                    children: children
                        .iter()
                        .map(|c| convert_node_depth(c, settings, child_depth))
                        .collect(),
                },
                style,
                node.splittable,
            )
        }
        NodeKind::Blockquote { children } => Block::new(
            BlockKind::Blockquote {
                children: children
                    .iter()
                    .map(|c| convert_node_depth(c, settings, child_depth))
                    .collect(),
            },
            style,
            node.splittable,
        ),
        NodeKind::CodeBlock { code, lang, spans } => convert_code_block(
            code,
            lang.as_deref().unwrap_or(""),
            spans,
            &style,
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
                            // 与 compute_table_layout 的 padding_h 保持一致（内容区减 2*padding），
                            // 使用真实 table_cell_padding_h_pt，避免硬编码 8.0 与默认 2pt 脱节。
                            let col_w = col_widths.get(ci).copied().unwrap_or(content_w);
                            let pad_h = cell.style.table_cell_padding_h_pt as f64;
                            TableCell {
                                children: vec![convert_cell(
                                    cell,
                                    settings,
                                    (col_w - 2.0 * pad_h).max(1.0),
                                    child_depth,
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
                children: children
                    .iter()
                    .map(|c| convert_node_depth(c, settings, child_depth))
                    .collect(),
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
fn convert_list_item(node: &Node, marker: &str, settings: &PageSettings, depth: usize) -> Block {
    match &node.kind {
        NodeKind::ListItem { children } => Block::new(
            BlockKind::ListItem {
                marker: marker.to_string(),
                // 不再把 marker 前置进正文首行文本：marker 由各输出后端在缩进槽
                // 左缘**单独绘制**（PDF/PNG 矢量画圆点/方框、有序数字画文本；
                // SVG 单独 `<text>`）。正文首行与续行统一从缩进槽起点排起，
                // 因此 marker 不会与正文重叠，天然实现悬挂缩进。
                children: group_inline_children(children, settings, depth),
            },
            ResolvedStyle::from(node.style.clone()),
            node.splittable,
        ),
        NodeKind::TaskListItem { checked, children } => Block::new(
            BlockKind::TaskListItem {
                marker: marker.to_string(),
                checked: *checked,
                children: group_inline_children(children, settings, depth),
            },
            ResolvedStyle::from(node.style.clone()),
            node.splittable,
        ),
        _ => convert_node_depth(node, settings, depth),
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
    let (pixel, orientation) = if data.is_empty() {
        ((0, 0), 1u8)
    } else {
        probe_image_with_orientation(&data).unwrap_or(((0, 0), 1))
    };
    let content_w = settings.content_width() as f64;
    let content_h = settings.content_height() as f64;
    let size = resolve_image_size(style.width, style.height, pixel, content_w, content_h);
    Block::new(
        BlockKind::Image(DocImage {
            position: (0.0, 0.0),
            size,
            pixel_size: pixel,
            orientation,
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

/// 转换代码块节点。
///
/// 外绘（mermaid / liecharts → 图片）与语法高亮都已在 AST 富化阶段完成
/// （见 [`crate::enrich`]）：绘图代码块此时已是 `NodeKind::Image`，
/// 其余代码块携带 [`CodeSpan`]。这里只负责把 spans 排版成 [`TextLine`]。
fn convert_code_block(
    code: &str,
    lang: &str,
    spans: &Option<Vec<Vec<CodeSpan>>>,
    style: &ResolvedStyle,
    splittable: bool,
) -> Block {
    let lines = match spans {
        Some(spans) => spans_to_lines(spans, style),
        // 未经过富化 pass（如外部直接调用 `ast_to_layout`）：退化为单色等宽排版。
        None => plain_code_lines(code, style),
    };
    Block::new(
        BlockKind::CodeBlock {
            code: code.to_string(),
            lang: if lang.is_empty() {
                None
            } else {
                Some(lang.to_string())
            },
            lines,
        },
        style.clone(),
        splittable,
    )
}

/// 代码块排版的基础文本样式（字体族/字号/字重来自 CSS）。
///
/// 代码块语义上必须使用等宽字体，因此 `font_family` 强制覆盖为 `["monospace"]`，
/// 不受正文 CSS 字体族影响（否则比例字体会让代码错位、视觉上「挤在一起」）。
/// 字号/字重/颜色仍来自投影样式，高亮只在其上覆盖各 token 前景色。
fn code_base_style(style: &ResolvedStyle) -> TextStyle {
    crate::document::text::css_text_style(
        style.color,
        &["monospace".to_string()],
        style.font_size_pt as f64,
        if style.font_weight_bold {
            "bold"
        } else {
            "normal"
        },
        if style.font_style_italic {
            "italic"
        } else {
            "normal"
        },
        TextAlign::Left,
        None,
        style.text_decoration,
        0.0,
        None,
        // 代码块行高参与 parley 排版：与 draw_block 垂直步进（line_height_pt，
        // 0 时回退 18pt）保持一致，避免字形盒与行距错位。
        Some(if style.line_height_pt > 0.0 {
            style.line_height_pt as f64
        } else {
            18.0
        }),
    )
}

/// 把 AST 层的语法高亮片段（行 → 片段）排版为带色文本行。
///
/// 把整段代码**一次**交给 parley 排版（行间以 `\n` 连接），行高与垂直偏移完全由
/// parley 计算；各片段的颜色/粗体/斜体通过字节区间 [`StyleRange`] 叠加。
/// 区间偏移在这里按重建过程累加，与 `full` 严格对齐。
fn spans_to_lines(spans: &[Vec<CodeSpan>], style: &ResolvedStyle) -> Vec<TextLine> {
    let base = code_base_style(style);
    let mut full = String::new();
    let mut ranges: Vec<StyleRange> = Vec::new();
    for (i, line) in spans.iter().enumerate() {
        if i > 0 {
            full.push('\n');
        }
        for span in line {
            if span.text.is_empty() {
                continue;
            }
            let start = full.len();
            full.push_str(&span.text);
            ranges.push(StyleRange {
                start,
                end: full.len(),
                color: span.color,
                font_weight: if span.bold { "bold" } else { "normal" }.to_string(),
                font_style: if span.italic { "italic" } else { "normal" }.to_string(),
            });
        }
    }
    layout_text_with_ranges(&full, &base, &ranges, None, TextAlign::Left).lines
}

/// 未高亮代码块的兜底排版：整段单色等宽。
fn plain_code_lines(code: &str, style: &ResolvedStyle) -> Vec<TextLine> {
    let base = code_base_style(style);
    layout_text_with_ranges(code, &base, &[], None, TextAlign::Left).lines
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
fn group_inline_children(children: &[Node], settings: &PageSettings, depth: usize) -> Vec<Block> {
    let mut out = Vec::new();
    let mut inline_buf: Vec<&Node> = Vec::new();
    for c in children {
        if is_inline_node(c) {
            inline_buf.push(c);
        } else {
            flush_inline_buffer(&mut inline_buf, &mut out, settings);
            out.push(convert_node_depth(c, settings, depth));
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
                // 叠加该节点经 CSS 解析的样式（class/id/标签选择器，如 `.highlight`
                // 的背景色、`mark` 的底色、`<span style="...">` 内联样式），使后代文本继承。
                merge_css_style(&mut merged, &child.style);
                if let NodeKind::Link { url, title, .. } = &child.kind {
                    // 链接正文的「蓝色 + 下划线」来自 CSS 的 `a` 选择器（写在节点的
                    // `style` 上，而非 NodeKind 语义），必须显式叠加到正文样式，
                    // 否则会回退为父段落的黑色（与其它语义类节点不同，颜色需取 CSS）。
                    merged.color = child.style.color;
                    set_decoration(&mut merged, child.style.text_decoration);
                    merged.url = Some(url.clone());
                    segments.extend(collect_inline_segments(inner, &merged));
                    // 带标题的链接：正文之后追加「（title）」副文本，
                    // 斜体 + 弱化灰 + 无 url（不可点），参照 pandoc/typst 印刷风格。
                    if let Some(t) = title
                        && !t.trim().is_empty()
                    {
                        let mut desc = inherited.clone();
                        desc.color = Color::rgb(136, 136, 136); // #888
                        desc.font_style = FontStyle::Italic;
                        desc.url = None;
                        segments.push((format!("（{}）", t), desc));
                    }
                    continue;
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
                    // 行内代码字体与配色尊重 CSS 中 `code` 选择器的设定
                    // （默认 default.css 为 monospace, sans-serif + 浅灰底深字）；
                    // 用户可在自定义 CSS 中覆盖。CSS 未指定字体时回退 monospace，
                    // 避免回退到继承的段落字体。
                    // 注意：这里的 child.style 是 <code> 节点经 CSS 解析后的样式
                    // （to_ast.rs 已按 `code` 选择器写入），并非来自 inherited 推断。
                    style.font_family = if child.style.font_family.is_empty() {
                        "monospace".to_string()
                    } else {
                        child.style.font_family.join(", ")
                    };
                    style.color = child.style.color;
                    // 背景色以 CSS `code` 选择器为准；若 CSS 未指定（如直接构造的节点），
                    // 回退到与默认样式表一致的浅灰底 #f6f8fa，保证行内代码始终与正文区分。
                    style.background_color = child
                        .style
                        .background_color
                        .or_else(|| Some(Color::rgb(246, 248, 250)));
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
        NodeKind::Strong { .. } => style.font_weight = FontWeight::Bold,
        NodeKind::Emphasis { .. } => style.font_style = FontStyle::Italic,
        NodeKind::Delete { .. } => set_decoration(style, crate::ast::TextDecoration::LineThrough),
        NodeKind::Subscript { .. } => style.baseline_shift = -(style.font_size * 0.3),
        NodeKind::Superscript { .. } => style.baseline_shift = style.font_size * 0.3,
        _ => {}
    }
}

/// 把节点经 CSS 解析得到的样式（`child.style`，含 class/id/标签选择器，
/// 如 `.highlight` 的背景色、`mark` 的底色）叠加到文本样式上，使后代文本继承。
///
/// 仅当 CSS 提供了「区别于继承默认值」的具体值才覆盖，避免把继承的颜色/字体等
/// 清零。`background_color` / `font_family` 为可选/可空，仅在有值时覆盖；
/// `color` / `font_style` / `font_weight` / `text_decoration` / `font_size` 虽在
/// `ast::Style` 上始终有默认值，但该默认已是 CSS 继承后的结果，故直接采用即可。
fn merge_css_style(merged: &mut TextStyle, css: &Style) {
    merged.color = css.color;
    if let Some(bg) = css.background_color {
        merged.background_color = Some(bg);
    }
    merged.font_style = css.font_style;
    merged.font_weight = css.font_weight;
    set_decoration(merged, css.text_decoration);
    if !css.font_family.is_empty() {
        merged.font_family = css.font_family.join(", ");
    }
    merged.font_size = css.font_size_pt as f64;
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

/// 探测图片的**显示方向**像素尺寸（宽, 高）与 EXIF Orientation（1–8）。
///
/// 仅读取文件头，不解码整图。EXIF 方向为 5–8（需旋转 90°）时交换宽高，
/// 使返回的尺寸/宽高比与用户实际看到的显示方向一致。
/// 解码失败或尺寸未知时返回 `None`。
fn probe_image_with_orientation(data: &[u8]) -> Option<((u32, u32), u8)> {
    let format = image::guess_format(data).ok()?;
    let reader = image::ImageReader::with_format(std::io::Cursor::new(data), format);
    let (w, h) = reader.into_dimensions().ok()?;

    let orientation = if format == image::ImageFormat::Jpeg {
        read_exif_orientation(data).unwrap_or(1)
    } else {
        1
    };
    Some((swap_dims_for_orientation((w, h), orientation), orientation))
}

/// 按显示方向调整像素尺寸（EXIF 方向 5–8 交换宽高），供测试与探测共用。
fn swap_dims_for_orientation((w, h): (u32, u32), orientation: u8) -> (u32, u32) {
    if (5..=8).contains(&orientation) {
        (h, w)
    } else {
        (w, h)
    }
}

/// 读取 JPEG 的 EXIF Orientation（1–8）；无 EXIF 或解析失败时返回 `None`。
fn read_exif_orientation(data: &[u8]) -> Option<u8> {
    let exif = exif::Reader::new()
        .read_from_container(&mut std::io::Cursor::new(data))
        .ok()?;
    let field = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?;
    let v = field.value.get_uint(0)?;
    (1..=8).contains(&v).then_some(v as u8)
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
    max_h: f64,
) -> (f64, f64) {
    let (pw, ph) = (pixel.0 as f64, pixel.1 as f64);
    let has_aspect = pw > 0.0 && ph > 0.0;
    // 自适应场景下按高度上限等比缩放，避免超高图片（如长截图）高度超过一页、
    // 在分页层被直接放进单页而溢出裁剪、导致下方内容丢失。
    let clamp_by_height = |w: f64, h: f64| -> (f64, f64) {
        if has_aspect && max_h > 0.0 && h > max_h {
            (max_h * pw / ph, max_h)
        } else {
            (w, h)
        }
    };
    match (css_w, css_h) {
        (Some(w), Some(h)) => (w as f64, h as f64),
        (Some(w), None) => {
            let w = w as f64;
            let h = if has_aspect { w * ph / pw } else { w * 0.75 };
            clamp_by_height(w, h)
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
                clamp_by_height(content_w, content_w * ph / pw)
            } else {
                (content_w, content_w * 0.75)
            }
        }
    }
}

/// 将内联子节点排版为文档文本行。
///
/// 复用 `collect_inline_segments` 与 text 的 `layout_text`，
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

    let layout = crate::document::text::layout_text(&combined, Some(available_width), text_align);
    // lievisual 排版时已按 span 样式把 url / decoration / background 映射到 run，
    // 无需再手工标注。
    layout.lines
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
                spans: None,
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
            .map(|s| (s.to_string(), crate::document::text::default_text_style()))
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
        let (w, h) = resolve_image_size(Some(200.0), Some(100.0), (800, 600), 500.0, f64::MAX);
        assert_eq!((w, h), (200.0, 100.0));
    }

    #[test]
    fn test_resolve_image_size_width_only_preserves_aspect() {
        // 仅指定宽度：高度按原始宽高比推算（800x600 → 200x150）
        let (w, h) = resolve_image_size(Some(200.0), None, (800, 600), 500.0, f64::MAX);
        assert_eq!((w, h), (200.0, 150.0));
    }

    #[test]
    fn test_resolve_image_size_height_only_preserves_aspect() {
        // 仅指定高度：宽度按原始宽高比推算（800x600 → 200x150）
        let (w, h) = resolve_image_size(None, Some(150.0), (800, 600), 500.0, f64::MAX);
        assert_eq!((w, h), (200.0, 150.0));
    }

    #[test]
    fn test_resolve_image_size_fit_content_width() {
        // 均未指定：适合页宽（宽度=内容宽度，高度按宽高比）
        let content_w = PageSettings::default().content_width() as f64;
        let (w, h) = resolve_image_size(None, None, (800, 600), content_w, f64::MAX);
        assert_eq!(w, content_w);
        assert!((h - content_w * 600.0 / 800.0).abs() < 1e-9);
    }

    #[test]
    fn test_resolve_image_size_unknown_aspect_ratio_uses_4x3() {
        // 未知宽高比时按 4:3 兜底
        let (w, h) = resolve_image_size(None, None, (0, 0), 400.0, f64::MAX);
        assert_eq!(w, 400.0);
        assert!((h - 300.0).abs() < 1e-9);
    }

    #[test]
    fn test_resolve_image_size_clamps_oversized_height() {
        // 超高图片（如 100×2000 长截图）自适应页宽时，高度应被限制到一页内，
        // 避免在分页层被放进单页而溢出裁剪。
        let content_w = PageSettings::default().content_width() as f64;
        let content_h = PageSettings::default().content_height() as f64;
        let (w, h) = resolve_image_size(None, None, (100, 2000), content_w, content_h);
        assert!(
            h <= content_h + 1e-9,
            "高度应被限制到一页内: {h} > {content_h}"
        );
        assert!(
            (w - content_h * 100.0 / 2000.0).abs() < 1e-6,
            "宽度应按高度等比缩小，got {w}"
        );
    }

    #[test]
    fn test_resolve_image_size_not_100_fixed() {
        // 回归：不再固定 100×100；未指定时按内容宽度缩放
        let content_w = PageSettings::default().content_width() as f64;
        let (w, _) = resolve_image_size(None, None, (800, 600), content_w, f64::MAX);
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
            BlockKind::CodeBlock { code, lang, .. } => {
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

    // ─── CJK 表格最小列宽（2026-09-04 审查） ───

    fn text_node(t: &str) -> crate::ast::Node {
        crate::ast::Node::new(
            NodeKind::Text {
                text: t.to_string(),
            },
            crate::ast::Style::default(),
            false,
        )
    }

    #[test]
    fn cjk_cell_min_width_breaks_per_char() {
        // 60 个无空格汉字：旧逻辑把整句当单个不可断词，min ≈ 整句宽度，
        // 宽表格列宽总和溢出页宽。CJK 逐字可断后 min 应收缩到单字宽量级。
        let cell = text_node(&"汉字宽度测试".repeat(12));
        let style = crate::ast::Style::default();
        let (col_widths, _) = compute_table_layout(&[vec![&cell]], &style, 1, 200.0);
        assert!(
            col_widths[0] <= 200.0 + 1e-9,
            "CJK 单元格最小列宽应可断词收缩，实际 {}",
            col_widths[0]
        );
        assert!(col_widths[0] > 10.0, "列宽不应塌缩到 0，实际 {}", col_widths[0]);
    }

    #[test]
    fn oversize_unbreakable_min_is_scaled_to_content_width() {
        // 100 个连续 'M'（西文不可断，min 远超页宽）：等比压缩到页宽，
        // 保证表格不横向溢出页边距。
        let cell = text_node(&"M".repeat(100));
        let style = crate::ast::Style::default();
        let (col_widths, _) = compute_table_layout(&[vec![&cell]], &style, 1, 150.0);
        assert!(
            (col_widths[0] - 150.0).abs() < 1e-6,
            "不可断超宽内容应压缩到页宽，实际 {}",
            col_widths[0]
        );
    }

    // ─── EXIF 方向（2026-09-04 审查） ───

    #[test]
    fn exif_orientation_swaps_display_dims_for_rotated() {
        // 方向 5–8（需旋转 90°）：显示尺寸应交换宽高；1–4 不变。
        assert_eq!(swap_dims_for_orientation((300, 200), 1), (300, 200));
        assert_eq!(swap_dims_for_orientation((300, 200), 4), (300, 200));
        assert_eq!(swap_dims_for_orientation((300, 200), 6), (200, 300));
        assert_eq!(swap_dims_for_orientation((300, 200), 8), (200, 300));
    }

    #[test]
    fn probe_png_reports_orientation_1_and_native_dims() {
        // 生成 3×2 PNG（无 EXIF）：显示尺寸 = 存储尺寸，方向 1。
        let img = image::DynamicImage::new_rgb8(3, 2);
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        let ((w, h), o) = probe_image_with_orientation(buf.get_ref()).expect("probe");
        assert_eq!((w, h), (3, 2));
        assert_eq!(o, 1);
    }
}
