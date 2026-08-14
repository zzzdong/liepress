//! 测试公共模块
//!
//! 提供测试共享的工具函数和类型

#![allow(dead_code)]

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

/// 确保测试用的图片存在，如果不存在则创建一个渐变色 PNG
pub fn ensure_test_image(path: &PathBuf) {
    if path.exists() {
        return;
    }
    fs::create_dir_all(path.parent().unwrap()).expect("Should create fixtures dir");
    let mut img = image::RgbaImage::new(400, 300);
    for x in 0..400 {
        for y in 0..300 {
            let r = ((x as f32 / 400.0) * 255.0) as u8;
            let g = ((y as f32 / 300.0) * 255.0) as u8;
            let b = ((1.0 - (x as f32 / 400.0 + y as f32 / 300.0) * 0.5) * 255.0) as u8;
            img.put_pixel(x, y, image::Rgba([r, g, b, 255]));
        }
    }
    img.save(path).expect("Should create test image");
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

/// 提取 PDF 目录（outline）所有标题文本。
///
/// 返回按文档顺序的标题列表；无目录时返回空 Vec。
pub fn extract_outline_titles(doc: &Document) -> Vec<String> {
    use lopdf::{Dictionary, ObjectId};
    let mut titles = Vec::new();

    // 从 trailer 的 /Root 取 /Outlines 根对象 id。
    let outlines_id: ObjectId = match doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .and_then(|root_id| doc.get_object(root_id).ok())
        .and_then(|root| root.as_dict().ok())
        .and_then(|d| d.get(b"Outlines").ok())
        .and_then(|o| o.as_reference().ok())
    {
        Some(rid) => rid,
        None => return titles,
    };

    fn collect_children(doc: &Document, id: ObjectId, out: &mut Vec<String>) {
        let mut cur = Some(id);
        let mut guard = 0usize;
        while let Some(oid) = cur {
            guard += 1;
            if guard > 10000 {
                break;
            }
            let dict: Dictionary = match doc.get_object(oid).ok().and_then(|o| o.as_dict().ok()) {
                Some(d) => d.clone(),
                None => break,
            };
            // 标题文本：/Title 条目（字节串 → UTF-8）
            if let Ok(title_bytes) = dict.get(b"Title").and_then(|o| o.as_str()) {
                out.push(String::from_utf8_lossy(title_bytes).to_string());
            }
            // 子项：/First（递归）
            if let Ok(first) = dict.get(b"First").and_then(|o| o.as_reference()) {
                collect_children(doc, first, out);
            }
            // 兄弟项：/Next
            match dict.get(b"Next").and_then(|o| o.as_reference()) {
                Ok(next) => cur = Some(next),
                Err(_) => break,
            }
        }
    }

    // 从 /Outlines 根的 /First 开始遍历
    if let Ok(dict) = doc.get_object(outlines_id)
        && let Ok(d) = dict.as_dict()
        && let Ok(first) = d.get(b"First").and_then(|o| o.as_reference())
    {
        collect_children(doc, first, &mut titles);
    }
    titles
}

/// 从注解字典中提取 URL
fn url_from_annot_dict(doc: &Document, annot_dict: &lopdf::Dictionary) -> String {
    annot_dict
        .get(b"A")
        .ok()
        .and_then(|a| {
            doc.dereference(a).ok().and_then(|(_, obj)| {
                obj.as_dict()
                    .ok()
                    .and_then(|d| d.get(b"URI").ok().and_then(|u| u.as_str().ok()))
            })
        })
        .map(|s| String::from_utf8_lossy(s).to_string())
        .or_else(|| {
            annot_dict.get(b"URI").ok().and_then(|u| {
                u.as_str()
                    .ok()
                    .map(|s| String::from_utf8_lossy(s).to_string())
            })
        })
        .unwrap_or_default()
}

/// 将 PDF 对象转换为 f32（兼容 Integer 和 Real）
fn obj_to_f32(obj: &lopdf::Object) -> Option<f32> {
    obj.as_f32()
        .ok()
        .or_else(|| obj.as_i64().ok().map(|v| v as f32))
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
                let url = url_from_annot_dict(doc, annot_dict);
                let rect = rect_from_annot_dict(annot_dict);
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
    assert!(
        found,
        "Should find link to {}, found: {:?}",
        expected_url, links
    );
}

/// 验证 PDF 中链接数量至少为 N
pub fn assert_link_count(doc: &Document, min: usize) -> Vec<(String, Vec<f32>)> {
    let links = extract_links(doc);
    assert!(
        links.len() >= min,
        "Should have at least {} links, found {}",
        min,
        links.len()
    );
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
                if let Ok(subtype) = annot.get(b"Subtype").and_then(|o| o.as_name())
                    && subtype == b"Link"
                {
                    let url = url_from_annot_dict(&doc, annot);
                    let rect = rect_from_annot_dict(annot);
                    page_info.annotations.push(LinkInfo { url, rect });
                }
            }
        }

        report.pages.push(page_info);
    }

    report
}

/// 按 URL 对链接矩形分组
pub fn group_links_by_url(report: &PdfReport) -> std::collections::HashMap<String, Vec<Vec<f32>>> {
    let mut groups: std::collections::HashMap<String, Vec<Vec<f32>>> =
        std::collections::HashMap::new();
    for page in &report.pages {
        for link in &page.annotations {
            groups
                .entry(link.url.clone())
                .or_default()
                .push(link.rect.clone());
        }
    }
    groups
}

// ─── lopdf 深度验证工具 ──────────────────────────────────

