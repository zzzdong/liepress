//! PNG 输出后端：将 [`Document`](crate::document::layout::Document) 渲染为 PNG 字节。
//!
//! 管线：先经 [`crate::document::to_scene::document_to_scene`] 统一转换为
//! `lievisual::Scene`，再委托 lievisual 的 `VelloPixmapRenderer` 光栅化。
//!
//! 画布尺寸参考 PDF：宽度取整页宽（含左右边距），高度取内容高 + 上下边距。
//! 坐标原点为页面左上角，内容从 `(margin_left, margin_top)` 开始绘制。
//! `dpi` 控制分辨率（默认 96）；72 时 1pt = 1px。
//!
//! ## 生成器模式
//!
//! 与 [`crate::output::PdfGenerator`] / [`crate::output::DocxGenerator`]
//! 一致：页面设置与 DPI 在构造时注入，`generate` 消费布局文档产出 PNG 字节。

use crate::document::layout::Document;
use crate::document::types::page::PageSettings;

/// PNG 生成器：持有页面设置与分辨率，消费布局文档产出 PNG 字节。
pub struct PngGenerator {
    settings: PageSettings,
    dpi: f32,
}

impl PngGenerator {
    /// 从页面设置与 DPI 构造生成器。
    pub fn new(settings: &PageSettings, dpi: f32) -> Self {
        Self {
            settings: settings.clone(),
            dpi,
        }
    }

    /// 生成 PNG 字节。
    pub fn generate(&mut self, document: &Document) -> crate::error::Result<Vec<u8>> {
        let scene =
            crate::document::to_scene::document_to_scene(document, &self.settings, self.dpi as f64);

        // VelloPixmapRenderer 内部以 u16 存储画布尺寸，需截断避免溢出。
        let pw = (scene.width.round() as u64).min(u16::MAX as u64) as u32;
        let ph = (scene.height.round() as u64).min(u16::MAX as u64) as u32;

        let mut r = lievisual::render::VelloPixmapRenderer::new(pw.max(1), ph.max(1))
            .with_background(lievisual::geometry::Color::WHITE);
        let png = r.render_png(&scene);

        Ok(png)
    }
}

/// 将布局文档渲染为 PNG 字节。
///
/// 便捷入口：等价于 `PngGenerator::new(settings, dpi).generate(document)`。
pub fn document_to_png(
    document: &Document,
    settings: &PageSettings,
    dpi: f32,
) -> crate::error::Result<Vec<u8>> {
    PngGenerator::new(settings, dpi).generate(document)
}
