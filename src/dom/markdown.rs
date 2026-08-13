//! Markdown 输入前端：pulldown-cmark 事件流直接构建 [`HtmlDocument`]。
//!
//! 管线位置：输入侧 Layer 1（与纯 HTML 输入的 [`crate::dom::parse_html`] 汇合到同一
//! `HtmlDocument`）。Markdown 不经过"先序列化为 HTML 字符串再二次解析"的往返，
//! 而是把 pulldown-cmark 的 `Event` 流直接映射为 HTML AST 节点，消除无意义的
//! 字符串序列化/反序列化开销与字符转义精度损失。

use std::collections::HashMap;
use std::path::Path;

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};

use crate::dom::{HtmlDocument, HtmlElement, HtmlNode, HtmlTag};

/// 将 Markdown 直接解析为 [`HtmlDocument`]（HTML AST），不经过中间字符串。
///
/// 等价于 `parse_html(&markdown_to_html(md))`，但只遍历一次事件流、且文本保真度更高。
/// 将 Markdown 解析为 [`HtmlDocument`]（无图片内嵌，纯解析）。
pub fn markdown_to_dom(markdown: &str) -> HtmlDocument {
    markdown_to_dom_with_resolver(markdown, &super::resource::ResourceResolver::new(None))
}

/// 将 Markdown 解析为 [`HtmlDocument`]，并在解析后**统一内嵌图片**。
///
/// 所有输出后端（PDF/SVG/PNG/DOCX）都经由 [`crate::dom::markdown_to_dom_with_resolver`]
/// 进入，故在此一次内嵌本地/相对路径图片为 data URI，后续各层不再需要单独处理。
pub fn markdown_to_dom_with_resolver(
    markdown: &str,
    resolver: &super::resource::ResourceResolver,
) -> HtmlDocument {
    let parser = Parser::new_ext(
        markdown,
        pulldown_cmark::Options::ENABLE_TABLES
            | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
            | pulldown_cmark::Options::ENABLE_TASKLISTS
            | pulldown_cmark::Options::ENABLE_DEFINITION_LIST
            | pulldown_cmark::Options::ENABLE_FOOTNOTES,
    );

    let mut root = HtmlElement {
        tag: HtmlTag::Body,
        attrs: HashMap::new(),
        children: Vec::new(),
    };
    // 栈：保存尚未闭合的父元素；栈顶是当前写入目标。
    let mut stack: Vec<HtmlElement> = Vec::new();

    // 脚注：label → 序号（按首次出现的顺序编号；引用与定义共用同一序号）。
    let mut footnote_index: HashMap<String, usize> = HashMap::new();

    let mut style_sheets: Vec<String> = Vec::new();

    for event in parser {
        match event {
            Event::Start(tag) => {
                if let Tag::CodeBlock(kind) = &tag {
                    // 代码块：直接构造 <pre class="language-X">，块内文本进入 pre.children。
                    // `extract_code_block` 在没有 <code> 子元素时会回退到 pre 自身文本，
                    // 语言则从 pre 的 class 提取。
                    let mut pre = simple(HtmlTag::Pre);
                    if let pulldown_cmark::CodeBlockKind::Fenced(lang) = kind {
                        if !lang.is_empty() {
                            pre.attrs
                                .insert("class".to_string(), format!("language-{}", lang));
                        }
                    }
                    stack.push(pre);
                    continue;
                }
                // 脚注定义：构造 <div id="fn-def-<label>" class="footnote-def">，
                // 定义内容由后续事件流填入。编号前缀在闭合时追加（见 Event::End）。
                if let Tag::FootnoteDefinition(label) = &tag {
                    let mut el = simple(HtmlTag::Div);
                    el.attrs
                        .insert("id".to_string(), format!("fn-def-{}", label));
                    el.attrs
                        .insert("class".to_string(), "footnote-def".to_string());
                    // 记录该 label 的序号（引用与定义共用；此时若引用在前则已分配）。
                    let n = footnote_index.len() + 1;
                    let idx = *footnote_index.entry(label.to_string()).or_insert(n);
                    el.attrs.insert("data-fn".to_string(), idx.to_string());
                    stack.push(el);
                    continue;
                }
                let el = element_from_start_tag(&tag);
                stack.push(el);
            }
            Event::End(_tag_end) => {
                // 当前元素闭合：弹出并挂回父（或根）。
                let mut closed = stack.pop().expect("pulldown 事件流闭合多于打开");
                // 脚注定义闭合：把编号前缀 "N. " 插入 children 最前面。
                if closed.tag == HtmlTag::Div
                    && closed
                        .attrs
                        .get("class")
                        .map(|c| c.split_whitespace().any(|w| w == "footnote-def"))
                        .unwrap_or(false)
                {
                    if let Some(n) = closed.attrs.get("data-fn") {
                        closed
                            .children
                            .insert(0, HtmlNode::Text(format!("{}. ", n)));
                        closed.attrs.remove("data-fn");
                    }
                }
                // 图片：把内部文本（alt 语法 `![alt]` 经 pulldown 展开为 Image 内文本）
                // 收集为 alt 属性。
                if closed.tag == HtmlTag::Img {
                    let alt: String = closed
                        .children
                        .iter()
                        .filter_map(|n| match n {
                            HtmlNode::Text(t) => Some(t.as_str()),
                            _ => None,
                        })
                        .collect();
                    if !alt.is_empty() {
                        closed.attrs.insert("alt".to_string(), alt);
                    }
                    closed.children.clear();
                }
                let node = HtmlNode::Element(closed);
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    root.children.push(node);
                }
            }
            Event::Text(text) => push_text(&mut stack, &mut root, &text),
            Event::Code(text) => {
                let mut el = simple(HtmlTag::Code);
                el.children.push(HtmlNode::Text(text.to_string()));
                attach(&mut stack, &mut root, &mut el);
            }
            Event::Html(raw) => {
                // 内嵌原始 HTML：用片段解析器递归成节点，回退到文本。
                let frag = crate::dom::parse_html_fragment(&raw);
                for node in frag {
                    if let HtmlNode::Element(ref e) = node {
                        if e.tag == HtmlTag::Style {
                            if let Some(css) = e.children.first().and_then(|c| match c {
                                HtmlNode::Text(t) => Some(t.clone()),
                                _ => None,
                            }) {
                                style_sheets.push(css);
                                continue;
                            }
                        }
                    }
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else {
                        root.children.push(node);
                    }
                }
            }
            Event::SoftBreak => push_text(&mut stack, &mut root, "\n"),
            Event::HardBreak => {
                let mut el = simple(HtmlTag::Br);
                attach(&mut stack, &mut root, &mut el);
            }
            Event::Rule => {
                let mut el = simple(HtmlTag::Hr);
                attach(&mut stack, &mut root, &mut el);
            }
            Event::TaskListMarker(checked) => {
                let mut el = simple(HtmlTag::Input);
                el.attrs.insert("type".to_string(), "checkbox".to_string());
                if checked {
                    el.attrs.insert("checked".to_string(), String::new());
                }
                attach(&mut stack, &mut root, &mut el);
            }
            Event::FootnoteReference(name) => {
                // 脚注引用：分配序号，生成上标锚点 <sup><a href="#fn-def-<label>">N</a></sup>。
                let n = footnote_index.len() + 1;
                footnote_index.entry(name.to_string()).or_insert(n);
                let n = *footnote_index.get(&name.to_string()).unwrap_or(&n);
                let mut el = simple(HtmlTag::Sup);
                let mut a = simple(HtmlTag::A);
                a.attrs.insert("href".to_string(), format!("#fn-def-{}", name));
                a.attrs
                    .insert("class".to_string(), "footnote-ref".to_string());
                a.children
                    .push(HtmlNode::Text(n.to_string()));
                el.children.push(HtmlNode::Element(a));
                attach(&mut stack, &mut root, &mut el);
            }
            // 少数变体（如 FootnoteDefinition 容器内文本）降级为透出文本。
            other => {
                if let Some(t) = event_text(&other) {
                    push_text(&mut stack, &mut root, &t);
                }
            }
        }
    }

    // 防备事件流不配对：把残留元素挂回根。
    while let Some(el) = stack.pop() {
        root.children.push(HtmlNode::Element(el));
    }

    let mut doc = HtmlDocument {
        root,
        style_sheets,
    };
    // 统一内嵌图片（本地路径 → data URI；data URI 保留；网络 URL 不处理）
    super::resource::embed_images(&mut doc, resolver);
    doc
}

