//! CSS 样式表解析与样式解析模块
//!
//! 实现 CSS 核心子集：标签选择器、类选择器、后代组合器、通用选择器。
//! 支持字体、颜色、字号、边距、分页等核心属性。
//!
//! 设计目标：
//! - 轻量级：只实现必要的 CSS 子集，不引入完整 CSS 引擎
//! - 与现有 Style 结构体无缝集成
//! - 支持内置样式 + 用户 CSS 覆盖

use super::style::*;
use crate::visual::Color;

// ─── CSS 选择器 ──────────────────────────────────────────────

/// CSS 选择器类型
#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    /// 通用选择器 `*`
    Universal,
    /// 标签选择器 `h1`, `p`, `table`
    Tag(String),
    /// 类选择器 `.title`, `.author`
    Class(String),
    /// 后代组合器 `article p`, `blockquote p`
    Descendant(Vec<SimpleSelector>),
}

/// 简单选择器（选择器序列中的一环）
#[derive(Debug, Clone, PartialEq)]
pub enum SimpleSelector {
    Universal,
    Tag(String),
    Class(String),
}

// ─── CSS at-rule ──────────────────────────────────────────────

/// CSS at-rule（如 @page）
#[derive(Debug, Clone)]
pub enum AtRule {
    /// @page 规则，包含声明块
    Page { declarations: Vec<Declaration> },
}

// ─── CSS 规则 ────────────────────────────────────────────────

/// 解析后的 CSS 声明
#[derive(Debug, Clone)]
pub struct Declaration {
    pub property: String,
    pub value: String,
}

/// 解析后的 CSS 规则
#[derive(Debug, Clone)]
pub struct CssRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

/// 样式表
#[derive(Debug, Clone)]
pub struct Stylesheet {
    pub rules: Vec<CssRule>,
    pub at_rules: Vec<AtRule>,
}

impl Stylesheet {
    pub fn new() -> Self {
        Self { rules: Vec::new(), at_rules: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty() && self.at_rules.is_empty()
    }
}

impl Default for Stylesheet {
    fn default() -> Self {
        Self::new()
    }
}

// ─── CSS 解析器 ─────────────────────────────────────────────

/// 解析 CSS 文本为样式表
pub fn parse_css(css: &str) -> Result<Stylesheet, String> {
    let css = strip_comments(css);
    let mut rules = Vec::new();
    let mut at_rules = Vec::new();
    let mut pos = 0;
    let bytes = css.as_bytes();
    let len = bytes.len();

    while pos < len {
        // 跳过空白
        while pos < len && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= len {
            break;
        }

        // 解析选择器列表（直到遇到 `{`）
        let selector_start = pos;
        while pos < len && bytes[pos] != b'{' {
            pos += 1;
        }
        if pos >= len {
            break;
        }
        let selector_str = &css[selector_start..pos].trim();
        pos += 1; // 跳过 `{`

        // 解析声明（直到遇到 `}`）
        let decl_start = pos;
        while pos < len && bytes[pos] != b'}' {
            pos += 1;
        }
        if pos >= len {
            break;
        }
        let decl_str = &css[decl_start..pos].trim();
        pos += 1; // 跳过 `}`

        if selector_str.is_empty() {
            continue;
        }

        // 检查 at-rule（如 @page）
        if selector_str.starts_with('@') {
            if selector_str.eq_ignore_ascii_case("@page") {
                let declarations = parse_declarations(decl_str);
                if !declarations.is_empty() {
                    at_rules.push(AtRule::Page { declarations });
                }
            }
            // 其他 at-rule 忽略
            continue;
        }

        // 解析选择器（支持逗号分隔的多个选择器）
        let selectors: Vec<Selector> = selector_str
            .split(',')
            .filter_map(|s| parse_selector(s.trim()))
            .collect();

        if selectors.is_empty() {
            continue;
        }

        // 解析声明
        let declarations = parse_declarations(decl_str);

        if !declarations.is_empty() {
            rules.push(CssRule {
                selectors,
                declarations,
            });
        }
    }

    Ok(Stylesheet { rules, at_rules })
}

/// 去除 CSS 注释
fn strip_comments(css: &str) -> String {
    let mut result = String::with_capacity(css.len());
    let bytes = css.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            // 查找注释结束
            let mut j = i + 2;
            while j + 1 < len && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
                j += 1;
            }
            i = j + 2;
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    result
}

/// 解析单个选择器
fn parse_selector(s: &str) -> Option<Selector> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // 检查是否有后代组合器（空格分隔）
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() > 1 {
        let mut simple_selectors = Vec::new();
        for part in parts {
            match parse_simple_selector(part) {
                Some(ss) => simple_selectors.push(ss),
                None => return None,
            }
        }
        return Some(Selector::Descendant(simple_selectors));
    }

    // 单个简单选择器
    let s = s.trim();
    if s == "*" {
        return Some(Selector::Universal);
    }
    if s.starts_with('.') {
        return Some(Selector::Class(s[1..].to_string()));
    }
    // 标签选择器，验证是合法标识符
    if s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Some(Selector::Tag(s.to_string()));
    }

    None
}

