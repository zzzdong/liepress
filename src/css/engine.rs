//! CSS 引擎实现
//!
//! 基于 Lightning CSS 的样式解析、选择器匹配和级联计算。
//! 将 Lightning CSS 的类型化属性值转换为 LiePress 内部的 Style 结构。

use crate::ast::style::{
    BorderSide, BorderStyle, BoxSides, CssLength, Display, FontStyle, LineHeight, ObjectFit,
    PageBreak, PageConfig, Style, TextAlign, TextDecoration, WhiteSpace,
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
    /// 包含块宽度（pt），用于解析盒模型属性（`width`/`margin`/`padding`）的 `%`
    containing_block_width: f32,
}

/// 解析后的 CSS 规则
#[derive(Debug, Clone)]
struct ResolvedRule {
    /// 选择器信息（用于匹配）
    selectors: Vec<SelectorInfo>,
    /// 原始声明（属性名 → 值字符串 + 是否 `!important`）
    declarations: Vec<Declaration>,
}

/// 单条已解析声明。
///
/// `important` 参与级联排序：见 [`CssEngine::resolve_style`]——带 `!important` 的
/// 声明被归到高优先级组，最终在普通声明之后应用，从而能压过更高特异性的普通声明
/// （符合 CSS 级联规范：important 覆盖 origin/specificity 顺序）。
#[derive(Debug, Clone)]
struct Declaration {
    property: String,
    value: String,
    important: bool,
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
            // 默认 A4 内容宽度，可在页面设置确定后由 `set_containing_block_width` 更新
            containing_block_width: crate::document::types::default_content_width(),
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

    /// 设置包含块宽度（pt），作为 `width`/`height`/`margin`/`padding` 百分比的基准。
    ///
    /// 应在页面尺寸/边距确定后、转换 Styled AST 之前调用（默认值为 A4 内容宽度）。
    pub fn set_containing_block_width(&mut self, pt: f32) {
        if pt.is_finite() && pt > 0.0 {
            self.containing_block_width = pt;
        }
    }

    /// 获取包含块宽度（pt）
    pub fn containing_block_width(&self) -> f32 {
        self.containing_block_width
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

        // CSS 级联：`!important` 声明优先于所有普通声明，与特异性/来源顺序无关。
        // 故分两趟应用：先按特异性升序应用普通声明，再按特异性升序应用 important
        // 声明（important 之间仍按特异性排序，高特异性胜出）。
        let ctx = ResolveCtx {
            font_size: style.font_size_pt,
            root_font_size: self.root_font_size,
            containing_block_width: self.containing_block_width,
        };

        let mut ctx = ctx;
        for (rule, _) in &matches {
            for decl in &rule.declarations {
                if !decl.important {
                    apply_declaration(&mut style, &decl.property, &decl.value, &ctx);
                    ctx.font_size = style.font_size_pt;
                }
            }
        }
        for (rule, _) in &matches {
            for decl in &rule.declarations {
                if decl.important {
                    apply_declaration(&mut style, &decl.property, &decl.value, &ctx);
                    ctx.font_size = style.font_size_pt;
                }
            }
        }

        style
    }

