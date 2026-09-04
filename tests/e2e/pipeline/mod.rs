//! 管线集成测试的共享工具。
//!
//! 将管线各步骤的输出转换为便于断言的字符串，供 `dom`/`styled`/`layout`
//! 等子模块复用，避免在多个文件里重复构造 `CssEngine` 与 `PageSettings`。

#![allow(dead_code)]

pub mod charts;
pub mod docx;
pub mod dom;
pub mod html_input;
pub mod html_output;
pub mod layout;
pub mod pdf;
pub mod png;
pub mod styled;
pub mod svg;

use liepress::ast::Node;
use liepress::ast::NodeKind;
use liepress::document::from_ast::ast_to_layout;
use liepress::document::layout::Document;
use liepress::document::types::page::PageSettings;

/// 解析 Markdown 为带样式的 AST（管线 Layer 2 真源）。
pub fn markdown_to_ast(markdown: &str) -> Node {
    liepress::ast::parse_markdown(markdown).expect("AST 解析应成功")
}

/// 收集整棵 AST 的纯文本内容。
pub fn ast_text(root: &Node) -> String {
    root.text_content()
}

/// 深度优先遍历，返回所有满足谓词的节点。
pub fn find_nodes<'a>(node: &'a Node, predicate: impl Fn(&'a Node) -> bool) -> Vec<&'a Node> {
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if predicate(n) {
            out.push(n);
        }
        stack.extend(node_children(n));
    }
    out
}

/// 查找第一个满足谓词的节点。
pub fn find_node<'a>(node: &'a Node, predicate: impl Fn(&'a Node) -> bool) -> Option<&'a Node> {
    find_nodes(node, predicate).into_iter().next()
}

/// 返回节点的子节点（根据 `NodeKind` 变体中的 `children` 字段）。
fn node_children(node: &Node) -> Vec<&Node> {
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
        | NodeKind::Superscript { children } => children.iter().collect(),
        NodeKind::DefinitionList { items } => items
            .iter()
            .flat_map(|it| it.term.iter().chain(it.definition.iter()))
            .collect(),
        NodeKind::FootnoteDef { children, .. } => children.iter().collect(),
        _ => Vec::new(),
    }
}

/// 运行完整管线（Markdown -> AST -> Layout），返回布局文档。
pub fn markdown_to_layout(markdown: &str) -> Document {
    let ast = markdown_to_ast(markdown);
    let settings = PageSettings::a4();
    ast_to_layout(&ast, &settings)
}

/// 收集布局文档中所有块（含嵌套）的纯文本内容。
pub fn layout_text(doc: &Document) -> String {
    doc.blocks
        .iter()
        .map(|b| b.text_content())
        .collect::<Vec<_>>()
        .join(" ")
}
