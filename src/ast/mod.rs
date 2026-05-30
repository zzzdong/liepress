//! 样式系统模块
//!
//! 三层 AST 架构的 Layer 2：Styled AST
//! - MDAST (Layer 1) → Node (Layer 2) → Layout AST (Layer 3)
//! - 每个 Node 附带 Style，布局引擎不再关心样式来源
//!
//! # 样式系统
//!
//! 本模块实现了完整的 CSS 样式系统：
//! - 内置默认 CSS 样式表（由 presets::DEFAULT_CSS 定义）
//! - 可选的用户 CSS 覆盖
//! - CSS 解析、选择器匹配、级联与特异性计算

pub mod css;
pub mod node;
pub mod presets;
pub mod style;

// 重新导出主要类型
pub use css::*;
pub use node::*;
pub use presets::*;
pub use style::*;

use markdown::mdast;

/// 使用内置默认样式解析 Markdown
///
/// 这是最简单的入口函数，使用内置 CSS 样式表。
/// 如果 Markdown 内容包含 `<style>` 标签，其中的 CSS 会被自动提取并应用。
/// 此函数使用非严格模式，CSS 解析错误会被静默忽略。
///
/// # 示例
/// ```
/// use liepress::ast::parse_markdown;
///
/// let markdown = "# Hello\n\nThis is a paragraph.";
/// let ast = parse_markdown(markdown).unwrap();
/// ```
pub fn parse_markdown(markdown: &str) -> Result<Node, String> {
    let (node, _page_config) = parse_markdown_with_css(markdown, "")?;
    Ok(node)
}

/// 使用自定义 CSS 解析 Markdown（非严格模式）
///
/// # 参数
/// - `markdown`: Markdown 源文本
/// - `user_css`: 用户提供的 CSS（空字符串表示不使用用户 CSS）
///
/// 用户 CSS 优先级高于内置样式表，可以实现样式覆盖。
/// 如果 Markdown 内容包含 `<style>` 标签，其中的 CSS 优先级最高。
///
/// **非严格模式**：如果用户 CSS 或内联 `<style>` 中的 CSS 解析失败，
/// 错误会被静默忽略，继续使用已有的有效样式。
/// 如果需要严格检查 CSS 语法错误，请使用 `parse_markdown_with_css_strict`。
///
/// # 返回值
/// 返回 `(Node, PageConfig)` 元组，其中 `PageConfig` 包含从 `@page` 规则提取的页面设置。
///
/// # 示例
/// ```
/// use liepress::ast::parse_markdown_with_css;
///
/// let markdown = "# Hello";
/// let user_css = "h1 { color: red; font-size: 32pt; }";
/// let (ast, _page_config) = parse_markdown_with_css(markdown, user_css).unwrap();
/// ```
pub fn parse_markdown_with_css(markdown: &str, user_css: &str) -> Result<(Node, PageConfig), String> {
    let mdast =
        markdown::to_mdast(markdown, &markdown::ParseOptions::gfm()).unwrap_or_else(|_| {
            mdast::Node::Root(mdast::Root {
                children: vec![],
                position: None,
            })
        });

    // 从 MDAST 中提取 <style> 标签内的 CSS
    let style_css = extract_style_css(&mdast);

    // 合并用户 CSS 和内联 CSS（内联 CSS 在后，优先级更高）
    let combined_css = if user_css.is_empty() {
        style_css
    } else if style_css.is_empty() {
        user_css.to_string()
    } else {
        format!("{}\n{}", user_css, style_css)
    };

    // 非严格模式：CSS 解析错误被静默忽略
    let resolver = StyleResolver::new(DEFAULT_CSS)?
        .with_strict_mode(false)
        .with_user_css(&combined_css)?;

    let node = node::build_ast(&mdast, &resolver);
    let page_config = resolver.page_config().clone();
    Ok((node, page_config))
}

