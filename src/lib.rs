pub mod ast;
pub mod css;
pub mod document; // 重构文档层（方案 refactor-document-layer.md）
pub mod dom; // HTML AST（管线 Layer 1）：Markdown/HTML → HtmlDocument
pub mod error;
pub mod output; // 输出层：语义树/HTML → PDF/HTML 等目标格式

use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub use ast::PageConfig;
pub use document::from_ast::ast_to_layout;
pub use document::layout::Document;
pub use document::types::page::PageSettings;
pub use dom::md_converter::{embed_local_images, markdown_to_html, markdown_to_html_document};
pub use dom::parse_html;
pub use output::html::node_to_html;
pub use output::pdf::PdfDocumentGenerator;

/// Markdown 转换配置
///
/// 控制样式、字体、严格模式等选项。通过 builder 风格方法快速构造：
///
/// ```
/// use liepress::ConvertOptions;
///
/// let opts = ConvertOptions::new()
///     .with_font_family(&["Noto Sans CJK SC", "sans-serif"])
///     .with_css("h1 { color: red; }")
///     .with_strict(true);
/// ```
#[derive(Debug, Clone)]
pub struct ConvertOptions {
    /// 全局默认字体家族列表（优先级从高到低）
    ///
    /// 设置后会自动生成 `body { font-family: ... }` 样式，
    /// 通过 CSS 继承机制应用到所有元素。如果同时提供了 `user_css`
    /// 或 `css_file`，其中的 `body { font-family }` 会覆盖此设置。
    pub font_family: Vec<String>,
    /// 用户提供的 CSS 样式字符串（叠加在默认样式之上）
    pub user_css: String,
    /// 用户提供的 CSS 样式文件路径（叠加在默认样式之上）
    /// 如果与 `user_css` 同时设置，两者会合并
    pub css_file: Option<PathBuf>,
    /// 严格模式：CSS 解析失败时返回错误（默认 false）
    pub strict: bool,
    /// 自动字体：根据文档内容自动选择合适的字体（默认 true）
    ///
    /// 启用后，如果没有显式设置 `font_family`，会根据文档中的字符分布
    /// 自动推荐字体列表（如中文优先仿宋 FangSong，日文优先 Noto Serif CJK JP）。
    /// 用户提供的 CSS（包括 `<style>` 中的 `body { font-family }`）始终最高优先级。
    pub auto_font: bool,
    /// 页面配置（页面尺寸、边距等）
    ///
    /// 可通过 `@page` CSS 规则或此字段设置。此字段优先级高于 CSS 中的 `@page` 规则。
    /// 如果为 `None`（默认），则完全由 CSS `@page` 或内置默认值决定。
    pub page_config: Option<PageConfig>,
}

