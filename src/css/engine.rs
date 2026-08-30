//! CSS 引擎实现
//!
//! 基于 Lightning CSS 的样式解析、选择器匹配和级联计算。
//! 将 Lightning CSS 的类型化属性值转换为 LiePress 内部的 Style 结构。

use crate::ast::style::{
    CssLength, Display, FontStyle, LineHeight, ObjectFit, PageBreak, PageConfig, Style, TextAlign,
    TextDecoration, WhiteSpace,
};
use lievisual::Color;
use lievisual::text::FontWeight;
use lightningcss::rules::CssRule;
use lightningcss::selector::Component;
use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleSheet};
use lightningcss::traits::ToCss;
use std::collections::HashMap;

/// CSS 引擎
///
/// 管理样式表的解析和样式解析。
/// 支持内置样式 + 用户 CSS 覆盖 + 内联样式。
pub struct CssEngine {
    /// 内置样式表（解析后的规则）
    builtin_rules: Vec<ResolvedRule>,
    /// 用户样式表（解析后的规则）
    user_rules: Vec<ResolvedRule>,
    /// 从 @page 规则提取的页面配置
    page_config: PageConfig,
    /// 严格模式
    strict: bool,
    /// 根元素字体大小（pt），用于解析 `rem` 单位
    root_font_size: f32,
}

/// 解析后的 CSS 规则
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ResolvedRule {
    /// 选择器信息（用于匹配）
    selectors: Vec<SelectorInfo>,
    /// 原始声明（属性名 → 值字符串）
    declarations: Vec<(String, String)>,
    /// 最大特异性
    max_specificity: u32,
}

/// 选择器信息
#[derive(Debug, Clone)]
struct SelectorInfo {
    /// 目标标签名（最后一个简单选择器的标签）
    target_tag: Option<String>,
    /// 目标类名列表
    target_classes: Vec<String>,
    /// 目标 ID
    target_id: Option<String>,
    /// 祖先选择器链（从近到远）
    ancestors: Vec<AncestorSelector>,
    /// 特异性
    specificity: u32,
}

/// 祖先选择器
#[derive(Debug, Clone)]
struct AncestorSelector {
    tag: Option<String>,
    classes: Vec<String>,
    id: Option<String>,
}

impl CssEngine {
    /// 使用内置样式表创建引擎
    pub fn new(builtin_css: &str) -> Result<Self, String> {
        let builtin_rules = parse_css_rules(builtin_css)?;
        let page_config = extract_page_config_from_css(builtin_css);
        Ok(Self {
            builtin_rules,
            user_rules: Vec::new(),
            page_config,
            strict: false,
            // 默认根元素字号 10.5pt，可在解析 html 元素后更新
            root_font_size: 10.5,
        })
    }

