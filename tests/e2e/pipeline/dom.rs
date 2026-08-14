//! 管线 Layer 1：Markdown / HTML -> DOM。
//!
//! 验证原始文档树的结构（标签、层级、属性），不关心样式或布局。

use liepress::dom::HtmlTag;
use liepress::dom::markdown_to_dom;
use liepress::dom::parse_html;

#[test]
fn dom_headings_have_levels() {
    let md = "# H1\n\n## H2\n\n### H3\n";
    let doc = markdown_to_dom(md);
    assert_eq!(doc.find_all(HtmlTag::H1).len(), 1);
    assert_eq!(doc.find_all(HtmlTag::H2).len(), 1);
    assert_eq!(doc.find_all(HtmlTag::H3).len(), 1);
}

#[test]
fn dom_paragraphs_and_inline() {
    let md = "A paragraph with **bold** and *em*.";
    let doc = markdown_to_dom(md);
    assert_eq!(doc.find_all(HtmlTag::P).len(), 1);
    assert_eq!(doc.find_all(HtmlTag::Strong).len(), 1);
    assert_eq!(doc.find_all(HtmlTag::Em).len(), 1);
}

#[test]
fn dom_lists_ul_ol() {
    let md = "- a\n- b\n\n1. one\n2. two\n";
    let doc = markdown_to_dom(md);
    assert_eq!(doc.find_all(HtmlTag::Ul).len(), 1);
    assert_eq!(doc.find_all(HtmlTag::Ol).len(), 1);
    assert_eq!(doc.find_all(HtmlTag::Li).len(), 4);
}

#[test]
fn dom_table() {
    let md = "| a | b |\n|---|---|\n| 1 | 2 |\n";
    let doc = markdown_to_dom(md);
    assert_eq!(doc.find_all(HtmlTag::Table).len(), 1);
    // 表头 + 数据共 4 个单元格（对齐分隔行不计入）
    let cells = doc.find_all(HtmlTag::Th).len() + doc.find_all(HtmlTag::Td).len();
    assert_eq!(cells, 4, "表格应含 2 表头 + 2 数据单元格");
}

#[test]
fn dom_blockquote_and_code() {
    let md = "> quote\n\n```\ncode\n```\n";
    let doc = markdown_to_dom(md);
    assert_eq!(doc.find_all(HtmlTag::Blockquote).len(), 1);
    assert_eq!(doc.find_all(HtmlTag::Pre).len(), 1);
}

#[test]
fn dom_links_and_images() {
    let md = "[t](http://e.com) ![alt](http://e.com/i.png)\n";
    let doc = markdown_to_dom(md);
    assert_eq!(doc.find_all(HtmlTag::A).len(), 1);
    assert_eq!(doc.find_all(HtmlTag::Img).len(), 1);
}

#[test]
fn dom_raw_html_passthrough() {
    let html = "<section><p>raw</p></section>";
    let doc = parse_html(html);
    assert_eq!(doc.find_all(HtmlTag::Section).len(), 1);
    assert_eq!(doc.find_all(HtmlTag::P).len(), 1);
}