fn event_text(event: &Event) -> Option<String> {
    match event {
        Event::Text(t) => Some(t.to_string()),
        Event::Code(t) => Some(t.to_string()),
        _ => None,
    }
}

/// 根据 Start 标签构造对应 HTML 元素（含属性）。
fn element_from_start_tag(tag: &Tag) -> HtmlElement {
    match tag {
        Tag::Paragraph => simple(HtmlTag::P),
        Tag::Heading { level, .. } => {
            let tag = match level {
                HeadingLevel::H1 => HtmlTag::H1,
                HeadingLevel::H2 => HtmlTag::H2,
                HeadingLevel::H3 => HtmlTag::H3,
                HeadingLevel::H4 => HtmlTag::H4,
                HeadingLevel::H5 => HtmlTag::H5,
                HeadingLevel::H6 => HtmlTag::H6,
            };
            simple(tag)
        }
        Tag::BlockQuote(_) => simple(HtmlTag::Blockquote),
        // CodeBlock 已在 Event::Start 分支特殊处理（构造 pre>code），此处保留以防直接调用。
        Tag::CodeBlock(_kind) => simple(HtmlTag::Pre),
        Tag::List(start) => {
            let mut el = simple(if start.is_some() {
                HtmlTag::Ol
            } else {
                HtmlTag::Ul
            });
            if let Some(s) = start {
                el.attrs.insert("start".to_string(), s.to_string());
            }
            el
        }
        Tag::Item => simple(HtmlTag::Li),
        Tag::Emphasis => simple(HtmlTag::Em),
        Tag::Strong => simple(HtmlTag::Strong),
        Tag::Strikethrough => simple(HtmlTag::Del),
        Tag::Link { dest_url, title, .. } => {
            let mut el = simple(HtmlTag::A);
            el.attrs.insert("href".to_string(), dest_url.to_string());
            if !title.is_empty() {
                el.attrs.insert("title".to_string(), title.to_string());
            }
            el
        }
        Tag::Image { dest_url, title, .. } => {
            let mut el = simple(HtmlTag::Img);
            el.attrs.insert("src".to_string(), dest_url.to_string());
            if !title.is_empty() {
                el.attrs.insert("title".to_string(), title.to_string());
            }
            el
        }
        Tag::Table(_alignments) => simple(HtmlTag::Table),
        Tag::TableHead => simple(HtmlTag::Thead),
        Tag::TableRow => simple(HtmlTag::Tr),
        Tag::TableCell => simple(HtmlTag::Td),
        Tag::DefinitionList => simple(HtmlTag::Dl),
        Tag::DefinitionListTitle => simple(HtmlTag::Dt),
        Tag::DefinitionListDefinition => simple(HtmlTag::Dd),
        _ => simple(HtmlTag::Span),
    }
}