impl ConvertOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置全局默认字体家族
    ///
    /// 列表按优先级从高到低排列，支持 CSS 通用家族关键字
    /// （`serif`、`sans-serif`、`monospace`）和具体字体名称。
    ///
    /// ```
    /// use liepress::ConvertOptions;
    ///
    /// // 单个字体 + 回退
    /// let opts = ConvertOptions::new().with_font_family(&["Noto Sans CJK SC", "sans-serif"]);
    ///
    /// // 使用通用字体
    /// let opts = ConvertOptions::new().with_font_family(&["serif"]);
    /// ```
    pub fn with_font_family(mut self, families: &[&str]) -> Self {
        self.font_family = families.iter().map(|f| f.to_string()).collect();
        self
    }

    /// 设置用户 CSS 样式字符串
    pub fn with_css(mut self, css: &str) -> Self {
        self.user_css = css.to_string();
        self
    }

    /// 设置用户 CSS 样式文件路径
    pub fn with_css_file(mut self, path: PathBuf) -> Self {
        self.css_file = Some(path);
        self
    }

    /// 设置严格模式
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// 设置自动字体模式
    pub fn with_auto_font(mut self, auto_font: bool) -> Self {
        self.auto_font = auto_font;
        self
    }

    /// 设置页面配置（页面尺寸、边距等）
    ///
    /// 优先级高于 CSS 中的 `@page` 规则。
    /// 可通过 `PageConfig::default()` 创建后按需设置字段。
    ///
    /// ```
    /// use liepress::{ConvertOptions, ast::PageConfig};
    ///
    /// let page_cfg = PageConfig {
    ///     width: Some(841.890),  // A4 landscape
    ///     height: Some(595.276),
    ///     ..PageConfig::default()
    /// };
    /// let opts = ConvertOptions::new().with_page_config(page_cfg);
    /// ```
    pub fn with_page_config(mut self, config: PageConfig) -> Self {
        self.page_config = Some(config);
        self
    }

    /// 设置页眉文本（支持 {page} 和 {total} 模板变量）
    ///
    /// 页眉会显示在每页的顶部边距区域，居中对齐。
    /// 使用 `{page}` 表示当前页码，`{total}` 表示总页数。
    ///
    /// ```
    /// use liepress::ConvertOptions;
    ///
    /// let opts = ConvertOptions::new()
    ///     .with_header("我的文档");
    ///
    /// let opts = ConvertOptions::new()
    ///     .with_header("第 {page} 页 / 共 {total} 页");
    /// ```
    pub fn with_header(mut self, header: &str) -> Self {
        let config = self.page_config.get_or_insert_with(PageConfig::default);
        config.header = Some(header.to_string());
        self
    }

    /// 设置页脚文本（支持 {page} 和 {total} 模板变量）
    ///
    /// 页脚会显示在每页的底部边距区域，居中对齐。
    /// 使用 `{page}` 表示当前页码，`{total}` 表示总页数。
    ///
    /// ```
    /// use liepress::ConvertOptions;
    ///
    /// let opts = ConvertOptions::new()
    ///     .with_footer("- {page} -");
    ///
    /// let opts = ConvertOptions::new()
    ///     .with_footer("第 {page} 页 / 共 {total} 页");
    /// ```
    pub fn with_footer(mut self, footer: &str) -> Self {
        let config = self.page_config.get_or_insert_with(PageConfig::default);
        config.footer = Some(footer.to_string());
        self
    }

    /// 设置页眉字体大小（pt）
    ///
    /// 默认 9pt。仅在设置了页眉时生效。
    pub fn with_header_font_size(mut self, size: f32) -> Self {
        let config = self.page_config.get_or_insert_with(PageConfig::default);
        config.header_font_size = Some(size);
        self
    }

    /// 设置页脚字体大小（pt）
    ///
    /// 默认 9pt。仅在设置了页脚时生效。
    pub fn with_footer_font_size(mut self, size: f32) -> Self {
        let config = self.page_config.get_or_insert_with(PageConfig::default);
        config.footer_font_size = Some(size);
        self
    }

    /// 启用无限高度模式（仅限定宽度，高度自适应内容）
    ///
    /// 启用后：
    /// - 内容不分页，所有元素连续排列在一个页面上
    /// - 页面高度根据实际内容自动扩展
    /// - 页眉页脚仍会显示，但 `{total}` 始终为 1
    ///
    /// ```
    /// use liepress::ConvertOptions;
    ///
    /// let opts = ConvertOptions::new()
    ///     .with_height_unlimited(true);
    /// ```
    pub fn with_height_unlimited(mut self, unlimited: bool) -> Self {
        let config = self.page_config.get_or_insert_with(PageConfig::default);
        config.height_unlimited = Some(unlimited);
        self
    }
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            font_family: Vec::new(),
            user_css: String::new(),
            css_file: None,
            strict: false,
            auto_font: true,
            page_config: None,
        }
    }
}

// ─── 内部渲染辅助函数 ─────────────────────────────────────

fn render_pdf(document: &Document, settings: &PageSettings) -> crate::error::Result<Vec<u8>> {
    let generator = PdfDocumentGenerator::from_layout(document.clone(), settings.clone());
    generator.generate()
}

// ─── 内部文件读取辅助函数 ─────────────────────────────────

fn read_markdown_file(path: &Path) -> crate::error::Result<(String, Option<PathBuf>)> {
    let markdown = fs::read_to_string(path)?;
    let base_dir = path.parent().map(|p| p.to_path_buf());
    Ok((markdown, base_dir))
}

// ─── 自动字体推断 ────────────────────────────────────────

/// 运行脚本范围（避免误判 URL、代码、标签中的字符）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ScriptRange {
    Han,
    Japanese,
    Korean,
    Latin,
    Other,
}

