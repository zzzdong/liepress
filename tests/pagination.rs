//! 分页回归测试：可分割块（代码块 / 段落）跨页时不得丢失内容。
//!
//! 背景：`paginate_layout` 曾只对表格做按行分页，超长代码块/段落被整块放进单页、
//! 溢出裁剪导致尾部内容丢失。本测试锁定该回归：长代码块与长段落必须跨多页。

use liepress::{ConvertOptions, markdown_to_pdf};

/// 用 lopdf 统计 PDF 页数（仅统计页面对象，不依赖文本提取）。
fn pdf_page_count(pdf: &[u8]) -> usize {
    let doc = lopdf::Document::load_mem(pdf).expect("load pdf");
    doc.get_pages().len()
}

#[test]
fn long_code_block_spans_multiple_pages() {
    // 200 行代码：修复前整块溢出到单页（尾部被裁剪），修复后应跨 3+ 页。
    let mut code = String::from("fn main() {\n");
    for i in 1..=200 {
        code.push_str(&format!("    println!(\"line {}\");\n", i));
    }
    code.push_str("}\n");
    let md = format!("# Long code\n\n```rust\n{}```\n", code);

    let pdf = markdown_to_pdf(&md, &ConvertOptions::default()).expect("generate pdf");
    let pages = pdf_page_count(&pdf);
    assert!(
        pages >= 3,
        "200 行代码块应跨多页，实际 {} 页（修复前溢出成 2 页）",
        pages
    );
}

#[test]
fn long_paragraph_spans_multiple_pages() {
    // 超长段落：一行接一行的文本，超过一页时应跨页而非溢出。
    let mut words = String::from("# Long paragraph\n\n");
    for _ in 0..2000 {
        words.push_str("这是一个很长的中文段落用于测试跨页分页行为。");
    }
    let pdf = markdown_to_pdf(&words, &ConvertOptions::default()).expect("generate pdf");
    let pages = pdf_page_count(&pdf);
    assert!(pages >= 2, "超长段落应跨页，实际 {} 页", pages);
}
