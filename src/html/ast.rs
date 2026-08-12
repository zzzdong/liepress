//! HTML AST 定义
//!
//! 定义了 LiePress 支持的 HTML 子集的 AST 结构。
//! 提供 DOM 风格的遍历、查询、操作和序列化能力。
//! 构建一个简易的 HTML 引擎核心。

use std::collections::HashMap;

// ─── HTML 文档 ─────────────────────────────────────────────

/// HTML 文档
#[derive(Debug, Clone)]
pub struct HtmlDocument {
    pub root: HtmlElement,
    /// 从 <style> 标签提取的样式表文本
    pub style_sheets: Vec<String>,
}

impl HtmlDocument {
    /// 序列化为 HTML 字符串
    pub fn to_html(&self) -> String {
        let mut s = String::with_capacity(4096);
        s.push_str("<!DOCTYPE html>\n");
        self.root.serialize(&mut s, 0);
        s
    }

    /// 查找第一个匹配标签的元素（深度优先）
    pub fn find(&self, tag: HtmlTag) -> Option<&HtmlElement> {
        self.root.find(tag)
    }

    /// 查找所有匹配标签的元素
    pub fn find_all(&self, tag: HtmlTag) -> Vec<&HtmlElement> {
        self.root.find_all(tag)
    }

    /// 通过 CSS 选择器查找第一个匹配元素（支持 tag, .class, #id）
    pub fn query_selector(&self, selector: &str) -> Option<&HtmlElement> {
        self.root.query_selector(selector)
    }

    /// 通过 CSS 选择器查找所有匹配元素
    pub fn query_selector_all(&self, selector: &str) -> Vec<&HtmlElement> {
        self.root.query_selector_all(selector)
    }
}

// ─── HTML 元素 ─────────────────────────────────────────────

/// HTML 元素
#[derive(Debug, Clone)]
pub struct HtmlElement {
    pub tag: HtmlTag,
    pub attrs: HashMap<String, String>,
    pub children: Vec<HtmlNode>,
}

impl HtmlElement {
    pub fn new(tag: HtmlTag) -> Self {
        Self {
            tag,
            attrs: HashMap::new(),
            children: Vec::new(),
        }
    }

    pub fn with_attr(mut self, key: &str, value: &str) -> Self {
        self.attrs.insert(key.to_string(), value.to_string());
        self
    }

    pub fn with_children(mut self, children: Vec<HtmlNode>) -> Self {
        self.children = children;
        self
    }

    // ── 属性操作 ──

    /// 获取 class 属性值
    pub fn classes(&self) -> Vec<String> {
        self.attrs
            .get("class")
            .map(|c| c.split_whitespace().map(|s| s.to_string()).collect())
            .unwrap_or_default()
    }

    /// 获取 style 属性值
    pub fn inline_style(&self) -> Option<&str> {
        self.attrs.get("style").map(|x| x.as_str())
    }

    /// 获取 id 属性值
    pub fn id(&self) -> Option<&str> {
        self.attrs.get("id").map(|s| s.as_str())
    }

    /// 判断是否包含指定 class
    pub fn has_class(&self, class: &str) -> bool {
        self.attrs
            .get("class")
            .map(|c| c.split_whitespace().any(|s| s == class))
            .unwrap_or(false)
    }

    /// 判断是否包含指定属性
    pub fn has_attribute(&self, key: &str) -> bool {
        self.attrs.contains_key(key)
    }

    /// 获取属性值
    pub fn get_attribute(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(|s| s.as_str())
    }

    /// 设置属性值
    pub fn set_attribute(&mut self, key: &str, value: &str) {
        self.attrs.insert(key.to_string(), value.to_string());
    }

    /// 移除属性
    pub fn remove_attribute(&mut self, key: &str) {
        self.attrs.remove(key);
    }

    // ── 树遍历 ──

    /// 获取元素中的所有文本内容
    pub fn text_content(&self) -> String {
        let mut s = String::new();
        for child in &self.children {
            s.push_str(&child.text_content());
        }
        s
    }

    /// 子元素数量
    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    /// 是否没有子节点
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// 获取第一个子节点
    pub fn first_child(&self) -> Option<&HtmlNode> {
        self.children.first()
    }

    /// 获取最后一个子节点
    pub fn last_child(&self) -> Option<&HtmlNode> {
        self.children.last()
    }