/// 解析简单选择器（如 `h1`, `.class`, `div.class`）
fn parse_simple_selector(s: &str) -> Option<SimpleSelector> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    if s == "*" {
        return Some(SimpleSelector::Universal);
    }

    // 处理 `tag.class` 形式 - 目前只取 tag
    // 未来可以扩展为同时匹配 tag 和 class
    if let Some(dot_pos) = s.find('.') {
        let tag = &s[..dot_pos];
        if tag.is_empty() || tag.chars().all(|c| c.is_alphanumeric() || c == '-') {
            return Some(SimpleSelector::Tag(tag.to_string()));
        }
        return None;
    }

    if s.starts_with('.') {
        return Some(SimpleSelector::Class(s[1..].to_string()));
    }

    if s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Some(SimpleSelector::Tag(s.to_string()));
    }

    None
}

/// 解析声明块
fn parse_declarations(s: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    let s = s.trim();

    if s.is_empty() {
        return declarations;
    }

    // 按分号分割声明
    for decl in s.split(';') {
        let decl = decl.trim();
        if decl.is_empty() {
            continue;
        }

        // 找冒号分割属性名和值
        if let Some(colon_pos) = decl.find(':') {
            let property = decl[..colon_pos].trim().to_string();
            let value = decl[colon_pos + 1..].trim().to_string();

            if !property.is_empty() && !value.is_empty() {
                declarations.push(Declaration { property, value });
            }
        }
    }

    declarations
}

// ─── 特异性计算 ─────────────────────────────────────────────

/// 计算选择器的特异性
///
/// CSS 标准：ID=100, 类=10, 标签=1
/// 返回值：特异性值（越高优先级越高）
fn selector_specificity(selector: &Selector) -> u32 {
    match selector {
        Selector::Universal => 0,
        Selector::Tag(_) => 1,
        Selector::Class(_) => 10,
        Selector::Descendant(parts) => {
            let mut spec = 0u32;
            for part in parts {
                spec += match part {
                    SimpleSelector::Universal => 0,
                    SimpleSelector::Tag(_) => 1,
                    SimpleSelector::Class(_) => 10,
                };
            }
            spec
        }
    }
}

/// 计算规则的最大特异性（取所有选择器中的最大值）
fn rule_specificity(rule: &CssRule) -> u32 {
    rule.selectors
        .iter()
        .map(selector_specificity)
        .max()
        .unwrap_or(0)
}

// ─── 选择器匹配 ─────────────────────────────────────────────

/// 匹配单个选择器到节点
///
/// # 参数
/// - `selector`: CSS 选择器
/// - `tag`: 节点的标签名
/// - `classes`: 节点的类名列表
/// - `ancestor_tags`: 祖先节点的标签名列表（从近到远）
///
/// # 返回值
/// - `Some(specificity)`: 匹配成功，返回特异性值
/// - `None`: 匹配失败
pub fn match_selector(
    selector: &Selector,
    tag: &str,
    classes: &[String],
    ancestor_tags: &[String],
) -> Option<u32> {
    match selector {
        Selector::Universal => Some(0),
        Selector::Tag(s) => {
            if s == tag {
                Some(1)
            } else {
                None
            }
        }
        Selector::Class(s) => {
            if classes.iter().any(|c| c == s) {
                Some(10)
            } else {
                None
            }
        }
        Selector::Descendant(parts) => {
            // 最后一个简单选择器匹配当前节点
            let last = parts.last()?;
            if !match_simple_selector(last, tag, classes) {
                return None;
            }

            // 前面的部分必须在祖先链中匹配（从近到远）
            if parts.len() == 1 {
                return Some(selector_specificity(selector));
            }

            let ancestors_to_match = &parts[..parts.len() - 1];
            let mut ancestor_idx = ancestor_tags.len();

            for part in ancestors_to_match.iter().rev() {
                // 从最近的祖先开始找
                let found = loop {
                    if ancestor_idx == 0 {
                        break false;
                    }
                    ancestor_idx -= 1;
                    if match_simple_selector_to_tag(part, &ancestor_tags[ancestor_idx]) {
                        break true;
                    }
                    // 对于后代组合器，继续向上搜索
                };
                if !found {
                    return None;
                }
            }

            Some(selector_specificity(selector))
        }
    }
}

/// 匹配简单选择器到节点
fn match_simple_selector(sel: &SimpleSelector, tag: &str, classes: &[String]) -> bool {
    match sel {
        SimpleSelector::Universal => true,
        SimpleSelector::Tag(s) => s == tag,
        SimpleSelector::Class(s) => classes.iter().any(|c| c == s),
    }
}

/// 匹配简单选择器到祖先标签（只匹配标签名）
fn match_simple_selector_to_tag(sel: &SimpleSelector, ancestor_tag: &str) -> bool {
    match sel {
        SimpleSelector::Universal => true,
        SimpleSelector::Tag(s) => s == ancestor_tag,
        SimpleSelector::Class(_) => {
            // 类选择器在祖先匹配中无法确认，保守地返回 true
            // 因为我们在树构建时没有完整的类名信息用于祖先匹配
            // 这是一个简化的做法
            true
        }
    }
}

// ─── 标签名映射 ─────────────────────────────────────────────

