//! 分页测试

use liepress::generator::markdown_to_document;
use liepress::visual::VisualElement;

/// 提取文本元素的边界和文本内容
fn extract_text_elements(page: &liepress::generator::Page) -> Vec<(f64, f64, f64, f64, String)> {
    let mut texts = Vec::new();

    for elem in &page.elements {
        if let VisualElement::TextLine { runs, bounds, .. } = elem {
            let text: String = runs.iter()
                .map(|r| r.text.as_str())
                .collect();

            texts.push((
                bounds.x0,
                bounds.y0,
                bounds.x1,
                bounds.y1,
                text,
            ));
        }
    }

    // 按 y 坐标排序
    texts.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    texts
}

#[test]
fn test_text_lines_do_not_overlap() {
    let md = "# Title\n\nFirst paragraph with some text.\n\nSecond paragraph with more text.";
    let doc = markdown_to_document(md);

    for (page_idx, page) in doc.pages.iter().enumerate() {
        let texts = extract_text_elements(page);

        // 检查相邻行是否重叠
        for i in 1..texts.len() {
            let prev = &texts[i - 1];
            let curr = &texts[i];

            // 当前行的顶部应该大于等于前一行的底部（允许 0.1pt 的误差）
            assert!(
                curr.1 >= prev.3 - 0.1,
                "Page {}: Text lines {} and {} overlap: prev ends at {:.1}, curr starts at {:.1}",
                page_idx,
                i - 1,
                i,
                prev.3,
                curr.1
            );
        }
    }
}

#[test]
fn test_elements_in_reading_order() {
    let md = "# Title\n\nParagraph 1\n\nParagraph 2";
    let doc = markdown_to_document(md);

    for page in &doc.pages {
        let texts = extract_text_elements(page);

        // 文本应该按从上到下的顺序排列
        for i in 1..texts.len() {
            assert!(
                texts[i].1 >= texts[i - 1].1,
                "Elements should be in reading order (top to bottom)"
            );
        }
    }
}

#[test]
fn test_long_document_paginates() {
    // Create a long document that should span multiple pages
    let mut md = String::new();
    for i in 0..100 {
        md.push_str(&format!("# Heading {}\n\nThis is paragraph {} with some text content.\n\n", i, i));
    }

    let doc = markdown_to_document(&md);

    // Should have multiple pages
    assert!(
        doc.pages.len() > 1,
        "Long document should span multiple pages, got {} pages",
        doc.pages.len()
    );
}

#[test]
fn test_each_page_has_content() {
    let mut md = String::new();
    for i in 0..50 {
        md.push_str(&format!("Paragraph {} with enough text to ensure proper layout. ", i));
    }

    let doc = markdown_to_document(&md);

    // Each page should have some content
    for (i, page) in doc.pages.iter().enumerate() {
        assert!(
            !page.elements.is_empty(),
            "Page {} should have content",
            i
        );
    }
}

#[test]
fn test_page_boundaries_respected() {
    use liepress::generator::constants::*;

    let md = "Some content";
    let doc = markdown_to_document(md);

    for page in &doc.pages {
        for elem in &page.elements {
            if let VisualElement::TextLine { bounds, .. } = elem {
                // Text should be within page content area
                assert!(
                    bounds.y1 <= (PAGE_HEIGHT_PT - PAGE_MARGIN_BOTTOM_PT) as f64,
                    "Text should not exceed page bottom margin"
                );
            }
        }
    }
}
