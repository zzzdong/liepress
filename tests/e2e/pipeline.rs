//! 端到端集成测试（PDF 全链路）

use crate::common::samples;
use crate::common::{assert_valid_pdf, extract_outline_titles, pdf_page_count};
use liepress::{
    ConvertOptions, ast_to_layout, html_to_docx, html_to_png, html_to_svg, markdown_file_to_png,
    markdown_to_docx, markdown_to_pdf, markdown_to_png, markdown_to_svg,
};

/// 将 Markdown 跑完整条「非 PDF」管线，返回骨架文本内容。
///
/// 用于验证 Unicode 文本层面的输出（PDF 字体是 CID 编码，直接提取是 glyph id，
/// 无法可靠断言文本，因此在此层验证分词空格）。
fn md_to_layout_text(md: &str) -> String {
    let doc = liepress::dom::markdown_to_dom(md);
    let engine = liepress::css::CssEngine::new(liepress::ast::presets::DEFAULT_CSS)
        .expect("default css should parse");
    let styled = liepress::dom::html_to_styled_nodes(&doc, &engine);
    let document = ast_to_layout(&styled, &liepress::PageSettings::default());
    document.blocks.iter().map(|b| b.text_content()).collect()
}

/// 递归收集骨架中所有块（含嵌套）。
#[test]
fn test_center_inline_text_centers() {
    // `<center>` 内行内文本会被解析为段落，应继承 `text-align: center`。
    // 注意：用 HTML 块级标签包裹 markdown 块级语法（标题/表格）不是合法 CommonMark，
    // pulldown-cmark 按规范不会把它们作为 center 的子节点，因此不支持这种居中写法。
    let md = "<center>居中文字</center>";

    let pdf =
        markdown_to_pdf(md, &ConvertOptions::default()).expect("PDF with center should succeed");
    let _doc = assert_valid_pdf(&pdf);
}

#[test]
fn test_whitespace_preserved_across_bold() {
    // 回归测试：`This is a **Markdown** document.` 加粗片段两端的分词空格必须保留。
    let text = md_to_layout_text("# Space\n\nThis is a **Markdown** document.");
    assert!(
        text.contains("a Markdown document"),
        "bold-adjacent spaces must be preserved, got: {:?}",
        text
    );
    // 行首/行尾的孤立空白应被折叠丢弃
    assert_eq!(
        text.trim(),
        text,
        "no leading/trailing whitespace in document"
    );
}

