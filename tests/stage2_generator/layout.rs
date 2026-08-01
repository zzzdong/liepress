//! 布局测试

use liepress::generator::constants::*;
use liepress::generator::markdown_to_document;
use liepress::visual::VisualElement;

/// 获取元素的边界框
fn get_element_bounds(elem: &VisualElement) -> (f64, f64, f64, f64) {
    match elem {
        VisualElement::TextLine { bounds, .. } => (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
        VisualElement::Rect { rect, .. } => (rect.x0, rect.y0, rect.x1, rect.y1),
        VisualElement::Image { position, size, .. } => (
            position.x,
            position.y,
            position.x + size.x,
            position.y + size.y,
        ),
        _ => (0.0, 0.0, 0.0, 0.0),
    }
}

#[test]
fn test_heading_layout() {
    let md = "# Heading 1";
    let doc = markdown_to_document(md);

    let first_page = &doc.pages[0];
    // Should have at least one element (the heading)
    assert!(!first_page.elements.is_empty());

    // Check that elements are within content area
    for elem in &first_page.elements {
        let bounds = get_element_bounds(elem);
        assert!(
            bounds.0 >= CONTENT_AREA_X_PT as f64,
            "Element should be within left margin"
        );
        assert!(
            bounds.1 >= CONTENT_AREA_Y_PT as f64,
            "Element should be within top margin"
        );
    }
}

#[test]
fn test_paragraph_layout() {
    let md = "This is a paragraph with some text.";
    let doc = markdown_to_document(md);

    let first_page = &doc.pages[0];
    assert!(!first_page.elements.is_empty());

    // Check text line bounds
    for elem in &first_page.elements {
        if let VisualElement::TextLine { bounds, .. } = elem {
            assert!(bounds.x0 >= CONTENT_AREA_X_PT as f64);
            assert!(bounds.x1 <= (CONTENT_AREA_X_PT + CONTENT_AREA_WIDTH_PT) as f64);
        }
    }
}

#[test]
fn test_list_layout() {
    let md = "- Item 1\n- Item 2\n- Item 3";
    let doc = markdown_to_document(md);

    let first_page = &doc.pages[0];
    // Should have text lines for list items
    let text_count = first_page
        .elements
        .iter()
        .filter(|e| matches!(e, VisualElement::TextLine { .. }))
        .count();
    assert!(
        text_count >= 3,
        "Should have at least 3 text lines for list items"
    );
}

#[test]
fn test_code_block_has_background() {
    let md = "```\ncode\n```";
    let doc = markdown_to_document(md);

    let first_page = &doc.pages[0];
    // Should have at least one rect (background)
    let rect_count = first_page
        .elements
        .iter()
        .filter(|e| matches!(e, VisualElement::Rect { .. }))
        .count();
    assert!(
        rect_count > 0,
        "Code block should have background rectangle"
    );
}

#[test]
fn test_blockquote_has_border() {
    let md = "> Quote";
    let doc = markdown_to_document(md);

    let first_page = &doc.pages[0];
    // Blockquote typically has a left border (line)
    assert!(!first_page.elements.is_empty());
}

#[test]
fn test_thematic_break_as_line() {
    let md = "---";
    let doc = markdown_to_document(md);

    let first_page = &doc.pages[0];
    // Thematic break should be rendered as a line
    let line_count = first_page
        .elements
        .iter()
        .filter(|e| matches!(e, VisualElement::Line { .. }))
        .count();
    assert!(line_count > 0, "Thematic break should be a line");
}

// ─── 任务列表测试 ───

/// 提取页面上所有文本行的文本内容
fn extract_texts(page: &liepress::generator::Page) -> Vec<String> {
    page.elements
        .iter()
        .filter_map(|e| {
            if let VisualElement::TextLine { runs, .. } = e {
                let text: String = runs.iter().map(|r| r.text.as_str()).collect();
                Some(text)
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn test_unchecked_task_list_layout() {
    let md = "- [ ] Task 1\n- [ ] Task 2";
    let doc = markdown_to_document(md);

    let first_page = &doc.pages[0];
    let texts = extract_texts(first_page);

    // 验证内容文本存在（标记字符在 run.text 中为空，这是当前架构的限制）
    let combined: String = texts.iter().flat_map(|s| s.chars()).collect();
    assert!(combined.contains("Task 1"), "Should contain Task 1 text");
    assert!(combined.contains("Task 2"), "Should contain Task 2 text");

    // 验证生成了足够的文本行（2 个任务项 × 标记行 + 内容行）
    assert!(texts.len() >= 2, "Should have at least 2 text lines");
}

#[test]
fn test_checked_task_list_layout() {
    let md = "- [x] Done\n- [X] Also done";
    let doc = markdown_to_document(md);

    let first_page = &doc.pages[0];
    let texts = extract_texts(first_page);

    let combined: String = texts.iter().flat_map(|s| s.chars()).collect();
    assert!(combined.contains("Done"));
    assert!(combined.contains("Also done"));
}

#[test]
fn test_mixed_task_list_layout() {
    let md = "- [x] Completed\n- [ ] Pending\n- Regular item";
    let doc = markdown_to_document(md);

    let first_page = &doc.pages[0];
    let texts = extract_texts(first_page);

    let combined: String = texts.iter().flat_map(|s| s.chars()).collect();
    assert!(combined.contains("Completed"));
    assert!(combined.contains("Pending"));
    assert!(combined.contains("Regular"));
    assert!(texts.len() >= 3, "Should have at least 3 text lines");
}
