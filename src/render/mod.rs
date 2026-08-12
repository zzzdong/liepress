//! 渲染模块 - 提供 PDF 输出后端。
//!
//! 各后端（PDF/DOCX）直接消费 [`crate::document::skeleton::DocumentSkeleton`]
//! （方案 §5.1），内部自行完成分页。本模块当前实现 PDF 后端。

pub use pdf::PdfDocumentGenerator;

pub mod pdf;
