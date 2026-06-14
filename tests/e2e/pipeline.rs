//! 端到端集成测试

use crate::common::samples;
use crate::common::{assert_valid_pdf, pdf_page_count};
use liepress::{ConvertOptions, markdown_to_pdf, markdown_to_png, markdown_to_svg};

#[test]
fn test_full_pipeline_svg() {
    let svgs = markdown_to_svg(samples::FULL_FEATURED, &ConvertOptions::default())
        .expect("Full SVG pipeline should succeed");

    assert!(!svgs.is_empty(), "SVG should produce pages");
    for svg in &svgs {
        assert!(svg.contains("<svg"), "Should produce valid SVG");
    }
}

#[test]
fn test_full_pipeline_png() {
    let pngs = markdown_to_png(samples::FULL_FEATURED, &ConvertOptions::default())
        .expect("Full PNG pipeline should succeed");

    assert!(!pngs.is_empty(), "PNG should produce pages");
    for png in &pngs {
        assert_eq!(
            &png[0..8],
            &[137, 80, 78, 71, 13, 10, 26, 10],
            "Should produce valid PNG"
        );
    }
}

#[test]
fn test_all_backends_produce_consistent_page_count() {
    let md = samples::FULL_FEATURED;

    let pdf_data = markdown_to_pdf(md, &ConvertOptions::default()).unwrap();
    let svgs = markdown_to_svg(md, &ConvertOptions::default()).unwrap();
    let pngs = markdown_to_png(md, &ConvertOptions::default()).unwrap();

    // Use lopdf to count pages accurately
    let pdf_pages = pdf_page_count(&pdf_data);

    println!(
        "PDF pages (lopdf): {}, SVG pages: {}, PNG pages: {}",
        pdf_pages,
        svgs.len(),
        pngs.len()
    );

    assert!(!svgs.is_empty(), "SVG should have pages");
    assert_eq!(
        svgs.len(),
        pngs.len(),
        "SVG and PNG should have same page count"
    );
    // PDF should also match (FULL_FEATURED fits in 1 page)
    assert_eq!(pdf_pages, svgs.len(), "PDF pages should match SVG pages");
}

#[test]
fn test_complex_document() {
    let md = r#"# Main Title

This is an introduction paragraph with **bold** and *italic* text.

## Section 1

- Point 1 with some explanation
- Point 2 with more details
- Point 3

```rust
fn example() -> i32 {
    42
}
```

## Section 2

> A wise quote goes here.

Some `inline code` for demonstration.

---

[Visit Example](https://example.com)"#;

    let pdf = markdown_to_pdf(md, &ConvertOptions::default()).expect("PDF should succeed");
    let svg = markdown_to_svg(md, &ConvertOptions::default()).expect("SVG should succeed");
    let png = markdown_to_png(md, &ConvertOptions::default()).expect("PNG should succeed");

    let _doc = assert_valid_pdf(&pdf);

    assert!(!svg.is_empty(), "SVG should have pages");
    assert!(!png.is_empty(), "PNG should have pages");
}

#[test]
fn test_unicode_content() {
    let md = r#"# Unicode Test

中文内容测试

日本語テキスト

한국어 텍스트

🎉 Emoji support 🚀

Math: α + β = γ"#;

    let pdf =
        markdown_to_pdf(md, &ConvertOptions::default()).expect("PDF with unicode should succeed");
    let svg =
        markdown_to_svg(md, &ConvertOptions::default()).expect("SVG with unicode should succeed");

    let _doc = assert_valid_pdf(&pdf);
    assert!(!svg.is_empty(), "SVG should have pages");
}

#[test]
fn test_special_characters() {
    let md = r#"# Special Chars

HTML entities: &lt; &gt; &amp;

Quotes: "double" and 'single'

Backslashes: \path\to\file

Symbols: © ® ™ § † ‡"#;

    let pdf = markdown_to_pdf(md, &ConvertOptions::default())
        .expect("PDF with special chars should succeed");
    let _doc = assert_valid_pdf(&pdf);
}

#[test]
fn test_nested_structures() {
    let md = r#"# Nested Test

1. First item
   - Sub item A
   - Sub item B
2. Second item
   1. Sub numbered 1
   2. Sub numbered 2
3. Third item

> Outer quote
> > Inner quote
> > > Deepest quote"#;

    let pdf = markdown_to_pdf(md, &ConvertOptions::default())
        .expect("PDF with nested structures should succeed");
    let svg = markdown_to_svg(md, &ConvertOptions::default())
        .expect("SVG with nested structures should succeed");

    let _doc = assert_valid_pdf(&pdf);
    assert!(!svg.is_empty(), "SVG should have pages");
}
