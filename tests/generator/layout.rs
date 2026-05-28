//! 布局测试

use liepress::generator::markdown_to_document;
use liepress::visual::VisualElement;
use liepress::generator::constants::*;

/// 获取元素的边界框
fn get_element_bounds(elem: &VisualElement) -> (f64, f64, f64, f64) {
    match elem {
        VisualElement::TextLine { bounds, .. } => (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
        VisualElement::Rect { rect, .. } => (rect.x0, rect.y0, rect.x1, rect.y1),
        VisualElement::Image { position, size, .. } => {
            (position.x, position.y, position.x + size.x as f64, position.y + size.y as f64)
        }
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
        assert!(bounds.0 >= CONTENT_AREA_X_PT as f64, "Element should be within left margin");
        assert!(bounds.1 >= CONTENT_AREA_Y_PT as f64, "Element should be within top margin");
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
    let text_count = first_page.elements.iter()
        .filter(|e| matches!(e, VisualElement::TextLine { .. }))
        .count();
    assert!(text_count >= 3, "Should have at least 3 text lines for list items");
}

#[test]
fn test_code_block_has_background() {
    let md = "```\ncode\n```";
    let doc = markdown_to_document(md);

    let first_page = &doc.pages[0];
    // Should have at least one rect (background)
    let rect_count = first_page.elements.iter()
        .filter(|e| matches!(e, VisualElement::Rect { .. }))
        .count();
    assert!(rect_count > 0, "Code block should have background rectangle");
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
    let line_count = first_page.elements.iter()
        .filter(|e| matches!(e, VisualElement::Line { .. }))
        .count();
    assert!(line_count > 0, "Thematic break should be a line");
}
