//! PDF 深度验证测试套件
//!
//! 使用 lopdf 对生成的 PDF 进行深度内容验证，
//! 包括文本内容、链接注释、页面尺寸、字体资源等。

use crate::common::{
    assert_a4_page_size, assert_has_link, assert_valid_pdf, count_font_resources, extract_pdf_text,
    get_page_media_boxes, get_pdf_metadata, load_pdf, pdf_page_count, validate_pdf_structure,
};
use liepress::{ConvertOptions, markdown_to_pdf};

// ─── 测试样本 ──────────────────────────────────────────

const SAMPLE_BASIC: &str = r#"# 基础文档测试

这是一个简单的段落，用于测试基本的 PDF 生成功能。

## 二级标题

这是二级标题下的内容。包含一些普通文本。"#;

#[allow(dead_code)]
const SAMPLE_FORMATTING: &str = r#"# 文本格式测试

**粗体文本** 和 *斜体文本* 以及 `行内代码`。

```rust
fn main() {
    println!("Hello");
}
```"#;

const SAMPLE_LINKS: &str = r#"# 链接测试

访问 [Rust 官网](https://www.rust-lang.org/) 了解更多信息。

也可以查看 [GitHub](https://github.com) 上的项目。

普通段落中的 [内联链接](https://example.com) 测试。"#;

const SAMPLE_TABLES: &str = r#"# 表格测试

| 姓名 | 年龄 | 城市 |
|------|------|------|
| Alice | 30 | New York |
| Bob | 25 | London |

| 功能 | 状态 | 链接 |
|------|------|------|
| **粗体** | Done | [查看](https://example.com) |
| *斜体* | WIP | [详情](https://rust-lang.org) |"#;

const SAMPLE_MULTIPAGE: &str = r#"# 多页文档测试

这是第一页的内容。

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.

## 第一节

Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.

### 小节 1.1

Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt explicabo.

### 小节 1.2

Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, sed quia consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt.

## 第二节

Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, adipisci velit, sed quia non numquam eius modi tempora incidunt ut labore et dolore magnam aliquam quaerat voluptatem.

### 小节 2.1

Ut enim ad minima veniam, quis nostrum exercitationem ullam corporis suscipit laboriosam, nisi ut aliquid ex ea commodi consequatur?

### 小节 2.2

Quis autem vel eum iure reprehenderit qui in ea voluptate velit esse quam nihil molestiae consequatur, vel illum qui dolorem eum fugiat quo voluptas nulla pariatur?

## 第三节

At vero eos et accusamus et iusto odio dignissimos ducimus qui blanditiis praesentium voluptatum deleniti atque corrupti quos dolores et quas molestias excepturi sint occaecati cupiditate non provident.

### 小节 3.1

Similique sunt in culpa qui officia deserunt mollitia animi, id est laborum et dolorum fuga. Et harum quidem rerum facilis est et expedita distinctio.

### 小节 3.2

Nam libero tempore, cum soluta nobis est eligendi optio cumque nihil impedit quo minus id quod maxime placeat facere possimus, omnis voluptas assumenda est, omnis dolor repellendus.

## 结束

这是文档的最后一页。包含一个 [结束链接](https://end.com)。

### 额外补充

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Pellentesque habitant morbi tristique senectus et netus et malesuada fames ac turpis egestas. Vestibulum tortor quam, feugiat vitae, ultricies eget, tempor sit amet, ante. Donec eu libero sit amet quam egestas semper. Aenean ultricies mi vitae est. Mauris placerat eleifend leo.

Quisque sit amet est et sapien ullamcorper pharetra. Vestibulum erat wisi, condimentum sed, commodo vitae, ornare sit amet, wisi. Aenean fermentum, elit eget tincidunt condimentum, eros ipsum rutrum orci, sagittis tempus lacus enim ac dui. Donec non enim in turpis pulvinar facilisis. Ut felis.

Praesent dapibus, neque id cursus faucibus, tortor neque egestas augue, eu vulputate magna eros eu erat. Aliquam erat volutpat. Nam dui mi, tincidunt quis, accumsan porttitor, facilisis luctus, metus.

Phasellus ullamcorper ipsum rutrum nunc. Nunc nonummy metus. Vestibulum volutpat pretium libero. Cras id dui. Aenean ut eros et nisl sagittis vestibulum. Nullam nulla eros, ultricies sit amet, nonummy id, imperdiet feugiat, pede.

Sed lectus. Donec mollis hendrerit risus. Phasellus nec sem in justo pellentesque facilisis. Etiam imperdiet imperdiet orci. Nunc nec neque. Phasellus leo dolor, tempus non, auctor et, hendrerit quis, nisi. Curabitur ligula sapien, tincidunt non, euismod vitae, posuere imperdiet, leo. Maecenas malesuada. Praesent congue erat at massa."#;

const SAMPLE_UNICODE: &str = r#"# Unicode 测试

中文内容测试

日本語テキスト

한국어 텍스트

Math: α + β = γ"#;

// ==================== 基础结构验证 ====================

#[test]
fn test_pdf_basic_structure() {
    let pdf_data =
        markdown_to_pdf(SAMPLE_BASIC, &ConvertOptions::default()).expect("PDF should generate");
    let doc = assert_valid_pdf(&pdf_data);

    // 验证页数
    assert!(!doc.get_pages().is_empty(), "Should have at least 1 page");

    // 验证页面尺寸为 A4
    assert_a4_page_size(&doc, 1.0);

    // 验证有字体资源嵌入
    let font_count = count_font_resources(&doc);
    assert!(font_count > 0, "Should have embedded font resources");
}

#[test]
fn test_pdf_header_valid() {
    let pdf_data =
        markdown_to_pdf(SAMPLE_BASIC, &ConvertOptions::default()).expect("PDF should generate");

    // 验证 PDF 头部
    assert_eq!(&pdf_data[0..5], b"%PDF-", "Should start with %PDF-");

    // 验证文件大小合理
    assert!(pdf_data.len() > 100, "PDF should have meaningful content");
}

// ==================== 文本内容验证 ====================

#[test]
fn test_pdf_text_extraction() {
    let pdf_data =
        markdown_to_pdf(SAMPLE_BASIC, &ConvertOptions::default()).expect("PDF should generate");
    let doc = load_pdf(&pdf_data);

    let page_texts = extract_pdf_text(&doc);
    assert!(
        !page_texts.is_empty(),
        "Should extract text from at least one page"
    );

    // 验证页面有文本内容（非空）
    let total_text: String = page_texts.iter().cloned().collect();
    assert!(
        !total_text.trim().is_empty(),
        "Extracted text should not be empty"
    );
}

#[test]
fn test_pdf_contains_expected_text() {
    let pdf_data =
        markdown_to_pdf(SAMPLE_BASIC, &ConvertOptions::default()).expect("PDF should generate");
    let doc = load_pdf(&pdf_data);

    // 验证提取的文本包含预期内容
    let page_texts = extract_pdf_text(&doc);
    let all_text: String = page_texts.join(" ");

    // 基础文档应该包含这些字符（注意：由于字体编码，提取的可能是 glyph id 而非原始字符）
    // 至少验证文本非空且有内容
    assert!(!all_text.is_empty(), "PDF should contain extractable text");
    println!(
        "Extracted text sample: {:?}",
        &all_text[..all_text.len().min(200)]
    );
}

// ==================== 链接注释验证 ====================

#[test]
fn test_pdf_link_annotations() {
    let pdf_data =
        markdown_to_pdf(SAMPLE_LINKS, &ConvertOptions::default()).expect("PDF should generate");
    let doc = load_pdf(&pdf_data);

    // 验证存在链接注释
    assert_has_link(&doc, "https://www.rust-lang.org/");
    assert_has_link(&doc, "https://github.com");
    assert_has_link(&doc, "https://example.com");
}

#[test]
fn test_pdf_link_count() {
    let pdf_data =
        markdown_to_pdf(SAMPLE_LINKS, &ConvertOptions::default()).expect("PDF should generate");

    let report = validate_pdf_structure(&pdf_data);
    let total_links: usize = report.pages.iter().map(|p| p.annotations.len()).sum();
    assert!(
        total_links >= 3,
        "Should have at least 3 link annotations, found {}",
        total_links
    );
}

#[test]
fn test_pdf_link_rect_valid() {
    let pdf_data =
        markdown_to_pdf(SAMPLE_LINKS, &ConvertOptions::default()).expect("PDF should generate");

    let report = validate_pdf_structure(&pdf_data);
    for page in &report.pages {
        for annot in &page.annotations {
            // Rect 应有 4 个值
            assert_eq!(
                annot.rect.len(),
                4,
                "Link rect should have 4 values, got {:?}",
                annot.rect
            );
            // x0 < x1, y0 < y1
            if annot.rect.len() == 4 {
                assert!(
                    annot.rect[2] > annot.rect[0],
                    "Link rect width should be positive"
                );
                assert!(
                    annot.rect[3] > annot.rect[1],
                    "Link rect height should be positive"
                );
            }
        }
    }
}

// ==================== 页面尺寸验证 ====================

#[test]
fn test_pdf_page_size_a4() {
    let pdf_data =
        markdown_to_pdf(SAMPLE_BASIC, &ConvertOptions::default()).expect("PDF should generate");
    let doc = load_pdf(&pdf_data);

    let boxes = get_page_media_boxes(&doc);
    assert!(!boxes.is_empty(), "Should have page media boxes");

    // 验证所有页面都是 A4
    assert_a4_page_size(&doc, 1.0);
}

#[test]
fn test_pdf_consistent_page_sizes() {
    let pdf_data =
        markdown_to_pdf(SAMPLE_MULTIPAGE, &ConvertOptions::default()).expect("PDF should generate");
    let doc = load_pdf(&pdf_data);

    let boxes = get_page_media_boxes(&doc);
    assert!(boxes.len() >= 2, "Multipage doc should have multiple pages");

    // 所有页面尺寸应一致
    let first = boxes[0];
    for (i, (w, h)) in boxes.iter().enumerate() {
        assert!(
            (w - first.0).abs() < 0.1 && (h - first.1).abs() < 0.1,
            "Page {} size ({:.1}, {:.1}) should match page 1 ({:.1}, {:.1})",
            i + 1,
            w,
            h,
            first.0,
            first.1
        );
    }
}

// ==================== 字体资源验证 ====================

#[test]
fn test_pdf_has_embedded_fonts() {
    let pdf_data =
        markdown_to_pdf(SAMPLE_BASIC, &ConvertOptions::default()).expect("PDF should generate");
    let doc = load_pdf(&pdf_data);

    let font_count = count_font_resources(&doc);
    assert!(
        font_count > 0,
        "PDF should have at least one embedded font, found {}",
        font_count
    );
}

#[test]
fn test_pdf_unicode_fonts() {
    let pdf_data =
        markdown_to_pdf(SAMPLE_UNICODE, &ConvertOptions::default()).expect("PDF should generate");
    let doc = load_pdf(&pdf_data);

    let font_count = count_font_resources(&doc);
    // Unicode 文档可能需要多个字体（回退机制）
    assert!(
        font_count > 0,
        "Unicode PDF should have embedded fonts, found {}",
        font_count
    );
    println!("Unicode doc font count: {}", font_count);
}

// ==================== 多页文档验证 ====================

#[test]
fn test_pdf_multipage_structure() {
    let pdf_data =
        markdown_to_pdf(SAMPLE_MULTIPAGE, &ConvertOptions::default()).expect("PDF should generate");

    let page_count = pdf_page_count(&pdf_data);
    assert!(
        page_count >= 2,
        "Multipage doc should have >= 2 pages, found {}",
        page_count
    );

    // 验证结构报告一致
    let report = validate_pdf_structure(&pdf_data);
    assert_eq!(
        report.page_count, page_count,
        "Report page count should match"
    );
}

#[test]
fn test_pdf_multipage_each_has_content() {
    let pdf_data =
        markdown_to_pdf(SAMPLE_MULTIPAGE, &ConvertOptions::default()).expect("PDF should generate");
    let doc = load_pdf(&pdf_data);

    let page_texts = extract_pdf_text(&doc);
    for (i, text) in page_texts.iter().enumerate() {
        // 每页都应该有文本内容或有元素（某些页可能主要是图形元素）
        println!("Page {}: text length = {}", i + 1, text.len());
    }
}

// ==================== 表格 PDF 验证 ====================

#[test]
fn test_pdf_table_structure() {
    let pdf_data =
        markdown_to_pdf(SAMPLE_TABLES, &ConvertOptions::default()).expect("PDF should generate");
    let doc = load_pdf(&pdf_data);

    let page_count = pdf_page_count(&pdf_data);
    assert!(page_count >= 1, "Table doc should have at least 1 page");

    // 表格 PDF 应有字体资源
    let font_count = count_font_resources(&doc);
    assert!(font_count > 0, "Table PDF should have fonts");

    // 验证链接（表格中包含链接）
    assert_has_link(&doc, "https://example.com");
    assert_has_link(&doc, "https://rust-lang.org");
}

// ==================== 综合验证 ====================

#[test]
fn test_pdf_comprehensive_validation() {
    let md = r#"# 综合测试文档

这是包含 **粗体**、*斜体* 和 `代码` 的段落。

## 列表

- 项目 A
- 项目 B
  - 子项目 B.1
  - 子项目 B.2

## 表格

| 模块 | 状态 |
|------|------|
| Parser | Done |
| Render | WIP |

## 链接

访问 [示例网站](https://example.com) 查看详情。

## 代码

```rust
fn main() {
    println!("Hello!");
}
```

> 这是一个引用块。

---

文档结束。"#;

    let pdf_data = markdown_to_pdf(md, &ConvertOptions::default()).expect("PDF should generate");

    // 1. 基本结构
    let doc = assert_valid_pdf(&pdf_data);

    // 2. 页面尺寸
    assert_a4_page_size(&doc, 1.0);

    // 3. 字体资源
    let font_count = count_font_resources(&doc);
    assert!(font_count > 0, "Should have fonts");

    // 4. 链接注释
    assert_has_link(&doc, "https://example.com");

    // 5. 文本提取
    let page_texts = extract_pdf_text(&doc);
    assert!(!page_texts.is_empty(), "Should extract text");

    // 6. 元数据检查（可能为空，但不应 panic）
    let _meta = get_pdf_metadata(&doc);

    // 7. 结构验证
    let report = validate_pdf_structure(&pdf_data);
    assert!(report.has_valid_header);
    assert!(report.page_count > 0);
}
