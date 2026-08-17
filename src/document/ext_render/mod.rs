//! 可插拔「代码块 → 图片」渲染器。
//!
//! 用于把特定语言的标记代码块（如 ` ```liecharts `、` ```mermaid `）渲染成图片，
//! 再作为 [`crate::document::types::DocImage`] 嵌入文档。
//!
//! 设计目标：核心只定义契约（[`BlockRenderer`]），具体渲染引擎可插拔、可缺省。
//! 新增一种绘图语言（mermaid / vega-lite / plantuml 等）只需：
//! 1. 实现 [`BlockRenderer`]（放在本模块子文件）；
//! 2. 在 [`builtin_renderers`] 注册表中登记（通常按 feature 门控）。
//! 主转换流程（[`crate::document::from_ast`]）不感知具体引擎。
//!
//! 这与主流文档工具的做法一致：图表引擎作为独立组件，通过统一契约接入。

#[cfg(feature = "charts")]
pub mod liecharts;

use std::collections::HashMap;

/// 渲染器接收的选项。
#[derive(Debug, Clone)]
pub struct RenderOpts {
    /// 建议像素宽。默认由调用方按页内容宽折算（见 [`default_opts`]）。
    pub width: u32,
    /// 建议像素高。默认按 7:12 比例（贴近常见图表 4:3 习惯）。
    pub height: u32,
    /// 主题名（"light" / "dark" / 引擎自定义）。渲染器不认识时忽略。
    pub theme: String,
    /// 渲染 DPI（用于像素 ↔ 排版单位换算）。默认 150。
    pub dpi: u32,
}

impl Default for RenderOpts {
    fn default() -> Self {
        Self {
            width: 720,
            height: 420,
            theme: "light".to_string(),
            dpi: 150,
        }
    }
}

impl RenderOpts {
    /// 按页内容宽（pt）与主题构造默认选项，并将 info-string 中的覆盖项应用进来。
    pub fn for_content_width(content_width_pt: f64, theme: &str) -> Self {
        let width = (content_width_pt.max(0.0)).round() as u32;
        let width = width.min(1440).max(240);
        Self {
            width,
            height: (width as f64 * 7.0 / 12.0).round() as u32,
            theme: theme.to_string(),
            dpi: 150,
        }
    }

    /// 用 info-string 的 `key=value` 覆盖项（width/height/theme/dpi）更新自身。
    pub fn apply_overrides(&mut self, overrides: &HashMap<String, String>) {
        if let Some(w) = overrides.get("width").and_then(|v| v.parse::<u32>().ok()) {
            if w > 0 {
                self.width = w.min(4000);
            }
        }
        if let Some(h) = overrides.get("height").and_then(|v| v.parse::<u32>().ok()) {
            if h > 0 {
                self.height = h.min(4000);
            }
        }
        if let Some(t) = overrides.get("theme") {
            if !t.is_empty() {
                self.theme = t.clone();
            }
        }
        if let Some(d) = overrides.get("dpi").and_then(|v| v.parse::<u32>().ok()) {
            if d > 0 {
                self.dpi = d.min(600);
            }
        }
    }
}

/// 渲染器产出的图片（自包含字节，与 [`crate::document::types::DocImage`] 同构）。
#[derive(Debug, Clone)]
pub struct RenderedImage {
    pub data: Vec<u8>,
    /// 图片格式标识，如 "png" / "svg"。
    pub format: String,
    /// 像素尺寸（宽, 高）。
    pub pixel_size: (u32, u32),
}

/// 渲染错误。转换流程据此软降级（见 [`crate::document::from_ast`]）。
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("渲染器 {0} 不支持的语言")]
    Unsupported(String),
    #[error("解析失败: {0}")]
    Parse(String),
    #[error("渲染失败: {0}")]
    Render(String),
    #[error("IO 错误: {0}")]
    Io(String),
}

/// 代码块渲染器统一契约。
///
/// 实现方认领一种 `lang`（见 [`BlockRenderer::lang`]），把代码块文本渲染成图片。
pub trait BlockRenderer: Send + Sync {
    /// 认领的代码块语言标识（如 "liecharts"、"mermaid"）。大小写敏感。
    fn lang(&self) -> &'static str;

    /// 把代码块 `code` 渲染成图片。失败返回 [`RenderError`]，由上层降级处理。
    fn render(&self, code: &str, opts: &RenderOpts) -> Result<RenderedImage, RenderError>;
}

/// 全局内置渲染器注册表。
///
/// 默认空（不开任何 feature 时，代码块不会走图片渲染）。
/// 开启对应 feature 时登记相应渲染器，主流程无需变动。
#[allow(unused_mut)]
pub fn builtin_renderers() -> Vec<Box<dyn BlockRenderer>> {
    let mut v: Vec<Box<dyn BlockRenderer>> = Vec::new();
    #[cfg(feature = "charts")]
    v.push(Box::new(crate::document::ext_render::liecharts::LieChartsRenderer));
    v
}

/// 按 `lang` 查找已注册的渲染器。
pub fn find_renderer(lang: &str) -> Option<Box<dyn BlockRenderer>> {
    builtin_renderers()
        .into_iter()
        .find(|r| r.lang() == lang)
}

/// 解析代码块 info string 为 `(lang, overrides)`。
///
/// info string 形如 `"liecharts width=900 height=500 theme=dark"`：
/// 第一段为 lang，其余 `key=value` 段收集为覆盖项。
pub fn parse_info_string(info: &str) -> (String, HashMap<String, String>) {
    let mut parts = info.split_whitespace();
    let lang = parts.next().unwrap_or("").to_string();
    let mut overrides = HashMap::new();
    for p in parts {
        if let Some((k, v)) = p.split_once('=') {
            let k = k.trim();
            let v = v.trim();
            if !k.is_empty() && !v.is_empty() {
                overrides.insert(k.to_lowercase(), v.to_string());
            }
        }
    }
    (lang, overrides)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_info_string_basic() {
        let (lang, ov) = parse_info_string("liecharts width=900 height=500 theme=dark");
        assert_eq!(lang, "liecharts");
        assert_eq!(ov.get("width").unwrap(), "900");
        assert_eq!(ov.get("height").unwrap(), "500");
        assert_eq!(ov.get("theme").unwrap(), "dark");
    }

    #[test]
    fn parse_info_string_lang_only() {
        let (lang, ov) = parse_info_string("mermaid");
        assert_eq!(lang, "mermaid");
        assert!(ov.is_empty());
    }

    #[test]
    fn render_opts_apply_overrides() {
        let mut o = RenderOpts::default();
        let mut ov = HashMap::new();
        ov.insert("width".into(), "1000".into());
        ov.insert("theme".into(), "dark".into());
        o.apply_overrides(&ov);
        assert_eq!(o.width, 1000);
        assert_eq!(o.theme, "dark");
    }
}
