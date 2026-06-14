//! 简化 AST 模块
//!
//! 将 HtmlElement 树转换为简化的、带样式的 Node 树。
//! 设计目标：
//! - 简化节点类型（heading、paragraph 等都可表示为带样式的文本块）
//! - 每个节点携带 Style，包含布局所需的所有样式信息
//! - 样式来源：CSS 样式表解析（内置 + 用户覆盖）

use super::css::*;
use super::style::*;
use crate::html::ast::{HtmlDocument, HtmlElement, HtmlNode, HtmlTag};

// ─── 简化 AST 节点定义 ───

/// 带样式的简化 AST 节点
///
/// 这是三层 AST 架构的 Layer 2。
/// 布局引擎只消费此结构，不关心 HTML 或 CSS 细节。
#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub style: Style,
    /// 布局时是否可分割
    /// - true: 可在页面间分割（如段落、列表项）
    /// - false: 不可分割，必须保持在同一页（如标题、表格行）
    pub splittable: bool,
}

impl Node {
    pub fn new(kind: NodeKind, style: Style, splittable: bool) -> Self {
        Self {
            kind,
            style,
            splittable,
        }
    }

    /// 获取文本内容的拼接值
    pub fn text_content(&self) -> String {
        self.kind.text_content()
    }
}

/// 简化的节点类型
///
/// 相比完整的 HTML AST，此枚举做了以下简化：
/// 1. 合并了语义上相似的节点（如 Heading 和 Paragraph 都是文本容器）
/// 2. 保留了布局引擎需要的类型信息
/// 3. 每个节点通过 Style 区分视觉表现
#[derive(Debug, Clone)]
pub enum NodeKind {
    /// 文档根节点
    Document { children: Vec<Node> },

    /// 标题（h1-h6）
    /// 通过 style 中的 font_size, font_weight 等属性区分级别
    Heading { level: u8, children: Vec<Node> },

    /// 段落
    Paragraph { children: Vec<Node> },

    /// 列表
    List {
        ordered: bool,
        start: Option<u32>,
        children: Vec<Node>,
    },

    /// 列表项
    ListItem { children: Vec<Node> },

    /// 任务列表项（GFM 复选框）
    TaskListItem { checked: bool, children: Vec<Node> },

    /// 图片
    Image {
        src: String,
        alt: String,
        title: Option<String>,
    },

    /// 代码块
    CodeBlock { code: String, lang: Option<String> },

    /// 引用块
    Blockquote { children: Vec<Node> },

    /// 分隔线
    ThematicBreak,

    /// 表格
    Table {
        children: Vec<Node>,
        align: Vec<TextAlign>,
    },

    /// 表格行
    TableRow { children: Vec<Node> },

    // ── 内联节点 ──
    /// 纯文本（叶节点）
    Text { text: String },

    /// 加粗
    Strong { children: Vec<Node> },

    /// 斜体
    Emphasis { children: Vec<Node> },

    /// 行内代码
    InlineCode { code: String },

    /// 链接
    Link {
        url: String,
        title: Option<String>,
        children: Vec<Node>,
    },

    /// 删除线
    Delete { children: Vec<Node> },

    /// 下标
    Subscript { children: Vec<Node> },
    /// 上标
    Superscript { children: Vec<Node> },

    // ── HTML 容器节点 ──
    /// 行内容器 (<span>)
    Span { children: Vec<Node> },
    /// 居中块级容器 (<center>)
    Center { children: Vec<Node> },
    /// 通用块级容器 (<div>, <section>, <article>)
    Container { children: Vec<Node> },

    /// 行内换行 (<br>)
    LineBreak,
}

