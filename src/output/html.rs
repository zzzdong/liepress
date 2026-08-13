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

use crate::ast::{Node, NodeKind, Style, TextAlign};

/// 将 `ast::Node` 语义树序列化为 HTML 片段（无 `<html>` 外壳）。
pub fn node_to_html(node: &Node) -> String {
    let mut out = String::new();
    serialize_node(node, &mut out);
    out
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

fn serialize_node(node: &Node, out: &mut String) {
    let sa = style_attr(&node.style);
    match &node.kind {
        NodeKind::Document { children } => {
            for c in children {
                serialize_node(c, out);
            }
        }
        NodeKind::Heading { level, children } => {
            let lvl = (*level).clamp(1, 6);
            out.push_str(&format!("<h{}{}>", lvl, sa));
            serialize_children(children, out);
            out.push_str(&format!("</h{}>", lvl));
        }
        NodeKind::Paragraph { children } => {
            out.push_str(&format!("<p{}>", sa));
            serialize_children(children, out);
            out.push_str("</p>");
        }
        NodeKind::List {
            ordered,
            start,
            children,
        } => {
            if *ordered {
                match start {
                    Some(s) if *s != 1 => {
                        out.push_str(&format!("<ol start=\"{}\"{}>", s, sa))
                    }
                    _ => out.push_str(&format!("<ol{}>", sa)),
                }
            } else {
                out.push_str(&format!("<ul{}>", sa));
            }
            for c in children {
                serialize_node(c, out);
            }
            out.push_str(if *ordered { "</ol>" } else { "</ul>" });
        }
        NodeKind::ListItem { children } => {
            out.push_str(&format!("<li{}>", sa));
            serialize_children(children, out);
            out.push_str("</li>");
        }
        NodeKind::TaskListItem { checked, children } => {
            out.push_str(&format!("<li{}>", sa));
            if *checked {
                out.push_str("<input checked=\"\" type=\"checkbox\"/> ");
            } else {
                out.push_str("<input type=\"checkbox\"/> ");
            }
            serialize_children(children, out);
            out.push_str("</li>");
        }
        NodeKind::DefinitionList { items } => {
            out.push_str(&format!("<dl{}>", sa));
            for item in items {
                out.push_str(&format!("<dt{}>", style_attr(&node.style)));
                for c in &item.term {
                    serialize_node(c, out);
                }
                out.push_str("</dt>");
                for c in &item.definition {
                    out.push_str(&format!("<dd{}>", style_attr(&node.style)));
                    serialize_node(c, out);
                    out.push_str("</dd>");
                }
            }
            out.push_str("</dl>");
        }
        NodeKind::FootnoteDef { id, children } => {
            out.push_str(&format!(
                "<div id=\"{}\" class=\"footnote-def\"{}>",
                id, sa
            ));
            for c in children {
                serialize_node(c, out);
            }
            out.push_str("</div>");
        }
        NodeKind::Image { src, alt, title } => {
            out.push_str("<img src=\"");
            out.push_str(&escape_attr(src));
            out.push_str("\" alt=\"");
            out.push_str(&escape_attr(alt));
            if let Some(t) = title {
                out.push_str("\" title=\"");
                out.push_str(&escape_attr(t));
            }
            out.push_str("\"/>");
        }
        NodeKind::CodeBlock { code, lang } => {
            out.push_str(&format!("<pre{}><code", sa));
            if let Some(l) = lang {
                if !l.is_empty() {
                    out.push_str(&format!(" class=\"language-{}\"", escape_attr(l)));
                }
            }
            out.push_str(">");
            out.push_str(&escape_html(code));
            out.push_str("</code></pre>");
        }
        NodeKind::Blockquote { children } => {
            out.push_str(&format!("<blockquote{}>", sa));
            for c in children {
                serialize_node(c, out);
            }
            out.push_str("</blockquote>");
        }
        NodeKind::ThematicBreak => out.push_str(&format!("<hr{}/>", sa)),
        NodeKind::Table { children, align } => {
            out.push_str(&format!("<table{}>", sa));
            serialize_table_rows(children, align, out);
            out.push_str("</table>");
        }
        NodeKind::TableRow { children } => {
            // 由父级 Table 调用 serialize_table_rows 时处理 <tr>；
            // 直接序列化时仍包裹 <tr> 以自洽。
            out.push_str(&format!("<tr{}>", sa));
            for c in children {
                serialize_node(c, out);
            }
            out.push_str("</tr>");
        }
        NodeKind::Text { text } => out.push_str(&escape_html(text)),
        NodeKind::Strong { children } => {
            out.push_str(&format!("<strong{}>", sa));
            serialize_children(children, out);
            out.push_str("</strong>");
        }
        NodeKind::Emphasis { children } => {
            out.push_str(&format!("<em{}>", sa));
            serialize_children(children, out);
            out.push_str("</em>");
        }
        NodeKind::InlineCode { code } => {
            out.push_str(&format!("<code{}>", sa));
            out.push_str(&escape_html(code));
            out.push_str("</code>");
        }
        NodeKind::Link {
            url,
            title,
            children,
        } => {
            out.push_str("<a href=\"");
            out.push_str(&escape_attr(url));
            if let Some(t) = title {
                out.push_str("\" title=\"");
                out.push_str(&escape_attr(t));
            }
            out.push_str(&format!("\"{}>", sa));
            serialize_children(children, out);
            out.push_str("</a>");
        }
        NodeKind::Delete { children } => {
            out.push_str(&format!("<del{}>", sa));
            serialize_children(children, out);
            out.push_str("</del>");
        }
        NodeKind::Subscript { children } => {
            out.push_str(&format!("<sub{}>", sa));
            serialize_children(children, out);
            out.push_str("</sub>");
        }
        NodeKind::Superscript { children } => {
            out.push_str(&format!("<sup{}>", sa));
            serialize_children(children, out);
            out.push_str("</sup>");
        }
        NodeKind::Span { children } => {
            out.push_str(&format!("<span{}>", sa));
            serialize_children(children, out);
            out.push_str("</span>");
        }
        NodeKind::Center { children } => {
            out.push_str(&format!("<center{}>", sa));
            for c in children {
                serialize_node(c, out);
            }
            out.push_str("</center>");
        }
        NodeKind::Container { children } => {
            out.push_str(&format!("<div{}>", sa));
            for c in children {
                serialize_node(c, out);
            }
            out.push_str("</div>");
        }
        NodeKind::LineBreak => out.push_str("<br/>"),
    }
}

/// 序列化子节点序列。
fn serialize_children(children: &[Node], out: &mut String) {
    for c in children {
        serialize_node(c, out);
    }
}

/// 表格行序列化：当前 `NodeKind` 把 `<th>/<td>` 都简化为 `Paragraph`，
/// 故统一以 `<td>` 输出；`align` 通过 `style="text-align:..."` 表达。
fn serialize_table_rows(rows: &[Node], align: &[TextAlign], out: &mut String) {
    for row in rows {
        if let NodeKind::TableRow { children } = &row.kind {
            out.push_str("<tr>");
            for (i, cell) in children.iter().enumerate() {
                let align_attr = align.get(i).map(align_to_style).unwrap_or_default();
                out.push_str(&format!("<td{}>", align_attr));
                serialize_node(cell, out);
                out.push_str("</td>");
            }
            out.push_str("</tr>");
        } else {
            serialize_node(row, out);
        }
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
        assert!(html.contains("<h1") && html.contains(">Title</h1>"), "got: {}", html);
    }

    #[test]
    fn test_node_to_html_emphasis_strong() {
        let html = node_to_html(&node_of("*italic* and **bold**"));
        assert!(html.contains("<em") && html.contains(">italic</em>"), "got: {}", html);
        assert!(html.contains("<strong") && html.contains(">bold</strong>"), "got: {}", html);
    }

    #[test]
    fn test_node_to_html_code_block() {
        let html = node_to_html(&node_of("```rust\nfn main() {}\n```"));
        assert!(html.contains("<pre") && html.contains("<code"), "got: {}", html);
        assert!(html.contains("fn main()"), "got: {}", html);
    }

    #[test]
    fn test_node_to_html_list() {
        let html = node_to_html(&node_of("- a\n- b"));
        assert!(html.contains("<ul"), "got: {}", html);
        assert!(html.contains("<li>a</li>") || html.contains("<li") && html.contains(">a</li>"), "got: {}", html);
    }
}
