//! 简化 AST 模块
//!
//! 将 HtmlElement 树转换为简化的、带样式的 Node 树。
//! 设计目标：
//! - 简化节点类型（heading、paragraph 等都可表示为带样式的文本块）
//! - 每个节点携带 Style，包含布局所需的所有样式信息
//! - 样式来源：CSS 样式表解析（内置 + 用户覆盖），由 html/styled.rs 统一处理

use super::style::{self, *};
// 显式绑定：Table.align 字段使用 crate::ast::style::TextAlign
#[allow(unused_imports)]
use style::TextAlign as _TableAlignUse;

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

    /// 定义列表（<dl>）：有序的 (术语, 定义) 项序列
    DefinitionList { items: Vec<DefinitionItem> },

    /// 脚注定义（<div class="footnote-def">，末尾聚合）：携带 label 供内部跳转
    FootnoteDef { id: String, children: Vec<Node> },

    /// 图片
    Image {
        src: String,
        alt: String,
        title: Option<String>,
    },

    /// 代码块
    ///
    /// `spans` 为语法高亮结果：外层按行、行内为若干着色片段，由
    /// [`crate::highlight::highlight_code_blocks`]（AST 富化 pass）填充。
    /// `None` 表示尚未经过高亮 pass，各后端退化为单色等宽文本。
    CodeBlock {
        code: String,
        lang: Option<String>,
        spans: Option<Vec<Vec<CodeSpan>>>,
    },

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

/// 代码块内一段同色文本（语法高亮产物，见 [`crate::highlight`]）。
///
/// 与布局层的 `TextRun` 不同，这里是**纯语义**的着色片段：只携带文本、前景色与
/// 粗/斜体标记，不含任何坐标或字体信息。各后端据此自行渲染：
/// - PDF/SVG/PNG：`document::from_ast` 把 spans 排版成 `TextLine`；
/// - DOCX：每段一个 `Run`（`Run::color` + `bold`/`italic`）；
/// - HTML：每段一个 `<span style="color:...">`。
#[derive(Debug, Clone)]
pub struct CodeSpan {
    /// 片段文本（不含换行）。
    pub text: String,
    /// 前景色。
    pub color: lievisual::Color,
    pub bold: bool,
    pub italic: bool,
}

/// 定义列表项（<dt> 术语 + <dd> 定义）
#[derive(Debug, Clone)]
pub struct DefinitionItem {
    /// 术语（<dt>）内容
    pub term: Vec<Node>,
    /// 定义（<dd>）内容
    pub definition: Vec<Node>,
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
            NodeKind::DefinitionList { items } => {
                let mut s = String::new();
                for item in items {
                    for child in &item.term {
                        s.push_str(&child.text_content());
                    }
                    for child in &item.definition {
                        s.push_str(&child.text_content());
                    }
                }
                s
            }
            NodeKind::FootnoteDef { children, .. } => {
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

// ─── 遍历与工具函数 ───────────────────────────────────────────

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
        NodeKind::DefinitionList { items } => {
            for item in items {
                for child in &item.term {
                    walk(child, callback);
                }
                for child in &item.definition {
                    walk(child, callback);
                }
            }
        }
        NodeKind::FootnoteDef { children, .. } => {
            for child in children {
                walk(child, callback);
            }
        }
        _ => {}
    }
}

/// 可变遍历 Node 树，对每个节点执行回调函数（可就地改写节点，如 AST 富化 pass）。
///
/// 与 [`walk`] 同序（先自身、后子节点）。回调替换了父节点后，递归会作用于
/// **替换后的**节点，因此「代码块 → 图片」这类改写是安全的。
pub fn walk_mut<F>(node: &mut Node, callback: &mut F)
where
    F: FnMut(&mut Node),
{
    callback(node);
    match &mut node.kind {
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
            for child in children.iter_mut() {
                walk_mut(child, callback);
            }
        }
        NodeKind::DefinitionList { items } => {
            for item in items.iter_mut() {
                for child in item.term.iter_mut() {
                    walk_mut(child, callback);
                }
                for child in item.definition.iter_mut() {
                    walk_mut(child, callback);
                }
            }
        }
        NodeKind::FootnoteDef { children, .. } => {
            for child in children.iter_mut() {
                walk_mut(child, callback);
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
