//! 内置默认样式与辅助函数
//!
//! # 设计
//!
//! 内置一套完整的 CSS 样式表（DEFAULT_CSS），作为默认样式。
//! 用户可以通过提供自己的 CSS 文件来覆盖默认样式。
//!
//! 此外保留必要的辅助函数：
//! - `computed_style_to_text_style`: 将 Style 转换为 text::TextStyle
//! - `list_marker_style`: 列表标记样式（用于 bullet/number 布局计算）

use lievisual::Color;

use super::style::*;

/// 内置默认 CSS 样式表
///
/// 等效于原来的硬编码样式函数（paragraph_style, heading_style 等）。
/// 用户可以提供自己的 CSS 来覆盖这些样式。
///
/// 定义在 `presets/default.css` 中。
pub const DEFAULT_CSS: &str = include_str!("presets/default.css");

/// 列表标记样式（用于 bullet 和 number 的布局计算）
///
/// 此样式专用于生成器计算标记宽度和布局，
/// 不由 CSS 系统控制（标记是自动生成的），因此保留为硬编码函数。
pub fn list_marker_style() -> Style {
    Style {
        font_family: vec![
            "FangSong".to_string(),
            "FangSong_GB2312".to_string(),
            "Noto Serif CJK SC".to_string(),
            "Source Han Serif SC".to_string(),
            "Noto Sans CJK SC".to_string(),
            "Noto Sans SC".to_string(),
            "serif".to_string(),
        ],
        font_size_pt: 10.5,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        color: Color::rgb(0, 0, 0),
        line_height_pt: 15.75,
        letter_spacing: 0.0,
        text_align: TextAlign::Right,
        display: Display::Inline,
        margin: BoxSides::ZERO,
        padding: BoxSides::ZERO,
        border: BoxBorders::NONE,
        width: None,
        height: None,
        object_fit: ObjectFit::None,
        background_color: None,
        page_break_before: PageBreak::Auto,
        page_break_after: PageBreak::Auto,
        table_border_color: Color::rgb(180, 180, 180),
        table_border_width_pt: 0.5,
        table_cell_padding_h_pt: 4.0,
        table_cell_padding_v_pt: 2.0,
        table_header_bg: None,
        table_alt_row_bg: None,
        link_url: None,
        text_decoration: TextDecoration::None,
        list_indent_pt: None,
        text_indent_em: 0.0,
        baseline_shift: 0.0,
        white_space: WhiteSpace::Normal,
        list_style_type: ListStyleType::Disc,
    }
}

/// 将 ComputedStyle 转换为 text::TextStyle
/// 供生成器在创建文本布局时使用
pub fn computed_style_to_text_style(style: &Style) -> crate::document::text::TextStyle {
    // Justify 暂不支持，回退为左对齐。
    let align = match style.text_align {
        TextAlign::Justify => crate::document::text::TextAlign::Left,
        a => a,
    };
    crate::document::text::css_text_style(
        style.color,
        &style.font_family,
        style.font_size_pt as f64,
        &font_weight_css(style.font_weight),
        font_style_css(style.font_style),
        align,
        style.link_url.clone(),
        style.text_decoration,
        style.baseline_shift,
        style.background_color,
        // line-height 为 0 表示未声明（保持字体默认）。
        (style.line_height_pt > 0.0).then_some(style.line_height_pt as f64),
    )
}