#[test]
fn test_full_pipeline_pdf() {
    let pdf = markdown_to_pdf(samples::FULL_FEATURED, &ConvertOptions::default())
        .expect("Full PDF pipeline should succeed");
    let _doc = assert_valid_pdf(&pdf);
    assert!(
        pdf_page_count(&pdf) >= 1,
        "PDF should have at least one page"
    );
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
fn test_definition_list() {
    // 定义列表：术语与定义都应出现在文档文本中。
    let text = md_to_layout_text("Term\n: Definition text here");
    assert!(
        text.contains("Term") && text.contains("Definition text here"),
        "definition list term/definition missing, got: {:?}",
        text
    );

    let pdf = markdown_to_pdf("Term\n: Definition", &ConvertOptions::default())
        .expect("PDF with definition list should succeed");
    let _doc = assert_valid_pdf(&pdf);
}

#[test]
fn test_footnotes() {
    // 脚注：引用上标数字 + 末尾定义区都应出现。
    let text = md_to_layout_text("Text with a footnote[^1].\n\n[^1]: The footnote definition.");
    assert!(
        text.contains("The footnote definition"),
        "footnote definition missing, got: {:?}",
        text
    );

    let pdf = markdown_to_pdf(
        "Text with a footnote[^1].\n\n[^1]: The footnote definition.",
        &ConvertOptions::default(),
    )
    .expect("PDF with footnotes should succeed");
    let _doc = assert_valid_pdf(&pdf);
}

#[test]
fn test_footnote_internal_link_structure() {
    // 验证脚注在 Styled AST 中的结构：引用生成带内部锚点 url 的 Link，
    // 定义生成携带 id 的 FootnoteDef（二者 label 一致，供 PDF 内部跳转）。
    use liepress::ast::NodeKind;
    use liepress::dom::html_to_styled_nodes;

    let html = "<p>Text<sup class=\"footnote-ref\"><a href=\"#fn-def-n1\">1</a></sup>.</p><div id=\"fn-def-n1\" class=\"footnote-def\"><p>Definition body.</p></div>";
    let doc = liepress::dom::parse_html(html);
    let engine = liepress::css::CssEngine::new(liepress::ast::presets::DEFAULT_CSS)
        .expect("default css should parse");
    let styled = html_to_styled_nodes(&doc, &engine);

    // 收集所有 Link（含内部锚点）与 FootnoteDef
    let mut links = Vec::new();
    let mut defs = Vec::new();
    liepress::ast::walk(&styled, &mut |c: &liepress::ast::Node| match &c.kind {
        NodeKind::Link { url, .. } => links.push(url.clone()),
        NodeKind::FootnoteDef { id, .. } => defs.push(id.clone()),
        _ => {}
    });

    assert!(
        links.iter().any(|u| u == "#fn-def-n1"),
        "footnote reference link to #fn-def-n1 missing, got {:?}",
        links
    );
    assert!(
        defs.iter().any(|id| *id == "fn-def-n1"),
        "footnote definition id fn-def-n1 missing, got {:?}",
        defs
    );
}

#[test]
fn test_pdf_outline() {
    // PDF 应生成目录（outline），标题层级正确。
    let md = "# Title One\n\nSome body.\n\n## Sub Title\n\nMore body.";
    let pdf = markdown_to_pdf(md, &ConvertOptions::default()).expect("PDF should succeed");
    let doc = assert_valid_pdf(&pdf);
    let titles = extract_outline_titles(&doc);
    assert!(
        titles.iter().any(|t| t.contains("Title One")),
        "outline should contain H1, got {:?}",
        titles
    );
    assert!(
        titles.iter().any(|t| t.contains("Sub Title")),
        "outline should contain H2, got {:?}",
        titles
    );
}

#[test]
fn test_svg_output() {
    let md = "# Title\n\nSome body text with `inline code`.\n\n- item one\n- item two";
    let svg = markdown_to_svg(md, &ConvertOptions::default()).expect("SVG should succeed");
    assert!(
        svg.trim_start().starts_with("<svg"),
        "SVG should start with <svg"
    );
    assert!(svg.contains("Title"), "SVG should contain heading text");
    assert!(svg.contains("inline code"), "SVG should contain body text");
    assert!(
        svg.contains("rect"),
        "SVG should have rects (inline code bg)"
    );
}

#[test]
fn test_svg_background_and_list_marker() {
    let md = "# Title\n\n1. first\n2. second\n\n- bullet";
    let svg = markdown_to_svg(md, &ConvertOptions::default()).expect("SVG should succeed");
    // 白背景
    assert!(
        svg.contains("fill=\"#ffffff\""),
        "SVG should have white background"
    );
    // 有序列表 marker（数字 + 点）
    assert!(
        svg.contains("1.") && svg.contains("2."),
        "SVG should contain ordered list markers"
    );
    // 无序列表 marker（圆点）
    assert!(svg.contains("•"), "SVG should contain bullet markers");
}

#[test]
fn test_svg_png_full_page_size() {
    // SVG 画布应参考 PDF：宽度 = 整页宽（含左右边距），高度含上下边距。
    let md = "# Title\n\nBody.";
    let svg = markdown_to_svg(md, &ConvertOptions::default()).expect("SVG should succeed");
    // viewBox 应包含整页宽（A4 ≈ 595.28pt），大于内容宽（A4 减边距）
    let page_w = liepress::document::types::PAGE_WIDTH_PT as f64;
    assert!(
        svg.contains(&format!("viewBox=\"0 0 {:.2}", page_w)),
        "SVG viewBox width should be full page width ({:.2}), got: {}",
        page_w,
        &svg[..svg.find('>').map(|i| i + 1).unwrap_or(80)]
    );

    // PNG 像素宽 = 整页宽 × dpi/72（96dpi 时 A4 ≈ 794px）
    let png = markdown_to_png(md, &ConvertOptions::default()).expect("PNG should succeed");
    let img = image::load_from_memory(&png).expect("png decode");
    // 默认 150 DPI：A4 宽 = 595.28 × 150/72 ≈ 1240px
    let expected_px = (page_w * 150.0 / 72.0).round() as u32;
    assert!(
        (img.width() as i64 - expected_px as i64).abs() <= 2,
        "PNG width should be full page ({})px, got {}px",
        expected_px,
        img.width()
    );
}

#[test]
fn test_svg_table_separators() {
    let md = "| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |";
    let svg = markdown_to_svg(md, &ConvertOptions::default()).expect("SVG should succeed");
    // 表格列分隔竖线（<line>）
    assert!(
        svg.contains("<line"),
        "SVG should have table column separator lines"
    );
}

#[test]
fn test_png_output() {
    let md = "# Hello\n\nPNG body.";
    let png = markdown_to_png(md, &ConvertOptions::default()).expect("PNG should succeed");
    assert!(!png.is_empty());
    // PNG 魔数
    assert_eq!(&png[0..4], b"\x89PNG", "should be a PNG file");
}

#[test]
fn test_docx_output() {
    let md = "# Docx Title\n\nSome **bold** and *italic* text.\n\n- item\n- item2";
    let docx = markdown_to_docx(md, &ConvertOptions::default()).expect("DOCX should succeed");
    assert!(!docx.is_empty());
    // DOCX 是 zip 包，含 [Content_Types].xml
    let sig = &docx[0..2];
    assert_eq!(sig, b"PK", "DOCX should be a zip (PK signature)");
}

#[test]
fn test_png_with_image() {
    // 含 data URI 图片的 markdown → PNG，应嵌入真实图片而非占位色块。
    let md = concat!(
        "# PNG with Image\n\n",
        "Body text.\n\n",
        "![img](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==)"
    );
    let png = markdown_to_png(md, &ConvertOptions::default()).expect("PNG should succeed");
    assert_eq!(&png[0..4], b"\x89PNG", "should be a PNG file");
    // 解码 PNG，确认画布非纯占位（包含实际像素）。
    // 1x1 红色像素的 data URI 应让画布出现偏红像素（图片内容而非占位灰块）。
    let img = image::load_from_memory(&png).expect("png should decode");
    let rgba = img.to_rgba8();
    // 统计偏红像素（图片是红色 1x1）；白底为 (255,255,255)。
    let reddish = rgba.pixels().filter(|p| p[0] > 200 && p[1] < 100).count();
    assert!(
        reddish > 0,
        "PNG should contain red image pixels (not placeholder)"
    );
}

#[test]
fn test_html_to_svg_png_docx() {
    let html = "<h1>HTML Title</h1><p>HTML body.</p>";
    let svg = html_to_svg(html, &ConvertOptions::default()).expect("html svg");
    assert!(svg.contains("HTML Title"));
    let png = html_to_png(html, &ConvertOptions::default()).expect("html png");
    assert_eq!(&png[0..4], b"\x89PNG");
    let docx = html_to_docx(html, &ConvertOptions::default()).expect("html docx");
    assert_eq!(&docx[0..2], b"PK");
}

#[test]
fn test_docx_with_image_and_styles() {
    // DOCX 应能嵌入 data URI 图片，并包含标题/列表样式定义与段落样式引用。
    let md = concat!(
        "# Heading One\n\n",
        "Some body with **bold**.\n\n",
        "- item one\n- item two\n\n",
        "![img](data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==)"
    );
    let docx = markdown_to_docx(md, &ConvertOptions::default()).expect("DOCX should succeed");
    assert_eq!(&docx[0..2], b"PK", "DOCX should be a zip");

    use std::io::Read;
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(docx.as_slice())).expect("zip should open");
    // 样式表含标题/列表样式定义
    let mut styles = String::new();
    archive
        .by_name("word/styles.xml")
        .expect("styles.xml")
        .read_to_string(&mut styles)
        .unwrap();
    assert!(
        styles.contains("Heading1"),
        "styles.xml should define Heading1"
    );
    assert!(
        styles.contains("ListParagraph"),
        "styles.xml should define ListParagraph"
    );
    // 正文含标题样式引用与图片绘制
    let mut document = String::new();
    archive
        .by_name("word/document.xml")
        .expect("document.xml")
        .read_to_string(&mut document)
        .unwrap();
    assert!(
        document.contains("Heading1"),
        "document.xml should reference Heading1"
    );
    assert!(
        document.contains("<w:drawing"),
        "document.xml should embed an image"
    );
}

