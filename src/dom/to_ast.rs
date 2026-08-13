//! Styled HTML 转换器
//!
//! 将 HTML AST + CSS 引擎转换为带样式的 Node 树，
//! 供文档层（`crate::document::from_ast`）与流式输出（HTML）消费。
//!
//! 管线：Markdown → HTML → HtmlDocument → Styled Node Tree → Document
//!
//! 架构：
//! - `StyleResolver` 负责 CSS 解析和祖先管理
//! - `convert_element` / `convert_children` 专注于 HTML → NodeKind 的结构映射
//! - 结构映射与样式解析分离，独立可测

use crate::ast::style::{Display, Style, TextAlign, TextDecoration};
use crate::ast::{DefinitionItem, Node, NodeKind};
use crate::css::engine::CssEngine;
use crate::dom::style_resolver::StyleResolver;
use crate::dom::*;

/// 将 HtmlDocument 转换为带样式的 Node 树
pub fn html_to_styled_nodes(doc: &HtmlDocument, engine: &CssEngine) -> Node {
    let mut resolver = StyleResolver::new(engine);
    convert_element(&doc.root, &mut resolver, &Style::default())
}

/// 转换 HTML 元素为 Styled Node
///
/// 职责链：
/// 1. `StyleResolver` 解析样式（CSS 选择器 + 祖先 + 内联）
/// 2. 本函数根据标签类型映射到 `NodeKind`
/// 3. 递归处理子元素
fn convert_element(elem: &HtmlElement, resolver: &mut StyleResolver, parent_style: &Style) -> Node {
    // ── 1. 解析样式 ──
    let mut style = resolver.resolve(elem, parent_style);

    // display: none — 不生成任何节点
    if style.display == Display::None {
        return Node::new(
            NodeKind::Text {
                text: String::new(),
            },
            Style::default(),
            false,
        );
    }

    // 跳过 <style> / <head> — 不参与布局
    match elem.tag {
        HtmlTag::Style | HtmlTag::Head => {
            return Node::new(
                NodeKind::Text {
                    text: String::new(),
                },
                Style::default(),
                false,
            );
        }
        _ => {}
    }

    // ── 1b. 标签特定样式调整 ──
    match elem.tag {
        HtmlTag::Mark => apply_mark_style(&mut style),
        HtmlTag::Small => apply_small_style(&mut style),
        HtmlTag::Sub | HtmlTag::Sup => apply_sub_sup_style(&mut style),
        HtmlTag::U => apply_underline_style(&mut style),
        _ => {}
    }

    // 特殊标签处理（在祖先上下文之外）
    match elem.tag {
        // 透明容器：Html, Body — 直接透传子节点
        HtmlTag::Html | HtmlTag::Body => {
            return resolver.with_ancestor(elem, |resolver| {
                let children = convert_children(&elem.children, resolver, &style);
                if children.len() == 1 {
                    let child = children.into_iter().next().unwrap();
                    if let NodeKind::Document { children: inner } = child.kind {
                        return Node::new(NodeKind::Document { children: inner }, style, false);
                    }
                    return child;
                }
                Node::new(NodeKind::Document { children }, style, false)
            });
        }

        // Thead / Tbody — 透明容器，子元素提升
        HtmlTag::Thead | HtmlTag::Tbody => {
            return resolver.with_ancestor(elem, |resolver| {
                let children = convert_children(&elem.children, resolver, &style);
                Node::new(NodeKind::Document { children }, style, false)
            });
        }

        _ => {}
    }

    // ── 2. 在祖先上下文中递归处理子元素 ──
    resolver.with_ancestor(elem, |resolver| {
        // 特殊标签：需要内联子元素
        let is_inline_parent = matches!(
            elem.tag,
            HtmlTag::P
                | HtmlTag::Span
                | HtmlTag::Strong
                | HtmlTag::B
                | HtmlTag::Em
                | HtmlTag::I
                | HtmlTag::Del
                | HtmlTag::S
                | HtmlTag::U
                | HtmlTag::Mark
                | HtmlTag::Small
                | HtmlTag::Sub
                | HtmlTag::Sup
                | HtmlTag::A
                | HtmlTag::Th
                | HtmlTag::Td
                | HtmlTag::Code
        );

        let kind = if is_inline_parent {
            convert_tag_inline(elem, &style, resolver)
        } else {
            convert_tag_block(elem, &style, resolver)
        };

        // 链接节点的 style 需要携带 link_url（用于生成注解/超链接）
        let mut final_style = style;
        if let NodeKind::Link { url, .. } = &kind
            && !url.is_empty()
        {
            final_style.link_url = Some(url.clone());
        }

        let splittable = kind.is_splittable();
        Node::new(kind, final_style, splittable)
    })
}