    /// 获取第 n 个子节点（从 0 开始）
    pub fn nth_child(&self, index: usize) -> Option<&HtmlNode> {
        self.children.get(index)
    }

    /// 查找第一个匹配标签的后代元素（深度优先）
    pub fn find(&self, tag: HtmlTag) -> Option<&HtmlElement> {
        if self.tag == tag {
            return Some(self);
        }
        for child in &self.children {
            if let HtmlNode::Element(elem) = child
                && let Some(found) = elem.find(tag)
            {
                return Some(found);
            }
        }
        None
    }

    /// 查找所有匹配标签的后代元素
    pub fn find_all(&self, tag: HtmlTag) -> Vec<&HtmlElement> {
        let mut result = Vec::new();
        self.collect_tag(tag, &mut result);
        result
    }

    fn collect_tag<'a>(&'a self, tag: HtmlTag, result: &mut Vec<&'a HtmlElement>) {
        if self.tag == tag {
            result.push(self);
        }
        for child in &self.children {
            if let HtmlNode::Element(elem) = child {
                elem.collect_tag(tag, result);
            }
        }
    }

    /// 通过简单 CSS 选择器查找第一个匹配元素
    ///
    /// 支持：
    /// - `tag`：标签名（如 `div`、`p`）
    /// - `.class`：类选择器
    /// - `#id`：ID 选择器
    /// - `tag.class` / `tag#id`：组合选择器
    ///
    /// 不支持：属性选择器、伪类、后代/子代组合器
    pub fn query_selector(&self, selector: &str) -> Option<&HtmlElement> {
        let selector = selector.trim();
        if self.matches_simple_selector(selector) {
            return Some(self);
        }
        for child in &self.children {
            if let HtmlNode::Element(elem) = child
                && let Some(found) = elem.query_selector(selector)
            {
                return Some(found);
            }
        }
        None
    }

    /// 通过简单 CSS 选择器查找所有匹配元素
    pub fn query_selector_all(&self, selector: &str) -> Vec<&HtmlElement> {
        let mut result = Vec::new();
        let selector = selector.trim();
        self.collect_by_selector(selector, &mut result);
        result
    }

    fn collect_by_selector<'a>(&'a self, selector: &str, result: &mut Vec<&'a HtmlElement>) {
        if self.matches_simple_selector(selector) {
            result.push(self);
        }
        for child in &self.children {
            if let HtmlNode::Element(elem) = child {
                elem.collect_by_selector(selector, result);
            }
        }
    }

    /// 判断元素是否匹配简单选择器
    fn matches_simple_selector(&self, selector: &str) -> bool {
        let selector = selector.trim();
        if selector.is_empty() {
            return false;
        }

        // 解析选择器：tag, .class, #id, tag.class, tag#id, tag#id.class, tag.class#id
        let mut tag_part: Option<&str> = None;
        let mut class_part: Option<&str> = None;
        let mut id_part: Option<&str> = None;

        let mut rest = selector;

        // 先尝试解析 tag 部分（开头到第一个 . 或 #）
        let tag_end = rest.find(['.', '#']).unwrap_or(rest.len());
        if tag_end > 0 {
            tag_part = Some(&rest[..tag_end]);
            rest = &rest[tag_end..];
        }

        // 解析剩余的 .class 和 #id 部分
        while !rest.is_empty() {
            if let Some(dot_pos) = rest.find('.') {
                let cls_end = rest[dot_pos + 1..]
                    .find(['.', '#'])
                    .map(|i| dot_pos + 1 + i)
                    .unwrap_or(rest.len());
                class_part = Some(&rest[dot_pos + 1..cls_end]);
                rest = &rest[cls_end..];
            } else if let Some(hash_pos) = rest.find('#') {
                let id_end = rest[hash_pos + 1..]
                    .find(['.', '#'])
                    .map(|i| hash_pos + 1 + i)
                    .unwrap_or(rest.len());
                id_part = Some(&rest[hash_pos + 1..id_end]);
                rest = &rest[id_end..];
            } else {
                break;
            }
        }

        // 匹配标签
        if let Some(tag) = tag_part
            && self.tag.as_str() != tag
        {
            return false;
        }

        // 匹配 ID
        if let Some(id) = id_part
            && self.id() != Some(id)
        {
            return false;
        }

        // 匹配 class
        if let Some(class) = class_part
            && !self.has_class(class)
        {
            return false;
        }

        true
    }

    // ── 树操作 ──

    /// 追加子节点
    pub fn append_child(&mut self, child: HtmlNode) {
        self.children.push(child);
    }

    /// 在开头插入子节点
    pub fn prepend_child(&mut self, child: HtmlNode) {
        self.children.insert(0, child);
    }

    /// 移除指定索引的子节点，返回被移除的节点
    pub fn remove_child(&mut self, index: usize) -> Option<HtmlNode> {
        if index < self.children.len() {
            Some(self.children.remove(index))
        } else {
            None
        }
    }

    /// 替换指定索引的子节点
    pub fn replace_child(&mut self, index: usize, child: HtmlNode) -> Option<HtmlNode> {
        if index < self.children.len() {
            Some(std::mem::replace(&mut self.children[index], child))
        } else {
            None
        }
    }

    /// 在指定索引前插入子节点
    pub fn insert_before(&mut self, index: usize, child: HtmlNode) {
        let index = index.min(self.children.len());
        self.children.insert(index, child);
    }

    /// 在指定索引后插入子节点
    pub fn insert_after(&mut self, index: usize, child: HtmlNode) {
        let index = (index + 1).min(self.children.len());
        self.children.insert(index, child);
    }

    /// 清空所有子节点
    pub fn clear_children(&mut self) {
        self.children.clear();
    }

    // ── 序列化 ──

    /// 将元素序列化为 HTML 字符串
    pub fn to_html(&self) -> String {
        let mut s = String::with_capacity(1024);
        self.serialize(&mut s, 0);
        s
    }

    fn serialize(&self, output: &mut String, depth: usize) {
        let tag_name = self.tag.as_str();

        output.push('<');
        output.push_str(tag_name);

        // 序列化属性
        let mut attrs: Vec<(&String, &String)> = self.attrs.iter().collect();
        attrs.sort_by(|a, b| a.0.cmp(b.0));
        for (key, val) in &attrs {
            output.push(' ');
            output.push_str(key);
            output.push_str("=\"");
            output.push_str(&escape_html(val));
            output.push('"');
        }

        if self.tag.is_void() {
            output.push_str(">\n");
            return;
        }

        output.push('>');

        if self.children.is_empty() {
            output.push_str(&format!("</{}>", tag_name));
            return;
        }

        // 判断是否所有子节点都是文本节点（行内内容不换行）
        let all_text = self.children.iter().all(|c| matches!(c, HtmlNode::Text(_)));
        let inline_tag = self.tag.is_inline();

        if all_text || inline_tag {
            for child in &self.children {
                child.serialize(output);
            }
            output.push_str(&format!("</{}>", tag_name));
        } else {
            output.push('\n');
            for child in &self.children {
                child.serialize_with_indent(output, depth + 1);
            }
            // 缩进结束标签
            for _ in 0..depth {
                output.push_str("  ");
            }
            output.push_str(&format!("</{}>\n", tag_name));
        }
    }
}

