//! 样式系统模块
//!
//! 三层 AST 架构的 Layer 2：Styled AST
//! - MDAST (Layer 1) → Node (Layer 2) → Layout AST (Layer 3)
//! - 每个 Node 附带 Style，布局引擎不再关心样式来源

pub mod node;
pub mod presets;
pub mod style;

// 重新导出主要类型
pub use node::*;
pub use presets::*;
pub use style::*;

use markdown::mdast;

/// 将 Markdown 文本转换为带样式的 AST
///
/// 这是 ast 模块的主要入口函数，完成 Layer 1 → Layer 2 的转换。
///
/// # 示例
/// ```
/// use liepress::ast::parse_markdown;
///
/// let markdown = "# Hello\n\nThis is a paragraph.";
/// let ast = parse_markdown(markdown);
/// ```
pub fn parse_markdown(markdown: &str) -> Node {
    let mdast =
        markdown::to_mdast(markdown, &markdown::ParseOptions::default()).unwrap_or_else(|_| {
            mdast::Node::Root(mdast::Root {
                children: vec![],
                position: None,
            })
        });
    node::build_ast(&mdast)
}