/// 使用自定义 CSS 解析 Markdown（严格模式）
///
/// # 参数
/// - `markdown`: Markdown 源文本
/// - `user_css`: 用户提供的 CSS（空字符串表示不使用用户 CSS）
///
/// **严格模式**：如果用户 CSS 或内联 `<style>` 中的 CSS 解析失败，
/// 错误会被传播给调用者。适用于需要确保 CSS 完全正确的场景。
///
/// # 返回值
/// 返回 `(Node, PageConfig)` 元组。
///
/// # 示例
/// ```
/// use liepress::ast::parse_markdown_with_css_strict;
///
/// let markdown = "# Hello";
/// let user_css = "h1 { color: red; font-size: 32pt; }";
/// let (ast, _page_config) = parse_markdown_with_css_strict(markdown, user_css).unwrap();
/// ```
pub fn parse_markdown_with_css_strict(markdown: &str, user_css: &str) -> Result<(Node, PageConfig), String> {
    let mdast =
        markdown::to_mdast(markdown, &markdown::ParseOptions::gfm()).unwrap_or_else(|_| {
            mdast::Node::Root(mdast::Root {
                children: vec![],
                position: None,
            })
        });

    // 从 MDAST 中提取 <style> 标签内的 CSS
    let style_css = extract_style_css(&mdast);

    // 合并用户 CSS 和内联 CSS（内联 CSS 在后，优先级更高）
    let combined_css = if user_css.is_empty() {
        style_css
    } else if style_css.is_empty() {
        user_css.to_string()
    } else {
        format!("{}\n{}", user_css, style_css)
    };

    // 严格模式：CSS 解析错误会传播
    let resolver = StyleResolver::new(DEFAULT_CSS)?
        .with_strict_mode(true)
        .with_user_css(&combined_css)?;

    let node = node::build_ast(&mdast, &resolver);
    let page_config = resolver.page_config().clone();
    Ok((node, page_config))
}

/// 使用自定义样式解析器解析 Markdown
///
/// 适用于需要多次解析但共享同一个解析器的场景（如批量处理）。
/// 注意：此函数不提取 Markdown 内的 `<style>` 标签。
/// 如果需要内联样式支持，请使用 `parse_markdown_with_css`。
pub fn parse_markdown_with_resolver(markdown: &str, resolver: &StyleResolver) -> Node {
    let mdast =
        markdown::to_mdast(markdown, &markdown::ParseOptions::gfm()).unwrap_or_else(|_| {
            mdast::Node::Root(mdast::Root {
                children: vec![],
                position: None,
            })
        });

    node::build_ast(&mdast, resolver)
}

// ─── 内联 <style> 标签 CSS 提取 ────────────────────────────

/// 从 MDAST 树中递归提取所有 `<style>` 标签内的 CSS 内容
fn extract_style_css(node: &mdast::Node) -> String {
    let mut css_parts = Vec::new();
    collect_style_css(node, &mut css_parts);
    css_parts.join("\n")
}

/// 递归收集所有 <style> 标签内的 CSS
fn collect_style_css(node: &mdast::Node, css_parts: &mut Vec<String>) {
    if let mdast::Node::Html(html) = node
        && let Some(css) = extract_style_text(&html.value) {
            css_parts.push(css);
        }

    if let Some(children) = mdast_children(node) {
        for child in children {
            collect_style_css(child, css_parts);
        }
    }
}

/// 从 HTML 片段中提取 <style>...</style> 之间的文本
fn extract_style_text(html: &str) -> Option<String> {
    let html = html.trim();
    let tag_start = html.find("<style")?;
    // 找到 <style> 或 <style ...> 的结束 >
    let after_tag = &html[tag_start..];
    let bracket_end = after_tag.find('>')?;
    let content_start = tag_start + bracket_end + 1;
    let remaining = &html[content_start..];
    let close_end = remaining.find("</style>")?;
    let css = remaining[..close_end].trim();
    if css.is_empty() { None } else { Some(css.to_string()) }
}

/// 获取 mdast 节点的子节点列表
fn mdast_children(node: &mdast::Node) -> Option<&[mdast::Node]> {
    match node {
        mdast::Node::Root(n) => Some(&n.children),
        mdast::Node::Paragraph(n) => Some(&n.children),
        mdast::Node::Heading(n) => Some(&n.children),
        mdast::Node::List(n) => Some(&n.children),
        mdast::Node::ListItem(n) => Some(&n.children),
        mdast::Node::Blockquote(n) => Some(&n.children),
        mdast::Node::Table(n) => Some(&n.children),
        mdast::Node::TableRow(n) => Some(&n.children),
        mdast::Node::TableCell(n) => Some(&n.children),
        mdast::Node::Strong(n) => Some(&n.children),
        mdast::Node::Emphasis(n) => Some(&n.children),
        mdast::Node::Link(n) => Some(&n.children),
        mdast::Node::Delete(n) => Some(&n.children),
        _ => None,
    }
}