// ─── HTML 节点 ─────────────────────────────────────────────

/// HTML 节点（元素或文本）
#[derive(Debug, Clone)]
pub enum HtmlNode {
    Element(HtmlElement),
    Text(String),
}

impl HtmlNode {
    pub fn text(text: impl Into<String>) -> Self {
        HtmlNode::Text(text.into())
    }

    pub fn element(elem: HtmlElement) -> Self {
        HtmlNode::Element(elem)
    }

    /// 获取节点中的所有文本内容
    pub fn text_content(&self) -> String {
        match self {
            HtmlNode::Text(s) => s.clone(),
            HtmlNode::Element(elem) => {
                let mut s = String::new();
                for child in &elem.children {
                    s.push_str(&child.text_content());
                }
                s
            }
        }
    }

    /// 判断是否为元素节点
    pub fn is_element(&self) -> bool {
        matches!(self, HtmlNode::Element(_))
    }

    /// 判断是否为文本节点
    pub fn is_text(&self) -> bool {
        matches!(self, HtmlNode::Text(_))
    }

    /// 如果是元素节点，返回引用
    pub fn as_element(&self) -> Option<&HtmlElement> {
        match self {
            HtmlNode::Element(e) => Some(e),
            _ => None,
        }
    }

    /// 如果是元素节点，返回可变引用
    pub fn as_element_mut(&mut self) -> Option<&mut HtmlElement> {
        match self {
            HtmlNode::Element(e) => Some(e),
            _ => None,
        }
    }

