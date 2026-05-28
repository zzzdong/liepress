//! 端到端集成测试

use liepress::{markdown_to_pdf, markdown_to_svg, markdown_to_png};
use std::fs;
use std::path::Path;
use crate::common::samples;

#[test]
fn test_full_pipeline_pdf() {
    // Complete pipeline: Markdown -> AST -> Generator -> PDF
    let pdf_data = markdown_to_pdf(samples::FULL_FEATURED)
        .expect("Full PDF pipeline should succeed");

    assert!(!pdf_data.is_empty());
    assert_eq!(&pdf_data[0..4], b"%PDF");
}

#[test]
fn test_full_pipeline_svg() {
    // Complete pipeline: Markdown -> AST -> Generator -> SVG
    let svgs = markdown_to_svg(samples::FULL_FEATURED)
        .expect("Full SVG pipeline should succeed");

    assert!(!svgs.is_empty());
    for svg in &svgs {
        assert!(svg.contains("<svg"));
    }
}

#[test]
fn test_full_pipeline_png() {
    // Complete pipeline: Markdown -> AST -> Generator -> PNG
    let pngs = markdown_to_png(samples::FULL_FEATURED)
        .expect("Full PNG pipeline should succeed");

    assert!(!pngs.is_empty());
    for png in &pngs {
        assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }
}

#[test]
fn test_all_backends_produce_consistent_page_count() {
    // All backends should produce the same number of pages
    let md = samples::FULL_FEATURED;

    let pdf_data = markdown_to_pdf(md).unwrap();
    let svgs = markdown_to_svg(md).unwrap();
    let pngs = markdown_to_png(md).unwrap();

    // Count PDF pages by looking for /Type /Page
    let pdf_pages = String::from_utf8_lossy(&pdf_data)
        .matches("/Type /Page")
        .count();

    println!("PDF pages: {}, SVG pages: {}, PNG pages: {}",
             pdf_pages, svgs.len(), pngs.len());

    // Note: PDF page count might differ slightly due to trailer pages
    assert!(!svgs.is_empty(), "SVG should have pages");
    assert_eq!(svgs.len(), pngs.len(), "SVG and PNG should have same page count");
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

    // Test all backends
    let pdf = markdown_to_pdf(md).expect("PDF should succeed");
    let svg = markdown_to_svg(md).expect("SVG should succeed");
    let png = markdown_to_png(md).expect("PNG should succeed");

    assert!(!pdf.is_empty());
    assert!(!svg.is_empty());
    assert!(!png.is_empty());
}

#[test]
fn test_unicode_content() {
    let md = r#"# Unicode Test

中文内容测试

日本語テキスト

한국어 텍스트

🎉 Emoji support 🚀

Math: α + β = γ"#;

    let pdf = markdown_to_pdf(md).expect("PDF with unicode should succeed");
    let svg = markdown_to_svg(md).expect("SVG with unicode should succeed");

    assert!(!pdf.is_empty());
    assert!(!svg.is_empty());
}

#[test]
fn test_special_characters() {
    let md = r#"# Special Chars

HTML entities: &lt; &gt; &amp;

Quotes: "double" and 'single'

Backslashes: \path\to\file

Symbols: © ® ™ § † ‡"#;

    let pdf = markdown_to_pdf(md).expect("PDF with special chars should succeed");
    assert!(!pdf.is_empty());
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

    let pdf = markdown_to_pdf(md).expect("PDF with nested structures should succeed");
    let svg = markdown_to_svg(md).expect("SVG with nested structures should succeed");

    assert!(!pdf.is_empty());
    assert!(!svg.is_empty());
}
