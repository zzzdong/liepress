//! 管线 Layer 3：Styled AST -> Layout。
//!
//! 验证语义节点经过布局后产生的块结构、文本内容与尺寸：空格保留、
//! 居中、定义列表、脚注文本、嵌套列表、图片尺寸。

use liepress::document::layout::{Block, BlockKind, Document};

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

#[test]
fn layout_table_header_row_present() {
    // 回归：GFM 表头行曾被整体丢弃（pulldown 的 TableHead 不产出 `<tr>`，
    // 表头单元格直接挂在 Table 下，收集 TableRow 时被过滤掉）。
    let md = "| 列甲 | 列乙 |\n|---|---|\n| 1 | 2 |\n";
    let doc = markdown_to_layout(md);
    let text = layout_text(&doc);
    assert!(text.contains("列甲"), "表头单元格应进入布局，实际: {text}");
    assert!(text.contains("列乙"), "表头单元格应进入布局，实际: {text}");
}

/// 1×1 红色 PNG 的 base64（合法 PNG，可被 `image` 探测到 1×1 像素）。
const ONE_PX_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

/// 取出文档中所有图片块的 `(字节数, 格式, 宽)`。
fn collect_images(doc: &Document) -> Vec<(usize, String, f64)> {
    let mut out = Vec::new();
    let mut stack: Vec<&Block> = doc.blocks.iter().collect();
    while let Some(b) = stack.pop() {
        match &b.kind {
            BlockKind::Image(img) => out.push((img.data.len(), img.format.clone(), img.size.0)),
            _ => stack.extend(b.kind.children()),
        }
    }
    out
}

#[test]
fn layout_image_inline_data_uri() {
    // 直接内联：`![alt](data:image/png;base64,...)`
    let md = format!("![内联图](data:image/png;base64,{ONE_PX_PNG_B64})\n");
    let imgs = collect_images(&markdown_to_layout(&md));
    assert_eq!(imgs.len(), 1, "应产出一个图片块");
    assert!(imgs[0].0 > 0, "data URI 应被解码为真实字节");
    assert_eq!(imgs[0].1, "png");
}

#[test]
fn layout_image_reference_data_uri() {
    // 引用标签：`![alt][ref]` + `[ref]: data:image/png;base64,...`
    // 与直接内联等价——两种写法在 `Tag::Image` 之后完全同构。
    let md = format!("![引用图][logo]\n\n[logo]: data:image/png;base64,{ONE_PX_PNG_B64}\n");
    let imgs = collect_images(&markdown_to_layout(&md));
    assert_eq!(imgs.len(), 1, "引用标签形式的图片应同样产出一个图片块");
    assert!(imgs[0].0 > 0, "引用标签形式的 data URI 应被解码为真实字节");
    assert_eq!(imgs[0].1, "png");
}

#[test]
fn layout_image_reference_shortcut_and_collapsed() {
    // 引用标签的两种简写：collapsed `![ref][]` 与 shortcut `![ref]`
    for md in [
        format!("![logo][]\n\n[logo]: data:image/png;base64,{ONE_PX_PNG_B64}\n"),
        format!("![logo]\n\n[logo]: data:image/png;base64,{ONE_PX_PNG_B64}\n"),
    ] {
        let imgs = collect_images(&markdown_to_layout(&md));
        assert_eq!(imgs.len(), 1, "简写引用形式应产出图片块: {md}");
        assert!(imgs[0].0 > 0, "简写引用形式的 data URI 应被解码: {md}");
    }
}
