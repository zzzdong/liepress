//! 测试公共模块
//!
//! 提供测试共享的工具函数和类型

use std::fs;
use std::path::PathBuf;

use lopdf::Document;

/// 获取测试输出目录
pub fn test_output_dir(test_name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push("liepress_tests");
    path.push(test_name);
    let _ = fs::create_dir_all(&path);
    path
}

/// 获取诊断输出目录
pub fn diag_output_dir(subdir: &str) -> PathBuf {
    let dir = PathBuf::from("target/diag_output").join(subdir);
    fs::create_dir_all(&dir).expect("Should create output directory");
    dir
}

/// 保存测试输出文件
pub fn save_test_output(path: &PathBuf, data: &[u8]) {
    fs::write(path, data).expect("Should write output file");
}

// ─── lopdf 验证工具 ───────────────────────────────────────

/// 加载 PDF 数据，返回 lopdf Document
pub fn load_pdf(data: &[u8]) -> Document {
    assert!(!data.is_empty(), "PDF data should not be empty");
    assert_eq!(&data[0..4], b"%PDF", "Should start with PDF header");
    Document::load_mem(data).expect("Should load PDF from memory")
}

/// 统计 PDF 页数
pub fn count_pages(doc: &Document) -> usize {
    let count = doc.get_pages().len();
    assert!(count > 0, "PDF should have at least one page");
    count
}

/// 从 PDF 数据中加载并返回页数
pub fn pdf_page_count(data: &[u8]) -> usize {
    let doc = load_pdf(data);
    count_pages(&doc)
}

/// 验证 PDF 基本结构（header + 至少一页）
pub fn assert_valid_pdf(data: &[u8]) -> Document {
    let doc = load_pdf(data);
    count_pages(&doc);
    doc
}

/// 从注解字典中提取 URL
fn url_from_annot_dict(doc: &Document, annot_dict: &lopdf::Dictionary) -> String {
    annot_dict
        .get(b"A")
        .ok()
        .and_then(|a| {
            doc.dereference(a).ok().and_then(|(_, obj)| {
                obj.as_dict().ok().and_then(|d| {
                    d.get(b"URI").ok().and_then(|u| u.as_str().ok())
                })
            })
        })
        .map(|s| String::from_utf8_lossy(s).to_string())
        .or_else(|| {
            annot_dict
                .get(b"URI")
                .ok()
                .and_then(|u| {
                    u.as_str().ok()
                        .map(|s| String::from_utf8_lossy(s).to_string())
                })
        })
        .unwrap_or_default()
}

/// 将 PDF 对象转换为 f32（兼容 Integer 和 Real）
fn obj_to_f32(obj: &lopdf::Object) -> Option<f32> {
    obj.as_f32().ok().or_else(|| obj.as_i64().ok().map(|v| v as f32))
}

/// 从注解字典中提取矩形区域
fn rect_from_annot_dict(annot_dict: &lopdf::Dictionary) -> Vec<f32> {
    annot_dict
        .get(b"Rect")
        .ok()
        .and_then(|r| r.as_array().ok())
        .map(|arr| arr.iter().filter_map(obj_to_f32).collect())
        .unwrap_or_default()
}

/// 提取 PDF 中所有链接注释
pub fn extract_links(doc: &Document) -> Vec<(String, Vec<f32>)> {
    let mut links = Vec::new();
    let pages = doc.get_pages();
    for (_, page_id) in pages {
        if let Ok(annots) = doc.get_page_annotations(page_id) {
            for annot_dict in annots {
                let subtype = annot_dict
                    .get(b"Subtype")
                    .ok()
                    .and_then(|o| o.as_name().ok())
                    .unwrap_or_default();
                if subtype != b"Link" {
                    continue;
                }
                let url = url_from_annot_dict(doc, &annot_dict);
                let rect = rect_from_annot_dict(&annot_dict);
                links.push((url, rect));
            }
        }
    }
    links
}

/// 验证 PDF 中存在指定 URL 的链接
pub fn assert_has_link(doc: &Document, expected_url: &str) {
    let links = extract_links(doc);
    let found = links.iter().any(|(url, _)| url == expected_url);
    assert!(found, "Should find link to {}, found: {:?}", expected_url, links);
}

/// 验证 PDF 中链接数量至少为 N
pub fn assert_link_count(doc: &Document, min: usize) -> Vec<(String, Vec<f32>)> {
    let links = extract_links(doc);
    assert!(links.len() >= min, "Should have at least {} links, found {}", min, links.len());
    links
}

/// 页面信息
#[derive(Debug)]
pub struct PageInfo {
    pub number: usize,
    pub annotations: Vec<LinkInfo>,
}

