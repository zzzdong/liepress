//! 简化 AST 模块
//!
//! 将 MDAST 节点转换为简化的、带样式的 Node。
//! 设计目标：
//! - 简化节点类型（heading、paragraph 等都可表示为带样式的文本块）
//! - 每个节点携带 Style，包含布局所需的所有样式信息
//! - 样式来源：CSS 样式表解析（内置 + 用户覆盖）

use markdown::mdast;

use super::css::*;
use super::style::*;

// ─── 简化 AST 节点定义 ───

/// 带样式的简化 AST 节点
///
/// 这是三层 AST 架构的 Layer 2。
/// 布局引擎只消费此结构，不关心 MDAST 或 CSS 细节。
#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub style: Style,
    /// 布局时是否可分割
    /// - true: 可在页面间分割（如段落、列表项）
    /// - false: 不可分割，必须保持在同一页（如标题、表格行）
    pub splittable: bool,
}

impl Node {
    pub fn new(kind: NodeKind, style: Style, splittable: bool) -> Self {
        Self {
            kind,
            style,
            splittable,
        }
    }

    /// 获取文本内容的拼接值
    pub fn text_content(&self) -> String {
        self.kind.text_content()
    }
}

/// 简化的节点类型
///
/// 相比 MDAST，此枚举做了以下简化：
/// 1. 合并了语义上相似的节点（如 Heading 和 Paragraph 都是文本容器）
/// 2. 保留了布局引擎需要的类型信息
/// 3. 每个节点通过 Style 区分视觉表现
#[derive(Debug, Clone)]
pub enum NodeKind {
    /// 文档根节点
    Document { children: Vec<Node> },

    /// 标题（h1-h6）
    /// 通过 style 中的 font_size, font_weight 等属性区分级别
    Heading { level: u8, children: Vec<Node> },

    /// 段落
    Paragraph { children: Vec<Node> },

    /// 列表
    List {
        ordered: bool,
        start: Option<u32>,
        children: Vec<Node>,
    },

    /// 列表项
    ListItem { children: Vec<Node> },

    /// 任务列表项（GFM 复选框）
    TaskListItem { checked: bool, children: Vec<Node> },

    /// 图片
    Image {
        src: String,
        alt: String,
        title: Option<String>,
    },

    /// 代码块
    CodeBlock { code: String, lang: Option<String> },

    /// 引用块
    Blockquote { children: Vec<Node> },

    /// 分隔线
    ThematicBreak,

    /// 表格
    Table {
        children: Vec<Node>,
        align: Vec<TextAlign>,
    },

    /// 表格行
    TableRow { children: Vec<Node> },

    // ── 内联节点 ──
    /// 纯文本（叶节点）
    Text { text: String },

    /// 加粗
    Strong { children: Vec<Node> },

    /// 斜体
    Emphasis { children: Vec<Node> },

    /// 行内代码
    InlineCode { code: String },

    /// 链接
    Link {
        url: String,
        title: Option<String>,
        children: Vec<Node>,
    },

    /// 删除线
    Delete { children: Vec<Node> },

    // ── HTML 容器节点 ──
    /// 行内容器 (<span>)
    Span { children: Vec<Node> },
    /// 居中块级容器 (<center>)
    Center { children: Vec<Node> },
}

impl NodeKind {
    /// 获取文本内容的拼接值
    pub fn text_content(&self) -> String {
        match self {
            NodeKind::Text { text } => text.clone(),
            NodeKind::Strong { children }
            | NodeKind::Emphasis { children }
            | NodeKind::Link { children, .. }
            | NodeKind::Delete { children } => {
                let mut s = String::new();
                for child in children {
                    s.push_str(&child.text_content());
                }
                s
            }
            NodeKind::InlineCode { code } => code.clone(),
            NodeKind::Heading { children, .. }
            | NodeKind::Paragraph { children }
            | NodeKind::ListItem { children }
            | NodeKind::TaskListItem { children, .. }
            | NodeKind::Blockquote { children }
            | NodeKind::TableRow { children }
            | NodeKind::Span { children }
            | NodeKind::Center { children } => {
                let mut s = String::new();
                for child in children {
                    s.push_str(&child.text_content());
                }
                s
            }
            _ => String::new(),
        }
    }
}

// ─── MDAST → Node 转换 ──────────────────────────────────────

