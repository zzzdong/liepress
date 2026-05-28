//! 内置样式定义模块
//!
//! 内置默认样式表。
//! 在没有用户 CSS 的情况下，使用这套硬编码的合理默认值。
//! 替换原来分散在 generator.rs 中的常量和辅助函数。

use crate::visual::Color;

use super::style::*;

/// 正文样式
pub fn paragraph_style() -> Style {
    Style {
        font_family: vec!["serif".to_string()],
        font_size_pt: 10.5,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        color: Color::new(0, 0, 0),
        line_height_pt: 15.75,
        margin_top_pt: 0.0,
        margin_bottom_pt: 12.0,
        text_align: TextAlign::Left,
        display: Display::Block,
        width: None,
        object_fit: ObjectFit::None,
    }
}

/// 标题样式（根据级别）
pub fn heading_style(level: u8) -> Style {
    let (size, line_height, margin_bottom) = match level {
        1 => (24.0, 28.8, 12.0),
        2 => (18.0, 21.6, 10.0),
        3 => (14.0, 16.8, 8.0),
        4 => (12.0, 14.4, 6.0),
        5 => (11.0, 13.2, 6.0),
        6 => (10.5, 12.6, 6.0),
        _ => (10.5, 12.6, 6.0),
    };
    Style {
        font_family: vec!["serif".to_string()],
        font_size_pt: size,
        font_weight: FontWeight::Bold,
        font_style: FontStyle::Normal,
        color: Color::new(0, 0, 0),
        line_height_pt: line_height,
        margin_top_pt: 0.0,
        margin_bottom_pt: margin_bottom,
        text_align: TextAlign::Left,
        display: Display::Block,
        width: None,
        object_fit: ObjectFit::None,
    }
}

/// 代码块样式
pub fn code_style() -> Style {
    Style {
        font_family: vec!["monospace".to_string()],
        font_size_pt: 9.0,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        color: Color::new(51, 51, 51),
        line_height_pt: 13.5,
        margin_top_pt: 0.0,
        margin_bottom_pt: 12.0,
        text_align: TextAlign::Left,
        display: Display::Block,
        width: None,
        object_fit: ObjectFit::None,
    }
}

/// 图片样式（容器级样式）
pub fn image_style() -> Style {
    Style {
        font_family: vec!["serif".to_string()],
        font_size_pt: 10.5,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        color: Color::new(0, 0, 0),
        line_height_pt: 15.75,
        margin_top_pt: 0.0,
        margin_bottom_pt: 12.0,
        text_align: TextAlign::Left,
        display: Display::Block,
        width: None,
        object_fit: ObjectFit::Contain,
    }
}

/// 列表项样式（容器级样式）
pub fn list_item_style() -> Style {
    Style {
        font_family: vec!["serif".to_string()],
        font_size_pt: 10.5,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        color: Color::new(0, 0, 0),
        line_height_pt: 15.75,
        margin_top_pt: 0.0,
        margin_bottom_pt: 4.0,
        text_align: TextAlign::Left,
        display: Display::Block,
        width: None,
        object_fit: ObjectFit::None,
    }
}

/// 分隔线样式
pub fn thematic_break_style() -> Style {
    Style {
        font_family: vec!["serif".to_string()],
        font_size_pt: 10.5,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        color: Color::new(0, 0, 0),
        line_height_pt: 15.75,
        margin_top_pt: 18.0,
        margin_bottom_pt: 18.0,
        text_align: TextAlign::Left,
        display: Display::Block,
        width: None,
        object_fit: ObjectFit::None,
    }
}

/// InlineCode 样式
pub fn inline_code_style() -> Style {
    Style {
        font_family: vec!["monospace".to_string()],
        font_size_pt: 10.5,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        color: Color::new(51, 51, 51),
        line_height_pt: 15.75,
        margin_top_pt: 0.0,
        margin_bottom_pt: 0.0,
        text_align: TextAlign::Left,
        display: Display::Inline,
        width: None,
        object_fit: ObjectFit::None,
    }
}

/// 链接样式
pub fn link_style() -> Style {
    Style {
        font_family: vec!["serif".to_string()],
        font_size_pt: 10.5,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        color: Color::new(0, 0, 255),
        line_height_pt: 15.75,
        margin_top_pt: 0.0,
        margin_bottom_pt: 0.0,
        text_align: TextAlign::Left,
        display: Display::Inline,
        width: None,
        object_fit: ObjectFit::None,
    }
}

/// 引用块样式
pub fn blockquote_style() -> Style {
    Style {
        font_family: vec!["serif".to_string()],
        font_size_pt: 10.5,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        color: Color::new(0, 0, 0),
        line_height_pt: 15.75,
        margin_top_pt: 12.0,
        margin_bottom_pt: 12.0,
        text_align: TextAlign::Left,
        display: Display::Block,
        width: None,
        object_fit: ObjectFit::None,
    }
}

/// 表格样式
pub fn table_style() -> Style {
    Style {
        font_family: vec!["serif".to_string()],
        font_size_pt: 10.5,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        color: Color::new(0, 0, 0),
        line_height_pt: 15.75,
        margin_top_pt: 12.0,
        margin_bottom_pt: 12.0,
        text_align: TextAlign::Left,
        display: Display::Block,
        width: None,
        object_fit: ObjectFit::None,
    }
}

/// 无序列表样式
pub fn unordered_list_style() -> Style {
    Style {
        font_family: vec!["serif".to_string()],
        font_size_pt: 10.5,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        color: Color::new(0, 0, 0),
        line_height_pt: 15.75,
        margin_top_pt: 0.0,
        margin_bottom_pt: 12.0,
        text_align: TextAlign::Left,
        display: Display::Block,
        width: None,
        object_fit: ObjectFit::None,
    }
}

/// 有序列表样式
pub fn ordered_list_style() -> Style {
    Style {
        font_family: vec!["serif".to_string()],
        font_size_pt: 10.5,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        color: Color::new(0, 0, 0),
        line_height_pt: 15.75,
        margin_top_pt: 0.0,
        margin_bottom_pt: 12.0,
        text_align: TextAlign::Left,
        display: Display::Block,
        width: None,
        object_fit: ObjectFit::None,
    }
}

/// 列表标记样式（用于 bullet 和 number）
pub fn list_marker_style() -> Style {
    Style {
        font_family: vec!["serif".to_string()],
        font_size_pt: 10.5,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        color: Color::new(0, 0, 0),
        line_height_pt: 15.75,
        margin_top_pt: 0.0,
        margin_bottom_pt: 0.0,
        text_align: TextAlign::Right,
        display: Display::Inline,
        width: None,
        object_fit: ObjectFit::None,
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
    }
}

// ─── 列表缩进宽度 ──────────────────────────────────────────

/// 默认列表缩进（2 个空格的近似宽度）
/// 基于正文字体 10.5pt，等宽字体中空格约为 0.6em
/// 2 个空格 ≈ 10.5 * 0.6 * 2 ≈ 12.6pt，取整为 12pt
pub const LIST_INDENT_PT: f32 = 12.0;

/// 计算基于字体大小的列表缩进
/// 2 个空格的宽度 ≈ font_size_pt * 0.6 * 2
pub fn calculate_list_indent(font_size_pt: f32) -> f32 {
    (font_size_pt * 1.2).max(10.0).min(20.0)
}
