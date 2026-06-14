//! 图片加载
//!
//! 仅处理 data URI（base64 嵌入）格式的图片。
//! 本地图片文件在 markdown→HTML 阶段已通过 `embed_local_images` 转换为 data URI。

use base64::Engine;

/// 图片加载结果
pub(crate) struct ImageLoadResult {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub dpi: Option<(f32, f32)>,
    pub format: image::ImageFormat,
}

/// 加载图片
///
/// 仅支持 data URI（`data:image/...;base64,...`）格式。
/// 其它 URL 返回 `None`（布局阶段会显示占位符）。
pub(crate) fn load_image(url: &str) -> Option<ImageLoadResult> {
    let data_uri = url.strip_prefix("data:")?;
    load_image_from_data_uri(data_uri)
}

// ─── Data URI ──────────────────────────────────────────────────

/// 从 data URI 加载图片（base64 嵌入）
fn load_image_from_data_uri(data_uri: &str) -> Option<ImageLoadResult> {
    let (mime, data) = data_uri.split_once(',')?;
    let is_base64 = mime.contains(";base64");

    let decoded = if is_base64 {
        let trimmed = data.trim();
        base64::engine::general_purpose::STANDARD
            .decode(trimmed)
            .ok()?
    } else {
        return None;
    };

    let format = mime_to_image_format(mime.split(';').next().unwrap_or(""))?;
    let img = image::load_from_memory(&decoded).ok()?;
    let (width, height) = (img.width(), img.height());

    let (output_data, output_format) = if is_pdf_supported_format(format) {
        (decoded, format)
    } else {
        let mut png_data = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png_data),
            image::ImageFormat::Png,
        )
        .ok()?;
        (png_data, image::ImageFormat::Png)
    };

    Some(ImageLoadResult {
        width,
        height,
        data: output_data,
        dpi: None,
        format: output_format,
    })
}

// ─── 格式判断 ──────────────────────────────────────────────────

/// 判断图片格式是否被 PDF 渲染器原生支持
fn is_pdf_supported_format(format: image::ImageFormat) -> bool {
    matches!(
        format,
        image::ImageFormat::Png
            | image::ImageFormat::Jpeg
            | image::ImageFormat::Gif
            | image::ImageFormat::WebP
    )
}

/// 从 MIME 类型推断 image 格式
fn mime_to_image_format(mime: &str) -> Option<image::ImageFormat> {
    match mime {
        "image/png" => Some(image::ImageFormat::Png),
        "image/jpeg" | "image/jpg" => Some(image::ImageFormat::Jpeg),
        "image/gif" => Some(image::ImageFormat::Gif),
        "image/webp" => Some(image::ImageFormat::WebP),
        "image/bmp" => Some(image::ImageFormat::Bmp),
        "image/x-icon" => Some(image::ImageFormat::Ico),
        "image/svg+xml" => None,
        _ => None,
    }
}

/// 将 image::ImageFormat 转换为字符串标识符
pub(crate) fn format_to_string(format: image::ImageFormat) -> String {
    match format {
        image::ImageFormat::Png => "png".to_string(),
        image::ImageFormat::Jpeg => "jpeg".to_string(),
        image::ImageFormat::Gif => "gif".to_string(),
        image::ImageFormat::WebP => "webp".to_string(),
        image::ImageFormat::Bmp => "bmp".to_string(),
        _ => "png".to_string(),
    }
}
