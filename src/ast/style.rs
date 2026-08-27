//! 样式定义模块
//!
//! 定义所有支持的样式属性和计算后的样式值。
//! 这是布局引擎消费的最终样式数据结构。

use lievisual::Color;

// ─── 文本排版类型（复用 lievisual 定义，不再自行定义） ───
//
// lievisual 已提供 `FontStyle` / `TextAlign` / `TextDecoration` / `FontWeight` /
// `FontWidth` 等类型，直接复用，不再自行定义。

pub use lievisual::text::{FontStyle, FontWeight, FontWidth, TextAlign, TextDecoration};

/// 字重 → CSS `font-weight` 字符串。
pub fn font_weight_css(w: FontWeight) -> String {
    if w == FontWeight::Bold {
        "bold".to_string()
    } else {
        format!("{}", w.value())
    }
}

/// 字体样式 → CSS `font-style` 字符串（转发到 lievisual 的 `as_str`）。
pub fn font_style_css(s: FontStyle) -> &'static str {
    s.as_str()
}

/// 文本对齐 → CSS `text-align` 字符串。
pub fn text_align_css(a: TextAlign) -> &'static str {
    match a {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
        TextAlign::Justify => "justify",
    }
}

/// 文本修饰 → CSS `text-decoration` 字符串。
pub fn text_decoration_css(d: TextDecoration) -> &'static str {
    match d {
        TextDecoration::None => "none",
        TextDecoration::Underline => "underline",
        TextDecoration::LineThrough => "line-through",
    }
}

// ─── 分页控制 ───

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PageBreak {
    #[default]
    Auto,
    Always,
    Avoid,
    Left,
    Right,
}

// ─── 显示类型 ───

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Display {
    Block,
    Inline,
    InlineBlock,
    None,
}

// ─── 空白处理 ───

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum WhiteSpace {
    /// 正常合并空白、自动换行（默认）
    #[default]
    Normal,
    /// 保留空白和换行、不自动换行（如 <pre>）
    Pre,
    /// 合并空白、不自动换行
    NoWrap,
}

// ─── 列表样式 ───

/// CSS list-style-type
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ListStyleType {
    #[default]
    Disc,
    Circle,
    Square,
    Decimal,
    DecimalLeadingZero,
    LowerRoman,
    UpperRoman,
    LowerAlpha,
    UpperAlpha,
    None,
}

// ─── 边框样式 ───

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BorderStyle {
    #[default]
    None,
    Solid,
    Dashed,
    Dotted,
}

// ─── Box Model 边 ───

/// 四边相同的盒模型边（用于 margin、padding 等）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxSides {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl BoxSides {
    pub const ZERO: Self = Self {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    pub fn all(v: f32) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    pub fn vertical(v: f32) -> Self {
        Self {
            top: v,
            right: 0.0,
            bottom: v,
            left: 0.0,
        }
    }

    pub fn horizontal(v: f32) -> Self {
        Self {
            top: 0.0,
            right: v,
            bottom: 0.0,
            left: v,
        }
    }

    pub fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// 水平总和（left + right）
    pub fn horizontal_sum(&self) -> f32 {
        self.left + self.right
    }

    /// 垂直总和（top + bottom）
    pub fn vertical_sum(&self) -> f32 {
        self.top + self.bottom
    }
}

impl Default for BoxSides {
    fn default() -> Self {
        Self::ZERO
    }
}

// ─── 边框 ───

/// 单条边的边框
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderSide {
    pub width: f32,
    pub style: BorderStyle,
    pub color: Color,
}

impl BorderSide {
    pub const NONE: Self = Self {
        width: 0.0,
        style: BorderStyle::None,
        color: Color::rgb(0, 0, 0),
    };

    pub fn new(width: f32, style: BorderStyle, color: Color) -> Self {
        Self {
            width,
            style,
            color,
        }
    }

    pub fn is_visible(&self) -> bool {
        self.style != BorderStyle::None && self.width > 0.0
    }
}

impl Default for BorderSide {
    fn default() -> Self {
        Self::NONE
    }
}