    /// 如果是文本节点，返回文本内容
    pub fn as_text(&self) -> Option<&str> {
        match self {
            HtmlNode::Text(s) => Some(s.as_str()),
            _ => None,
        }
    }

    // ── 序列化 ──

    /// 序列化为 HTML 字符串（无缩进）
    fn serialize(&self, output: &mut String) {
        match self {
            HtmlNode::Element(elem) => elem.serialize(output, 0),
            HtmlNode::Text(text) => output.push_str(&escape_html(text)),
        }
    }

    /// 带缩进序列化
    fn serialize_with_indent(&self, output: &mut String, depth: usize) {
        match self {
            HtmlNode::Element(elem) => {
                for _ in 0..depth {
                    output.push_str("  ");
                }
                elem.serialize(output, depth);
            }
            HtmlNode::Text(text) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    for _ in 0..depth {
                        output.push_str("  ");
                    }
                    output.push_str(&escape_html(trimmed));
                    output.push('\n');
                }
            }
        }
    }

    /// 转换为带缩进的 HTML 字符串
    pub fn to_html(&self) -> String {
        match self {
            HtmlNode::Element(elem) => elem.to_html(),
            HtmlNode::Text(text) => escape_html(text),
        }
    }
}

// ─── HTML 标签枚举 ─────────────────────────────────────────

/// HTML 标签枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HtmlTag {
    // 文档结构
    Html,
    Head,
    Body,
    Div,
    Span,
    Center,

    // HTML5 语义结构
    Section,
    Article,
    Nav,
    Aside,
    Header,
    Footer,
    Main,

    // 标题
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,

    // 段落与分组
    P,
    Br,
    Hr,
    Pre,

    // 文本样式
    Strong,
    B,
    Em,
    I,
    U,
    Del,
    S,
    Mark,
    Small,
    Sub,
    Sup,

    // 链接与媒体
    A,
    Img,
    Figure,
    Figcaption,

    // 列表
    Ul,
    Ol,
    Li,

    // 代码
    Code,

    // 引用
    Blockquote,

    // 表格
    Table,
    Thead,
    Tbody,
    Tr,
    Th,
    Td,

    // 样式
    Style,

    // 输入
    Input,

    // 未知标签
    Unknown,
}