impl NodeKind {
    /// 获取文本内容的拼接值
    pub fn text_content(&self) -> String {
        match self {
            NodeKind::Text { text } => text.clone(),
            NodeKind::LineBreak => "\n".to_string(),
            NodeKind::Strong { children }
            | NodeKind::Emphasis { children }
            | NodeKind::Link { children, .. }
            | NodeKind::Delete { children }
            | NodeKind::Subscript { children }
            | NodeKind::Superscript { children } => {
                let mut s = String::new();
                for child in children {
                    s.push_str(&child.text_content());
                }
                s
            }
            NodeKind::InlineCode { code } => code.clone(),
            NodeKind::Heading { children, .. }
            | NodeKind::Paragraph { children }
            | NodeKind::ListItem { children }
            | NodeKind::TaskListItem { children, .. }
            | NodeKind::Blockquote { children }
            | NodeKind::TableRow { children }
            | NodeKind::Span { children }
            | NodeKind::Center { children }
            | NodeKind::Container { children } => {
                let mut s = String::new();
                for child in children {
                    s.push_str(&child.text_content());
                }
                s
            }
            _ => String::new(),
        }
    }
}

// ─── HtmlElement → Node 转换 ──────────────────────────────

/// 从 HtmlDocument 构建简化 Node 树
///
/// 使用给定的样式解析器为每个节点解析样式。
pub fn build_from_html(doc: &HtmlDocument, resolver: &StyleResolver) -> Node {
    build_element(&doc.root, resolver, &[], &Style::default())
}