impl ScriptRange {
    fn from_char(c: char) -> Self {
        let code = c as u32;
        match code {
            // 日文：平假名、片假名
            0x3040..=0x309F | 0x30A0..=0x30FF | 0x31F0..=0x31FF => ScriptRange::Japanese,
            // 中文/汉字：
            //   CJK 统一表意文字 (4E00-9FFF)
            //   CJK 扩展 A (3400-4DBF)
            //   CJK 扩展 B (20000-2A6DF)
            //   CJK 扩展 C (2A700-2B73F)
            //   CJK 扩展 D (2B740-2B81F)
            //   CJK 扩展 E (2B820-2CEAF)
            //   CJK 扩展 F (2CEB0-2EBE0)
            //   CJK 兼容表意文字 (F900-FAFF)
            //   CJK 兼容表意文字补充 (2F800-2FA1F)
            0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
            | 0x2CEB0..=0x2EBE0
            | 0x2F800..=0x2FA1F => ScriptRange::Han,
            // 韩文
            0xAC00..=0xD7AF => ScriptRange::Korean,
            // 拉丁文基础、补充标点
            0x0000..=0x00FF | 0x2000..=0x206F => ScriptRange::Latin,
            // 其他有字母属性的字符归为 Latin
            _ if c.is_alphabetic() => ScriptRange::Latin,
            _ => ScriptRange::Other,
        }
    }
}

/// 从 Markdown 文本中推断主要语言，返回推荐字体列表
fn infer_font_family(markdown: &str) -> Vec<String> {
    let mut counts = std::collections::HashMap::new();
    let mut in_code = false;
    let mut in_link = false;

    for line in markdown.lines() {
        // 简单状态机：代码块用 ``` 包围
        if line.trim().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }

        // 跳过标题标记和列表标记
        let content = line.trim_start().trim_start_matches('#').trim_start();

        for c in content.chars() {
            // 跳过链接内容 [text](url)
            if c == '[' {
                in_link = true;
                continue;
            }
            if in_link && c == ']' {
                in_link = false;
                continue;
            }
            if in_link {
                continue;
            }
            if c == '`' {
                continue;
            }

            let range = ScriptRange::from_char(c);
            if range != ScriptRange::Other {
                *counts.entry(range).or_insert(0) += 1;
            }
        }
    }

    let total: usize = counts.values().sum();
    if total == 0 {
        return vec!["serif".to_string()];
    }

    // 找出占比最高的脚本
    let dominant = counts
        .iter()
        .max_by_key(|&(_, count)| *count)
        .map(|(k, _)| *k)
        .unwrap_or(ScriptRange::Other);

    // 基础中文字体列表（作为所有语言场景的回退）
    // 包含衬线体（Serif）和无衬线体（Sans-serif）。
    // 中文 serif 优先使用仿宋（FangSong，公文/报告排版惯例）。
    // 说明：代码/CLI 场景优先使用 Noto CJK 统一系列（与 JP/KR 同族、字形规范统一），
    // 再回退到独立 Noto SC / 思源宋体（Source Han）及系统宋体。
    let chinese_serif_fonts = vec![
        "FangSong".to_string(),
        "FangSong_GB2312".to_string(),
        "Noto Serif CJK SC".to_string(),
        "Source Han Serif SC".to_string(),
        "Noto Serif SC".to_string(),
        "SimSun".to_string(),
        "SimSun-ExtB".to_string(),
    ];
    let chinese_sans_fonts = vec![
        "Noto Sans CJK SC".to_string(),
        "Source Han Sans SC".to_string(),
        "Noto Sans SC".to_string(),
        "Microsoft YaHei".to_string(),
        "WenQuanYi Micro Hei".to_string(),
    ];

    match dominant {
        ScriptRange::Han => {
            let mut fonts = chinese_serif_fonts;
            fonts.extend(chinese_sans_fonts);
            fonts.push("serif".to_string());
            fonts.push("sans-serif".to_string());
            fonts
        }
        ScriptRange::Japanese => vec![
            "Noto Serif CJK JP".to_string(),
            "Noto Serif JP".to_string(),
            "Noto Sans CJK JP".to_string(),
            "Noto Sans JP".to_string(),
            "serif".to_string(),
            "sans-serif".to_string(),
        ],
        ScriptRange::Korean => vec![
            "Noto Serif CJK KR".to_string(),
            "Noto Serif KR".to_string(),
            "Noto Sans CJK KR".to_string(),
            "Noto Sans KR".to_string(),
            "serif".to_string(),
            "sans-serif".to_string(),
        ],
        ScriptRange::Latin => {
            // CJK 优先（用户决策）：即使文档以拉丁字符为主，数字/英文也优先
            // 使用 CJK 字体（含数字字形），西文字体仅作为最后的回退。
            let mut fonts = chinese_serif_fonts;
            fonts.extend(chinese_sans_fonts);
            fonts.extend(vec![
                "Noto Serif".to_string(),
                "Georgia".to_string(),
                "Times New Roman".to_string(),
            ]);
            fonts.push("serif".to_string());
            fonts.push("sans-serif".to_string());
            fonts
        }
        ScriptRange::Other => {
            let mut fonts = chinese_serif_fonts;
            fonts.extend(chinese_sans_fonts);
            fonts.push("serif".to_string());
            fonts.push("sans-serif".to_string());
            fonts
        }
    }
}

