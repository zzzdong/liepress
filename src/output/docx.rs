//! DOCX 输出后端。
//!
//! 消费 [`crate::ast::Node`]（Styled AST，保留语义），用 `docx-rs` 生成结构化
//! Word 文档。DOCX 是流式/可编辑格式，不消费已布局的 `document::Document`
//! （其 `Paragraph` 绑定 parley 字形坐标），而是从 `ast::Node` 重建，保留
//! 标题/列表/表格等语义。

use crate::ast::{FontStyle, FontWeight, Node, NodeKind};
use crate::color::Color;
use docx_rs::{
    Document, Docx, Paragraph, Pic, Run, Style, StyleType, Styles, Table, TableCell, TableRow,
};

/// 把带样式的 AST 根节点转换为 DOCX 字节（完整 .docx zip 包）。
pub fn node_to_docx(root: &Node) -> crate::error::Result<Vec<u8>> {
    let doc = emit_children(Document::new(), std::slice::from_ref(root));
    let docx = Docx::new().document(doc).styles(build_styles());
    // 用 Cursor<Vec<u8>> 提供 Write + Seek（zip 打包需要）
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    docx.pack(&mut cursor)
        .map_err(|e| crate::error::Error::RenderError(format!("docx pack: {}", e)))?;
    Ok(buf)
}

/// 构建 Word 样式表：标题（Heading1-6）与列表（ListParagraph）。
///
/// `Style::size` 使用 Word 的半磅单位（sz）：`pt * 2`。
fn build_styles() -> Styles {
    let mut styles = Styles::new();
    for level in 1..=6u8 {
        let pt = match level {
            1 => 22.0,
            2 => 18.0,
            3 => 15.0,
            4 => 13.0,
            5 => 12.0,
            _ => 11.0,
        };
        styles = styles.add_style(
            Style::new(format!("Heading{}", level), StyleType::Paragraph)
                .name(format!("Heading {}", level))
                .size((pt * 2.0) as usize)
                .bold(),
        );
    }
    // 列表段落样式：左侧缩进（0.25in = 360 twips）
    styles = styles.add_style(
        Style::new("ListParagraph", StyleType::Paragraph)
            .name("List Paragraph")
            .indent(
                Some(720),
                Some(docx_rs::SpecialIndentType::Hanging(360)),
                Some(0),
                None,
            ),
    );
    styles
}