impl HtmlTag {
    pub fn as_str(&self) -> &'static str {
        match self {
            HtmlTag::Html => "html",
            HtmlTag::Head => "head",
            HtmlTag::Body => "body",
            HtmlTag::Div => "div",
            HtmlTag::Span => "span",
            HtmlTag::Center => "center",
            HtmlTag::Section => "section",
            HtmlTag::Article => "article",
            HtmlTag::Nav => "nav",
            HtmlTag::Aside => "aside",
            HtmlTag::Header => "header",
            HtmlTag::Footer => "footer",
            HtmlTag::Main => "main",
            HtmlTag::H1 => "h1",
            HtmlTag::H2 => "h2",
            HtmlTag::H3 => "h3",
            HtmlTag::H4 => "h4",
            HtmlTag::H5 => "h5",
            HtmlTag::H6 => "h6",
            HtmlTag::P => "p",
            HtmlTag::Br => "br",
            HtmlTag::Hr => "hr",
            HtmlTag::Pre => "pre",
            HtmlTag::Strong => "strong",
            HtmlTag::B => "b",
            HtmlTag::Em => "em",
            HtmlTag::I => "i",
            HtmlTag::U => "u",
            HtmlTag::Del => "del",
            HtmlTag::S => "s",
            HtmlTag::Mark => "mark",
            HtmlTag::Small => "small",
            HtmlTag::Sub => "sub",
            HtmlTag::Sup => "sup",
            HtmlTag::A => "a",
            HtmlTag::Img => "img",
            HtmlTag::Figure => "figure",
            HtmlTag::Figcaption => "figcaption",
            HtmlTag::Ul => "ul",
            HtmlTag::Ol => "ol",
            HtmlTag::Li => "li",
            HtmlTag::Code => "code",
            HtmlTag::Blockquote => "blockquote",
            HtmlTag::Table => "table",
            HtmlTag::Thead => "thead",
            HtmlTag::Tbody => "tbody",
            HtmlTag::Tr => "tr",
            HtmlTag::Th => "th",
            HtmlTag::Td => "td",
            HtmlTag::Style => "style",
            HtmlTag::Input => "input",
            HtmlTag::Unknown => "unknown",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "html" => HtmlTag::Html,
            "head" => HtmlTag::Head,
            "body" => HtmlTag::Body,
            "div" => HtmlTag::Div,
            "span" => HtmlTag::Span,
            "center" => HtmlTag::Center,
            "section" => HtmlTag::Section,
            "article" => HtmlTag::Article,
            "nav" => HtmlTag::Nav,
            "aside" => HtmlTag::Aside,
            "header" => HtmlTag::Header,
            "footer" => HtmlTag::Footer,
            "main" => HtmlTag::Main,
            "h1" => HtmlTag::H1,
            "h2" => HtmlTag::H2,
            "h3" => HtmlTag::H3,
            "h4" => HtmlTag::H4,
            "h5" => HtmlTag::H5,
            "h6" => HtmlTag::H6,
            "p" => HtmlTag::P,
            "br" => HtmlTag::Br,
            "hr" => HtmlTag::Hr,
            "pre" => HtmlTag::Pre,
            "strong" => HtmlTag::Strong,
            "b" => HtmlTag::B,
            "em" => HtmlTag::Em,
            "i" => HtmlTag::I,
            "u" => HtmlTag::U,
            "del" => HtmlTag::Del,
            "s" => HtmlTag::S,
            "mark" => HtmlTag::Mark,
            "small" => HtmlTag::Small,
            "sub" => HtmlTag::Sub,
            "sup" => HtmlTag::Sup,
            "a" => HtmlTag::A,
            "img" => HtmlTag::Img,
            "figure" => HtmlTag::Figure,
            "figcaption" => HtmlTag::Figcaption,
            "ul" => HtmlTag::Ul,
            "ol" => HtmlTag::Ol,
            "li" => HtmlTag::Li,
            "code" => HtmlTag::Code,
            "blockquote" => HtmlTag::Blockquote,
            "table" => HtmlTag::Table,
            "thead" => HtmlTag::Thead,
            "tbody" => HtmlTag::Tbody,
            "tr" => HtmlTag::Tr,
            "th" => HtmlTag::Th,
            "td" => HtmlTag::Td,
            "style" => HtmlTag::Style,
            "input" => HtmlTag::Input,
            _ => HtmlTag::Unknown,
        }
    }

    /// 判断是否为块级元素
    pub fn is_block(&self) -> bool {
        matches!(
            self,
            HtmlTag::Html
                | HtmlTag::Head
                | HtmlTag::Body
                | HtmlTag::Div
                | HtmlTag::Center
                | HtmlTag::Section
                | HtmlTag::Article
                | HtmlTag::Nav
                | HtmlTag::Aside
                | HtmlTag::Header
                | HtmlTag::Footer
                | HtmlTag::Main
                | HtmlTag::H1
                | HtmlTag::H2
                | HtmlTag::H3
                | HtmlTag::H4
                | HtmlTag::H5
                | HtmlTag::H6
                | HtmlTag::P
                | HtmlTag::Pre
                | HtmlTag::Ul
                | HtmlTag::Ol
                | HtmlTag::Li
                | HtmlTag::Blockquote
                | HtmlTag::Hr
                | HtmlTag::Table
                | HtmlTag::Thead
                | HtmlTag::Tbody
                | HtmlTag::Tr
                | HtmlTag::Th
                | HtmlTag::Td
                | HtmlTag::Figure
                | HtmlTag::Figcaption
        )
    }

    /// 判断是否为行内元素
    pub fn is_inline(&self) -> bool {
        !self.is_block() && !self.is_void()
    }

    /// 判断是否为空元素（无子内容）
    pub fn is_void(&self) -> bool {
        matches!(
            self,
            HtmlTag::Br | HtmlTag::Hr | HtmlTag::Img | HtmlTag::Input
        )
    }

    /// 判断是否为已知支持的标签
    pub fn is_supported(&self) -> bool {
        !matches!(self, HtmlTag::Unknown)
    }

    /// 获取所有已知标签名列表
    pub fn known_tags() -> &'static [&'static str] {
        &[
            "html",
            "head",
            "body",
            "div",
            "span",
            "center",
            "section",
            "article",
            "nav",
            "aside",
            "header",
            "footer",
            "main",
            "h1",
            "h2",
            "h3",
            "h4",
            "h5",
            "h6",
            "p",
            "br",
            "hr",
            "pre",
            "strong",
            "b",
            "em",
            "i",
            "u",
            "del",
            "s",
            "mark",
            "small",
            "sub",
            "sup",
            "a",
            "img",
            "figure",
            "figcaption",
            "ul",
            "ol",
            "li",
            "code",
            "blockquote",
            "table",
            "thead",
            "tbody",
            "tr",
            "th",
            "td",
            "style",
            "input",
        ]
    }
}

