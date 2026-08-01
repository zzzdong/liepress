//! 样式系统模块
//!
//! 三层 AST 架构的 Layer 2：Styled AST
//! - HTML (Layer 1) → Node (Layer 2) → Layout AST (Layer 3)
//! - 每个 Node 附带 Style，布局引擎不再关心样式来源
//!
//! # 样式系统
//!
//! 样式解析统一由 `crate::css::engine::CssEngine`（基于 Lightning CSS）处理：
//! - 内置默认 CSS 样式表（由 presets::DEFAULT_CSS 定义）
//! - 可选的用户 CSS 覆盖
//! - HTML→Styled Node 的转换统一在 `crate::html::styled` 中完成

pub mod node;
pub mod presets;
pub mod style;

// 重新导出主要类型
pub use node::*;
pub use presets::*;
pub use style::*;

use crate::css::engine::CssEngine;

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

/// 内部共享：构建 CssEngine + 解析 Styled Node
fn build_engine_and_parse(
    markdown: &str,
    user_css: &str,
    strict_mode: bool,
) -> Result<(Node, PageConfig), String> {
    // Step 1: Markdown → HTML
    let html = crate::html::md_converter::markdown_to_html(markdown);

    // Step 2: HTML → HtmlDocument（含 <style> 标签 CSS 提取）
    let doc = crate::html::parser::parse_html(&html);

    // Step 3: 合并 CSS（内置 + 用户 + 内联 <style>）
    let inline_css = doc.style_sheets.join("\n");
    let combined_css = if user_css.is_empty() {
        inline_css
    } else if inline_css.is_empty() {
        user_css.to_string()
    } else {
        format!("{}\n{}", user_css, inline_css)
    };

    // Step 4: 构建 CssEngine（Lightning CSS），内置样式始终加载
    let mut engine = CssEngine::new(DEFAULT_CSS)?.with_strict_mode(strict_mode);
    if !combined_css.is_empty() {
        engine = engine.with_user_css(&combined_css)?;
    }

    // Step 5: 从 HtmlDocument 构建 Node AST（使用新管线 html/styled）
    let mut node = crate::html::styled::html_to_styled_nodes(&doc, &engine);
    // 保证根节点一定是 Document（兼容测试和下游消费者）
    if !matches!(node.kind, NodeKind::Document { .. }) {
        node = Node::new(
            NodeKind::Document {
                children: vec![node],
            },
            Style::default(),
            false,
        );
    }
    let page_config = engine.page_config().clone();
    Ok((node, page_config))
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
pub fn parse_markdown_with_css(
    markdown: &str,
    user_css: &str,
) -> Result<(Node, PageConfig), String> {
    build_engine_and_parse(markdown, user_css, false)
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
pub fn parse_markdown_with_css_strict(
    markdown: &str,
    user_css: &str,
) -> Result<(Node, PageConfig), String> {
    build_engine_and_parse(markdown, user_css, true)
}

/// 使用自定义样式解析器解析 Markdown
///
/// 适用于需要多次解析但共享同一个解析器的场景（如批量处理）。
/// 注意：此函数不提取 Markdown 内的 `<style>` 标签。
/// 如果需要内联样式支持，请使用 `parse_markdown_with_css`。
pub fn parse_markdown_with_resolver(markdown: &str, engine: &CssEngine) -> Node {
    let html = crate::html::md_converter::markdown_to_html(markdown);
    let doc = crate::html::parser::parse_html(&html);
    let mut node = crate::html::styled::html_to_styled_nodes(&doc, engine);
    // 保证根节点一定是 Document
    if !matches!(node.kind, NodeKind::Document { .. }) {
        node = Node::new(
            NodeKind::Document {
                children: vec![node],
            },
            Style::default(),
            false,
        );
    }
    node
}