/// 将 MDAST 节点类型映射为 CSS 标签名
pub fn node_tag_name(kind: &super::NodeKind) -> &'static str {
    match kind {
        super::NodeKind::Document { .. } => "body",
        super::NodeKind::Heading { level, .. } => {
            match level {
                1 => "h1",
                2 => "h2",
                3 => "h3",
                4 => "h4",
                5 => "h5",
                6 => "h6",
                _ => "h1",
            }
        }
        super::NodeKind::Paragraph { .. } => "p",
        super::NodeKind::List { ordered, .. } => {
            if *ordered { "ol" } else { "ul" }
        }
        super::NodeKind::ListItem { .. } | super::NodeKind::TaskListItem { .. } => "li",
        super::NodeKind::Image { .. } => "img",
        super::NodeKind::CodeBlock { .. } => "pre",
        super::NodeKind::Blockquote { .. } => "blockquote",
        super::NodeKind::ThematicBreak => "hr",
        super::NodeKind::Table { .. } => "table",
        super::NodeKind::TableRow { .. } => "tr",
        super::NodeKind::Text { .. } => "span",
        super::NodeKind::Strong { .. } => "strong",
        super::NodeKind::Emphasis { .. } => "em",
        super::NodeKind::InlineCode { .. } => "code",
        super::NodeKind::Link { .. } => "a",
        super::NodeKind::Delete { .. } => "del",
    }
}

// ─── 样式解析 ───────────────────────────────────────────────

/// 将 CSS 声明解析并应用到 Style 上
fn apply_declaration(style: &mut Style, property: &str, value: &str) {
    match property {
        "font-family" => {
            // 解析字体家族列表（逗号分隔，支持引号）
            let families = parse_font_family(value);
            if !families.is_empty() {
                style.font_family = families;
            }
        }
        "font-size" => {
            if let Some(pt) = parse_length(value, style.font_size_pt) {
                style.font_size_pt = pt;
            }
        }
        "font-weight" => {
            style.font_weight = parse_font_weight(value);
        }
        "font-style" => {
            style.font_style = parse_font_style(value);
        }
        "color" => {
            if let Some(c) = parse_color(value) {
                style.color = c;
            }
        }
        "line-height" => {
            if let Some(lh) = parse_line_height(value, style.font_size_pt) {
                style.line_height_pt = lh;
            }
        }
        "text-align" => {
            style.text_align = parse_text_align(value);
        }
        "margin-top" => {
            if let Some(v) = parse_length(value, style.font_size_pt) {
                style.margin_top_pt = v;
            }
        }
        "margin-bottom" => {
            if let Some(v) = parse_length(value, style.font_size_pt) {
                style.margin_bottom_pt = v;
            }
        }
        "display" => {
            style.display = parse_display(value);
        }
        "width" => {
            if value == "auto" {
                style.width = None;
            } else if let Some(v) = parse_length(value, style.font_size_pt) {
                style.width = Some(v);
            }
        }
        "background-color" => {
            if let Some(c) = parse_color(value) {
                style.background_color = Some(c);
            }
        }
        "page-break-before" => {
            style.page_break_before = parse_page_break(value);
        }
        "page-break-after" => {
            style.page_break_after = parse_page_break(value);
        }
        "letter-spacing" => {
            if let Some(v) = parse_length(value, style.font_size_pt) {
                style.letter_spacing = v;
            }
        }
        "padding-top" => {
            if let Some(v) = parse_length(value, style.font_size_pt) {
                style.padding_top_pt = v;
            }
        }
        "padding-bottom" => {
            if let Some(v) = parse_length(value, style.font_size_pt) {
                style.padding_bottom_pt = v;
            }
        }
        "margin-left" => {
            if let Some(v) = parse_length(value, style.font_size_pt) {
                style.margin_left_pt = v;
            }
        }
        "margin-right" => {
            if let Some(v) = parse_length(value, style.font_size_pt) {
                style.margin_right_pt = v;
            }
        }
        "padding-left" => {
            if let Some(v) = parse_length(value, style.font_size_pt) {
                style.padding_left_pt = v;
            }
        }
        "padding-right" => {
            if let Some(v) = parse_length(value, style.font_size_pt) {
                style.padding_right_pt = v;
            }
        }
        "height" => {
            if value == "auto" {
                style.height = None;
            } else if let Some(v) = parse_length(value, style.font_size_pt) {
                style.height = Some(v);
            }
        }
        "object-fit" => {
            style.object_fit = parse_object_fit(value);
        }
        "border-collapse" => {
            // 表格属性 - 简单跳过
        }
        "table-header-background" | "table-header-bg" => {
            if let Some(c) = parse_color(value) {
                style.table_header_bg = Some(c);
            }
        }
        "table-alt-row-background" | "table-alt-row-bg" => {
            if let Some(c) = parse_color(value) {
                style.table_alt_row_bg = Some(c);
            }
        }
        "table-border-color" => {
            if let Some(c) = parse_color(value) {
                style.table_border_color = c;
            }
        }
        "table-border-width" => {
            if let Some(v) = parse_length(value, style.font_size_pt) {
                style.table_border_width_pt = v;
            }
        }
        "table-cell-padding-horizontal" | "table-cell-padding-h" => {
            if let Some(v) = parse_length(value, style.font_size_pt) {
                style.table_cell_padding_h_pt = v;
            }
        }
        "table-cell-padding-vertical" | "table-cell-padding-v" => {
            if let Some(v) = parse_length(value, style.font_size_pt) {
                style.table_cell_padding_v_pt = v;
            }
        }
        "list-indent" => {
            if let Some(v) = parse_length(value, style.font_size_pt) {
                style.list_indent_pt = Some(v);
            }
        }
        _ => {
            // 未知属性，忽略
        }
    }
}

