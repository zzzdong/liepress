//! HTML 处理模块
//!
//! 提供 HTML 子集的解析、AST 定义、序列化和 DOM 操作。
//! 使用 html5ever 进行浏览器级 HTML 解析，
//! 支持 Markdown 中嵌入的 HTML 标签。
//!
//! 本模块的目标是成为一个简易的 HTML 引擎核心，提供：
//! - **解析**：完整文档和片段的 HTML 解析
//! - **AST**：DOM 风格的树结构，支持遍历、查询和操作
//! - **序列化**：将 AST 输出为格式化的 HTML 字符串
//! - **样式转换**：将 HTML AST + CSS 转换为带样式的 Node 树供布局引擎消费
//!
//! 管线：Markdown → HTML → HtmlDocument → Styled Node Tree → Layout

pub mod ast;
pub mod md_converter;
pub mod parser;
pub mod style_resolver;
pub mod styled;

pub use ast::*;
pub use md_converter::{embed_local_images, markdown_to_html, markdown_to_html_document};
pub use parser::{parse_html, parse_html_document, parse_html_fragment};
pub use styled::html_to_styled_nodes;