/// 行内标签 → NodeKind 映射
///
/// 处理需要 inline 子元素的标签（P, Span, Strong, Em, A 等）
fn convert_tag_inline(elem: &HtmlElement, style: &Style, resolver: &mut StyleResolver) -> NodeKind {
    match elem.tag {
        HtmlTag::P => {
            let children = convert_inline_children(&elem.children, resolver, style);
            NodeKind::Paragraph { children }
        }

        HtmlTag::Span => {
            let children = convert_inline_children(&elem.children, resolver, style);
            NodeKind::Span { children }
        }

        HtmlTag::Strong | HtmlTag::B => {
            let children = convert_inline_children(&elem.children, resolver, style);
            NodeKind::Strong { children }
        }

        HtmlTag::Em | HtmlTag::I => {
            let children = convert_inline_children(&elem.children, resolver, style);
            NodeKind::Emphasis { children }
        }

        HtmlTag::Del | HtmlTag::S => {
            let children = convert_inline_children(&elem.children, resolver, style);
            NodeKind::Delete { children }
        }

        HtmlTag::U => {
            // 下划线通过 style.text_decoration 处理
            let children = convert_inline_children(&elem.children, resolver, style);
            NodeKind::Span { children }
        }

        HtmlTag::Mark => {
            // 标记通过 background_color 处理，已在 style 中设置
            let children = convert_inline_children(&elem.children, resolver, style);
            NodeKind::Span { children }
        }

        HtmlTag::Small => {
            let children = convert_inline_children(&elem.children, resolver, style);
            NodeKind::Span { children }
        }

        HtmlTag::Sub | HtmlTag::Sup => {
            // 脚注引用：<sup class="footnote-ref"><a href="#fn-def-<id>">N</a></sup>
            // → 带内部锚点链接的上标数字（PDF 端据此生成可点击的内部跳转）。
            let is_footnote_ref = elem
                .attrs
                .get("class")
                .map(|c| c.split_whitespace().any(|w| w == "footnote-ref"))
                .unwrap_or(false);
            if is_footnote_ref {
                let (num, href) = footnote_ref_content(elem);
                let sup = Node::new(
                    NodeKind::Superscript {
                        children: vec![Node::new(
                            NodeKind::Text { text: num },
                            crate::ast::Style::default(),
                            false,
                        )],
                    },
                    style.clone(),
                    false,
                );
                return NodeKind::Link {
                    url: href.unwrap_or_default(),
                    title: None,
                    children: vec![sup],
                };
            }
            let children = convert_inline_children(&elem.children, resolver, style);
            if elem.tag == HtmlTag::Sup {
                NodeKind::Superscript { children }
            } else {
                NodeKind::Subscript { children }
            }
        }

        HtmlTag::A => {
            let url = elem.attrs.get("href").cloned().unwrap_or_default();
            let title = elem.attrs.get("title").cloned();
            let mut link_style = style.clone();
            link_style.link_url = (!url.is_empty()).then(|| url.clone());
            let children = convert_inline_children(&elem.children, resolver, &link_style);
            NodeKind::Link {
                url,
                title,
                children,
            }
        }

        HtmlTag::Th | HtmlTag::Td => {
            let children = convert_inline_children(&elem.children, resolver, style);
            NodeKind::Paragraph { children }
        }

        HtmlTag::Code => {
            let code = elem.text_content();
            NodeKind::InlineCode { code }
        }

        _ => {
            // fallback: 作为段落处理
            let children = convert_inline_children(&elem.children, resolver, style);
            NodeKind::Paragraph { children }
        }
    }
}