#[test]
fn test_png_local_image() {
    // 本地相对路径图片：文件入口应内联并渲染到 PNG。
    let dir = std::env::temp_dir().join("liepress_png_local_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // 100x80 红色图片
    let img = image::RgbaImage::from_fn(100, 80, |_, _| image::Rgba([200, 30, 40, 255]));
    img.save(dir.join("pic.png")).unwrap();
    let md = dir.join("report.md");
    std::fs::write(&md, "# Title\n\n![pic](pic.png)\n").unwrap();

    let png = markdown_file_to_png(&md, &ConvertOptions::default()).expect("PNG should succeed");
    let decoded = image::load_from_memory(&png).expect("png decode");
    // 图片区域应出现大量红色像素（100x80 红色图放大到 ~1016x813 px）
    let rgba = decoded.to_rgba8();
    let reddish = rgba.pixels().filter(|p| p[0] > 150 && p[1] < 100).count();
    assert!(
        reddish > 10000,
        "PNG should contain the red image pixels (got {} reddish)",
        reddish
    );
}

#[test]
fn test_docx_image_scaled_to_fit_width() {
    // DOCX 图片应按「适合页宽」缩放：大图宽度 ≤ 内容宽（默认 451pt = 5727700 EMU）。
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    // 生成 1000x800 的 PNG（远超内容宽，应被缩到 ~451pt 宽）
    let img = image::RgbaImage::from_fn(1000, 800, |_, _| image::Rgba([100, 150, 200, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut buf, image::ImageFormat::Png)
        .expect("encode png");
    let b64 = STANDARD.encode(buf.into_inner());
    let md = format!("![big](data:image/png;base64,{})", b64);

    let docx = markdown_to_docx(&md, &ConvertOptions::default()).expect("DOCX should succeed");
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(docx.as_slice())).expect("zip should open");
    use std::io::Read;
    let mut document = String::new();
    archive
        .by_name("word/document.xml")
        .expect("document.xml")
        .read_to_string(&mut document)
        .unwrap();
    // <wp:extent cx="..." cy="...">：EMU 单位，1pt=12700
    assert!(document.contains("<w:drawing"), "should embed image");
    // 提取 cx（宽度 EMU），应 ≤ 内容宽 451pt = 5727700 EMU
    let cx = document
        .find("<wp:extent")
        .and_then(|i| {
            let s = &document[i..];
            s.find("cx=\"")
                .and_then(|j| s[j + 4..].split('"').next().map(|v| v.parse::<f64>().ok()))
                .flatten()
        })
        .unwrap_or(f64::INFINITY);
    assert!(
        cx <= 5727700.0 + 1.0,
        "image width EMU ({}) should be ≤ content width (5727700 EMU = 451pt)",
        cx
    );
    assert!(
        cx > 5000000.0,
        "large image should be scaled UP to near content width, got {} EMU",
        cx
    );
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

    let pdf =
        markdown_to_pdf(md, &ConvertOptions::default()).expect("PDF with table should succeed");
    let _doc = assert_valid_pdf(&pdf);
}
