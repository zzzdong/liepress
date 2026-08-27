//! 管线 Layer 2：DOM -> Styled AST。
//!
//! 验证 CSS 引擎解析后、布局之前的语义节点结构（脚注、链接、图片、
//! 样式属性已落入 `Node.style`）。

use super::{find_node, markdown_to_ast};
use liepress::ast::NodeKind;

#[test]
fn styled_footnote_def_created() {
    let md = "正文[^1]\n\n[^1]: 脚注内容\n";
    let ast = markdown_to_ast(md);
    let fn_def = find_node(&ast, |n| matches!(&n.kind, NodeKind::FootnoteDef { .. }));
    assert!(fn_def.is_some(), "脚注定义应生成 FootnoteDef 节点");
}

#[test]
fn styled_footnote_ref_links_to_def() {
    let md = "正文[^1]\n\n[^1]: 脚注内容\n";
    let ast = markdown_to_ast(md);
    // 脚注引用渲染为 Superscript 内含 Link，href 形如 #fn-def-1
    let link = find_node(&ast, |n| {
        if let NodeKind::Link { url, .. } = &n.kind {
            return url.starts_with("#fn-def-") || url.starts_with("#fn");
        }
        false
    });
    assert!(link.is_some(), "脚注引用应生成指向脚注定义的链接");
}

#[test]
fn styled_css_width_applied() {
    let md = "<div style=\"width: 200pt\">内容</div>\n";
    let ast = markdown_to_ast(md);
    // width 样式应解析并落到某个节点的 style.width 字段
    let has_width = find_node(&ast, |n| n.style.width == Some(200.0)).is_some();
    assert!(has_width, "width: 200pt 样式应解析到节点的 style.width");
}

#[test]
fn styled_css_color_applied() {
    let md = "<p style=\"color: rgb(255, 0, 0)\">红</p>\n";
    let ast = markdown_to_ast(md);
    let p = find_node(&ast, |n| matches!(&n.kind, NodeKind::Paragraph { .. }));
    assert!(p.is_some());
    if let Some(p) = p {
        assert!(p.style.color.a > 0.0, "color 样式应解析到段落节点");
    }
}

#[test]
fn styled_class_style_from_css() {
    let md = "<style>.hl { color: rgb(0, 0, 255); }</style>\n<p class=\"hl\">高亮</p>\n";
    let ast = markdown_to_ast(md);
    let p = find_node(&ast, |n| matches!(&n.kind, NodeKind::Paragraph { .. }));
    assert!(p.is_some());
    if let Some(p) = p {
        assert!(
            p.style.color.a > 0.0 && p.style.color.b > 0.0,
            "class 选择器样式应解析并应用到节点"
        );
    }
}

#[test]
fn styled_heading_level_preserved() {
    let md = "## 二级标题\n";
    let ast = markdown_to_ast(md);
    let h = find_node(&ast, |n| {
        matches!(&n.kind, NodeKind::Heading { level: 2, .. })
    });
    assert!(h.is_some(), "H2 应解析为 level=2 的 Heading 节点");
}
