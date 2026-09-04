//! DOCX 输出后端。
//!
//! 消费 [`crate::ast::Node`]（Styled AST，保留语义），用 `docx-rs` 生成结构化
//! Word 文档。DOCX 是流式/可编辑格式，不消费已布局的 `document::Document`
//! （其 `Paragraph` 绑定 parley 字形坐标），而是从 `ast::Node` 重建，保留
//! 标题/列表/表格等语义。
//!
//! ## 生成器模式
//!
//! 与 [`crate::output::PdfGenerator`] 一致，采用「构造器 + `generate`」
//! 的生成器模式：页面几何等上下文在构造时注入并持有为字段，遍历逻辑以
//! `&mut self` 方法渐进累积内部 `docx_rs::Document`，替代裸函数 + 逐层
//! 传参的形式参数（早期版本的 `DocxCtx` 线程化写法）。

use crate::ast::{CodeSpan, FontStyle, Node, NodeKind};
use crate::document::types::page::PageSettings;
use docx_rs::{
    BreakType, Document, Docx, Paragraph, Pic, Run, Style, StyleType, Styles, Table, TableCell,
    TableRow,
};
use lievisual::Color;

/// DOCX 生成器：持有页面几何与累积中的 Word 文档，以 `&mut self` 方法
/// 渐进消费 AST 节点。
///
/// DOCX 直接消费 AST（不经过 `document::Document` 布局层），无法享受
/// `from_ast::resolve_image_size` 的按页钳制，故页面几何在构造时注入，
/// 图片显示尺寸按「适合页宽 + 页高上限等比缩小」钳制，与 PDF 端一致。
pub struct DocxGenerator {
    /// 页面几何（图片钳制上限的来源）。
    settings: PageSettings,
    /// 累积中的 Word 文档主体。
    doc: Document,
}

impl DocxGenerator {
    /// 从页面设置构造生成器。
    pub fn new(settings: &PageSettings) -> Self {
        Self {
            settings: settings.clone(),
            doc: Document::new(),
        }
    }