/// 链接信息
#[derive(Debug, Clone)]
pub struct LinkInfo {
    pub url: String,
    pub rect: Vec<f32>,
}

/// 验证报告
#[derive(Debug, Default)]
pub struct PdfReport {
    pub has_valid_header: bool,
    pub page_count: usize,
    pub pages: Vec<PageInfo>,
}

/// 对 PDF 数据进行完整的结构验证
pub fn validate_pdf_structure(data: &[u8]) -> PdfReport {
    let doc = load_pdf(data);
    let mut report = PdfReport {
        has_valid_header: true,
        page_count: 0,
        pages: Vec::new(),
    };

    let pages = doc.get_pages();
    report.page_count = pages.len();
    assert!(report.page_count > 0, "PDF should have at least one page");

    for (page_num, (_, page_id)) in pages.iter().enumerate() {
        let mut page_info = PageInfo {
            number: page_num + 1,
            annotations: Vec::new(),
        };

        if let Ok(annots) = doc.get_page_annotations(*page_id) {
            for annot in annots {
                if let Ok(subtype) = annot.get(b"Subtype").and_then(|o| o.as_name()) {
                    if subtype == b"Link" {
                        let url = url_from_annot_dict(&doc, &annot);
                        let rect = rect_from_annot_dict(&annot);
                        page_info.annotations.push(LinkInfo { url, rect });
                    }
                }
            }
        }

        report.pages.push(page_info);
    }

    report
}

/// 按 URL 对链接矩形分组
pub fn group_links_by_url(report: &PdfReport) -> std::collections::HashMap<String, Vec<Vec<f32>>> {
    let mut groups: std::collections::HashMap<String, Vec<Vec<f32>>> = std::collections::HashMap::new();
    for page in &report.pages {
        for link in &page.annotations {
            groups.entry(link.url.clone()).or_default().push(link.rect.clone());
        }
    }
    groups
}

/// 测试用的 Markdown 样本
pub mod samples {
    /// 基础文档
    pub const BASIC: &str = r#"# Test Document

This is a test paragraph."#;

    /// 完整功能展示
    pub const FULL_FEATURED: &str = r#"# Heading 1

This is a paragraph with **bold** and *italic* text.

## Heading 2

- List item 1
- List item 2
- List item 3

```rust
fn main() {
    println!("Hello, world!");
}
```

> This is a blockquote.

---

[Link to example](https://example.com)"#;

    /// 代码块
    pub const CODE_BLOCK: &str = r#"# Code Example

```rust
fn main() {
    println!("hello");
}
```"#;

    /// 嵌套列表
    pub const NESTED_LIST: &str = r#"# Nested List

- Item 1
  - Sub item 1.1
  - Sub item 1.2
- Item 2
  - Sub item 2.1"#;

    /// 有序列表
    pub const ORDERED_LIST: &str = r#"1. First item
2. Second item
3. Third item"#;

    /// 简单表格
    pub const SIMPLE_TABLE: &str = r#"| Header 1 | Header 2 |
|----------|----------|
| Cell A1  | Cell B1  |
| Cell A2  | Cell B2  |"#;

    /// 多列表格
    pub const WIDE_TABLE: &str = r#"| Name   | Age | City      | Country   |
|--------|-----|-----------|-----------|
| Alice  | 30  | New York  | USA       |
| Bob    | 25  | London    | UK        |
| Charlie| 35  | Beijing   | China     |"#;

    /// 各种对齐的表格
    pub const ALIGNED_TABLE: &str = r#"| Left   | Center | Right |
|:-------|:------:|------:|
| L1     | C1     | R1    |
| L2     | C2     | R2    |"#;

    /// 大表格（用于测试跨页）
    pub const LARGE_TABLE: &str = r#"| #  | Name        | Description                              |
|----|-------------|------------------------------------------|
| 1  | Item One    | This is the first item with a longer description that wraps |
| 2  | Item Two    | The second item description goes here and might wrap too |
| 3  | Item Three  | Short description                        |
| 4  | Item Four   | Another item with some details here      |
| 5  | Item Five   | Yet another item with description text that could wrap |
| 6  | Item Six    | Short                                     |
| 7  | Item Seven  | A longer description for item seven here |
| 8  | Item Eight  | Eighth item with description              |
| 9  | Item Nine   | Ninth item description goes here         |
|10  | Item Ten    | Tenth and final item description          |"#;

    /// 带内联格式的表格
    pub const FORMATTED_TABLE: &str = r#"| Feature        | Status |
|----------------|--------|
| **Bold text**  | ✅ Done |
| *Italic text*  | ✅ Done |
| `inline code`  | ⏳ WIP  |"#;

    /// 空表格
    pub const EMPTY_TABLE: &str = r#"| H1 | H2 |
|----|----|"#;
}
