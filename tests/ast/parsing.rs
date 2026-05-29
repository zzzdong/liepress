//! AST 解析测试

use liepress::ast::{parse_markdown, NodeKind};

#[test]
fn test_parse_simple_paragraph() {
    let md = "Hello, world!";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            assert_eq!(children.len(), 1);
            match &children[0].kind {
                NodeKind::Paragraph { children } => {
                    assert_eq!(children.len(), 1);
                    match &children[0].kind {
                        NodeKind::Text { text } => {
                            assert_eq!(text, "Hello, world!");
                        }
                        _ => panic!("Expected Text node"),
                    }
                }
                _ => panic!("Expected Paragraph node"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_heading() {
    let md = "# Heading 1\n\n## Heading 2";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            assert_eq!(children.len(), 2);

            match &children[0].kind {
                NodeKind::Heading { level, .. } => {
                    assert_eq!(*level, 1);
                }
                _ => panic!("Expected Heading level 1"),
            }

            match &children[1].kind {
                NodeKind::Heading { level, .. } => {
                    assert_eq!(*level, 2);
                }
                _ => panic!("Expected Heading level 2"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_all_heading_levels() {
    let md = "# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            assert_eq!(children.len(), 6);
            for (i, child) in children.iter().enumerate() {
                match &child.kind {
                    NodeKind::Heading { level, .. } => {
                        assert_eq!(*level, (i + 1) as u8);
                    }
                    _ => panic!("Expected Heading at position {}", i),
                }
            }
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_empty_document() {
    let md = "";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            assert!(children.is_empty());
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_multiple_paragraphs() {
    let md = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            assert_eq!(children.len(), 3);
            for child in children {
                match &child.kind {
                    NodeKind::Paragraph { .. } => {}
                    _ => panic!("Expected Paragraph"),
                }
            }
        }
        _ => panic!("Expected Document root"),
    }
}
