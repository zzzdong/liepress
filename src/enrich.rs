//! AST 富化阶段（Layer 2 → Layer 2）。
//!
//! 在 **AST 建好之后、交给任何输出后端之前** 统一执行的两个 pass：
//!
//! 1. [`crate::ext_render::pass::render_ext_blocks`]：把 ` ```mermaid ` /
//!    ` ```liecharts ` 等绘图代码块渲染为 PNG，就地替换为图片节点（base64 data URI）。
//! 2. [`crate::highlight::highlight_code_blocks`]：为剩余代码块填充语法高亮
//!    （`NodeKind::CodeBlock::spans`）。
//!
//! 顺序有意义：**先外绘、后高亮** —— 外绘失败软降级时会把错误注释写回代码块，
//! 随后的高亮 pass 才能对「带注释的代码」正确着色。
//!
//! 设计原则：
//! - 富化产物属于**文档内容**（图片字节 / token 颜色），不是渲染细节，
//!   因此必须前置到 AST 层，否则只有经过 `document` 层的后端（PDF/SVG/PNG）能拿到，
//!   而直出 `ast::Node` 的后端（DOCX/HTML）会丢失图表与高亮。
//! - 本模块不隶属于 `ast` 或 `document`，由 [`crate::lib`] 的管线入口显式调用，
//!   以维持「不在 ast/dom 层直接绘制」的分层红线（docs/design.md §12.4）。

use crate::ast::Node;
use crate::document::types::PageSettings;

/// 对 AST 执行一次完整的富化（外绘 + 高亮）。幂等，可安全重复调用。
pub fn enrich_ast(node: &mut Node, settings: &PageSettings) {
    crate::ext_render::pass::render_ext_blocks(node, settings);
    crate::highlight::highlight_code_blocks(node);
}
