//! HTML 解析器
//!
//! 使用 html5ever 将 HTML 字符串解析为 HtmlDocument。
//! 通过自定义 TreeSink 直接生成我们的 HtmlElement 树，
//! 不再依赖已被移除的 RcDom。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use html5ever::driver::ParseOpts;
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::tree_builder::TreeBuilderOpts;
use html5ever::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::{Attribute, ExpandedName, LocalName, QualName, parse_document, parse_fragment};

use super::ast::*;

// ─── Custom DOM TreeSink ───────────────────────────────────

/// Simple DOM node for building our HtmlElement tree.
///
/// Design note: element_name is stored outside RefCell so that
/// `TreeSink::elem_name()` can return references without lifetime conflicts.
struct DomNode {
    parent: Cell<Option<Weak<DomNode>>>,
    children: RefCell<Vec<Rc<DomNode>>>,
    /// Element name; None for non-element nodes (document, text, comment)
    element_name: Option<QualName>,
    /// Element attributes (mutated by add_attrs_if_missing)
    element_attrs: RefCell<Vec<Attribute>>,
    /// Text content for text nodes
    text_content: RefCell<Option<StrTendril>>,
    /// Comment content for comment nodes
    _comment_content: RefCell<Option<StrTendril>>,
    /// Whether this is the document node
    is_document: bool,
}

type Handle = Rc<DomNode>;

/// Our custom TreeSink that builds a DOM tree
struct DomSink {
    document: Handle,
    _errors: Vec<String>,
    _quirks_mode: QuirksMode,
}

impl DomSink {
    fn new() -> Self {
        DomSink {
            document: Rc::new(DomNode {
                parent: Cell::new(None),
                children: RefCell::new(Vec::new()),
                element_name: None,
                element_attrs: RefCell::new(Vec::new()),
                text_content: RefCell::new(None),
                _comment_content: RefCell::new(None),
                is_document: true,
            }),
            _errors: Vec::new(),
            _quirks_mode: QuirksMode::NoQuirks,
        }
    }

    /// Convert the internal DOM tree to our HtmlDocument
    fn into_html_document(&self) -> HtmlDocument {
        let root = self.collect_children(&self.document);
        let html_elem = root
            .into_iter()
            .find_map(|node| match node {
                HtmlNode::Element(e) if e.tag == HtmlTag::Html || e.tag == HtmlTag::Body => Some(e),
                _ => None,
            })
            .unwrap_or_else(|| HtmlElement {
                tag: HtmlTag::Unknown,
                attrs: HashMap::new(),
                children: Vec::new(),
            });

        let style_sheets = extract_style_sheets(&html_elem);
        HtmlDocument {
            root: html_elem,
            style_sheets,
        }
    }

    fn collect_children(&self, handle: &Handle) -> Vec<HtmlNode> {
        self.collect_children_with_opts(handle, false)
    }

    /// 收集子节点，`in_pre` 为 true 时不 trim 文本节点（保留 <pre> 内的原始空白）
    fn collect_children_with_opts(&self, handle: &Handle, in_pre: bool) -> Vec<HtmlNode> {
        let mut result = Vec::new();
        let mut pending_text = String::new();

        for child in handle.children.borrow().iter() {
            if let Some(ref name) = child.element_name {
                // Element node - flush pending text first
                let text = if in_pre {
                    pending_text.clone()
                } else {
                    pending_text.trim().to_string()
                };
                if !text.is_empty() {
                    result.push(HtmlNode::Text(text));
                }
                pending_text.clear();

                let tag_name = name.local.as_ref().to_string();
                let attr_map = {
                    let attrs = child.element_attrs.borrow();
                    let mut map = HashMap::new();
                    for attr in attrs.iter() {
                        map.insert(attr.name.local.as_ref().to_string(), attr.value.to_string());
                    }
                    map
                };

                let child_in_pre = tag_name == "pre" || (in_pre && tag_name != "pre");
                let children = self.collect_children_with_opts(child, child_in_pre);

                let children = if tag_name == "head" {
                    children
                        .into_iter()
                        .filter(|c| {
                            if let HtmlNode::Element(e) = c {
                                e.tag == HtmlTag::Style
                            } else {
                                false
                            }
                        })
                        .collect()
                } else {
                    children
                };

                result.push(HtmlNode::Element(HtmlElement {
                    tag: HtmlTag::from_str(&tag_name),
                    attrs: attr_map,
                    children,
                }));
            } else if child.is_document {
                // Document node - recurse into children, preserving in_pre context
                let doc_children = self.collect_children_with_opts(child, in_pre);
                result.extend(doc_children);
            } else {
                // Text or comment node
                if let Some(ref text) = *child.text_content.borrow() {
                    pending_text.push_str(text.as_ref());
                }
            }
        }

        // Flush remaining text
        let text = if in_pre {
            pending_text.clone()
        } else {
            pending_text.trim().to_string()
        };
        if !text.is_empty() {
            result.push(HtmlNode::Text(text));
        }

        result
    }
}