fn build_element(
    elem: &HtmlElement,
    resolver: &StyleResolver,
    ancestor_tags: &[String],
    parent_style: &Style,
) -> Node {
    let tag_name = elem.tag.as_str();
    let classes = elem.classes();
    let mut style = resolver.resolve_style(tag_name, &classes, ancestor_tags, parent_style);

    // 处理内联 style 属性
    if let Some(inline_css) = elem.inline_style() {
        resolver.apply_inline_style(&mut style, inline_css);
    }

    let mut new_ancestors = ancestor_tags.to_vec();
    new_ancestors.push(tag_name.to_string());

    match elem.tag {
        HtmlTag::Html | HtmlTag::Body => {
            // Extract content children from <body>, skip <head>
            let content_nodes: Vec<&HtmlNode> = elem
                .children
                .iter()
                .flat_map(|child| {
                    if let HtmlNode::Element(e) = child {
                        if e.tag == HtmlTag::Body || e.tag == HtmlTag::Html {
                            // Inline the body/html grandchildren
                            e.children.iter().collect::<Vec<_>>()
                        } else if e.tag == HtmlTag::Head {
                            vec![] // Skip head
                        } else {
                            vec![child]
                        }
                    } else {
                        vec![child]
                    }
                })
                .collect();
            // Convert to owned HtmlNodes for build_children
            let owned: Vec<HtmlNode> = content_nodes.into_iter().cloned().collect();
            let children = build_children(&owned, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Document { children }, style, false)
        }

        HtmlTag::H1 | HtmlTag::H2 | HtmlTag::H3 | HtmlTag::H4 | HtmlTag::H5 | HtmlTag::H6 => {
            let level = heading_level(elem.tag);
            let children = build_inline_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Heading { level, children }, style, false)
        }

        HtmlTag::P => {
            let children = build_inline_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Paragraph { children }, style, true)
        }

        HtmlTag::Ul | HtmlTag::Ol => {
            let ordered = elem.tag == HtmlTag::Ol;
            // 尝试提取 start 属性
            let start = elem.attrs.get("start").and_then(|s| s.parse::<u32>().ok());
            let children = build_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(
                NodeKind::List {
                    ordered,
                    start,
                    children,
                },
                style,
                true,
            )
        }

        HtmlTag::Li => {
            // 检测是否为任务列表项（第一个子节点是 <input type="checkbox">）
            if let Some(first) = elem.children.first()
                && let HtmlNode::Element(child_elem) = first
                && child_elem.tag == HtmlTag::Input
                && child_elem.attrs.get("type").map(|s| s.as_str()) == Some("checkbox")
            {
                let checked = child_elem.attrs.contains_key("checked");
                // 跳过 checkbox input 节点，用剩余子节点
                let children =
                    build_inline_children(&elem.children[1..], resolver, &new_ancestors, &style);
                return Node::new(NodeKind::TaskListItem { checked, children }, style, true);
            }
            let children = build_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::ListItem { children }, style, true)
        }

        HtmlTag::Pre => {
            // <pre> 的子节点应该是 <code>
            let (code_text, lang) = extract_code_content(&elem.children);
            Node::new(
                NodeKind::CodeBlock {
                    code: code_text,
                    lang,
                },
                style,
                false,
            )
        }

        HtmlTag::Img => {
            let src = elem.attrs.get("src").cloned().unwrap_or_default();
            let alt = elem.attrs.get("alt").cloned().unwrap_or_default();
            let title = elem.attrs.get("title").cloned();
            Node::new(NodeKind::Image { src, alt, title }, style, false)
        }

        HtmlTag::Blockquote => {
            let children = build_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Blockquote { children }, style, true)
        }

        HtmlTag::Hr => Node::new(NodeKind::ThematicBreak, style, false),

        HtmlTag::Table => {
            let align = extract_table_align(&elem.children);
            let children = build_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Table { children, align }, style, false)
        }

        HtmlTag::Tr => {
            let children = build_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::TableRow { children }, style, false)
        }

        HtmlTag::Th | HtmlTag::Td => {
            // 表格单元格作为段落处理
            let children = build_inline_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Paragraph { children }, style, true)
        }

        HtmlTag::A => {
            let url = elem.attrs.get("href").cloned().unwrap_or_default();
            let title = elem.attrs.get("title").cloned();
            style.link_url = (!url.is_empty()).then(|| url.clone());
            let children = build_inline_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(
                NodeKind::Link {
                    url,
                    title,
                    children,
                },
                style,
                true,
            )
        }

        HtmlTag::Strong | HtmlTag::B => {
            let children = build_inline_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Strong { children }, style, true)
        }

        HtmlTag::Em | HtmlTag::I => {
            let children = build_inline_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Emphasis { children }, style, true)
        }

        HtmlTag::Del | HtmlTag::S => {
            let children = build_inline_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Delete { children }, style, true)
        }

        HtmlTag::Code => {
            // 如果其父元素是 <pre>，则作为代码块内容处理（已在 Pre 分支中处理）
            // 否则作为行内代码
            let text = collect_text_content(&elem.children);
            Node::new(NodeKind::InlineCode { code: text }, style, true)
        }

        HtmlTag::Span => {
            let children = build_inline_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Span { children }, style, true)
        }

        HtmlTag::Center => {
            let children = build_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Center { children }, style, true)
        }

        HtmlTag::Div | HtmlTag::Section | HtmlTag::Article => {
            let children = build_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Container { children }, style, true)
        }

        HtmlTag::Sub => {
            let orig_font_size = style.font_size_pt;
            style.font_size_pt *= 0.65;
            style.baseline_shift = -(orig_font_size * 0.2);
            let children = build_inline_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Subscript { children }, style, true)
        }

        HtmlTag::Sup => {
            let orig_font_size = style.font_size_pt;
            style.font_size_pt *= 0.65;
            style.baseline_shift = orig_font_size * 0.35;
            let children = build_inline_children(&elem.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Superscript { children }, style, true)
        }

        // 未知/忽略的标签：透传其子节点
        _ => {
            let children = build_children(&elem.children, resolver, &new_ancestors, &style);
            if children.is_empty() {
                Node::new(
                    NodeKind::Text {
                        text: String::new(),
                    },
                    style,
                    true,
                )
            } else if children.len() == 1 {
                // 单个子节点直接替换
                children.into_iter().next().unwrap()
            } else {
                // 多个子节点作为段落
                Node::new(NodeKind::Paragraph { children }, style, true)
            }
        }
    }
}

/// 构建块级子节点列表
fn build_children(
    nodes: &[HtmlNode],
    resolver: &StyleResolver,
    ancestor_tags: &[String],
    parent_style: &Style,
) -> Vec<Node> {
    nodes
        .iter()
        .filter_map(|node| match node {
            HtmlNode::Element(elem) => {
                Some(build_element(elem, resolver, ancestor_tags, parent_style))
            }
            HtmlNode::Text(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    let style = resolver.resolve_style("span", &[], ancestor_tags, parent_style);
                    Some(Node::new(
                        NodeKind::Text {
                            text: trimmed.to_string(),
                        },
                        style,
                        true,
                    ))
                }
            }
        })
        .collect()
}

