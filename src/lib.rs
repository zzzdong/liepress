pub mod error;
pub mod generator;
pub mod render;
pub mod ast;
pub mod text;
pub mod visual;

use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub use render::{
    PixmapDocumentGenerator, PixmapRenderer,
    SvgDocumentGenerator, SvgRenderer,
    PdfDocumentGenerator, PdfRenderer, PageRenderer,
};

pub use ast::PageConfig;

use generator::{
    markdown_to_document, markdown_to_document_with_base_dir,
    markdown_to_document_with_css_and_page_config,
    Document,
};

/// Markdown 转换配置
///
/// 控制样式、字体、严格模式等选项。通过 builder 风格方法快速构造：
///
/// ```
/// use liepress::ConvertOptions;
///
/// let opts = ConvertOptions::new()
///     .with_font_family(&["Noto Sans SC", "sans-serif"])
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
    /// 自动字体：根据文档内容自动选择合适的字体（默认 false）
    ///
    /// 启用后，如果没有显式设置 `font_family`，会根据文档中的字符分布
    /// 自动推荐字体列表（如中文优先 Noto Serif SC，日文优先 Noto Serif JP）。
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
    /// let opts = ConvertOptions::new().with_font_family(&["Noto Sans SC", "sans-serif"]);
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
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            font_family: Vec::new(),
            user_css: String::new(),
            css_file: None,
            strict: false,
            auto_font: false,
            page_config: None,
        }
    }
}

// ─── 内部渲染辅助函数 ─────────────────────────────────────

fn render_pdf(document: &Document) -> crate::error::Result<Vec<u8>> {
    let mut pdf_gen = PdfDocumentGenerator::new("output".to_string());
    for page in &document.pages {
        pdf_gen.render_page(page)?;
    }
    pdf_gen.finalize()
}

fn render_svg(document: &Document) -> Vec<String> {
    let mut svgs = Vec::new();
    for page in &document.pages {
        let mut renderer = SvgRenderer::new(page.width, page.height);
        renderer.render_elements(&page.elements);
        svgs.push(renderer.finalize());
    }
    svgs
}