impl TreeSink for DomSink {
    type Handle = Handle;
    type Output = HtmlDocument;
    type ElemName<'a> = ExpandedName<'a>;

    fn finish(self) -> HtmlDocument {
        self.into_html_document()
    }

    fn parse_error(&self, _msg: std::borrow::Cow<'static, str>) {}

    fn get_document(&self) -> Handle {
        self.document.clone()
    }

    fn elem_name<'a>(&'a self, target: &'a Handle) -> ExpandedName<'a> {
        match target.element_name {
            Some(ref name) => name.expanded(),
            None => panic!("elem_name called on non-element node"),
        }
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        _flags: ElementFlags,
    ) -> Handle {
        Rc::new(DomNode {
            parent: Cell::new(None),
            children: RefCell::new(Vec::new()),
            element_name: Some(name),
            element_attrs: RefCell::new(attrs),
            text_content: RefCell::new(None),
            _comment_content: RefCell::new(None),
            is_document: false,
        })
    }

    fn create_comment(&self, text: StrTendril) -> Handle {
        Rc::new(DomNode {
            parent: Cell::new(None),
            children: RefCell::new(Vec::new()),
            element_name: None,
            element_attrs: RefCell::new(Vec::new()),
            text_content: RefCell::new(None),
            _comment_content: RefCell::new(Some(text)),
            is_document: false,
        })
    }

    fn create_pi(&self, _target: StrTendril, _data: StrTendril) -> Handle {
        Rc::new(DomNode {
            parent: Cell::new(None),
            children: RefCell::new(Vec::new()),
            element_name: None,
            element_attrs: RefCell::new(Vec::new()),
            text_content: RefCell::new(None),
            _comment_content: RefCell::new(Some(_data)),
            is_document: false,
        })
    }

    fn append(&self, parent: &Handle, child: NodeOrText<Handle>) {
        let child = match child {
            NodeOrText::AppendNode(node) => node,
            NodeOrText::AppendText(text) => {
                // Clone the last child handle if it's a text node we can merge with
                let last_handle = {
                    let children = parent.children.borrow();
                    children.last().and_then(|last| {
                        if last.element_name.is_none() && !last.is_document {
                            Some(Rc::clone(last))
                        } else {
                            None
                        }
                    })
                };
                if let Some(last) = last_handle {
                    let mut last_text = last.text_content.borrow_mut();
                    if let Some(ref mut existing) = *last_text {
                        existing.push_slice(&text);
                        return;
                    }
                }
                Rc::new(DomNode {
                    parent: Cell::new(None),
                    children: RefCell::new(Vec::new()),
                    element_name: None,
                    element_attrs: RefCell::new(Vec::new()),
                    text_content: RefCell::new(Some(text)),
                    _comment_content: RefCell::new(None),
                    is_document: false,
                })
            }
        };
        child.parent.set(Some(Rc::downgrade(parent)));
        parent.children.borrow_mut().push(child);
    }

    fn append_based_on_parent_node(
        &self,
        _element: &Handle,
        prev_element: &Handle,
        child: NodeOrText<Handle>,
    ) {
        self.append(prev_element, child);
    }

    fn append_doctype_to_document(
        &self,
        _name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
        // Ignore doctype
    }

    fn get_template_contents(&self, target: &Handle) -> Handle {
        target.clone()
    }

    fn same_node(&self, x: &Handle, y: &Handle) -> bool {
        Rc::ptr_eq(x, y)
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {}

    fn append_before_sibling(&self, sibling: &Handle, new_node: NodeOrText<Handle>) {
        // Find the parent and insert before the sibling
        if let Some(parent_weak) = sibling.parent.take() {
            if let Some(parent) = parent_weak.upgrade() {
                sibling.parent.set(Some(parent_weak));
                let child = match new_node {
                    NodeOrText::AppendNode(node) => node,
                    NodeOrText::AppendText(text) => Rc::new(DomNode {
                        parent: Cell::new(None),
                        children: RefCell::new(Vec::new()),
                        element_name: None,
                        element_attrs: RefCell::new(Vec::new()),
                        text_content: RefCell::new(Some(text)),
                        _comment_content: RefCell::new(None),
                        is_document: false,
                    }),
                };
                child.parent.set(Some(Rc::downgrade(&parent)));
                let pos = parent
                    .children
                    .borrow()
                    .iter()
                    .position(|c| Rc::ptr_eq(c, sibling));
                if let Some(pos) = pos {
                    parent.children.borrow_mut().insert(pos, child);
                } else {
                    parent.children.borrow_mut().push(child);
                }
            } else {
                sibling.parent.set(Some(parent_weak));
                // Parent is gone, just append to orphan sibling as fallback
                let child = match new_node {
                    NodeOrText::AppendNode(node) => node,
                    NodeOrText::AppendText(text) => Rc::new(DomNode {
                        parent: Cell::new(None),
                        children: RefCell::new(Vec::new()),
                        element_name: None,
                        element_attrs: RefCell::new(Vec::new()),
                        text_content: RefCell::new(Some(text)),
                        _comment_content: RefCell::new(None),
                        is_document: false,
                    }),
                };
                child.parent.set(None);
                sibling.children.borrow_mut().push(child);
            }
        }
    }

    fn add_attrs_if_missing(&self, target: &Handle, attrs: Vec<Attribute>) {
        let mut existing = target.element_attrs.borrow_mut();
        for attr in attrs {
            if !existing.iter().any(|a| a.name == attr.name) {
                existing.push(attr);
            }
        }
    }

    fn remove_from_parent(&self, target: &Handle) {
        if let Some(parent_weak) = target.parent.take()
            && let Some(parent) = parent_weak.upgrade()
        {
            parent
                .children
                .borrow_mut()
                .retain(|c| !Rc::ptr_eq(c, target));
        }
    }

    fn reparent_children(&self, node: &Handle, new_parent: &Handle) {
        let children = std::mem::take(&mut *node.children.borrow_mut());
        for child in children {
            child.parent.set(Some(Rc::downgrade(new_parent)));
            new_parent.children.borrow_mut().push(child);
        }
    }
}

// ─── Public API ────────────────────────────────────────────

/// 将 HTML 字符串解析为 HtmlDocument（完整文档模式）
pub fn parse_html(html: &str) -> HtmlDocument {
    let opts = ParseOpts {
        tree_builder: TreeBuilderOpts {
            drop_doctype: true,
            ..Default::default()
        },
        ..Default::default()
    };

    parse_document(DomSink::new(), opts)
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .expect("html5ever parse failed")
}

/// 将 HTML 字符串解析为 HtmlDocument（明确文档语义的别名）
pub fn parse_html_document(html: &str) -> HtmlDocument {
    parse_html(html)
}

/// 解析 HTML 片段，返回顶级节点列表
pub fn parse_html_fragment(html: &str) -> Vec<HtmlNode> {
    let opts = ParseOpts {
        tree_builder: TreeBuilderOpts {
            ..Default::default()
        },
        ..Default::default()
    };

    let doc: HtmlDocument = parse_fragment(
        DomSink::new(),
        opts,
        QualName::new(None, html5ever::ns!(html), LocalName::from("body")),
        vec![],
        false,
    )
    .from_utf8()
    .read_from(&mut html.as_bytes())
    .expect("html5ever parse_fragment failed");

    // Extract children from the parsed document
    if let Some(body) = doc.root.find(HtmlTag::Body) {
        body.children.clone()
    } else {
        doc.root.children.clone()
    }
}

// ─── Helpers ───────────────────────────────────────────────

/// 从 HTML 树中提取所有 <style> 标签内容
fn extract_style_sheets(root: &HtmlElement) -> Vec<String> {
    let mut sheets = Vec::new();
    collect_styles(root, &mut sheets);
    sheets
}

fn collect_styles(element: &HtmlElement, sheets: &mut Vec<String>) {
    if element.tag == HtmlTag::Style {
        for child in &element.children {
            sheets.push(child.text_content());
        }
        return;
    }
    for child in &element.children {
        if let HtmlNode::Element(elem) = child {
            collect_styles(elem, sheets);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_html() {
        let html = "<p>Hello <strong>world</strong></p>";
        let doc = parse_html(html);
        assert!(!doc.root.children.is_empty());
    }

    #[test]
    fn test_parse_style_extraction() {
        let html = "<style>h1 { color: red; }</style><h1>Hello</h1>";
        let doc = parse_html(html);
        assert_eq!(doc.style_sheets.len(), 1);
        assert!(doc.style_sheets[0].contains("color: red"));
    }

    #[test]
    fn test_element_classes() {
        let html = r#"<div class="foo bar">content</div>"#;
        let doc = parse_html(html);
        if let Some(elem) = doc.root.find(HtmlTag::Div) {
            let classes = elem.classes();
            assert_eq!(classes.len(), 2);
            assert!(classes.contains(&"foo".to_string()));
            assert!(classes.contains(&"bar".to_string()));
        } else {
            panic!("Div not found");
        }
    }

    #[test]
    fn test_parse_fragment_basic() {
        let nodes = parse_html_fragment("<p>Hello</p><span>World</span>");
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn test_parse_empty() {
        let doc = parse_html("");
        // html5ever 会为空白输入生成 <html><head></head><body></body></html>
        assert_eq!(doc.root.tag, HtmlTag::Html, "Expected Html element root");
        assert!(!doc.root.children.is_empty(), "Should have head/body");
    }
}
