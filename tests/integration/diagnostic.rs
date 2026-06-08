//! 诊断测试
//!
//! 生成输出文件用于人工检查

use crate::common::{diag_output_dir, ensure_test_image};
use liepress::{markdown_to_pdf, markdown_to_svg};
use std::fs;
use std::path::PathBuf;

fn output_dir_markdown_to_svg(md: &str, output_dir: &std::path::PathBuf, name: &str) {
    use liepress::PageRenderer;
    use liepress::generator::markdown_to_document;
    use liepress::render::SvgRenderer;

    let doc = markdown_to_document(md);
    for (i, page) in doc.pages.iter().enumerate() {
        let mut renderer = SvgRenderer::new(page.width, page.height);
        renderer.render_elements(&page.elements);
        let svg = renderer.finalize();
        let path = output_dir.join(format!("{}_{}.svg", name, i));
        fs::write(&path, &svg).expect("Should write SVG file");
        println!("SVG saved to: {}", path.display());
    }
}

fn output_dir_markdown_to_pdf(md: &str, output_dir: &std::path::PathBuf, name: &str) {
    let pdf_data = markdown_to_pdf(md).expect("PDF generation should succeed");
    let path = output_dir.join(format!("{}.pdf", name));
    fs::write(&path, &pdf_data).expect("Should write PDF file");
    println!("PDF saved to: {}", path.display());
}

#[test]
fn test_code_block_diagnostic() {
    let output_dir = diag_output_dir("code_block_diag");

    let md = r#"# Test

Some text before.

```
fn main() {
    println!("hello");
}
```

Some text after."#;

    output_dir_markdown_to_svg(md, &output_dir, "code_block");
    output_dir_markdown_to_pdf(md, &output_dir, "code_block");
}

#[test]
fn test_list_diagnostic() {
    let output_dir = diag_output_dir("list_diag");

    let md = r#"# List Test

Unordered list:
- Item 1
- Item 2
  - Sub 2.1
  - Sub 2.2
- Item 3

Ordered list:
1. First
2. Second
3. Third"#;

    output_dir_markdown_to_svg(md, &output_dir, "list");
    output_dir_markdown_to_pdf(md, &output_dir, "list");
}

#[test]
fn test_pagination_diagnostic() {
    let output_dir = diag_output_dir("pagination_diag");

    let mut md = String::from("# Pagination Test\n\n");
    for i in 0..50 {
        md.push_str(&format!(
            "Paragraph {} with enough text to test pagination across multiple pages. ",
            i
        ));
        md.push_str("Lorem ipsum dolor sit amet, consectetur adipiscing elit. ");
        md.push_str("Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.\n\n");
    }

    output_dir_markdown_to_svg(&md, &output_dir, "pagination");
    output_dir_markdown_to_pdf(&md, &output_dir, "pagination");
}

#[test]
fn test_image_diagnostic() {
    let output_dir = diag_output_dir("image_diag");

    let md = r#"# Image Test

Text before image.

![Test Image](tests/fixtures/test_image.png)

Text after image."#;

    output_dir_markdown_to_svg(md, &output_dir, "image");
    output_dir_markdown_to_pdf(md, &output_dir, "image");
}

#[test]
fn test_full_featured_diagnostic() {
    let output_dir = diag_output_dir("full_featured");

    let md = r#"# Complete Document

## Typography

Regular, **bold**, *italic*, and `code` text.

## Lists

### Unordered
- Item A
- Item B
- Item C

### Ordered
1. First
2. Second
3. Third

## Code Block

```rust
fn main() {
    println!("Hello, world!");
}
```

## Blockquote

> A famous quote goes here.
> It can span multiple lines.

## Thematic Break

---

## Link

Visit [Rust](https://www.rust-lang.org/) for more information."#;

    output_dir_markdown_to_svg(md, &output_dir, "full_featured");
    output_dir_markdown_to_pdf(md, &output_dir, "full_featured");
}

#[test]
fn test_image_example_diagnostic() {
    let fixtures_path = PathBuf::from("tests/fixtures/test_image.png");
    ensure_test_image(&fixtures_path);

    let output_dir = diag_output_dir("image_example_diag");

    let md = crate::common::samples::IMAGE_EXAMPLE;
    output_dir_markdown_to_svg(md, &output_dir, "image_example");
    output_dir_markdown_to_pdf(md, &output_dir, "image_example");
}
