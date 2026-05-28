//! 文档生成测试

use liepress::generator::{markdown_to_document, constants::*};

#[test]
fn test_basic_document_structure() {
    let md = "# Title\n\nA paragraph.";
    let doc = markdown_to_document(md);

    // Should have at least one page
    assert!(!doc.pages.is_empty(), "Document should have at least one page");

    // First page should have elements
    let first_page = &doc.pages[0];
    assert!(!first_page.elements.is_empty(), "First page should have elements");
}

#[test]
fn test_page_dimensions() {
    let md = "Simple text.";
    let doc = markdown_to_document(md);

    for page in &doc.pages {
        // Check page dimensions match A4
        assert!((page.width - PAGE_WIDTH_PT).abs() < 0.1, "Page width should be A4");
        assert!((page.height - PAGE_HEIGHT_PT).abs() < 0.1, "Page height should be A4");
    }
}

#[test]
fn test_empty_document() {
    let md = "";
    let doc = markdown_to_document(md);

    // Empty document produces no pages
    assert!(doc.pages.is_empty(), "Empty document should have no pages");
}

#[test]
fn test_single_paragraph() {
    let md = "Just one paragraph.";
    let doc = markdown_to_document(md);

    assert_eq!(doc.pages.len(), 1, "Single paragraph should fit on one page");
}

#[test]
fn test_multiple_headings() {
    let md = "# H1\n\n## H2\n\n### H3";
    let doc = markdown_to_document(md);

    // Should have elements for each heading
    let first_page = &doc.pages[0];
    assert!(!first_page.elements.is_empty());
}