/// 四边边框
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxBorders {
    pub top: BorderSide,
    pub right: BorderSide,
    pub bottom: BorderSide,
    pub left: BorderSide,
    pub radius: f32, // border-radius，所有角相同
}

impl BoxBorders {
    pub const NONE: Self = Self {
        top: BorderSide::NONE,
        right: BorderSide::NONE,
        bottom: BorderSide::NONE,
        left: BorderSide::NONE,
        radius: 0.0,
    };

    /// 从统一的 border 属性创建
    pub fn uniform(width: f32, style: BorderStyle, color: Color) -> Self {
        Self {
            top: BorderSide::new(width, style, color),
            right: BorderSide::new(width, style, color),
            bottom: BorderSide::new(width, style, color),
            left: BorderSide::new(width, style, color),
            radius: 0.0,
        }
    }

    pub fn is_any_visible(&self) -> bool {
        self.top.is_visible()
            || self.right.is_visible()
            || self.bottom.is_visible()
            || self.left.is_visible()
    }

    /// 最大边框宽度（用于计算 box 尺寸）
    pub fn max_width(&self) -> f32 {
        self.top
            .width
            .max(self.right.width)
            .max(self.bottom.width)
            .max(self.left.width)
    }
}

impl Default for BoxBorders {
    fn default() -> Self {
        Self::NONE
    }
}

// ─── 图片适应方式 ───

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ObjectFit {
    Contain,
    Cover,
    Fill,
    None,
}

// ─── 文本修饰（复用 lievisual::text::TextDecoration，见文件顶部） ───

// ─── CSS 长度值（未解析） ───────────

/// CSS 长度值，保留原始单位。
/// 在 CSS 解析阶段使用，在布局前解析为 pt。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CssLength {
    /// 点（印刷单位）
    Pt(f32),
    /// 像素（屏幕单位）
    Px(f32),
    /// 相对当前元素字号
    Em(f32),
    /// 相对根元素字号
    Rem(f32),
    /// 百分比
    Percent(f32),
}

impl CssLength {
    /// 解析为 pt 值
    ///
    /// # 参数
    /// - `font_size`: 当前元素的计算字号（pt），用于解析 `em`
    /// - `root_font_size`: 根元素字号（pt），用于解析 `rem`
    pub fn resolve(&self, font_size: f32, root_font_size: f32) -> f32 {
        match self {
            CssLength::Pt(v) => *v,
            CssLength::Px(v) => v * 0.75,
            CssLength::Em(v) => v * font_size,
            CssLength::Rem(v) => v * root_font_size,
            CssLength::Percent(v) => v / 100.0 * font_size,
        }
    }
}

/// 行高值（可能是长度或纯数字乘数）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeight {
    /// 带单位的长度值
    Length(CssLength),
    /// 纯数字乘数（如 line-height: 1.5）
    Number(f32),
}

impl LineHeight {
    /// 解析为 pt 值
    pub fn resolve(&self, font_size: f32, root_font_size: f32) -> f32 {
        match self {
            LineHeight::Length(l) => l.resolve(font_size, root_font_size),
            LineHeight::Number(n) => n * font_size,
        }
    }
}

// ─── 计算样式 ───

/// 计算后的样式值（所有单位已解析为 pt）
/// 这是布局引擎消费的最终样式数据
#[derive(Debug, Clone)]
pub struct Style {
    // 排版（优先级从高到低的字体家族列表）
    pub font_family: Vec<String>,
    pub font_size_pt: f32,
    /// 字重，复用 `lievisual::FontWeight`。
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub color: Color,

    // 布局
    pub line_height_pt: f32,
    pub letter_spacing: f32,
    pub text_indent_em: f32,
    pub text_align: TextAlign,
    pub display: Display,
    pub white_space: WhiteSpace,
    pub list_style_type: ListStyleType,

    // Box Model
    pub margin: BoxSides,
    pub padding: BoxSides,
    pub border: BoxBorders,

    // 尺寸
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub object_fit: ObjectFit,

    // 装饰
    pub background_color: Option<Color>,

    // 分页控制
    pub page_break_before: PageBreak,
    pub page_break_after: PageBreak,