    /// 设置严格模式
    pub fn with_strict_mode(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// 添加用户 CSS 覆盖
    pub fn with_user_css(self, user_css: &str) -> Result<Self, String> {
        if user_css.trim().is_empty() {
            return Ok(self);
        }

        match parse_css_rules(user_css) {
            Ok(user_rules) => {
                let mut page_config = self.page_config.clone();
                let user_page = extract_page_config_from_css(user_css);
                merge_page_config(&mut page_config, user_page);

                Ok(Self {
                    user_rules,
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

    /// 获取页面配置
    pub fn page_config(&self) -> &PageConfig {
        &self.page_config
    }

    /// 获取根元素字号（pt）
    pub fn root_font_size(&self) -> f32 {
        self.root_font_size
    }

    /// 设置根元素字号（pt）
    pub fn set_root_font_size(&mut self, pt: f32) {
        self.root_font_size = pt;
    }

    /// 解析节点的最终样式
    pub fn resolve_style(
        &self,
        tag: &str,
        classes: &[String],
        id: Option<&str>,
        ancestor_info: &[AncestorInfo],
        parent_style: &Style,
    ) -> Style {
        let mut style = Style::inherit_from(parent_style);

        // 收集所有匹配的规则
        let all_rules = self.all_rules();
        let mut matches: Vec<(&ResolvedRule, u32)> = Vec::new();

        for rule in all_rules {
            for selector in &rule.selectors {
                if let Some(specificity) =
                    match_selector_info(selector, tag, classes, id, ancestor_info)
                {
                    matches.push((rule, specificity));
                    break;
                }
            }
        }

        // 按特异性排序（低优先级先应用，高优先级覆盖）
        matches.sort_by_key(|(_, spec)| *spec);

        // 依次应用声明
        for (rule, _) in &matches {
            for (property, value) in &rule.declarations {
                apply_declaration(&mut style, property, value, self.root_font_size);
            }
        }

        style
    }

    /// 解析并应用内联 CSS 样式字符串
    pub fn apply_inline_style(&self, style: &mut Style, inline_css: &str) {
        let declarations = parse_inline_declarations(inline_css);
        for (property, value) in declarations {
            apply_declaration(style, &property, &value, self.root_font_size);
        }
    }

    /// 合并所有规则
    fn all_rules(&self) -> Vec<&ResolvedRule> {
        let mut rules: Vec<&ResolvedRule> = self.builtin_rules.iter().collect();
        rules.extend(self.user_rules.iter());
        rules
    }
}

/// 祖先信息（用于选择器匹配）
#[derive(Debug, Clone)]
pub struct AncestorInfo {
    pub tag: String,
    pub classes: Vec<String>,
    pub id: Option<String>,
}

// ─── CSS 解析 ─────────────────────────────────────────────

/// 使用 Lightning CSS 解析 CSS 规则
fn parse_css_rules(css: &str) -> Result<Vec<ResolvedRule>, String> {
    let stylesheet = match StyleSheet::parse(css, ParserOptions::default()) {
        Ok(s) => s,
        Err(e) => return Err(format!("CSS parse error: {:?}", e)),
    };

    let mut rules = Vec::new();
    extract_rules(&stylesheet.rules.0, &mut rules);
    Ok(rules)
}

fn extract_rules(css_rules: &[CssRule], output: &mut Vec<ResolvedRule>) {
    for rule in css_rules {
        match rule {
            CssRule::Style(style_rule) => {
                let selectors = extract_selector_info(&style_rule.selectors);
                let declarations = extract_declarations(&style_rule.declarations);
                let max_specificity = selectors.iter().map(|s| s.specificity).max().unwrap_or(0);

                if !selectors.is_empty() && !declarations.is_empty() {
                    output.push(ResolvedRule {
                        selectors,
                        declarations,
                        max_specificity,
                    });
                }
            }
            CssRule::Media(media_rule) => {
                extract_rules(&media_rule.rules.0, output);
            }
            _ => {}
        }
    }
}

/// 从 Lightning CSS 的选择器列表中提取选择器信息
fn extract_selector_info(
    selector_list: &lightningcss::selector::SelectorList,
) -> Vec<SelectorInfo> {
    selector_list
        .0
        .iter()
        .map(|selector| {
            let mut target_tag = None;
            let mut target_classes = Vec::new();
            let mut target_id = None;
            let mut ancestors = Vec::new();
            let mut specificity = 0u32;

            let mut current_ancestor_tag = None;
            let mut current_ancestor_classes = Vec::new();
            let mut current_ancestor_id = None;

            let mut is_first_segment = true;

            for component in selector.iter() {
                match component {
                    Component::LocalName(local_name) => {
                        let name = local_name.name.as_ref().to_string();
                        specificity += 1;
                        if is_first_segment {
                            target_tag = Some(name);
                        } else {
                            current_ancestor_tag = Some(name);
                        }
                    }
                    Component::Class(class) => {
                        specificity += 10;
                        if is_first_segment {
                            target_classes.push(class.as_ref().to_string());
                        } else {
                            current_ancestor_classes.push(class.as_ref().to_string());
                        }
                    }
                    Component::ID(id) => {
                        specificity += 100;
                        if is_first_segment {
                            target_id = Some(id.as_ref().to_string());
                        } else {
                            current_ancestor_id = Some(id.as_ref().to_string());
                        }
                    }
                    Component::Combinator(_) => {
                        // 遇到组合器，将当前祖先信息保存
                        if current_ancestor_tag.is_some()
                            || !current_ancestor_classes.is_empty()
                            || current_ancestor_id.is_some()
                        {
                            ancestors.push(AncestorSelector {
                                tag: current_ancestor_tag.take(),
                                classes: std::mem::take(&mut current_ancestor_classes),
                                id: current_ancestor_id.take(),
                            });
                        }
                        is_first_segment = false;
                    }
                    _ => {
                        // 忽略通用选择器、伪类、伪元素等
                    }
                }
            }

            // 处理最后一个祖先段
            if current_ancestor_tag.is_some()
                || !current_ancestor_classes.is_empty()
                || current_ancestor_id.is_some()
            {
                ancestors.push(AncestorSelector {
                    tag: current_ancestor_tag,
                    classes: current_ancestor_classes,
                    id: current_ancestor_id,
                });
            }

            SelectorInfo {
                target_tag,
                target_classes,
                target_id,
                ancestors,
                specificity,
            }
        })
        .collect()
}

/// 从 Lightning CSS 的声明块中提取属性-值对
fn extract_declarations(
    declarations: &lightningcss::declaration::DeclarationBlock,
) -> Vec<(String, String)> {
    let mut result = Vec::new();

    // 处理普通声明
    for prop in &declarations.declarations {
        if let Some(pair) = property_to_string_pair(prop) {
            result.push(pair);
        }
    }

    // 处理 !important 声明
    for prop in &declarations.important_declarations {
        if let Some(pair) = property_to_string_pair(prop) {
            result.push(pair);
        }
    }

    result
}

/// 将 Lightning CSS Property 转换为 (name, value) 字符串对
fn property_to_string_pair(prop: &lightningcss::properties::Property) -> Option<(String, String)> {
    let name = prop
        .property_id()
        .to_css_string(PrinterOptions::default())
        .ok()?;
    let full_decl = prop.to_css_string(false, PrinterOptions::default()).ok()?;
    // to_css_string 输出格式为 "property-name: value"，需要提取值部分
    let value = if let Some(colon_pos) = full_decl.find(':') {
        full_decl[colon_pos + 1..].trim().to_string()
    } else {
        full_decl
    };
    Some((name, value))
}

/// 解析内联样式声明
fn parse_inline_declarations(css: &str) -> Vec<(String, String)> {
    // 使用 Lightning CSS 解析内联样式
    let decl_block =
        lightningcss::declaration::DeclarationBlock::parse_string(css, ParserOptions::default());

    match decl_block {
        Ok(block) => extract_declarations(&block),
        Err(_) => {
            // 回退到简单解析
            let mut declarations = Vec::new();
            for decl in css.split(';') {
                let decl = decl.trim();
                if decl.is_empty() {
                    continue;
                }
                if let Some(colon_pos) = decl.find(':') {
                    let property = decl[..colon_pos].trim().to_string();
                    let value = decl[colon_pos + 1..].trim().to_string();
                    if !property.is_empty() && !value.is_empty() {
                        declarations.push((property, value));
                    }
                }
            }
            declarations
        }
    }
}

// ─── 选择器匹配 ─────────────────────────────────────────────

fn match_selector_info(
    selector: &SelectorInfo,
    tag: &str,
    classes: &[String],
    id: Option<&str>,
    ancestor_info: &[AncestorInfo],
) -> Option<u32> {
    // 匹配目标标签
    if let Some(ref target_tag) = selector.target_tag
        && target_tag != tag
    {
        return None;
    }

    // 匹配目标类
    for class in &selector.target_classes {
        if !classes.contains(class) {
            return None;
        }
    }

    // 匹配目标 ID
    if let Some(ref target_id) = selector.target_id
        && id != Some(target_id.as_str())
    {
        return None;
    }

    // 匹配祖先选择器链
    if !selector.ancestors.is_empty() {
        let mut ancestor_idx = ancestor_info.len();

        for ancestor_sel in selector.ancestors.iter().rev() {
            let found = loop {
                if ancestor_idx == 0 {
                    break false;
                }
                ancestor_idx -= 1;
                let info = &ancestor_info[ancestor_idx];

                if let Some(ref req_tag) = ancestor_sel.tag
                    && req_tag != &info.tag
                {
                    continue;
                }

                let class_match = ancestor_sel
                    .classes
                    .iter()
                    .all(|c| info.classes.contains(c));
                if !class_match {
                    continue;
                }

                if let Some(ref req_id) = ancestor_sel.id
                    && info.id.as_deref() != Some(req_id.as_str())
                {
                    continue;
                }

                break true;
            };

            if !found {
                return None;
            }
        }
    }

    Some(selector.specificity)
}

// ─── 样式声明应用 ─────────────────────────────────────────────

fn apply_declaration(style: &mut Style, property: &str, value: &str, root_font_size: f32) {
    match property {
        "font-family" => {
            let families = parse_font_family(value);
            if !families.is_empty() {
                style.font_family = families;
            }
        }
        "font-size" => {
            if let Some(len) = parse_length(value) {
                style.font_size_pt = len.resolve(style.font_size_pt, root_font_size);
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
            style.line_height_pt =
                parse_line_height(value).resolve(style.font_size_pt, root_font_size);
        }
        "text-align" => {
            style.text_align = parse_text_align(value);
        }
        "margin-top" => {
            if let Some(len) = parse_length(value) {
                style.margin.top = len.resolve(style.font_size_pt, root_font_size);
            }
        }
        "margin-bottom" => {
            if let Some(len) = parse_length(value) {
                style.margin.bottom = len.resolve(style.font_size_pt, root_font_size);
            }
        }
        "margin-left" => {
            if let Some(len) = parse_length(value) {
                style.margin.left = len.resolve(style.font_size_pt, root_font_size);
            }
        }
        "margin-right" => {
            if let Some(len) = parse_length(value) {
                style.margin.right = len.resolve(style.font_size_pt, root_font_size);
            }
        }
        "padding-top" => {
            if let Some(len) = parse_length(value) {
                style.padding.top = len.resolve(style.font_size_pt, root_font_size);
            }
        }
        "padding-bottom" => {
            if let Some(len) = parse_length(value) {
                style.padding.bottom = len.resolve(style.font_size_pt, root_font_size);
            }
        }
        "padding-left" => {
            if let Some(len) = parse_length(value) {
                style.padding.left = len.resolve(style.font_size_pt, root_font_size);
            }
        }
        "padding-right" => {
            if let Some(len) = parse_length(value) {
                style.padding.right = len.resolve(style.font_size_pt, root_font_size);
            }
        }
        "display" => {
            style.display = parse_display(value);
        }
        "width" => {
            if value == "auto" {
                style.width = None;
            } else if let Some(len) = parse_length(value) {
                style.width = Some(len.resolve(style.font_size_pt, root_font_size));
            }
        }
        "height" => {
            if value == "auto" {
                style.height = None;
            } else if let Some(len) = parse_length(value) {
                style.height = Some(len.resolve(style.font_size_pt, root_font_size));
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
            if let Some(len) = parse_length(value) {
                style.letter_spacing = len.resolve(style.font_size_pt, root_font_size);
            }
        }
        "object-fit" => {
            style.object_fit = parse_object_fit(value);
        }
        "white-space" => {
            style.white_space = parse_white_space(value);
        }
        "text-decoration" => {
            let v = value.trim().to_lowercase();
            if v == "line-through" {
                style.text_decoration = TextDecoration::LineThrough;
            } else if v == "underline" {
                style.text_decoration = TextDecoration::Underline;
            } else if v == "none" {
                style.text_decoration = TextDecoration::None;
            }
        }
        "border" | "border-width" => {
            if let Some(len) = parse_length(value) {
                let v = len.resolve(style.font_size_pt, root_font_size);
                let bs = crate::ast::style::BorderSide::new(
                    v,
                    crate::ast::style::BorderStyle::Solid,
                    lievisual::Color::rgb(0, 0, 0),
                );
                style.border.top = bs;
                style.border.right = bs;
                style.border.bottom = bs;
                style.border.left = bs;
            }
        }
        "border-top" | "border-top-width" => {
            if let Some(len) = parse_length(value) {
                let v = len.resolve(style.font_size_pt, root_font_size);
                style.border.top = crate::ast::style::BorderSide::new(
                    v,
                    crate::ast::style::BorderStyle::Solid,
                    lievisual::Color::rgb(0, 0, 0),
                );
            }
        }
        "border-bottom" | "border-bottom-width" => {
            if let Some(len) = parse_length(value) {
                let v = len.resolve(style.font_size_pt, root_font_size);
                style.border.bottom = crate::ast::style::BorderSide::new(
                    v,
                    crate::ast::style::BorderStyle::Solid,
                    lievisual::Color::rgb(0, 0, 0),
                );
            }
        }
        "border-left" | "border-left-width" => {
            if let Some(len) = parse_length(value) {
                let v = len.resolve(style.font_size_pt, root_font_size);
                style.border.left = crate::ast::style::BorderSide::new(
                    v,
                    crate::ast::style::BorderStyle::Solid,
                    lievisual::Color::rgb(0, 0, 0),
                );
            }
        }
        "border-right" | "border-right-width" => {
            if let Some(len) = parse_length(value) {
                let v = len.resolve(style.font_size_pt, root_font_size);
                style.border.right = crate::ast::style::BorderSide::new(
                    v,
                    crate::ast::style::BorderStyle::Solid,
                    lievisual::Color::rgb(0, 0, 0),
                );
            }
        }
        "border-color" => {
            if let Some(c) = parse_color(value) {
                style.border.top.color = c;
                style.border.right.color = c;
                style.border.bottom.color = c;
                style.border.left.color = c;
            }
        }
        "border-style" => {
            let v = value.trim().to_lowercase();
            let bs = match v.as_str() {
                "solid" => crate::ast::style::BorderStyle::Solid,
                "dashed" => crate::ast::style::BorderStyle::Dashed,
                "dotted" => crate::ast::style::BorderStyle::Dotted,
                _ => crate::ast::style::BorderStyle::None,
            };
            style.border.top.style = bs;
            style.border.right.style = bs;
            style.border.bottom.style = bs;
            style.border.left.style = bs;
        }
        "border-radius" => {
            if let Some(len) = parse_length(value) {
                style.border.radius = len.resolve(style.font_size_pt, root_font_size);
            }
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
            if let Some(len) = parse_length(value) {
                style.table_border_width_pt = len.resolve(style.font_size_pt, root_font_size);
            }
        }
        "table-cell-padding-horizontal" | "table-cell-padding-h" => {
            if let Some(len) = parse_length(value) {
                style.table_cell_padding_h_pt = len.resolve(style.font_size_pt, root_font_size);
            }
        }
        "table-cell-padding-vertical" | "table-cell-padding-v" => {
            if let Some(len) = parse_length(value) {
                style.table_cell_padding_v_pt = len.resolve(style.font_size_pt, root_font_size);
            }
        }
        "list-indent" => {
            if let Some(len) = parse_length(value) {
                style.list_indent_pt = Some(len.resolve(style.font_size_pt, root_font_size));
            }
        }
        _ => {}
    }
}

// ─── CSS 值解析 ─────────────────────────────────────────────

fn parse_font_family(value: &str) -> Vec<String> {
    let mut families = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = ' ';

    for c in value.chars() {
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
        } else {
            current.push(c);
        }
    }

    if !current.is_empty() {
        families.push(current.trim().to_string());
    }

    families
}

fn parse_font_weight(value: &str) -> FontWeight {
    let v = crate::document::text::weight_to_f32(value);
    FontWeight::from_value(v.clamp(100.0, 900.0))
}

fn parse_font_style(value: &str) -> FontStyle {
    FontStyle::parse(value).unwrap_or(FontStyle::Normal)
}

fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim();

    match value.to_lowercase().as_str() {
        "black" => return Some(Color::rgb(0, 0, 0)),
        "white" => return Some(Color::rgb(255, 255, 255)),
        "red" => return Some(Color::rgb(255, 0, 0)),
        "green" => return Some(Color::rgb(0, 128, 0)),
        "blue" => return Some(Color::rgb(0, 0, 255)),
        "gray" | "grey" => return Some(Color::rgb(128, 128, 128)),
        "silver" => return Some(Color::rgb(192, 192, 192)),
        "yellow" => return Some(Color::rgb(255, 255, 0)),
        "orange" => return Some(Color::rgb(255, 165, 0)),
        "purple" => return Some(Color::rgb(128, 0, 128)),
        _ => {}
    }

    if let Some(hex) = value.strip_prefix('#') {
        // 仅当全部为 ASCII 十六进制字符时才按字节切片，避免在多字节字符
        // （如 `#é1`）的中间做字节索引而 panic。
        if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            match hex.len() {
                3 => {
                    let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                    let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                    let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                    return Some(Color::rgb(r, g, b));
                }
                6 => {
                    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                    return Some(Color::rgb(r, g, b));
                }
                _ => {}
            }
        }
    }

    if value.starts_with("rgb(") && value.ends_with(')') {
        let inner = &value[4..value.len() - 1];
        let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
        if parts.len() == 3 {
            let r = parts[0].parse::<u8>().ok()?;
            let g = parts[1].parse::<u8>().ok()?;
            let b = parts[2].parse::<u8>().ok()?;
            return Some(Color::rgb(r, g, b));
        }
    }

    None
}

fn parse_length(value: &str) -> Option<CssLength> {
    let value = value.trim();
    if value == "0" {
        return Some(CssLength::Pt(0.0));
    }

    if let Some(stripped) = value.strip_suffix("pt") {
        return stripped.trim().parse::<f32>().ok().map(CssLength::Pt);
    }

    if let Some(stripped) = value.strip_suffix("px") {
        return stripped.trim().parse::<f32>().ok().map(CssLength::Px);
    }

    if let Some(stripped) = value.strip_suffix("em") {
        return stripped.trim().parse::<f32>().ok().map(CssLength::Em);
    }

    if let Some(stripped) = value.strip_suffix("rem") {
        return stripped.trim().parse::<f32>().ok().map(CssLength::Rem);
    }

    if let Some(stripped) = value.strip_suffix('%') {
        return stripped.trim().parse::<f32>().ok().map(CssLength::Percent);
    }

    if let Ok(n) = value.parse::<f32>() {
        return Some(CssLength::Pt(n));
    }

    None
}

fn parse_line_height(value: &str) -> LineHeight {
    let value = value.trim();
    if let Ok(multiplier) = value.parse::<f32>() {
        return LineHeight::Number(multiplier);
    }
    match parse_length(value) {
        Some(len) => LineHeight::Length(len),
        None => LineHeight::Number(1.2),
    }
}

fn parse_text_align(value: &str) -> TextAlign {
    match value.trim().to_lowercase().as_str() {
        "left" => TextAlign::Left,
        "center" => TextAlign::Center,
        "right" => TextAlign::Right,
        "justify" => TextAlign::Justify,
        _ => TextAlign::Left,
    }
}

fn parse_display(value: &str) -> Display {
    match value.trim().to_lowercase().as_str() {
        "block" => Display::Block,
        "inline" => Display::Inline,
        "inline-block" => Display::InlineBlock,
        "none" => Display::None,
        _ => Display::Block,
    }
}

fn parse_page_break(value: &str) -> PageBreak {
    match value.trim().to_lowercase().as_str() {
        "always" => PageBreak::Always,
        "avoid" => PageBreak::Avoid,
        "left" => PageBreak::Left,
        "right" => PageBreak::Right,
        _ => PageBreak::Auto,
    }
}

fn parse_object_fit(value: &str) -> ObjectFit {
    match value.trim().to_lowercase().as_str() {
        "contain" => ObjectFit::Contain,
        "cover" => ObjectFit::Cover,
        "fill" => ObjectFit::Fill,
        "none" => ObjectFit::None,
        _ => ObjectFit::Contain,
    }
}

fn parse_white_space(value: &str) -> WhiteSpace {
    match value.trim().to_lowercase().as_str() {
        "pre" => WhiteSpace::Pre,
        "nowrap" => WhiteSpace::NoWrap,
        _ => WhiteSpace::Normal,
    }
}

// ─── @page 配置 ─────────────────────────────────────────────

fn extract_page_config_from_css(css: &str) -> PageConfig {
    let stylesheet = match StyleSheet::parse(css, ParserOptions::default()) {
        Ok(s) => s,
        Err(_) => return PageConfig::default(),
    };

    let mut config = PageConfig::default();

    for rule in &stylesheet.rules.0 {
        if let CssRule::Page(page_rule) = rule {
            for prop in &page_rule.declarations.declarations {
                if let Some((name, value)) = property_to_string_pair(prop) {
                    match name.as_str() {
                        "margin-top" => {
                            config.margin_top =
                                parse_length(&value).map(|len| len.resolve(12.0, 12.0));
                        }
                        "margin-bottom" => {
                            config.margin_bottom =
                                parse_length(&value).map(|len| len.resolve(12.0, 12.0));
                        }
                        "margin-left" => {
                            config.margin_left =
                                parse_length(&value).map(|len| len.resolve(12.0, 12.0));
                        }
                        "margin-right" => {
                            config.margin_right =
                                parse_length(&value).map(|len| len.resolve(12.0, 12.0));
                        }
                        "margin" => {
                            let parts: Vec<&str> = value.split_whitespace().collect();
                            match parts.len() {
                                1 => {
                                    let v =
                                        parse_length(parts[0]).map(|len| len.resolve(12.0, 12.0));
                                    config.margin_top = v;
                                    config.margin_right = v;
                                    config.margin_bottom = v;
                                    config.margin_left = v;
                                }
                                2 => {
                                    let v_tb =
                                        parse_length(parts[0]).map(|len| len.resolve(12.0, 12.0));
                                    let v_lr =
                                        parse_length(parts[1]).map(|len| len.resolve(12.0, 12.0));
                                    config.margin_top = v_tb;
                                    config.margin_bottom = v_tb;
                                    config.margin_left = v_lr;
                                    config.margin_right = v_lr;
                                }
                                4 => {
                                    config.margin_top =
                                        parse_length(parts[0]).map(|len| len.resolve(12.0, 12.0));
                                    config.margin_right =
                                        parse_length(parts[1]).map(|len| len.resolve(12.0, 12.0));
                                    config.margin_bottom =
                                        parse_length(parts[2]).map(|len| len.resolve(12.0, 12.0));
                                    config.margin_left =
                                        parse_length(parts[3]).map(|len| len.resolve(12.0, 12.0));
                                }
                                _ => {}
                            }
                        }
                        "size" => {
                            parse_page_size(&value, &mut config);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    config
}

fn parse_page_size(value: &str, config: &mut PageConfig) {
    let parts: Vec<&str> = value.split_whitespace().collect();

    let named_sizes: HashMap<&str, (f32, f32)> = [
        ("A3", (841.890, 1190.551)),
        ("A4", (595.276, 841.890)),
        ("A5", (419.528, 595.276)),
        ("Letter", (612.0, 792.0)),
        ("Legal", (612.0, 1008.0)),
        ("Tabloid", (792.0, 1224.0)),
    ]
    .into_iter()
    .collect();

    let mut is_landscape = false;
    let mut size_name = None;

    for part in &parts {
        match part.to_lowercase().as_str() {
            "landscape" => is_landscape = true,
            "portrait" => is_landscape = false,
            _ => {
                if named_sizes.contains_key(*part) {
                    size_name = Some(*part);
                }
            }
        }
    }

    if let Some(name) = size_name {
        let (w, h) = named_sizes[name];
        if is_landscape {
            config.width = Some(h);
            config.height = Some(w);
        } else {
            config.width = Some(w);
            config.height = Some(h);
        }
        return;
    }

    let dims: Vec<f32> = parts
        .iter()
        .filter_map(|p| parse_length(p).map(|len| len.resolve(12.0, 12.0)))
        .collect();

    if dims.len() >= 2 {
        if is_landscape {
            config.width = Some(dims[1].max(dims[0]));
            config.height = Some(dims[0].min(dims[1]));
        } else {
            config.width = Some(dims[0]);
            config.height = Some(dims[1]);
        }
    }
}

fn merge_page_config(target: &mut PageConfig, source: PageConfig) {
    if source.margin_top.is_some() {
        target.margin_top = source.margin_top;
    }
    if source.margin_bottom.is_some() {
        target.margin_bottom = source.margin_bottom;
    }
    if source.margin_left.is_some() {
        target.margin_left = source.margin_left;
    }
    if source.margin_right.is_some() {
        target.margin_right = source.margin_right;
    }
    if source.width.is_some() {
        target.width = source.width;
    }
    if source.height.is_some() {
        target.height = source.height;
    }
    if source.header.is_some() {
        target.header = source.header;
    }
    if source.footer.is_some() {
        target.footer = source.footer;
    }
    if source.header_font_size.is_some() {
        target.header_font_size = source.header_font_size;
    }
    if source.footer_font_size.is_some() {
        target.footer_font_size = source.footer_font_size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_css_engine_basic() {
        let css = r#"
            body { font-family: serif; font-size: 10.5pt; color: #000; }
            h1 { font-size: 24pt; font-weight: bold; }
        "#;
        let engine = CssEngine::new(css).unwrap();
        let parent = Style::default();
        let ancestors = vec![AncestorInfo {
            tag: "body".to_string(),
            classes: vec![],
            id: None,
        }];
        let style = engine.resolve_style("h1", &[], None, &ancestors, &parent);
        assert_eq!(style.font_size_pt, 24.0);
        assert_eq!(style.font_weight, FontWeight::Bold);
    }

    #[test]
    fn test_class_selector() {
        let css = r#"
            p { color: #000; }
            .special { color: #00f; }
        "#;
        let engine = CssEngine::new(css).unwrap();
        let parent = Style::default();
        let style = engine.resolve_style("p", &["special".to_string()], None, &[], &parent);
        assert_eq!(style.color.b, 255);
    }

    #[test]
    fn test_inline_style() {
        let engine = CssEngine::new("").unwrap();
        let mut style = Style::default();
        engine.apply_inline_style(&mut style, "color: red; font-size: 14pt");
        assert_eq!(style.color.r, 255);
        assert_eq!(style.font_size_pt, 14.0);
    }

    #[test]
    fn test_page_config() {
        let css = r#"
            @page {
                margin: 36pt 54pt;
                size: A4;
            }
        "#;
        let engine = CssEngine::new(css).unwrap();
        let config = engine.page_config();
        assert_eq!(config.margin_top, Some(36.0));
        assert_eq!(config.margin_left, Some(54.0));
        assert_eq!(config.width, Some(595.276));
    }
}
