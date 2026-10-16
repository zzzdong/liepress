//! liemermaid 渲染器：把 ` ```mermaid ` 代码块（Mermaid DSL 文本）渲染为 PNG。

use crate::ext_render::{BlockRenderer, RenderError, RenderOpts, RenderedImage};

/// 把 Mermaid DSL 文本交给 [`liemermaid`] 渲染为 PNG 的渲染器。
pub struct LieMermaidRenderer;

impl BlockRenderer for LieMermaidRenderer {
    fn lang(&self) -> &'static str {
        "mermaid"
    }

    fn render(&self, code: &str, opts: &RenderOpts) -> Result<RenderedImage, RenderError> {
        // 超采样：mermaid 的 `width` 是「目标宽度」，`fit_scene` 会把整张图
        // （几何 + 文字，包进 Group 变换）等比缩放到该宽度后再按 1pt = 1px
        // 光栅化，因此把目标宽度乘上超采样倍率即得到 N 倍像素的真超采样图，
        // 插入文档后版式不变、有效 DPI 翻倍。
        // `height` 设为 `None`（不限高），由内容决定，避免竖长图（如 flowchart TD）
        // 被固定高度限制放大倍数。
        let scale = opts.scale_factor();
        let width = opts.width.saturating_mul(scale);
        let config = liemermaid::OutputConfig {
            width: Some(width as f64),
            height: None,
            upscale: true,
            ..Default::default()
        };
        let png = liemermaid::render_png_with_config(code, &config).map_err(map_diagram_error)?;
        if png.is_empty() {
            return Err(RenderError::Render("liemermaid 返回空图片".into()));
        }
        Ok(RenderedImage {
            data: png,
            format: "png".into(),
            // 真实尺寸由 from_ast 用 PNG 解码尺寸覆盖，此处仅作回退。
            pixel_size: (width, opts.height.saturating_mul(scale)),
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

    /// 读取 PNG IHDR 宽度（测试专用）。
    fn png_width(png: &[u8]) -> u32 {
        assert_eq!(&png[12..16], b"IHDR", "内嵌数据应为 PNG");
        u32::from_be_bytes([png[16], png[17], png[18], png[19]])
    }

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
        // 默认 2× 超采样：720 标称目标宽 → 1440px（fit 后 PNG 宽恰为目标宽）。
        assert_eq!(png_width(&img.data), 1440);
    }

    #[cfg(feature = "mermaid")]
    #[test]
    fn scale_one_renders_at_nominal_width() {
        let code = "flowchart TD\n  A --> B\n";
        let opts = RenderOpts {
            scale: 1,
            ..Default::default()
        };
        let img = LieMermaidRenderer
            .render(code, &opts)
            .expect("liemermaid 渲染应成功");
        assert_eq!(png_width(&img.data), 720, "scale=1 应按标称目标宽光栅化");
    }

    #[cfg(feature = "mermaid")]
    #[test]
    fn invalid_mermaid_degrades_to_error() {
        let r = LieMermaidRenderer;
        let err = r.render("this is not a diagram", &RenderOpts::default());
        assert!(matches!(err, Err(RenderError::Parse(_))));
    }
}
