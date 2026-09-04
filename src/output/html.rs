//! `ast::Node` 语义树 → HTML 序列化
//!
//! 方案 Y（双中间层）的核心环节：HTML / DOCX 等流式布局输出直接从
//! `ast::Node` 语义树生成，跳过 `Document`（精确排版层），与 PDF 路径
//! 共享同一棵 `ast::Node`，保证语义一致性。
//!
//! 注意：本序列化器是 `html_to_styled_nodes` 的逆向。由于 `ast::Node`
//! 对部分结构做了简化（如表格单元格 `<th>/<td>` 统一为 `Paragraph`），
//! 序列化结果以 `<td>` 表达单元格，表头语义由 CSS 行首样式近似。这是
//! 当前 `NodeKind` 设计的已知限制，未来可在 `NodeKind` 引入 `TableCell`
//! 节点以精确还原 `<th>`/`<td>`。
//!
//! ## 生成器模式
//!
//! 与 [`crate::output::PdfGenerator`] / [`crate::output::DocxGenerator`]
//! 一致：输出缓冲持有为生成器字段，遍历以 `&mut self` 方法渐进写入，
//! 替代裸函数 + 外置 `&mut String` 缓冲的形式参数写法。

use crate::ast::{Node, NodeKind, Style, TextAlign};

/// HTML 生成器：持有输出缓冲，以 `&mut self` 方法渐进序列化 AST 节点。
pub struct HtmlGenerator {
    out: String,
}

impl Default for HtmlGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl HtmlGenerator {
    /// 构造空的 HTML 生成器。
    pub fn new() -> Self {
        Self { out: String::new() }
    }

    /// 序列化 `ast::Node` 语义树为 HTML 片段（无 `<html>` 外壳）。
    pub fn generate(&mut self, root: &Node) -> String {
        self.serialize_node(root);
        std::mem::take(&mut self.out)
    }

