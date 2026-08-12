//! 页面设置（分页后端输入）。
//!
//! `PageSettings` 不属于 `document` 源 IR（源 IR 不分页），而是各输出后端
//! （PDF/DOCX）做分页时的输入参数。放在此处是因为它语义上属于"文档层公共类型"，
//! 且被 `from_ast`（排版宽度计算）与 `render::pdf`（分页）共同使用。

use crate::ast::PageConfig;

/// 页面设置 - 可配置的页面尺寸和边距
#[derive(Debug, Clone)]
pub struct PageSettings {
    pub width_pt: f32,
    pub height_pt: f32,
    pub margin_top_pt: f32,
    pub margin_bottom_pt: f32,
    pub margin_left_pt: f32,
    pub margin_right_pt: f32,

    // ─── 无限高度模式 ──────────────────────────────────────
    /// 仅限定宽度，高度无限（不分页，所有内容连续排列在一个页面上）
    ///
    /// 启用后：
    /// - `content_height()` 返回 `f32::MAX`，所有分页检查永不触发
    /// - 最终输出单页文档，页面高度 = 实际内容高度
    pub height_unlimited: bool,

    // ─── 页眉页脚 ──────────────────────────────────────────
    /// 页眉文本（支持 {page} 和 {total} 模板变量）
    pub header: Option<String>,
    /// 页脚文本（支持 {page} 和 {total} 模板变量）
    pub footer: Option<String>,
    /// 页眉字体大小（pt），默认 9pt
    pub header_font_size: f32,
    /// 页脚字体大小（pt），默认 9pt
    pub footer_font_size: f32,
}

impl Default for PageSettings {
    fn default() -> Self {
        Self {
            width_pt: crate::document::types::PAGE_WIDTH_PT,
            height_pt: crate::document::types::PAGE_HEIGHT_PT,
            margin_top_pt: crate::document::types::PAGE_MARGIN_TOP_PT,
            margin_bottom_pt: crate::document::types::PAGE_MARGIN_BOTTOM_PT,
            margin_left_pt: crate::document::types::PAGE_MARGIN_LEFT_PT,
            margin_right_pt: crate::document::types::PAGE_MARGIN_RIGHT_PT,
            height_unlimited: false,
            header: None,
            footer: Some("- {page} -".to_string()),
            header_font_size: 9.0,
            footer_font_size: 9.0,
        }
    }
}

impl PageSettings {
    /// A4 页面（默认）
    pub fn a4() -> Self {
        Self::default()
    }

    /// 自定义页面尺寸和边距
    pub fn new(width_pt: f32, height_pt: f32) -> Self {
        Self {
            width_pt,
            height_pt,
            ..Default::default()
        }
    }

    /// 设置边距
    pub fn with_margins(mut self, top: f32, bottom: f32, left: f32, right: f32) -> Self {
        self.margin_top_pt = top;
        self.margin_bottom_pt = bottom;
        self.margin_left_pt = left;
        self.margin_right_pt = right;
        self
    }

    /// 启用无限高度模式（仅限定宽度，高度自适应内容）
    pub fn with_height_unlimited(mut self, unlimited: bool) -> Self {
        self.height_unlimited = unlimited;
        self
    }

    /// 内容区左上角 X 坐标
    pub fn content_x(&self) -> f32 {
        self.margin_left_pt
    }

    /// 内容区左上角 Y 坐标
    pub fn content_y(&self) -> f32 {
        self.margin_top_pt
    }

    /// 内容区宽度
    pub fn content_width(&self) -> f32 {
        self.width_pt - self.margin_left_pt - self.margin_right_pt
    }

    /// 内容区高度（无限高度模式下返回 f32::MAX）
    pub fn content_height(&self) -> f32 {
        if self.height_unlimited {
            f32::MAX
        } else {
            self.height_pt - self.margin_top_pt - self.margin_bottom_pt
        }
    }
}

impl From<PageConfig> for PageSettings {
    fn from(config: PageConfig) -> Self {
        Self {
            width_pt: config.width.unwrap_or(crate::document::types::PAGE_WIDTH_PT),
            height_pt: config.height.unwrap_or(crate::document::types::PAGE_HEIGHT_PT),
            margin_top_pt: config
                .margin_top
                .unwrap_or(crate::document::types::PAGE_MARGIN_TOP_PT),
            margin_bottom_pt: config
                .margin_bottom
                .unwrap_or(crate::document::types::PAGE_MARGIN_BOTTOM_PT),
            margin_left_pt: config
                .margin_left
                .unwrap_or(crate::document::types::PAGE_MARGIN_LEFT_PT),
            margin_right_pt: config
                .margin_right
                .unwrap_or(crate::document::types::PAGE_MARGIN_RIGHT_PT),
            height_unlimited: config.height_unlimited.unwrap_or(false),
            header: config.header,
            footer: config.footer,
            header_font_size: config.header_font_size.unwrap_or(9.0),
            footer_font_size: config.footer_font_size.unwrap_or(9.0),
        }
    }
}