    // 表格
    pub table_border_color: Color,
    pub table_border_width_pt: f32,
    pub table_cell_padding_h_pt: f32,
    pub table_cell_padding_v_pt: f32,
    pub table_header_bg: Option<Color>,
    pub table_alt_row_bg: Option<Color>,

    // 链接
    pub link_url: Option<String>,

    // 文本修饰
    pub text_decoration: TextDecoration,

    // 列表
    pub list_indent_pt: Option<f32>,

    // 上下标基线偏移（pt，正数=上标上移，负数=下标下移）
    pub baseline_shift: f32,
}

/// 计算样式的"继承"策略。
/// 当内联元素（Strong、Emphasis 等）没有显式指定某属性时，从父元素继承。
///
/// 继承规则（遵循 CSS 标准）：
/// - 可继承属性：字体、颜色、行高、字间距、文本对齐、分页控制
/// - 不可继承属性：边距、填充、尺寸、显示、背景、表格、object-fit
impl Style {
    /// 将计算后的样式序列化为可用于 HTML `style="..."` 的 CSS 字符串。
    ///
    /// 用途：HTML 文档输出路径（方案 Y 统一路径）先把 Markdown 解析为
    /// 已套用 CSS 的 styled `ast::Node`，再据此序列化出**自包含、内联样式**
    /// 的 HTML。这样 HTML 输出与 PDF 输出共享同一棵 styled ast（样式真源一致），
    /// 不再依赖 `<style>` 块（仍保留作为兜底）。
    ///
    /// 仅输出"有值"的属性（Option 已设置、数值非零、或枚举非默认），避免冗余。
    pub fn to_inline_css(&self) -> String {
        let mut decls: Vec<String> = Vec::new();

        if !self.font_family.is_empty() {
            let fam = self
                .font_family
                .iter()
                .map(|f| {
                    if f.contains(|c: char| c.is_whitespace()) {
                        format!("\"{}\"", f)
                    } else {
                        f.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            decls.push(format!("font-family: {}", fam));
        }
        if self.font_size_pt > 0.0 {
            decls.push(format!("font-size: {:.2}pt", self.font_size_pt));
        }
        decls.push(format!(
            "font-weight: {}",
            font_weight_css(self.font_weight)
        ));
        decls.push(format!("font-style: {}", font_style_css(self.font_style)));
        if self.color.a > 0.0 {
            decls.push(format!("color: {}", self.color.to_hex()));
        }
        if self.line_height_pt > 0.0 {
            decls.push(format!("line-height: {:.2}pt", self.line_height_pt));
        }
        if self.letter_spacing != 0.0 {
            decls.push(format!("letter-spacing: {:.2}pt", self.letter_spacing));
        }
        if self.text_align != TextAlign::Left {
            decls.push(format!("text-align: {}", text_align_css(self.text_align)));
        }
        if self.white_space != WhiteSpace::Normal {
            let ws = match self.white_space {
                WhiteSpace::Pre => "pre",
                WhiteSpace::NoWrap => "nowrap",
                WhiteSpace::Normal => "normal",
            };
            decls.push(format!("white-space: {}", ws));
        }
        if self.text_decoration != TextDecoration::None {
            decls.push(format!(
                "text-decoration: {}",
                text_decoration_css(self.text_decoration)
            ));
        }
        Self::push_box(&mut decls, "margin", &self.margin);
        Self::push_box(&mut decls, "padding", &self.padding);
        if let Some(bg) = self.background_color {
            decls.push(format!("background-color: {}", bg.to_hex()));
        }
        Self::push_border(&mut decls, &self.border);

        decls.join("; ")
    }

    fn push_box(decls: &mut Vec<String>, name: &str, b: &BoxSides) {
        if *b == BoxSides::ZERO {
            return;
        }
        decls.push(format!(
            "{}: {:.2}pt {:.2}pt {:.2}pt {:.2}pt",
            name, b.top, b.right, b.bottom, b.left
        ));
    }

    fn push_border(decls: &mut Vec<String>, border: &BoxBorders) {
        for (side, prop) in [
            (&border.top, "border-top"),
            (&border.right, "border-right"),
            (&border.bottom, "border-bottom"),
            (&border.left, "border-left"),
        ] {
            if side.is_visible() {
                let style = match side.style {
                    BorderStyle::Dashed => "dashed",
                    BorderStyle::Dotted => "dotted",
                    _ => "solid",
                };
                decls.push(format!(
                    "{}: {:.2}pt {} {}",
                    prop,
                    side.width,
                    style,
                    side.color.to_hex()
                ));
            }
        }
    }

    /// 从父样式继承创建一个子样式，只覆盖特定属性
    pub fn inherit_from(parent: &Style) -> Self {
        Self {
            font_family: parent.font_family.clone(),
            font_size_pt: parent.font_size_pt,
            font_weight: parent.font_weight,
            font_style: parent.font_style,
            color: parent.color,
            line_height_pt: parent.line_height_pt,
            letter_spacing: parent.letter_spacing,
            text_indent_em: parent.text_indent_em,
            text_align: parent.text_align,
            white_space: parent.white_space,
            list_style_type: parent.list_style_type,
            display: Display::Inline,
            margin: BoxSides::ZERO,
            padding: BoxSides::ZERO,
            border: BoxBorders::NONE,
            width: None,
            height: None,
            // 与 `Style::default()` 保持一致（图片默认不拉伸、保持比例）。
            // object-fit 为非继承属性，这里固定默认值而非继承父值。
            object_fit: ObjectFit::Contain,
            background_color: None,
            page_break_before: parent.page_break_before,
            page_break_after: parent.page_break_after,
            table_border_color: parent.table_border_color,
            table_border_width_pt: parent.table_border_width_pt,
            table_cell_padding_h_pt: parent.table_cell_padding_h_pt,
            table_cell_padding_v_pt: parent.table_cell_padding_v_pt,
            table_header_bg: parent.table_header_bg,
            table_alt_row_bg: parent.table_alt_row_bg,
            link_url: parent.link_url.clone(),
            text_decoration: parent.text_decoration,
            list_indent_pt: None,
            baseline_shift: parent.baseline_shift,
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
            color: Color::rgb(0, 0, 0),
            line_height_pt: 15.75,
            letter_spacing: 0.0,
            text_indent_em: 0.0,
            text_align: TextAlign::Left,
            white_space: WhiteSpace::Normal,
            list_style_type: ListStyleType::Disc, // 默认 disc，列表元素通过 CSS 覆盖
            display: Display::Block,
            margin: BoxSides::new(0.0, 0.0, 12.0, 0.0),
            padding: BoxSides::ZERO,
            border: BoxBorders::NONE,
            width: None,
            height: None,
            object_fit: ObjectFit::Contain,
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
            baseline_shift: 0.0,
        }
    }
}

/// 页面配置（从 CSS @page 规则提取）
#[derive(Debug, Clone)]
pub struct PageConfig {
    pub margin_top: Option<f32>,
    pub margin_bottom: Option<f32>,
    pub margin_left: Option<f32>,
    pub margin_right: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,

    // ─── 无限高度模式 ──────────────────────────────────────
    /// 仅限定宽度，高度无限（不分页，所有内容连续排列在一个页面上）
    pub height_unlimited: Option<bool>,

    // ─── 页眉页脚 ──────────────────────────────────────────
    /// 页眉文本（支持 {page} 和 {total} 模板变量）
    pub header: Option<String>,
    /// 页脚文本（支持 {page} 和 {total} 模板变量），默认显示页码
    pub footer: Option<String>,
    /// 页眉字体大小（pt）
    pub header_font_size: Option<f32>,
    /// 页脚字体大小（pt）
    pub footer_font_size: Option<f32>,
}

impl Default for PageConfig {
    fn default() -> Self {
        Self {
            margin_top: None,
            margin_bottom: None,
            margin_left: None,
            margin_right: None,
            width: None,
            height: None,
            height_unlimited: None,
            header: None,
            footer: Some("- {page} -".to_string()),
            header_font_size: None,
            footer_font_size: None,
        }
    }
}
