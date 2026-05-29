//! PDF 验证测试套件
//!
//! 包含多个示例 Markdown 文档，生成 PDF 后使用 lopdf 验证结构，
//! 同时输出到文件供人工查看。

use liepress::markdown_to_pdf;
use std::fs;
use std::path::PathBuf;
use crate::common::{extract_links, assert_has_link, assert_link_count, group_links_by_url, PdfReport};

/// 获取测试输出目录
fn test_output_dir() -> PathBuf {
    let path = PathBuf::from("target/test_output/pdf_validation");
    fs::create_dir_all(&path).expect("Should create output directory");
    path
}

/// 保存 PDF 文件并返回路径
fn save_pdf(name: &str, data: &[u8]) -> PathBuf {
    let path = test_output_dir().join(format!("{}.pdf", name));
    fs::write(&path, data).expect("Should write PDF file");
    println!("📄 PDF saved to: {:?}", path);
    path
}

/// 使用 lopdf 验证 PDF 基本结构
fn validate_pdf_structure(data: &[u8]) -> Result<PdfReport, String> {
    Ok(crate::common::validate_pdf_structure(data))
}

// ==================== 测试用例 ====================

/// 示例 1: 基础文档 - 标题和段落
const SAMPLE_BASIC: &str = r#"# 基础文档测试

这是一个简单的段落，用于测试基本的 PDF 生成功能。

## 二级标题

这是二级标题下的内容。包含一些普通文本。

### 三级标题

更多内容在这里。"#;

/// 示例 2: 文本格式 - 粗体、斜体、代码
const SAMPLE_FORMATTING: &str = r#"# 文本格式测试

**粗体文本** 和 *斜体文本* 以及 `行内代码`。

**粗体和*斜体*混合**

```rust
fn main() {
    println!("代码块测试");
}
```

普通文本 **粗体** *斜体* `代码` 普通文本。"#;

/// 示例 3: 超链接 - 单链接、多链接、链接与文本混合
const SAMPLE_LINKS: &str = r#"# 超链接测试

