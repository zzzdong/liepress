//! 页面常量定义
//!
//! 定义 A4 页面尺寸、边距、内容区域等常量。

// ─── 页面尺寸（A4）───────────────────────────────────────

/// A4 页面宽度（pt）
pub const PAGE_WIDTH_PT: f32 = 595.276;

/// A4 页面高度（pt）
pub const PAGE_HEIGHT_PT: f32 = 841.890;

/// 默认边距（pt） - 紧凑方案
/// 上下 36pt (0.5"), 左右 54pt (0.75")
pub const DEFAULT_MARGIN_PT: f32 = 36.0;

/// 默认 DPI
pub const DEFAULT_DPI: u32 = 72;

// ─── 页面边距 ────────────────────────────────────────────

/// 上边距（pt）- 0.5 英寸
pub const PAGE_MARGIN_TOP_PT: f32 = 36.0;

/// 下边距（pt）- 0.5 英寸
pub const PAGE_MARGIN_BOTTOM_PT: f32 = 36.0;

/// 左边距（pt）- 0.75 英寸
pub const PAGE_MARGIN_LEFT_PT: f32 = 54.0;

/// 右边距（pt）- 0.75 英寸
pub const PAGE_MARGIN_RIGHT_PT: f32 = 54.0;

// ─── 内容区域 ────────────────────────────────────────────

/// 内容区左上角 X 坐标
pub const CONTENT_AREA_X_PT: f32 = PAGE_MARGIN_LEFT_PT;

/// 内容区左上角 Y 坐标
pub const CONTENT_AREA_Y_PT: f32 = PAGE_MARGIN_TOP_PT;

/// 内容区宽度
pub const CONTENT_AREA_WIDTH_PT: f32 =
    PAGE_WIDTH_PT - PAGE_MARGIN_LEFT_PT - PAGE_MARGIN_RIGHT_PT;

/// 内容区高度
pub const CONTENT_AREA_HEIGHT_PT: f32 =
    PAGE_HEIGHT_PT - PAGE_MARGIN_TOP_PT - PAGE_MARGIN_BOTTOM_PT;