/// 从 PDF 页面内容流中提取文本字符串（解析 Tj / TJ 操作符）
///
/// 返回每个页面的文本字符串列表
pub fn extract_pdf_text(doc: &Document) -> Vec<String> {
    let mut page_texts = Vec::new();
    let pages = doc.get_pages();
    for page_id in pages.values() {
        let mut text = String::new();
        if let Ok(content) = doc.get_and_decode_page_content(*page_id) {
            // 遍历 Content Stream 的操作符
            for operation in &content.operations {
                match operation.operator.as_str() {
                    "Tj" => {
                        // 单字符串文本
                        if let Some(lopdf::Object::String(bytes, _)) = operation.operands.first() {
                            text.push_str(&String::from_utf8_lossy(bytes));
                        }
                    }
                    "TJ" => {
                        // 数组形式文本（可能混合字符串和间距调整）
                        if let Some(lopdf::Object::Array(arr)) = operation.operands.first() {
                            for item in arr {
                                if let lopdf::Object::String(bytes, _) = item {
                                    text.push_str(&String::from_utf8_lossy(bytes));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        page_texts.push(text);
    }
    page_texts
}

/// 断言 PDF 中包含指定文本
pub fn assert_pdf_contains_text(doc: &Document, expected: &str) {
    let page_texts = extract_pdf_text(doc);
    let all_text: String = page_texts.join(" ");
    assert!(
        all_text.contains(expected),
        "PDF should contain text {:?}, but extracted text is: {:?}",
        expected,
        &all_text[..all_text.len().min(500)]
    );
}

/// 提取 PDF 元数据（Title, Author, Subject 等）
pub fn get_pdf_metadata(doc: &Document) -> std::collections::HashMap<String, String> {
    let mut meta = std::collections::HashMap::new();
    if let Ok(catalog) = doc.catalog()
        && let Ok(info_ref) = catalog.get(b"Info")
        && let Ok(info_id) = info_ref.as_reference()
        && let Ok(info_dict) = doc.get_object(info_id).and_then(|o| o.as_dict())
    {
        for key in &["Title", "Author", "Subject", "Creator", "Producer"] {
            if let Ok(val) = info_dict.get(key.as_bytes())
                && let Ok(s) = val.as_str()
            {
                meta.insert(key.to_string(), String::from_utf8_lossy(s).to_string());
            }
        }
    }
    // 回退：从 trailer Info 字典中读取
    if meta.is_empty()
        && let Some(info_id) = doc
            .trailer
            .get(b"Info")
            .ok()
            .and_then(|o| o.as_reference().ok())
        && let Ok(info_dict) = doc.get_object(info_id).and_then(|o| o.as_dict())
    {
        for key in &["Title", "Author", "Subject", "Creator", "Producer"] {
            if let Ok(val) = info_dict.get(key.as_bytes())
                && let Ok(s) = val.as_str()
            {
                meta.insert(key.to_string(), String::from_utf8_lossy(s).to_string());
            }
        }
    }
    meta
}

/// 统计 PDF 中嵌入的字体资源数量
pub fn count_font_resources(doc: &Document) -> usize {
    let mut font_count = 0;
    let pages = doc.get_pages();
    for page_id in pages.values() {
        if let Ok(fonts) = doc.get_page_fonts(*page_id) {
            font_count += fonts.len();
        }
    }
    font_count
}

/// 获取所有页面的 MediaBox 尺寸 [(width, height), ...]
pub fn get_page_media_boxes(doc: &Document) -> Vec<(f64, f64)> {
    let mut boxes = Vec::new();
    let pages = doc.get_pages();
    for page_id in pages.values() {
        if let Ok(page_dict) = doc.get_dictionary(*page_id)
            && let Ok(media_box) = page_dict.get(b"MediaBox").and_then(|o| o.as_array())
            && media_box.len() == 4
        {
            let x0 = media_box[0].as_float().unwrap_or(0.0) as f64;
            let y0 = media_box[1].as_float().unwrap_or(0.0) as f64;
            let x1 = media_box[2].as_float().unwrap_or(0.0) as f64;
            let y1 = media_box[3].as_float().unwrap_or(0.0) as f64;
            boxes.push((x1 - x0, y1 - y0));
        }
    }
    boxes
}

/// 验证 PDF 页面尺寸是否为 A4 (595.276 x 841.890 pt)
pub fn assert_a4_page_size(doc: &Document, tolerance: f64) {
    let boxes = get_page_media_boxes(doc);
    for (i, (w, h)) in boxes.iter().enumerate() {
        // A4 纵向：595.276 x 841.890 pt
        let is_a4_portrait = (w - 595.276).abs() < tolerance && (h - 841.890).abs() < tolerance;
        // A4 横向：841.890 x 595.276 pt
        let is_a4_landscape = (w - 841.890).abs() < tolerance && (h - 595.276).abs() < tolerance;
        assert!(
            is_a4_portrait || is_a4_landscape,
            "Page {} should be A4 size, got ({:.1}, {:.1})",
            i + 1,
            w,
            h
        );
    }
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

    /// 图片插入示例
    pub const IMAGE_EXAMPLE: &str = r#"# Image Example

This document demonstrates image insertion.

## Basic Image

Text before the image.

![Test Image](tests/fixtures/test_image.png)

Text after the image.

## Image with Caption

A colorful gradient pattern with caption.

![Colorful Gradient Pattern](tests/fixtures/test_image.png)

## Image Between Text

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.

![Centered Image](tests/fixtures/test_image.png)

Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur.

## Multiple Images

![First Image](tests/fixtures/test_image.png)

![Second Image](tests/fixtures/test_image.png)
"#;
}
