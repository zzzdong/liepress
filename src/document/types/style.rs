//! 文档样式类型（投影自 [`crate::ast::Style`]）。
//!
//! 文档层使用"已解析"的样式（所有单位已落定为 pt），与布局引擎消费的
//! [`crate::ast::Style`] 保持一致。此处取出布局/分页所需的稳定子集，
//! 避免文档层依赖 ast 内部的全部字段。
//!
//! 样式枚举（`TextDecoration`/`TextAlign`/`WhiteSpace`/`ObjectFit` 等）以
//! [`crate::ast`] 为唯一真源，文档层不再重复定义，直接复用。

use crate::ast::{ObjectFit, PageBreak, TextAlign, TextDecoration, WhiteSpace};
use lievisual::Color;

/// 已解析的文档样式（布局/分页使用）。
///
/// 字段取自在 [`crate::ast::Style`] 中由 CSS 计算、单位已解析为 pt 的值。
/// 仅保留文档层需要的稳定子集。
#[derive(Clone, Debug)]
pub struct ResolvedStyle {
    // 排版
    pub font_family: Vec<String>,
    pub font_size_pt: f32,
    pub font_weight_bold: bool,
    pub font_style_italic: bool,
    pub color: Color,

    // 布局
    pub line_height_pt: f32,
    pub letter_spacing: f32,
    pub text_indent_em: f32,
    pub text_align: TextAlign,
    pub white_space: WhiteSpace,

    // Box Model
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub margin_left: f32,
    pub margin_right: f32,
    pub padding_top: f32,
    pub padding_bottom: f32,
    pub padding_left: f32,
    pub padding_right: f32,
    pub border_width_top: f32,
    pub border_width_bottom: f32,
    pub border_width_left: f32,
    pub border_width_right: f32,

    // 尺寸
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub object_fit: ObjectFit,

    // 装饰
    pub background_color: Option<Color>,
    /// 边框颜色：从 ast 的 border 四边中取第一个可见边的颜色；无可见边则为 None。
    pub border_color: Option<Color>,

    // 文本修饰
    pub text_decoration: TextDecoration,

    // 基线偏移（pt，正数=上标上移，负数=下标下移）
    pub baseline_shift: f32,

    // 表格（投影自 ast::Style 的 table_* 字段）
    pub table_border_color: Color,
    pub table_border_width_pt: f32,
    pub table_cell_padding_h_pt: f32,
    pub table_cell_padding_v_pt: f32,
    pub table_header_bg: Option<Color>,
    pub table_alt_row_bg: Option<Color>,

    // 分页控制（投影自 ast::Style；仅分页后端 PDF 消费，SVG/PNG 单画布忽略）
    pub page_break_before: PageBreak,
    pub page_break_after: PageBreak,
}

impl Default for ResolvedStyle {
    fn default() -> Self {
        ResolvedStyle {
            font_family: vec!["serif".to_string()],
            font_size_pt: 10.5,
            font_weight_bold: false,
            font_style_italic: false,
            color: Color::BLACK,
            line_height_pt: 15.0,
            letter_spacing: 0.0,
            text_indent_em: 0.0,
            text_align: TextAlign::Left,
            white_space: WhiteSpace::Normal,
            margin_top: 0.0,
            margin_bottom: 0.0,
            margin_left: 0.0,
            margin_right: 0.0,
            padding_top: 0.0,
            padding_bottom: 0.0,
            padding_left: 0.0,
            padding_right: 0.0,
            border_width_top: 0.0,
            border_width_bottom: 0.0,
            border_width_left: 0.0,
            border_width_right: 0.0,
            width: None,
            height: None,
            object_fit: ObjectFit::Contain,
            background_color: None,
            border_color: None,
            text_decoration: TextDecoration::None,
            baseline_shift: 0.0,
            table_border_color: Color::rgb(180, 180, 180),
            table_border_width_pt: 0.5,
            table_cell_padding_h_pt: 4.0,
            table_cell_padding_v_pt: 2.0,
            table_header_bg: None,
            table_alt_row_bg: None,
            page_break_before: PageBreak::Auto,
            page_break_after: PageBreak::Auto,
        }
    }
}

impl From<crate::ast::Style> for ResolvedStyle {
    /// 从计算样式投影出文档层所需的稳定子集。
    fn from(s: crate::ast::Style) -> Self {
        use crate::ast::{Display, FontStyle};
        // 注意：被 CSS 覆盖为 inline 的块级元素其 margin 会被
        // `inherit_from` 清零；此处原样投影，语义由上层决定。
        let _ = (Display::Inline, FontStyle::Italic);
        ResolvedStyle {
            font_family: s.font_family.clone(),
            font_size_pt: s.font_size_pt,
            font_weight_bold: s.font_weight == crate::ast::FontWeight::Bold,
            font_style_italic: s.font_style == FontStyle::Italic,
            color: s.color,
            line_height_pt: s.line_height_pt,
            letter_spacing: s.letter_spacing,
            text_indent_em: s.text_indent_em,
            text_align: s.text_align,
            white_space: s.white_space,
            margin_top: s.margin.top,
            margin_bottom: s.margin.bottom,
            margin_left: s.margin.left,
            margin_right: s.margin.right,
            padding_top: s.padding.top,
            padding_bottom: s.padding.bottom,
            padding_left: s.padding.left,
            padding_right: s.padding.right,
            border_width_top: s.border.top.width,
            border_width_bottom: s.border.bottom.width,
            border_width_left: s.border.left.width,
            border_width_right: s.border.right.width,
            width: s.width,
            height: s.height,
            object_fit: s.object_fit,
            background_color: s.background_color,
            border_color: [
                &s.border.top,
                &s.border.right,
                &s.border.bottom,
                &s.border.left,
            ]
            .into_iter()
            .find(|side| side.is_visible())
            .map(|side| side.color),
            text_decoration: s.text_decoration,
            baseline_shift: s.baseline_shift,
            table_border_color: s.table_border_color,
            table_border_width_pt: s.table_border_width_pt,
            table_cell_padding_h_pt: s.table_cell_padding_h_pt,
            table_cell_padding_v_pt: s.table_cell_padding_v_pt,
            table_header_bg: s.table_header_bg,
            table_alt_row_bg: s.table_alt_row_bg,
            page_break_before: s.page_break_before,
            page_break_after: s.page_break_after,
        }
    }
}