// ─── CSS 值解析器 ───────────────────────────────────────────

/// 解析字体家族列表
fn parse_font_family(value: &str) -> Vec<String> {
    let mut families = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = ' ';
    let chars: Vec<char> = value.chars().collect();

    for &c in &chars {
        if in_quote {
            if c == quote_char {
                in_quote = false;
                if !current.is_empty() {
                    families.push(current.trim().to_string());
                    current = String::new();
                }
            } else {
                current.push(c);
            }
        } else if c == '"' || c == '\'' {
            in_quote = true;
            quote_char = c;
        } else if c == ',' {
            if !current.is_empty() {
                families.push(current.trim().to_string());
                current = String::new();
            }
        } else if !c.is_whitespace() || !current.is_empty() {
            current.push(c);
        }
    }

    if !current.is_empty() {
        families.push(current.trim().to_string());
    }

    families
}

/// 解析长度值（支持 pt, px, em, %, mm, cm, in）
pub(crate) fn parse_length(value: &str, parent_font_size: f32) -> Option<f32> {
    let value = value.trim();

    // 处理 `0`（无单位）
    if value == "0" {
        return Some(0.0);
    }

    if let Some(v) = value.strip_suffix("pt") {
        v.trim().parse::<f32>().ok()
    } else if let Some(v) = value.strip_suffix("px") {
        v.trim().parse::<f32>().ok()
    } else if let Some(v) = value.strip_suffix("em") {
        let em = v.trim().parse::<f32>().ok()?;
        Some(em * parent_font_size)
    } else if let Some(v) = value.strip_suffix("mm") {
        let mm = v.trim().parse::<f32>().ok()?;
        Some(mm * 72.0 / 25.4)
    } else if let Some(v) = value.strip_suffix("cm") {
        let cm = v.trim().parse::<f32>().ok()?;
        Some(cm * 72.0 / 2.54)
    } else if let Some(v) = value.strip_suffix("in") {
        let inches = v.trim().parse::<f32>().ok()?;
        Some(inches * 72.0)
    } else if let Some(v) = value.strip_suffix('%') {
        let pct = v.trim().parse::<f32>().ok()?;
        Some(pct * parent_font_size / 100.0)
    } else {
        // 尝试直接解析为数字（视为 pt）
        value.parse::<f32>().ok()
    }
}

/// 解析行高
fn parse_line_height(value: &str, font_size: f32) -> Option<f32> {
    let value = value.trim();

    // 无单位数字（相对于 font-size 的倍数）
    if let Ok(num) = value.parse::<f32>() {
        return Some(num * font_size);
    }

    // 带单位的值
    parse_length(value, font_size)
}

/// 解析字重
fn parse_font_weight(value: &str) -> FontWeight {
    match value.trim() {
        "bold" | "700" | "800" | "900" => FontWeight::Bold,
        _ => FontWeight::Normal,
    }
}

/// 解析字体样式
fn parse_font_style(value: &str) -> FontStyle {
    match value.trim() {
        "italic" | "oblique" => FontStyle::Italic,
        _ => FontStyle::Normal,
    }
}

/// 解析颜色
fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim().to_lowercase();

    // 命名颜色
    let named = match value.as_str() {
        "black" => Color::new(0, 0, 0),
        "white" => Color::new(255, 255, 255),
        "red" => Color::new(255, 0, 0),
        "green" | "lime" => Color::new(0, 128, 0),
        "blue" => Color::new(0, 0, 255),
        "yellow" => Color::new(255, 255, 0),
        "gray" | "grey" => Color::new(128, 128, 128),
        "silver" => Color::new(192, 192, 192),
        "maroon" => Color::new(128, 0, 0),
        "purple" => Color::new(128, 0, 128),
        "fuchsia" | "magenta" => Color::new(255, 0, 255),
        "teal" => Color::new(0, 128, 128),
        "aqua" | "cyan" => Color::new(0, 255, 255),
        "navy" => Color::new(0, 0, 128),
        "orange" => Color::new(255, 165, 0),
        "transparent" => Color::new(0, 0, 0),
        _ => return parse_hex_color(&value).or_else(|| parse_rgb_color(&value)),
    };

    Some(named)
}

/// 解析十六进制颜色 (#RGB, #RRGGBB)
fn parse_hex_color(value: &str) -> Option<Color> {
    let value = value.trim_start_matches('#');
    if value.len() == 3 {
        let r = u8::from_str_radix(&value[0..1], 16).ok()? * 17;
        let g = u8::from_str_radix(&value[1..2], 16).ok()? * 17;
        let b = u8::from_str_radix(&value[2..3], 16).ok()? * 17;
        Some(Color::new(r, g, b))
    } else if value.len() == 6 {
        let r = u8::from_str_radix(&value[0..2], 16).ok()?;
        let g = u8::from_str_radix(&value[2..4], 16).ok()?;
        let b = u8::from_str_radix(&value[4..6], 16).ok()?;
        Some(Color::new(r, g, b))
    } else {
        None
    }
}

