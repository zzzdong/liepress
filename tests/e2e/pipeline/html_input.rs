//! HTML 输入源：以 HTML（而非 Markdown）作为管线入口。
//!
//! 验证 `html_to_pdf` / `html_to_svg` / `html_to_png` / `html_to_docx` 系列
//! 在原始 HTML 输入下的行为。

use liepress::ConvertOptions;
use liepress::html_to_docx;
use liepress::html_to_pdf;
use liepress::html_to_png;
use liepress::html_to_svg;

fn opts() -> ConvertOptions {
    ConvertOptions::new().with_auto_font(false)
}

const HTML: &str =
    "<h1>标题</h1><p>这是一段<b>加粗</b>正文。</p><ul><li>项一</li><li>项二</li></ul>";

#[test]
fn html_to_pdf_pipeline() {
    let bytes = html_to_pdf(HTML, &opts()).expect("转换应成功");
    assert!(!bytes.is_empty());
    let doc = lopdf::Document::load_mem(&bytes).expect("PDF 应可解析");
    assert!(doc.get_pages().len() >= 1);
}

#[test]
fn html_to_svg_pipeline() {
    let svg = html_to_svg(HTML, &opts()).expect("转换应成功");
    assert!(!svg.is_empty());
    assert!(svg.contains("<svg"));
}

#[test]
fn html_to_png_pipeline() {
    let bytes = html_to_png(HTML, &opts()).expect("转换应成功");
    assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
}

#[test]
fn html_to_docx_pipeline() {
    let bytes = html_to_docx(HTML, &opts()).expect("转换应成功");
    assert_eq!(&bytes[..4], &[0x50, 0x4B, 0x03, 0x04]);
}

#[test]
fn html_to_pdf_with_table() {
    let html = "<table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2</td></tr></table>";
    let bytes = html_to_pdf(html, &opts()).expect("转换应成功");
    let doc = lopdf::Document::load_mem(&bytes).expect("PDF 应可解析");
    assert!(doc.get_pages().len() >= 1);
}
