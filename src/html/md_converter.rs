//! Markdown → HTML 转换器
//!
//! 将 Markdown 源文本转换为 HTML，同时保留原始 HTML 标签。
//! 使用 pulldown-cmark 解析 Markdown，直接输出 HTML。

use std::path::Path;

/// 将 Markdown 转换为 HTML 字符串（片段，无 `<html>/<head>/<body>` 包装）
pub fn markdown_to_html(markdown: &str) -> String {
    use pulldown_cmark::{Options, Parser, html};

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_HEADING_ATTRIBUTES);

    let parser = Parser::new_ext(markdown, opts);
    let mut html = String::new();
    html::push_html(&mut html, parser);
    html
}

/// 将 Markdown 转换为完整的 HTML 文档
///
/// 输出标准 HTML 结构：`<html><head><style>...</style></head><body>...</body></html>`
/// - 内置默认 CSS 始终包含在 `<head>` 中
/// - 用户提供的额外 CSS 会追加在默认样式之后
/// - `<title>` 优先链：`title` 参数 > 第一个 `<h1>` > `fallback_title`（如文件名）> "Document"
/// - 本地图片需提前嵌入为 base64 data URI（参见 `embed_local_images`）
pub fn markdown_to_html_document(
    markdown: &str,
    user_css: Option<&str>,
    title: Option<&str>,
    fallback_title: Option<&str>,
) -> String {
    let body = markdown_to_html(markdown);

    // 优先链：--title > <h1> > fallback（文件名）> "Document"
    let title = title
        .map(|t| t.to_string())
        .or_else(|| extract_first_h1_text(&body))
        .or_else(|| fallback_title.map(|t| t.to_string()))
        .unwrap_or_else(|| "Document".to_string());

    let mut doc = String::new();
    doc.push_str("<!DOCTYPE html>\n");
    doc.push_str("<html>\n<head>\n");
    doc.push_str("<meta charset=\"utf-8\">\n");
    doc.push_str(&format!("<title>{}</title>\n", escape_html(&title)));

    // 内置默认样式
    doc.push_str("<style>\n");
    doc.push_str(crate::ast::presets::DEFAULT_CSS);
    doc.push_str("\n</style>\n");

    // 用户自定义样式
    if let Some(css) = user_css
        && !css.is_empty()
    {
        doc.push_str("<style>\n");
        doc.push_str(css);
        doc.push_str("\n</style>\n");
    }

    doc.push_str("</head>\n<body>\n");
    doc.push_str(&body);
    doc.push_str("\n</body>\n</html>");

    doc
}

/// 将 HTML 中的本地图片路径替换为 base64 data URI
///
/// 只处理 `<img src="...">` 中的相对路径，跳过：
/// - 已经是 data URI 的
/// - 网络地址（http/https）
/// - 绝对路径（以 / 开头）
pub fn embed_local_images(html: &str, base_dir: Option<&Path>) -> String {
    let Some(base_dir) = base_dir else {
        return html.to_string();
    };

    let mut result = String::with_capacity(html.len());
    let mut pos = 0;

    while let Some(img_start) = html[pos..].find("<img ") {
        let abs_img_start = pos + img_start;
        result.push_str(&html[pos..abs_img_start]);

        // 找到 <img ...> 的结束位置
        let rest = &html[abs_img_start..];
        let Some(img_end) = rest.find('>') else {
            result.push_str(rest);
            break;
        };
        let img_tag = &rest[..=img_end];

        // 在 img 标签中查找 src 属性
        let new_tag = if let Some(src_start) = img_tag.find("src=\"") {
            let src_val_start = abs_img_start + src_start + 5; // after src="
            let src_val_end = html[src_val_start..].find('"').map(|i| src_val_start + i);
            if let Some(src_val_end) = src_val_end {
                let src_value = &html[src_val_start..src_val_end];
                if should_embed(src_value) {
                    if let Some(data_uri) = try_load_as_data_uri(src_value, base_dir) {
                        let mut new = String::with_capacity(img_tag.len() + data_uri.len());
                        new.push_str(&img_tag[..src_start + 5]);
                        new.push_str(&data_uri);
                        new.push_str(&img_tag[src_start + 5 + src_value.len()..]);
                        new
                    } else {
                        img_tag.to_string()
                    }
                } else {
                    img_tag.to_string()
                }
            } else {
                img_tag.to_string()
            }
        } else {
            img_tag.to_string()
        };

        result.push_str(&new_tag);
        pos = abs_img_start + img_end + 1;
    }

    result.push_str(&html[pos..]);
    result
}

