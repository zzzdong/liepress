//! SVG 输出后端：将 [`Document`](crate::document::layout::Document) 渲染为 SVG 字符串。
//!
//! 管线：先经 [`crate::document::to_scene::document_to_scene`] 统一转换为
//! `lievisual::Scene`，再委托 lievisual 的 `SvgRenderer` 序列化。由此移除原先手写的
//! `<rect>`/`<text>`/`<image>` 绘制逻辑，所有图元由 lievisual 负责。
//!
//! 画布尺寸参考 PDF：宽度取整页宽（含左右边距），高度取内容高 + 上下边距。
//! 坐标原点为页面左上角，内容从 `(margin_left, margin_top)` 开始绘制。
//!
//! ## 生成器模式
//!
//! 与 [`crate::output::PdfGenerator`] / [`crate::output::DocxGenerator`]
//! 一致：页面设置在构造时注入，`generate` 消费布局文档产出 SVG 字符串。

use crate::document::layout::Document;
use crate::document::types::page::PageSettings;

/// SVG 生成器：持有页面设置，消费布局文档产出 SVG。
pub struct SvgGenerator {
    settings: PageSettings,
}

impl SvgGenerator {
    /// 从页面设置构造生成器。
    pub fn new(settings: &PageSettings) -> Self {
        Self {
            settings: settings.clone(),
        }
    }

    /// 生成 SVG 字符串。
    pub fn generate(&mut self, document: &Document) -> String {
        let scene = crate::document::to_scene::document_to_scene(document, &self.settings, 72.0);

        let mut r = lievisual::render::SvgRenderer::new(scene.width, scene.height)
            .with_background(lievisual::geometry::Color::WHITE);
        use lievisual::render::Renderer;
        r.render_scene(&scene);
        r.into_string()
    }
}

/// 将布局文档渲染为 SVG 字符串。
///
/// 便捷入口：等价于 `SvgGenerator::new(settings).generate(document)`。
pub fn document_to_svg(document: &Document, settings: &PageSettings) -> String {
    SvgGenerator::new(settings).generate(document)
}