/// 块级标签 → NodeKind 映射
///
/// 处理需要块级子元素的标签（Div, H1-H6, Ul, Ol, Li, Pre 等）
fn convert_tag_block(elem: &HtmlElement, style: &Style, resolver: &mut StyleResolver) -> NodeKind {
    match elem.tag {
        // 标题
        HtmlTag::H1 | HtmlTag::H2 | HtmlTag::H3 | HtmlTag::H4 | HtmlTag::H5 | HtmlTag::H6 => {
            let level = heading_level(elem.tag);
            let children = convert_inline_children(&elem.children, resolver, style);
            NodeKind::Heading { level, children }
        }

        // 列表
        HtmlTag::Ul | HtmlTag::Ol => {
            let ordered = elem.tag == HtmlTag::Ol;
            let start = elem.attrs.get("start").and_then(|s| s.parse::<u32>().ok());
            let children = convert_children(&elem.children, resolver, style);
            NodeKind::List {
                ordered,
                start,
                children,
            }
        }

        HtmlTag::Li => {
            let (checked, remaining) = extract_checkbox(&elem.children);
            if let Some(checked) = checked {
                let children = convert_inline_children(&remaining, resolver, style);
                NodeKind::TaskListItem { checked, children }
            } else {
                let children = convert_children(&elem.children, resolver, style);
                NodeKind::ListItem { children }
            }
        }

        // 定义列表（<dl>）：配对 <dt> 术语与 <dd> 定义
        HtmlTag::Dl => {
            let mut items: Vec<DefinitionItem> = Vec::new();
            let mut current_term: Option<Vec<Node>> = None;
            for child in &elem.children {
                if let HtmlNode::Element(e) = child {
                    match e.tag {
                        HtmlTag::Dt => {
                            if let Some(term) = current_term.take() {
                                items.push(DefinitionItem {
                                    term,
                                    definition: Vec::new(),
                                });
                            }
                            current_term =
                                Some(convert_inline_children(&e.children, resolver, style));
                        }
                        HtmlTag::Dd => {
                            let definition = convert_children(&e.children, resolver, style);
                            match current_term.take() {
                                Some(term) => {
                                    items.push(DefinitionItem { term, definition });
                                }
                                None => {
                                    // 孤儿 <dd>（前面无 <dt>）：作为空术语项
                                    items.push(DefinitionItem {
                                        term: Vec::new(),
                                        definition,
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            // 末尾残留 <dt>（无对应 <dd>）
            if let Some(term) = current_term.take() {
                items.push(DefinitionItem {
                    term,
                    definition: Vec::new(),
                });
            }
            NodeKind::DefinitionList { items }
        }

        // 代码块
        HtmlTag::Pre => {
            let (code, lang) = extract_code_block(elem);
            NodeKind::CodeBlock { code, lang }
        }

        // 引用
        HtmlTag::Blockquote => {
            let children = convert_children(&elem.children, resolver, style);
            NodeKind::Blockquote { children }
        }

        // 分隔线
        HtmlTag::Hr => NodeKind::ThematicBreak,

        // 图片
        HtmlTag::Img => {
            let src = elem.attrs.get("src").cloned().unwrap_or_default();
            let alt = elem.attrs.get("alt").cloned().unwrap_or_default();
            let title = elem.attrs.get("title").cloned();
            NodeKind::Image { src, alt, title }
        }

        // 表格
        HtmlTag::Table => {
            let children = convert_children(&elem.children, resolver, style);
            let align = extract_table_align(elem);
            NodeKind::Table { children, align }
        }

        HtmlTag::Tr => {
            let children = convert_children(&elem.children, resolver, style);
            NodeKind::TableRow { children }
        }

        // 居中容器
        HtmlTag::Center => {
            let children = convert_children(&elem.children, resolver, style);
            NodeKind::Center { children }
        }

        // 通用块级容器
        HtmlTag::Div => {
            // 脚注定义区：<div id="fn-def-<id>" class="footnote-def"> → 小号脚注定义块（末尾聚合）。
            let is_footnote_def = elem
                .attrs
                .get("class")
                .map(|c| c.split_whitespace().any(|w| w == "footnote-def"))
                .unwrap_or(false);
            if is_footnote_def {
                let id = elem.attrs.get("id").cloned().unwrap_or_default();
                let mut f_style = style.clone();
                f_style.font_size_pt = (style.font_size_pt * 0.85).max(7.0);
                let children = convert_children(&elem.children, resolver, &f_style);
                return NodeKind::FootnoteDef { id, children };
            }
            let children = convert_children(&elem.children, resolver, style);
            NodeKind::Paragraph { children }
        }

        // HTML5 语义结构标签
        HtmlTag::Section
        | HtmlTag::Article
        | HtmlTag::Nav
        | HtmlTag::Aside
        | HtmlTag::Header
        | HtmlTag::Footer
        | HtmlTag::Main
        | HtmlTag::Figure
        | HtmlTag::Figcaption => {
            let children = convert_children(&elem.children, resolver, style);
            NodeKind::Paragraph { children }
        }

        // <br> — 行内换行
        HtmlTag::Br => NodeKind::LineBreak,

        // Input — 仅在 <li> 中作为 checkbox 有意义
        HtmlTag::Input => NodeKind::Text {
            text: String::new(),
        },

        // 未知标签
        HtmlTag::Unknown => {
            let children = convert_children(&elem.children, resolver, style);
            NodeKind::Paragraph { children }
        }

        // 其余行内标签：Span 容器
        _ => {
            let children = convert_inline_children(&elem.children, resolver, style);
            NodeKind::Span { children }
        }
    }
}

/// 转换块级子元素
fn convert_children(
    children: &[HtmlNode],
    resolver: &mut StyleResolver,
    parent_style: &Style,
) -> Vec<Node> {
    let mut result = Vec::new();

    for child in children {
        match child {
            HtmlNode::Text(text) => {
                let text = collapse_whitespace(text);
                if !text.is_empty() {
                    let style = resolver.resolve_text(parent_style);
                    result.push(Node::new(NodeKind::Text { text }, style, true));
                }
            }
            HtmlNode::Element(elem) => {
                // 跳过不参与布局的元素
                if matches!(elem.tag, HtmlTag::Head | HtmlTag::Style) {
                    continue;
                }
                // thead/tbody 的子元素直接提升
                if matches!(elem.tag, HtmlTag::Thead | HtmlTag::Tbody) {
                    result.extend(convert_children(&elem.children, resolver, parent_style));
                } else {
                    result.push(convert_element(elem, resolver, parent_style));
                }
            }
        }
    }

    // 块级文本流的真正起点/终点丢弃孤立空白（CSS 行首/行尾语义）
    trim_text_sequence(&mut result, true);
    result
}

/// 转换行内子元素
fn convert_inline_children(
    children: &[HtmlNode],
    resolver: &mut StyleResolver,
    parent_style: &Style,
) -> Vec<Node> {
    let mut result = Vec::new();
    for child in children {
        match child {
            HtmlNode::Text(text) => {
                let text = collapse_whitespace(text);
                if !text.is_empty() {
                    let style = resolver.resolve_text(parent_style);
                    result.push(Node::new(NodeKind::Text { text }, style, true));
                }
            }
            HtmlNode::Element(elem) => {
                if matches!(elem.tag, HtmlTag::Head | HtmlTag::Style) {
                    continue;
                }
                result.push(convert_element(elem, resolver, parent_style));
            }
        }
    }
    // 行内流：边界空格必须保留（与父级相邻文本跨边界合并），
    // 只删除折叠后为空的文本节点。
    trim_text_sequence(&mut result, false);
    result
}

// ─── 辅助函数 ─────────────────────────────────────────────

/// 对兄弟节点序列做边界空白修剪。
///
/// `block_boundary=true` 表示该序列是一个**块级文本流**的边界：
/// 去掉首节点（若为 Text）的开头空白、尾节点（若为 Text）的结尾空白，
/// 即 CSS 行首/行尾孤立的折叠空格不渲染。中间节点的边界空格保留，
/// 保证跨片段分词（`Hello` + `<b> world </b>` 的衔接空格不丢）。
///
/// `block_boundary=false` 表示行内流：边界空格保留（与父级相邻文本
/// 跨边界合并），只删除折叠后为空的文本节点。
///
/// 空文本节点（折叠后为空白串）始终被移除。
fn trim_text_sequence(nodes: &mut Vec<Node>, block_boundary: bool) {
    // 1) 移除空文本节点
    nodes.retain(|n| match &n.kind {
        NodeKind::Text { text } => !text.trim().is_empty(),
        _ => true,
    });

    // 2) 块级流边界：首节点去开头空白、尾节点去结尾空白
    if block_boundary {
        if let Some(first) = nodes.first_mut() {
            strip_leading_space(first);
        }
        if let Some(last) = nodes.last_mut() {
            strip_trailing_space(last);
        }
    }

    // 3) 再次移除因去空而变空的文本节点（例如首尾节点原本就是纯空白）
    nodes.retain(|n| match &n.kind {
        NodeKind::Text { text } => !text.is_empty(),
        _ => true,
    });
}

/// 去掉节点文本流的**开头**空白。
///
/// 若节点是 Text，直接 trim_start；若是行内容器（Strong/Em/Span/...），
/// 穿透到其第一个产生内容的子节点（CSS 行首空白在块级边界被丢弃）。
fn strip_leading_space(node: &mut Node) {
    if let NodeKind::Text { text } = &mut node.kind {
        let trimmed = text.trim_start();
        if trimmed != text.as_str() {
            *text = trimmed.to_string();
        }
        return;
    }
    match &mut node.kind {
        NodeKind::Span { children }
        | NodeKind::Strong { children }
        | NodeKind::Emphasis { children }
        | NodeKind::Delete { children }
        | NodeKind::Link { children, .. }
        | NodeKind::Subscript { children }
        | NodeKind::Superscript { children }
        | NodeKind::Paragraph { children } => {
            if let Some(first) = children.first_mut() {
                strip_leading_space(first);
            }
        }
        _ => {}
    }
}

/// 去掉节点文本流的**结尾**空白。
///
/// 若节点是 Text，直接 trim_end；若是行内容器，穿透到其最后一个
/// 产生内容的子节点。
fn strip_trailing_space(node: &mut Node) {
    if let NodeKind::Text { text } = &mut node.kind {
        let trimmed = text.trim_end();
        if trimmed != text.as_str() {
            *text = trimmed.to_string();
        }
        return;
    }
    match &mut node.kind {
        NodeKind::Span { children }
        | NodeKind::Strong { children }
        | NodeKind::Emphasis { children }
        | NodeKind::Delete { children }
        | NodeKind::Link { children, .. }
        | NodeKind::Subscript { children }
        | NodeKind::Superscript { children }
        | NodeKind::Paragraph { children } => {
            if let Some(last) = children.last_mut() {
                strip_trailing_space(last);
            }
        }
        _ => {}
    }
}

/// 获取标题级别
fn heading_level(tag: HtmlTag) -> u8 {
    match tag {
        HtmlTag::H1 => 1,
        HtmlTag::H2 => 2,
        HtmlTag::H3 => 3,
        HtmlTag::H4 => 4,
        HtmlTag::H5 => 5,
        HtmlTag::H6 => 6,
        _ => 1,
    }
}

/// 从 <li> 的子节点中提取 checkbox 信息
///
/// 从脚注引用元素中提取（数字, 内部锚点 url）。
///
/// 结构：`<sup class="footnote-ref"><a href="#fn-def-<id>">N</a></sup>`。
/// 若 `<a>` 缺失，回退为 (children 文本, 空 url)。
fn footnote_ref_content(elem: &HtmlElement) -> (String, Option<String>) {
    for child in &elem.children {
        if let HtmlNode::Element(e) = child
            && e.tag == HtmlTag::A
        {
            let num: String = e
                .children
                .iter()
                .filter_map(|n| match n {
                    HtmlNode::Text(t) => Some(t.clone()),
                    _ => None,
                })
                .collect();
            let href = e.attrs.get("href").cloned();
            return (num, href);
        }
    }
    // 无 <a>：退化为纯文本数字，无链接
    let num: String = elem
        .children
        .iter()
        .filter_map(|n| match n {
            HtmlNode::Text(t) => Some(t.clone()),
            _ => None,
        })
        .collect();
    (num, None)
}

/// 返回 (checked, remaining_children)：
/// - 如果第一个子元素是 `<input type="checkbox">`，返回 `(Some(checked), 剩余子节点)`
/// - 否则返回 `(None, 原始子节点)`
fn extract_checkbox(children: &[HtmlNode]) -> (Option<bool>, Vec<HtmlNode>) {
    let mut found_checkbox = false;
    let mut checked = false;
    let mut remaining = Vec::new();

    for child in children {
        if !found_checkbox {
            if let HtmlNode::Element(elem) = child
                && elem.tag == HtmlTag::Input
                && elem.attrs.get("type").map(|s| s.as_str()) == Some("checkbox")
            {
                found_checkbox = true;
                checked = elem.attrs.contains_key("checked");
                continue;
            }
            // checkbox 前的空白文本也跳过
            if let HtmlNode::Text(t) = child
                && t.trim().is_empty()
            {
                continue;
            }
        }
        remaining.push(child.clone());
    }

    if found_checkbox {
        (Some(checked), remaining)
    } else {
        (None, children.to_vec())
    }
}

/// 从 <pre> 元素中提取代码块内容
fn extract_code_block(elem: &HtmlElement) -> (String, Option<String>) {
    // 优先查找 <code> 子元素
    for child in &elem.children {
        if let HtmlNode::Element(code_elem) = child
            && code_elem.tag == HtmlTag::Code
        {
            let lang = code_elem
                .attrs
                .get("class")
                .and_then(|c| c.strip_prefix("language-").map(|s| s.to_string()));
            let code = code_elem.text_content();
            return (code, lang);
        }
    }
    // 回退：代码文本直接在 <pre> 自身（无 <code> 子元素），语言从 pre 的 class 提取。
    let lang = elem
        .attrs
        .get("class")
        .and_then(|c| c.strip_prefix("language-").map(|s| s.to_string()));
    (elem.text_content(), lang)
}

/// 从表格元素中提取列对齐方式
fn extract_table_align(elem: &HtmlElement) -> Vec<TextAlign> {
    let mut aligns = Vec::new();

    for child in &elem.children {
        if let HtmlNode::Element(row) = child {
            if row.tag == HtmlTag::Tr {
                for cell in &row.children {
                    if let HtmlNode::Element(cell_elem) = cell
                        && matches!(cell_elem.tag, HtmlTag::Th | HtmlTag::Td)
                    {
                        let align = cell_elem
                            .inline_style()
                            .and_then(parse_text_align_from_inline)
                            .unwrap_or(TextAlign::Left);
                        aligns.push(align);
                    }
                }
                break;
            }
            // thead/tbody 内
            if matches!(row.tag, HtmlTag::Thead | HtmlTag::Tbody) {
                for gc in &row.children {
                    if let HtmlNode::Element(inner_row) = gc
                        && inner_row.tag == HtmlTag::Tr
                    {
                        for cell in &inner_row.children {
                            if let HtmlNode::Element(cell_elem) = cell
                                && matches!(cell_elem.tag, HtmlTag::Th | HtmlTag::Td)
                            {
                                let align = cell_elem
                                    .inline_style()
                                    .and_then(parse_text_align_from_inline)
                                    .unwrap_or(TextAlign::Left);
                                aligns.push(align);
                            }
                        }
                        break;
                    }
                }
                break;
            }
        }
    }

    aligns
}

fn parse_text_align_from_inline(inline_css: &str) -> Option<TextAlign> {
    for decl in inline_css.split(';') {
        let decl = decl.trim();
        if let Some(colon_pos) = decl.find(':') {
            let prop = decl[..colon_pos].trim();
            let val = decl[colon_pos + 1..].trim();
            if prop == "text-align" {
                return Some(match val {
                    "left" => TextAlign::Left,
                    "center" => TextAlign::Center,
                    "right" => TextAlign::Right,
                    "justify" => TextAlign::Justify,
                    _ => TextAlign::Left,
                });
            }
        }
    }
    None
}

// ─── NodeKind 辅助方法 ────────────────────────────────────

/// 判断 NodeKind 是否可分割（跨页）
impl NodeKind {
    fn is_splittable(&self) -> bool {
        matches!(
            self,
            NodeKind::Paragraph { .. }
                | NodeKind::List { .. }
                | NodeKind::ListItem { .. }
                | NodeKind::TaskListItem { .. }
                | NodeKind::Blockquote { .. }
                | NodeKind::Span { .. }
                | NodeKind::Strong { .. }
                | NodeKind::Emphasis { .. }
                | NodeKind::Delete { .. }
                | NodeKind::Link { .. }
                | NodeKind::Center { .. }
                | NodeKind::Subscript { .. }
                | NodeKind::Superscript { .. }
                | NodeKind::Text { .. }
                | NodeKind::LineBreak
        )
    }
}

// ─── Mark 标签样式预置 ────────────────────────────────────

/// Mark 标签的样式调整（黄色背景高亮）
pub fn apply_mark_style(style: &mut Style) {
    style.background_color = Some(crate::color::Color::new(255, 255, 0));
}

/// Small 标签的样式调整（缩小字号）
pub fn apply_small_style(style: &mut Style) {
    style.font_size_pt *= 0.85;
}

/// Sub/Sup 标签的样式调整
pub fn apply_sub_sup_style(style: &mut Style) {
    style.font_size_pt *= 0.75;
}

/// U 标签的样式调整（下划线）
pub fn apply_underline_style(style: &mut Style) {
    style.text_decoration = TextDecoration::Underline;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::parser::parse_html;

    /// 辅助：从 HTML 字符串构建完整的 Styled Node 树
    fn html_to_styled(html: &str) -> Node {
        let doc = parse_html(html);
        let css = crate::ast::presets::DEFAULT_CSS;
        let engine = CssEngine::new(css).unwrap();
        html_to_styled_nodes(&doc, &engine)
    }

    /// 辅助：在 Node 树中查找第一个匹配的 NodeKind 变体
    fn find_node<F>(node: &Node, predicate: F) -> Option<&Node>
    where
        F: Fn(&NodeKind) -> bool + Copy,
    {
        if predicate(&node.kind) {
            return Some(node);
        }
        match &node.kind {
            NodeKind::Document { children }
            | NodeKind::Paragraph { children }
            | NodeKind::Heading { children, .. }
            | NodeKind::List { children, .. }
            | NodeKind::ListItem { children }
            | NodeKind::Blockquote { children }
            | NodeKind::Table { children, .. }
            | NodeKind::TableRow { children }
            | NodeKind::Strong { children }
            | NodeKind::Emphasis { children }
            | NodeKind::Delete { children }
            | NodeKind::Link { children, .. }
            | NodeKind::Span { children }
            | NodeKind::Center { children } => {
                for child in children {
                    if let Some(found) = find_node(child, predicate) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
    }

    // ─── 测试用例 ──────────────────────────────────────────

    #[test]
    fn test_paragraph_to_styled_node() {
        let node = html_to_styled("<p>Hello world</p>");
        let para = find_node(&node, |k| matches!(k, NodeKind::Paragraph { .. })).unwrap();
        assert!(matches!(&para.kind, NodeKind::Paragraph { .. }));
    }

    #[test]
    fn test_heading_to_styled_node() {
        let node = html_to_styled("<h1>Title</h1>");
        let h1 = find_node(&node, |k| matches!(k, NodeKind::Heading { level: 1, .. })).unwrap();
        assert!(matches!(&h1.kind, NodeKind::Heading { level: 1, .. }));
    }

    #[test]
    fn test_div_as_paragraph() {
        let node = html_to_styled("<div>content</div>");
        let para = find_node(&node, |k| matches!(k, NodeKind::Paragraph { .. })).unwrap();
        assert!(matches!(&para.kind, NodeKind::Paragraph { .. }));
    }

    #[test]
    fn test_unordered_list_to_styled_node() {
        let node = html_to_styled("<ul><li>item</li></ul>");
        let list = find_node(&node, |k| matches!(k, NodeKind::List { .. })).unwrap();
        assert!(matches!(&list.kind, NodeKind::List { ordered: false, .. }));
    }

    #[test]
    fn test_ordered_list_to_styled_node() {
        let node = html_to_styled("<ol><li>item</li></ol>");
        let list = find_node(&node, |k| matches!(k, NodeKind::List { .. })).unwrap();
        assert!(matches!(&list.kind, NodeKind::List { ordered: true, .. }));
    }

    #[test]
    fn test_task_list_unchecked_to_styled_node() {
        let node = html_to_styled("<ul><li><input type=\"checkbox\"> task</li></ul>");
        let item = find_node(&node, |k| matches!(k, NodeKind::TaskListItem { .. })).unwrap();
        assert!(matches!(
            &item.kind,
            NodeKind::TaskListItem { checked: false, .. }
        ));
    }

    #[test]
    fn test_task_list_checked_to_styled_node() {
        let node = html_to_styled("<ul><li><input type=\"checkbox\" checked> task</li></ul>");
        let item = find_node(&node, |k| matches!(k, NodeKind::TaskListItem { .. })).unwrap();
        assert!(matches!(
            &item.kind,
            NodeKind::TaskListItem { checked: true, .. }
        ));
    }

    #[test]
    fn test_task_list_mixed_with_regular_items() {
        let html = "<ul><li><input type=\"checkbox\"> todo</li><li>regular</li><li><input type=\"checkbox\" checked> done</li></ul>";
        let node = html_to_styled(html);
        let tasks: Vec<&Node> =
            find_all_nodes(&node, |k| matches!(k, NodeKind::TaskListItem { .. }));
        assert_eq!(tasks.len(), 2);
        assert!(matches!(
            &tasks[0].kind,
            NodeKind::TaskListItem { checked: false, .. }
        ));
        assert!(matches!(
            &tasks[1].kind,
            NodeKind::TaskListItem { checked: true, .. }
        ));
    }

    #[test]
    fn test_code_block_to_styled_node() {
        let node = html_to_styled("<pre><code>let x = 1;</code></pre>");
        let code = find_node(&node, |k| matches!(k, NodeKind::CodeBlock { .. })).unwrap();
        assert!(matches!(&code.kind, NodeKind::CodeBlock { code, .. } if code == "let x = 1;"));
    }

    #[test]
    fn test_code_block_preserves_exact_content() {
        // 多行代码块，换行和缩进必须原样保留
        // 直接构造 HTML，验证 styled 转换后 CodeBlock 内容精确匹配
        let html = concat!(
            "<pre><code class=\"language-rust\">fn main() {\n",
            "    println!(\"Hello, World!\");\n",
            "}\n",
            "</code></pre>"
        );
        let styled = html_to_styled(html);
        let code_node = find_node(&styled, |k| matches!(k, NodeKind::CodeBlock { .. })).unwrap();
        let expected = concat!("fn main() {\n", "    println!(\"Hello, World!\");\n", "}\n");
        assert!(matches!(&code_node.kind, NodeKind::CodeBlock { code, .. } if code == expected));
    }

    #[test]
    fn test_blockquote_to_styled_node() {
        let node = html_to_styled("<blockquote>quote</blockquote>");
        let bq = find_node(&node, |k| matches!(k, NodeKind::Blockquote { .. })).unwrap();
        assert!(matches!(&bq.kind, NodeKind::Blockquote { .. }));
    }

    #[test]
    fn test_hr_to_styled_node() {
        let node = html_to_styled("<hr>");
        let hr = find_node(&node, |k| matches!(k, NodeKind::ThematicBreak)).unwrap();
        assert!(matches!(&hr.kind, NodeKind::ThematicBreak));
    }

    #[test]
    fn test_image_to_styled_node() {
        let node = html_to_styled("<img src=\"test.png\" alt=\"test\">");
        let img = find_node(&node, |k| matches!(k, NodeKind::Image { .. })).unwrap();
        assert!(
            matches!(&img.kind, NodeKind::Image { src, alt, .. } if src == "test.png" && alt == "test")
        );
    }

    #[test]
    fn test_link_to_styled_node() {
        let node = html_to_styled("<a href=\"https://example.com\">link</a>");
        let link = find_node(&node, |k| matches!(k, NodeKind::Link { .. })).unwrap();
        assert!(matches!(&link.kind, NodeKind::Link { url, .. } if url == "https://example.com"));
    }

    #[test]
    fn test_inline_code_to_styled_node() {
        let node = html_to_styled("<p>text <code>code</code></p>");
        let code = find_node(&node, |k| matches!(k, NodeKind::InlineCode { .. })).unwrap();
        assert!(matches!(&code.kind, NodeKind::InlineCode { code } if code == "code"));
    }

    #[test]
    fn test_inline_styles_in_paragraph() {
        let node = html_to_styled("<p><strong>bold</strong> and <em>italic</em></p>");
        let strong = find_node(&node, |k| matches!(k, NodeKind::Strong { .. })).unwrap();
        let em = find_node(&node, |k| matches!(k, NodeKind::Emphasis { .. })).unwrap();
        assert!(matches!(&strong.kind, NodeKind::Strong { .. }));
        assert!(matches!(&em.kind, NodeKind::Emphasis { .. }));
    }

    #[test]
    fn test_whitespace_preserved_around_inline_style() {
        // 跨样式片段的分词空格必须保留：
        // `This is a <b>Markdown</b> document.`
        // → `This is a` + `Markdown` + ` document.`（前后空格都在）
        let node = html_to_styled("<p>This is a <strong>Markdown</strong> document.</p>");
        let para = find_node(&node, |k| matches!(k, NodeKind::Paragraph { .. })).unwrap();
        let children = match &para.kind {
            NodeKind::Paragraph { children } => children,
            _ => unreachable!("already matched Paragraph"),
        };

        // 3 个直接子节点：Text("This is a ") / Strong / Text(" document.")
        assert_eq!(children.len(), 3, "paragraph should have 3 direct children");
        let first = match &children[0].kind {
            NodeKind::Text { text } => text.as_str(),
            _ => panic!("first child should be text"),
        };
        let last = match &children[2].kind {
            NodeKind::Text { text } => text.as_str(),
            _ => panic!("last child should be text"),
        };
        assert_eq!(
            first, "This is a ",
            "leading text trailing space must survive"
        );
        assert_eq!(
            last, " document.",
            "trailing text leading space must survive"
        );
    }

    #[test]
    fn test_delete_strikethrough() {
        let node = html_to_styled("<p><del>deleted</del></p>");
        let del = find_node(&node, |k| matches!(k, NodeKind::Delete { .. })).unwrap();
        assert!(matches!(&del.kind, NodeKind::Delete { .. }));
    }

    #[test]
    fn test_span_as_inline_container() {
        let node = html_to_styled("<p><span>text</span></p>");
        let span = find_node(&node, |k| matches!(k, NodeKind::Span { .. })).unwrap();
        assert!(matches!(&span.kind, NodeKind::Span { .. }));
    }

    #[test]
    fn test_center_container() {
        let node = html_to_styled("<center>centered</center>");
        let center = find_node(&node, |k| matches!(k, NodeKind::Center { .. })).unwrap();
        assert!(matches!(&center.kind, NodeKind::Center { .. }));
    }

    #[test]
    fn test_table_to_styled_node() {
        let node = html_to_styled("<table><tr><th>H</th></tr></table>");
        let table = find_node(&node, |k| matches!(k, NodeKind::Table { .. })).unwrap();
        assert!(matches!(&table.kind, NodeKind::Table { .. }));
    }

    #[test]
    fn test_heading_levels_map_correctly() {
        let html = "<h1>a</h1><h2>b</h2><h3>c</h3><h4>d</h4><h5>e</h5><h6>f</h6>";
        let node = html_to_styled(html);

        fn find_heading(node: &Node, level: u8) -> Option<&Node> {
            find_all_nodes(
                node,
                |k| matches!(k, NodeKind::Heading { level: l, .. } if *l == level),
            )
            .into_iter()
            .next()
        }

        assert!(find_heading(&node, 1).is_some());
        assert!(find_heading(&node, 2).is_some());
        assert!(find_heading(&node, 3).is_some());
        assert!(find_heading(&node, 4).is_some());
        assert!(find_heading(&node, 5).is_some());
        assert!(find_heading(&node, 6).is_some());
    }

    #[test]
    fn test_css_applied_to_styled_node() {
        let doc = parse_html("<p style=\"color: red;\">text</p>");
        // 使用包含 color 预设的 CSS
        let css = "p { font-size: 12pt; }";
        let engine = CssEngine::new(css).unwrap();
        let node = html_to_styled_nodes(&doc, &engine);
        let para = find_node(&node, |k| matches!(k, NodeKind::Paragraph { .. })).unwrap();
        // 内联样式应生效
        assert_eq!(para.style.color.r, 255);
        assert_eq!(para.style.color.g, 0);
        assert_eq!(para.style.color.b, 0);
    }

    #[test]
    fn test_class_selector_applied() {
        let doc = parse_html("<p class=\"highlight\">text</p>");
        let css = r#"
            .highlight { background-color: #ffff00; }
            p { font-size: 12pt; }
        "#;
        let engine = CssEngine::new(css).unwrap();
        let node = html_to_styled_nodes(&doc, &engine);
        let para = find_node(&node, |k| matches!(k, NodeKind::Paragraph { .. })).unwrap();
        assert!(para.style.background_color.is_some());
    }

    #[test]
    fn test_inline_style_applied() {
        let doc = parse_html("<p style=\"text-align: center;\">text</p>");
        let css = "p { font-size: 12pt; }";
        let engine = CssEngine::new(css).unwrap();
        let node = html_to_styled_nodes(&doc, &engine);
        let para = find_node(&node, |k| matches!(k, NodeKind::Paragraph { .. })).unwrap();
        assert_eq!(para.style.text_align, TextAlign::Center);
    }

    /// 辅助：收集所有匹配的节点
    fn find_all_nodes<F>(node: &Node, predicate: F) -> Vec<&Node>
    where
        F: Fn(&NodeKind) -> bool + Copy,
    {
        let mut result = Vec::new();
        if predicate(&node.kind) {
            result.push(node);
        }
        match &node.kind {
            NodeKind::Document { children }
            | NodeKind::Paragraph { children }
            | NodeKind::Heading { children, .. }
            | NodeKind::List { children, .. }
            | NodeKind::ListItem { children }
            | NodeKind::TaskListItem { children, .. }
            | NodeKind::Blockquote { children }
            | NodeKind::Table { children, .. }
            | NodeKind::TableRow { children }
            | NodeKind::Strong { children }
            | NodeKind::Emphasis { children }
            | NodeKind::Delete { children }
            | NodeKind::Link { children, .. }
            | NodeKind::Span { children }
            | NodeKind::Center { children } => {
                for child in children {
                    result.extend(find_all_nodes(child, predicate));
                }
            }
            _ => {}
        }
        result
    }
}
