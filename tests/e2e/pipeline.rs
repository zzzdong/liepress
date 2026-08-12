//! 端到端集成测试（PDF 全链路）

use crate::common::samples;
use crate::common::{assert_valid_pdf, pdf_page_count};
use liepress::{ConvertOptions, ast_to_skeleton, markdown_to_pdf};

/// 将 Markdown 跑完整条「非 PDF」管线，返回骨架文本内容。
///
/// 用于验证 Unicode 文本层面的输出（PDF 字体是 CID 编码，直接提取是 glyph id，
/// 无法可靠断言文本，因此在此层验证分词空格）。
fn md_to_skeleton_text(md: &str) -> String {
    let html = liepress::markdown_to_html(md);
    let doc = liepress::html::parse_html(&html);
    let engine = liepress::css::CssEngine::new(liepress::ast::presets::DEFAULT_CSS)
        .expect("default css should parse");
    let styled = liepress::html::html_to_styled_nodes(&doc, &engine);
    let skeleton = ast_to_skeleton(&styled, &liepress::PageSettings::default());
    skeleton.blocks.iter().map(|b| b.text_content()).collect()
}

#[test]
fn test_whitespace_preserved_across_bold() {
    // 回归测试：`This is a **Markdown** document.` 加粗片段两端的分词空格必须保留。
    let text = md_to_skeleton_text("# Space\n\nThis is a **Markdown** document.");
    assert!(
        text.contains("a Markdown document"),
        "bold-adjacent spaces must be preserved, got: {:?}",
        text
    );
    // 行首/行尾的孤立空白应被折叠丢弃
    assert_eq!(text.trim(), text, "no leading/trailing whitespace in skeleton");
}

#[test]
fn test_full_pipeline_pdf() {
    let pdf = markdown_to_pdf(samples::FULL_FEATURED, &ConvertOptions::default())
        .expect("Full PDF pipeline should succeed");
    let _doc = assert_valid_pdf(&pdf);
    assert!(pdf_page_count(&pdf) >= 1, "PDF should have at least one page");
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
    let _doc = assert_valid_pdf(&pdf);
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
    let _doc = assert_valid_pdf(&pdf);
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
    let _doc = assert_valid_pdf(&pdf);
}

#[test]
fn test_table() {
    let md = r#"# Table Test

| Name | Age | City |
|------|-----|------|
| Alice | 30 | Beijing |
| Bob | 25 | Shanghai |
| Carol | 35 | Shenzhen |

A paragraph after the table."#;

    let pdf = markdown_to_pdf(md, &ConvertOptions::default())
        .expect("PDF with table should succeed");
    let _doc = assert_valid_pdf(&pdf);
}
