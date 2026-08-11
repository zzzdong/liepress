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
//! - `LIST_INDENT_PT` / `calculate_list_indent`: 列表缩进计算

use crate::visual::Color;

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
        color: Color::new(0, 0, 0),
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
        table_border_color: Color::new(180, 180, 180),
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
pub fn computed_style_to_text_style(style: &Style) -> crate::text::TextStyle {
    use crate::text::TextAlign as TextAlign2;

    let align = match style.text_align {
        TextAlign::Left => TextAlign2::Left,
        TextAlign::Center => TextAlign2::Center,
        TextAlign::Right => TextAlign2::Right,
        TextAlign::Justify => TextAlign2::Left, // 暂不支持两端对齐，回退到左对齐
    };

    crate::text::TextStyle {
        color: style.color,
        font_family: style.font_family.clone(),
        font_size: style.font_size_pt as f64,
        font_weight: style.font_weight.as_str().to_string(),
        font_style: style.font_style.as_str().to_string(),
        align,
        url: style.link_url.clone(),
        decoration: style.text_decoration,
        baseline_shift: style.baseline_shift,
        background_color: style.background_color,
    }
}

// ─── 列表缩进宽度 ──────────────────────────────────────────

/// 默认列表缩进（2em 标准宽度）
/// 遵循 CSS 常见做法，缩进宽度为当前字号的两倍。
/// 正文 10.5pt 下为 21pt。
pub const LIST_INDENT_PT: f32 = 21.0;

/// 计算基于字体大小的列表缩进
/// 2em = font_size_pt * 2.0
pub fn calculate_list_indent(font_size_pt: f32) -> f32 {
    font_size_pt * 2.0
}