    /// 解析并应用内联 CSS 样式字符串
    pub fn apply_inline_style(&self, style: &mut Style, inline_css: &str) {
        let declarations = parse_inline_declarations(inline_css);
        // 内联样式优先级最高：先应用普通声明，再应用 important 声明。
        let mut ctx = ResolveCtx {
            font_size: style.font_size_pt,
            root_font_size: self.root_font_size,
            containing_block_width: self.containing_block_width,
        };
        for decl in &declarations {
            if !decl.important {
                apply_declaration(style, &decl.property, &decl.value, &ctx);
                ctx.font_size = style.font_size_pt;
            }
        }
        for decl in &declarations {
            if decl.important {
                apply_declaration(style, &decl.property, &decl.value, &ctx);
                ctx.font_size = style.font_size_pt;
            }
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

                if !selectors.is_empty() && !declarations.is_empty() {
                    output.push(ResolvedRule {
                        selectors,
                        declarations,
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

            // 注意：`Selector::iter()`（parcel_selectors）在遇到组合器时**停止**，
            // 仅产出最右侧 compound；须用 `next_sequence()` 逐段推进才能遍历
            // 完整的 `A B C` 链，否则祖先约束整体丢失、`div p span` 会匹配任意 span。
            // 迭代序为匹配序（自右向左），故祖先段保存顺序为近→远。
            let mut selector_iter = selector.iter();
            loop {
                for component in &mut selector_iter {
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
                        _ => {
                            // 忽略通用选择器、伪类、伪元素等
                        }
                    }
                }

                // 当前 compound 结束：若有祖先段内容，保存（近→远）。
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

                match selector_iter.next_sequence() {
                    Some(_combinator) => {
                        // 后续 compound 均为祖先段（`>`/` `/`~` 统一按后代语义处理）。
                        is_first_segment = false;
                    }
                    None => break,
                }
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

/// 从 Lightning CSS 的声明块中提取声明（含 `!important` 标记）。
fn extract_declarations(
    declarations: &lightningcss::declaration::DeclarationBlock,
) -> Vec<Declaration> {
    let mut result = Vec::new();

    // 处理普通声明
    for prop in &declarations.declarations {
        if let Some(pair) = property_to_string_pair(prop) {
            result.push(Declaration {
                property: pair.0,
                value: pair.1,
                important: false,
            });
        }
    }

    // 处理 !important 声明
    for prop in &declarations.important_declarations {
        if let Some(pair) = property_to_string_pair(prop) {
            result.push(Declaration {
                property: pair.0,
                value: pair.1,
                important: true,
            });
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

/// 解析内联样式声明（含 `!important` 识别）。
fn parse_inline_declarations(css: &str) -> Vec<Declaration> {
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
                    let raw_value = decl[colon_pos + 1..].trim();
                    // 剥离 `!important`（大小写不敏感，前后可有空白）。
                    let (value, important) =
                        match raw_value.to_lowercase().rsplit_once("!important") {
                            Some((v, _)) => (v.trim().to_string(), true),
                            None => (raw_value.to_string(), false),
                        };
                    if !property.is_empty() && !value.is_empty() {
                        declarations.push(Declaration {
                            property,
                            value,
                            important,
                        });
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

    // 匹配祖先选择器链。
    //
    // 两个序列的顺序约定：
    // - `selector.ancestors` 从**近到远**（`ancestors[0]` 为紧邻 target 的祖先段，
    //   见 `extract_selector_info`——selectors crate 按匹配序自右向左迭代）；
    // - `ancestor_info` 从**远到近**（`ancestor_info[len-1]` 为直接父级，
    //   见 `StyleResolver::with_ancestor` 的入栈顺序）。
    //
    // CSS 后代语义：`A B C` 要求 C 的某个祖先匹配 B，且该祖先的某个祖先匹配 A，
    // 即每一段必须严格位于上一段**之上**（更远离目标）。故从最近祖先向根部
    // 扫描 `ancestors[0]`，命中后把扫描上界压到命中位置，再为 `ancestors[1]`
    // 扫描其上方，依此递推。（贪心自最近端匹配对纯后代组合器是正确且最优的。）
    if !selector.ancestors.is_empty() {
        // `upper`：当前段允许匹配的祖先范围上界（不含）——后续段必须严格更远。
        let mut upper = ancestor_info.len();
        for ancestor_sel in &selector.ancestors {
            let mut idx = upper;
            let found = loop {
                if idx == 0 {
                    break false;
                }
                idx -= 1;
                let info = &ancestor_info[idx];

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
            upper = idx;
        }
    }

    Some(selector.specificity)
}

// ─── 样式声明应用 ─────────────────────────────────────────────

// ─── 样式声明应用 ─────────────────────────────────────────────

/// 长度解析上下文。
///
/// 承载解析 `em` / `rem` / `%` 所需的基准值。`%` 的基准**按属性而异**：
/// - `font-size` / `line-height` / `letter-spacing` 等排版属性 → 相对字号
/// - `width` / `height` / `margin` / `padding` 等盒模型属性 → 相对**包含块宽度**
///
/// 二者混用（一律按字号换算）会让 `width: 50%` 得到「半个字宽」的荒谬盒子，
/// 故在此显式区分（见 [`ResolveCtx::box_length`] 与 [`ResolveCtx::font_length`]）。
#[derive(Clone, Copy)]
struct ResolveCtx {
    /// 当前元素已计算字号（pt），`em` 基准。
    font_size: f32,
    /// 根元素字号（pt），`rem` 基准。
    root_font_size: f32,
    /// 包含块宽度（pt），盒模型属性的 `%` 基准。
    containing_block_width: f32,
}

impl ResolveCtx {
    /// 解析**相对字号**的长度（`font-size` / `line-height` / `letter-spacing`）。
    fn font_length(&self, len: CssLength) -> f32 {
        len.resolve(self.font_size, self.root_font_size)
    }

    /// 解析**盒模型**长度（`width` / `height` / `margin` / `padding`），
    /// 其中 `%` 相对包含块宽度。
    fn box_length(&self, len: CssLength) -> f32 {
        match len {
            CssLength::Percent(v) => v / 100.0 * self.containing_block_width,
            other => other.resolve(self.font_size, self.root_font_size),
        }
    }

    /// 解析行高（纯数字乘数 / 长度，均相对字号）。
    fn font_length_line(&self, lh: LineHeight) -> f32 {
        lh.resolve(self.font_size, self.root_font_size)
    }
}

/// 简写展开结果：上 / 右 / 下 / 左 四个值。
type Sides4<T> = [T; 4];

/// 解析 1–4 值简写（`margin` / `padding` / `border-width`）。
///
/// 规则（CSS 盒模型简写）：
/// - 1 值 → 四边同值
/// - 2 值 → [上下, 左右]
/// - 3 值 → [上, 左右, 下]
/// - 4 值 → [上, 右, 下, 左]
///
/// `auto` 按 0 处理：本布局引擎无 auto 外边距居中能力，但至少不能因为
/// 一个 `auto` 就把 `margin: 0 auto` 的其余三边整条丢弃。
///
/// 任一分词无法解析时整体返回 `None`（保守，避免半套用）。
fn parse_sides<T: Clone>(value: &str, parse_one: impl Fn(&str) -> Option<T>) -> Option<Sides4<T>> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    let mut parsed = Vec::with_capacity(parts.len());
    for p in &parts {
        parsed.push(parse_one(p.trim())?);
    }
    let sides = match parsed.len() {
        1 => [&parsed[0], &parsed[0], &parsed[0], &parsed[0]],
        2 => [&parsed[0], &parsed[1], &parsed[0], &parsed[1]],
        3 => [&parsed[0], &parsed[1], &parsed[2], &parsed[1]],
        4 => [&parsed[0], &parsed[1], &parsed[2], &parsed[3]],
        _ => return None,
    };
    // 逐个克隆出拥有所有权的数组（T: Clone）。
    Some(std::array::from_fn(|i| sides[i].clone()))
}

/// 解析单个长度分词，`auto` 视为 0。
fn parse_side_length(token: &str) -> Option<CssLength> {
    if token.eq_ignore_ascii_case("auto") {
        return Some(CssLength::Pt(0.0));
    }
    parse_length(token)
}

/// `border` 简写：`<width>? <style>? <color>?`（顺序任意，均可省略）。
///
/// 省略分量按 CSS 初值：宽度 `medium`（此处按 0 处理，避免凭空出黑框）、
/// 样式 `none`、颜色 `currentColor`（即元素 `color`）。
fn parse_border_shorthand(value: &str) -> Option<(CssLength, BorderStyle, Option<Color>)> {
    let mut width: Option<CssLength> = None;
    let mut style: Option<BorderStyle> = None;
    let mut color: Option<Color> = None;

    for token in value.split_whitespace() {
        if width.is_none()
            && let Some(len) = parse_length(token)
        {
            width = Some(len);
            continue;
        }
        if style.is_none()
            && let Some(bs) = parse_border_style(token)
        {
            style = Some(bs);
            continue;
        }
        if color.is_none()
            && let Some(c) = parse_color(token)
        {
            color = Some(c);
        }
    }

    if width.is_none() && style.is_none() && color.is_none() {
        return None;
    }
    Some((
        width.unwrap_or(CssLength::Pt(0.0)),
        style.unwrap_or(BorderStyle::None),
        color,
    ))
}

fn apply_declaration(style: &mut Style, property: &str, value: &str, ctx: &ResolveCtx) {
    let value = value.trim();
    match property {
        "font-family" => {
            let families = parse_font_family(value);
            if !families.is_empty() {
                style.font_family = families;
            }
        }
        "font-size" => {
            if let Some(len) = parse_length(value) {
                // `%` 在 font-size 上相对**父级字号**：此时 style.font_size_pt 仍是继承值。
                style.font_size_pt = ctx.font_length(len);
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
            if value.eq_ignore_ascii_case("normal") {
                // CSS `normal` ≈ 1.2（浏览器常用值），与解析器回退值一致。
                style.line_height_pt = ctx.font_length(CssLength::Em(1.2));
            } else {
                style.line_height_pt = ctx.font_length_line(parse_line_height(value));
            }
        }
        "text-align" => {
            style.text_align = parse_text_align(value);
        }
        // ─── margin / padding：简写 + 单边 ───
        "margin" => {
            if let Some(s) = parse_sides(value, parse_side_length) {
                style.margin = BoxSides::new(
                    ctx.box_length(s[0]),
                    ctx.box_length(s[1]),
                    ctx.box_length(s[2]),
                    ctx.box_length(s[3]),
                );
            }
        }
        "padding" => {
            if let Some(s) = parse_sides(value, parse_side_length) {
                style.padding = BoxSides::new(
                    ctx.box_length(s[0]),
                    ctx.box_length(s[1]),
                    ctx.box_length(s[2]),
                    ctx.box_length(s[3]),
                );
            }
        }
        "margin-top" => {
            if let Some(len) = parse_length(value) {
                style.margin.top = ctx.box_length(len);
            }
        }
        "margin-bottom" => {
            if let Some(len) = parse_length(value) {
                style.margin.bottom = ctx.box_length(len);
            }
        }
        "margin-left" => {
            if let Some(len) = parse_length(value) {
                style.margin.left = ctx.box_length(len);
            }
        }
        "margin-right" => {
            if let Some(len) = parse_length(value) {
                style.margin.right = ctx.box_length(len);
            }
        }
        "padding-top" => {
            if let Some(len) = parse_length(value) {
                style.padding.top = ctx.box_length(len);
            }
        }
        "padding-bottom" => {
            if let Some(len) = parse_length(value) {
                style.padding.bottom = ctx.box_length(len);
            }
        }
        "padding-left" => {
            if let Some(len) = parse_length(value) {
                style.padding.left = ctx.box_length(len);
            }
        }
        "padding-right" => {
            if let Some(len) = parse_length(value) {
                style.padding.right = ctx.box_length(len);
            }
        }
        "display" => {
            style.display = parse_display(value);
        }
        "width" => {
            if value == "auto" {
                style.width = None;
            } else if let Some(len) = parse_length(value) {
                style.width = Some(ctx.box_length(len));
            }
        }
        "height" => {
            if value == "auto" {
                style.height = None;
            } else if let Some(len) = parse_length(value) {
                // CSS 中 `height: %` 相对包含块**高度**；本引擎无高度链，退化为
                // 相对包含块宽度（与 `width` 同一基准），至少不再是「半个字宽」。
                style.height = Some(ctx.box_length(len));
            }
        }
        "background-color" => {
            if let Some(c) = parse_color(value) {
                style.background_color = Some(c);
            }
        }
        "background" => {
            // 简写：仅取颜色分量（最后一个可解析为颜色的分词），
            // 背景图/定位等分量本引擎不支持，忽略。
            if let Some(c) =
                parse_color(value).or_else(|| value.split_whitespace().rev().find_map(parse_color))
            {
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
                style.letter_spacing = ctx.font_length(len);
            }
        }
        "object-fit" => {
            style.object_fit = parse_object_fit(value);
        }
        "white-space" => {
            style.white_space = parse_white_space(value);
        }
        "text-decoration" => {
            let v = value.to_lowercase();
            if v == "line-through" {
                style.text_decoration = TextDecoration::LineThrough;
            } else if v == "underline" {
                style.text_decoration = TextDecoration::Underline;
            } else if v == "none" {
                style.text_decoration = TextDecoration::None;
            }
        }
        // ─── border：简写 + 单边 + 分量 ───
        "border" => {
            if let Some((w, bs, color)) = parse_border_shorthand(value) {
                // 未指定颜色时用 currentColor（即本元素 color），符合 CSS 初值语义。
                let c = color.unwrap_or(style.color);
                let side = BorderSide::new(ctx.box_length(w), bs, c);
                style.border.top = side;
                style.border.right = side;
                style.border.bottom = side;
                style.border.left = side;
            }
        }
        "border-width" => {
            if let Some(s) = parse_sides(value, parse_side_length) {
                style.border.top.width = ctx.font_length(s[0]);
                style.border.right.width = ctx.font_length(s[1]);
                style.border.bottom.width = ctx.font_length(s[2]);
                style.border.left.width = ctx.font_length(s[3]);
            }
        }
        "border-top" => {
            apply_border_side(&mut style.border.top, value, ctx, style.color);
        }
        "border-bottom" => {
            apply_border_side(&mut style.border.bottom, value, ctx, style.color);
        }
        "border-left" => {
            apply_border_side(&mut style.border.left, value, ctx, style.color);
        }
        "border-right" => {
            apply_border_side(&mut style.border.right, value, ctx, style.color);
        }
        "border-top-width" => {
            if let Some(len) = parse_length(value) {
                style.border.top.width = ctx.font_length(len);
            }
        }
        "border-bottom-width" => {
            if let Some(len) = parse_length(value) {
                style.border.bottom.width = ctx.font_length(len);
            }
        }
        "border-left-width" => {
            if let Some(len) = parse_length(value) {
                style.border.left.width = ctx.font_length(len);
            }
        }
        "border-right-width" => {
            if let Some(len) = parse_length(value) {
                style.border.right.width = ctx.font_length(len);
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
            let bs = parse_border_style(value).unwrap_or(BorderStyle::None);
            style.border.top.style = bs;
            style.border.right.style = bs;
            style.border.bottom.style = bs;
            style.border.left.style = bs;
        }
        "border-radius" => {
            if let Some(len) = parse_length(value) {
                style.border.radius = ctx.font_length(len);
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
                style.table_border_width_pt = ctx.font_length(len);
            }
        }
        "table-cell-padding-horizontal" | "table-cell-padding-h" => {
            if let Some(len) = parse_length(value) {
                style.table_cell_padding_h_pt = ctx.font_length(len);
            }
        }
        "table-cell-padding-vertical" | "table-cell-padding-v" => {
            if let Some(len) = parse_length(value) {
                style.table_cell_padding_v_pt = ctx.font_length(len);
            }
        }
        "list-indent" => {
            if let Some(len) = parse_length(value) {
                style.list_indent_pt = Some(ctx.font_length(len));
            }
        }
        _ => {}
    }
}

/// 应用单边 `border-*` 简写（`<width>? <style>? <color>?`）到指定边。
///
/// 兼容纯宽度写法（`border-left: 3pt`）：此时样式默认 solid、颜色用 currentColor，
/// 与内置 `default.css` 的 `border-left: 3pt; border-color: #b0b0b0` 组合语义一致
/// （`border-color` 随后覆盖颜色）。
fn apply_border_side(side: &mut BorderSide, value: &str, ctx: &ResolveCtx, current_color: Color) {
    if let Some((w, bs, color)) = parse_border_shorthand(value) {
        let has_style = value
            .split_whitespace()
            .any(|t| parse_border_style(t).is_some());
        // 只给了宽度（无样式关键字）时，沿用旧的「默认 solid」行为，
        // 否则 `border-left: 3pt` 会因 style=none 而完全不绘制。
        let style_kind = if has_style { bs } else { BorderStyle::Solid };
        *side = BorderSide::new(
            ctx.font_length(w),
            style_kind,
            color.unwrap_or(current_color),
        );
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

/// 解析 CSS 颜色值。
///
/// 支持：`#rgb` / `#rgba` / `#rrggbb` / `#rrggbbaa`、CSS 命名色（完整 148 色
/// 中的常用子集，含 `transparent` 与 `rebeccapurple`）、`rgb()` / `rgba()`
/// （逗号与现代空格斜杠语法）。
///
/// 不支持 `hsl()`/`hwb()`/`lab()` 等——但 lightningcss 在解析阶段已把它们
/// 归一化为 `rgb(...)`，故落到本函数时已是可识别形式。
fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    // ─── 命名色 ───
    if let Some(c) = named_color(&value.to_lowercase()) {
        return Some(c);
    }

    // ─── 十六进制（支持 #rgb / #rgba / #rrggbb / #rrggbbaa）───
    if let Some(hex) = value.strip_prefix('#') {
        // 仅当全部为 ASCII 十六进制字符时才按字节切片，避免在多字节字符
        // （如 `#é1`）的中间做字节索引而 panic。
        if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            let expand = |hi: &str| u8::from_str_radix(hi, 16).ok();
            match hex.len() {
                3 => {
                    let r = expand(&hex[0..1].repeat(2))?;
                    let g = expand(&hex[1..2].repeat(2))?;
                    let b = expand(&hex[2..3].repeat(2))?;
                    return Some(Color::rgb(r, g, b));
                }
                4 => {
                    let r = expand(&hex[0..1].repeat(2))?;
                    let g = expand(&hex[1..2].repeat(2))?;
                    let b = expand(&hex[2..3].repeat(2))?;
                    let a = expand(&hex[3..4].repeat(2))?;
                    return Some(Color::rgba(r, g, b, a));
                }
                6 => {
                    let r = expand(&hex[0..2])?;
                    let g = expand(&hex[2..4])?;
                    let b = expand(&hex[4..6])?;
                    return Some(Color::rgb(r, g, b));
                }
                8 => {
                    let r = expand(&hex[0..2])?;
                    let g = expand(&hex[2..4])?;
                    let b = expand(&hex[4..6])?;
                    let a = expand(&hex[6..8])?;
                    return Some(Color::rgba(r, g, b, a));
                }
                _ => {}
            }
        }
    }

    // ─── rgb() / rgba() ───
    // 同时接受两种语法：
    // - 传统逗号：`rgb(255, 0, 0)` / `rgba(255, 0, 0, 0.5)`
    // - 现代空格+斜杠：`rgb(255 0 0)` / `rgb(255 0 0 / 50%)`
    for prefix in ["rgb(", "rgba("] {
        if let Some(inner) = value
            .to_lowercase()
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix(')'))
        {
            return parse_rgb_components(inner);
        }
    }

    None
}

/// 解析 `rgb()`/`rgba()` 的分量串（不含括号）。
fn parse_rgb_components(inner: &str) -> Option<Color> {
    // alpha 以 `/` 分隔（`rgb(r g b / a)`）；若无 `/` 且为 4 分量则末位为 alpha。
    let (channel_part, alpha_part) = match inner.split_once('/') {
        Some((c, a)) => (c, Some(a.trim())),
        None => (inner, None),
    };

    // 分量可用逗号或空白分隔，统一用「逗号或空白」切分。
    let nums: Vec<&str> = channel_part
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    let alpha = match alpha_part {
        Some(a) => parse_alpha(a)?,
        None if nums.len() == 4 => parse_alpha(nums[3])?,
        None => 255,
    };
    if nums.len() < 3 {
        return None;
    }

    let r = parse_color_channel(nums[0])?;
    let g = parse_color_channel(nums[1])?;
    let b = parse_color_channel(nums[2])?;
    if alpha == 255 {
        Some(Color::rgb(r, g, b))
    } else {
        Some(Color::rgba(r, g, b, alpha))
    }
}

/// 解析 0–255 的颜色分量，支持百分比写法（`50%`）。
fn parse_color_channel(token: &str) -> Option<u8> {
    let token = token.trim();
    if let Some(pct) = token.strip_suffix('%') {
        let v: f32 = pct.trim().parse().ok()?;
        if !v.is_finite() {
            return None;
        }
        return Some((v / 100.0 * 255.0).round().clamp(0.0, 255.0) as u8);
    }
    // 允许 `255.0` 这类浮点写法（`round` 后钳制到 0–255）。
    let v: f32 = token.parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    Some(v.round().clamp(0.0, 255.0) as u8)
}

/// 解析 alpha 分量：`0.5` / `50%` 均支持。
fn parse_alpha(token: &str) -> Option<u8> {
    let token = token.trim();
    if let Some(pct) = token.strip_suffix('%') {
        let v: f32 = pct.trim().parse().ok()?;
        if !v.is_finite() {
            return None;
        }
        return Some((v / 100.0 * 255.0).round().clamp(0.0, 255.0) as u8);
    }
    let v: f32 = token.parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    Some((v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// CSS 命名色（含 `transparent`；`transparent` 为全透明黑）。
fn named_color(name: &str) -> Option<Color> {
    const NAMED: &[(&str, (u8, u8, u8))] = &[
        ("black", (0, 0, 0)),
        ("silver", (192, 192, 192)),
        ("gray", (128, 128, 128)),
        ("grey", (128, 128, 128)),
        ("white", (255, 255, 255)),
        ("maroon", (128, 0, 0)),
        ("red", (255, 0, 0)),
        ("purple", (128, 0, 128)),
        ("fuchsia", (255, 0, 255)),
        ("magenta", (255, 0, 255)),
        ("green", (0, 128, 0)),
        ("lime", (0, 255, 0)),
        ("olive", (128, 128, 0)),
        ("yellow", (255, 255, 0)),
        ("navy", (0, 0, 128)),
        ("blue", (0, 0, 255)),
        ("teal", (0, 128, 128)),
        ("aqua", (0, 255, 255)),
        ("cyan", (0, 255, 255)),
        ("orange", (255, 165, 0)),
        // 扩展命名色（常见子集）
        ("aliceblue", (240, 248, 255)),
        ("antiquewhite", (250, 235, 215)),
        ("aquamarine", (127, 255, 212)),
        ("azure", (240, 255, 255)),
        ("beige", (245, 245, 220)),
        ("bisque", (255, 228, 196)),
        ("blanchedalmond", (255, 235, 205)),
        ("blueviolet", (138, 43, 226)),
        ("brown", (165, 42, 42)),
        ("burlywood", (222, 184, 135)),
        ("cadetblue", (95, 158, 160)),
        ("chartreuse", (127, 255, 0)),
        ("chocolate", (210, 105, 30)),
        ("coral", (255, 127, 80)),
        ("cornflowerblue", (100, 149, 237)),
        ("cornsilk", (255, 248, 220)),
        ("crimson", (220, 20, 60)),
        ("darkblue", (0, 0, 139)),
        ("darkcyan", (0, 139, 139)),
        ("darkgoldenrod", (184, 134, 11)),
        ("darkgray", (169, 169, 169)),
        ("darkgreen", (0, 100, 0)),
        ("darkgrey", (169, 169, 169)),
        ("darkkhaki", (189, 183, 107)),
        ("darkmagenta", (139, 0, 139)),
        ("darkolivegreen", (85, 107, 47)),
        ("darkorange", (255, 140, 0)),
        ("darkorchid", (153, 50, 204)),
        ("darkred", (139, 0, 0)),
        ("darksalmon", (233, 150, 122)),
        ("darkseagreen", (143, 188, 143)),
        ("darkslateblue", (72, 61, 139)),
        ("darkslategray", (47, 79, 79)),
        ("darkturquoise", (0, 206, 209)),
        ("darkviolet", (148, 0, 211)),
        ("deeppink", (255, 20, 147)),
        ("deepskyblue", (0, 191, 255)),
        ("dimgray", (105, 105, 105)),
        ("dodgerblue", (30, 144, 255)),
        ("firebrick", (178, 34, 34)),
        ("floralwhite", (255, 250, 240)),
        ("forestgreen", (34, 139, 34)),
        ("gainsboro", (220, 220, 220)),
        ("ghostwhite", (248, 248, 255)),
        ("gold", (255, 215, 0)),
        ("goldenrod", (218, 165, 32)),
        ("greenyellow", (173, 255, 47)),
        ("honeydew", (240, 255, 240)),
        ("hotpink", (255, 105, 180)),
        ("indianred", (205, 92, 92)),
        ("indigo", (75, 0, 130)),
        ("ivory", (255, 255, 240)),
        ("khaki", (240, 230, 140)),
        ("lavender", (230, 230, 250)),
        ("lavenderblush", (255, 240, 245)),
        ("lawngreen", (124, 252, 0)),
        ("lemonchiffon", (255, 250, 205)),
        ("lightblue", (173, 216, 230)),
        ("lightcoral", (240, 128, 128)),
        ("lightcyan", (224, 255, 255)),
        ("lightgoldenrodyellow", (250, 250, 210)),
        ("lightgray", (211, 211, 211)),
        ("lightgreen", (144, 238, 144)),
        ("lightgrey", (211, 211, 211)),
        ("lightpink", (255, 182, 193)),
        ("lightsalmon", (255, 160, 122)),
        ("lightseagreen", (32, 178, 170)),
        ("lightskyblue", (135, 206, 250)),
        ("lightslategray", (119, 136, 153)),
        ("lightsteelblue", (176, 196, 222)),
        ("lightyellow", (255, 255, 224)),
        ("limegreen", (50, 205, 50)),
        ("linen", (250, 240, 230)),
        ("mediumaquamarine", (102, 205, 170)),
        ("mediumblue", (0, 0, 205)),
        ("mediumorchid", (186, 85, 211)),
        ("mediumpurple", (147, 112, 219)),
        ("mediumseagreen", (60, 179, 113)),
        ("mediumslateblue", (123, 104, 238)),
        ("mediumspringgreen", (0, 250, 154)),
        ("mediumturquoise", (72, 209, 204)),
        ("mediumvioletred", (199, 21, 133)),
        ("midnightblue", (25, 25, 112)),
        ("mintcream", (245, 255, 250)),
        ("mistyrose", (255, 228, 225)),
        ("moccasin", (255, 228, 181)),
        ("navajowhite", (255, 222, 173)),
        ("oldlace", (253, 245, 230)),
        ("olivedrab", (107, 142, 35)),
        ("orangered", (255, 69, 0)),
        ("orchid", (218, 112, 214)),
        ("palegoldenrod", (238, 232, 170)),
        ("palegreen", (152, 251, 152)),
        ("paleturquoise", (175, 238, 238)),
        ("palevioletred", (219, 112, 147)),
        ("papayawhip", (255, 239, 213)),
        ("peachpuff", (255, 218, 185)),
        ("peru", (205, 133, 63)),
        ("pink", (255, 192, 203)),
        ("plum", (221, 160, 221)),
        ("powderblue", (176, 224, 230)),
        ("rebeccapurple", (102, 51, 153)),
        ("rosybrown", (188, 143, 143)),
        ("royalblue", (65, 105, 225)),
        ("saddlebrown", (139, 69, 19)),
        ("salmon", (250, 128, 114)),
        ("sandybrown", (244, 164, 96)),
        ("seagreen", (46, 139, 87)),
        ("seashell", (255, 245, 238)),
        ("sienna", (160, 82, 45)),
        ("skyblue", (135, 206, 235)),
        ("slateblue", (106, 90, 205)),
        ("slategray", (112, 128, 144)),
        ("snow", (255, 250, 250)),
        ("springgreen", (0, 255, 127)),
        ("steelblue", (70, 130, 180)),
        ("tan", (210, 180, 140)),
        ("thistle", (216, 191, 216)),
        ("tomato", (255, 99, 71)),
        ("turquoise", (64, 224, 208)),
        ("violet", (238, 130, 238)),
        ("wheat", (245, 222, 179)),
        ("whitesmoke", (245, 245, 245)),
        ("yellowgreen", (154, 205, 50)),
    ];
    if name == "transparent" {
        return Some(Color::rgba(0, 0, 0, 0));
    }
    NAMED
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, (r, g, b))| Color::rgb(*r, *g, *b))
}

/// 解析带单位的长度值。
///
/// **拒绝 `NaN` / `inf`**：`f32::parse` 对 `"NaN"` / `"inf"` 会返回非有限值，
/// 一旦落入几何坐标会污染整条布局链（尺寸、分页、绘制坐标全部异常且无 panic）。
/// 此处统一挡掉，等价于该声明无效。
fn parse_length(value: &str) -> Option<CssLength> {
    let value = value.trim();
    if value == "0" {
        return Some(CssLength::Pt(0.0));
    }

    // 单位按长度降序匹配，避免 "em" 命中 "rem" 的后缀（先判 rem 即可）。
    for (suffix, ctor) in [
        ("rem", CssLength::Rem as fn(f32) -> CssLength),
        ("em", CssLength::Em as fn(f32) -> CssLength),
        ("pt", CssLength::Pt as fn(f32) -> CssLength),
        ("px", CssLength::Px as fn(f32) -> CssLength),
        ("%", CssLength::Percent as fn(f32) -> CssLength),
    ] {
        if let Some(stripped) = value.strip_suffix(suffix) {
            return parse_finite_f32(stripped.trim()).map(ctor);
        }
    }

    // 无单位：CSS 中仅 0 合法，但沿用历史行为按 pt 处理。
    parse_finite_f32(value).map(CssLength::Pt)
}

/// 解析 f32，拒绝非有限值（`NaN` / `±inf`）。
fn parse_finite_f32(s: &str) -> Option<f32> {
    let v = s.parse::<f32>().ok()?;
    v.is_finite().then_some(v)
}

fn parse_line_height(value: &str) -> LineHeight {
    let value = value.trim();
    if let Some(multiplier) = parse_finite_f32(value) {
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

/// 解析 CSS `border-*-style` 关键字。
///
/// 仅本引擎能绘制的三种有样式边框有区别；`groove` / `ridge` / `inset` / `outset`
/// 归为 solid（有可见边框），`hidden` 与 `none` 归为 None。
fn parse_border_style(value: &str) -> Option<BorderStyle> {
    match value.trim().to_lowercase().as_str() {
        "none" | "hidden" => Some(BorderStyle::None),
        "solid" | "groove" | "ridge" | "inset" | "outset" => Some(BorderStyle::Solid),
        "dashed" => Some(BorderStyle::Dashed),
        "dotted" => Some(BorderStyle::Dotted),
        _ => None,
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
                            // 复用 1–4 值简写展开（含 3 值 `上/左右/下`），
                            // 与元素级 `margin` 简写行为保持一致。
                            if let Some(s) = parse_sides(&value, parse_side_length) {
                                let r = |len: CssLength| len.resolve(12.0, 12.0);
                                config.margin_top = Some(r(s[0]));
                                config.margin_right = Some(r(s[1]));
                                config.margin_bottom = Some(r(s[2]));
                                config.margin_left = Some(r(s[3]));
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

    // ─── H1：多级后代选择器（组合器 ≥2）匹配方向（2026-09-04 审查） ───

    fn ancestors_from(tags: &[&str]) -> Vec<AncestorInfo> {
        tags.iter()
            .map(|t| AncestorInfo {
                tag: t.to_string(),
                classes: vec![],
                id: None,
            })
            .collect()
    }

    #[test]
    fn test_multi_level_descendant_selector_matches_correct_nesting() {
        let engine = CssEngine::new("div p span { color: #f00; }").unwrap();
        let parent = Style::default();
        // 正确嵌套 <div><p><span>：ancestor_info 从远到近（末位为直接父级）。
        let ancestors = ancestors_from(&["html", "body", "div", "p"]);
        let style = engine.resolve_style("span", &[], None, &ancestors, &parent);
        assert_eq!(style.color.r, 255, "div p span 应命中正确嵌套的 DOM");
    }

    #[test]
    fn test_multi_level_descendant_selector_rejects_inverted_nesting() {
        let engine = CssEngine::new("div p span { color: #f00; }").unwrap();
        let parent = Style::default();
        // 倒置嵌套 <p><div><span>：p 比 div 更远离目标，不满足链序。
        let ancestors = ancestors_from(&["html", "body", "p", "div"]);
        let style = engine.resolve_style("span", &[], None, &ancestors, &parent);
        assert_ne!(style.color.r, 255, "倒置嵌套不得命中 div p span");
        // 缺失祖先段（无 div）也不得命中。
        let ancestors2 = ancestors_from(&["html", "body", "p"]);
        let style2 = engine.resolve_style("span", &[], None, &ancestors2, &parent);
        assert_ne!(style2.color.r, 255);
    }

    #[test]
    fn test_descendant_selector_with_id_and_multi_class_ancestor() {
        let engine = CssEngine::new("#main .note p { color: #0a0; }").unwrap();
        let parent = Style::default();
        // <div id=main><div class="note extra"><p>：id 段 + 多 class 段 + 目标。
        let ancestors = vec![
            AncestorInfo {
                tag: "div".to_string(),
                classes: vec![],
                id: Some("main".to_string()),
            },
            AncestorInfo {
                tag: "div".to_string(),
                classes: vec!["extra".to_string(), "note".to_string()],
                id: None,
            },
        ];
        let style = engine.resolve_style("p", &[], None, &ancestors, &parent);
        assert_eq!(style.color.g, 170, "#0a0 的绿分量应为 0xaa=170");
    }

    // ─── S-4：非有限长度拒绝（2026-09-03 审查） ───

    #[test]
    fn test_parse_length_rejects_non_finite() {
        assert_eq!(parse_length("NaN"), None);
        assert_eq!(parse_length("inf"), None);
        assert_eq!(parse_length("-inf"), None);
        assert_eq!(parse_length("infinity"), None);
        assert_eq!(parse_length("NaNpt"), None);
        assert_eq!(parse_length("infpx"), None);
        // 正常值不受影响
        assert_eq!(parse_length("12.5pt"), Some(CssLength::Pt(12.5)));
        assert_eq!(parse_length("0"), Some(CssLength::Pt(0.0)));
    }

    #[test]
    fn test_parse_line_height_rejects_non_finite() {
        // NaN 行高回退为默认 1.2，而非污染步进几何
        assert!(matches!(
            parse_line_height("NaN"),
            LineHeight::Number(m) if (m - 1.2).abs() < 1e-4
        ));
        assert!(matches!(
            parse_line_height("inf"),
            LineHeight::Number(m) if (m - 1.2).abs() < 1e-4
        ));
    }

    // ─── P1-1：简写展开（2026-09-03 审查） ───

    #[test]
    fn test_margin_shorthand_expansion() {
        let engine = CssEngine::new("p { margin: 10pt 20pt }").unwrap();
        let style = engine.resolve_style("p", &[], None, &[], &Style::default());
        assert_eq!(style.margin.top, 10.0);
        assert_eq!(style.margin.bottom, 10.0);
        assert_eq!(style.margin.left, 20.0);
        assert_eq!(style.margin.right, 20.0);
    }

    #[test]
    fn test_border_shorthand() {
        let engine = CssEngine::new("p { border: 2pt dashed #ff0000 }").unwrap();
        let style = engine.resolve_style("p", &[], None, &[], &Style::default());
        assert_eq!(style.border.top.width, 2.0);
        assert_eq!(style.border.top.style, BorderStyle::Dashed);
        assert_eq!(style.border.top.color, lievisual::Color::rgb(255, 0, 0));
    }

    // ─── P1-2：百分比基准（2026-09-03 审查） ───

    #[test]
    fn test_width_percent_uses_containing_block() {
        let mut engine = CssEngine::new("div { width: 50% }").unwrap();
        engine.set_containing_block_width(400.0);
        let style = engine.resolve_style("div", &[], None, &[], &Style::default());
        assert_eq!(style.width, Some(200.0));
    }

    // ─── P2-3：!important 级联（2026-09-03 审查） ───

    #[test]
    fn test_important_beats_specificity() {
        let engine = CssEngine::new(
            "p { color: red !important }
             .x { color: blue }",
        )
        .unwrap();
        let style = engine.resolve_style("p", &["x".to_string()], None, &[], &Style::default());
        assert_eq!(style.color.r, 255);
        assert_eq!(style.color.g, 0);
    }

    // ─── P2-1：颜色扩展（2026-09-03 审查） ───

    #[test]
    fn test_extended_colors() {
        let engine = CssEngine::new("").unwrap();
        let mut s = Style::default();
        engine.apply_inline_style(&mut s, "color: rgba(255, 0, 0, 0.5)");
        assert_eq!(s.color.r, 255);
        assert_eq!(s.color.a, 128);
        let mut s2 = Style::default();
        engine.apply_inline_style(&mut s2, "color: transparent");
        assert_eq!(s2.color.a, 0);
        let mut s3 = Style::default();
        engine.apply_inline_style(&mut s3, "color: #11223344");
        assert_eq!(s3.color.r, 0x11);
        assert_eq!(s3.color.a, 0x44);
        let mut s4 = Style::default();
        engine.apply_inline_style(&mut s4, "color: navy");
        assert_eq!(s4.color.b, 128);
    }
}
