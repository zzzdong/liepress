//! AST 外绘 pass：把绘图语言代码块就地替换为图片节点。
//!
//! 在 AST 建好之后、交给任何输出后端之前运行（由 [`crate::enrich`] 编排），
//! 把 ` ```mermaid ` / ` ```liecharts ` 等代码块渲染为 PNG，并以 **base64 data URI**
//! 写回 `NodeKind::Image`。此后五个后端无差别地消费图片节点：
//! - PDF/SVG/PNG：`document::from_ast::convert_image_node` 解码 data URI；
//! - DOCX：`output::docx::emit_image` 嵌入 `Pic`；
//! - HTML：`output::html` 直接序列化 `<img src="data:...">`。
//!
//! 幂等：替换后的节点是 `Image`，再次运行不会命中任何代码块。
//! 渲染失败**软降级**：保留代码块并把错误注释写进 `code`，不中断整篇文档。

use crate::ast::{Node, NodeKind, Style, TextAlign, walk_mut};
use crate::document::types::PageSettings;
use crate::dom::resource::base64_encode;
use crate::ext_render::{RenderOpts, RenderedImage, find_renderer, parse_info_string};

/// 遍历整棵 AST，把所有「已注册绘图语言」的代码块替换为图片节点。
pub fn render_ext_blocks(node: &mut Node, settings: &PageSettings) {
    walk_mut(node, &mut |n| try_replace(n, settings));
}

/// 尝试把单个代码块节点替换为图片节点（非绘图语言则原样返回）。
fn try_replace(node: &mut Node, settings: &PageSettings) {
    // 先取出必要字段（克隆成本低：代码块文本通常很小），避免借用冲突。
    let (code, lang) = match &node.kind {
        NodeKind::CodeBlock {
            code,
            lang: Some(lang),
            ..
        } => (code.clone(), lang.clone()),
        _ => return,
    };

    let (parsed_lang, overrides) = parse_info_string(&lang);
    let Some(renderer) = find_renderer(&parsed_lang) else {
        return;
    };

    let mut opts = RenderOpts::for_content_width(settings.content_width() as f64, "light");
    opts.apply_overrides(&overrides);

    match renderer.render(&code, &opts) {
        Ok(img) => {
            *node = image_node(img, &parsed_lang, &node.style, settings);
        }
        Err(e) => {
            // 软降级：错误注释进代码块，后续高亮/排版照常显示，便于定位问题。
            if let NodeKind::CodeBlock { code, .. } = &mut node.kind {
                *code = format!("// render failed ({lang}): {e}\n{code}");
            }
        }
    }
}

/// 用渲染结果构造图片节点。
///
/// 宽度显式设为内容宽（pt），让 DOCX/HTML 与 PDF 的显示宽度一致；高度留空，
/// 由各后端按 PNG 真实宽高比推算（PDF 侧还会按页高上限等比缩放，避免长图溢出）。
fn image_node(img: RenderedImage, lang: &str, base_style: &Style, settings: &PageSettings) -> Node {
    let mut style = base_style.clone();
    style.width = Some(settings.content_width());
    style.height = None;
    // 图表默认居中更合理。
    style.text_align = TextAlign::Center;

    let mime = match img.format.as_str() {
        "svg" => "image/svg+xml",
        "jpeg" | "jpg" => "image/jpeg",
        _ => "image/png",
    };
    let src = format!("data:{mime};base64,{}", base64_encode(&img.data));

    Node::new(
        NodeKind::Image {
            src,
            alt: lang.to_string(),
            title: None,
        },
        style,
        // 图片不参与跨页拆分。
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parse_markdown;

    fn settings() -> PageSettings {
        PageSettings::default()
    }

    /// 统计树中指定变体的节点数。
    fn count<F: Fn(&NodeKind) -> bool>(node: &Node, pred: F) -> usize {
        let mut n = 0;
        crate::ast::walk(node, &mut |node| {
            if pred(&node.kind) {
                n += 1;
            }
        });
        n
    }

    #[cfg(feature = "mermaid")]
    #[test]
    fn mermaid_block_becomes_image() {
        let md = "```mermaid\nflowchart TD\n  A --> B\n```\n";
        let mut node = parse_markdown(md).unwrap();
        render_ext_blocks(&mut node, &settings());

        assert_eq!(
            count(&node, |k| matches!(k, NodeKind::Image { .. })),
            1,
            "mermaid 代码块应被替换为图片节点"
        );
        assert_eq!(count(&node, |k| matches!(k, NodeKind::CodeBlock { .. })), 0);
    }

    #[cfg(feature = "charts")]
    #[test]
    fn liecharts_block_becomes_image() {
        let md = "```liecharts\n{\"xAxis\":[{\"type\":\"category\",\"data\":[\"a\"]}],\"yAxis\":[{\"type\":\"value\"}],\"series\":[{\"type\":\"bar\",\"data\":[1]}]}\n```\n";
        let mut node = parse_markdown(md).unwrap();
        render_ext_blocks(&mut node, &settings());

        let mut src = String::new();
        crate::ast::walk(&node, &mut |n| {
            if let NodeKind::Image { src: s, .. } = &n.kind {
                src = s.clone();
            }
        });
        assert!(
            src.starts_with("data:image/png;base64,"),
            "应产出 PNG data URI"
        );
    }

    #[cfg(feature = "charts")]
    #[test]
    fn invalid_chart_degrades_to_code_block() {
        let md = "```liecharts\n{ not valid json\n```\n";
        let mut node = parse_markdown(md).unwrap();
        render_ext_blocks(&mut node, &settings());

        assert_eq!(count(&node, |k| matches!(k, NodeKind::Image { .. })), 0);
        let mut code = String::new();
        crate::ast::walk(&node, &mut |n| {
            if let NodeKind::CodeBlock { code: c, .. } = &n.kind {
                code = c.clone();
            }
        });
        assert!(
            code.contains("render failed"),
            "失败应保留带错误注释的代码块"
        );
    }

    #[test]
    fn ordinary_code_block_untouched() {
        let md = "```rust\nfn main() {}\n```\n";
        let mut node = parse_markdown(md).unwrap();
        render_ext_blocks(&mut node, &settings());

        assert_eq!(count(&node, |k| matches!(k, NodeKind::Image { .. })), 0);
        assert_eq!(count(&node, |k| matches!(k, NodeKind::CodeBlock { .. })), 1);
    }
}