/// 将 MDAST 根节点转换为简化 Node 树
///
/// 使用给定的样式解析器为每个节点解析样式。
pub fn build_ast(root: &mdast::Node, resolver: &StyleResolver) -> Node {
    build_node(root, resolver, &[], &Style::default())
}

fn build_node(
    node: &mdast::Node,
    resolver: &StyleResolver,
    ancestor_tags: &[String],
    parent_style: &Style,
) -> Node {
    match node {
        mdast::Node::Root(root) => {
            let tag = "body";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children =
                build_html_aware_children(&root.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Document { children }, style, false)
        }

        mdast::Node::Paragraph(_para) => {
            let tag = "p";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children = build_inline_children(&_para.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Paragraph { children }, style, true)
        }

        mdast::Node::Heading(heading) => {
            let tag = match heading.depth {
                1 => "h1",
                2 => "h2",
                3 => "h3",
                4 => "h4",
                5 => "h5",
                6 => "h6",
                _ => "h1",
            };
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children =
                build_inline_children(&heading.children, resolver, &new_ancestors, &style);
            Node::new(
                NodeKind::Heading {
                    level: heading.depth,
                    children,
                },
                style,
                false,
            )
        }

        mdast::Node::Code(code) => {
            let tag = "pre";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            Node::new(
                NodeKind::CodeBlock {
                    code: code.value.clone(),
                    lang: code.lang.clone(),
                },
                style,
                false,
            )
        }

        mdast::Node::Image(image) => {
            let tag = "img";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            Node::new(
                NodeKind::Image {
                    src: image.url.clone(),
                    alt: image.alt.clone(),
                    title: image.title.clone(),
                },
                style,
                false,
            )
        }

        mdast::Node::List(list) => {
            let tag = if list.ordered { "ol" } else { "ul" };
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = list
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(
                NodeKind::List {
                    ordered: list.ordered,
                    start: list.start,
                    children,
                },
                style,
                true,
            )
        }

        mdast::Node::ListItem(item) => {
            let tag = "li";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = item
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            match item.checked {
                Some(checked) => {
                    Node::new(NodeKind::TaskListItem { checked, children }, style, true)
                }
                None => Node::new(NodeKind::ListItem { children }, style, true),
            }
        }

        mdast::Node::Blockquote(blockquote) => {
            let tag = "blockquote";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = blockquote
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(NodeKind::Blockquote { children }, style, true)
        }

        mdast::Node::ThematicBreak(_) => {
            let tag = "hr";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            Node::new(NodeKind::ThematicBreak, style, false)
        }

        mdast::Node::Table(table) => {
            let tag = "table";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let align: Vec<TextAlign> = table
                .align
                .iter()
                .map(|a| match a {
                    mdast::AlignKind::Left => TextAlign::Left,
                    mdast::AlignKind::Right => TextAlign::Right,
                    mdast::AlignKind::Center => TextAlign::Center,
                    mdast::AlignKind::None => TextAlign::Left,
                })
                .collect();
            let children: Vec<Node> = table
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(NodeKind::Table { children, align }, style, false)
        }

        mdast::Node::TableRow(row) => {
            let tag = "tr";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = row
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(NodeKind::TableRow { children }, style, false)
        }

        mdast::Node::TableCell(cell) => {
            // 表格单元格使用 td 标签，但样式与段落类似
            let tag = "td";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children = build_inline_children(&cell.children, resolver, &new_ancestors, &style);
            Node::new(NodeKind::Paragraph { children }, style, true)
        }

        // ── 内联节点 ──
        mdast::Node::Text(text) => {
            let tag = "span";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            Node::new(
                NodeKind::Text {
                    text: text.value.clone(),
                },
                style,
                true,
            )
        }

        mdast::Node::Strong(strong) => {
            let tag = "strong";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = strong
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(NodeKind::Strong { children }, style, true)
        }

        mdast::Node::Emphasis(emph) => {
            let tag = "em";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = emph
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(NodeKind::Emphasis { children }, style, true)
        }

        mdast::Node::InlineCode(code) => {
            let tag = "code";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            Node::new(
                NodeKind::InlineCode {
                    code: code.value.clone(),
                },
                style,
                true,
            )
        }

        mdast::Node::Link(link) => {
            let tag = "a";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            // 设置链接 URL（CSS 无法设置动态 URL，所以从 MDAST 中取）
            let mut style = style;
            style.link_url = Some(link.url.clone());
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = link
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(
                NodeKind::Link {
                    url: link.url.clone(),
                    title: link.title.clone(),
                    children,
                },
                style,
                true,
            )
        }

        mdast::Node::Delete(del) => {
            let tag = "del";
            let style = resolver.resolve_style(tag, &[], ancestor_tags, parent_style);
            let mut new_ancestors = ancestor_tags.to_vec();
            new_ancestors.push(tag.to_string());
            let children: Vec<Node> = del
                .children
                .iter()
                .map(|child| build_node(child, resolver, &new_ancestors, &style))
                .collect();
            Node::new(NodeKind::Delete { children }, style, true)
        }

        // HTML 节点——提取 <style> CSS 的已在 extract_style_css 中处理，
        // 使用 html5ever 统一分类处理
        mdast::Node::Html(html) => match classify_html(&html.value) {
            HtmlClassification::SelfClosing(info) => match info.tag.as_str() {
                "br" => Node::new(NodeKind::ThematicBreak, Style::default(), false),
                _ => Node::new(NodeKind::Text { text: String::new() }, Style::default(), false),
            },
            HtmlClassification::Container(info, inner_content) => {
                // 完整容器：<tag>内容</tag>，内部内容重新解析为 markdown
                let style =
                    resolver.resolve_style(&info.tag, &info.classes, ancestor_tags, parent_style);
                let mut style = style;
                if let Some(inline_css) = &info.inline_style {
                    resolver.apply_inline_style(&mut style, inline_css);
                }
                let mut new_ancestors = ancestor_tags.to_vec();
                new_ancestors.push(info.tag.clone());

                let inner_mdast =
                    markdown::to_mdast(&inner_content, &markdown::ParseOptions::default());
                let inner_children = match inner_mdast {
                    Ok(mdast::Node::Root(root)) => {
                        build_html_aware_children(&root.children, resolver, &new_ancestors, &style)
                    }
                    _ => vec![],
                };

                match info.tag.as_str() {
                    "center" => {
                        Node::new(NodeKind::Center { children: inner_children }, style, true)
                    }
                    _ => Node::new(NodeKind::Text { text: String::new() }, Style::default(), false),
                }
            }
            HtmlClassification::OpenTag(info) => {
                // 开放标签作为空容器等待配对——由 build_html_aware_children 处理
                let style =
                    resolver.resolve_style(&info.tag, &info.classes, ancestor_tags, parent_style);
                let mut style = style;
                if let Some(inline_css) = &info.inline_style {
                    resolver.apply_inline_style(&mut style, inline_css);
                }
                match info.tag.as_str() {
                    "center" => Node::new(NodeKind::Center { children: vec![] }, style, true),
                    "span" => Node::new(NodeKind::Span { children: vec![] }, style, true),
                    _ => Node::new(NodeKind::Text { text: String::new() }, Style::default(), false),
                }
            }
            HtmlClassification::CloseTag(_) | HtmlClassification::Text => {
                Node::new(NodeKind::Text { text: String::new() }, Style::default(), false)
            }
        },

        // MDAST 中的其他节点类型暂不处理
        _ => Node::new(
            NodeKind::Text {
                text: String::new(),
            },
            Style::default(),
            true,
        ),
    }
}