/// 构建内联子节点列表（保留文本顺序，不丢弃空白）
fn build_inline_children(
    nodes: &[HtmlNode],
    resolver: &StyleResolver,
    ancestor_tags: &[String],
    parent_style: &Style,
) -> Vec<Node> {
    nodes
        .iter()
        .filter_map(|node| match node {
            HtmlNode::Element(elem) => {
                Some(build_element(elem, resolver, ancestor_tags, parent_style))
            }
            HtmlNode::Text(text) => {
                if text.is_empty() {
                    None
                } else {
                    let style = resolver.resolve_style("span", &[], ancestor_tags, parent_style);
                    Some(Node::new(
                        NodeKind::Text { text: text.clone() },
                        style,
                        true,
                    ))
                }
            }
        })
        .collect()
}

// ─── 辅助函数 ───

/// 根据 HtmlTag 获取标题级别
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

/// 从 <pre> 的子节点中提取代码内容和语言
fn extract_code_content(children: &[HtmlNode]) -> (String, Option<String>) {
    for child in children {
        if let HtmlNode::Element(elem) = child
            && elem.tag == HtmlTag::Code
        {
            let code = collect_text_content(&elem.children);
            let lang = elem
                .attrs
                .get("class")
                .and_then(|c| c.strip_prefix("language-"))
                .map(|s| s.to_string());
            return (code, lang);
        }
    }
    // 没有 <code>，直接从 <pre> 的文本中取
    (collect_text_content(children), None)
}

/// 收集 HtmlNode 列表中的纯文本
fn collect_text_content(nodes: &[HtmlNode]) -> String {
    let mut result = String::new();
    for node in nodes {
        match node {
            HtmlNode::Text(text) => result.push_str(text),
            HtmlNode::Element(elem) => {
                // 递归收集子元素文本
                result.push_str(&elem.text_content());
            }
        }
    }
    result
}

/// 从表格的 <th>/<td> 中提取对齐信息
fn extract_table_align(children: &[HtmlNode]) -> Vec<TextAlign> {
    let mut align = Vec::new();
    // 从第一行（表头）的每个单元格的 style 属性中提取 text-align
    if let Some(first_row) = children.first()
        && let HtmlNode::Element(row_elem) = first_row
        && row_elem.tag == HtmlTag::Tr
    {
        for cell in &row_elem.children {
            if let HtmlNode::Element(cell_elem) = cell {
                let a = cell_elem
                    .inline_style()
                    .and_then(|s| {
                        if s.contains("text-align: left") {
                            Some(TextAlign::Left)
                        } else if s.contains("text-align: center") {
                            Some(TextAlign::Center)
                        } else if s.contains("text-align: right") {
                            Some(TextAlign::Right)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(TextAlign::Left);
                align.push(a);
            }
        }
    }
    align
}

/// 遍历 Node 树，对每个节点执行回调函数
pub fn walk<F>(node: &Node, callback: &mut F)
where
    F: FnMut(&Node),
{
    callback(node);
    match &node.kind {
        NodeKind::Document { children }
        | NodeKind::Heading { children, .. }
        | NodeKind::Paragraph { children }
        | NodeKind::List { children, .. }
        | NodeKind::ListItem { children }
        | NodeKind::TaskListItem { children, .. }
        | NodeKind::Blockquote { children }
        | NodeKind::Table { children, .. }
        | NodeKind::TableRow { children }
        | NodeKind::Strong { children }
        | NodeKind::Emphasis { children }
        | NodeKind::Link { children, .. }
        | NodeKind::Delete { children }
        | NodeKind::Span { children }
        | NodeKind::Center { children }
        | NodeKind::Container { children }
        | NodeKind::Subscript { children }
        | NodeKind::Superscript { children } => {
            for child in children {
                walk(child, callback);
            }
        }
        _ => {}
    }
}

/// 收集 Node 树中所有文本节点的文本内容
pub fn collect_text(node: &Node) -> String {
    let mut result = String::new();
    walk(node, &mut |n| match &n.kind {
        NodeKind::Text { text } => result.push_str(text),
        NodeKind::LineBreak => result.push('\n'),
        _ => {}
    });
    result
}
