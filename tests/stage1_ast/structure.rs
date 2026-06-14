//! AST 结构测试

use liepress::ast::{NodeKind, parse_markdown};

#[test]
fn test_parse_unordered_list() {
    let md = "- Item 1\n- Item 2\n- Item 3";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            assert_eq!(children.len(), 1);
            match &children[0].kind {
                NodeKind::List {
                    ordered, children, ..
                } => {
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
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => match &children[0].kind {
            NodeKind::List { ordered, .. } => {
                assert!(*ordered);
            }
            _ => panic!("Expected ordered List"),
        },
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_nested_list() {
    let md = "- Item 1\n  - Sub 1\n  - Sub 2\n- Item 2";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            match &children[0].kind {
                NodeKind::List {
                    children: items, ..
                } => {
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
    let node = parse_markdown(md).unwrap();

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
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => match &children[0].kind {
            NodeKind::CodeBlock { lang, code } => {
                assert!(lang.is_none());
                assert_eq!(code, "some code");
            }
            _ => panic!("Expected CodeBlock"),
        },
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_blockquote() {
    let md = "> This is a quote";
    let node = parse_markdown(md).unwrap();

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
    let node = parse_markdown(md).unwrap();

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
    let node = parse_markdown(md).unwrap();

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
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => match &children[0].kind {
            NodeKind::Paragraph { children } => match &children[0].kind {
                NodeKind::Link { url, .. } => {
                    assert_eq!(url, "https://example.com");
                }
                _ => panic!("Expected Link"),
            },
            _ => panic!("Expected Paragraph"),
        },
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_image() {
    let md = "![alt text](image.png)";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => match &children[0].kind {
            NodeKind::Paragraph { children } => match &children[0].kind {
                NodeKind::Image { src, alt, .. } => {
                    assert_eq!(src, "image.png");
                    assert_eq!(alt, "alt text");
                }
                _ => panic!("Expected Image"),
            },
            _ => panic!("Expected Paragraph"),
        },
        _ => panic!("Expected Document root"),
    }
}

// ─── 任务列表测试 ───

#[test]
fn test_parse_unchecked_task_list() {
    let md = "- [ ] Buy groceries\n- [ ] Clean the house";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            assert_eq!(children.len(), 1);
            match &children[0].kind {
                NodeKind::List {
                    ordered,
                    children: items,
                    ..
                } => {
                    assert!(!ordered);
                    assert_eq!(items.len(), 2);

                    // 每个都是未勾选的任务列表项
                    match &items[0].kind {
                        NodeKind::TaskListItem { checked, children } => {
                            assert!(!checked, "First item should be unchecked");
                            assert!(!children.is_empty());
                        }
                        _ => panic!("Expected TaskListItem, got {:?}", items[0].kind),
                    }
                    match &items[1].kind {
                        NodeKind::TaskListItem { checked, .. } => {
                            assert!(!checked, "Second item should be unchecked");
                        }
                        _ => panic!("Expected TaskListItem"),
                    }
                }
                _ => panic!("Expected List node"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_checked_task_list() {
    let md = "- [x] Completed task\n- [X] Another done";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            match &children[0].kind {
                NodeKind::List {
                    children: items, ..
                } => {
                    assert_eq!(items.len(), 2);

                    // 第一个已勾选（小写 x）
                    match &items[0].kind {
                        NodeKind::TaskListItem { checked, .. } => {
                            assert!(*checked, "First item should be checked");
                        }
                        _ => panic!("Expected TaskListItem"),
                    }
                    // 第二个已勾选（大写 X，GFM 标准也支持）
                    match &items[1].kind {
                        NodeKind::TaskListItem { checked, .. } => {
                            assert!(*checked, "Second item (with X) should be checked");
                        }
                        _ => panic!("Expected TaskListItem"),
                    }
                }
                _ => panic!("Expected List node"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_parse_mixed_task_list() {
    let md = "- Regular item\n- [x] Task done\n- Another regular\n- [ ] Task pending";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            match &children[0].kind {
                NodeKind::List {
                    children: items, ..
                } => {
                    assert_eq!(items.len(), 4);

                    // 第 1 项：普通列表项
                    assert!(
                        matches!(items[0].kind, NodeKind::ListItem { .. }),
                        "Item 0 should be ListItem"
                    );
                    // 第 2 项：已勾选任务
                    match &items[1].kind {
                        NodeKind::TaskListItem { checked, .. } => assert!(*checked),
                        _ => panic!("Item 1 should be TaskListItem"),
                    }
                    // 第 3 项：普通列表项
                    assert!(
                        matches!(items[2].kind, NodeKind::ListItem { .. }),
                        "Item 2 should be ListItem"
                    );
                    // 第 4 项：未勾选任务
                    match &items[3].kind {
                        NodeKind::TaskListItem { checked, .. } => assert!(!*checked),
                        _ => panic!("Item 3 should be TaskListItem"),
                    }
                }
                _ => panic!("Expected List node"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}

#[test]
fn test_task_list_text_content() {
    use liepress::ast::collect_text;

    let md = "- [ ] Buy groceries\n- [x] Pay bills";
    let node = parse_markdown(md).unwrap();
    let text = collect_text(&node);

    assert!(text.contains("Buy groceries"));
    assert!(text.contains("Pay bills"));
}

#[test]
fn test_task_list_with_nested_content() {
    let md = "- [x] **Bold task**\n- [ ] *Italic subtask*";
    let node = parse_markdown(md).unwrap();

    match &node.kind {
        NodeKind::Document { children } => {
            match &children[0].kind {
                NodeKind::List {
                    children: items, ..
                } => {
                    assert_eq!(items.len(), 2);

                    // 第一个任务项：已勾选，且内容包含 Strong（直接作为子节点）
                    match &items[0].kind {
                        NodeKind::TaskListItem { checked, children } => {
                            assert!(*checked);
                            assert!(!children.is_empty());
                            // pulldown-cmark 在列表项中不添加 <p> 包裹，Strong 直接作为子节点
                            let has_strong = children
                                .iter()
                                .any(|c| matches!(c.kind, NodeKind::Strong { .. }));
                            assert!(
                                has_strong,
                                "Task item should contain Strong (got children kinds: {:?})",
                                children
                                    .iter()
                                    .map(|c| format!("{:?}", c.kind))
                                    .collect::<Vec<_>>()
                            );
                        }
                        _ => panic!("Expected TaskListItem"),
                    }

                    // 第二个任务项：未勾选，且内容包含 Emphasis（直接作为子节点）
                    match &items[1].kind {
                        NodeKind::TaskListItem { checked, children } => {
                            assert!(!*checked);
                            let has_em = children
                                .iter()
                                .any(|c| matches!(c.kind, NodeKind::Emphasis { .. }));
                            assert!(
                                has_em,
                                "Task item should contain Emphasis (got children kinds: {:?})",
                                children
                                    .iter()
                                    .map(|c| format!("{:?}", c.kind))
                                    .collect::<Vec<_>>()
                            );
                        }
                        _ => panic!("Expected TaskListItem"),
                    }
                }
                _ => panic!("Expected List node"),
            }
        }
        _ => panic!("Expected Document root"),
    }
}