/// 构建内联子节点列表（带 <span> 处理）
fn build_inline_children(
    children: &[mdast::Node],
    resolver: &StyleResolver,
    ancestor_tags: &[String],
    parent_style: &Style,
) -> Vec<Node> {
    build_inline_html_aware_children(children, resolver, ancestor_tags, parent_style)
}

// ─── 辅助函数 ───

/// 遍历 Node 树，对每个节点执行回调函数
pub fn walk<F>(node: &Node, callback: &mut F)
where
    F: FnMut(&Node),
{
    callback(node);
    match &node.kind {
        NodeKind::Document { children }
        | NodeKind::Heading { children, .. }
        | NodeKind::Paragraph { children }
        | NodeKind::List { children, .. }
        | NodeKind::ListItem { children }
        | NodeKind::TaskListItem { children, .. }
        | NodeKind::Blockquote { children }
        | NodeKind::Table { children, .. }
        | NodeKind::TableRow { children }
        | NodeKind::Strong { children }
        | NodeKind::Emphasis { children }
        | NodeKind::Link { children, .. }
        | NodeKind::Delete { children }
        | NodeKind::Span { children }
        | NodeKind::Center { children } => {
            for child in children {
                walk(child, callback);
            }
        }
        _ => {}
    }
}

/// 收集 Node 树中所有文本节点的文本内容
pub fn collect_text(node: &Node) -> String {
    let mut result = String::new();
    walk(node, &mut |n| {
        if let NodeKind::Text { text } = &n.kind {
            result.push_str(text);
        }
    });
    result
}