fn render_png(document: &Document) -> crate::error::Result<Vec<Vec<u8>>> {
    let mut pngs = Vec::new();
    for page in &document.pages {
        let mut renderer = PixmapRenderer::new_default_dpi(page.width, page.height);
        renderer.render_elements(&page.elements);
        pngs.push(renderer.render_to_png()?);
    }
    Ok(pngs)
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
            0x3040..=0x309F | 0x30A0..=0x30FF => ScriptRange::Japanese,
            0x3400..=0x4DBF | 0x4E00..=0x9FFF => ScriptRange::Han,
            0xAC00..=0xD7AF => ScriptRange::Korean,
            0x0000..=0x00FF | 0x2000..=0x206F => ScriptRange::Latin,
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
    let dominant = counts.iter().max_by_key(|&(_, count)| *count).map(|(k, _)| *k).unwrap_or(ScriptRange::Other);

    match dominant {
        ScriptRange::Han => vec!["Noto Serif SC".to_string(), "Noto Sans SC".to_string(), "serif".to_string()],
        ScriptRange::Japanese => vec!["Noto Serif JP".to_string(), "Noto Sans JP".to_string(), "sans-serif".to_string()],
        ScriptRange::Korean => vec!["Noto Serif KR".to_string(), "Noto Sans KR".to_string(), "sans-serif".to_string()],
        ScriptRange::Latin => vec!["Noto Serif".to_string(), "Georgia".to_string(), "Times New Roman".to_string(), "serif".to_string()],
        ScriptRange::Other => vec!["serif".to_string()],
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
    let user_has_font_css = file_css.contains("font-family")
        || options.user_css.contains("font-family");

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

    let parts: Vec<&str> = [font_css.as_str(), options.user_css.as_str(), file_css.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();

    if parts.is_empty() {
        Ok(String::new())
    } else {
        Ok(parts.join("\n"))
    }
}

// ─── 快速入口（无额外配置） ───────────────────────────────

pub fn markdown_to_pdf(markdown: &str) -> crate::error::Result<Vec<u8>> {
    render_pdf(&markdown_to_document(markdown))
}

pub fn markdown_to_svg(markdown: &str) -> crate::error::Result<Vec<String>> {
    Ok(render_svg(&markdown_to_document(markdown)))
}

pub fn markdown_to_png(markdown: &str) -> crate::error::Result<Vec<Vec<u8>>> {
    render_png(&markdown_to_document(markdown))
}

pub fn markdown_file_to_pdf(path: &Path) -> crate::error::Result<Vec<u8>> {
    let (markdown, base_dir) = read_markdown_file(path)?;
    render_pdf(&markdown_to_document_with_base_dir(&markdown, base_dir))
}

pub fn markdown_file_to_svg(path: &Path) -> crate::error::Result<Vec<String>> {
    let (markdown, base_dir) = read_markdown_file(path)?;
    Ok(render_svg(&markdown_to_document_with_base_dir(&markdown, base_dir)))
}

pub fn markdown_file_to_png(path: &Path) -> crate::error::Result<Vec<Vec<u8>>> {
    let (markdown, base_dir) = read_markdown_file(path)?;
    render_png(&markdown_to_document_with_base_dir(&markdown, base_dir))
}

// ─── 带配置的入口 ────────────────────────────────────────

pub fn markdown_to_pdf_with_options(
    markdown: &str,
    options: &ConvertOptions,
) -> crate::error::Result<Vec<u8>> {
    let user_css = resolve_user_css(options, Some(markdown))?;
    let doc = (if options.strict {
        markdown_to_document_with_css_and_page_config(
            markdown, &user_css, options.page_config.clone(), None, true,
        )
    } else {
        markdown_to_document_with_css_and_page_config(
            markdown, &user_css, options.page_config.clone(), None, false,
        )
    })
    .map_err(crate::error::Error::CssParseError)?;
    render_pdf(&doc)
}

pub fn markdown_to_svg_with_options(
    markdown: &str,
    options: &ConvertOptions,
) -> crate::error::Result<Vec<String>> {
    let user_css = resolve_user_css(options, Some(markdown))?;
    let doc = (if options.strict {
        markdown_to_document_with_css_and_page_config(
            markdown, &user_css, options.page_config.clone(), None, true,
        )
    } else {
        markdown_to_document_with_css_and_page_config(
            markdown, &user_css, options.page_config.clone(), None, false,
        )
    })
    .map_err(crate::error::Error::CssParseError)?;
    Ok(render_svg(&doc))
}

pub fn markdown_to_png_with_options(
    markdown: &str,
    options: &ConvertOptions,
) -> crate::error::Result<Vec<Vec<u8>>> {
    let user_css = resolve_user_css(options, Some(markdown))?;
    let doc = (if options.strict {
        markdown_to_document_with_css_and_page_config(
            markdown, &user_css, options.page_config.clone(), None, true,
        )
    } else {
        markdown_to_document_with_css_and_page_config(
            markdown, &user_css, options.page_config.clone(), None, false,
        )
    })
    .map_err(crate::error::Error::CssParseError)?;
    render_png(&doc)
}

pub fn markdown_file_to_pdf_with_options(
    path: &Path,
    options: &ConvertOptions,
) -> crate::error::Result<Vec<u8>> {
    let (markdown, base_dir) = read_markdown_file(path)?;
    let user_css = resolve_user_css(options, Some(&markdown))?;
    let doc = (if options.strict {
        markdown_to_document_with_css_and_page_config(
            &markdown, &user_css, options.page_config.clone(), base_dir, true,
        )
    } else {
        markdown_to_document_with_css_and_page_config(
            &markdown, &user_css, options.page_config.clone(), base_dir, false,
        )
    })
    .map_err(crate::error::Error::CssParseError)?;
    render_pdf(&doc)
}

pub fn markdown_file_to_svg_with_options(
    path: &Path,
    options: &ConvertOptions,
) -> crate::error::Result<Vec<String>> {
    let (markdown, base_dir) = read_markdown_file(path)?;
    let user_css = resolve_user_css(options, Some(&markdown))?;
    let doc = (if options.strict {
        markdown_to_document_with_css_and_page_config(
            &markdown, &user_css, options.page_config.clone(), base_dir, true,
        )
    } else {
        markdown_to_document_with_css_and_page_config(
            &markdown, &user_css, options.page_config.clone(), base_dir, false,
        )
    })
    .map_err(crate::error::Error::CssParseError)?;
    Ok(render_svg(&doc))
}

pub fn markdown_file_to_png_with_options(
    path: &Path,
    options: &ConvertOptions,
) -> crate::error::Result<Vec<Vec<u8>>> {
    let (markdown, base_dir) = read_markdown_file(path)?;
    let user_css = resolve_user_css(options, Some(&markdown))?;
    let doc = (if options.strict {
        markdown_to_document_with_css_and_page_config(
            &markdown, &user_css, options.page_config.clone(), base_dir, true,
        )
    } else {
        markdown_to_document_with_css_and_page_config(
            &markdown, &user_css, options.page_config.clone(), base_dir, false,
        )
    })
    .map_err(crate::error::Error::CssParseError)?;
    render_png(&doc)
}
