//! 资源解析器：统一图片/资源加载。
//!
//! 将所有输出后端（PDF/SVG/PNG/DOCX）的图片内嵌逻辑集中到此。当前支持：
//! - **本地路径**：`src="chart_1.png"` → 用 `base_dir` 解析 → 读文件 → base64 data URI。
//! - **base64 data URI**：`src="data:image/...;base64,..."` → 原样保留（已内联）。
//! - **绝对路径**：默认**拒绝**（S-1 路径限制），可信场景可用
//!   [`ResourceResolver::with_allow_absolute_paths`] 显式开启。
//!
//! 网络下载（http/https）暂不支持，返回 [`ResolvedResource::Unchanged`] 保持原样。
//!
//! ## 安全（S-1）
//!
//! 本地图片读取施加三重限制（见 [`is_absolute_src`] / [`has_parent_component`] /
//! [`has_image_extension`]）：拒绝目录穿越（`..`）、默认拒绝绝对路径
//!（Unix `/…`、Windows 盘符 `C:\…`、UNC `\\…`）、仅允许图片扩展名白名单。
//! 服务端拼接不可信输入前应保持默认配置。

use std::path::{Path, PathBuf};

use super::{HtmlDocument, HtmlElement, HtmlNode, HtmlTag};

/// 本地图片扩展名白名单（小写）。
const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "avif",
];

/// 判断路径扩展名是否在图片白名单内。
pub fn has_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.as_str()))
}

/// 判断 `src` 是否包含 `..` 组件（目录穿越）。
pub fn has_parent_component(src: &str) -> bool {
    Path::new(src)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// 判断 `src` 是否为绝对路径（跨平台：覆盖 Unix `/…`、Windows 盘符 `C:\…`、UNC `\\…`）。
///
/// Windows 盘符/UNC 形式在非 Windows 平台上 `Path::is_absolute()` 返回 `false`，
/// 故按字面前缀额外判定。
pub fn is_absolute_src(src: &str) -> bool {
    if Path::new(src).is_absolute() {
        return true;
    }
    let bytes = src.as_bytes();
    // UNC：\\server\share
    if src.starts_with("\\\\") {
        return true;
    }
    // 盘符：C:\ 或 C:/
    bytes.len() >= 3 && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/')
}

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
///
/// 默认安全策略（S-1）：拒绝绝对路径与 `..` 目录穿越，仅读取图片扩展名白名单内的
/// 文件。处理**可信本机文档**且确需绝对路径图片时，用
/// [`ResourceResolver::with_allow_absolute_paths`] 显式放开绝对路径限制。
pub struct ResourceResolver {
    /// 本地相对路径的解析基准目录。
    base_dir: Option<PathBuf>,
    /// 是否允许绝对路径图片 src（默认 `false`）。
    allow_absolute_paths: bool,
}

impl ResourceResolver {
    /// 新建解析器。`base_dir` 为相对图片路径的基准目录（如 markdown 文件所在目录）。
    pub fn new(base_dir: Option<PathBuf>) -> Self {
        ResourceResolver {
            base_dir,
            allow_absolute_paths: false,
        }
    }

    /// 设置是否允许绝对路径图片 src（默认拒绝）。
    pub fn with_allow_absolute_paths(mut self, allow: bool) -> Self {
        self.allow_absolute_paths = allow;
        self
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

    /// 读取本地图片并编码为 data URI。失败（路径被拒/文件不存在/读取错误）返回 `None`。
    fn load_local(&self, src: &str) -> Option<String> {
        // S-1 路径限制：目录穿越 / 绝对路径（默认拒绝）/ 图片扩展名白名单。
        if has_parent_component(src) || !has_image_extension(Path::new(src)) {
            return None;
        }
        if is_absolute_src(src) && !self.allow_absolute_paths {
            return None;
        }
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
