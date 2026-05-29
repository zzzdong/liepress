//! 样式定义模块
//!
//! 定义所有支持的样式属性和计算后的样式值。
//! 这是布局引擎消费的最终样式数据结构。

use crate::visual::Color;

// ─── 字体字重 ───

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontWeight {
    Normal,
    Bold,
}

impl FontWeight {
    pub fn as_str(&self) -> &'static str {
        match self {
            FontWeight::Normal => "normal",
            FontWeight::Bold => "bold",
        }
    }
}

// ─── 字体样式 ───

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontStyle {
    Normal,
    Italic,
}

impl FontStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            FontStyle::Normal => "normal",
            FontStyle::Italic => "italic",
        }
    }
}

// ─── 文本对齐 ───

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justify,
}

impl TextAlign {
    pub fn as_str(&self) -> &'static str {
        match self {
            TextAlign::Left => "left",
            TextAlign::Center => "center",
            TextAlign::Right => "right",
            TextAlign::Justify => "justify",
        }
    }
}

// ─── 显示类型 ───

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Display {
    Block,
    Inline,
    InlineBlock,
}

// ─── 图片适应方式 ───

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ObjectFit {
    Contain,
    Cover,
    Fill,
    None,
}

// ─── 计算样式 ───

/// 计算后的样式值（所有单位已解析为 pt）
/// 这是布局引擎消费的最终样式数据
#[derive(Debug, Clone)]
pub struct Style {
    // 排版（优先级从高到低的字体家族列表）
    pub font_family: Vec<String>,
    pub font_size_pt: f32,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub color: Color,

    // 布局
    pub line_height_pt: f32,
    pub margin_top_pt: f32,
    pub margin_bottom_pt: f32,
    pub text_align: TextAlign,
    pub display: Display,

    // 图片
    pub width: Option<f32>,
    pub object_fit: ObjectFit,

    // 表格
    pub table_border_color: Color,
    pub table_border_width_pt: f32,
    pub table_cell_padding_h_pt: f32,
    pub table_cell_padding_v_pt: f32,
    pub table_header_bg: Option<Color>,
    pub table_alt_row_bg: Option<Color>,

    // 链接
    pub link_url: Option<String>,
}

/// 计算样式的"继承"策略。
/// 当内联元素（Strong、Emphasis 等）没有显式指定某属性时，从父元素继承。
impl Style {
    /// 从父样式继承创建一个子样式，只覆盖特定属性
    pub fn inherit_from(parent: &Style) -> Self {
        Self {
            font_family: parent.font_family.clone(),
            font_size_pt: parent.font_size_pt,
            font_weight: parent.font_weight,
            font_style: parent.font_style,
            color: parent.color,
            line_height_pt: parent.line_height_pt,
            margin_top_pt: 0.0,
            margin_bottom_pt: 0.0,
            text_align: parent.text_align,
            display: Display::Inline,
            width: None,
            object_fit: ObjectFit::None,
            table_border_color: parent.table_border_color,
            table_border_width_pt: parent.table_border_width_pt,
            table_cell_padding_h_pt: parent.table_cell_padding_h_pt,
            table_cell_padding_v_pt: parent.table_cell_padding_v_pt,
            table_header_bg: parent.table_header_bg,
            table_alt_row_bg: parent.table_alt_row_bg,
            link_url: parent.link_url.clone(),
        }
    }
}

impl Default for Style {
    fn default() -> Self {
        Self {
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
            table_border_color: Color::new(180, 180, 180),
            table_border_width_pt: 0.5,
            table_cell_padding_h_pt: 4.0,
            table_cell_padding_v_pt: 2.0,
            table_header_bg: None,
            table_alt_row_bg: None,
            link_url: None,
        }
    }
}
