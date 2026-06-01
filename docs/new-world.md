# LiePress 架构重构设计方案

## HTML 子集 + Lightning CSS 样式增强

---

## 目录

1. [架构总览](#1-架构总览)
2. [HTML 子集定义](#2-html-子集定义)
3. [模块设计](#3-模块设计)
4. [CSS 样式系统](#4-css-样式系统)
5. [数据流与处理流程](#5-数据流与处理流程)
6. [实现规划](#6-实现规划)

---

## 1. 架构总览

### 1.1 新旧架构对比

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              当前架构                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Markdown ──▶ MDAST ──▶ Styled AST ──▶ Document ──▶ PDF/SVG/PNG            │
│              (markdown)   (自定义)      (generator)     (render)            │
│                                                                             │
│  问题：                                                                      │
│  - HTML 标签处理薄弱（仅提取 <style>）                                        │
│  - CSS 解析器功能有限（基础选择器）                                           │
│  - 样式系统与 HTML 标准脱节                                                   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│                              新架构                                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐  │
│  │ Markdown │──▶│   HTML   │──▶│  Styled  │──▶│ Document │──▶│  Output  │  │
│  │  Source  │   │   AST    │   │   HTML   │   │  Layout  │   │ PDF/...  │  │
│  └──────────┘   └──────────┘   └──────────┘   └──────────┘   └──────────┘  │
│       │              │               │              │                       │
│       ▼              ▼               ▼              ▼                       │
│  ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐                  │
│  │ markdown │   │  html    │   │   css    │   │ generator│                  │
│  │  crate   │   │  parser  │   │ (lightning│   │  module  │                  │
│  │          │   │  module  │   │   css)   │   │          │                  │
│  └──────────┘   └──────────┘   └──────────┘   └──────────┘                  │
│                                                                             │
│  优势：                                                                      │
│  - 明确的 HTML 子集支持                                                       │
│  - 浏览器级 CSS 解析（Lightning CSS）                                         │
│  - 标准 HTML + CSS 处理流程                                                   │
│  - 更好的可维护性和扩展性                                                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 模块依赖关系

```
┌─────────────────────────────────────────────────────────────────┐
│                         模块依赖图                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│                         ┌─────────┐                             │
│                         │  lib.rs │  (公共 API)                  │
│                         └────┬────┘                             │
│                              │                                  │
│        ┌─────────────────────┼─────────────────────┐            │
│        │                     │                     │            │
│        ▼                     ▼                     ▼            │
│   ┌─────────┐          ┌─────────┐           ┌─────────┐        │
│   │  html   │◄────────►│   css   │           │ render  │        │
│   │ module  │          │ module  │           │ module  │        │
│   └────┬────┘          └────┬────┘           └────┬────┘        │
│        │                     │                     │            │
│        │              ┌──────┴──────┐              │            │
│        │              │             │              │            │
│        │              ▼             ▼              │            │
│        │        ┌─────────┐   ┌─────────┐         │            │
│        │        │lightning│   │ adapter │         │            │
│        │        │  css    │   │  layer  │         │            │
│        │        └─────────┘   └─────────┘         │            │
│        │                                           │            │
│        └──────────────────┬────────────────────────┘            │
│                           │                                     │
│                           ▼                                     │
│                      ┌─────────┐                                │
│                      │generator│                                │
│                      │ module  │                                │
│                      └─────────┘                                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. HTML 子集定义

### 2.1 支持的 HTML 标签清单

#### 2.1.1 文档结构

| 标签 | 类型 | 属性支持 | 说明 |
|------|------|---------|------|
| `<div>` | 块级 | `style`, `class`, `id` | 通用容器 |
| `<span>` | 行内 | `style`, `class`, `id` | 行内容器 |
| `<center>` | 块级 | `style`, `class` | 居中容器（兼容旧文档） |

#### 2.1.2 标题

| 标签 | 类型 | 属性支持 | 说明 |
|------|------|---------|------|
| `<h1>` - `<h6>` | 块级 | `style`, `class`, `id` | 标题，1-6 级 |

#### 2.1.3 段落与文本

| 标签 | 类型 | 属性支持 | 说明 |
|------|------|---------|------|
| `<p>` | 块级 | `style`, `class`, `id` | 段落 |
| `<br/>` | 空行内 | - | 强制换行 |
| 文本节点 | - | - | 纯文本内容 |

#### 2.1.4 文本样式（内联）

| 标签 | 类型 | 属性支持 | 说明 |
|------|------|---------|------|
| `<strong>`, `<b>` | 行内 | `style`, `class` | 加粗 |
| `<em>`, `<i>` | 行内 | `style`, `class` | 斜体 |
| `<u>` | 行内 | `style`, `class` | 下划线 |
| `<del>`, `<s>` | 行内 | `style`, `class` | 删除线 |
| `<mark>` | 行内 | `style`, `class` | 高亮标记 |
| `<kbd>` | 行内 | `style`, `class` | 键盘按键 |
| `<sup>` | 行内 | `style`, `class` | 上标 |
| `<sub>` | 行内 | `style`, `class` | 下标 |

#### 2.1.5 链接与图片

| 标签 | 类型 | 属性支持 | 说明 |
|------|------|---------|------|
| `<a>` | 行内 | `href`, `title`, `style`, `class` | 超链接 |
| `<img>` | 空行内 | `src`, `alt`, `title`, `width`, `height`, `style`, `class` | 图片 |

#### 2.1.6 列表

| 标签 | 类型 | 属性支持 | 说明 |
|------|------|---------|------|
| `<ul>` | 块级 | `style`, `class` | 无序列表 |
| `<ol>` | 块级 | `start`, `style`, `class` | 有序列表 |
| `<li>` | 块级 | `style`, `class` | 列表项 |

#### 2.1.7 代码

| 标签 | 类型 | 属性支持 | 说明 |
|------|------|---------|------|
| `<pre>` + `<code>` | 块级 | `class` (语言) | 代码块 |
| `<code>` | 行内 | `style`, `class` | 行内代码 |

#### 2.1.8 其他块级元素

| 标签 | 类型 | 属性支持 | 说明 |
|------|------|---------|------|
| `<blockquote>` | 块级 | `style`, `class`, `id` | 引用块 |
| `<hr/>` | 空块级 | `style`, `class` | 水平分隔线 |

#### 2.1.9 表格

| 标签 | 类型 | 属性支持 | 说明 |
|------|------|---------|------|
| `<table>` | 块级 | `style`, `class` | 表格 |
| `<thead>` | 块级 | `style`, `class` | 表头区域 |
| `<tbody>` | 块级 | `style`, `class` | 表体区域 |
| `<tr>` | 块级 | `style`, `class` | 表格行 |
| `<th>` | 块级 | `style`, `class`, `colspan`, `rowspan` | 表头单元格 |
| `<td>` | 块级 | `style`, `class`, `colspan`, `rowspan` | 数据单元格 |

### 2.2 不支持的 HTML 标签

| 类别 | 标签 | 原因 |
|------|------|------|
| 表单 | `<form>`, `<input>`, `<button>` 等 | PDF 非交互式文档 |
| 多媒体 | `<video>`, `<audio>`, `<canvas>` | 需要复杂渲染支持 |
| 框架 | `<iframe>`, `<frame>`, `<frameset>` | 安全性和复杂性 |
| 脚本 | `<script>`, `<noscript>` | PDF 不支持脚本 |
| 元数据 | `<meta>`, `<link>` (除样式) | 无意义 |
| 复杂布局 | `<article>`, `<section>`, `<aside>` 等语义标签 | 可用 `<div>` 替代 |

### 2.3 HTML AST 定义

```rust
// src/html/ast.rs

use std::collections::HashMap;

/// HTML 文档
#[derive(Debug, Clone)]
pub struct HtmlDocument {
    pub root: HtmlElement,
    /// 从 <style> 标签提取的样式表
    pub style_sheets: Vec<String>,
}

/// HTML 元素
#[derive(Debug, Clone)]
pub struct HtmlElement {
    pub tag: HtmlTag,
    pub attrs: HashMap<String, String>,
    pub children: Vec<HtmlNode>,
}

/// HTML 节点（元素或文本）
#[derive(Debug, Clone)]
pub enum HtmlNode {
    Element(HtmlElement),
    Text(String),
}

/// HTML 标签枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HtmlTag {
    // 文档结构
    Div,
    Span,
    Center,
    
    // 标题
    H1, H2, H3, H4, H5, H6,
    
    // 段落
    P,
    Br,
    
    // 文本样式
    Strong, B,
    Em, I,
    U,
    Del, S,
    Mark,
    Kbd,
    Sup,
    Sub,
    
    // 链接与图片
    A,
    Img,
    
    // 列表
    Ul, Ol, Li,
    
    // 代码
    Pre, Code,
    
    // 其他块级
    Blockquote,
    Hr,
    
    // 表格
    Table, Thead, Tbody, Tr, Th, Td,
}

impl HtmlTag {
    /// 获取标签字符串表示
    pub fn as_str(&self) -> &'static str {
        match self {
            HtmlTag::Div => "div",
            HtmlTag::Span => "span",
            HtmlTag::Center => "center",
            HtmlTag::H1 => "h1",
            HtmlTag::H2 => "h2",
            HtmlTag::H3 => "h3",
            HtmlTag::H4 => "h4",
            HtmlTag::H5 => "h5",
            HtmlTag::H6 => "h6",
            HtmlTag::P => "p",
            HtmlTag::Br => "br",
            HtmlTag::Strong => "strong",
            HtmlTag::B => "b",
            HtmlTag::Em => "em",
            HtmlTag::I => "i",
            HtmlTag::U => "u",
            HtmlTag::Del => "del",
            HtmlTag::S => "s",
            HtmlTag::Mark => "mark",
            HtmlTag::Kbd => "kbd",
            HtmlTag::Sup => "sup",
            HtmlTag::Sub => "sub",
            HtmlTag::A => "a",
            HtmlTag::Img => "img",
            HtmlTag::Ul => "ul",
            HtmlTag::Ol => "ol",
            HtmlTag::Li => "li",
            HtmlTag::Pre => "pre",
            HtmlTag::Code => "code",
            HtmlTag::Blockquote => "blockquote",
            HtmlTag::Hr => "hr",
            HtmlTag::Table => "table",
            HtmlTag::Thead => "thead",
            HtmlTag::Tbody => "tbody",
            HtmlTag::Tr => "tr",
            HtmlTag::Th => "th",
            HtmlTag::Td => "td",
        }
    }
    
    /// 从字符串解析标签
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "div" => Some(HtmlTag::Div),
            "span" => Some(HtmlTag::Span),
            "center" => Some(HtmlTag::Center),
            "h1" => Some(HtmlTag::H1),
            "h2" => Some(HtmlTag::H2),
            "h3" => Some(HtmlTag::H3),
            "h4" => Some(HtmlTag::H4),
            "h5" => Some(HtmlTag::H5),
            "h6" => Some(HtmlTag::H6),
            "p" => Some(HtmlTag::P),
            "br" => Some(HtmlTag::Br),
            "strong" => Some(HtmlTag::Strong),
            "b" => Some(HtmlTag::B),
            "em" => Some(HtmlTag::Em),
            "i" => Some(HtmlTag::I),
            "u" => Some(HtmlTag::U),
            "del" => Some(HtmlTag::Del),
            "s" => Some(HtmlTag::S),
            "mark" => Some(HtmlTag::Mark),
            "kbd" => Some(HtmlTag::Kbd),
            "sup" => Some(HtmlTag::Sup),
            "sub" => Some(HtmlTag::Sub),
            "a" => Some(HtmlTag::A),
            "img" => Some(HtmlTag::Img),
            "ul" => Some(HtmlTag::Ul),
            "ol" => Some(HtmlTag::Ol),
            "li" => Some(HtmlTag::Li),
            "pre" => Some(HtmlTag::Pre),
            "code" => Some(HtmlTag::Code),
            "blockquote" => Some(HtmlTag::Blockquote),
            "hr" => Some(HtmlTag::Hr),
            "table" => Some(HtmlTag::Table),
            "thead" => Some(HtmlTag::Thead),
            "tbody" => Some(HtmlTag::Tbody),
            "tr" => Some(HtmlTag::Tr),
            "th" => Some(HtmlTag::Th),
            "td" => Some(HtmlTag::Td),
            _ => None,
        }
    }
    
    /// 判断是否为块级元素
    pub fn is_block(&self) -> bool {
        matches!(self,
            HtmlTag::Div | HtmlTag::Center |
            HtmlTag::H1 | HtmlTag::H2 | HtmlTag::H3 | 
            HtmlTag::H4 | HtmlTag::H5 | HtmlTag::H6 |
            HtmlTag::P | HtmlTag::Pre |
            HtmlTag::Ul | HtmlTag::Ol | HtmlTag::Li |
            HtmlTag::Blockquote | HtmlTag::Hr |
            HtmlTag::Table | HtmlTag::Thead | HtmlTag::Tbody | 
            HtmlTag::Tr | HtmlTag::Th | HtmlTag::Td
        )
    }
    
    /// 判断是否为行内元素
    pub fn is_inline(&self) -> bool {
        !self.is_block() && !self.is_void()
    }
    
    /// 判断是否为空元素（无内容）
    pub fn is_void(&self) -> bool {
        matches!(self, HtmlTag::Br | HtmlTag::Hr | HtmlTag::Img)
    }
}
```

---

## 3. 模块设计

### 3.1 模块结构

```
src/
├── lib.rs                    # 公共 API
├── error.rs                  # 错误类型
├── html/                     # HTML 处理模块（新增）
│   ├── mod.rs               # 模块入口
│   ├── ast.rs               # HTML AST 定义
│   ├── parser.rs            # HTML 解析器
│   ├── md_converter.rs      # Markdown → HTML 转换
│   └── element.rs           # HTML 元素工具
├── css/                      # CSS 样式模块（重构）
│   ├── mod.rs               # 模块入口
│   ├── engine.rs            # CSS 引擎（基于 Lightning CSS）
│   ├── adapter.rs           # Lightning CSS 适配层
│   ├── computed.rs          # 计算后样式定义
│   ├── selector.rs          # 选择器匹配
│   └── default.css          # 默认样式表
├── layout/                   # 布局引擎（从 generator 重构）
│   ├── mod.rs
│   ├── context.rs           # 布局上下文
│   ├── flow.rs              # 流式布局
│   ├── text.rs              # 文本布局
│   ├── table.rs             # 表格布局
│   └── pagination.rs        # 分页处理
├── render/                   # 渲染模块（保持）
│   ├── mod.rs
│   ├── pdf.rs
│   ├── svg.rs
│   └── pixmap.rs
└── text.rs                   # 文本处理（保持）
```

### 3.2 HTML 解析器模块

```rust
// src/html/parser.rs

use super::ast::*;
use super::element::ElementBuilder;

/// HTML 解析器
pub struct HtmlParser;

impl HtmlParser {
    /// 解析 HTML 字符串
    pub fn parse(html: &str) -> Result<HtmlDocument, HtmlParseError> {
        let mut parser = Self;
        parser.parse_document(html)
    }
    
    /// 解析内联 HTML（用于 Markdown 中的 HTML 标签）
    pub fn parse_inline(html: &str) -> Result<Vec<HtmlNode>, HtmlParseError> {
        // 处理简单的内联标签如 <br/>, <span>, <b> 等
        let mut nodes = Vec::new();
        // 实现解析逻辑...
        Ok(nodes)
    }
    
    fn parse_document(&mut self, html: &str) -> Result<HtmlDocument, HtmlParseError> {
        // 使用 roxmltree 或手动解析
        // 这里简化展示核心逻辑
        let root = self.parse_element(html)?;
        
        // 提取 <style> 标签内容
        let style_sheets = Self::extract_style_sheets(&root);
        
        Ok(HtmlDocument { root, style_sheets })
    }
    
    fn parse_element(&mut self, html: &str) -> Result<HtmlElement, HtmlParseError> {
        // 解析标签、属性、子元素
        todo!()
    }
    
    /// 从文档中提取所有 <style> 标签内容
    fn extract_style_sheets(root: &HtmlElement) -> Vec<String> {
        let mut sheets = Vec::new();
        Self::collect_styles(root, &mut sheets);
        sheets
    }
    
    fn collect_styles(element: &HtmlElement, sheets: &mut Vec<String>) {
        if element.tag == HtmlTag::Style {
            // 提取样式内容
            for child in &element.children {
                if let HtmlNode::Text(text) = child {
                    sheets.push(text.clone());
                }
            }
        }
        
        for child in &element.children {
            if let HtmlNode::Element(el) = child {
                Self::collect_styles(el, sheets);
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HtmlParseError {
    #[error("Invalid HTML: {0}")]
    InvalidHtml(String),
    #[error("Unsupported tag: {0}")]
    UnsupportedTag(String),
    #[error("Unclosed tag: {0}")]
    UnclosedTag(String),
}
```

### 3.3 Markdown 到 HTML 转换器

```rust
// src/html/md_converter.rs

use markdown::mdast;
use super::ast::*;
use super::element::ElementBuilder;

/// Markdown 到 HTML 转换器
pub struct MdToHtmlConverter;

impl MdToHtmlConverter {
    /// 将 Markdown 转换为 HTML 文档
    pub fn convert(mdast: &mdast::Node) -> Result<HtmlDocument, ConvertError> {
        let converter = Self;
        let root = converter.convert_node(mdast)?;
        
        // 提取 <style> 标签
        let style_sheets = Self::extract_styles_from_mdast(mdast);
        
        Ok(HtmlDocument {
            root: HtmlElement {
                tag: HtmlTag::Div, // 包装在 div 中
                attrs: HashMap::new(),
                children: vec![HtmlNode::Element(root)],
            },
            style_sheets,
        })
    }
    
    /// 转换单个 MDAST 节点
    fn convert_node(&self, node: &mdast::Node) -> Result<HtmlElement, ConvertError> {
        match node {
            mdast::Node::Root(root) => self.convert_root(root),
            mdast::Node::Heading(heading) => self.convert_heading(heading),
            mdast::Node::Paragraph(para) => self.convert_paragraph(para),
            mdast::Node::Text(text) => self.convert_text(&text.value),
            mdast::Node::Strong(strong) => self.convert_strong(strong),
            mdast::Node::Emphasis(em) => self.convert_emphasis(em),
            mdast::Node::List(list) => self.convert_list(list),
            mdast::Node::ListItem(item) => self.convert_list_item(item),
            mdast::Node::Code(code) => self.convert_code(code),
            mdast::Node::InlineCode(code) => self.convert_inline_code(code),
            mdast::Node::Link(link) => self.convert_link(link),
            mdast::Node::Image(image) => self.convert_image(image),
            mdast::Node::Blockquote(bq) => self.convert_blockquote(bq),
            mdast::Node::ThematicBreak(_) => self.convert_hr(),
            mdast::Node::Table(table) => self.convert_table(table),
            mdast::Node::Delete(del) => self.convert_delete(del),
            mdast::Node::Html(html) => self.convert_html(&html.value),
            _ => Err(ConvertError::UnsupportedNode(format!("{:?}", node))),
        }
    }
    
    fn convert_heading(&self, heading: &mdast::Heading) -> Result<HtmlElement, ConvertError> {
        let tag = match heading.depth {
            1 => HtmlTag::H1,
            2 => HtmlTag::H2,
            3 => HtmlTag::H3,
            4 => HtmlTag::H4,
            5 => HtmlTag::H5,
            6 => HtmlTag::H6,
            _ => HtmlTag::H6,
        };
        
        let children = self.convert_children(&heading.children)?;
        
        Ok(ElementBuilder::new(tag).children(children).build())
    }
    
    fn convert_paragraph(&self, para: &mdast::Paragraph) -> Result<HtmlElement, ConvertError> {
        let children = self.convert_children(&para.children)?;
        Ok(ElementBuilder::new(HtmlTag::P).children(children).build())
    }
    
    fn convert_strong(&self, strong: &mdast::Strong) -> Result<HtmlElement, ConvertError> {
        let children = self.convert_children(&strong.children)?;
        Ok(ElementBuilder::new(HtmlTag::Strong).children(children).build())
    }
    
    fn convert_html(&self, html: &str) -> Result<HtmlElement, ConvertError> {
        // 解析内联 HTML 字符串
        super::parser::HtmlParser::parse_inline(html)
            .map_err(|e| ConvertError::HtmlParseError(e.to_string()))
            .and_then(|nodes| {
                // 如果只有一个元素，返回它
                if nodes.len() == 1 {
                    if let HtmlNode::Element(el) = &nodes[0] {
                        return Ok(el.clone());
                    }
                }
                // 否则包装在 div 中
                Ok(ElementBuilder::new(HtmlTag::Div).children(nodes).build())
            })
    }
    
    // ... 其他转换方法
    
    /// 转换子节点列表
    fn convert_children(&self, children: &[mdast::Node]) -> Result<Vec<HtmlNode>, ConvertError> {
        let mut result = Vec::new();
        for child in children {
            match self.convert_node(child)? {
                el if el.tag == HtmlTag::Div && el.attrs.is_empty() => {
                    // 展开空的 div 包装
                    result.extend(el.children);
                }
                el => result.push(HtmlNode::Element(el)),
            }
        }
        Ok(result)
    }
}

/// 元素构建器（辅助）
mod element {
    use super::*;
    
    pub struct ElementBuilder {
        tag: HtmlTag,
        attrs: HashMap<String, String>,
        children: Vec<HtmlNode>,
    }
    
    impl ElementBuilder {
        pub fn new(tag: HtmlTag) -> Self {
            Self {
                tag,
                attrs: HashMap::new(),
                children: Vec::new(),
            }
        }
        
        pub fn attr(mut self, key: &str, value: &str) -> Self {
            self.attrs.insert(key.to_string(), value.to_string());
            self
        }
        
        pub fn children(mut self, children: Vec<HtmlNode>) -> Self {
            self.children = children;
            self
        }
        
        pub fn build(self) -> HtmlElement {
            HtmlElement {
                tag: self.tag,
                attrs: self.attrs,
                children: self.children,
            }
        }
    }
}
```

---

## 4. CSS 样式系统

### 4.1 架构设计

```
┌─────────────────────────────────────────────────────────────────┐
│                    CSS 样式系统架构                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              Lightning CSS 层                            │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐      │   │
│  │  │  CSS 解析   │  │ 选择器匹配  │  │ 属性处理    │      │   │
│  │  │             │  │             │  │             │      │   │
│  │  │ - 完整 CSS  │  │ - ID/类/标签│  │ - 类型安全  │      │   │
│  │  │ - 简写展开  │  │ - 属性选择器│  │ - 单位转换  │      │   │
│  │  │ - 颜色处理  │  │ - 伪类      │  │ - 计算值    │      │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘      │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                  │
│                              ▼                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              liepress 适配层                             │   │
│  │                                                         │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐      │   │
│  │  │ 样式转换    │  │ PDF 扩展    │  │ 单位处理    │      │   │
│  │  │             │  │             │  │             │      │   │
│  │  │ Lightning   │  │ - 分页控制  │  │ - px→pt     │      │   │
│  │  │ 属性 →      │  │ - 页眉页脚  │  │ - em/rem    │      │   │
│  │  │ liepress    │  │ - 打印优化  │  │ - 百分比    │      │   │
│  │  │ 样式        │  │             │  │             │      │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘      │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                  │
│                              ▼                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              ComputedStyles                              │   │
│  │         （布局引擎使用的最终样式）                        │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 依赖配置

```toml
# Cargo.toml

[dependencies]
# ... 现有依赖 ...

# CSS 处理
lightningcss = { version = "1.0.0-alpha.70", features = ["into_owned"] }

# HTML 解析（可选，用于处理内联 HTML）
roxmltree = "0.20"

# 选择器匹配（Lightning CSS 已包含，但可能需要单独使用）
selectors = "0.25"
cssparser = "0.36"
```

### 4.3 CSS 引擎实现

```rust
// src/css/engine.rs

use lightningcss::stylesheet::{StyleSheet, ParserOptions, PrinterOptions};
use lightningcss::selector::Selector;
use lightningcss::properties::Property;
use lightningcss::values::length::Length;
use lightningcss::declaration::DeclarationBlock;

use super::computed::ComputedStyles;
use super::adapter::LightningCssAdapter;

/// CSS 样式引擎
pub struct CssEngine {
    /// 用户代理样式表（内置默认）
    ua_stylesheet: StyleSheet<'static, 'static>,
    /// 用户样式表
    user_stylesheet: Option<StyleSheet<'static, 'static>>,
    /// 作者样式表（来自 <style> 标签）
    author_stylesheet: Option<StyleSheet<'static, 'static>>,
    /// 适配器
    adapter: LightningCssAdapter,
}

impl CssEngine {
    /// 创建新的 CSS 引擎
    pub fn new() -> Result<Self, CssError> {
        let default_css = include_str!("default.css");
        let ua_stylesheet = StyleSheet::parse(default_css, ParserOptions::default())
            .map_err(|e| CssError::ParseError(e.to_string()))?;
        
        Ok(Self {
            ua_stylesheet,
            user_stylesheet: None,
            author_stylesheet: None,
            adapter: LightningCssAdapter::new(),
        })
    }
    
    /// 添加用户 CSS
    pub fn with_user_css(mut self, css: &str) -> Result<Self, CssError> {
        self.user_stylesheet = Some(
            StyleSheet::parse(css, ParserOptions::default())
                .map_err(|e| CssError::ParseError(e.to_string()))?
        );
        Ok(self)
    }
    
    /// 添加作者 CSS（来自 <style> 标签）
    pub fn with_author_css(mut self, css: &str) -> Result<Self, CssError> {
        self.author_stylesheet = Some(
            StyleSheet::parse(css, ParserOptions::default())
                .map_err(|e| CssError::ParseError(e.to_string()))?
        );
        Ok(self)
    }
    
    /// 计算元素的样式
    pub fn compute_styles(
        &self,
        element: &HtmlElement,
        parent_styles: Option<&ComputedStyles>,
    ) -> ComputedStyles {
        // 1. 创建样式上下文
        let mut context = StyleContext::new(parent_styles);
        
        // 2. 收集所有匹配的规则（按优先级排序）
        let rules = self.collect_matching_rules(element);
        
        // 3. 应用样式规则
        for (specificity, declarations) in rules {
            context.apply_declarations(declarations, specificity);
        }
        
        // 4. 应用内联样式（最高优先级）
        if let Some(style_attr) = element.attrs.get("style") {
            if let Ok(inline_sheet) = StyleSheet::parse(
                &format!("{{ {style_attr} }}"),
                ParserOptions::default()
            ) {
                for rule in inline_sheet.rules.0 {
                    if let Some(declarations) = rule.declarations {
                        context.apply_declarations(declarations, SPECIFICITY_INLINE);
                    }
                }
            }
        }
        
        // 5. 计算最终值
        context.into_computed_styles(&self.adapter)
    }
    
    /// 收集匹配的规则
    fn collect_matching_rules(
        &self,
        element: &HtmlElement,
    ) -> Vec<(u32, DeclarationBlock<'static, 'static>)> {
        let mut rules = Vec::new();
        
        // UA 样式（最低优先级）
        self.collect_from_stylesheet(&self.ua_stylesheet, element, &mut rules);
        
        // 用户样式
        if let Some(sheet) = &self.user_stylesheet {
            self.collect_from_stylesheet(sheet, element, &mut rules);
        }
        
        // 作者样式
        if let Some(sheet) = &self.author_stylesheet {
            self.collect_from_stylesheet(sheet, element, &mut rules);
        }
        
        // 按特异性排序
        rules.sort_by_key(|(spec, _)| *spec);
        
        rules
    }
    
    fn collect_from_stylesheet(
        &self,
        sheet: &StyleSheet,
        element: &HtmlElement,
        rules: &mut Vec<(u32, DeclarationBlock<'static, 'static>)>,
    ) {
        for rule in &sheet.rules.0 {
            if let Some(selector) = &rule.selectors {
                if self.matches_selector(selector, element) {
                    let specificity = self.calculate_specificity(selector);
                    if let Some(declarations) = &rule.declarations {
                        rules.push((specificity, declarations.clone()));
                    }
                }
            }
        }
    }
    
    /// 检查选择器是否匹配元素
    fn matches_selector(&self, selector: &Selector, element: &HtmlElement) -> bool {
        // 简化实现：检查标签名、类名、ID
        // 实际实现需要使用 Lightning CSS 的选择器匹配
        todo!("实现选择器匹配")
    }
    
    /// 计算选择器特异性
    fn calculate_specificity(&self, selector: &Selector) -> u32 {
        // ID: 100, 类/属性/伪类: 10, 标签: 1
        todo!("计算特异性")
    }
}

const SPECIFICITY_INLINE: u32 = 10000;

/// 样式上下文（构建中）
struct StyleContext {
    parent: Option<ComputedStyles>,
    properties: HashMap<String, Property>,
}

impl StyleContext {
    fn new(parent: Option<&ComputedStyles>) -> Self {
        Self {
            parent: parent.cloned(),
            properties: HashMap::new(),
        }
    }
    
    fn apply_declarations(&mut self, declarations: DeclarationBlock, specificity: u32) {
        for decl in &declarations.declarations {
            // 存储属性，后续处理层叠
            self.properties.insert(
                decl.property_id().name().to_string(),
                decl.clone()
            );
        }
    }
    
    fn into_computed_styles(self, adapter: &LightningCssAdapter) -> ComputedStyles {
        let mut computed = self.parent.unwrap_or_default();
        
        for (_, property) in self.properties {
            adapter.apply_property(&mut computed, &property);
        }
        
        computed
    }
}
```

### 4.4 Lightning CSS 适配层

```rust
// src/css/adapter.rs

use lightningcss::properties::Property;
use lightningcss::values::length::{Length, LengthValue};
use lightningcss::values::color::CssColor;
use lightningcss::values::percentage::Percentage;

use super::computed::{ComputedStyles, CssValue, LengthUnit};

/// Lightning CSS 到 liepress 的适配器
pub struct LightningCssAdapter;

impl LightningCssAdapter {
    pub fn new() -> Self {
        Self
    }
    
    /// 将 Lightning CSS 属性应用到 liepress 样式
    pub fn apply_property(&self, computed: &mut ComputedStyles, property: &Property) {
        match property {
            // 字体相关
            Property::FontSize(size) => {
                computed.font_size = self.convert_length(size);
            }
            Property::FontFamily(families) => {
                computed.font_family = families.0.iter()
                    .map(|f| f.to_string())
                    .collect();
            }
            Property::FontWeight(weight) => {
                computed.font_weight = self.convert_font_weight(weight);
            }
            Property::FontStyle(style) => {
                computed.font_style = self.convert_font_style(style);
            }
            Property::LineHeight(height) => {
                computed.line_height = self.convert_line_height(height);
            }
            Property::LetterSpacing(spacing) => {
                computed.letter_spacing = self.convert_length(&spacing.0);
            }
            
            // 文本相关
            Property::Color(color) => {
                computed.color = self.convert_color(color);
            }
            Property::TextAlign(align) => {
                computed.text_align = self.convert_text_align(align);
            }
            Property::TextIndent(indent) => {
                computed.text_indent = self.convert_length(indent);
            }
            Property::TextDecoration(decoration) => {
                computed.text_decoration = self.convert_text_decoration(decoration);
            }
            
            // 布局相关
            Property::Display(display) => {
                computed.display = self.convert_display(display);
            }
            Property::Width(width) => {
                computed.width = Some(self.convert_length(&width.0));
            }
            Property::Height(height) => {
                computed.height = Some(self.convert_length(&height.0));
            }
            
            // 边距
            Property::Margin(margin) => {
                computed.margin_top = self.convert_length(&margin.top);
                computed.margin_right = self.convert_length(&margin.right);
                computed.margin_bottom = self.convert_length(&margin.bottom);
                computed.margin_left = self.convert_length(&margin.left);
            }
            Property::MarginTop(m) => computed.margin_top = self.convert_length(m),
            Property::MarginRight(m) => computed.margin_right = self.convert_length(m),
            Property::MarginBottom(m) => computed.margin_bottom = self.convert_length(m),
            Property::MarginLeft(m) => computed.margin_left = self.convert_length(m),
            
            // 填充
            Property::Padding(padding) => {
                computed.padding_top = self.convert_length(&padding.top);
                computed.padding_right = self.convert_length(&padding.right);
                computed.padding_bottom = self.convert_length(&padding.bottom);
                computed.padding_left = self.convert_length(&padding.left);
            }
            Property::PaddingTop(p) => computed.padding_top = self.convert_length(p),
            Property::PaddingRight(p) => computed.padding_right = self.convert_length(p),
            Property::PaddingBottom(p) => computed.padding_bottom = self.convert_length(p),
            Property::PaddingLeft(p) => computed.padding_left = self.convert_length(p),
            
            // 边框
            Property::BorderTopWidth(w) => computed.border_top_width = self.convert_length(w),
            Property::BorderRightWidth(w) => computed.border_right_width = self.convert_length(w),
            Property::BorderBottomWidth(w) => computed.border_bottom_width = self.convert_length(w),
            Property::BorderLeftWidth(w) => computed.border_left_width = self.convert_length(w),
            
            Property::BorderTopColor(c) => computed.border_top_color = Some(self.convert_color(c)),
            Property::BorderRightColor(c) => computed.border_right_color = Some(self.convert_color(c)),
            Property::BorderBottomColor(c) => computed.border_bottom_color = Some(self.convert_color(c)),
            Property::BorderLeftColor(c) => computed.border_left_color = Some(self.convert_color(c)),
            
            // 背景
            Property::BackgroundColor(color) => {
                computed.background_color = Some(self.convert_color(color));
            }
            
            // 分页控制（liepress 扩展）
            Property::Custom(custom) if custom.name.as_ref() == "page-break-before" => {
                computed.page_break_before = self.convert_page_break(&custom.value);
            }
            Property::Custom(custom) if custom.name.as_ref() == "page-break-after" => {
                computed.page_break_after = self.convert_page_break(&custom.value);
            }
            
            // 忽略不支持的属性
            _ => {}
        }
    }
    
    /// 转换长度值
    fn convert_length(&self, length: &Length) -> CssValue {
        match length {
            Length::Value(val) => match val {
                LengthValue::Px(v) => CssValue::Length(*v, LengthUnit::Px),
                LengthValue::Pt(v) => CssValue::Length(*v, LengthUnit::Pt),
                LengthValue::Em(v) => CssValue::Length(*v, LengthUnit::Em),
                LengthValue::Rem(v) => CssValue::Length(*v, LengthUnit::Rem),
                LengthValue::Percent(Percentage(v)) => CssValue::Percentage(*v),
                LengthValue::Mm(v) => CssValue::Length(*v, LengthUnit::Mm),
                LengthValue::Cm(v) => CssValue::Length(*v, LengthUnit::Cm),
                LengthValue::In(v) => CssValue::Length(*v, LengthUnit::In),
                _ => CssValue::Length(0.0, LengthUnit::Px),
            },
            _ => CssValue::Length(0.0, LengthUnit::Px),
        }
    }
    
    /// 转换颜色
    fn convert_color(&self, color: &CssColor) -> crate::visual::Color {
        match color {
            CssColor::RGB(rgb) => crate::visual::Color::new(
                rgb.red,
                rgb.green,
                rgb.blue,
            ),
            CssColor::RGBA(rgba) => crate::visual::Color::with_alpha(
                rgba.red,
                rgba.green,
                rgba.blue,
                (rgba.alpha * 255.0) as u8,
            ),
            CssColor::Hex(hex) => crate::visual::Color::new(
                ((hex >> 16) & 0xFF) as u8,
                ((hex >> 8) & 0xFF) as u8,
                (hex & 0xFF) as u8,
            ),
            _ => crate::visual::Color::new(0, 0, 0),
        }
    }
    
    // ... 其他转换方法
}
```

### 4.5 计算后样式定义

```rust
// src/css/computed.rs

use crate::visual::Color;

/// CSS 值类型
#[derive(Debug, Clone)]
pub enum CssValue {
    Length(f32, LengthUnit),
    Percentage(f32),
    Color(Color),
    String(String),
    Ident(String),
    Number(f32),
    List(Vec<CssValue>),
    None,
}

/// 长度单位
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LengthUnit {
    Px,
    Pt,
    Em,
    Rem,
    In,
    Cm,
    Mm,
    Percent,
}

/// 计算后的样式（供布局引擎使用）
#[derive(Debug, Clone)]
pub struct ComputedStyles {
    // 字体
    pub font_family: Vec<String>,
    pub font_size: CssValue,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub line_height: CssValue,
    pub letter_spacing: CssValue,
    pub color: Color,
    
    // 文本
    pub text_align: TextAlign,
    pub text_indent: CssValue,
    pub text_decoration: TextDecoration,
    pub text_transform: TextTransform,
    pub white_space: WhiteSpace,
    
    // 布局
    pub display: Display,
    pub width: Option<CssValue>,
    pub height: Option<CssValue>,
    
    // 盒模型
    pub margin_top: CssValue,
    pub margin_right: CssValue,
    pub margin_bottom: CssValue,
    pub margin_left: CssValue,
    
    pub padding_top: CssValue,
    pub padding_right: CssValue,
    pub padding_bottom: CssValue,
    pub padding_left: CssValue,
    
    // 边框
    pub border_top_width: CssValue,
    pub border_right_width: CssValue,
    pub border_bottom_width: CssValue,
    pub border_left_width: CssValue,
    
    pub border_top_color: Option<Color>,
    pub border_right_color: Option<Color>,
    pub border_bottom_color: Option<Color>,
    pub border_left_color: Option<Color>,
    
    pub border_top_style: BorderStyle,
    pub border_right_style: BorderStyle,
    pub border_bottom_style: BorderStyle,
    pub border_left_style: BorderStyle,
    
    // 背景
    pub background_color: Option<Color>,
    
    // 分页
    pub page_break_before: PageBreak,
    pub page_break_after: PageBreak,
    
    // 表格
    pub border_collapse: BorderCollapse,
    pub border_spacing: (CssValue, CssValue),
    
    // 列表
    pub list_style_type: ListStyleType,
    
    // 链接（特殊）
    pub link_url: Option<String>,
}

impl Default for ComputedStyles {
    fn default() -> Self {
        Self {
            font_family: vec!["serif".to_string()],
            font_size: CssValue::Length(10.5, LengthUnit::Pt),
            font_weight: FontWeight::Normal,
            font_style: FontStyle::Normal,
            line_height: CssValue::Number(1.5),
            letter_spacing: CssValue::Length(0.0, LengthUnit::Px),
            color: Color::new(0, 0, 0),
            text_align: TextAlign::Left,
            text_indent: CssValue::Length(0.0, LengthUnit::Pt),
            text_decoration: TextDecoration::None,
            text_transform: TextTransform::None,
            white_space: WhiteSpace::Normal,
            display: Display::Block,
            width: None,
            height: None,
            margin_top: CssValue::Length(0.0, LengthUnit::Pt),
            margin_right: CssValue::Length(0.0, LengthUnit::Pt),
            margin_bottom: CssValue::Length(12.0, LengthUnit::Pt),
            margin_left: CssValue::Length(0.0, LengthUnit::Pt),
            padding_top: CssValue::Length(0.0, LengthUnit::Pt),
            padding_right: CssValue::Length(0.0, LengthUnit::Pt),
            padding_bottom: CssValue::Length(0.0, LengthUnit::Pt),
            padding_left: CssValue::Length(0.0, LengthUnit::Pt),
            border_top_width: CssValue::Length(0.0, LengthUnit::Pt),
            border_right_width: CssValue::Length(0.0, LengthUnit::Pt),
            border_bottom_width: CssValue::Length(0.0, LengthUnit::Pt),
            border_left_width: CssValue::Length(0.0, LengthUnit::Pt),
            border_top_color: None,
            border_right_color: None,
            border_bottom_color: None,
            border_left_color: None,
            border_top_style: BorderStyle::None,
            border_right_style: BorderStyle::None,
            border_bottom_style: BorderStyle::None,
            border_left_style: BorderStyle::None,
            background_color: None,
            page_break_before: PageBreak::Auto,
            page_break_after: PageBreak::Auto,
            border_collapse: BorderCollapse::Collapse,
            border_spacing: (CssValue::Length(0.0, LengthUnit::Pt), CssValue::Length(0.0, LengthUnit::Pt)),
            list_style_type: ListStyleType::Disc,
            link_url: None,
        }
    }
}

impl ComputedStyles {
    /// 从父样式继承
    pub fn inherit_from(&mut self, parent: &Self) {
        // 可继承属性
        self.font_family = parent.font_family.clone();
        self.font_size = parent.font_size.clone();
        self.font_weight = parent.font_weight;
        self.font_style = parent.font_style;
        self.line_height = parent.line_height.clone();
        self.letter_spacing = parent.letter_spacing.clone();
        self.color = parent.color;
        self.text_align = parent.text_align;
        self.text_transform = parent.text_transform;
        self.white_space = parent.white_space;
        self.page_break_before = parent.page_break_before;
        self.page_break_after = parent.page_break_after;
        self.list_style_type = parent.list_style_type;
        
        // 不可继承属性保持默认值
    }
    
    /// 解析长度值为 pt
    pub fn length_to_pt(&self, value: &CssValue, parent_size: f32) -> f32 {
        match value {
            CssValue::Length(v, LengthUnit::Pt) => *v,
            CssValue::Length(v, LengthUnit::Px) => *v * 0.75, // 假设 96 DPI
            CssValue::Length(v, LengthUnit::Em) => *v * self.font_size_pt(),
            CssValue::Length(v, LengthUnit::Rem) => *v * 10.5, // 根字体大小
            CssValue::Length(v, LengthUnit::In) => *v * 72.0,
            CssValue::Length(v, LengthUnit::Cm) => *v * 28.35,
            CssValue::Length(v, LengthUnit::Mm) => *v * 2.835,
            CssValue::Percentage(v) => *v * parent_size / 100.0,
            _ => 0.0,
        }
    }
    
    fn font_size_pt(&self) -> f32 {
        match &self.font_size {
            CssValue::Length(v, LengthUnit::Pt) => *v,
            CssValue::Length(v, LengthUnit::Px) => *v * 0.75,
            _ => 10.5,
        }
    }
}

// 枚举定义
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontWeight {
    Normal,
    Bold,
    W100, W200, W300, W400, W500, W600, W700, W800, W900,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextDecoration {
    None,
    Underline,
    LineThrough,
    Overline,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextTransform {
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WhiteSpace {
    Normal,
    Nowrap,
    Pre,
    PreWrap,
    PreLine,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Display {
    Block,
    Inline,
    InlineBlock,
    None,
    Table,
    TableRow,
    TableCell,
    Flex, // 未来支持
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BorderStyle {
    None,
    Solid,
    Dashed,
    Dotted,
    Double,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageBreak {
    Auto,
    Always,
    Avoid,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BorderCollapse {
    Collapse,
    Separate,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ListStyleType {
    None,
    Disc,
    Circle,
    Square,
    Decimal,
    DecimalLeadingZero,
    LowerRoman,
    UpperRoman,
    LowerAlpha,
    UpperAlpha,
}
```

---

## 5. 数据流与处理流程

### 5.1 完整处理流程

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          完整处理流程                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. 输入阶段                                                                 │
│  ┌─────────────┐                                                            │
│  │  Markdown   │  用户输入的 Markdown 文本（可能包含 HTML 标签）              │
│  │   Source    │                                                            │
│  └──────┬──────┘                                                            │
│         │                                                                   │
│         ▼                                                                   │
│  2. 解析阶段                                                                 │
│  ┌─────────────────────────────────────────────────────────────┐            │
│  │                    Markdown Parser                           │            │
│  │  - 使用 markdown crate 解析为 MDAST                          │            │
│  │  - 保留 HTML 标签为 Html 节点                                │            │
│  └────────────────────────┬────────────────────────────────────┘            │
│                           │                                                 │
│                           ▼                                                 │
│  ┌─────────────────────────────────────────────────────────────┐            │
│  │                    MDAST 树                                  │            │
│  │  Root                                                       │            │
│  │  ├── Heading(1, "标题")                                      │            │
│  │  ├── Paragraph                                               │            │
│  │  │   └── Text("段落内容")                                    │            │
│  │  ├── Html("<div style='...'>")                               │            │
│  │  │   └── Paragraph                                           │            │
│  │  │       └── Text("HTML 内容")                               │            │
│  │  └── Html("</div>")                                          │            │
│  └────────────────────────┬────────────────────────────────────┘            │
│                           │                                                 │
│                           ▼                                                 │
│  3. 转换阶段                                                                 │
│  ┌─────────────────────────────────────────────────────────────┐            │
│  │                 Markdown → HTML Converter                    │            │
│  │  - 遍历 MDAST 树                                             │            │
│  │  - 将 Markdown 节点转换为 HTML AST 节点                       │            │
│  │  - 解析内联 HTML 字符串                                       │            │
│  │  - 提取 <style> 标签内容                                      │            │
│  └────────────────────────┬────────────────────────────────────┘            │
│                           │                                                 │
│                           ▼                                                 │
│  4. HTML 处理阶段                                                            │
│  ┌─────────────────────────────────────────────────────────────┐            │
│  │                    HTML AST                                  │            │
│  │  Element(div)                                               │            │
│  │  ├── Element(h1)                                            │            │
│  │  │   └── Text("标题")                                        │            │
│  │  ├── Element(p)                                             │            │
│  │  │   └── Text("段落内容")                                    │            │
│  │  └── Element(div, style="...")                              │            │
│  │      └── Element(p)                                         │            │
│  │          └── Text("HTML 内容")                               │            │
│  └────────────────────────┬────────────────────────────────────┘            │
│                           │                                                 │
│                           ▼                                                 │
│  5. 样式计算阶段                                                             │
│  ┌─────────────────────────────────────────────────────────────┐            │
│  │                    CSS Engine                                │            │
│  │  - 解析内置默认 CSS                                          │            │
│  │  - 解析用户提供的 CSS                                        │            │
│  │  - 解析 <style> 标签中的 CSS                                  │            │
│  │  - 递归计算每个元素的 ComputedStyles                          │            │
│  └────────────────────────┬────────────────────────────────────┘            │
│                           │                                                 │
│                           ▼                                                 │
│  6. 样式化 HTML                                                              │
│  ┌─────────────────────────────────────────────────────────────┐            │
│  │                 Styled HTML Document                         │            │
│  │  Element(div, styles={...})                                 │            │
│  │  ├── Element(h1, styles={font_size: 24pt, ...})             │            │
│  │  │   └── Text("标题")                                        │            │
│  │  └── ...                                                    │            │
│  └────────────────────────┬────────────────────────────────────┘            │
│                           │                                                 │
│                           ▼                                                 │
│  7. 布局阶段                                                                 │
│  ┌─────────────────────────────────────────────────────────────┐            │
│  │                    Layout Engine                             │            │
│  │  - 流式布局计算                                              │            │
│  │  - 分页处理                                                  │            │
│  │  - 生成 Document 对象                                        │            │
│  └────────────────────────┬────────────────────────────────────┘            │
│                           │                                                 │
│                           ▼                                                 │
│  8. 渲染阶段                                                                 │
│  ┌─────────────────────────────────────────────────────────────┐            │
│  │                    Document                                  │            │
│  │  - 页面列表                                                  │            │
│  │  - 视觉元素列表                                              │            │
│  │  - 布局信息                                                  │            │
│  └────────────────────────┬────────────────────────────────────┘            │
│                           │                                                 │
│                           ▼                                                 │
│  9. 输出阶段                                                                 │
│  ┌─────────────────────────────────────────────────────────────┐            │
│  │                    Renderer                                  │            │
│  │  ├── PDF Renderer (krilla)                                   │            │
│  │  ├── SVG Renderer (vello_cpu)                                │            │
│  │  └── Pixmap Renderer (vello_cpu)                             │            │
│  └─────────────────────────────────────────────────────────────┘            │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 核心 API 设计

```rust
// src/lib.rs

pub mod html;
pub mod css;
pub mod layout;
pub mod render;

use html::{HtmlDocument, HtmlParser, MdToHtmlConverter};
use css::CssEngine;
use layout::LayoutEngine;
use render::{PdfRenderer, SvgRenderer, PixmapRenderer};

/// 主转换器
pub struct Liepress {
    css_engine: CssEngine,
    layout_engine: LayoutEngine,
}

impl Liepress {
    /// 创建新的转换器实例
    pub fn new() -> Result<Self, LiepressError> {
        Ok(Self {
            css_engine: CssEngine::new()?,
            layout_engine: LayoutEngine::new(),
        })
    }
    
    /// 添加用户 CSS
    pub fn with_css(mut self, css: &str) -> Result<Self, LiepressError> {
        self.css_engine = self.css_engine.with_user_css(css)?;
        Ok(self)
    }
    
    /// 将 Markdown 转换为 PDF
    pub fn markdown_to_pdf(&self, markdown: &str) -> Result<PdfDocument, LiepressError> {
        // 1. 解析 Markdown
        let mdast = markdown::to_mdast(markdown, &ParseOptions::gfm())?;
        
        // 2. 转换为 HTML
        let html_doc = MdToHtmlConverter::convert(&mdast)?;
        
        // 3. 添加 <style> 标签中的 CSS
        for style in &html_doc.style_sheets {
            self.css_engine.add_author_css(style)?;
        }
        
        // 4. 计算样式
        let styled_doc = self.apply_styles(html_doc)?;
        
        // 5. 布局
        let document = self.layout_engine.layout(styled_doc)?;
        
        // 6. 渲染
        PdfRenderer::render(&document)
    }
    
    /// 将 Markdown 转换为 SVG
    pub fn markdown_to_svg(&self, markdown: &str) -> Result<SvgDocument, LiepressError> {
        // 类似实现...
        todo!()
    }
    
    /// 应用样式到 HTML 文档
    fn apply_styles(&self, doc: HtmlDocument) -> Result<StyledDocument, LiepressError> {
        let mut styled_root = StyledElement::from(doc.root);
        self.style_element_recursive(&mut styled_root, None)?;
        Ok(StyledDocument::new(styled_root))
    }
    
    fn style_element_recursive(
        &self,
        element: &mut StyledElement,
        parent_styles: Option<&ComputedStyles>,
    ) -> Result<(), LiepressError> {
        // 计算当前元素样式
        let computed = self.css_engine.compute_styles(&element.element, parent_styles);
        
        // 应用样式
        element.styles = computed.clone();
        
        // 递归处理子元素
        for child in &mut element.children {
            if let Some(el) = child.as_element_mut() {
                self.style_element_recursive(el, Some(&computed))?;
            }
        }
        
        Ok(())
    }
}
```

---

## 6. 实现规划

### 6.1 阶段划分

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          实现阶段规划                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  阶段 1：基础架构（2-3 周）                                                   │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  ✓ 创建 src/html/ 模块                                               │   │
│  │  ✓ 定义 HTML AST 结构（ast.rs）                                      │   │
│  │  ✓ 实现 HTML 标签枚举和工具方法                                       │   │
│  │  ✓ 创建 src/css/ 模块结构                                            │   │
│  │  ✓ 添加 lightningcss 依赖                                            │   │
│  │  ✓ 定义 ComputedStyles 结构                                          │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ▼                                        │
│  阶段 2：Markdown → HTML 转换（2 周）                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  ✓ 实现 MdToHtmlConverter                                            │   │
│  │  ✓ 支持所有 Markdown 基础语法转换                                     │   │
│  │  ✓ 实现内联 HTML 解析                                                │   │
│  │  ✓ 提取 <style> 标签内容                                             │   │
│  │  ✓ 编写转换测试                                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ▼                                        │
│  阶段 3：CSS 样式系统（3 周）                                                │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  ✓ 实现 CssEngine（基于 Lightning CSS）                              │   │
│  │  ✓ 实现 LightningCssAdapter                                          │   │
│  │  ✓ 支持基础选择器（标签、类、ID）                                     │   │
│  │  ✓ 支持常用 CSS 属性                                                 │   │
│  │  ✓ 实现样式继承和层叠                                                │   │
│  │  ✓ 单位转换（px→pt, em, rem, %）                                     │   │
│  │  ✓ 编写样式测试                                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ▼                                        │
│  阶段 4：布局引擎重构（2 周）                                                │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  ✓ 适配新的 StyledDocument 结构                                      │   │
│  │  ✓ 重构文本布局（支持新的 ComputedStyles）                           │   │
│  │  ✓ 重构块级元素布局                                                  │   │
│  │  ✓ 保持现有分页逻辑                                                  │   │
│  │  ✓ 保持表格布局                                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ▼                                        │
│  阶段 5：HTML 标签支持（2 周）                                               │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  ✓ 实现 HTML 解析器（支持 HTML 子集）                                  │   │
│  │  ✓ 支持所有定义的 HTML 标签                                            │   │
│  │  ✓ 支持标签属性（class, id, style）                                    │   │
│  │  ✓ 支持嵌套 HTML 结构                                                  │   │
│  │  ✓ 编写 HTML 解析测试                                                  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ▼                                        │
│  阶段 6：集成与优化（2-3 周）                                                │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  ✓ 整合所有模块                                                        │   │
│  │  ✓ 性能优化                                                            │   │
│  │  ✓ 完整集成测试                                                        │   │
│  │  ✓ 文档更新                                                            │   │
│  │  ✓ 迁移指南                                                            │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  预计总工期：11-13 周                                                       │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘

### 6.2 迁移策略

#### 6.2.1 向后兼容性

```rust
// src/lib.rs

/// 渲染 Markdown（兼容旧 API）
pub fn render_markdown(input: &str, options: &RenderOptions) -> Result<Document, Error> {
    // 使用新的 HTML 子集架构
    let html_doc = MarkdownToHtmlConverter::convert(input)?;
    let styled_doc = CssEngine::compute_styles(html_doc, &options.css)?;
    Document::from_styled_html(styled_doc, options)
}

/// 渲染 Markdown（新 API，支持更多控制）
pub fn render_html(html: &str, options: &HtmlRenderOptions) -> Result<Document, Error> {
    let html_doc = HtmlParser::parse(html)?;
    let styled_doc = CssEngine::compute_styles(html_doc, &options.css)?;
    Document::from_styled_html(styled_doc, &options.into())
}
```

#### 6.2.2 特性开关

```toml
# Cargo.toml

[features]
default = ["html-subset"]
html-subset = ["lightningcss"]
legacy-mode = []  # 保留旧架构用于回退
```

#### 6.2.3 渐进式迁移步骤

1. **阶段 1**：新架构作为独立模块开发，不影响现有代码
2. **阶段 2**：添加 `render_html` 新 API，旧 API 保持不变
3. **阶段 3**：内部实现切换到新架构，保持 API 兼容
4. **阶段 4**：废弃旧 API，提供迁移指南
5. **阶段 5**：完全移除旧架构代码

### 6.3 风险评估

| 风险 | 可能性 | 影响 | 缓解措施 |
|------|--------|------|----------|
| Lightning CSS 依赖问题 | 低 | 高 | 准备备用 CSS 解析方案 |
| 性能下降 | 中 | 中 | 早期性能基准测试，必要时优化 |
| 兼容性问题 | 中 | 高 | 完整的回归测试套件 |
| 开发延期 | 中 | 中 | 分阶段交付，优先核心功能 |

### 6.4 成功标准

1. **功能完整性**
   - 支持所有定义的 HTML 子集标签
   - 支持所有定义的 CSS 属性
   - 100% 向后兼容现有 Markdown 功能

2. **性能指标**
   - 渲染速度不低于当前版本
   - 内存使用增长不超过 20%

3. **代码质量**
   - 测试覆盖率 > 80%
   - 无 Clippy 警告
   - 文档完整

---

## 附录

### A. 参考资源

- [Lightning CSS 文档](https://lightningcss.dev/)
- [CommonMark 规范](https://spec.commonmark.org/)
- [CSS Selectors Level 4](https://www.w3.org/TR/selectors-4/)
- [CSS Box Model](https://www.w3.org/TR/CSS22/box.html)

### B. 相关 crate

- `markdown` - Markdown 解析
- `lightningcss` - CSS 解析和处理
- `roxmltree` - XML/HTML 解析（可选）
- `selectors` - CSS 选择器匹配（如 Lightning CSS 不满足需求）

### C. 示例 Markdown + HTML

```markdown
# 文档标题

<div style="text-align: center; color: #666;">

## 居中副标题

这是居中显示的内容。

</div>

普通段落。<span style="color: red;">红色文字</span> 和 **加粗文字**。

<br/>

强制换行后的内容。

<div class="warning-box" style="background: #fff3cd; padding: 10px; border-left: 4px solid #ffc107;">
⚠️ 警告：这是一个警告框
</div>
```

---

*文档版本：1.0*
*最后更新：2026-06-01*
│  │