// ─── HTML 标签解析 ───────────────────────────────────────

/// HTML 标签解析信息
#[derive(Debug, Clone)]
pub(crate) struct TagInfo {
    pub tag: String,
    pub classes: Vec<String>,
    #[allow(dead_code)]
    pub id: Option<String>,
    pub inline_style: Option<String>,
    pub is_close: bool,
    pub is_self_closing: bool,
}

/// HTML 节点分类结果——统一取代 parse_html_tag + parse_html_block 的两路拆分
enum HtmlClassification {
    /// 自封闭标签 `<br/>`
    SelfClosing(TagInfo),
    /// 开放标签 `<center>`——由 build_html_aware_children 配对
    OpenTag(TagInfo),
    /// 关闭标签 `</center>`
    CloseTag(TagInfo),
    /// 完整容器 `<center>内容</center>`
    Container(TagInfo, String),
    /// 纯文本（无标签）
    Text,
}

/// 使用 html5ever tokenizer 对 HTML 字符串进行一次完整的 token 分析，
/// 统一返回分类结果，无需调用者再行拆分处理。
fn classify_html(html: &str) -> HtmlClassification {
    use std::cell::RefCell;
    use std::rc::Rc;

    use html5ever::tokenizer::{BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer};
    use html5ever::tendril::StrTendril;

    let html = html.trim();

    // 快速路径：不以 < 开头，直接视为 Text
    if !html.starts_with('<') {
        return HtmlClassification::Text;
    }

    /// Token 简化为分类器需要的信息
    #[derive(Debug, Clone)]
    enum SimpleToken {
        StartTag { name: String, self_closing: bool },
        EndTag { name: String },
        Other,
    }

    struct ClassifySink {
        tokens: Rc<RefCell<Vec<SimpleToken>>>,
    }

    impl TokenSink for ClassifySink {
        type Handle = ();

        fn process_token(&self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
            match token {
                Token::TagToken(tag) => match tag.kind {
                    TagKind::StartTag => {
                        self.tokens.borrow_mut().push(SimpleToken::StartTag {
                            name: tag.name.to_string(),
                            self_closing: tag.self_closing,
                        });
                    }
                    TagKind::EndTag => {
                        self.tokens.borrow_mut().push(SimpleToken::EndTag {
                            name: tag.name.to_string(),
                        });
                    }
                },
                _ => {
                    self.tokens.borrow_mut().push(SimpleToken::Other);
                }
            }
            TokenSinkResult::Continue
        }
    }

    // Step 1: 先用 tokenizer 收集所有 token
    let tokens_rc = Rc::new(RefCell::new(Vec::new()));
    let sink = ClassifySink {
        tokens: tokens_rc.clone(),
    };
    let mut tokenizer = Tokenizer::new(sink, Default::default());
    let mut input = BufferQueue::default();
    input.push_back(StrTendril::from(html));
    let _ = tokenizer.feed(&mut input);
    tokenizer.end();

    let tokens = tokens_rc.take(); // Vec<SimpleToken>

    // Step 2: 分析 token 序列
    // 找到第一个 Tag token 作为主要标签
    let first_tag_idx = match tokens.iter().position(|t| !matches!(t, SimpleToken::Other)) {
        Some(i) => i,
        None => return HtmlClassification::Text,
    };
    let first_tag = &tokens[first_tag_idx];

    match first_tag {
        SimpleToken::StartTag {
            name: _,
            self_closing: true,
        } => {
            // 自封闭标签
            HtmlClassification::SelfClosing(parse_html_tag(html).unwrap())
        }
        SimpleToken::StartTag {
            name,
            self_closing: false,
        } => {
            // 查找匹配的 EndTag
            let mut depth: i32 = 1;
            let mut end_idx = None;
            for (i, t) in tokens.iter().enumerate().skip(first_tag_idx + 1) {
                match t {
                    SimpleToken::StartTag { name: n, .. } if n == name => depth += 1,
                    SimpleToken::EndTag { name: n } if n == name => {
                        depth -= 1;
                        if depth == 0 {
                            end_idx = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }

            if end_idx.is_some() {
                // 是完整容器
                // 从原始字符串提取内部内容：第一组 > 到 </name>
                let tag_start = format!("<{}", name);
                let name_start = match html.find(&tag_start) {
                    Some(pos) => pos,
                    None => return HtmlClassification::Text,
                };
                let after_open = match html[name_start..].find('>') {
                    Some(pos) => pos,
                    None => return HtmlClassification::Text,
                };
                let content_start = name_start + after_open + 1;
                let close_tag = format!("</{}>", name);
                let content_end = match html[content_start..].rfind(&close_tag) {
                    Some(pos) => pos,
                    None => return HtmlClassification::Text,
                };
                let inner = html[content_start..content_start + content_end].to_string();

                let info = match parse_html_tag(&html[name_start..=name_start + after_open]) {
                    Some(info) => info,
                    None => return HtmlClassification::Text,
                };
                HtmlClassification::Container(info, inner)
            } else {
                // 单独的开放标签
                HtmlClassification::OpenTag(parse_html_tag(html).unwrap())
            }
        }
        SimpleToken::EndTag { name } => HtmlClassification::CloseTag(TagInfo {
            tag: name.clone(),
            classes: vec![],
            id: None,
            inline_style: None,
            is_close: true,
            is_self_closing: false,
        }),
        _ => HtmlClassification::Text,
    }
}

/// 使用 html5ever 从 HTML 字符串中解析标签信息
///
/// 支持的格式：
/// - `<div>`              → tag=div, open
/// - `</div>`             → tag=div, close
/// - `<br/>`              → tag=br, self-closing
/// - `<div class="foo">`  → tag=div, class=[foo]
/// - `<div style="...">`  → tag=div, inline_style=...
/// - `<p class="a b">`    → tag=p, class=[a, b]
pub(crate) fn parse_html_tag(html: &str) -> Option<TagInfo> {
    use std::cell::RefCell;
    use std::rc::Rc;

    use html5ever::tokenizer::{BufferQueue, Tag, Token, TokenSink, TokenSinkResult, Tokenizer};
    use html5ever::tokenizer::TagKind;
    use html5ever::tendril::StrTendril;

    let html = html.trim();

    // 快速路径：必须以 < 开头，以 > 结尾
    if !html.starts_with('<') || !html.ends_with('>') {
        return None;
    }

    struct HtmlTagSink {
        result: Rc<RefCell<Option<TagInfo>>>,
    }

    impl TokenSink for HtmlTagSink {
        type Handle = ();

        fn process_token(&self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
            match token {
                Token::TagToken(tag) => match tag.kind {
                    TagKind::StartTag => {
                        let info = tag_to_info(tag);
                        *self.result.borrow_mut() = Some(info);
                    }
                    TagKind::EndTag => {
                        *self.result.borrow_mut() = Some(TagInfo {
                            tag: tag.name.to_string(),
                            classes: vec![],
                            id: None,
                            inline_style: None,
                            is_close: true,
                            is_self_closing: false,
                        });
                    }
                },
                _ => {}
            }
            TokenSinkResult::Continue
        }
    }

    fn tag_to_info(tag: Tag) -> TagInfo {
        let mut classes: Vec<String> = Vec::new();
        let mut id: Option<String> = None;
        let mut inline_style: Option<String> = None;

        for attr in tag.attrs {
            let name: String = attr.name.local.to_string();
            match name.to_lowercase().as_str() {
                "class" => {
                    classes = attr
                        .value
                        .to_string()
                        .split_whitespace()
                        .map(|s| s.to_string())
                        .collect();
                }
                "id" => {
                    id = Some(attr.value.to_string());
                }
                "style" => {
                    inline_style = Some(attr.value.to_string());
                }
                _ => {}
            }
        }

        TagInfo {
            tag: tag.name.to_string(),
            classes,
            id,
            inline_style,
            is_close: false,
            is_self_closing: tag.self_closing,
        }
    }

    let result = Rc::new(RefCell::new(None));
    let sink = HtmlTagSink {
        result: result.clone(),
    };
    let mut tokenizer = Tokenizer::new(sink, Default::default());

    let mut input = BufferQueue::default();
    input.push_back(StrTendril::from(html));
    let _ = tokenizer.feed(&mut input);
    tokenizer.end();

    let borrowed = result.borrow();
    borrowed.clone()
}

/// HTML 感知的子节点构建
///
/// 在文档级别扫描子节点列表，检测 HTML 开放/关闭标签配对，
/// 为 div/center 等块级容器创建嵌套的 Node 树。
fn build_html_aware_children(
    nodes: &[mdast::Node],
    resolver: &StyleResolver,
    ancestor_tags: &[String],
    parent_style: &Style,
) -> Vec<Node> {
    let mut result: Vec<Node> = Vec::new();
    let mut i = 0;

    while i < nodes.len() {
        let node = &nodes[i];

        // 检查是否是 block-level HTML 开放标签
        if let mdast::Node::Html(html) = node
            && let Some(info) = parse_html_tag(&html.value)
            && !info.is_close
            && !info.is_self_closing
        {
            let is_container = matches!(info.tag.as_str(), "center");

            if is_container {
                // 找到匹配的关闭标签
                let mut inner_nodes: Vec<mdast::Node> = Vec::new();
                let mut depth = 1;
                let mut j = i + 1;
                while j < nodes.len() && depth > 0 {
                    if let mdast::Node::Html(inner_html) = &nodes[j]
                        && let Some(inner_info) = parse_html_tag(&inner_html.value)
                    {
                        if inner_info.is_close && inner_info.tag == info.tag {
                            depth -= 1;
                            if depth == 0 {
                                j += 1;
                                break;
                            }
                        } else if !inner_info.is_close
                            && !inner_info.is_self_closing
                            && inner_info.tag == info.tag
                        {
                            depth += 1;
                        }
                    }
                    if depth > 0 {
                        inner_nodes.push(nodes[j].clone());
                    }
                    j += 1;
                }

                // 解析信息
                let style =
                    resolver.resolve_style("center", &info.classes, ancestor_tags, parent_style);
                let mut style = style;
                if let Some(inline_css) = &info.inline_style {
                    resolver.apply_inline_style(&mut style, inline_css);
                }
                let mut new_ancestors = ancestor_tags.to_vec();
                new_ancestors.push("center".to_string());

                let children =
                    build_html_aware_children(&inner_nodes, resolver, &new_ancestors, &style);

                let node = Node::new(NodeKind::Center { children }, style, true);
                result.push(node);
                i = j;
                continue;
            }
        }

        // 非 HTML 容器节点，正常构建
        result.push(build_node(node, resolver, ancestor_tags, parent_style));
        i += 1;
    }

    result
}

/// 在 inline 级别构建子节点，处理 <span> 标签配对
pub(crate) fn build_inline_html_aware_children(
    nodes: &[mdast::Node],
    resolver: &StyleResolver,
    ancestor_tags: &[String],
    parent_style: &Style,
) -> Vec<Node> {
    let mut result: Vec<Node> = Vec::new();
    let mut i = 0;

    while i < nodes.len() {
        let node = &nodes[i];

        if let mdast::Node::Html(html) = node
            && let Some(info) = parse_html_tag(&html.value)
            && !info.is_close
            && !info.is_self_closing
            && info.tag == "span"
        {
            // 找到 </span>
            let mut inner_nodes: Vec<mdast::Node> = Vec::new();
            let mut j = i + 1;
            let mut found_close = false;
            while j < nodes.len() {
                if let mdast::Node::Html(inner_html) = &nodes[j]
                    && let Some(inner_info) = parse_html_tag(&inner_html.value)
                    && inner_info.is_close
                    && inner_info.tag == "span"
                {
                    found_close = true;
                    j += 1;
                    break;
                }
                inner_nodes.push(nodes[j].clone());
                j += 1;
            }

            if found_close {
                let style =
                    resolver.resolve_style("span", &info.classes, ancestor_tags, parent_style);
                let mut style = style;
                if let Some(inline_css) = &info.inline_style {
                    resolver.apply_inline_style(&mut style, inline_css);
                }
                let mut new_ancestors = ancestor_tags.to_vec();
                new_ancestors.push("span".to_string());
                let children = build_inline_html_aware_children(
                    &inner_nodes,
                    resolver,
                    &new_ancestors,
                    &style,
                );
                result.push(Node::new(NodeKind::Span { children }, style, true));
                i = j;
                continue;
            }
        }

        result.push(build_node(node, resolver, ancestor_tags, parent_style));
        i += 1;
    }

    result
}
