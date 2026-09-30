//! 页面设置（分页后端输入）。
//!
//! `PageSettings` 不属于 `document` 源 IR（源 IR 不分页），而是各输出后端
//! （PDF/DOCX）做分页时的输入参数。放在此处是因为它语义上属于"文档层公共类型"，
//! 且被 `from_ast`（排版宽度计算）与 `render::pdf`（分页）共同使用。

use crate::ast::{PageConfig, TextAlign};

/// 页眉/页脚区行高相对字号的系数（行高 = 字号 × 该系数）。
///
/// 页眉/页脚各自占据内容区上方/下方的一条独立条带，排版内容区时先扣除，
/// 避免正文与页眉/页脚重叠（见 [`PageSettings::header_height`]）。
const HEADER_FOOTER_LINE_FACTOR: f32 = 1.4;

/// 页眉/页脚默认字体族（与历史行为一致）。
fn default_header_footer_family() -> Vec<String> {
    vec!["serif".to_string()]
}

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
    /// 页眉文本对齐，默认居中
    pub header_align: TextAlign,
    /// 页脚文本对齐，默认居中
    pub footer_align: TextAlign,
    /// 页眉字体族（优先级从高到低的回退列表），默认 `["serif"]`
    pub header_font_family: Vec<String>,
    /// 页脚字体族（优先级从高到低的回退列表），默认 `["serif"]`
    pub footer_font_family: Vec<String>,
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
            header_align: TextAlign::Center,
            footer_align: TextAlign::Center,
            header_font_family: default_header_footer_family(),
            footer_font_family: default_header_footer_family(),
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

    /// 内容区左上角 Y 坐标（= 上边距 + 页眉区高度）
    ///
    /// 排版起点位于页眉条带下方，正文不会与页眉重叠。
    pub fn content_y(&self) -> f32 {
        self.margin_top_pt + self.header_height()
    }

    /// 内容区宽度
    pub fn content_width(&self) -> f32 {
        self.width_pt - self.margin_left_pt - self.margin_right_pt
    }

    /// 页眉区高度（pt）：未设置页眉时为 0。
    ///
    /// 分页/排版时从内容区顶部扣除，使正文让位给页眉。
    pub fn header_height(&self) -> f32 {
        if self.header.is_some() {
            self.header_font_size * HEADER_FOOTER_LINE_FACTOR
        } else {
            0.0
        }
    }

    /// 页脚区高度（pt）：未设置页脚时为 0。
    ///
    /// 分页/排版时从内容区底部扣除，使正文让位给页脚。
    pub fn footer_height(&self) -> f32 {
        if self.footer.is_some() {
            self.footer_font_size * HEADER_FOOTER_LINE_FACTOR
        } else {
            0.0
        }
    }

    /// 内容区高度（已扣除上/下边距与页眉/页脚区高度）。
    ///
    /// 无限高度模式下返回 `f32::MAX`（不分页，内容高度自适应）。
    pub fn content_height(&self) -> f32 {
        if self.height_unlimited {
            f32::MAX
        } else {
            (self.height_pt
                - self.margin_top_pt
                - self.margin_bottom_pt
                - self.header_height()
                - self.footer_height())
            .max(0.0)
        }
    }
}

impl From<PageConfig> for PageSettings {
    fn from(config: PageConfig) -> Self {
        Self {
            width_pt: config
                .width
                .unwrap_or(crate::document::types::PAGE_WIDTH_PT),
            height_pt: config
                .height
                .unwrap_or(crate::document::types::PAGE_HEIGHT_PT),
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
            header_align: config.header_align.unwrap_or(TextAlign::Center),
            footer_align: config.footer_align.unwrap_or(TextAlign::Center),
            header_font_family: config
                .header_font_family
                .unwrap_or_else(default_header_footer_family),
            footer_font_family: config
                .footer_font_family
                .unwrap_or_else(default_header_footer_family),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_area_reserves_header_and_footer_bands() {
        // 默认仅页脚（"- {page} -"），无页眉：内容区顶部仍在上边距处，
        // 但可用高度已扣除下边距与页脚条带。
        let mut s = PageSettings {
            header: None,
            footer: Some("- {page} -".to_string()),
            ..PageSettings::default()
        };
        let footer_h = 9.0 * HEADER_FOOTER_LINE_FACTOR;
        assert_eq!(s.header_height(), 0.0);
        assert!((s.footer_height() - footer_h).abs() < 1e-6);
        assert!((s.content_y() - s.margin_top_pt).abs() < 1e-6);
        let expect_no_header = s.height_pt - s.margin_top_pt - s.margin_bottom_pt - footer_h;
        assert!((s.content_height() - expect_no_header).abs() < 1e-4);

        // 加页眉后：内容区顶部下移一个页眉条带，可用高度再扣除页眉条带。
        s.header = Some("head".to_string());
        let header_h = 9.0 * HEADER_FOOTER_LINE_FACTOR;
        assert!((s.header_height() - header_h).abs() < 1e-6);
        assert!((s.content_y() - (s.margin_top_pt + header_h)).abs() < 1e-4);
        let expect_both = s.height_pt - s.margin_top_pt - s.margin_bottom_pt - header_h - footer_h;
        assert!((s.content_height() - expect_both).abs() < 1e-4);
    }

    #[test]
    fn content_height_never_negative_with_huge_header_footer() {
        // 页眉/页脚条带超过页面可用高度时，内容区高度钳制为 0（不得为负）。
        let s = PageSettings {
            header: Some("h".to_string()),
            footer: Some("f".to_string()),
            header_font_size: 400.0,
            footer_font_size: 400.0,
            ..PageSettings::default()
        };
        assert_eq!(s.content_height(), 0.0);
    }

    #[test]
    fn header_footer_align_defaults_to_center_and_is_configurable() {
        let s = PageSettings::default();
        assert_eq!(s.header_align, TextAlign::Center);
        assert_eq!(s.footer_align, TextAlign::Center);

        let cfg = PageConfig {
            header_align: Some(TextAlign::Left),
            footer_align: Some(TextAlign::Right),
            ..PageConfig::default()
        };
        let s = PageSettings::from(cfg);
        assert_eq!(s.header_align, TextAlign::Left);
        assert_eq!(s.footer_align, TextAlign::Right);
    }

    #[test]
    fn header_footer_font_family_defaults_to_serif_and_is_configurable() {
        let s = PageSettings::default();
        assert_eq!(s.header_font_family, vec!["serif".to_string()]);
        assert_eq!(s.footer_font_family, vec!["serif".to_string()]);

        let cfg = PageConfig {
            header_font_family: Some(vec!["Noto Serif CJK SC".to_string(), "serif".to_string()]),
            footer_font_family: Some(vec!["Noto Sans CJK SC".to_string()]),
            ..PageConfig::default()
        };
        let s = PageSettings::from(cfg);
        assert_eq!(
            s.header_font_family,
            vec!["Noto Serif CJK SC".to_string(), "serif".to_string()]
        );
        assert_eq!(s.footer_font_family, vec!["Noto Sans CJK SC".to_string()]);
    }
}
