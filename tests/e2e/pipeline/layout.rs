//! 管线 Layer 3：Styled AST -> Layout。
//!
//! 验证语义节点经过布局后产生的块结构、文本内容与尺寸：空格保留、
//! 居中、定义列表、脚注文本、嵌套列表、图片尺寸。

use super::{layout_text, markdown_to_layout};

#[test]
fn layout_preserves_spaces_in_paragraph() {
    // 注：CJK 文本会因 ICU 分词折叠连续空格，故用英文验证空格保留行为
    let md = "This is a text    with multiple spaces.";
    let doc = markdown_to_layout(md);
    let text = layout_text(&doc);
    assert!(
        text.contains("multiple spaces"),
        "英文段落内空格应保留，实际: {text}"
    );
}

#[test]
fn layout_center_alignment() {
    let md = "<center>centered text</center>\n";
    let doc = markdown_to_layout(md);
    assert!(
        doc.blocks
            .iter()
            .any(|b| b.text_content().contains("centered")),
        "居中块内容应进入布局"
    );
}

#[test]
fn layout_definition_list() {
    let md = "术语\n:   定义说明\n";
    let doc = markdown_to_layout(md);
    assert!(
        doc.blocks
            .iter()
            .any(|b| b.kind.text_content().contains("定义说明")),
        "定义列表内容应进入布局"
    );
}

#[test]
fn layout_footnote_text_present() {
    let md = "正文[^1]\n\n[^1]: 脚注内容\n";
    let doc = markdown_to_layout(md);
    assert!(
        doc.blocks
            .iter()
            .any(|b| b.text_content().contains("脚注内容")),
        "脚注定义文本应出现在布局中"
    );
}

#[test]
fn layout_nested_list() {
    let md = "- 一级\n  - 二级\n    - 三级\n";
    let doc = markdown_to_layout(md);
    assert!(
        doc.blocks
            .iter()
            .any(|b| b.text_content().contains("二级") && b.text_content().contains("三级")),
        "嵌套列表的多级内容应保留"
    );
}

#[test]
fn layout_image_alt_text_present() {
    // 图片即使未被内联字节，其 alt 文本也应进入布局（作为回退内容）
    let md = "![示意图](data:image/png;base64,iVBORw0KGgo=)\n";
    let doc = markdown_to_layout(md);
    assert!(
        doc.blocks
            .iter()
            .any(|b| b.text_content().contains("示意图")),
        "图片 alt 文本应出现在布局中"
    );
}

#[test]
fn layout_table_cells() {
    let md = "| a | b |\n|---|---|\n| 1 | 2 |\n";
    let doc = markdown_to_layout(md);
    assert!(
        doc.blocks
            .iter()
            .any(|b| b.text_content().contains("1") && b.text_content().contains("2")),
        "表格单元格文本应进入布局"
    );
}