/// 判断 src 是否应该被嵌入为 base64
fn should_embed(src: &str) -> bool {
    // 跳过 data URI、网络地址、绝对路径
    !src.starts_with("data:")
        && !src.starts_with("http://")
        && !src.starts_with("https://")
        && !src.starts_with('/')
}

/// 尝试将本地文件读取为 base64 data URI
fn try_load_as_data_uri(src: &str, base_dir: &Path) -> Option<String> {
    let path = base_dir.join(src);
    if !path.exists() {
        return None;
    }

    let bytes = std::fs::read(&path).ok()?;
    let mime = guess_mime_type(&path);
    let b64 = base64_encode(&bytes);
    Some(format!("data:{};base64,{}", mime, b64))
}

/// 根据 extension 猜测 MIME 类型
fn guess_mime_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

/// Base64 编码（避免引入额外依赖）
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    let chunks = data.chunks(3);
    for chunk in chunks {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// 从 HTML 片段中提取第一个 <h1> 的文本内容
fn extract_first_h1_text(html: &str) -> Option<String> {
    let start = html.find("<h1>")?;
    let end = html.find("</h1>")?;
    if end <= start + 4 {
        return None;
    }
    let content = &html[start + 4..end];
    // 去除内嵌标签，只保留纯文本
    let text = strip_html_tags(content);
    if text.is_empty() { None } else { Some(text) }
}

/// 去除 HTML 标签，只保留文本内容
fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

/// 简易 HTML 转义
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::ast::*;
    use crate::html::parser::parse_html;

    /// 辅助：将 Markdown 转换为 HtmlDocument
    fn md_to_doc(md: &str) -> HtmlDocument {
        let html = markdown_to_html(md);
        parse_html(&html)
    }

    /// 辅助：在 HtmlElement 树中查找第一个匹配标签的元素
    fn find_element(elem: &HtmlElement, tag: HtmlTag) -> Option<&HtmlElement> {
        if elem.tag == tag {
            return Some(elem);
        }
        for child in &elem.children {
            if let HtmlNode::Element(e) = child
                && let Some(found) = find_element(e, tag)
            {
                return Some(found);
            }
        }
        None
    }

    /// 辅助：收集所有匹配标签的元素
    fn find_all_elements(elem: &HtmlElement, tag: HtmlTag) -> Vec<&HtmlElement> {
        let mut result = Vec::new();
        if elem.tag == tag {
            result.push(elem);
        }
        for child in &elem.children {
            if let HtmlNode::Element(e) = child {
                result.extend(find_all_elements(e, tag));
            }
        }
        result
    }

    /// 辅助：获取元素的直接文本内容（不含子元素文本）
    fn direct_text(elem: &HtmlElement) -> String {
        elem.children
            .iter()
            .filter_map(|c| match c {
                HtmlNode::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    // ─── Markdown → HTML AST 测试 ─────────────────────────

    #[test]
    fn test_heading_structure() {
        let doc = md_to_doc("# Hello");
        let h1 = find_element(&doc.root, HtmlTag::H1);
        assert!(h1.is_some(), "Should find <h1> element");
        let h1 = h1.unwrap();
        assert_eq!(direct_text(h1), "Hello");
    }

    #[test]
    fn test_heading_levels() {
        for level in 1..=6 {
            let md = format!("{} Level {}", "#".repeat(level), level);
            let doc = md_to_doc(&md);
            let tag = match level {
                1 => HtmlTag::H1,
                2 => HtmlTag::H2,
                3 => HtmlTag::H3,
                4 => HtmlTag::H4,
                5 => HtmlTag::H5,
                _ => HtmlTag::H6,
            };
            let elem = find_element(&doc.root, tag);
            assert!(elem.is_some(), "Should find h{} element", level);
            assert!(
                elem.unwrap()
                    .text_content()
                    .contains(&format!("Level {}", level))
            );
        }
    }

    #[test]
    fn test_paragraph_with_inline_styles() {
        let doc = md_to_doc("This is **bold** and *italic*");
        let p = find_element(&doc.root, HtmlTag::P);
        assert!(p.is_some(), "Should find <p> element");

        let p = p.unwrap();
        let strong = find_element(p, HtmlTag::Strong);
        assert!(strong.is_some(), "Should find <strong> inside <p>");
        assert_eq!(direct_text(strong.unwrap()), "bold");

        let em = find_element(p, HtmlTag::Em);
        assert!(em.is_some(), "Should find <em> inside <p>");
        assert_eq!(direct_text(em.unwrap()), "italic");
    }

    #[test]
    fn test_code_block_structure() {
        let doc = md_to_doc("```rust\nfn main() {}\n```");
        let pre = find_element(&doc.root, HtmlTag::Pre);
        assert!(pre.is_some(), "Should find <pre> element");

        let pre = pre.unwrap();
        let code = find_element(pre, HtmlTag::Code);
        assert!(code.is_some(), "Should find <code> inside <pre>");

        let code = code.unwrap();
        let class = code.attrs.get("class").map(|s| s.as_str()).unwrap_or("");
        assert_eq!(
            class, "language-rust",
            "Code should have exact language-rust class, got: {}",
            class
        );
        // 代码块内容应原样保留，包括换行
        assert_eq!(
            code.text_content(),
            "fn main() {}\n",
            "Code content should be preserved exactly with newlines"
        );
    }

    #[test]
    fn test_unordered_list_structure() {
        let doc = md_to_doc("- item1\n- item2\n- item3");
        let ul = find_element(&doc.root, HtmlTag::Ul);
        assert!(ul.is_some(), "Should find <ul> element");

        let ul = ul.unwrap();
        let items = find_all_elements(ul, HtmlTag::Li);
        assert_eq!(items.len(), 3, "Should have 3 list items");
        // Markdown 列表项文本可能被包裹在 <p> 中，使用 text_content 获取全部文本
        assert_eq!(items[0].text_content(), "item1");
        assert_eq!(items[1].text_content(), "item2");
        assert_eq!(items[2].text_content(), "item3");
    }

    #[test]
    fn test_ordered_list_with_start() {
        let doc = md_to_doc("5. fifth\n6. sixth");
        let ol = find_element(&doc.root, HtmlTag::Ol);
        assert!(ol.is_some(), "Should find <ol> element");

        let ol = ol.unwrap();
        let start = ol.attrs.get("start").map(|s| s.as_str()).unwrap_or("");
        assert_eq!(start, "5", "Ordered list should have start=5");

        let items = find_all_elements(ol, HtmlTag::Li);
        assert_eq!(items.len(), 2, "Should have 2 list items");
    }

    #[test]
    fn test_html_passthrough() {
        let doc = md_to_doc("<center>centered</center>");
        let center = find_element(&doc.root, HtmlTag::Center);
        assert!(center.is_some(), "Should find <center> element");
        assert_eq!(direct_text(center.unwrap()), "centered");
    }

    #[test]
    fn test_style_tag_extraction() {
        let doc = md_to_doc("<style>h1 { color: red; }</style>\n# Hello");
        assert_eq!(doc.style_sheets.len(), 1, "Should extract 1 style sheet");
        assert!(
            doc.style_sheets[0].contains("color: red"),
            "Style sheet should contain the CSS rule"
        );

        let h1 = find_element(&doc.root, HtmlTag::H1);
        assert!(h1.is_some(), "Should still find <h1> element");
    }

    #[test]
    fn test_table_structure() {
        let doc = md_to_doc("| A | B |\n|---|---|\n| 1 | 2 |");
        let table = find_element(&doc.root, HtmlTag::Table);
        assert!(table.is_some(), "Should find <table> element");

        let table = table.unwrap();
        let rows = find_all_elements(table, HtmlTag::Tr);
        assert_eq!(rows.len(), 2, "Should have 2 rows (header + body)");

        // 第一行应该有 <th>
        let ths = find_all_elements(rows[0], HtmlTag::Th);
        assert_eq!(ths.len(), 2, "Header row should have 2 <th> cells");
        assert_eq!(direct_text(ths[0]), "A");
        assert_eq!(direct_text(ths[1]), "B");

        // 第二行应该有 <td>
        let tds = find_all_elements(rows[1], HtmlTag::Td);
        assert_eq!(tds.len(), 2, "Body row should have 2 <td> cells");
        assert_eq!(direct_text(tds[0]), "1");
        assert_eq!(direct_text(tds[1]), "2");
    }

    #[test]
    fn test_link_structure() {
        let doc = md_to_doc("[Example](https://example.com)");
        let a = find_element(&doc.root, HtmlTag::A);
        assert!(a.is_some(), "Should find <a> element");

        let a = a.unwrap();
        assert_eq!(
            a.attrs.get("href").map(|s| s.as_str()),
            Some("https://example.com")
        );
        assert_eq!(direct_text(a), "Example");
    }

    #[test]
    fn test_image_structure() {
        let doc = md_to_doc("![Alt text](image.png)");
        let img = find_element(&doc.root, HtmlTag::Img);
        assert!(img.is_some(), "Should find <img> element");

        let img = img.unwrap();
        assert_eq!(img.attrs.get("src").map(|s| s.as_str()), Some("image.png"));
        assert_eq!(img.attrs.get("alt").map(|s| s.as_str()), Some("Alt text"));
    }

    #[test]
    fn test_blockquote_structure() {
        let doc = md_to_doc("> quoted text");
        let bq = find_element(&doc.root, HtmlTag::Blockquote);
        assert!(bq.is_some(), "Should find <blockquote> element");
        assert!(bq.unwrap().text_content().contains("quoted text"));
    }

    #[test]
    fn test_inline_code() {
        let doc = md_to_doc("Use `cargo build` to compile");
        let code = find_element(&doc.root, HtmlTag::Code);
        assert!(code.is_some(), "Should find <code> element");
        assert_eq!(direct_text(code.unwrap()), "cargo build");
    }

    #[test]
    fn test_delete_strikethrough() {
        let doc = markdown_to_html("~~deleted~~");
        assert!(
            doc.contains("<del>deleted</del>"),
            "GFM strikethrough should produce <del>"
        );
    }

    #[test]
    fn test_task_list_html_output() {
        let html = markdown_to_html("- [ ] unchecked\n- [x] checked");
        // pulldown-cmark 输出格式: <input disabled="" type="checkbox"/> 和 checked=""
        assert!(
            html.contains(r#"disabled="" type="checkbox"/>"#)
                || html.contains(r#"type="checkbox" disabled"#),
            "Unchecked item should have checkbox (got: {html:?})"
        );
        assert!(
            html.contains(r#"checked=""#) || html.contains(" checked"),
            "Checked item should have checked attr (got: {html:?})"
        );
        // 任务列表项不应包含 <p> 包裹
        assert!(
            !html.contains("<p>unchecked</p>"),
            "Task list item text should NOT be wrapped in <p>"
        );
        assert!(
            !html.contains("<p>checked</p>"),
            "Task list item text should NOT be wrapped in <p>"
        );
    }

    #[test]
    fn test_task_list_ast_structure() {
        let doc = md_to_doc("- [ ] unchecked\n- [x] checked");
        let ul = find_element(&doc.root, HtmlTag::Ul);
        assert!(ul.is_some(), "Should find <ul>");

        let ul = ul.unwrap();
        let items = find_all_elements(ul, HtmlTag::Li);
        assert_eq!(items.len(), 2, "Should have 2 list items");

        // 第一个 li 应包含 unchecked checkbox
        let input1 = find_element(items[0], HtmlTag::Input);
        assert!(input1.is_some(), "First item should have <input>");
        let input1 = input1.unwrap();
        assert_eq!(
            input1.attrs.get("type").map(|s| s.as_str()),
            Some("checkbox")
        );
        assert!(
            !input1.attrs.contains_key("checked"),
            "First checkbox should not be checked"
        );

        // 第二个 li 应包含 checked checkbox
        let input2 = find_element(items[1], HtmlTag::Input);
        assert!(input2.is_some(), "Second item should have <input>");
        let input2 = input2.unwrap();
        assert!(
            input2.attrs.contains_key("checked"),
            "Second checkbox should be checked"
        );
    }

    // ─── markdown_to_html_document 测试 ──────────────────────

    #[test]
    fn test_html_document_structure() {
        let doc = markdown_to_html_document("# Title\n\nParagraph", None, None, None);
        assert!(
            doc.starts_with("<!DOCTYPE html>"),
            "Should start with DOCTYPE"
        );
        assert!(doc.contains("<html>"), "Should have <html> tag");
        assert!(doc.contains("<head>"), "Should have <head> tag");
        assert!(doc.contains("</head>"), "Should close <head>");
        assert!(doc.contains("<body>"), "Should have <body> tag");
        assert!(doc.contains("</body>"), "Should close <body>");
        assert!(doc.contains("</html>"), "Should close <html>");
    }

    #[test]
    fn test_html_document_title_from_h1() {
        let doc = markdown_to_html_document("# My Title\n\nContent", None, None, None);
        assert!(
            doc.contains("<title>My Title</title>"),
            "Title should come from first h1"
        );
    }

    #[test]
    fn test_html_document_title_default_when_no_h1() {
        let doc = markdown_to_html_document("Just a paragraph", None, None, None);
        assert!(
            doc.contains("<title>Document</title>"),
            "Title should default to 'Document'"
        );
    }

    #[test]
    fn test_html_document_title_override() {
        let doc =
            markdown_to_html_document("# H1 Title\n\nContent", None, Some("Custom Title"), None);
        assert!(
            doc.contains("<title>Custom Title</title>"),
            "Title should use override value"
        );
        assert!(
            !doc.contains("<title>H1 Title</title>"),
            "Title should NOT use h1 when override provided"
        );
    }

    #[test]
    fn test_html_document_title_fallback_from_filename() {
        // 无 h1 时，使用 fallback（文件名）
        let doc = markdown_to_html_document("Just a paragraph", None, None, Some("my-report"));
        assert!(
            doc.contains("<title>my-report</title>"),
            "Title should use fallback when no h1"
        );
    }

    #[test]
    fn test_html_document_title_priority() {
        // --title > h1 > fallback
        let doc = markdown_to_html_document(
            "# H1 Title\n\nContent",
            None,
            Some("CLI Title"),
            Some("filename"),
        );
        assert!(
            doc.contains("<title>CLI Title</title>"),
            "CLI --title should win"
        );

        // h1 > fallback
        let doc = markdown_to_html_document("# H1 Title\n\nContent", None, None, Some("filename"));
        assert!(
            doc.contains("<title>H1 Title</title>"),
            "h1 should win over fallback"
        );
    }

    #[test]
    fn test_html_document_builtin_css() {
        let doc = markdown_to_html_document("# Test", None, None, None);
        assert!(doc.contains("<style>"), "Should have <style> tag");
        assert!(
            doc.contains("font-family: serif"),
            "Should include builtin CSS"
        );
    }

    #[test]
    fn test_html_document_user_css() {
        let doc = markdown_to_html_document("# Test", Some("h1 { color: red; }"), None, None);
        assert!(
            doc.contains("h1 { color: red; }"),
            "Should include user CSS"
        );
        assert!(
            doc.contains("font-family: serif"),
            "Should still include builtin CSS"
        );
    }

    #[test]
    fn test_html_document_charset() {
        let doc = markdown_to_html_document("# Test", None, None, None);
        assert!(
            doc.contains(r#"<meta charset="utf-8">"#),
            "Should have charset meta"
        );
    }

    // ─── embed_local_images 测试 ────────────────────────────

    #[test]
    fn test_embed_skips_data_uri() {
        let html = r#"<img src="data:image/png;base64,abc123">"#;
        let result = embed_local_images(html, Some(std::path::Path::new(".")));
        assert_eq!(result, html, "Should not modify data URIs");
    }

    #[test]
    fn test_embed_skips_http_urls() {
        let html = r#"<img src="https://example.com/img.png">"#;
        let result = embed_local_images(html, Some(std::path::Path::new(".")));
        assert_eq!(result, html, "Should not modify HTTP URLs");
    }

    #[test]
    fn test_embed_skips_absolute_paths() {
        let html = r#"<img src="/images/logo.png">"#;
        let result = embed_local_images(html, Some(std::path::Path::new(".")));
        assert_eq!(result, html, "Should not modify absolute paths");
    }

    #[test]
    fn test_embed_no_base_dir() {
        let html = r#"<img src="photo.png">"#;
        let result = embed_local_images(html, None);
        assert_eq!(result, html, "Should not modify when no base_dir");
    }

    #[test]
    fn test_embed_missing_file_keeps_original() {
        let html = r#"<img src="nonexistent.png" alt="missing">"#;
        let result = embed_local_images(html, Some(std::path::Path::new(".")));
        assert!(
            result.contains(r#"src="nonexistent.png""#),
            "Should keep original src when file not found"
        );
    }

    #[test]
    fn test_base64_encode() {
        // "Hello" in base64 is "SGVsbG8="
        assert_eq!(base64_encode(b"Hello"), "SGVsbG8=");
        // "Man" in base64 is "TWFu"
        assert_eq!(base64_encode(b"Man"), "TWFu");
        // "M" in base64 is "TQ=="
        assert_eq!(base64_encode(b"M"), "TQ==");
        // "Ma" in base64 is "TWE="
        assert_eq!(base64_encode(b"Ma"), "TWE=");
    }

    #[test]
    fn test_guess_mime_type() {
        assert_eq!(
            guess_mime_type(std::path::Path::new("img.png")),
            "image/png"
        );
        assert_eq!(
            guess_mime_type(std::path::Path::new("img.jpg")),
            "image/jpeg"
        );
        assert_eq!(
            guess_mime_type(std::path::Path::new("img.jpeg")),
            "image/jpeg"
        );
        assert_eq!(
            guess_mime_type(std::path::Path::new("img.gif")),
            "image/gif"
        );
        assert_eq!(
            guess_mime_type(std::path::Path::new("img.svg")),
            "image/svg+xml"
        );
        assert_eq!(
            guess_mime_type(std::path::Path::new("img.webp")),
            "image/webp"
        );
        assert_eq!(
            guess_mime_type(std::path::Path::new("img.unknown")),
            "application/octet-stream"
        );
    }
}
