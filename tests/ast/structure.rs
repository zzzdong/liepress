//! AST 结构测试

use liepress::ast::{parse_markdown, NodeKind};

#[test]
fn test_parse_unordered_list() {
    let md = "- Item 1\n- Item 2\n- Item 3";
    let node = parse_markdown(md);

    match &node.kind {
        NodeKind::Document { children } => {
            assert_eq!(children.len(), 1);
            match &children[0].kind {
                NodeKind::List { ordered, children, .. } => {
                    assert!(!ordered);
                    assert_eq!(children.len(), 3);
                }
                _ => panic!("Expected List node"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_ordered_list() {
    let md = "1. First\n2. Second\n3. Third";
    let node = parse_markdown(md);

    match &node.kind {
        NodeKind::Document { children } => {
            match &children[0].kind {
                NodeKind::List { ordered, .. } => {
                    assert!(*ordered);
                }
                _ => panic!("Expected ordered List"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_nested_list() {
    let md = "- Item 1\n  - Sub 1\n  - Sub 2\n- Item 2";
    let node = parse_markdown(md);

    match &node.kind {
        NodeKind::Document { children } => {
            match &children[0].kind {
                NodeKind::List { children: items, .. } => {
                    assert_eq!(items.len(), 2);
                    // First item has nested list
                    match &items[0].kind {
                        NodeKind::ListItem { children } => {
                            assert!(children.len() > 1);
                        }
                        _ => panic!("Expected ListItem"),
                    }
                }
                _ => panic!("Expected List"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_code_block() {
    let md = "```rust\nfn main() {}\n```";
    let node = parse_markdown(md);

    match &node.kind {
        NodeKind::Document { children } => {
            assert_eq!(children.len(), 1);
            match &children[0].kind {
                NodeKind::CodeBlock { lang, code } => {
                    assert_eq!(lang.as_deref(), Some("rust"));
                    assert_eq!(code, "fn main() {}");
                }
                _ => panic!("Expected CodeBlock"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_codeblock_without_lang() {
    let md = "```\nsome code\n```";
    let node = parse_markdown(md);

    match &node.kind {
        NodeKind::Document { children } => {
            match &children[0].kind {
                NodeKind::CodeBlock { lang, code } => {
                    assert!(lang.is_none());
                    assert_eq!(code, "some code");
                }
                _ => panic!("Expected CodeBlock"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_blockquote() {
    let md = "> This is a quote";
    let node = parse_markdown(md);

    match &node.kind {
        NodeKind::Document { children } => {
            assert_eq!(children.len(), 1);
            match &children[0].kind {
                NodeKind::Blockquote { children } => {
                    assert!(!children.is_empty());
                }
                _ => panic!("Expected Blockquote"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_thematic_break() {
    let md = "---";
    let node = parse_markdown(md);

    match &node.kind {
        NodeKind::Document { children } => {
            assert_eq!(children.len(), 1);
            match &children[0].kind {
                NodeKind::ThematicBreak => {}
                _ => panic!("Expected ThematicBreak"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_inline_formatting() {
    let md = "**bold** and *italic* and `code`";
    let node = parse_markdown(md);

    match &node.kind {
        NodeKind::Document { children } => {
            match &children[0].kind {
                NodeKind::Paragraph { children } => {
                    // Should have multiple inline elements
                    assert!(!children.is_empty());
                }
                _ => panic!("Expected Paragraph"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_link() {
    let md = "[link text](https://example.com)";
    let node = parse_markdown(md);

    match &node.kind {
        NodeKind::Document { children } => {
            match &children[0].kind {
                NodeKind::Paragraph { children } => {
                    match &children[0].kind {
                        NodeKind::Link { url, .. } => {
                            assert_eq!(url, "https://example.com");
                        }
                        _ => panic!("Expected Link"),
                    }
                }
                _ => panic!("Expected Paragraph"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_image() {
    let md = "![alt text](image.png)";
    let node = parse_markdown(md);

    match &node.kind {
        NodeKind::Document { children } => {
            match &children[0].kind {
                NodeKind::Paragraph { children } => {
                    match &children[0].kind {
                        NodeKind::Image { src, alt, .. } => {
                            assert_eq!(src, "image.png");
                            assert_eq!(alt, "alt text");
                        }
                        _ => panic!("Expected Image"),
                    }
                }
                _ => panic!("Expected Paragraph"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}