// ─── CSS 解析 ───────────────────────────────────────────────

fn resolve_user_css(
    options: &ConvertOptions,
    markdown: Option<&str>,
) -> crate::error::Result<String> {
    let file_css = match &options.css_file {
        Some(path) => fs::read_to_string(path)?,
        None => String::new(),
    };

    // 判断用户是否已经显式设置了 font-family（通过 CSS 字符串）
    // 注意：这里做简单启发式判断。更严谨的做法是在 CSS 解析阶段
    // 检查是否有 body { font-family: ... } 规则。当前先以显式 font_family 为首要考虑。
    let user_has_font_css =
        file_css.contains("font-family") || options.user_css.contains("font-family");

    // 优先级：用户 CSS > auto-font > font_family
    let font_css = if user_has_font_css || !options.font_family.is_empty() {
        if !options.font_family.is_empty() {
            let families: Vec<String> = options
                .font_family
                .iter()
                .map(|f| {
                    if f.contains(' ') {
                        format!("\"{}\"", f)
                    } else {
                        f.clone()
                    }
                })
                .collect();
            format!("body {{ font-family: {}; }}\n", families.join(", "))
        } else {
            String::new()
        }
    } else if options.auto_font {
        if let Some(md) = markdown {
            let families = infer_font_family(md);
            format!(
                "body {{ font-family: {}; }}\n",
                families
                    .iter()
                    .map(|f| {
                        if f.contains(' ') {
                            format!("\"{}\"", f)
                        } else {
                            f.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let parts: Vec<&str> = [
        font_css.as_str(),
        options.user_css.as_str(),
        file_css.as_str(),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect();

    if parts.is_empty() {
        Ok(String::new())
    } else {
        Ok(parts.join("\n"))
    }
}

// ─── Markdown 管线入口 ──────────────────────────────────────

/// 将 [`ConvertOptions`] 中的页面设置解析为 PDF 后端用的 [`PageSettings`]。
fn page_settings_from(options: &ConvertOptions) -> PageSettings {
    PageSettings::from(options.page_config.clone().unwrap_or_default())
}

/// 核心转换逻辑：Markdown → PDF
///
/// 管线：Markdown → HTML → HtmlDocument → Styled Node → Document → PDF
/// 其中分页（切页、跨页表格）由 PDF 后端独立完成。
pub fn markdown_to_pdf(markdown: &str, options: &ConvertOptions) -> crate::error::Result<Vec<u8>> {
    let user_css = resolve_user_css(options, Some(markdown))?;
    let resolver = dom::ResourceResolver::new(None);
    let doc = dom::markdown_to_dom_with_resolver(markdown, &resolver);
    let document = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    render_pdf(&document, &page_settings_from(options))
}

/// Markdown 文件 → PDF（自动将本地图片嵌入为 base64）
pub fn markdown_file_to_pdf(
    path: &Path,
    options: &ConvertOptions,
) -> crate::error::Result<Vec<u8>> {
    let (markdown, base_dir) = read_markdown_file(path)?;
    let user_css = resolve_user_css(options, Some(&markdown))?;
    let resolver = dom::ResourceResolver::new(base_dir);
    let doc = dom::markdown_to_dom_with_resolver(&markdown, &resolver);
    let document = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    render_pdf(&document, &page_settings_from(options))
}

// ─── HTML → PDF ─────────────────────────────────────────────

/// HTML → PDF 转换
///
/// 直接将 HTML 内容转换为 PDF，不经过 Markdown 解析。
/// 适用于已有 HTML 文件的场景。
pub fn html_to_pdf(html: &str, options: &ConvertOptions) -> crate::error::Result<Vec<u8>> {
    let user_css = resolve_user_css(options, None)?;
    let resolver = dom::ResourceResolver::new(None);
    let doc = dom::parse_html_with_resolver(html, &resolver);
    let document = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    render_pdf(&document, &page_settings_from(options))
}

/// HTML 文件 → PDF（自动将本地图片嵌入为 base64）
pub fn html_file_to_pdf(path: &Path, options: &ConvertOptions) -> crate::error::Result<Vec<u8>> {
    let html = std::fs::read_to_string(path).map_err(crate::error::Error::IoError)?;
    let base_dir = path.parent().map(|p| p.to_path_buf());
    let resolver = dom::ResourceResolver::new(base_dir);
    let doc = dom::parse_html_with_resolver(&html, &resolver);
    let user_css = resolve_user_css(options, None)?;
    let document = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    render_pdf(&document, &page_settings_from(options))
}

// ─── SVG 输出 ──────────────────────────────────────────────────

/// Markdown → SVG（不分页长图）
pub fn markdown_to_svg(markdown: &str, options: &ConvertOptions) -> crate::error::Result<String> {
    let user_css = resolve_user_css(options, Some(markdown))?;
    let resolver = dom::ResourceResolver::new(None);
    let doc = dom::markdown_to_dom_with_resolver(markdown, &resolver);
    let document = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    Ok(output::svg::document_to_svg(
        &document,
        &page_settings_from(options),
    ))
}

/// HTML → SVG（不分页长图）
pub fn html_to_svg(html: &str, options: &ConvertOptions) -> crate::error::Result<String> {
    let user_css = resolve_user_css(options, None)?;
    let resolver = dom::ResourceResolver::new(None);
    let doc = dom::parse_html_with_resolver(html, &resolver);
    let document = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    Ok(output::svg::document_to_svg(
        &document,
        &page_settings_from(options),
    ))
}

/// Markdown 文件 → SVG（自动内联本地图片）。
pub fn markdown_file_to_svg(path: &Path, options: &ConvertOptions) -> crate::error::Result<String> {
    let (markdown, base_dir) = read_markdown_file(path)?;
    let user_css = resolve_user_css(options, Some(&markdown))?;
    let resolver = dom::ResourceResolver::new(base_dir);
    let doc = dom::markdown_to_dom_with_resolver(&markdown, &resolver);
    let document = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    Ok(output::svg::document_to_svg(
        &document,
        &page_settings_from(options),
    ))
}

/// HTML 文件 → SVG（自动内联本地图片）。
pub fn html_file_to_svg(path: &Path, options: &ConvertOptions) -> crate::error::Result<String> {
    let html = std::fs::read_to_string(path).map_err(crate::error::Error::IoError)?;
    let resolver = dom::ResourceResolver::new(path.parent().map(|p| p.to_path_buf()));
    let doc = dom::parse_html_with_resolver(&html, &resolver);
    let user_css = resolve_user_css(options, None)?;
    let document = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    Ok(output::svg::document_to_svg(
        &document,
        &page_settings_from(options),
    ))
}

// ─── PNG 输出 ──────────────────────────────────────────────────

/// Markdown → PNG（不分页长图，默认 150 DPI，保证清晰度）
pub fn markdown_to_png(markdown: &str, options: &ConvertOptions) -> crate::error::Result<Vec<u8>> {
    markdown_to_png_dpi(markdown, options, 150.0)
}

/// Markdown → PNG，可指定分辨率（DPI）
pub fn markdown_to_png_dpi(
    markdown: &str,
    options: &ConvertOptions,
    dpi: f32,
) -> crate::error::Result<Vec<u8>> {
    let user_css = resolve_user_css(options, Some(markdown))?;
    let resolver = dom::ResourceResolver::new(None);
    let doc = dom::markdown_to_dom_with_resolver(markdown, &resolver);
    let document = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    output::png::document_to_png(&document, &page_settings_from(options), dpi)
}

/// HTML → PNG（不分页长图，默认 150 DPI，保证清晰度）
pub fn html_to_png(html: &str, options: &ConvertOptions) -> crate::error::Result<Vec<u8>> {
    let user_css = resolve_user_css(options, None)?;
    let resolver = dom::ResourceResolver::new(None);
    let doc = dom::parse_html_with_resolver(html, &resolver);
    let document = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    output::png::document_to_png(&document, &page_settings_from(options), 150.0)
}

/// Markdown 文件 → PNG（自动内联本地图片，默认 150 DPI）。
pub fn markdown_file_to_png(
    path: &Path,
    options: &ConvertOptions,
) -> crate::error::Result<Vec<u8>> {
    markdown_file_to_png_dpi(path, options, 150.0)
}

/// Markdown 文件 → PNG，可指定 DPI（自动内联本地图片）。
pub fn markdown_file_to_png_dpi(
    path: &Path,
    options: &ConvertOptions,
    dpi: f32,
) -> crate::error::Result<Vec<u8>> {
    let (markdown, base_dir) = read_markdown_file(path)?;
    let user_css = resolve_user_css(options, Some(&markdown))?;
    let resolver = dom::ResourceResolver::new(base_dir);
    let doc = dom::markdown_to_dom_with_resolver(&markdown, &resolver);
    let document = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    output::png::document_to_png(&document, &page_settings_from(options), dpi)
}

/// HTML 文件 → PNG（自动内联本地图片，默认 150 DPI）。
pub fn html_file_to_png(path: &Path, options: &ConvertOptions) -> crate::error::Result<Vec<u8>> {
    let html = std::fs::read_to_string(path).map_err(crate::error::Error::IoError)?;
    let resolver = dom::ResourceResolver::new(path.parent().map(|p| p.to_path_buf()));
    let doc = dom::parse_html_with_resolver(&html, &resolver);
    let user_css = resolve_user_css(options, None)?;
    let document = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    output::png::document_to_png(&document, &page_settings_from(options), 150.0)
}

// ─── DOCX 输出 ─────────────────────────────────────────────────

/// Markdown → DOCX（消费 Styled AST，保留语义）。
///
/// 字符串输入无文件路径上下文，无法解析相对路径本地图片；
/// 若需嵌入本地图片，请使用 [`markdown_file_to_docx`]。
pub fn markdown_to_docx(markdown: &str, options: &ConvertOptions) -> crate::error::Result<Vec<u8>> {
    let user_css = resolve_user_css(options, Some(markdown))?;
    let resolver = dom::ResourceResolver::new(None);
    let doc = dom::markdown_to_dom_with_resolver(markdown, &resolver);
    let node = html_to_styled_node(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
    )?;
    output::docx::node_to_docx(&node)
}

/// Markdown 文件 → DOCX（自动将本地图片嵌入为 base64）。
pub fn markdown_file_to_docx(
    path: &Path,
    options: &ConvertOptions,
) -> crate::error::Result<Vec<u8>> {
    let (markdown, base_dir) = read_markdown_file(path)?;
    let user_css = resolve_user_css(options, Some(&markdown))?;
    let resolver = dom::ResourceResolver::new(base_dir);
    let doc = dom::markdown_to_dom_with_resolver(&markdown, &resolver);
    let node = html_to_styled_node(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
    )?;
    output::docx::node_to_docx(&node)
}

/// HTML → DOCX（消费 Styled AST，保留语义）。
///
/// 字符串输入无文件路径上下文，无法解析相对路径本地图片；
/// 若需嵌入本地图片，请使用 [`html_file_to_docx`]。
pub fn html_to_docx(html: &str, options: &ConvertOptions) -> crate::error::Result<Vec<u8>> {
    let user_css = resolve_user_css(options, None)?;
    let resolver = dom::ResourceResolver::new(None);
    let doc = dom::parse_html_with_resolver(html, &resolver);
    let node = html_to_styled_node(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
    )?;
    output::docx::node_to_docx(&node)
}

/// HTML 文件 → DOCX（自动将本地图片嵌入为 base64）。
pub fn html_file_to_docx(path: &Path, options: &ConvertOptions) -> crate::error::Result<Vec<u8>> {
    let html = std::fs::read_to_string(path).map_err(crate::error::Error::IoError)?;
    let base_dir = path.parent().map(|p| p.to_path_buf());
    let resolver = dom::ResourceResolver::new(base_dir);
    let doc = dom::parse_html_with_resolver(&html, &resolver);
    let user_css = resolve_user_css(options, None)?;
    let node = html_to_styled_node(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
    )?;
    output::docx::node_to_docx(&node)
}

// ─── 内部：HTML → Document 公共逻辑 ──────────────────────────────

/// HtmlDocument → Styled AST Node 的核心转换逻辑（供 DOCX 等语义输出使用）。
///
/// 图片内嵌已由 [`crate::dom::markdown_to_dom_with_resolver`]（Markdown）或
/// [`crate::dom::parser::parse_html_with_resolver`]（HTML）在 DOM 层完成，
/// 此处不再重复处理。
fn html_to_styled_node(
    doc: &crate::dom::HtmlDocument,
    user_css: Option<&str>,
    strict: bool,
) -> crate::error::Result<crate::ast::Node> {
    let builtin_css = ast::presets::DEFAULT_CSS;
    let mut engine =
        css::engine::CssEngine::new(builtin_css).map_err(crate::error::Error::CssParseError)?;
    for sheet in &doc.style_sheets {
        engine = engine
            .with_user_css(sheet)
            .map_err(crate::error::Error::CssParseError)?;
    }
    if let Some(css) = user_css.filter(|c| !c.is_empty()) {
        engine = engine
            .with_user_css(css)
            .map_err(crate::error::Error::CssParseError)?;
    }
    if strict {
        engine = engine.with_strict_mode(true);
    }
    let default_style = ast::Style::default();
    let root_style = engine.resolve_style("html", &[], None, &[], &default_style);
    engine.set_root_font_size(root_style.font_size_pt);
    Ok(dom::to_ast::html_to_styled_nodes(doc, &engine))
}

/// HtmlDocument → Document 的核心转换逻辑
///
/// 被所有 `*_to_pdf`/`*_to_svg`/`*_to_png` 入口共享。源 IR 不分页，分页由各输出后端负责。
/// 输入已是管线 Layer 1 的 `HtmlDocument`（Markdown/HTML 两个输入源在此汇合）。
///
/// 图片内嵌已由 [`crate::dom::markdown_to_dom_with_resolver`]（Markdown）或
/// [`crate::dom::parser::parse_html_with_resolver`]（HTML）在 DOM 层完成。
fn html_to_layout(
    doc: &crate::dom::HtmlDocument,
    user_css: Option<&str>,
    strict: bool,
    page_config: Option<PageConfig>,
) -> crate::error::Result<Document> {
    // 1. 合并 CSS：内置样式 + <style> 标签 + 用户 CSS
    let builtin_css = ast::presets::DEFAULT_CSS;
    let mut engine =
        css::engine::CssEngine::new(builtin_css).map_err(crate::error::Error::CssParseError)?;

    for sheet in &doc.style_sheets {
        engine = engine
            .with_user_css(sheet)
            .map_err(crate::error::Error::CssParseError)?;
    }

    if let Some(css) = user_css.filter(|c| !c.is_empty()) {
        engine = engine
            .with_user_css(css)
            .map_err(crate::error::Error::CssParseError)?;
    }

    if strict {
        engine = engine.with_strict_mode(true);
    }

    let default_style = ast::Style::default();
    let root_style = engine.resolve_style("html", &[], None, &[], &default_style);
    engine.set_root_font_size(root_style.font_size_pt);

    // 3. HtmlDocument → Styled Node Tree
    let styled_node = dom::to_ast::html_to_styled_nodes(doc, &engine);

    // 4. Styled Node → Document（源 IR，不分页）
    let page_settings = page_config.map(PageSettings::from).unwrap_or_default();

    Ok(ast_to_layout(&styled_node, &page_settings))
}

#[cfg(test)]
mod pipeline_tests {
    use super::*;

    fn sample_markdown() -> &'static str {
        "# Hello World\n\nThis is a **test** paragraph with *italic* text.\n\n- item 1\n- item 2\n- [ ] unchecked task\n- [x] checked task\n\n> A blockquote\n\n| A | B |\n|---|---|\n| 1 | 2 |"
    }

    #[test]
    fn test_pdf_generation() {
        let opts = ConvertOptions::default();
        let result = markdown_to_pdf(sample_markdown(), &opts);
        assert!(
            result.is_ok(),
            "PDF generation should succeed: {:?}",
            result.err()
        );
        let pdf = result.unwrap();
        assert!(!pdf.is_empty(), "PDF bytes should not be empty");
        assert!(pdf.starts_with(b"%PDF"), "Should be valid PDF");
    }
}
