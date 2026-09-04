pub mod ast;
pub mod css;
pub mod document; // 重构文档层（方案 refactor-document-layer.md）
pub mod dom; // HTML AST（管线 Layer 1）：Markdown/HTML → HtmlDocument
pub mod enrich; // AST 富化阶段：外绘（mermaid/liecharts）+ 语法高亮
pub mod error;
pub mod ext_render; // 可插拔「代码块 → 图片」渲染器（顶层模块，见 docs/design.md §2）
pub mod highlight; // 代码块语法高亮（AST 层，基于 syntect）
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

/// 核心转换逻辑：Markdown → PDF
///
/// 管线：Markdown → HTML → HtmlDocument → Styled Node → Document → PDF
/// 其中分页（切页、跨页表格）由 PDF 后端独立完成。
pub fn markdown_to_pdf(markdown: &str, options: &ConvertOptions) -> crate::error::Result<Vec<u8>> {
    let user_css = resolve_user_css(options, Some(markdown))?;
    let resolver = dom::ResourceResolver::new(None);
    let doc = dom::markdown_to_dom_with_resolver(markdown, &resolver);
    let (document, page_settings) = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    render_pdf(&document, &page_settings)
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
    let (document, page_settings) = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    render_pdf(&document, &page_settings)
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
    let (document, page_settings) = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    render_pdf(&document, &page_settings)
}

/// HTML 文件 → PDF（自动将本地图片嵌入为 base64）
pub fn html_file_to_pdf(path: &Path, options: &ConvertOptions) -> crate::error::Result<Vec<u8>> {
    let html = std::fs::read_to_string(path).map_err(crate::error::Error::IoError)?;
    let base_dir = path.parent().map(|p| p.to_path_buf());
    let resolver = dom::ResourceResolver::new(base_dir);
    let doc = dom::parse_html_with_resolver(&html, &resolver);
    let user_css = resolve_user_css(options, None)?;
    let (document, page_settings) = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    render_pdf(&document, &page_settings)
}

// ─── SVG 输出 ──────────────────────────────────────────────────

/// Markdown → SVG（不分页长图）
pub fn markdown_to_svg(markdown: &str, options: &ConvertOptions) -> crate::error::Result<String> {
    let user_css = resolve_user_css(options, Some(markdown))?;
    let resolver = dom::ResourceResolver::new(None);
    let doc = dom::markdown_to_dom_with_resolver(markdown, &resolver);
    let (document, page_settings) = html_to_layout(
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
        &page_settings,
    ))
}

/// HTML → SVG（不分页长图）
pub fn html_to_svg(html: &str, options: &ConvertOptions) -> crate::error::Result<String> {
    let user_css = resolve_user_css(options, None)?;
    let resolver = dom::ResourceResolver::new(None);
    let doc = dom::parse_html_with_resolver(html, &resolver);
    let (document, page_settings) = html_to_layout(
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
        &page_settings,
    ))
}

/// Markdown 文件 → SVG（自动内联本地图片）。
pub fn markdown_file_to_svg(path: &Path, options: &ConvertOptions) -> crate::error::Result<String> {
    let (markdown, base_dir) = read_markdown_file(path)?;
    let user_css = resolve_user_css(options, Some(&markdown))?;
    let resolver = dom::ResourceResolver::new(base_dir);
    let doc = dom::markdown_to_dom_with_resolver(&markdown, &resolver);
    let (document, page_settings) = html_to_layout(
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
        &page_settings,
    ))
}

