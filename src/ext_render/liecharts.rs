//! liecharts 渲染器：把 ` ```liecharts ` 代码块（echarts 风格 JSON 配置）渲染为 PNG。

use crate::ext_render::{BlockRenderer, RenderError, RenderOpts, RenderedImage};

/// 把 JSON 配置交给 [`liecharts`] 渲染为 PNG 的渲染器。
pub struct LieChartsRenderer;

impl BlockRenderer for LieChartsRenderer {
    fn lang(&self) -> &'static str {
        "liecharts"
    }

    fn render(&self, code: &str, opts: &RenderOpts) -> Result<RenderedImage, RenderError> {
        let chart = liecharts::ChartBuilder::from_option_json(code)
            .map_err(|e| RenderError::Parse(e.to_string()))?
            .build(opts.width, opts.height)
            .map_err(|e| RenderError::Render(e.to_string()))?;
        let elements = chart
            .collect_visual_elements()
            .map_err(|e| RenderError::Render(e.to_string()))?;

        // 超采样：图表必须按**标称像素尺寸**排版——echarts 字号是画布内的绝对 px，
        // 直接放大画布会把文字相对缩小、改变版式。因此把 Scene 交给 lievisual 的
        // 密度旋钮 `scene.scale`（viewBox = 输出尺寸 / scale，再 ×scale 光栅化），
        // 以 `scale` 倍像素光栅化：几何与文字都在最终变换下绘制（vello 的
        // `glyph_transform`），是真正的 N 倍超采样而非位图插值，插入文档后
        // 版式不变、有效 DPI 翻倍。
        let (rw, rh) = opts.raster_size();
        let scale = opts.scale_factor();
        let mut scene = liecharts::render::to_scene(&elements, opts.width, opts.height);
        scene.scale = f64::from(scale);
        let mut renderer = lievisual::render::VelloPixmapRenderer::new(rw, rh);
        let png = renderer.render_png(&scene);
        if png.is_empty() {
            return Err(RenderError::Render("liecharts 返回空图片".into()));
        }
        Ok(RenderedImage {
            data: png,
            format: "png".into(),
            pixel_size: (rw, rh),
        })
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
        // 默认 2× 超采样：720×420 标称 → 1440×840 像素（版式不变、密度翻倍）。
        assert_eq!(png_width(&img.data), 1440);
        assert_eq!(img.pixel_size, (1440, 840));
    }

    #[cfg(feature = "charts")]
    #[test]
    fn scale_one_renders_at_nominal_size() {
        let code = r#"{
            "xAxis": [{ "type": "category", "data": ["a"] }],
            "yAxis": [{ "type": "value" }],
            "series": [{ "type": "bar", "data": [1] }]
        }"#;
        let opts = RenderOpts {
            scale: 1,
            ..Default::default()
        };
        let img = LieChartsRenderer
            .render(code, &opts)
            .expect("liecharts 渲染应成功");
        assert_eq!(png_width(&img.data), 720, "scale=1 应按标称宽光栅化");
        assert_eq!(img.pixel_size, (720, 420));
    }

    #[cfg(feature = "charts")]
    #[test]
    fn invalid_json_degrades_to_error() {
        let r = LieChartsRenderer;
        let err = r.render("{ not valid json", &RenderOpts::default());
        assert!(matches!(err, Err(RenderError::Parse(_))));
    }
}
