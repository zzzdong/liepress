//! PNG 输出后端：将 [`Document`](crate::document::layout::Document) 渲染为 PNG 字节。
//!
//! 管线：先经 [`crate::document::to_scene::document_to_scene`] 统一转换为
//! `lievisual::Scene`，再委托 lievisual 的 `VelloPixmapRenderer` 光栅化。
//! 由此移除原先手写的 vello_cpu 字形 / 图片绘制逻辑，所有图元由 lievisual 负责。
//!
//! 画布尺寸参考 PDF：宽度取整页宽（含左右边距），高度取内容高 + 上下边距。
//! 坐标原点为页面左上角，内容从 `(margin_left, margin_top)` 开始绘制。
/// `dpi` 控制分辨率（默认 96）；72 时 1pt = 1px。
pub fn document_to_png(
    document: &crate::document::layout::Document,
    settings: &crate::document::types::page::PageSettings,
    dpi: f32,
) -> crate::error::Result<Vec<u8>> {
    let scene = crate::document::to_scene::document_to_scene(document, settings, dpi as f64);

    // VelloPixmapRenderer 内部以 u16 存储画布尺寸，需截断避免溢出。
    let pw = (scene.width.round() as u64).min(u16::MAX as u64) as u32;
    let ph = (scene.height.round() as u64).min(u16::MAX as u64) as u32;

    let mut r = lievisual::render::VelloPixmapRenderer::new(pw.max(1), ph.max(1))
        .with_background(lievisual::geometry::Color::WHITE);
    let png = r.render_png(&scene);

    Ok(png)
}