    /// 消费 Styled AST 根节点，生成 DOCX 字节（完整 .docx zip 包）。
    pub fn generate(&mut self, root: &Node) -> crate::error::Result<Vec<u8>> {
        self.emit_children(std::slice::from_ref(root));
        let doc = std::mem::replace(&mut self.doc, Document::new());
        let docx = Docx::new().document(doc).styles(build_styles());
        // 用 Cursor<Vec<u8>> 提供 Write + Seek（zip 打包需要）
        let mut buf = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut buf);
        docx.pack(&mut cursor)
            .map_err(|e| crate::error::Error::RenderError(format!("docx pack: {}", e)))?;
        Ok(buf)
    }

    /// 图片最大显示宽（pt）：页内容宽。
    fn content_w_pt(&self) -> f64 {
        self.settings.content_width() as f64
    }

    /// 图片最大显示高（pt）：页内容高（无限高度模式时为极大值，不钳制）。
    fn max_img_h_pt(&self) -> f64 {
        self.settings.content_height() as f64
    }

    /// 追加段落到内部文档。
    ///
    /// `docx_rs::Document::add_paragraph` 按值消费 `self` 并返回新文档，
    /// 用 `mem::replace` 绕开「从 `&mut self` 移出字段」的借用限制。
    fn add_paragraph(&mut self, p: Paragraph) {
        let doc = std::mem::replace(&mut self.doc, Document::new());
        self.doc = doc.add_paragraph(p);
    }

    /// 追加表格到内部文档（同 [`Self::add_paragraph`] 的 swap 技巧）。
    fn add_table(&mut self, t: Table) {
        let doc = std::mem::replace(&mut self.doc, Document::new());
        self.doc = doc.add_table(t);
    }

    /// 遍历节点序列，追加到内部文档（保序）。
    fn emit_children(&mut self, nodes: &[Node]) {
        for n in nodes {
            self.emit_node(n);
        }
    }

    /// 处理单个节点（块级），追加到内部文档。
    fn emit_node(&mut self, n: &Node) {
        match &n.kind {
            NodeKind::Document { children } | NodeKind::Container { children, .. } => {
                self.emit_children(children)
            }
            NodeKind::Heading { level, children } => {
                let style_id: &str = &format!("Heading{}", level);
                let p = self.emit_inline_children(Paragraph::new().style(style_id), children);
                self.add_paragraph(p);
            }
            NodeKind::Paragraph { children } | NodeKind::Center { children } => {
                let p = self.emit_inline_children(Paragraph::new(), children);
                self.add_paragraph(p);
            }
            NodeKind::List {
                children,
                ordered,
                start,
            } => {
                let mut num = start.unwrap_or(1);
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
                    p = self.emit_list_item(p, item);
                    self.add_paragraph(p);
                }
            }
            NodeKind::ListItem { children } => {
                let marker = "•  ".to_string();
                let marker_run = run_from_style(&marker, &n.style);
                let mut p = Paragraph::new().style("ListParagraph").add_run(marker_run);
                p = self.emit_inline_children(p, children);
                self.add_paragraph(p);
            }
            NodeKind::TaskListItem { checked, children } => {
                let prefix = if *checked { "[x] " } else { "[ ] " };
                let p = self.emit_inline_children(
                    Paragraph::new().add_run(Run::new().add_text(prefix)),
                    children,
                );
                self.add_paragraph(p);
            }
            NodeKind::Blockquote { children } => self.emit_children(children),
            NodeKind::CodeBlock { code, spans, .. } => match spans {
                // AST 富化阶段已产出语法高亮片段：每段一个带颜色的 Run。
                Some(lines) => {
                    let p = emit_code_lines(lines, &n.style);
                    self.add_paragraph(p);
                }
                None => {
                    let mono = mono_font();
                    let mut p = Paragraph::new();
                    for (i, line) in code.lines().enumerate() {
                        if i > 0 {
                            p = p.add_run(Run::new().add_break(BreakType::TextWrapping));
                        }
                        p = p.add_run(Run::new().add_text(line).fonts(mono()));
                    }
                    self.add_paragraph(p);
                }
            },
            NodeKind::ThematicBreak => self.add_paragraph(Paragraph::new()),
            NodeKind::Table { children, .. } => self.emit_table(children),
            NodeKind::DefinitionList { items } => {
                for item in items {
                    let p = self.emit_inline_runs_bold(Paragraph::new(), &item.term);
                    self.add_paragraph(p);
                    self.emit_children(&item.definition);
                }
            }
            NodeKind::FootnoteDef { children, .. } => self.emit_children(children),
            // 内联节点出现在块级位置时，包成段落
            _ => {
                let p = self.emit_inline_runs(Paragraph::new(), n);
                self.add_paragraph(p);
            }
        }
    }

    /// 列表项：可能是内联内容（直接加 run）或嵌套块。
    fn emit_list_item(&self, p: Paragraph, item: &Node) -> Paragraph {
        match &item.kind {
            NodeKind::ListItem { children } => {
                let mut p = p;
                for c in children {
                    match &c.kind {
                        NodeKind::Paragraph { children } => {
                            p = self.emit_inline_children(p, children)
                        }
                        _ => p = self.emit_inline_runs(p, c),
                    }
                }
                p
            }
            _ => self.emit_inline_runs(p, item),
        }
    }

    /// 表格：`rows` 是 TableRow 节点。
    fn emit_table(&mut self, rows: &[Node]) {
        let mut rows_out: Vec<TableRow> = Vec::new();
        for row_node in rows {
            if let NodeKind::TableRow { children: cells } = &row_node.kind {
                let mut cells_out: Vec<TableCell> = Vec::new();
                for cell in cells {
                    let p =
                        self.emit_inline_children(Paragraph::new(), std::slice::from_ref(cell));
                    cells_out.push(TableCell::new().add_paragraph(p));
                }
                rows_out.push(TableRow::new(cells_out));
            }
        }
        self.add_table(Table::new(rows_out));
    }

    /// 把节点序列作为内联 run 追加到段落（返回累积后的段落）。
    fn emit_inline_children(&self, p: Paragraph, nodes: &[Node]) -> Paragraph {
        let mut p = p;
        for n in nodes {
            p = self.emit_inline_runs(p, n);
        }
        p
    }

    /// 把单个节点作为内联 run 追加到段落（处理内联语义节点）。
    /// 文本/行内代码等 run 应用 `ast::Style` 的字体族/字号/颜色/字重/字形。
    fn emit_inline_runs(&self, p: Paragraph, n: &Node) -> Paragraph {
        match &n.kind {
            NodeKind::Text { text } => p.add_run(run_from_style(text, &n.style)),
            NodeKind::Strong { children } => self.emit_inline_bold(p, children),
            NodeKind::Emphasis { children } => self.emit_inline_italic(p, children),
            NodeKind::InlineCode { code } => p.add_run(
                run_from_style(code, &n.style).fonts(
                    docx_rs::RunFonts::new()
                        .ascii("Consolas")
                        .east_asia("Consolas"),
                ),
            ),
            NodeKind::Link {
                children,
                title,
                url: _,
            } => {
                let mut p = self.emit_inline_children(p, children);
                // 带标题的链接：正文之后追加「（title）」副文本（斜体灰字，不可点），
                // 参照 pandoc/typst 印刷风格，与 PDF/PNG/SVG/HTML 三端一致。
                if let Some(t) = title
                    && !t.trim().is_empty()
                {
                    let desc = crate::ast::Style {
                        color: Color::rgb(136, 136, 136), // #888
                        font_style: crate::ast::FontStyle::Italic,
                        ..crate::ast::Style::default()
                    };
                    p = p.add_run(run_from_style(&format!("（{}）", t), &desc));
                }
                p
            }
            NodeKind::Delete { children }
            | NodeKind::Subscript { children }
            | NodeKind::Superscript { children }
            | NodeKind::Span { children } => self.emit_inline_children(p, children),
            NodeKind::LineBreak => p.add_run(Run::new().add_text("\n")),
            NodeKind::Paragraph { children } | NodeKind::FootnoteDef { children, .. } => {
                self.emit_inline_children(p, children)
            }
            NodeKind::Image { src, alt, .. } => self.emit_image(p, src, alt, &n.style),
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
    /// **图片缩放（与 PDF 端一致）**：解码原始像素尺寸后做两级钳制 ——
    /// 1. 宽度不超过内容宽（优先取节点显式 `style.width`，即 AST 外绘 pass 写入的
    ///    内容宽；缺省取页内容宽），高度按宽高比保持；
    /// 2. 高度不超过页内容高——超高图（如长 flowchart）若不钳制，Word 会按完整
    ///    高度嵌入并截断超出页高的部分。
    fn emit_image(&self, p: Paragraph, src: &str, alt: &str, style: &crate::ast::Style) -> Paragraph {
        let bytes = decode_data_uri(src);
        if bytes.is_empty() {
            // 无字节时回退为 alt 文本
            return p.add_run(Run::new().add_text(alt.to_string()));
        }
        let content_width_pt = style
            .width
            .filter(|w| *w > 0.0)
            .map(|w| w as f64)
            .unwrap_or_else(|| self.content_w_pt());
        // 96dpi 下 1px = 0.75pt；1pt = 12700 EMU
        let px_to_pt = 0.75;
        let pt_to_emu = 12700.0;

        use image::GenericImageView;
        match image::load_from_memory(&bytes) {
            Ok(img) => {
                let (w_px, h_px) = img.dimensions();
                let w_pt = w_px as f64 * px_to_pt;
                let h_pt = h_px as f64 * px_to_pt;
                // 1. 宽度钳制
                let (mut tw_pt, mut th_pt) = if w_pt > content_width_pt {
                    (content_width_pt, h_pt * content_width_pt / w_pt)
                } else {
                    (w_pt, h_pt)
                };
                // 2. 高度钳制（等比缩小，与 PDF resolve_image_size 的 clamp_by_height 一致）
                let max_h = self.max_img_h_pt();
                if max_h.is_finite() && th_pt > max_h && th_pt > 0.0 {
                    tw_pt *= max_h / th_pt;
                    th_pt = max_h;
                }
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

    /// 加粗内联（在节点样式基础上加粗）。
    fn emit_inline_bold(&self, p: Paragraph, nodes: &[Node]) -> Paragraph {
        let mut p = p;
        for n in nodes {
            p = match &n.kind {
                NodeKind::Text { text } => p.add_run(run_from_style(text, &n.style).bold()),
                NodeKind::Emphasis { children } => self.emit_inline_bold_italic(p, children),
                _ => self.emit_inline_runs(p, n),
            };
        }
        p
    }

    /// 斜体内联（在节点样式基础上斜体）。
    fn emit_inline_italic(&self, p: Paragraph, nodes: &[Node]) -> Paragraph {
        let mut p = p;
        for n in nodes {
            p = match &n.kind {
                NodeKind::Text { text } => p.add_run(run_from_style(text, &n.style).italic()),
                NodeKind::Strong { children } => self.emit_inline_bold_italic(p, children),
                _ => self.emit_inline_runs(p, n),
            };
        }
        p
    }

    /// 加粗 + 斜体。
    fn emit_inline_bold_italic(&self, p: Paragraph, nodes: &[Node]) -> Paragraph {
        let mut p = p;
        for n in nodes {
            p = match &n.kind {
                NodeKind::Text { text } => {
                    p.add_run(run_from_style(text, &n.style).bold().italic())
                }
                _ => self.emit_inline_runs(p, n),
            };
        }
        p
    }

    /// 定义列表术语（加粗）。
    fn emit_inline_runs_bold(&self, p: Paragraph, nodes: &[Node]) -> Paragraph {
        let mut p = p;
        for n in nodes {
            p = match &n.kind {
                NodeKind::Text { text } => p.add_run(run_from_style(text, &n.style).bold()),
                _ => self.emit_inline_runs(p, n),
            };
        }
        p
    }
}

/// 把带样式的 AST 根节点转换为 DOCX 字节（完整 .docx zip 包）。
///
/// 便捷入口：等价于 `DocxGenerator::new(settings).generate(root)`。
/// `settings` 提供页面几何（图片按页宽/页高钳制显示尺寸）。
pub fn node_to_docx(root: &Node, settings: &PageSettings) -> crate::error::Result<Vec<u8>> {
    let mut generator = DocxGenerator::new(settings);
    generator.generate(root)
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
    // docx-rs 的 `color` 需要不带 `#` 的 6 位十六进制。
    run = run.color(style.color.to_hex().trim_start_matches('#').to_string());
    if style.font_weight == crate::ast::FontWeight::Bold {
        run = run.bold();
    }
    if style.font_style == FontStyle::Italic {
        run = run.italic();
    }
    run
}

/// 高亮代码块：每行若干着色片段，片段之间不换行，行间用真实换行符分隔。
///
/// 字号取 CSS 投影字号（缺省 9pt，与内置 `pre` 规则一致），字体强制等宽。
fn emit_code_lines(lines: &[Vec<CodeSpan>], style: &crate::ast::Style) -> Paragraph {
    let mono = mono_font();
    let size_half = ((if style.font_size_pt > 0.0 {
        style.font_size_pt
    } else {
        9.0
    }) * 2.0) as usize;
    let mut p = Paragraph::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            p = p.add_run(Run::new().add_break(BreakType::TextWrapping));
        }
        for span in line {
            if span.text.is_empty() {
                continue;
            }
            let mut run = Run::new()
                .add_text(span.text.clone())
                .fonts(mono())
                // docx-rs 的 `color` 需要不带 `#` 的 6 位十六进制。
                .color(span.color.to_hex().trim_start_matches('#').to_string());
            if span.bold {
                run = run.bold();
            }
            if span.italic {
                run = run.italic();
            }
            if size_half > 0 {
                run = run.size(size_half);
            }
            p = p.add_run(run);
        }
    }
    p
}

/// 代码块等宽字体（Consolas 西文 + 东亚回退）。
fn mono_font() -> impl Fn() -> docx_rs::RunFonts {
    || {
        docx_rs::RunFonts::new()
            .ascii("Consolas")
            .east_asia("Consolas")
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