/// 解析 rgb() 颜色
fn parse_rgb_color(value: &str) -> Option<Color> {
    let value = value.trim();
    if !value.starts_with("rgb(") || !value.ends_with(')') {
        return None;
    }
    let inner = value[4..value.len() - 1].trim();
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 3 {
        return None;
    }
    let r = parts[0].trim().parse::<u8>().ok()?;
    let g = parts[1].trim().parse::<u8>().ok()?;
    let b = parts[2].trim().parse::<u8>().ok()?;
    Some(Color::new(r, g, b))
}

/// 解析文本对齐
fn parse_text_align(value: &str) -> TextAlign {
    match value.trim() {
        "center" => TextAlign::Center,
        "right" => TextAlign::Right,
        "justify" => TextAlign::Justify,
        _ => TextAlign::Left,
    }
}

/// 解析显示类型
fn parse_display(value: &str) -> Display {
    match value.trim() {
        "inline" => Display::Inline,
        "inline-block" => Display::InlineBlock,
        "none" => Display::None,
        _ => Display::Block,
    }
}

/// 解析分页控制
fn parse_page_break(value: &str) -> PageBreak {
    match value.trim() {
        "always" | "page" => PageBreak::Always,
        "avoid" => PageBreak::Avoid,
        "left" => PageBreak::Left,
        "right" => PageBreak::Right,
        _ => PageBreak::Auto,
    }
}

/// 解析 object-fit
fn parse_object_fit(value: &str) -> ObjectFit {
    match value.trim() {
        "cover" => ObjectFit::Cover,
        "fill" => ObjectFit::Fill,
        "none" => ObjectFit::None,
        _ => ObjectFit::Contain,
    }
}

// ─── 页面配置提取 ─────────────────────────────────────────────

/// 将页面相关的 CSS 声明应用到 PageConfig
fn apply_page_declaration(config: &mut PageConfig, property: &str, value: &str) {
    match property {
        "margin-top" => {
            if let Some(v) = parse_length(value, 10.5) {
                config.margin_top = Some(v);
            }
        }
        "margin-bottom" => {
            if let Some(v) = parse_length(value, 10.5) {
                config.margin_bottom = Some(v);
            }
        }
        "margin-left" => {
            if let Some(v) = parse_length(value, 10.5) {
                config.margin_left = Some(v);
            }
        }
        "margin-right" => {
            if let Some(v) = parse_length(value, 10.5) {
                config.margin_right = Some(v);
            }
        }
        "margin" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            let vals: Vec<f32> = parts.iter()
                .filter_map(|s| parse_length(s, 10.5))
                .collect();
            match vals.len() {
                1 => {
                    config.margin_top = Some(vals[0]);
                    config.margin_bottom = Some(vals[0]);
                    config.margin_left = Some(vals[0]);
                    config.margin_right = Some(vals[0]);
                }
                2 => {
                    config.margin_top = Some(vals[0]);
                    config.margin_bottom = Some(vals[0]);
                    config.margin_left = Some(vals[1]);
                    config.margin_right = Some(vals[1]);
                }
                3 => {
                    config.margin_top = Some(vals[0]);
                    config.margin_left = Some(vals[1]);
                    config.margin_right = Some(vals[1]);
                    config.margin_bottom = Some(vals[2]);
                }
                _ => {
                    if vals.len() >= 4 {
                        config.margin_top = Some(vals[0]);
                        config.margin_right = Some(vals[1]);
                        config.margin_bottom = Some(vals[2]);
                        config.margin_left = Some(vals[3]);
                    }
                }
            }
        }
        "size" => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            let mut width: Option<f32> = None;
            let mut height: Option<f32> = None;
            let mut is_landscape = false;
            let mut is_portrait = false;

            for part in &parts {
                let lower = part.to_ascii_lowercase();
                match lower.as_str() {
                    "landscape" => {
                        is_landscape = true;
                    }
                    "portrait" => {
                        is_portrait = true;
                    }
                    "a3" => {
                        width = Some(841.890);
                        height = Some(1190.551);
                    }
                    "a4" => {
                        width = Some(595.276);
                        height = Some(841.890);
                    }
                    "a5" => {
                        width = Some(419.528);
                        height = Some(595.276);
                    }
                    "a6" => {
                        width = Some(297.638);
                        height = Some(419.528);
                    }
                    "letter" => {
                        width = Some(612.0);
                        height = Some(792.0);
                    }
                    "legal" => {
                        width = Some(612.0);
                        height = Some(1008.0);
                    }
                    "tabloid" | "ledger" => {
                        width = Some(792.0);
                        height = Some(1224.0);
                    }
                    _ => {
                        if let Some(v) = parse_length(part, 10.5) {
                            if width.is_none() {
                                width = Some(v);
                            } else if height.is_none() {
                                height = Some(v);
                            }
                        }
                    }
                }
            }

            if is_landscape {
                if let (Some(w), Some(h)) = (width, height) {
                    config.width = Some(w.max(h));
                    config.height = Some(w.min(h));
                } else {
                    config.width = Some(841.890);
                    config.height = Some(595.276);
                }
            } else if is_portrait {
                if let (Some(w), Some(h)) = (width, height) {
                    config.width = Some(w.min(h));
                    config.height = Some(w.max(h));
                }
            } else {
                if let Some(w) = width {
                    config.width = Some(w);
                }
                if let Some(h) = height {
                    config.height = Some(h);
                }
            }
        }
        "header" => {
            let trimmed = value.trim();
            if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
                config.header = Some(trimmed[1..trimmed.len()-1].to_string());
            } else {
                config.header = Some(trimmed.to_string());
            }
        }
        "footer" => {
            let trimmed = value.trim();
            if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
                config.footer = Some(trimmed[1..trimmed.len()-1].to_string());
            } else {
                config.footer = Some(trimmed.to_string());
            }
        }
        "header-font-size" => {
            config.header_font_size = parse_length(value, 10.5);
        }
        "footer-font-size" => {
            config.footer_font_size = parse_length(value, 10.5);
        }
        _ => {}
    }
}