// ─── HTML 工具函数 ─────────────────────────────────────────

/// 转义 HTML 特殊字符
pub fn escape_html(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            _ => result.push(ch),
        }
    }
    result
}

/// 反转义 HTML 实体
pub fn unescape_html(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '&' {
            let mut entity = String::new();
            for ch in &mut chars {
                if ch == ';' {
                    break;
                }
                entity.push(ch);
            }
            let decoded: String = match entity.as_str() {
                "amp" => "&".into(),
                "lt" => "<".into(),
                "gt" => ">".into(),
                "quot" => "\"".into(),
                "#39" | "apos" => "'".into(),
                "nbsp" => "\u{00A0}".into(),
                "ndash" => "\u{2013}".into(),
                "mdash" => "\u{2014}".into(),
                "ldquo" => "\u{201C}".into(),
                "rdquo" => "\u{201D}".into(),
                "lsquo" => "\u{2018}".into(),
                "rsquo" => "\u{2019}".into(),
                "laquo" => "\u{00AB}".into(),
                "raquo" => "\u{00BB}".into(),
                "copy" => "\u{00A9}".into(),
                "reg" => "\u{00AE}".into(),
                "trade" => "\u{2122}".into(),
                "bull" => "\u{2022}".into(),
                "hellip" => "\u{2026}".into(),
                "deg" => "\u{00B0}".into(),
                "plusmn" => "\u{00B1}".into(),
                "times" => "\u{00D7}".into(),
                "divide" => "\u{00F7}".into(),
                "frac14" => "\u{00BC}".into(),
                "frac12" => "\u{00BD}".into(),
                "frac34" => "\u{00BE}".into(),
                // 数字实体 &#NNN;
                _ => {
                    if let Some(num_str) = entity.strip_prefix('#') {
                        if let Ok(code) = num_str.parse::<u32>() {
                            if let Some(c) = char::from_u32(code) {
                                let mut buf = [0u8; 4];
                                let s = c.encode_utf8(&mut buf);
                                s.to_string()
                            } else {
                                format!("&{};", entity)
                            }
                        } else {
                            format!("&{};", entity)
                        }
                    } else {
                        format!("&{};", entity)
                    }
                }
            };
            result.push_str(&decoded);
        } else {
            result.push(ch);
        }
    }

    result
}

