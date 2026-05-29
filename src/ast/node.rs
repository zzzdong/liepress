//! 简化 AST 模块
//!
//! 将 MDAST 节点转换为简化的、带样式的 Node。
//! 设计目标：
//! - 简化节点类型（heading、paragraph 等都可表示为带样式的文本块）
//! - 每个节点携带 ComputedStyle，包含布局所需的所有样式信息
//! - 添加布局相关属性（如是否可分割）

use markdown::mdast;

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
/// 3. 每个节点通过 ComputedStyle 区分视觉表现
#[derive(Debug, Clone)]
pub enum NodeKind {
    /// 文档根节点
    Document { children: Vec<Node> },

    /// 标题（h1-h6）
    /// 通过 style 中的 font_size, font_weight 等属性区分级别
    Heading { level: u8, children: Vec<Node> },

    /// 段落
    /// 最基本的文本块，可分割
    Paragraph { children: Vec<Node> },

    /// 列表
    List {
        ordered: bool,
        start: Option<u32>, // 有序列表的起始编号
        children: Vec<Node>,
    },

    /// 列表项
    ListItem { children: Vec<Node> },

    /// 图片
    /// 不可分割，必须完整显示
    Image {
        src: String,
        alt: String,
        title: Option<String>,
    },

    /// 代码块
    /// 通常不可分割，保持完整
    CodeBlock { code: String, lang: Option<String> },

    /// 引用块
    Blockquote { children: Vec<Node> },

    /// 分隔线
    ThematicBreak,

    /// 表格
    Table {
        children: Vec<Node>,   // TableRow 节点列表
        align: Vec<TextAlign>, // 每列对齐方式
    },

