//! 文档逻辑类型。
//!
//! 这些类型是对既有渲染/样式类型的**逻辑投影**，目的是让文档层拥有
//! 与渲染后端（parley/vello）无关的稳定数据结构。运行期通过
//! `From<源类型>` 实现从 [`crate::ast`]、[`crate::text`] 的转换
//! （见 `from_ast`）。
//!
//! 旧像素层 `visual` 已删除；`style`/`image`/`color` 等在此重新投影，
//! 供 `Document` 与 PDF 输出后端共用。

mod image;
pub mod page;
mod style;

pub use crate::ast::{ObjectFit, TextAlign, TextDecoration, WhiteSpace};
pub use image::DocImage;
pub use page::PageSettings;
pub use style::ResolvedStyle;

// ─── 页面尺寸常量（A4，pt）────────────────────────────────
// 原位于 generator::constants，迁移到此供 document 层（排版宽度）与
// 各输出后端（分页）共享。

/// A4 页面宽度（pt）
pub const PAGE_WIDTH_PT: f32 = 595.276;
/// A4 页面高度（pt）
pub const PAGE_HEIGHT_PT: f32 = 841.890;
/// 上边距（pt）- 0.5 英寸
pub const PAGE_MARGIN_TOP_PT: f32 = 36.0;
/// 下边距（pt）- 0.5 英寸
pub const PAGE_MARGIN_BOTTOM_PT: f32 = 36.0;
/// 左边距（pt）- 0.75 英寸
pub const PAGE_MARGIN_LEFT_PT: f32 = 54.0;
/// 右边距（pt）- 0.75 英寸
pub const PAGE_MARGIN_RIGHT_PT: f32 = 54.0;
