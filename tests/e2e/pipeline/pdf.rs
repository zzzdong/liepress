//! 管线输出：PDF。
//!
//! 覆盖完整管线（Markdown -> PDF）端到端行为：基础结构、Unicode、
//! 特殊字符、嵌套结构、表格、大纲。深度 PDF 校验（字体/对象/资源）见
//! `../pdf_validation.rs`。

use liepress::ConvertOptions;
use liepress::markdown_to_pdf;
use lopdf::Document;

fn opts() -> ConvertOptions {
    ConvertOptions::new().with_auto_font(false)
}

#[test]
fn pdf_full_pipeline() {
    let md = "# 标题\n\n这是一段正文内容，用于验证完整管线。\n";
    let bytes = markdown_to_pdf(md, &opts()).expect("转换应成功");
    assert!(!bytes.is_empty());

    let doc = Document::load_mem(&bytes).expect("PDF 应可解析");
    let pages = doc.get_pages();
    assert_eq!(pages.len(), 1, "单页内容应生成 1 页");
}

#[test]
fn pdf_unicode_content() {
    let md = "# 测试\n\n支持中文、日本語、한국어 与 emoji 🚀🌟 混合文本。\n";
    let bytes = markdown_to_pdf(md, &opts()).expect("转换应成功");
    assert!(!bytes.is_empty());
    let doc = Document::load_mem(&bytes).expect("PDF 应可解析");
    assert!(doc.get_pages().len() >= 1);
}

#[test]
fn pdf_special_characters() {
    let md = "# 特殊字符\n\n& < > \" ' © ® ™ § ¶ • — – … € £ ¥\n";
    let bytes = markdown_to_pdf(md, &opts()).expect("转换应成功");
    assert!(!bytes.is_empty());
    let doc = Document::load_mem(&bytes).expect("PDF 应可解析");
    assert!(doc.get_pages().len() >= 1);
}

#[test]
fn pdf_nested_structure() {
    let md = "# 一级\n\n## 二级\n\n### 三级\n\n- 列表项\n  - 嵌套项\n\n> 引用文本\n";
    let bytes = markdown_to_pdf(md, &opts()).expect("转换应成功");
    assert!(!bytes.is_empty());
    let doc = Document::load_mem(&bytes).expect("PDF 应可解析");
    assert!(doc.get_pages().len() >= 1);
}

#[test]
fn pdf_table() {
    let md = "| 名称 | 值 |\n|------|----|\n| A | 1 |\n| B | 2 |\n";
    let bytes = markdown_to_pdf(md, &opts()).expect("转换应成功");
    assert!(!bytes.is_empty());
    let doc = Document::load_mem(&bytes).expect("PDF 应可解析");
    assert!(doc.get_pages().len() >= 1);
}

#[test]
fn pdf_outline_exists() {
    let md = "# 第一章\n\n内容一\n\n# 第二章\n\n内容二\n";
    let bytes = markdown_to_pdf(md, &opts()).expect("转换应成功");
    let doc = Document::load_mem(&bytes).expect("PDF 应可解析");
    // 大纲（书签）应在目录对象中存在
    let has_outline = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Outlines").ok())
        .is_some();
    assert!(has_outline, "含多个 H1 的文档应生成大纲");
}