impl Stylesheet {
    /// 从样式表中提取页面配置（合并所有 @page 规则）
    pub fn extract_page_config(&self) -> PageConfig {
        let mut config = PageConfig::default();
        for at_rule in &self.at_rules {
            let AtRule::Page { declarations } = at_rule;
            for decl in declarations {
                apply_page_declaration(&mut config, &decl.property, &decl.value);
            }
        }
        config
    }
}

// ─── 样式解析入口 ───────────────────────────────────────────

/// 样式解析上下文
///
/// 包含解析后的样式表和可选的外部外部样式表。
/// 用户样式表优先级高于内置样式表。
///
/// 支持严格模式（strict mode）：
/// - 严格模式：CSS 解析失败时返回错误
/// - 非严格模式（默认）：CSS 解析失败时静默忽略，继续使用已有样式
pub struct StyleResolver {
    /// 内置样式表
    builtin: Stylesheet,
    /// 用户提供的覆盖样式表
    user: Stylesheet,
    /// 严格模式开关
    /// - true: CSS 解析错误会传播
    /// - false（默认）: CSS 解析错误被静默忽略
    strict: bool,
    /// 从 @page 规则提取的页面配置
    page_config: PageConfig,
}

impl StyleResolver {
    /// 使用内置样式表创建解析器
    ///
    /// 内置 CSS 始终以严格模式解析（如果内置 CSS 失效，说明有 bug）。
    /// 用户 CSS 和内联 CSS 的严格/非严格行为由 `with_strict_mode` 控制。
    pub fn new(builtin_css: &str) -> Result<Self, String> {
        let builtin = parse_css(builtin_css)?;
        let page_config = builtin.extract_page_config();
        Ok(Self {
            builtin,
            user: Stylesheet::new(),
            strict: false,
            page_config,
        })
    }

