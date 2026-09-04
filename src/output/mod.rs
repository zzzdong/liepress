//! 输出层：将中间表示渲染为目标格式。
//!
//! 管线位置（输出侧）：`ast::Node` / `HtmlDocument` → 目标字节。
//! - [`html`]：语义树 `ast::Node` 序列化为（自包含、内联样式）HTML（流式）。
//! - [`pdf`]：PDF 输出后端，直接消费 [`crate::document::layout::Document`]
//!   （方案 §5.1：后端直接消费 `Document`，内部自行分页）。
//! - [`docx`]：DOCX 输出后端，消费 `ast::Node`（保留语义，可编辑）。
//!
//! 注意：Markdown 源 → HTML 的转换（含降级入口 `markdown_to_html*`
//! / `embed_local_images`）属于**输入侧**，位于 [`crate::dom::md_converter`]，
//! 不属于本输出层。

pub use docx::DocxGenerator;
pub use html::HtmlGenerator;
pub use pdf::PdfGenerator;
pub use png::PngGenerator;
pub use svg::SvgGenerator;

pub mod common;
pub mod docx;
pub mod html;
pub mod pdf;
pub mod png;
pub mod svg;