    /// 表格行
    TableRow { children: Vec<Node> }, // TableCell/Paragraph 节点列表

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
            NodeKind::Strong { children } => {
                let mut s = String::new();
                for child in children {
                    s.push_str(&child.text_content());
                }
                s
            }
            NodeKind::Emphasis { children } => {
                let mut s = String::new();
                for child in children {
                    s.push_str(&child.text_content());
                }
                s
            }
            NodeKind::InlineCode { code } => code.clone(),
            NodeKind::Link { children, .. } => {
                let mut s = String::new();
                for child in children {
                    s.push_str(&child.text_content());
                }
                s
            }
            NodeKind::Delete { children } => {
                let mut s = String::new();
                for child in children {
                    s.push_str(&child.text_content());
                }
                s
            }
            NodeKind::Heading { children, .. } => {
                let mut s = String::new();
                for child in children {
                    s.push_str(&child.text_content());
                }
                s
            }
            NodeKind::Paragraph { children } => {
                let mut s = String::new();
                for child in children {
                    s.push_str(&child.text_content());
                }
                s
            }
            NodeKind::ListItem { children } => {
                let mut s = String::new();
                for child in children {
                    s.push_str(&child.text_content());
                }
                s
            }
            NodeKind::Blockquote { children } => {
                let mut s = String::new();
                for child in children {
                    s.push_str(&child.text_content());
                }
                s
            }
            NodeKind::TableRow { children } => {
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

// ─── MDAST → Node 转换 ───

/// 将 MDAST 根节点转换为简化 Node 树
///
/// 每个节点的样式由默认样式系统确定（后续可扩展为 CSS 匹配）
pub fn build_ast(root: &mdast::Node) -> Node {
    let root_style = paragraph_style(); // Root 使用正文样式作为默认
    build_node(root, &root_style)
}

fn build_node(node: &mdast::Node, parent_style: &Style) -> Node {
    match node {
        mdast::Node::Root(root) => {
            let style = paragraph_style();
            let children: Vec<Node> = root
                .children
                .iter()
                .map(|child| build_node(child, &style))
                .collect();
            Node::new(
                NodeKind::Document { children },
                style,
                false, // 根节点不可分割
            )
        }

        mdast::Node::Paragraph(_para) => {
            let style = paragraph_style();
            let children = build_inline_children(&_para.children, &style);
            Node::new(
                NodeKind::Paragraph { children },
                style,
                true, // 段落可分割
            )
        }

        mdast::Node::Heading(heading) => {
            let style = heading_style(heading.depth);
            let children = build_inline_children(&heading.children, &style);
            Node::new(
                NodeKind::Heading {
                    level: heading.depth,
                    children,
                },
                style,
                false, // 标题不可分割，保持完整
            )
        }

        mdast::Node::Code(code) => {
            let style = code_style();
            Node::new(
                NodeKind::CodeBlock {
                    code: code.value.clone(),
                    lang: code.lang.clone(),
                },
                style,
                false, // 代码块不可分割，保持完整
            )
        }

        mdast::Node::Image(image) => {
            let style = image_style();
            Node::new(
                NodeKind::Image {
                    src: image.url.clone(),
                    alt: image.alt.clone(),
                    title: image.title.clone(),
                },
                style,
                false, // 图片不可分割
            )
        }

        mdast::Node::List(list) => {
            let style = list_item_style();
            let children: Vec<Node> = list
                .children
                .iter()
                .map(|child| build_node(child, &style))
                .collect();
            Node::new(
                NodeKind::List {
                    ordered: list.ordered,
                    start: list.start,
                    children,
                },
                style,
                true, // 列表可分割（单个 item 可能跨页）
            )
        }

        mdast::Node::ListItem(item) => {
            let style = list_item_style();
            let children: Vec<Node> = item
                .children
                .iter()
                .map(|child| build_node(child, &style))
                .collect();
            Node::new(
                NodeKind::ListItem { children },
                style,
                true, // 列表项可分割
            )
        }

        mdast::Node::Blockquote(blockquote) => {
            let style = blockquote_style();
            let children: Vec<Node> = blockquote
                .children
                .iter()
                .map(|child| build_node(child, &style))
                .collect();
            Node::new(
                NodeKind::Blockquote { children },
                style,
                true, // 引用块可分割
            )
        }

        mdast::Node::ThematicBreak(_) => {
            let style = thematic_break_style();
            Node::new(
                NodeKind::ThematicBreak,
                style,
                false, // 分隔线不可分割
            )
        }

        mdast::Node::Table(table) => {
            let style = table_style();
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
                .map(|child| build_node(child, &style))
                .collect();
            Node::new(
                NodeKind::Table { children, align },
                style,
                false, // 表格不可分割
            )
        }

        mdast::Node::TableRow(row) => {
            let style = table_style();
            let children: Vec<Node> = row
                .children
                .iter()
                .map(|child| build_node(child, &style))
                .collect();
            Node::new(
                NodeKind::TableRow { children },
                style,
                false, // 表格行不可分割
            )
        }

        mdast::Node::TableCell(cell) => {
            let style = table_style();
            let children: Vec<Node> = cell
                .children
                .iter()
                .map(|child| build_node(child, &style))
                .collect();
            Node::new(
                NodeKind::Paragraph { children },
                style,
                true,
            )
        }

        // ── 内联节点 ──
        mdast::Node::Text(text) => {
            let style = Style::inherit_from(parent_style);
            Node::new(
                NodeKind::Text {
                    text: text.value.clone(),
                },
                style,
                true, // 内联文本可分割
            )
        }

        mdast::Node::Strong(strong) => {
            let mut style = Style::inherit_from(parent_style);
            style.font_weight = FontWeight::Bold;
            let children: Vec<Node> = strong
                .children
                .iter()
                .map(|child| build_node(child, &style))
                .collect();
            Node::new(
                NodeKind::Strong { children },
                style,
                true, // 内联元素可分割
            )
        }

        mdast::Node::Emphasis(emph) => {
            let mut style = Style::inherit_from(parent_style);
            style.font_style = FontStyle::Italic;
            let children: Vec<Node> = emph
                .children
                .iter()
                .map(|child| build_node(child, &style))
                .collect();
            Node::new(
                NodeKind::Emphasis { children },
                style,
                true, // 内联元素可分割
            )
        }

        mdast::Node::InlineCode(code) => {
            let mut style = Style::inherit_from(parent_style);
            let code_default = inline_code_style();
            style.font_family = code_default.font_family;
            style.color = code_default.color;
            Node::new(
                NodeKind::InlineCode {
                    code: code.value.clone(),
                },
                style,
                true, // 行内代码可分割
            )
        }

        mdast::Node::Link(link) => {
            let mut style = Style::inherit_from(parent_style);
            style.color = crate::visual::Color::new(0, 0, 255);
            let children: Vec<Node> = link
                .children
                .iter()
                .map(|child| build_node(child, &style))
                .collect();
            Node::new(
                NodeKind::Link {
                    url: link.url.clone(),
                    title: link.title.clone(),
                    children,
                },
                style,
                true, // 链接可分割
            )
        }

        mdast::Node::Delete(del) => {
            let style = Style::inherit_from(parent_style);
            let children: Vec<Node> = del
                .children
                .iter()
                .map(|child| build_node(child, &style))
                .collect();
            Node::new(
                NodeKind::Delete { children },
                style,
                true, // 删除线可分割
            )
        }

        // MDAST 中的其他节点类型暂不处理（如脚注、定义等）
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
fn build_inline_children(children: &[mdast::Node], parent_style: &Style) -> Vec<Node> {
    children
        .iter()
        .map(|child| build_node(child, parent_style))
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
