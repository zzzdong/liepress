//! 简化 AST 模块
//!
//! 将 MDAST 节点转换为简化的、带样式的 Node。
//! 设计目标：
//! - 简化节点类型（heading、paragraph 等都可表示为带样式的文本块）
//! - 每个节点携带 Style，包含布局所需的所有样式信息
//! - 样式来源：CSS 样式表解析（内置 + 用户覆盖）

use markdown::mdast;

use super::css::*;
use super::presets::*;
use super::style::*;

// ─── 简化 AST 节点定义 ───

/// 带样式的简化 AST 节点
///
/// 这是三层 AST 架构的 Layer 2。
/// 布局引擎只消费此结构，不关心 MDAST 或 CSS 细节。
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
/// 相比 MDAST，此枚举做了以下简化：
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
}

impl NodeKind {
    /// 获取文本内容的拼接值
    pub fn text_content(&self) -> String {
        match self {
            NodeKind::Text { text } => text.clone(),
            NodeKind::Strong { children }
            | NodeKind::Emphasis { children }
            | NodeKind::Link { children, .. }
            | NodeKind::Delete { children } => {
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
            | NodeKind::Blockquote { children }
            | NodeKind::TableRow { children } => {
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

// ─── MDAST → Node 转换 ──────────────────────────────────────

/// 将 MDAST 根节点转换为简化 Node 树
///
/// 使用给定的样式解析器为每个节点解析样式。
pub fn build_ast(root: &mdast::Node, resolver: &StyleResolver) -> Node {
    build_node(root, resolver, &[], &Style::default())
}

fn build_node(
    node: &mdast::Node,
    resolver: &StyleResolver,
    ancestor_tags: &[String],
    parent_style: &Style,
) -> Node {
    match node {
        mdast::Node::Root(root) => {
            let tag = "body";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = root
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(
                NodeKind::Document { children },
                style,
                false,
            )
        }

        mdast::Node::Paragraph(_para) => {
            let tag = "p";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children = build_inline_children(&_para.children, resolver, &new_ancestors, &style);
            Node::new(
                NodeKind::Paragraph { children },
                style,
                true,
            )
        }

        mdast::Node::Heading(heading) => {
            let tag = match heading.depth {
                1 => "h1", 2 => "h2", 3 => "h3",
                4 => "h4", 5 => "h5", 6 => "h6",
                _ => "h1",
            };
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children = build_inline_children(&heading.children, resolver, &new_ancestors, &style);
            Node::new(
                NodeKind::Heading {
                    level: heading.depth,
                    children,
                },
                style,
                false,
            )
        }

        mdast::Node::Code(code) => {
            let tag = "pre";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            Node::new(
                NodeKind::CodeBlock {
                    code: code.value.clone(),
                    lang: code.lang.clone(),
                },
                style,
                false,
            )
        }

        mdast::Node::Image(image) => {
            let tag = "img";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            Node::new(
                NodeKind::Image {
                    src: image.url.clone(),
                    alt: image.alt.clone(),
                    title: image.title.clone(),
                },
                style,
                false,
            )
        }

        mdast::Node::List(list) => {
            let tag = if list.ordered { "ol" } else { "ul" };
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = list
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(
                NodeKind::List {
                    ordered: list.ordered,
                    start: list.start,
                    children,
                },
                style,
                true,
            )
        }

        mdast::Node::ListItem(item) => {
            let tag = "li";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = item
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(
                NodeKind::ListItem { children },
                style,
                true,
            )
        }

        mdast::Node::Blockquote(blockquote) => {
            let tag = "blockquote";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = blockquote
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(
                NodeKind::Blockquote { children },
                style,
                true,
            )
        }

        mdast::Node::ThematicBreak(_) => {
            let tag = "hr";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            Node::new(
                NodeKind::ThematicBreak,
                style,
                false,
            )
        }

        mdast::Node::Table(table) => {
            let tag = "table";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let align: Vec<TextAlign> = table
                .align
                .iter()
                .map(|a| match a {
                    mdast::AlignKind::Left => TextAlign::Left,
                    mdast::AlignKind::Right => TextAlign::Right,
                    mdast::AlignKind::Center => TextAlign::Center,
                    mdast::AlignKind::None => TextAlign::Left,
                })
                .collect();
            let children: Vec<Node> = table
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(
                NodeKind::Table { children, align },
                style,
                false,
            )
        }

        mdast::Node::TableRow(row) => {
            let tag = "tr";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = row
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(
                NodeKind::TableRow { children },
                style,
                false,
            )
        }

        mdast::Node::TableCell(cell) => {
            // 表格单元格使用 td 标签，但样式与段落类似
            let tag = "td";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children = build_inline_children(&cell.children, resolver, &new_ancestors, &style);
            Node::new(
                NodeKind::Paragraph { children },
                style,
                true,
            )
        }

        // ── 内联节点 ──
        mdast::Node::Text(text) => {
            let tag = "span";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            Node::new(
                NodeKind::Text {
                    text: text.value.clone(),
                },
                style,
                true,
            )
        }

        mdast::Node::Strong(strong) => {
            let tag = "strong";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = strong
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(
                NodeKind::Strong { children },
                style,
                true,
            )
        }

        mdast::Node::Emphasis(emph) => {
            let tag = "em";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = emph
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(
                NodeKind::Emphasis { children },
                style,
                true,
            )
        }

        mdast::Node::InlineCode(code) => {
            let tag = "code";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            Node::new(
                NodeKind::InlineCode {
                    code: code.value.clone(),
                },
                style,
                true,
            )
        }

        mdast::Node::Link(link) => {
            let tag = "a";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            // 设置链接 URL（CSS 无法设置动态 URL，所以从 MDAST 中取）
            let mut style = style;
            style.link_url = Some(link.url.clone());
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = link
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(
                NodeKind::Link {
                    url: link.url.clone(),
                    title: link.title.clone(),
                    children,
                },
                style,
                true,
            )
        }

        mdast::Node::Delete(del) => {
            let tag = "del";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = del
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(
                NodeKind::Delete { children },
                style,
                true,
            )
        }

        // HTML 节点（用于提取 <style> CSS，不在输出中产生内容）
        mdast::Node::Html(_) => Node::new(
            NodeKind::Text {
                text: String::new(),
            },
            Style::default(),
            false,
        ),

        // MDAST 中的其他节点类型暂不处理
        _ => Node::new(
            NodeKind::Text {
                text: String::new(),
            },
            Style::default(),
            true,
        ),
    }
}

/// 构建内联子节点列表
fn build_inline_children(
    children: &[mdast::Node],
    resolver: &StyleResolver,
    ancestor_tags: &[String],
    parent_style: &Style,
) -> Vec<Node> {
    children
        .iter()
        .map(|child| build_node(child, resolver, ancestor_tags, parent_style))
        .collect()
}

// ─── 辅助函数 ───

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
        | NodeKind::Blockquote { children }
        | NodeKind::Table { children, .. }
        | NodeKind::TableRow { children }
        | NodeKind::Strong { children }
        | NodeKind::Emphasis { children }
        | NodeKind::Link { children, .. }
        | NodeKind::Delete { children } => {
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
    walk(node, &mut |n| {
        if let NodeKind::Text { text } = &n.kind {
            result.push_str(text);
        }
    });
    result
}