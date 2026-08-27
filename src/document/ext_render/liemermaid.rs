//! liemermaid 渲染器：把 ` ```mermaid ` 代码块（Mermaid DSL 文本）渲染为 PNG。

use crate::document::ext_render::{BlockRenderer, RenderError, RenderOpts, RenderedImage};

/// 把 Mermaid DSL 文本交给 [`liemermaid`] 渲染为 PNG 的渲染器。
pub struct LieMermaidRenderer;

impl BlockRenderer for LieMermaidRenderer {
    fn lang(&self) -> &'static str {
        "mermaid"
    }

    fn render(&self, code: &str, opts: &RenderOpts) -> Result<RenderedImage, RenderError> {
        let png =
            liemermaid::render_png(code, opts.width, opts.height).map_err(map_diagram_error)?;
        if png.is_empty() {
            return Err(RenderError::Render("liemermaid 返回空图片".into()));
        }
        Ok(RenderedImage {
            data: png,
            format: "png".into(),
            pixel_size: (opts.width, opts.height),
        })
    }
}

/// 把 [`liemermaid::error::DiagramError`] 映射为 [`RenderError`]：
/// 解析/不支持的类型视为 Parse，其余布局/字体/渲染错误视为 Render。
fn map_diagram_error(e: liemermaid::error::DiagramError) -> RenderError {
    match e {
        liemermaid::error::DiagramError::Parse(_)
        | liemermaid::error::DiagramError::UnsupportedType(_) => RenderError::Parse(e.to_string()),
        _ => RenderError::Render(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "mermaid")]
    #[test]
    fn renders_valid_mermaid_to_png() {
        let code = r#"flowchart TD
    A[Start]
    B[End]
    A --> B
"#;
        let r = LieMermaidRenderer;
        let img = r
            .render(code, &RenderOpts::default())
            .expect("liemermaid 渲染应成功");
        assert_eq!(img.format, "png");
        assert!(!img.data.is_empty());
        // PNG 文件头：89 50 4E 47
        assert_eq!(&img.data[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[cfg(feature = "mermaid")]
    #[test]
    fn invalid_mermaid_degrades_to_error() {
        let r = LieMermaidRenderer;
        let err = r.render("this is not a diagram", &RenderOpts::default());
        assert!(matches!(err, Err(RenderError::Parse(_))));
    }
}
