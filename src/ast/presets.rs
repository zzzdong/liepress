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
pub const DEFAULT_CSS: &str = r##"/* liepress 内置默认样式表 */

body {
    font-family: serif;
    font-size: 10.5pt;
    line-height: 1.5;
    color: #000;
    margin: 0;
    text-align: left;
}

/* 标题 (h1-h6) */
h1 { font-size: 24pt; line-height: 1.2; font-weight: bold; margin-bottom: 12pt; }
h2 { font-size: 18pt; line-height: 1.2; font-weight: bold; margin-bottom: 10pt; }
h3 { font-size: 14pt; line-height: 1.2; font-weight: bold; margin-bottom: 8pt; }
h4 { font-size: 12pt; line-height: 1.2; font-weight: bold; margin-bottom: 6pt; }
h5 { font-size: 11pt; line-height: 1.2; font-weight: bold; margin-bottom: 6pt; }
h6 { font-size: 10.5pt; line-height: 1.2; font-weight: bold; margin-bottom: 6pt; }

/* 段落 */
p {
    margin-top: 0;
    margin-bottom: 12pt;
}

/* 代码块 */
pre {
    font-family: monospace;
    font-size: 9pt;
    line-height: 1.5;
    color: #333;
    margin-top: 0;
    margin-bottom: 12pt;
    display: block;
}

/* 行内代码 */
code {
    font-family: monospace;
    font-size: 10.5pt;
    color: #333;
    display: inline;
}

/* 图片 */
img {
    display: block;
    object-fit: contain;
    margin-top: 0;
    margin-bottom: 12pt;
}

/* 列表容器 */
ul, ol {
    margin-top: 0;
    margin-bottom: 12pt;
    display: block;
    list-indent: 2em;
}

/* 列表项 */
li {
    display: block;
    margin-top: 0;
    margin-bottom: 4pt;
}

/* 引用块 */
blockquote {
    margin-top: 12pt;
    margin-bottom: 12pt;
    display: block;
}

/* 分隔线 */
hr {
    margin-top: 18pt;
    margin-bottom: 18pt;
    display: block;
}

/* 表格 */
table {
    display: block;
    margin-top: 12pt;
    margin-bottom: 12pt;
    border-collapse: collapse;
    table-header-background: #e8e8e8;
    table-alt-row-background: #f8f8f8;
    table-border-color: #b4b4b4;
    table-border-width: 0.5pt;
    table-cell-padding-horizontal: 4pt;
    table-cell-padding-vertical: 2pt;
}

/* 加粗 */
strong {
    font-weight: bold;
    display: inline;
}

/* 斜体 */
em {
    font-style: italic;
    display: inline;
}

/* 链接 */
a {
    color: #00f;
    font-style: italic;
    display: inline;
}

/* 删除线 */
del {
    display: inline;
}

/* 普通文本 */
span {
    display: inline;
}
"##;

/// 列表标记样式（用于 bullet 和 number 的布局计算）
///
/// 此样式专用于生成器计算标记宽度和布局，
/// 不由 CSS 系统控制（标记是自动生成的），因此保留为硬编码函数。
pub fn list_marker_style() -> Style {
    Style {
        font_family: vec!["serif".to_string()],
        font_size_pt: 10.5,
        font_weight: FontWeight::Normal,
        font_style: FontStyle::Normal,
        color: Color::new(0, 0, 0),
        line_height_pt: 15.75,
        letter_spacing: 0.0,
        text_align: TextAlign::Right,
        display: Display::Inline,
        margin_top_pt: 0.0,
        margin_bottom_pt: 0.0,
        margin_left_pt: 0.0,
        margin_right_pt: 0.0,
        padding_top_pt: 0.0,
        padding_bottom_pt: 0.0,
        padding_left_pt: 0.0,
        padding_right_pt: 0.0,
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
        list_indent_pt: None,
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