/// HTML 文件 → SVG（自动内联本地图片）。
pub fn html_file_to_svg(path: &Path, options: &ConvertOptions) -> crate::error::Result<String> {
    let html = std::fs::read_to_string(path).map_err(crate::error::Error::IoError)?;
    let resolver = dom::ResourceResolver::new(path.parent().map(|p| p.to_path_buf()));
    let doc = dom::parse_html_with_resolver(&html, &resolver);
    let user_css = resolve_user_css(options, None)?;
    let (document, page_settings) = html_to_layout(
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
        &page_settings,
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
    let (document, page_settings) = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    output::png::document_to_png(&document, &page_settings, dpi)
}

/// HTML → PNG（不分页长图，默认 150 DPI，保证清晰度）
pub fn html_to_png(html: &str, options: &ConvertOptions) -> crate::error::Result<Vec<u8>> {
    let user_css = resolve_user_css(options, None)?;
    let resolver = dom::ResourceResolver::new(None);
    let doc = dom::parse_html_with_resolver(html, &resolver);
    let (document, page_settings) = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    output::png::document_to_png(&document, &page_settings, 150.0)
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
    let (document, page_settings) = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    output::png::document_to_png(&document, &page_settings, dpi)
}

/// HTML 文件 → PNG（自动内联本地图片，默认 150 DPI）。
pub fn html_file_to_png(path: &Path, options: &ConvertOptions) -> crate::error::Result<Vec<u8>> {
    let html = std::fs::read_to_string(path).map_err(crate::error::Error::IoError)?;
    let resolver = dom::ResourceResolver::new(path.parent().map(|p| p.to_path_buf()));
    let doc = dom::parse_html_with_resolver(&html, &resolver);
    let user_css = resolve_user_css(options, None)?;
    let (document, page_settings) = html_to_layout(
        &doc,
        if user_css.is_empty() {
            None
        } else {
            Some(&user_css)
        },
        options.strict,
        options.page_config.clone(),
    )?;
    output::png::document_to_png(&document, &page_settings, 150.0)
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
        options.page_config.as_ref(),
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
        options.page_config.as_ref(),
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
        options.page_config.as_ref(),
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
        options.page_config.as_ref(),
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
    page_config: Option<&PageConfig>,
) -> crate::error::Result<crate::ast::Node> {
    let builtin_css = ast::presets::DEFAULT_CSS;
    let mut engine =
        css::engine::CssEngine::new(builtin_css).map_err(crate::error::Error::CssParseError)?;

    // 合并所有「用户级」CSS 到单次 with_user_css 调用，避免文档 <style> 规则被 user_css 覆盖。
    // 优先级：文档 <style> < 用户 CSS（用户级覆盖文档级），故 user_css 置于末尾。
    let mut merged_user_css = String::new();
    for sheet in &doc.style_sheets {
        merged_user_css.push_str(sheet);
        merged_user_css.push('\n');
    }
    if let Some(css) = user_css.filter(|c| !c.is_empty()) {
        merged_user_css.push_str(css);
        merged_user_css.push('\n');
    }
    if !merged_user_css.trim().is_empty() {
        engine = engine
            .with_user_css(&merged_user_css)
            .map_err(crate::error::Error::CssParseError)?;
    }
    if strict {
        engine = engine.with_strict_mode(true);
    }
    let default_style = ast::Style::default();
    let root_style = engine.resolve_style("html", &[], None, &[], &default_style);
    engine.set_root_font_size(root_style.font_size_pt);
    // 百分比基准 = 页面内容宽度（显式 PageConfig 优先，与 PDF 路径一致）。
    apply_page_metrics(&mut engine, page_config);
    let mut node = dom::to_ast::html_to_styled_nodes(doc, &engine);
    // AST 富化：外绘 + 语法高亮（与 PDF 路径共享同一份产物，保证 DOCX 也有图表/高亮）。
    // 页面几何与 `%` 基准一致：取 `@page` 与显式配置的合并结果。
    let page_settings =
        PageSettings::from(merged_page_config(engine.page_config(), page_config));
    enrich::enrich_ast(&mut node, &page_settings);
    Ok(node)
}

/// 依据页面配置设定 CSS 引擎的包含块宽度（盒模型 `%` 的基准）。
///
/// 合并优先级见 [`merged_page_config`]。必须在 Styled AST 转换之前调用，
/// 否则 `%` 会退回默认值。
fn apply_page_metrics(engine: &mut css::engine::CssEngine, page_config: Option<&PageConfig>) {
    let pc = merged_page_config(engine.page_config(), page_config);
    let settings = PageSettings::from(pc);
    engine.set_containing_block_width(settings.content_width());
}

/// 合并页面几何来源，优先级：显式 [`PageConfig`]（来自 [`ConvertOptions`]）
/// > CSS `@page` 声明 > 内置默认（A4）。
///
/// 这是**唯一**的页面几何派生入口：`%` 宽度基准（`apply_page_metrics`）与
/// 渲染端实际页尺寸（PDF/SVG/PNG/DOCX）必须消费同一份合并结果，否则
/// 「仅通过 `@page` 设置页面」时会出现「`%` 按 @page 算、页面却是 A4」的错位。
fn merged_page_config(engine_page: &PageConfig, page_config: Option<&PageConfig>) -> PageConfig {
    let mut pc = engine_page.clone();
    if let Some(explicit) = page_config {
        if explicit.width.is_some() {
            pc.width = explicit.width;
        }
        if explicit.height.is_some() {
            pc.height = explicit.height;
        }
        if explicit.margin_top.is_some() {
            pc.margin_top = explicit.margin_top;
        }
        if explicit.margin_bottom.is_some() {
            pc.margin_bottom = explicit.margin_bottom;
        }
        if explicit.margin_left.is_some() {
            pc.margin_left = explicit.margin_left;
        }
        if explicit.margin_right.is_some() {
            pc.margin_right = explicit.margin_right;
        }
        if explicit.height_unlimited.is_some() {
            pc.height_unlimited = explicit.height_unlimited;
        }
        if explicit.header.is_some() {
            pc.header = explicit.header.clone();
        }
        if explicit.footer.is_some() {
            pc.footer = explicit.footer.clone();
        }
        if explicit.header_font_size.is_some() {
            pc.header_font_size = explicit.header_font_size;
        }
        if explicit.footer_font_size.is_some() {
            pc.footer_font_size = explicit.footer_font_size;
        }
    }
    pc
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
) -> crate::error::Result<(Document, PageSettings)> {
    // 1. 合并 CSS：内置样式 + <style> 标签 + 用户 CSS
    let builtin_css = ast::presets::DEFAULT_CSS;
    let mut engine =
        css::engine::CssEngine::new(builtin_css).map_err(crate::error::Error::CssParseError)?;

    // 合并所有「用户级」CSS 到单次 with_user_css 调用：
    // `with_user_css` 会整体替换 user_rules，循环/分次调用会互相覆盖，
    // 导致文档内 <style> 规则被后续 user_css 覆盖而失效。
    // 优先级：文档 <style> < 用户 CSS（用户级覆盖文档级），故 user_css 置于末尾。
    let mut merged_user_css = String::new();
    for sheet in &doc.style_sheets {
        merged_user_css.push_str(sheet);
        merged_user_css.push('\n');
    }
    if let Some(css) = user_css.filter(|c| !c.is_empty()) {
        merged_user_css.push_str(css);
        merged_user_css.push('\n');
    }
    if !merged_user_css.trim().is_empty() {
        engine = engine
            .with_user_css(&merged_user_css)
            .map_err(crate::error::Error::CssParseError)?;
    }

    if strict {
        engine = engine.with_strict_mode(true);
    }

    let default_style = ast::Style::default();
    let root_style = engine.resolve_style("html", &[], None, &[], &default_style);
    engine.set_root_font_size(root_style.font_size_pt);
    // 百分比基准 = 页面内容宽度（须在 Styled AST 转换前设置）。
    apply_page_metrics(&mut engine, page_config.as_ref());

    // 3. HtmlDocument → Styled Node Tree
    let mut styled_node = dom::to_ast::html_to_styled_nodes(doc, &engine);

    // 4. Styled Node → Document（源 IR，不分页）
    // 页面几何 = CSS `@page` 与显式 `PageConfig` 合并后的结果（与 `%` 基准一致）。
    // 若只取显式配置，仅通过 `@page { size/margin }` 设置页面时渲染端会退回
    // A4 默认值，导致 `%` 宽度按 @page 计算而实际页面仍是 A4。
    let page_settings =
        PageSettings::from(merged_page_config(engine.page_config(), page_config.as_ref()));

    // 3.5 AST 富化：外绘（mermaid/liecharts → 图片节点）+ 语法高亮。
    // 必须在 `ast_to_layout` 之前完成，各后端（含 DOCX/HTML）据此消费同一份产物。
    enrich::enrich_ast(&mut styled_node, &page_settings);

    Ok((ast_to_layout(&styled_node, &page_settings), page_settings))
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

    // ─── H2：CSS @page 页面几何（2026-09-04 审查） ───

    #[test]
    fn test_at_page_css_sets_page_geometry() {
        // 仅通过 <style> 中的 @page 设置页面（无显式 ConvertOptions::page_config）：
        // 渲染端 PageSettings 必须采用 @page 的尺寸/边距，而非退回 A4 默认值。
        let md = "<style>@page { size: A5; margin: 24pt; }</style>\n\n# Hi\n";
        let resolver = dom::ResourceResolver::new(None);
        let doc = dom::markdown_to_dom_with_resolver(md, &resolver);
        let options = ConvertOptions::default();
        let (_document, settings) = html_to_layout(
            &doc,
            None,
            options.strict,
            options.page_config.clone(),
        )
        .expect("layout");
        // A5 = 419.53 × 595.28 pt
        assert!(
            (settings.width_pt - 419.53).abs() < 0.5,
            "width 应为 A5 宽 419.53，实际 {}",
            settings.width_pt
        );
        assert!(
            (settings.height_pt - 595.28).abs() < 0.5,
            "height 应为 A5 高 595.28，实际 {}",
            settings.height_pt
        );
        assert!(
            (settings.margin_top_pt - 24.0).abs() < 0.1,
            "margin_top 应为 @page 的 24pt，实际 {}",
            settings.margin_top_pt
        );
    }

    #[test]
    fn test_explicit_page_config_overrides_at_page() {
        // 优先级：显式 PageConfig > @page。
        let md = "<style>@page { size: A5; margin: 24pt; }</style>\n\n# Hi\n";
        let resolver = dom::ResourceResolver::new(None);
        let doc = dom::markdown_to_dom_with_resolver(md, &resolver);
        let options =
            ConvertOptions::default().with_page_config(PageConfig {
                width: Some(300.0),
                ..Default::default()
            });
        let (_document, settings) = html_to_layout(
            &doc,
            None,
            options.strict,
            options.page_config.clone(),
        )
        .expect("layout");
        assert!(
            (settings.width_pt - 300.0).abs() < 0.5,
            "显式 width 应覆盖 @page，实际 {}",
            settings.width_pt
        );
        // 未被显式覆盖的 margin 仍取 @page 值。
        assert!((settings.margin_top_pt - 24.0).abs() < 0.1);
    }
}