    fn serialize_node(&mut self, node: &Node) {
        let sa = style_attr(&node.style);
        match &node.kind {
            NodeKind::Document { children } => {
                for c in children {
                    self.serialize_node(c);
                }
            }
            NodeKind::Heading { level, children } => {
                let lvl = (*level).clamp(1, 6);
                self.out.push_str(&format!("<h{}{}>", lvl, sa));
                self.serialize_children(children);
                self.out.push_str(&format!("</h{}>", lvl));
            }
            NodeKind::Paragraph { children } => {
                self.out.push_str(&format!("<p{}>", sa));
                self.serialize_children(children);
                self.out.push_str("</p>");
            }
            NodeKind::List {
                ordered,
                start,
                children,
            } => {
                if *ordered {
                    match start {
                        Some(s) if *s != 1 => {
                            self.out.push_str(&format!("<ol start=\"{}\"{}>", s, sa))
                        }
                        _ => self.out.push_str(&format!("<ol{}>", sa)),
                    }
                } else {
                    self.out.push_str(&format!("<ul{}>", sa));
                }
                for c in children {
                    self.serialize_node(c);
                }
                self.out.push_str(if *ordered { "</ol>" } else { "</ul>" });
            }
            NodeKind::ListItem { children } => {
                self.out.push_str(&format!("<li{}>", sa));
                self.serialize_children(children);
                self.out.push_str("</li>");
            }
            NodeKind::TaskListItem { checked, children } => {
                self.out.push_str(&format!("<li{}>", sa));
                if *checked {
                    self.out.push_str("<input checked=\"\" type=\"checkbox\"/> ");
                } else {
                    self.out.push_str("<input type=\"checkbox\"/> ");
                }
                self.serialize_children(children);
                self.out.push_str("</li>");
            }
            NodeKind::DefinitionList { items } => {
                self.out.push_str(&format!("<dl{}>", sa));
                for item in items {
                    self.out.push_str(&format!("<dt{}>", style_attr(&node.style)));
                    for c in &item.term {
                        self.serialize_node(c);
                    }
                    self.out.push_str("</dt>");
                    for c in &item.definition {
                        self.out.push_str(&format!("<dd{}>", style_attr(&node.style)));
                        self.serialize_node(c);
                        self.out.push_str("</dd>");
                    }
                }
                self.out.push_str("</dl>");
            }
            NodeKind::FootnoteDef { id, children } => {
                self.out
                    .push_str(&format!("<div id=\"{}\" class=\"footnote-def\"{}>", id, sa));
                for c in children {
                    self.serialize_node(c);
                }
                self.out.push_str("</div>");
            }
            NodeKind::Image { src, alt, title } => {
                self.out.push_str("<img src=\"");
                self.out.push_str(&escape_attr(src));
                self.out.push_str("\" alt=\"");
                self.out.push_str(&escape_attr(alt));
                if let Some(t) = title {
                    self.out.push_str("\" title=\"");
                    self.out.push_str(&escape_attr(t));
                }
                self.out.push_str("\"/>");
            }
            NodeKind::CodeBlock { code, lang, spans } => {
                self.out.push_str(&format!("<pre{}><code", sa));
                if let Some(l) = lang
                    && !l.is_empty()
                {
                    self.out
                        .push_str(&format!(" class=\"language-{}\"", escape_attr(l)));
                }
                self.out.push('>');
                match spans {
                    // AST 富化阶段已产出语法高亮：每段一个内联着色的 <span>。
                    Some(lines) => {
                        for (i, line) in lines.iter().enumerate() {
                            if i > 0 {
                                self.out.push('\n');
                            }
                            for span in line {
                                if span.text.is_empty() {
                                    continue;
                                }
                                let mut css = format!("color:{}", span.color.to_hex());
                                if span.bold {
                                    css.push_str(";font-weight:bold");
                                }
                                if span.italic {
                                    css.push_str(";font-style:italic");
                                }
                                self.out.push_str(&format!(
                                    "<span style=\"{}\">{}</span>",
                                    css,
                                    escape_html(&span.text)
                                ));
                            }
                        }
                    }
                    None => self.out.push_str(&escape_html(code)),
                }
                self.out.push_str("</code></pre>");
            }
            NodeKind::Blockquote { children } => {
                self.out.push_str(&format!("<blockquote{}>", sa));
                for c in children {
                    self.serialize_node(c);
                }
                self.out.push_str("</blockquote>");
            }
            NodeKind::ThematicBreak => self.out.push_str(&format!("<hr{}/>", sa)),
            NodeKind::Table { children, align } => {
                self.out.push_str(&format!("<table{}>", sa));
                self.serialize_table_rows(children, align);
                self.out.push_str("</table>");
            }
            NodeKind::TableRow { children } => {
                // 由父级 Table 调用 serialize_table_rows 时处理 <tr>；
                // 直接序列化时仍包裹 <tr> 以自洽。
                self.out.push_str(&format!("<tr{}>", sa));
                for c in children {
                    self.serialize_node(c);
                }
                self.out.push_str("</tr>");
            }
            NodeKind::Text { text } => self.out.push_str(&escape_html(text)),
            NodeKind::Strong { children } => {
                self.out.push_str(&format!("<strong{}>", sa));
                self.serialize_children(children);
                self.out.push_str("</strong>");
            }
            NodeKind::Emphasis { children } => {
                self.out.push_str(&format!("<em{}>", sa));
                self.serialize_children(children);
                self.out.push_str("</em>");
            }
            NodeKind::InlineCode { code } => {
                self.out.push_str(&format!("<code{}>", sa));
                self.out.push_str(&escape_html(code));
                self.out.push_str("</code>");
            }
            NodeKind::Link {
                url,
                title,
                children,
            } => {
                self.out.push_str("<a href=\"");
                self.out.push_str(&escape_attr(url));
                if let Some(t) = title {
                    self.out.push_str("\" title=\"");
                    self.out.push_str(&escape_attr(t));
                }
                self.out.push_str(&format!("\"{}>", sa));
                self.serialize_children(children);
                self.out.push_str("</a>");
                // 带标题的链接：正文之后追加「（title）」副文本（斜体灰字，不可点），
                // 与 PDF/PNG/SVG/DOCX 三端一致（pandoc/typst 印刷风格）。
                if let Some(t) = title
                    && !t.trim().is_empty()
                {
                    self.out.push_str(&format!(
                        "<span class=\"link-desc\">（{}）</span>",
                        escape_html(t)
                    ));
                }
            }
            NodeKind::Delete { children } => {
                self.out.push_str(&format!("<del{}>", sa));
                self.serialize_children(children);
                self.out.push_str("</del>");
            }
            NodeKind::Subscript { children } => {
                self.out.push_str(&format!("<sub{}>", sa));
                self.serialize_children(children);
                self.out.push_str("</sub>");
            }
            NodeKind::Superscript { children } => {
                self.out.push_str(&format!("<sup{}>", sa));
                self.serialize_children(children);
                self.out.push_str("</sup>");
            }
            NodeKind::Span { children } => {
                self.out.push_str(&format!("<span{}>", sa));
                self.serialize_children(children);
                self.out.push_str("</span>");
            }
            NodeKind::Center { children } => {
                self.out.push_str(&format!("<center{}>", sa));
                for c in children {
                    self.serialize_node(c);
                }
                self.out.push_str("</center>");
            }
            NodeKind::Container { children } => {
                self.out.push_str(&format!("<div{}>", sa));
                for c in children {
                    self.serialize_node(c);
                }
                self.out.push_str("</div>");
            }
            NodeKind::LineBreak => self.out.push_str("<br/>"),
        }
    }

