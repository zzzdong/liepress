//! 文档逻辑类型。
//!
//! 这些类型是对既有渲染/样式类型的**逻辑投影**，目的是让文档层拥有
//! 与渲染后端（parley/vello）无关的稳定数据结构。运行期通过
//! `From<源类型>` 实现从 [`crate::ast`]、[`crate::text`]、[`crate::visual`]
//! 的转换（见 S1+ 的 `from_ast`）。
//!
//! S0 阶段只定义类型与 `From` 转换。

mod color;
mod style;
mod text;
mod image;
pub mod page;

pub use color::DocColor;
pub use image::DocImage;
pub use page::PageSettings;
pub use style::{ResolvedStyle, TextDecoration, TextAlign, WhiteSpace, ObjectFit};
pub use text::{DocTextLine, DocTextRun, DocGlyph};

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