/// AST 颜色 → docx 颜色字符串（"RRGGBB"）。
fn color_hex(c: &Color) -> String {
    format!("{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

/// 从 `ast::Style` 构造一个 Run（应用字体族/字号/颜色/字重/字形）。
fn run_from_style(text: &str, style: &crate::ast::Style) -> Run {
    let mut run = Run::new();
    let family = style
        .font_family
        .first()
        .cloned()
        .unwrap_or_else(|| "sans-serif".to_string());
    run = run
        .add_text(text.to_string())
        .fonts(docx_rs::RunFonts::new().ascii(&family).east_asia(&family));
    let size_half = (style.font_size_pt * 2.0) as usize;
    if size_half > 0 {
        run = run.size(size_half);
    }
    run = run.color(color_hex(&style.color));
    if style.font_weight == FontWeight::Bold {
        run = run.bold();
    }
    if style.font_style == FontStyle::Italic {
        run = run.italic();
    }
    run
}

/// 遍历节点序列，累积为 DOCX Document（保序）。
fn emit_children(doc: Document, nodes: &[Node]) -> Document {
    let mut doc = doc;
    for n in nodes {
        doc = emit_node(doc, n);
    }
    doc
}

/// 处理单个节点（块级），返回累积后的 Document。
fn emit_node(doc: Document, n: &Node) -> Document {
    match &n.kind {
        NodeKind::Document { children } | NodeKind::Container { children, .. } => {
            emit_children(doc, children)
        }
        NodeKind::Heading { level, children } => {
            let style_id: &str = &format!("Heading{}", level);
            let p = emit_inline_children(Paragraph::new().style(style_id), children);
            doc.add_paragraph(p)
        }
        NodeKind::Paragraph { children } | NodeKind::Center { children } => {
            let p = emit_inline_children(Paragraph::new(), children);
            doc.add_paragraph(p)
        }
        NodeKind::List {
            children,
            ordered,
            start,
        } => {
            let mut num = start.unwrap_or(1);
            let mut doc = doc;
            for item in children {
                let marker = if *ordered {
                    let m = format!("{}. ", num);
                    num += 1;
                    m
                } else {
                    "•  ".to_string()
                };
                // 列表段落套用 ListParagraph 样式（左侧缩进）
                let marker_run = run_from_style(&marker, &item.style);
                let mut p = Paragraph::new().style("ListParagraph").add_run(marker_run);
                p = emit_list_item(p, item);
                doc = doc.add_paragraph(p);
            }
            doc
        }
        NodeKind::ListItem { children } => {
            let marker = "•  ".to_string();
            let marker_run = run_from_style(&marker, &n.style);
            let mut p = Paragraph::new().style("ListParagraph").add_run(marker_run);
            p = emit_inline_children(p, children);
            doc.add_paragraph(p)
        }
        NodeKind::TaskListItem { checked, children } => {
            let prefix = if *checked { "[x] " } else { "[ ] " };
            let p = emit_inline_children(
                Paragraph::new().add_run(Run::new().add_text(prefix)),
                children,
            );
            doc.add_paragraph(p)
        }
        NodeKind::Blockquote { children } => emit_children(doc, children),
        NodeKind::CodeBlock { code, .. } => {
            let mono = || {
                docx_rs::RunFonts::new()
                    .ascii("Consolas")
                    .east_asia("Consolas")
            };
            let mut p = Paragraph::new();
            for (i, line) in code.lines().enumerate() {
                if i > 0 {
                    p = p.add_run(Run::new().add_text("\n"));
                }
                p = p.add_run(Run::new().add_text(line).fonts(mono()));
            }
            doc.add_paragraph(p)
        }
        NodeKind::ThematicBreak => doc.add_paragraph(Paragraph::new()),
        NodeKind::Table { children, .. } => emit_table(doc, children),
        NodeKind::DefinitionList { items } => {
            let mut doc = doc;
            for item in items {
                let p = emit_inline_runs_bold(Paragraph::new(), &item.term);
                doc = doc.add_paragraph(p);
                doc = emit_children(doc, &item.definition);
            }
            doc
        }
        NodeKind::FootnoteDef { children, .. } => emit_children(doc, children),
        // 内联节点出现在块级位置时，包成段落
        _ => {
            let p = emit_inline_runs(Paragraph::new(), n);
            doc.add_paragraph(p)
        }
    }
}

/// 列表项：可能是内联内容（直接加 run）或嵌套块。
fn emit_list_item(p: Paragraph, item: &Node) -> Paragraph {
    match &item.kind {
        NodeKind::ListItem { children } => {
            let mut p = p;
            for c in children {
                match &c.kind {
                    NodeKind::Paragraph { children } => p = emit_inline_children(p, children),
                    _ => p = emit_inline_runs(p, c),
                }
            }
            p
        }
        _ => emit_inline_runs(p, item),
    }
}

/// 表格：`rows` 是 TableRow 节点。
fn emit_table(doc: Document, rows: &[Node]) -> Document {
    let mut rows_out: Vec<TableRow> = Vec::new();
    for row_node in rows {
        if let NodeKind::TableRow { children: cells } = &row_node.kind {
            let mut cells_out: Vec<TableCell> = Vec::new();
            for cell in cells {
                let p = emit_inline_children(Paragraph::new(), std::slice::from_ref(cell));
                cells_out.push(TableCell::new().add_paragraph(p));
            }
            rows_out.push(TableRow::new(cells_out));
        }
    }
    doc.add_table(Table::new(rows_out))
}

/// 把节点序列作为内联 run 追加到段落（返回累积后的段落）。
fn emit_inline_children(p: Paragraph, nodes: &[Node]) -> Paragraph {
    let mut p = p;
    for n in nodes {
        p = emit_inline_runs(p, n);
    }
    p
}

/// 把单个节点作为内联 run 追加到段落（处理内联语义节点）。
/// 文本/行内代码等 run 应用 `ast::Style` 的字体族/字号/颜色/字重/字形。
fn emit_inline_runs(p: Paragraph, n: &Node) -> Paragraph {
    match &n.kind {
        NodeKind::Text { text } => p.add_run(run_from_style(text, &n.style)),
        NodeKind::Strong { children } => emit_inline_bold(p, children),
        NodeKind::Emphasis { children } => emit_inline_italic(p, children),
        NodeKind::InlineCode { code } => p.add_run(
            run_from_style(code, &n.style).fonts(
                docx_rs::RunFonts::new()
                    .ascii("Consolas")
                    .east_asia("Consolas"),
            ),
        ),
        NodeKind::Link { children, .. } => emit_inline_children(p, children),
        NodeKind::Delete { children }
        | NodeKind::Subscript { children }
        | NodeKind::Superscript { children }
        | NodeKind::Span { children } => emit_inline_children(p, children),
        NodeKind::LineBreak => p.add_run(Run::new().add_text("\n")),
        NodeKind::Paragraph { children } | NodeKind::FootnoteDef { children, .. } => {
            emit_inline_children(p, children)
        }
        NodeKind::Image { src, alt, .. } => emit_image(p, src, alt),
        _ => {
            let t = n.text_content();
            if t.is_empty() {
                p
            } else {
                p.add_run(run_from_style(&t, &n.style))
            }
        }
    }
}

/// 嵌入图片：若 `src` 为 data URI（`data:image/...;base64,...`）则解码并嵌入为 `Pic`。
///
/// **图片缩放（参考 PDF）**：解码原始像素尺寸，按「适合页宽」缩放 ——
/// 目标宽度不超过内容宽（默认 A4 边距下的内容宽），高度按宽高比保持，
/// 避免大图在 Word 中按原始像素超宽显示。
fn emit_image(p: Paragraph, src: &str, alt: &str) -> Paragraph {
    let bytes = decode_data_uri(src);
    if bytes.is_empty() {
        // 无字节时回退为 alt 文本
        return p.add_run(Run::new().add_text(alt.to_string()));
    }
    // 适合页宽缩放：内容宽（pt），默认 A4（595.3pt）减 2×1in 边距
    const CONTENT_WIDTH_PT: f64 = 451.0;
    // 96dpi 下 1px = 0.75pt；1pt = 12700 EMU
    let px_to_pt = 0.75;
    let pt_to_emu = 12700.0;

    use image::GenericImageView;
    match image::load_from_memory(&bytes) {
        Ok(img) => {
            let (w_px, h_px) = img.dimensions();
            let w_pt = w_px as f64 * px_to_pt;
            let h_pt = h_px as f64 * px_to_pt;
            let (tw_pt, th_pt) = if w_pt > CONTENT_WIDTH_PT {
                (CONTENT_WIDTH_PT, h_pt * CONTENT_WIDTH_PT / w_pt)
            } else {
                (w_pt, h_pt)
            };
            let w_emu = (tw_pt * pt_to_emu) as u32;
            let h_emu = (th_pt * pt_to_emu) as u32;
            let pic = Pic::new(&bytes).size(w_emu, h_emu);
            p.add_run(Run::new().add_image(pic))
        }
        Err(_) => {
            // 解码失败：按原始尺寸嵌入
            p.add_run(Run::new().add_image(Pic::new(&bytes)))
        }
    }
}

/// 解码 data URI 为图片字节（支持 `data:image/<fmt>;base64,<payload>`）。
/// 前缀大小写不敏感，但 payload 保留原始大小写（base64 区分大小写）。
fn decode_data_uri(src: &str) -> Vec<u8> {
    let lower = src.to_ascii_lowercase();
    // 定位 ";base64," 在 lower 中的字节偏移，再从原串切出 payload。
    let b64marker = ";base64,";
    let Some(idx) = lower.find(b64marker) else {
        return Vec::new();
    };
    let payload = &src[idx + b64marker.len()..];
    use base64::Engine;
    use base64::engine::general_purpose::{STANDARD, URL_SAFE};
    STANDARD
        .decode(payload)
        .or_else(|_| URL_SAFE.decode(payload))
        .unwrap_or_default()
}

/// 加粗内联（在节点样式基础上加粗）。
fn emit_inline_bold(p: Paragraph, nodes: &[Node]) -> Paragraph {
    let mut p = p;
    for n in nodes {
        p = match &n.kind {
            NodeKind::Text { text } => p.add_run(run_from_style(text, &n.style).bold()),
            NodeKind::Emphasis { children } => emit_inline_bold_italic(p, children),
            _ => emit_inline_runs(p, n),
        };
    }
    p
}

/// 斜体内联（在节点样式基础上斜体）。
fn emit_inline_italic(p: Paragraph, nodes: &[Node]) -> Paragraph {
    let mut p = p;
    for n in nodes {
        p = match &n.kind {
            NodeKind::Text { text } => p.add_run(run_from_style(text, &n.style).italic()),
            NodeKind::Strong { children } => emit_inline_bold_italic(p, children),
            _ => emit_inline_runs(p, n),
        };
    }
    p
}

/// 加粗 + 斜体。
fn emit_inline_bold_italic(p: Paragraph, nodes: &[Node]) -> Paragraph {
    let mut p = p;
    for n in nodes {
        p = match &n.kind {
            NodeKind::Text { text } => p.add_run(run_from_style(text, &n.style).bold().italic()),
            _ => emit_inline_runs(p, n),
        };
    }
    p
}

/// 定义列表术语（加粗）。
fn emit_inline_runs_bold(p: Paragraph, nodes: &[Node]) -> Paragraph {
    let mut p = p;
    for n in nodes {
        p = match &n.kind {
            NodeKind::Text { text } => p.add_run(run_from_style(text, &n.style).bold()),
            _ => emit_inline_runs(p, n),
        };
    }
    p
}