    /// 序列化子节点序列。
    fn serialize_children(&mut self, children: &[Node]) {
        for c in children {
            self.serialize_node(c);
        }
    }

    /// 表格行序列化：当前 `NodeKind` 把 `<th>/<td>` 都简化为 `Paragraph`，
    /// 故统一以 `<td>` 输出；`align` 通过 `style="text-align:..."` 表达。
    fn serialize_table_rows(&mut self, rows: &[Node], align: &[TextAlign]) {
        for row in rows {
            if let NodeKind::TableRow { children } = &row.kind {
                self.out.push_str("<tr>");
                for (i, cell) in children.iter().enumerate() {
                    let align_attr = align.get(i).map(align_to_style).unwrap_or_default();
                    self.out.push_str(&format!("<td{}>", align_attr));
                    self.serialize_node(cell);
                    self.out.push_str("</td>");
                }
                self.out.push_str("</tr>");
            } else {
                self.serialize_node(row);
            }
        }
    }
}

/// 将 `ast::Node` 语义树序列化为 HTML 片段（无 `<html>` 外壳）。
///
/// 便捷入口：等价于 `HtmlGenerator::new().generate(node)`。
pub fn node_to_html(node: &Node) -> String {
    HtmlGenerator::new().generate(node)
}

/// 生成开标签上的 `style="..."` 片段（样式非空才有），例如 ` style="color: #ff0000"`。
fn style_attr(style: &Style) -> String {
    let css = style.to_inline_css();
    if css.is_empty() {
        String::new()
    } else {
        format!(" style=\"{}\"", css)
    }
}

fn align_to_style(a: &TextAlign) -> String {
    match a {
        TextAlign::Left => " style=\"text-align: left;\"".to_string(),
        TextAlign::Center => " style=\"text-align: center;\"".to_string(),
        TextAlign::Right => " style=\"text-align: right;\"".to_string(),
        TextAlign::Justify => " style=\"text-align: justify;\"".to_string(),
    }
}

/// HTML 文本转义（用于元素内容）。
fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// HTML 属性值转义（用于 `src`/`href`/`alt`/`title` 等属性）。
fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parse_markdown;

    fn node_of(md: &str) -> Node {
        parse_markdown(md).expect("parse markdown")
    }

    #[test]
    fn test_node_to_html_heading() {
        let html = node_to_html(&node_of("# Title"));
        // 标签带内联 style 属性，故用宽松匹配标签名 + 内容
        assert!(
            html.contains("<h1") && html.contains(">Title</h1>"),
            "got: {}",
            html
        );
    }

    #[test]
    fn test_node_to_html_emphasis_strong() {
        let html = node_to_html(&node_of("*italic* and **bold**"));
        assert!(
            html.contains("<em") && html.contains(">italic</em>"),
            "got: {}",
            html
        );
        assert!(
            html.contains("<strong") && html.contains(">bold</strong>"),
            "got: {}",
            html
        );
    }

    #[test]
    fn test_node_to_html_code_block() {
        let html = node_to_html(&node_of("```rust\nfn main() {}\n```"));
        assert!(
            html.contains("<pre") && html.contains("<code"),
            "got: {}",
            html
        );
        assert!(html.contains("fn main()"), "got: {}", html);
    }

    #[test]
    fn test_node_to_html_list() {
        let html = node_to_html(&node_of("- a\n- b"));
        assert!(html.contains("<ul"), "got: {}", html);
        assert!(
            html.contains("<li>a</li>") || html.contains("<li") && html.contains(">a</li>"),
            "got: {}",
            html
        );
    }
}