fn simple(tag: HtmlTag) -> HtmlElement {
    HtmlElement {
        tag,
        attrs: HashMap::new(),
        children: Vec::new(),
    }
}

fn push_text(stack: &mut [HtmlElement], root: &mut HtmlElement, text: &str) {
    let node = HtmlNode::Text(text.to_string());
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        root.children.push(node);
    }
}

/// 把 `el` 挂到当前父（或根），`el` 保持打开（不 push 到 stack，由调用者决定）。
/// 这里用于无子节点的空元素（br/hr/img/input）。
fn attach(stack: &mut [HtmlElement], root: &mut HtmlElement, el: &mut HtmlElement) {
    let node = HtmlNode::Element(HtmlElement {
        tag: el.tag,
        attrs: std::mem::take(&mut el.attrs),
        children: std::mem::take(&mut el.children),
    });
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        root.children.push(node);
    }
}

/// 将文档中所有本地图片（`img[src]` 指向本地文件）内联为 base64 data URI。
///
/// 对应输出层 `output::html::embed_local_images` 的 DOM 版：Markdown 输入路径走直连
/// DOM，故在此对 `HtmlDocument` 直接做内联，避免先序列化再走字符串 regex。
pub fn inline_local_images(doc: &mut HtmlDocument, base_dir: Option<&Path>) {
    let resolver = super::resource::ResourceResolver::new(base_dir.map(Path::to_path_buf));
    super::resource::embed_images(doc, &resolver);
}
