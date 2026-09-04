//! 管线输出：HTML（`ast::Node` → `node_to_html`）。
//!
//! 与 DOCX 同属「直出 `ast::Node`」路线，因此同样依赖 AST 富化阶段的产物：
//! 预渲染图片（mermaid / liecharts）与代码块语法高亮。

use liepress::markdown_to_html_document;

/// 用默认样式把 Markdown 渲染为完整 HTML 文档。
fn render(md: &str) -> String {
    markdown_to_html_document(md, None, None, None)
}

#[test]
fn html_code_block_has_highlight_spans() {
    let html = render("```rust\nfn main() {\n    let x: i32 = 42;\n}\n```\n");
    assert!(
        html.contains("<span style=\"color:#"),
        "代码块应输出带颜色的高亮 span，实际：{html}"
    );
}

#[test]
fn html_code_block_without_lang_has_single_color() {
    let html = render("```\nsome plain text\n```\n");
    // 无语言时退化为单色：仍应有 span，但只有一种颜色。
    assert!(html.contains("<span style=\"color:#"), "实际：{html}");
}

#[cfg(feature = "mermaid")]
#[test]
fn html_mermaid_block_embeds_png_data_uri() {
    let html = render("```mermaid\nflowchart TD\n  A[开始] --> B[结束]\n```\n");
    assert!(
        html.contains("<img src=\"data:image/png;base64,"),
        "mermaid 图应预渲染为内嵌 PNG，实际：{}",
        &html[..html.len().min(400)]
    );
}

#[cfg(feature = "charts")]
#[test]
fn html_liecharts_block_embeds_png_data_uri() {
    let html = render(concat!(
        "```liecharts\n",
        "{\"xAxis\":[{\"type\":\"category\",\"data\":[\"a\"]}],",
        "\"yAxis\":[{\"type\":\"value\"}],\"series\":[{\"type\":\"bar\",\"data\":[1]}]}\n",
        "```\n"
    ));
    assert!(
        html.contains("<img src=\"data:image/png;base64,"),
        "liecharts 图应预渲染为内嵌 PNG，实际：{}",
        &html[..html.len().min(400)]
    );
}

#[test]
fn html_ordinary_code_block_stays_code() {
    let html = render("```rust\nfn main() {}\n```\n");
    assert!(html.contains("<pre"), "普通代码块应保持 <pre> 结构");
    assert!(
        !html.contains("data:image/png;base64,"),
        "普通代码块不应被当作图表渲染"
    );
}