这是 [单个链接](https://example.com) 的测试。

这是 [第一个链接](https://first.com) 和 [第二个链接](https://second.com) 的测试。

访问 [Rust 官网](https://www.rust-lang.org/) 学习更多关于 Rust 的知识。

在段落中间 [链接A](http://link-a.com) 有一些文本 [链接B](http://link-b.com) 更多文本。"#;

/// 示例 4: 列表 - 无序列表、有序列表、嵌套列表
const SAMPLE_LISTS: &str = r#"# 列表测试

## 无序列表
- 第一项
- 第二项
- 第三项

## 有序列表
1. 第一步
2. 第二步
3. 第三步

## 嵌套列表
- 父项 1
  - 子项 1.1
  - 子项 1.2
- 父项 2
  - 子项 2.1
    - 孙项 2.1.1
    - 孙项 2.1.2
- 父项 3"#;

/// 示例 5: 表格 - 简单表格、对齐、复杂内容
const SAMPLE_TABLES: &str = r#"# 表格测试

## 简单表格
| 姓名 | 年龄 | 城市 |
|------|------|------|
| Alice | 30 | New York |
| Bob | 25 | London |

## 对齐表格
| 左对齐 | 居中 | 右对齐 |
|:-------|:------:|------:|
| Left | Center | Right |
| L2 | C2 | R2 |

## 带格式的表格
| 功能 | 状态 | 链接 |
|------|------|------|
| **粗体** | ✅ | [查看](https://example.com) |
| *斜体* | ⏳ | [详情](https://rust-lang.org) |"#;

/// 示例 6: 引用和分隔线
const SAMPLE_QUOTES: &str = r#"# 引用和分隔线测试

> 这是一个引用块。
> 包含多行内容。

普通段落。

---

> 嵌套引用
> > 第二层引用
> > 更多内容
> 回到第一层

普通段落。

---

> 引用中的 **粗体** 和 *斜体* 以及 [链接](https://example.com)。"#;

/// 示例 7: 复杂文档 - 综合所有元素
const SAMPLE_COMPLEX: &str = r#"# 复杂文档测试

这是引言段落，包含 **粗体**、*斜体* 和 `代码`。

## 功能列表

- 核心功能
  - 功能 A：支持 [链接跳转](https://example.com)
  - 功能 B：支持 **格式化**
- 高级功能
  1. 有序列表项
  2. 另一个列表项

## 数据表格

| 模块 | 状态 | 文档 |
|------|------|------|
| Parser | ✅ 完成 | [API](https://docs.rs/parser) |
| Renderer | ⏳ 进行中 | [指南](https://guide.rs) |
| Generator | ✅ 完成 | [教程](https://tutorial.rs) |

## 代码示例

```rust
fn main() {
    println!("Hello, World!");
}
```

## 注意事项

> **注意**：这是一个重要的提示块。
> 包含 [参考链接](https://reference.com) 供查阅。

---

文档结束。"#;

/// 示例 8: 多页文档 - 长文档测试分页
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

这是文档的最后一页。包含一个 [结束链接](https://end.com)。"#;

/// 示例 9: 多行超链接 - 链接文本被断行
const SAMPLE_WRAPPED_LINKS: &str = r#"# 长链接断行测试

这个段落包含一个 [非常长的超链接文本，确保它会在页面中换行到第二行甚至第三行](https://super-long-link.example.com/very/deep/path) 来测试断行超链接。

另一个包含 [短链接](https://short.example) 和 [非常非常非常非常非常非常非常非常非常长的超链接文本用于测试换行](https://very-long.example.com) 在同一个段落中。

## 标题中的长链接

[这是一个非常长的标题级别超链接，也应该支持断行处理](https://title-link.example.com)。

## 列表中的长链接

- 列表项包含 [一个非常长的超链接文本用于测试列表中换行行为是否正确](https://list-link.example.com)
- 另一个列表项和 [短链接](https://short2.example) 混合"#;

// ==================== 测试函数 ====================

#[test]
fn test_pdf_basic_document() {
    let pdf_data = markdown_to_pdf(SAMPLE_BASIC).expect("PDF should generate");
    let path = save_pdf("01_basic", &pdf_data);
    
    let report = validate_pdf_structure(&pdf_data).expect("Should validate PDF");
    
    assert!(report.has_valid_header, "PDF should have valid header");
    assert!(report.page_count > 0, "PDF should have at least one page");
    
    println!("✅ Basic document: {} page(s)", report.page_count);
    println!("   Output: {:?}", path);
}

// ==================== 超链接专项测试 ====================

#[test]
fn test_link_simple() {
    let md = r#"[Example](http://example.com)"#;
    let pdf_data = markdown_to_pdf(md).expect("PDF should generate");
    let doc = crate::common::assert_valid_pdf(&pdf_data);
    assert_has_link(&doc, "http://example.com");
}

#[test]
fn test_link_multiple() {
    let md = r#"Link one: [Example](http://example.com) and link two: [Rust](https://rust-lang.org)."#;
    let pdf_data = markdown_to_pdf(md).expect("PDF should generate");
    let doc = crate::common::assert_valid_pdf(&pdf_data);

    let links = assert_link_count(&doc, 2);
    let urls: Vec<String> = links.into_iter().map(|(u, _)| u).collect();
    assert!(
        urls.iter().any(|u| u == "http://example.com"),
        "Should contain example.com, found: {:?}",
        urls
    );
    assert!(
        urls.iter().any(|u| u == "https://rust-lang.org"),
        "Should contain rust-lang.org, found: {:?}",
        urls
    );
}

#[test]
fn test_link_in_paragraph() {
    let md = r#"This is a paragraph with a [link](http://example.com) in the middle."#;
    let pdf_data = markdown_to_pdf(md).expect("PDF should generate");
    let doc = crate::common::assert_valid_pdf(&pdf_data);
    assert_has_link(&doc, "http://example.com");
}

#[test]
fn test_link_rect_valid() {
    let md = r#"[Click Here](http://example.com)"#;
    let pdf_data = markdown_to_pdf(md).expect("PDF should generate");
    let doc = crate::common::assert_valid_pdf(&pdf_data);

    let links = extract_links(&doc);
    assert!(!links.is_empty(), "Should have link annotations");

    for (url, rect) in &links {
        assert_eq!(url, "http://example.com");
        assert!(!rect.is_empty(), "Rect should have values, got: {:?}", rect);
        assert!(rect.len() >= 4, "Rect should have 4 values [x1, y1, x2, y2], got {:?}", rect);
        assert!(rect[2] > rect[0], "x2 ({}) should be > x1 ({})", rect[2], rect[0]);
        assert!(rect[3] > rect[1], "y2 ({}) should be > y1 ({})", rect[3], rect[1]);
    }
}

#[test]
fn test_pdf_formatting() {
    let pdf_data = markdown_to_pdf(SAMPLE_FORMATTING).expect("PDF should generate");
    let path = save_pdf("02_formatting", &pdf_data);
    
    let report = validate_pdf_structure(&pdf_data).expect("Should validate PDF");
    
    assert!(report.page_count > 0, "PDF should have pages");
    
    println!("✅ Formatting document: {} page(s)", report.page_count);
    println!("   Output: {:?}", path);
}

#[test]
fn test_pdf_links() {
    let pdf_data = markdown_to_pdf(SAMPLE_LINKS).expect("PDF should generate");
    let path = save_pdf("03_links", &pdf_data);
    
    let report = validate_pdf_structure(&pdf_data).expect("Should validate PDF");
    
    // 统计链接数量
    let total_links: usize = report.pages.iter()
        .map(|p| p.annotations.len())
        .sum();
    
    assert!(total_links >= 5, "Should have at least 5 links, found {}", total_links);
    
    println!("✅ Links document: {} page(s), {} link(s)", report.page_count, total_links);
    
    // 打印所有链接
    for page in &report.pages {
        for link in &page.annotations {
            println!("   Page {}: {} -> {:?}", page.number, link.url, link.rect);
        }
    }
    
    println!("   Output: {:?}", path);
}

#[test]
fn test_pdf_lists() {
    let pdf_data = markdown_to_pdf(SAMPLE_LISTS).expect("PDF should generate");
    let path = save_pdf("04_lists", &pdf_data);
    
    let report = validate_pdf_structure(&pdf_data).expect("Should validate PDF");
    
    assert!(report.page_count > 0, "PDF should have pages");
    
    println!("✅ Lists document: {} page(s)", report.page_count);
    println!("   Output: {:?}", path);
}

#[test]
fn test_pdf_tables() {
    let pdf_data = markdown_to_pdf(SAMPLE_TABLES).expect("PDF should generate");
    let path = save_pdf("05_tables", &pdf_data);
    
    let report = validate_pdf_structure(&pdf_data).expect("Should validate PDF");
    
    assert!(report.page_count > 0, "PDF should have pages");
    
    println!("✅ Tables document: {} page(s)", report.page_count);
    println!("   Output: {:?}", path);
}

#[test]
fn test_pdf_quotes() {
    let pdf_data = markdown_to_pdf(SAMPLE_QUOTES).expect("PDF should generate");
    let path = save_pdf("06_quotes", &pdf_data);
    
    let report = validate_pdf_structure(&pdf_data).expect("Should validate PDF");
    
    assert!(report.page_count > 0, "PDF should have pages");
    
    println!("✅ Quotes document: {} page(s)", report.page_count);
    println!("   Output: {:?}", path);
}

#[test]
fn test_pdf_complex() {
    let pdf_data = markdown_to_pdf(SAMPLE_COMPLEX).expect("PDF should generate");
    let path = save_pdf("07_complex", &pdf_data);
    
    let report = validate_pdf_structure(&pdf_data).expect("Should validate PDF");
    
    // 验证链接
    let total_links: usize = report.pages.iter()
        .map(|p| p.annotations.len())
        .sum();
    
    assert!(total_links >= 4, "Complex doc should have at least 4 links, found {}", total_links);
    
    println!("✅ Complex document: {} page(s), {} link(s)", report.page_count, total_links);
    println!("   Output: {:?}", path);
}

#[test]
fn test_pdf_multipage() {
    let pdf_data = markdown_to_pdf(SAMPLE_MULTIPAGE).expect("PDF should generate");
    let path = save_pdf("08_multipage", &pdf_data);
    
    let report = validate_pdf_structure(&pdf_data).expect("Should validate PDF");
    
    // 多页文档应该至少有 2 页
    assert!(report.page_count >= 2, "Should have multiple pages, found {}", report.page_count);
    
    println!("✅ Multipage document: {} page(s)", report.page_count);
    println!("   Output: {:?}", path);
}

/// 运行所有测试并生成报告
#[test]
fn test_pdf_wrapped_links() {
    let pdf_data = markdown_to_pdf(SAMPLE_WRAPPED_LINKS).expect("PDF should generate");
    let path = save_pdf("09_wrapped_links", &pdf_data);
    
    let report = validate_pdf_structure(&pdf_data).expect("Should validate PDF");
    
    // 统计链接
    let total_links: usize = report.pages.iter()
        .map(|p| p.annotations.len())
        .sum();
    
    // 应该有至少 5 个链接，因为长链接会被断行成多个 run，每个 run 产生一个链接 annotation
    assert!(total_links >= 5, "Should have at least 5 link annotations, found {}", total_links);
    
    println!("✅ Wrapped links document: {} page(s), {} link annotation(s)", report.page_count, total_links);
    
    // 打印所有链接的区域信息，用于验证多行链接
    for page in &report.pages {
        for link in &page.annotations {
            println!("   Page {}: {} -> rect=[{:.2}, {:.2}, {:.2}, {:.2}]", 
                page.number, link.url,
                link.rect.first().copied().unwrap_or(0.0),
                link.rect.get(1).copied().unwrap_or(0.0),
                link.rect.get(2).copied().unwrap_or(0.0),
                link.rect.get(3).copied().unwrap_or(0.0));
        }
    }
    
    // 验证同一个 URL 在不同行上有不同的 y 坐标（说明链接被断行）
    let link_groups = group_links_by_url(&report);
    for (url, rects) in &link_groups {
        if rects.len() > 1 {
            let y_positions: Vec<f32> = rects.iter().map(|r| r[1]).collect();
            let min_y = y_positions.iter().cloned().fold(f32::MAX, f32::min);
            let max_y = y_positions.iter().cloned().fold(f32::MIN, f32::max);
            let y_diff = max_y - min_y;
            assert!(y_diff > 10.0, "Multi-line link '{}' should have annotations at different Y positions (diff={:.2}), indicating line wrapping", url, y_diff);
            println!("   ✅ Multi-line link '{}': {} annotations spanning {:.2}pt vertically", url, rects.len(), y_diff);
        }
    }
    
    println!("   Output: {:?}", path);
}

#[test]
fn test_all_samples() {
    println!("\n{}", "=".repeat(60));
    println!("PDF 验证测试套件");
    println!("{}", "=".repeat(60));
    
    let samples = vec![
        ("基础文档", SAMPLE_BASIC),
        ("文本格式", SAMPLE_FORMATTING),
        ("超链接", SAMPLE_LINKS),
        ("列表", SAMPLE_LISTS),
        ("表格", SAMPLE_TABLES),
        ("引用", SAMPLE_QUOTES),
        ("复杂文档", SAMPLE_COMPLEX),
        ("多页文档", SAMPLE_MULTIPAGE),
        ("长链接断行", SAMPLE_WRAPPED_LINKS),
    ];
    
    let mut total_pages = 0;
    let mut total_links = 0;
    
    for (i, (name, content)) in samples.iter().enumerate() {
        let pdf_data = markdown_to_pdf(content).expect(&format!("PDF should generate for {}", name));
        let report = validate_pdf_structure(&pdf_data).expect(&format!("Should validate PDF for {}", name));
        
        let links: usize = report.pages.iter().map(|p| p.annotations.len()).sum();
        total_pages += report.page_count;
        total_links += links;
        
        println!("\n{}. {}: {} 页, {} 链接", i + 1, name, report.page_count, links);
        
        // 保存文件
        let filename = format!("{:02}_{}", i + 1, name);
        save_pdf(&filename, &pdf_data);
    }
    
    println!("\n{}", "=".repeat(60));
    println!("总计: {} 个样本, {} 页, {} 链接", samples.len(), total_pages, total_links);
    println!("输出目录: {:?}", test_output_dir());
    println!("{}", "=".repeat(60));
}
