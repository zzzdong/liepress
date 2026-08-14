//! 资源解析器：统一图片/资源加载。
//!
//! 将所有输出后端（PDF/SVG/PNG/DOCX）的图片内嵌逻辑集中到此。当前支持：
//! - **本地路径**：`src="chart_1.png"` → 用 `base_dir` 解析 → 读文件 → base64 data URI。
//! - **base64 data URI**：`src="data:image/...;base64,..."` → 原样保留（已内联）。
//! - **绝对路径**：直接读文件 → data URI。
//!
//! 网络下载（http/https）暂不支持，返回 [`ResolvedResource::Unchanged`] 保持原样。

use std::path::{Path, PathBuf};

use super::{HtmlDocument, HtmlElement, HtmlNode, HtmlTag};

/// 遍历 `HtmlDocument`，将所有图片 `img[src]` 用 `resolver` 内嵌为 data URI。
pub fn embed_images(doc: &mut HtmlDocument, resolver: &ResourceResolver) {
    embed_in_element(&mut doc.root, resolver);
}

fn embed_in_element(el: &mut HtmlElement, resolver: &ResourceResolver) {
    if el.tag == HtmlTag::Img
        && let Some(src) = el.attrs.get("src")
        && let super::resource::ResolvedResource::DataUri(data_uri) = resolver.resolve_image(src)
    {
        el.attrs.insert("src".to_string(), data_uri);
    }
    for child in &mut el.children {
        if let HtmlNode::Element(c) = child {
            embed_in_element(c, resolver);
        }
    }
}

/// 图片 `src` 解析结果。
pub enum ResolvedResource {
    /// data URI 字符串，可直接写入 `img[src]`。
    DataUri(String),
    /// 无需/无法处理（网络 URL、协议相对、读取失败），保留原样。
    Unchanged,
}

/// 资源解析器。
///
/// 持有 `base_dir`（相对路径的解析基准），对外提供 `resolve_image`。
/// 后续可扩展网络下载（在 resolver 内加入 HTTP 客户端）。
pub struct ResourceResolver {
    /// 本地相对路径的解析基准目录。
    base_dir: Option<PathBuf>,
}

impl ResourceResolver {
    /// 新建解析器。`base_dir` 为相对图片路径的基准目录（如 markdown 文件所在目录）。
    pub fn new(base_dir: Option<PathBuf>) -> Self {
        ResourceResolver { base_dir }
    }

    /// 解析图片 `src` 为可内联的 data URI。
    pub fn resolve_image(&self, src: &str) -> ResolvedResource {
        // 已是 data URI：原样保留
        if src.starts_with("data:") {
            return ResolvedResource::DataUri(src.to_string());
        }
        // 网络 URL / 协议相对 / 锚点：不处理
        if src.starts_with("http://")
            || src.starts_with("https://")
            || src.starts_with("//")
            || src.starts_with("#")
        {
            return ResolvedResource::Unchanged;
        }
        match self.load_local(src) {
            Some(data_uri) => ResolvedResource::DataUri(data_uri),
            None => ResolvedResource::Unchanged,
        }
    }

    /// 读取本地图片并编码为 data URI。失败（文件不存在/读取错误）返回 `None`。
    fn load_local(&self, src: &str) -> Option<String> {
        let path: PathBuf = if let Some(base) = &self.base_dir {
            base.join(src)
        } else {
            PathBuf::from(src)
        };
        let bytes = std::fs::read(&path).ok()?;
        let mime = mime_for_path(&path);
        Some(format!("data:{};base64,{}", mime, base64_encode(&bytes)))
    }
}

/// 根据文件扩展名推断 MIME 类型。
pub fn mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    }
}

/// 标准 base64 编码。
pub fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    STANDARD.encode(bytes)
}
