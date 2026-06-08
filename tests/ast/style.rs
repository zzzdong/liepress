//! AST 样式测试

use liepress::ast::{FontStyle, FontWeight, NodeKind, Style, parse_markdown};

#[test]
fn test_paragraph_has_style() {
    let md = "Simple paragraph";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            let paragraph = &children[0];
            // Paragraph should have default style
            assert!(paragraph.style.font_size_pt > 0.0);
        }
        _ => panic!("Expected Document"),
    }
}

#[test]
fn test_heading_style_by_level() {
    let md = "# H1\n## H2\n### H3";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            let h1_size = children[0].style.font_size_pt;
            let h2_size = children[1].style.font_size_pt;
            let h3_size = children[2].style.font_size_pt;

            assert!(h1_size > h2_size, "H1 should be larger than H2");
            assert!(h2_size > h3_size, "H2 should be larger than H3");
        }
        _ => panic!("Expected Document"),
    }
}

#[test]
fn test_code_block_has_monospace_font() {
    let md = "```\ncode\n```";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            match &children[0].kind {
                NodeKind::CodeBlock { .. } => {
                    // Should have monospace font family
                    assert!(!children[0].style.font_family.is_empty());
                }
                _ => panic!("Expected CodeBlock"),
            }
        }
        _ => panic!("Expected Document"),
    }
}

#[test]
fn test_inline_code_style() {
    let md = "Some `inline code` here";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            match &children[0].kind {
                NodeKind::Paragraph { children } => {
                    // Find inline code node
                    for child in children {
                        if let NodeKind::InlineCode { .. } = &child.kind {
                            // Inline code should have monospace font
                            assert!(!child.style.font_family.is_empty());
                            return;
                        }
                    }
                    panic!("Expected InlineCode node");
                }
                _ => panic!("Expected Paragraph"),
            }
        }
        _ => panic!("Expected Document"),
    }
}

#[test]
fn test_link_has_color() {
    let md = "[link](http://example.com)";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            match &children[0].kind {
                NodeKind::Paragraph { children } => {
                    for child in children {
                        if let NodeKind::Link { .. } = &child.kind {
                            // Links typically have blue color
                            assert_eq!(child.style.color.r, 0);
                            assert_eq!(child.style.color.g, 0);
                            assert_eq!(child.style.color.b, 255);
                            // Links have URL set
                            assert_eq!(child.style.link_url.as_deref(), Some("http://example.com"));
                            return;
                        }
                    }
                    panic!("Expected Link node");
                }
                _ => panic!("Expected Paragraph"),
            }
        }
        _ => panic!("Expected Document"),
    }
}

#[test]
fn test_strong_has_bold_weight() {
    let md = "**bold text**";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => match &children[0].kind {
            NodeKind::Paragraph { children } => {
                for child in children {
                    if let NodeKind::Strong { .. } = &child.kind {
                        assert_eq!(child.style.font_weight, FontWeight::Bold);
                        return;
                    }
                }
                panic!("Expected Strong node");
            }
            _ => panic!("Expected Paragraph"),
        },
        _ => panic!("Expected Document"),
    }
}

#[test]
fn test_emphasis_has_italic_style() {
    let md = "*italic text*";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => match &children[0].kind {
            NodeKind::Paragraph { children } => {
                for child in children {
                    if let NodeKind::Emphasis { .. } = &child.kind {
                        assert_eq!(child.style.font_style, FontStyle::Italic);
                        return;
                    }
                }
                panic!("Expected Emphasis node");
            }
            _ => panic!("Expected Paragraph"),
        },
        _ => panic!("Expected Document"),
    }
}