/// 折叠空白：将连续的空白字符合并为单个空格。
///
/// CSS `white-space: normal` 语义下，折叠只发生在文本节点内部；
/// **边界单空格必须保留**，因为它用于与相邻节点的空白跨边界合并
/// （如 `Hello` + `<b> world </b>` 的分词）。首/尾孤立的折叠空格
/// 是否丢弃由文本流级处理决定，不应在本函数内 trim。
///
/// `\u{00A0}`（`&nbsp;`）不属于 ASCII 空白，保持不变。
pub fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_space = false;
    for ch in s.chars() {
        if ch.is_ascii_whitespace() {
            if !in_space {
                result.push(' ');
                in_space = true;
            }
        } else {
            result.push(ch);
            in_space = false;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("<script>"), "&lt;script&gt;");
        assert_eq!(escape_html("hello & world"), "hello &amp; world");
        assert_eq!(escape_html("a\"b"), "a&quot;b");
    }

    #[test]
    fn test_unescape_html() {
        assert_eq!(unescape_html("&lt;tag&gt;"), "<tag>");
        assert_eq!(unescape_html("&amp;"), "&");
        assert_eq!(unescape_html("&#60;"), "<");
        assert_eq!(unescape_html("hello &amp; world"), "hello & world");
    }

    #[test]
    fn test_collapse_whitespace() {
        assert_eq!(collapse_whitespace("hello   world"), "hello world");
        // 边界单空格保留（CSS 折叠语义），由文本流级决定是否丢弃
        assert_eq!(collapse_whitespace("  leading"), " leading");
        assert_eq!(collapse_whitespace("trailing  "), "trailing ");
        assert_eq!(collapse_whitespace("hello\nworld"), "hello world");
        // &nbsp; (U+00A0) 不被折叠/删除
        assert_eq!(collapse_whitespace("a\u{00A0}\u{00A0}b"), "a\u{00A0}\u{00A0}b");
    }

    #[test]
    fn test_element_tree_operations() {
        let mut parent = HtmlElement::new(HtmlTag::Div);
        let child1 = HtmlNode::text("hello");
        let child2 = HtmlNode::text("world");

        parent.append_child(child1);
        parent.append_child(child2.clone());
        assert_eq!(parent.child_count(), 2);

        let removed = parent.remove_child(0);
        assert!(removed.is_some());
        assert_eq!(parent.child_count(), 1);

        parent.prepend_child(removed.unwrap());
        assert_eq!(parent.child_count(), 2);

        parent.replace_child(1, child2);
        assert_eq!(parent.child_count(), 2);
    }

    #[test]
    fn test_query_selector() {
        let html = r#"<div id="main" class="container"><p class="text">Hello</p></div>"#;
        let doc = crate::html::parser::parse_html(html);

        // 按标签查找
        let found = doc.root.query_selector("p");
        assert!(found.is_some());
        assert_eq!(found.unwrap().tag, HtmlTag::P);

        // 按 class 查找
        let found = doc.root.query_selector(".text");
        assert!(found.is_some());
        assert_eq!(found.unwrap().tag, HtmlTag::P);

        // 按 ID 查找
        let found = doc.root.query_selector("#main");
        assert!(found.is_some());
        assert_eq!(found.unwrap().tag, HtmlTag::Div);

        // 组合选择器
        let found = doc.root.query_selector("div.container");
        assert!(found.is_some());
    }

    #[test]
    fn test_html_serialization() {
        use crate::html::parser::parse_html;
        let html = r#"<div class="foo"><p>Hello <strong>world</strong></p></div>"#;
        let doc = parse_html(html);

        let serialized = doc.to_html();
        // 序列化结果应包含原始语义
        assert!(serialized.contains("<div"));
        assert!(serialized.contains("class=\"foo\""));
        assert!(serialized.contains("<p>"));
        assert!(serialized.contains("<strong>"));
        assert!(serialized.contains("Hello"));
        assert!(serialized.contains("world"));
    }

    #[test]
    fn test_matches_simple_selector() {
        let mut elem = HtmlElement::new(HtmlTag::Div);
        elem.set_attribute("id", "myid");
        elem.set_attribute("class", "foo bar");

        assert!(elem.matches_simple_selector("div"));
        assert!(elem.matches_simple_selector("#myid"));
        assert!(elem.matches_simple_selector(".foo"));
        assert!(elem.matches_simple_selector(".bar"));
        assert!(elem.matches_simple_selector("div#myid"));
        assert!(elem.matches_simple_selector("div.foo"));
        assert!(elem.matches_simple_selector("div#myid.foo"));
        assert!(!elem.matches_simple_selector("span"));
        assert!(!elem.matches_simple_selector("#other"));
        assert!(!elem.matches_simple_selector(".baz"));
    }

    #[test]
    fn test_find_all() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li></ul>"#;
        let doc = crate::html::parser::parse_html(html);
        let lis = doc.root.find_all(HtmlTag::Li);
        assert_eq!(lis.len(), 3);
    }

    #[test]
    fn test_known_tags() {
        let tags = HtmlTag::known_tags();
        assert!(tags.contains(&"div"));
        assert!(tags.contains(&"section"));
        assert!(tags.contains(&"mark"));
        assert!(tags.contains(&"article"));
    }

    #[test]
    fn test_is_void() {
        assert!(HtmlTag::Br.is_void());
        assert!(HtmlTag::Hr.is_void());
        assert!(HtmlTag::Img.is_void());
        assert!(HtmlTag::Input.is_void());
        assert!(!HtmlTag::Div.is_void());
    }

    #[test]
    fn test_insert_before_after() {
        let mut parent = HtmlElement::new(HtmlTag::Div);
        parent.append_child(HtmlNode::text("B"));
        parent.insert_before(0, HtmlNode::text("A"));
        parent.insert_after(1, HtmlNode::text("C"));

        assert_eq!(parent.child_count(), 3);
        assert_eq!(parent.nth_child(0).and_then(|n| n.as_text()), Some("A"));
        assert_eq!(parent.nth_child(1).and_then(|n| n.as_text()), Some("B"));
        assert_eq!(parent.nth_child(2).and_then(|n| n.as_text()), Some("C"));
    }
}
