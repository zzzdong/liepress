//! 诊断测试
//!
//! 生成输出文件用于人工检查，包含调试性质的视觉元素分析和字体 glyph 测试

use crate::common::{diag_output_dir, ensure_test_image};
use liepress::generator::markdown_to_document;
use liepress::visual::VisualElement;
use liepress::{ConvertOptions, markdown_to_pdf};
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
    let pdf_data =
        markdown_to_pdf(md, &ConvertOptions::default()).expect("PDF generation should succeed");
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

#[test]
fn test_center_tag_diagnostic() {
    let output_dir = diag_output_dir("center_tag_diag");

    // 测试块级 center 标签（正确格式）
    let md_block = r#"<center>

# LiePress

</center>

# 安装
test"#;

    output_dir_markdown_to_svg(md_block, &output_dir, "center_tag_block");
    output_dir_markdown_to_pdf(md_block, &output_dir, "center_tag_block");

    // 测试行内 center 标签（无效格式，应被忽略）
    let md_inline = r#"<center>#LiePress</center>

# 安装
test"#;

    output_dir_markdown_to_svg(md_inline, &output_dir, "center_tag_inline");
    output_dir_markdown_to_pdf(md_inline, &output_dir, "center_tag_inline");
}

// ─── 来自 layout.rs 的调试测试 ───────────────────────────

/// 调试测试：打印任务列表的视觉元素详情
#[test]
fn test_tasklist_debug_elements() {
    let md = "- [ ] 未勾选\n- [x] 已勾选\n\n- Regular bullet item";
    let doc = markdown_to_document(md);

    eprintln!("=== Task List Debug ===");
    for (pi, page) in doc.pages.iter().enumerate() {
        eprintln!("Page {}: {} elements", pi, page.elements.len());
        for (ei, elem) in page.elements.iter().enumerate() {
            match elem {
                VisualElement::TextLine {
                    runs,
                    bounds,
                    line_height,
                } => {
                    let text_info: Vec<String> = runs
                        .iter()
                        .map(|r| {
                            let glyph_info: Vec<String> =
                                r.glyphs.iter().map(|g| format!("id={}", g.id)).collect();
                            format!(
                                "text={:?} gl={} adv={:.1} glyphs=[{}]",
                                r.text,
                                r.glyphs.len(),
                                r.advance,
                                glyph_info.join(",")
                            )
                        })
                        .collect();
                    eprintln!(
                        "  [{}] TextLine b=({:.0},{:.0})-({:.0},{:.0}) lh={} {}",
                        ei,
                        bounds.x0,
                        bounds.y0,
                        bounds.x1,
                        bounds.y1,
                        line_height,
                        text_info.join(" | ")
                    );
                }
                _ => eprintln!("  [{}] {:?}", ei, elem),
            }
        }
    }
    assert!(doc.pages[0].elements.len() >= 2);
}

/// 测试不同标记字符在 serif 字体中的 glyph id
#[test]
fn test_marker_char_glyphs() {
    let test_chars = [
        ("☐", "U+2610 BALLOT BOX"),
        ("☑", "U+2611 BALLOT BOX WITH CHECK"),
        ("☒", "U+2612 BALLOT BOX WITH X"),
        ("□", "U+25A1 WHITE SQUARE"),
        ("■", "U+25A0 BLACK SQUARE"),
        ("○", "U+25CB WHITE CIRCLE"),
        ("●", "U+25CF BLACK CIRCLE"),
        ("•", "U+2022 BULLET"),
        ("[ ]", "ASCII [ ]"),
        ("[x]", "ASCII [x]"),
        ("✓", "U+2713 CHECK MARK"),
        ("✔", "U+2714 HEAVY CHECK MARK"),
        ("✗", "U+2717 BALLOT X"),
        ("✘", "U+2718 HEAVY BALLOT X"),
        ("⬜", "U+2B1C WHITE LARGE SQUARE"),
        ("⬛", "U+2B1B BLACK LARGE SQUARE"),
        ("✅", "U+2705 WHITE HEAVY CHECK MARK"),
        ("❌", "U+274C CROSS MARK"),
        ("❎", "U+274E NEGATIVE SQUARED CROSS MARK"),
        ("🗹", "U+1F5F9 BALLOT BOX WITH BOLD CHECK"),
        ("🗸", "U+1F5F8 LIGHT CHECK MARK"),
    ];

    let marker_style = liepress::ast::list_marker_style();
    let text_style = liepress::ast::computed_style_to_text_style(&marker_style);

    eprintln!("=== Marker Character Glyph Test ===");
    for (ch, desc) in &test_chars {
        let layout = liepress::text::create_text_layout(ch, &text_style, None);
        for line in &layout.lines {
            for run in &line.runs {
                let ids: Vec<u32> = run.glyphs.iter().map(|g| g.id).collect();
                let adv = run.advance;
                eprintln!(
                    "{} ({:>8}): glyph_ids=[{}], advance={:.1}",
                    desc,
                    ch,
                    ids.iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>()
                        .join(","),
                    adv
                );
            }
        }
    }
}

// ─── 来自 diagnostic_test.rs 的链接诊断测试 ──────────────────────

#[test]
fn diag_trace_url() {
    use liepress::generator::markdown_to_document;
    use liepress::visual::VisualElement;

    // 简单链接
    let md1 = r#"[Example](http://example.com)"#;
    let doc1 = markdown_to_document(md1);
    println!("=== Simple link ===");
    for page in &doc1.pages {
        for elem in &page.elements {
            if let VisualElement::TextLine { runs, bounds, .. } = elem {
                println!("  TextLine bounds={:?}", bounds);
                for (ri, run) in runs.iter().enumerate() {
                    println!(
                        "    Run[{}]: text={:?}, url={:?}, baseline_x={}, advance={}, font_size={}",
                        ri, run.text, run.url, run.baseline_x, run.advance, run.font_size
                    );
                }
            }
        }
    }

    // 段落中的链接
    let md2 = r#"This is a paragraph with a [link](http://example.com) in the middle."#;
    let doc2 = markdown_to_document(md2);
    println!("=== Link in paragraph ===");
    for page in &doc2.pages {
        for elem in &page.elements {
            if let VisualElement::TextLine { runs, bounds, .. } = elem {
                println!("  TextLine bounds={:?}", bounds);
                for (ri, run) in runs.iter().enumerate() {
                    println!(
                        "    Run[{}]: text={:?}, url={:?}, baseline_x={}, advance={}, font_size={}",
                        ri, run.text, run.url, run.baseline_x, run.advance, run.font_size
                    );
                }
            }
        }
    }
}