    /// 设置严格模式
    ///
    /// - `true`: 用户 CSS 解析失败时返回错误
    /// - `false`（默认）: 用户 CSS 解析失败时静默忽略
    pub fn with_strict_mode(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// 添加用户 CSS 覆盖
    ///
    /// 在非严格模式下，如果 CSS 解析失败会静默忽略并返回自身。
    pub fn with_user_css(self, user_css: &str) -> Result<Self, String> {
        if user_css.trim().is_empty() {
            return Ok(self);
        }
        match parse_css(user_css) {
            Ok(user) => {
                // 合并页面配置：用户覆盖内置
                let mut page_config = self.builtin.extract_page_config();
                let user_page = user.extract_page_config();
                if let Some(v) = user_page.margin_top {
                    page_config.margin_top = Some(v);
                }
                if let Some(v) = user_page.margin_bottom {
                    page_config.margin_bottom = Some(v);
                }
                if let Some(v) = user_page.margin_left {
                    page_config.margin_left = Some(v);
                }
                if let Some(v) = user_page.margin_right {
                    page_config.margin_right = Some(v);
                }
                if let Some(v) = user_page.width {
                    page_config.width = Some(v);
                }
                if let Some(v) = user_page.height {
                    page_config.height = Some(v);
                }
                if let Some(v) = user_page.header {
                    page_config.header = Some(v);
                }
                if let Some(v) = user_page.footer {
                    page_config.footer = Some(v);
                }
                if let Some(v) = user_page.header_font_size {
                    page_config.header_font_size = Some(v);
                }
                if let Some(v) = user_page.footer_font_size {
                    page_config.footer_font_size = Some(v);
                }
                Ok(Self {
                    user,
                    page_config,
                    ..self
                })
            }
            Err(e) => {
                if self.strict {
                    Err(e)
                } else {
                    Ok(self)
                }
            }
        }
    }

    /// 合并所有规则（用户规则在最后，优先级更高）
    fn all_rules(&self) -> Vec<&CssRule> {
        let mut rules: Vec<&CssRule> = self.builtin.rules.iter().collect();
        rules.extend(self.user.rules.iter());
        rules
    }

    /// 获取从 @page 规则提取的页面配置
    pub fn page_config(&self) -> &PageConfig {
        &self.page_config
    }

    /// 解析节点的最终样式
    ///
    /// # 参数
    /// - `tag`: 节点的 CSS 标签名
    /// - `classes`: 节点的类名列表
    /// - `ancestor_tags`: 祖先标签名列表（从根到父节点）
    /// - `parent_style`: 父节点的计算后样式（用于继承）
    pub fn resolve_style(
        &self,
        tag: &str,
        classes: &[String],
        ancestor_tags: &[String],
        parent_style: &Style,
    ) -> Style {
        let mut style = Style::inherit_from(parent_style);

        // 收集所有匹配的规则
        let mut matches: Vec<(&CssRule, u32)> = Vec::new();
        for rule in &self.all_rules() {
            for selector in &rule.selectors {
                if let Some(specificity) = match_selector(selector, tag, classes, ancestor_tags) {
                    matches.push((rule, specificity));
                    break; // 一条规则只需要匹配一个选择器
                }
            }
        }

        // 按特异性排序（CSS 标准：低优先级先应用，然后被高优先级覆盖）
        matches.sort_by_key(|(_, spec)| *spec);

        // 依次应用声明
        for (rule, _) in &matches {
            for decl in &rule.declarations {
                apply_declaration(&mut style, &decl.property, &decl.value);
            }
        }

        style
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_rules() {
        let css = r#"
            h1 { font-size: 24pt; font-weight: bold; }
            p { font-size: 10.5pt; line-height: 1.5; }
        "#;
        let stylesheet = parse_css(css).unwrap();
        assert_eq!(stylesheet.rules.len(), 2);
        assert_eq!(stylesheet.rules[0].declarations.len(), 2);
        assert_eq!(stylesheet.rules[1].declarations.len(), 2);
    }

    #[test]
    fn test_parse_descendant_selector() {
        let css = r#"
            article p { color: #333; }
            blockquote p { font-style: italic; }
        "#;
        let stylesheet = parse_css(css).unwrap();
        assert_eq!(stylesheet.rules.len(), 2);
        match &stylesheet.rules[0].selectors[0] {
            Selector::Descendant(parts) => assert_eq!(parts.len(), 2),
            _ => panic!("Expected Descendant selector"),
        }
    }

    #[test]
    fn test_parse_comments() {
        let css = r#"
            /* This is a comment */
            h1 { color: red; /* inline comment */ }
        "#;
        let stylesheet = parse_css(css).unwrap();
        assert_eq!(stylesheet.rules.len(), 1);
    }

    #[test]
    fn test_match_tag_selector() {
        let css = "h1 { font-size: 24pt; }";
        let stylesheet = parse_css(css).unwrap();
        let tag = "h1";
        let result = match_selector(
            &stylesheet.rules[0].selectors[0],
            tag,
            &[],
            &[],
        );
        assert!(result.is_some());
    }

    #[test]
    fn test_match_universal_selector() {
        let sel = Selector::Universal;
        let result = match_selector(&sel, "p", &[], &[]);
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_font_family() {
        let result = parse_font_family("serif");
        assert_eq!(result, vec!["serif"]);

        let result = parse_font_family("'Times New Roman', serif");
        assert_eq!(result, vec!["Times New Roman", "serif"]);
    }

    #[test]
    fn test_parse_color_hex() {
        let c = parse_color("#ff0000").unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);

        let c = parse_color("#f00").unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn test_parse_color_named() {
        let c = parse_color("red").unwrap();
        assert_eq!(c.r, 255);

        let c = parse_color("black").unwrap();
        assert_eq!(c.r, 0);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn test_parse_length() {
        assert_eq!(parse_length("12pt", 10.0), Some(12.0));
        assert_eq!(parse_length("1.5em", 10.0), Some(15.0));
        assert_eq!(parse_length("150%", 10.0), Some(15.0));
        assert_eq!(parse_length("0", 10.0), Some(0.0));
    }

    #[test]
    fn test_style_resolver() {
        let builtin = r#"
            body { font-family: serif; font-size: 10.5pt; color: #000; line-height: 1.5; }
            h1 { font-size: 24pt; font-weight: bold; margin-bottom: 12pt; }
        "#;
        let resolver = StyleResolver::new(builtin).unwrap();

        let parent = Style::default();
        let ancestors = vec!["body".to_string()];

        let style = resolver.resolve_style("h1", &[], &ancestors, &parent);
        assert_eq!(style.font_size_pt, 24.0);
        assert_eq!(style.font_weight, FontWeight::Bold);
        // 继承自 body 的 font-family
        assert_eq!(style.font_family[0], "serif");
    }

    #[test]
    fn test_user_css_override() {
        let builtin = r#"
            body { font-family: serif; font-size: 10.5pt; }
            h1 { font-size: 24pt; color: #000; }
        "#;
        let user = r#"
            h1 { font-size: 28pt; color: #333; }
        "#;
        let resolver = StyleResolver::new(builtin)
            .unwrap()
            .with_user_css(user)
            .unwrap();

        let parent = Style::default();
        let ancestors = vec!["body".to_string()];

        let style = resolver.resolve_style("h1", &[], &ancestors, &parent);
        // 用户 CSS 覆盖了 font-size 和 color
        assert_eq!(style.font_size_pt, 28.0);
        assert_eq!(style.color.r, 0x33);
        assert_eq!(style.color.g, 0x33);
        assert_eq!(style.color.b, 0x33);
        // font-family 继承自 body
        assert_eq!(style.font_family[0], "serif");
    }

    #[test]
    fn test_specificity_order() {
        let builtin = r#"
            p { color: red; }
            .special { color: blue; }
        "#;
        let resolver = StyleResolver::new(builtin).unwrap();

        let parent = Style::default();
        let style = resolver.resolve_style("p", &["special".to_string()], &[], &parent);
        // .special 特异性更高（10 > 1），所以 color 应该是 blue
        assert_eq!(style.color.r, 0);
        assert_eq!(style.color.g, 0);
        assert_eq!(style.color.b, 255);
    }

    #[test]
    fn test_node_tag_name() {
        use super::super::NodeKind;
        assert_eq!(node_tag_name(&NodeKind::Paragraph { children: vec![] }), "p");
        assert_eq!(node_tag_name(&NodeKind::Heading { level: 1, children: vec![] }), "h1");
        assert_eq!(node_tag_name(&NodeKind::Heading { level: 3, children: vec![] }), "h3");
        assert_eq!(node_tag_name(&NodeKind::Strong { children: vec![] }), "strong");
        assert_eq!(node_tag_name(&NodeKind::Image { src: String::new(), alt: String::new(), title: None }), "img");
        assert_eq!(node_tag_name(&NodeKind::CodeBlock { code: String::new(), lang: None }), "pre");
    }

    #[test]
    fn test_page_at_rule() {
        let css = r#"
            @page {
                margin: 36pt 54pt;
                size: A4;
            }
        "#;
        let sheet = parse_css(css).unwrap();
        let config = sheet.extract_page_config();
        assert_eq!(config.margin_top, Some(36.0));
        assert_eq!(config.margin_bottom, Some(36.0));
        assert_eq!(config.margin_left, Some(54.0));
        assert_eq!(config.margin_right, Some(54.0));
        assert_eq!(config.width, Some(595.276));
        assert_eq!(config.height, Some(841.890));
    }

    #[test]
    fn test_page_at_rule_margin_shorthand() {
        let css = r#"
            @page {
                margin: 10px 20px 30px 40px;
            }
        "#;
        let sheet = parse_css(css).unwrap();
        let config = sheet.extract_page_config();
        assert_eq!(config.margin_top, Some(10.0));
        assert_eq!(config.margin_right, Some(20.0));
        assert_eq!(config.margin_bottom, Some(30.0));
        assert_eq!(config.margin_left, Some(40.0));
    }

    #[test]
    fn test_page_at_rule_size_named() {
        let css = r#"
            @page {
                size: Letter;
            }
        "#;
        let sheet = parse_css(css).unwrap();
        let config = sheet.extract_page_config();
        assert_eq!(config.width, Some(612.0));
        assert_eq!(config.height, Some(792.0));
    }

    #[test]
    fn test_page_at_rule_size_a3() {
        let css = r#"
            @page {
                size: A3;
            }
        "#;
        let sheet = parse_css(css).unwrap();
        let config = sheet.extract_page_config();
        assert_eq!(config.width, Some(841.890));
        assert_eq!(config.height, Some(1190.551));
    }

    #[test]
    fn test_page_at_rule_size_legal() {
        let css = r#"
            @page {
                size: Legal;
            }
        "#;
        let sheet = parse_css(css).unwrap();
        let config = sheet.extract_page_config();
        assert_eq!(config.width, Some(612.0));
        assert_eq!(config.height, Some(1008.0));
    }

    #[test]
    fn test_page_at_rule_size_tabloid() {
        let css = r#"
            @page {
                size: Tabloid;
            }
        "#;
        let sheet = parse_css(css).unwrap();
        let config = sheet.extract_page_config();
        assert_eq!(config.width, Some(792.0));
        assert_eq!(config.height, Some(1224.0));
    }

    #[test]
    fn test_page_at_rule_size_landscape() {
        let css = r#"
            @page {
                size: landscape;
            }
        "#;
        let sheet = parse_css(css).unwrap();
        let config = sheet.extract_page_config();
        // landscape without size name defaults to A4 landscape (swapped)
        assert_eq!(config.width, Some(841.890));
        assert_eq!(config.height, Some(595.276));
    }

    #[test]
    fn test_page_at_rule_size_a4_landscape() {
        let css = r#"
            @page {
                size: A4 landscape;
            }
        "#;
        let sheet = parse_css(css).unwrap();
        let config = sheet.extract_page_config();
        assert_eq!(config.width, Some(841.890));
        assert_eq!(config.height, Some(595.276));
    }

    #[test]
    fn test_page_at_rule_size_custom_landscape() {
        let css = r#"
            @page {
                size: 600pt 800pt landscape;
            }
        "#;
        let sheet = parse_css(css).unwrap();
        let config = sheet.extract_page_config();
        assert_eq!(config.width, Some(800.0));
        assert_eq!(config.height, Some(600.0));
    }

    #[test]
    fn test_page_at_rule_size_portrait() {
        let css = r#"
            @page {
                size: Letter portrait;
            }
        "#;
        let sheet = parse_css(css).unwrap();
        let config = sheet.extract_page_config();
        assert_eq!(config.width, Some(612.0));
        assert_eq!(config.height, Some(792.0));
    }
}