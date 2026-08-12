//! 文档样式类型（投影自 [`crate::ast::Style`]）。
//!
//! 文档层使用"已解析"的样式（所有单位已落定为 pt），与布局引擎消费的
//! [`crate::ast::Style`] 保持一致。此处取出布局/分页所需的稳定子集，
//! 避免文档层依赖 ast 内部的全部字段。

use crate::document::types::DocColor;

/// 文本装饰（投影自 [`crate::ast::TextDecoration`]，与
/// [`crate::text::TextDecoration`] 同源）。
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum TextDecoration {
    #[default]
    None,
    Underline,
    LineThrough,
}

impl From<crate::ast::TextDecoration> for TextDecoration {
    fn from(d: crate::ast::TextDecoration) -> Self {
        match d {
            crate::ast::TextDecoration::None => TextDecoration::None,
            crate::ast::TextDecoration::Underline => TextDecoration::Underline,
            crate::ast::TextDecoration::LineThrough => TextDecoration::LineThrough,
        }
    }
}

impl From<TextDecoration> for crate::ast::TextDecoration {
    fn from(d: TextDecoration) -> Self {
        match d {
            TextDecoration::None => crate::ast::TextDecoration::None,
            TextDecoration::Underline => crate::ast::TextDecoration::Underline,
            TextDecoration::LineThrough => crate::ast::TextDecoration::LineThrough,
        }
    }
}

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
    pub color: DocColor,

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
    pub background_color: Option<DocColor>,

    // 文本修饰
    pub text_decoration: TextDecoration,

    // 基线偏移（pt，正数=上标上移，负数=下标下移）
    pub baseline_shift: f32,
}

/// 文本对齐（投影自 [`crate::ast::TextAlign`]）。
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

impl From<crate::ast::TextAlign> for TextAlign {
    fn from(a: crate::ast::TextAlign) -> Self {
        match a {
            crate::ast::TextAlign::Left => TextAlign::Left,
            crate::ast::TextAlign::Center => TextAlign::Center,
            crate::ast::TextAlign::Right => TextAlign::Right,
            crate::ast::TextAlign::Justify => TextAlign::Justify,
        }
    }
}

impl From<TextAlign> for crate::ast::TextAlign {
    fn from(a: TextAlign) -> Self {
        match a {
            TextAlign::Left => crate::ast::TextAlign::Left,
            TextAlign::Center => crate::ast::TextAlign::Center,
            TextAlign::Right => crate::ast::TextAlign::Right,
            TextAlign::Justify => crate::ast::TextAlign::Justify,
        }
    }
}

/// 空白处理（投影自 [`crate::ast::WhiteSpace`]）。
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum WhiteSpace {
    #[default]
    Normal,
    Pre,
    NoWrap,
}

impl From<crate::ast::WhiteSpace> for WhiteSpace {
    fn from(w: crate::ast::WhiteSpace) -> Self {
        match w {
            crate::ast::WhiteSpace::Normal => WhiteSpace::Normal,
            crate::ast::WhiteSpace::Pre => WhiteSpace::Pre,
            crate::ast::WhiteSpace::NoWrap => WhiteSpace::NoWrap,
        }
    }
}

impl From<WhiteSpace> for crate::ast::WhiteSpace {
    fn from(w: WhiteSpace) -> Self {
        match w {
            WhiteSpace::Normal => crate::ast::WhiteSpace::Normal,
            WhiteSpace::Pre => crate::ast::WhiteSpace::Pre,
            WhiteSpace::NoWrap => crate::ast::WhiteSpace::NoWrap,
        }
    }
}

/// 图片适应方式（投影自 [`crate::ast::ObjectFit`]）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ObjectFit {
    Contain,
    Cover,
    Fill,
    None,
}

impl From<crate::ast::ObjectFit> for ObjectFit {
    fn from(o: crate::ast::ObjectFit) -> Self {
        match o {
            crate::ast::ObjectFit::Contain => ObjectFit::Contain,
            crate::ast::ObjectFit::Cover => ObjectFit::Cover,
            crate::ast::ObjectFit::Fill => ObjectFit::Fill,
            crate::ast::ObjectFit::None => ObjectFit::None,
        }
    }
}

impl From<ObjectFit> for crate::ast::ObjectFit {
    fn from(o: ObjectFit) -> Self {
        match o {
            ObjectFit::Contain => crate::ast::ObjectFit::Contain,
            ObjectFit::Cover => crate::ast::ObjectFit::Cover,
            ObjectFit::Fill => crate::ast::ObjectFit::Fill,
            ObjectFit::None => crate::ast::ObjectFit::None,
        }
    }
}

impl Default for ResolvedStyle {
    fn default() -> Self {
        ResolvedStyle {
            font_family: vec!["serif".to_string()],
            font_size_pt: 10.5,
            font_weight_bold: false,
            font_style_italic: false,
            color: DocColor::BLACK,
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
            text_decoration: TextDecoration::None,
            baseline_shift: 0.0,
        }
    }
}

impl From<crate::ast::Style> for ResolvedStyle {
    /// 从计算样式投影出文档层所需的稳定子集。
    fn from(s: crate::ast::Style) -> Self {
        use crate::ast::{Display, FontStyle, FontWeight};
        // 注意：被 CSS 覆盖为 inline 的块级元素其 margin 会被
        // `inherit_from` 清零；此处原样投影，语义由上层决定。
        let _ = (Display::Inline, FontStyle::Italic, FontWeight::Bold);
        ResolvedStyle {
            font_family: s.font_family.clone(),
            font_size_pt: s.font_size_pt,
            font_weight_bold: s.font_weight == FontWeight::Bold,
            font_style_italic: s.font_style == FontStyle::Italic,
            color: DocColor::from(s.color),
            line_height_pt: s.line_height_pt,
            letter_spacing: s.letter_spacing,
            text_indent_em: s.text_indent_em,
            text_align: TextAlign::from(s.text_align),
            white_space: WhiteSpace::from(s.white_space),
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
            object_fit: ObjectFit::from(s.object_fit),
            background_color: s.background_color.map(DocColor::from),
            text_decoration: TextDecoration::from(s.text_decoration),
            baseline_shift: s.baseline_shift,
        }
    }
}
