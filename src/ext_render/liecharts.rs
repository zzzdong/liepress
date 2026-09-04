//! liecharts 渲染器：把 ` ```liecharts ` 代码块（echarts 风格 JSON 配置）渲染为 PNG。

use crate::ext_render::{BlockRenderer, RenderError, RenderOpts, RenderedImage};

/// 把 JSON 配置交给 [`liecharts`] 渲染为 PNG 的渲染器。
pub struct LieChartsRenderer;

impl BlockRenderer for LieChartsRenderer {
    fn lang(&self) -> &'static str {
        "liecharts"
    }

    fn render(&self, code: &str, opts: &RenderOpts) -> Result<RenderedImage, RenderError> {
        let builder = liecharts::ChartBuilder::from_option_json(code)
            .map_err(|e| RenderError::Parse(e.to_string()))?;
        let png = builder
            .render_png(opts.width, opts.height)
            .map_err(|e| RenderError::Render(e.to_string()))?;
        if png.is_empty() {
            return Err(RenderError::Render("liecharts 返回空图片".into()));
        }
        Ok(RenderedImage {
            data: png,
            format: "png".into(),
            pixel_size: (opts.width, opts.height),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "charts")]
    #[test]
    fn renders_valid_json_to_png() {
        let code = r#"{
            "title": { "text": "t" },
            "xAxis": [{ "type": "category", "data": ["a", "b"] }],
            "yAxis": [{ "type": "value" }],
            "series": [{ "type": "bar", "data": [1, 2] }]
        }"#;
        let r = LieChartsRenderer;
        let img = r
            .render(code, &RenderOpts::default())
            .expect("liecharts 渲染应成功");
        assert_eq!(img.format, "png");
        assert!(!img.data.is_empty());
        // PNG 文件头：89 50 4E 47
        assert_eq!(&img.data[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[cfg(feature = "charts")]
    #[test]
    fn invalid_json_degrades_to_error() {
        let r = LieChartsRenderer;
        let err = r.render("{ not valid json", &RenderOpts::default());
        assert!(matches!(err, Err(RenderError::Parse(_))));
    }
}
