pub mod error;
pub mod generator;
pub mod render;
pub mod ast;
pub mod text;
pub mod visual;

use std::path::Path;
use std::path::PathBuf;
use std::fs;

pub use render::{PixmapDocumentGenerator, PixmapRenderer, SvgDocumentGenerator, SvgRenderer, PdfDocumentGenerator, PdfRenderer, PageRenderer};

use generator::{
    markdown_to_document, markdown_to_document_with_base_dir,
    markdown_to_document_with_css, markdown_to_document_with_css_and_base_dir,
    markdown_to_document_with_css_strict, markdown_to_document_with_css_and_base_dir_strict,
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
    /// 设置后会自动生成 `body {{ font-family: ... }}` 样式，
    /// 通过 CSS 继承机制应用到所有元素。如果同时提供了 `user_css`
    /// 或 `css_file`，其中的 `body {{ font-family }}` 会覆盖此设置。
    pub font_family: Vec<String>,
    /// 用户提供的 CSS 样式字符串（叠加在默认样式之上）
    pub user_css: String,
    /// 用户提供的 CSS 样式文件路径（叠加在默认样式之上）
    /// 如果与 `user_css` 同时设置，两者会合并
    pub css_file: Option<PathBuf>,
    /// 严格模式：CSS 解析失败时返回错误（默认 false）
    pub strict: bool,
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
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            font_family: Vec::new(),
            user_css: String::new(),
            css_file: None,
            strict: false,
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

fn resolve_user_css(options: &ConvertOptions) -> crate::error::Result<String> {
    let file_css = match &options.css_file {
        Some(path) => fs::read_to_string(path)?,
        None => String::new(),
    };

    // 如果设置了全局字体，生成 body { font-family: ... } 注入到用户 CSS
    let font_css = if !options.font_family.is_empty() {
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

pub fn markdown_to_pdf_with_options(markdown: &str, options: &ConvertOptions) -> crate::error::Result<Vec<u8>> {
    let user_css = resolve_user_css(options)?;
    let doc = if options.strict {
        markdown_to_document_with_css_strict(markdown, &user_css)
            .map_err(crate::error::Error::CssParseError)?
    } else {
        markdown_to_document_with_css(markdown, &user_css)
    };
    render_pdf(&doc)
}

pub fn markdown_to_svg_with_options(markdown: &str, options: &ConvertOptions) -> crate::error::Result<Vec<String>> {
    let user_css = resolve_user_css(options)?;
    let doc = if options.strict {
        markdown_to_document_with_css_strict(markdown, &user_css)
            .map_err(crate::error::Error::CssParseError)?
    } else {
        markdown_to_document_with_css(markdown, &user_css)
    };
    Ok(render_svg(&doc))
}

pub fn markdown_to_png_with_options(markdown: &str, options: &ConvertOptions) -> crate::error::Result<Vec<Vec<u8>>> {
    let user_css = resolve_user_css(options)?;
    let doc = if options.strict {
        markdown_to_document_with_css_strict(markdown, &user_css)
            .map_err(crate::error::Error::CssParseError)?
    } else {
        markdown_to_document_with_css(markdown, &user_css)
    };
    render_png(&doc)
}

pub fn markdown_file_to_pdf_with_options(path: &Path, options: &ConvertOptions) -> crate::error::Result<Vec<u8>> {
    let (markdown, base_dir) = read_markdown_file(path)?;
    let user_css = resolve_user_css(options)?;
    let doc = if options.strict {
        markdown_to_document_with_css_and_base_dir_strict(&markdown, &user_css, base_dir)
            .map_err(crate::error::Error::CssParseError)?
    } else {
        markdown_to_document_with_css_and_base_dir(&markdown, &user_css, base_dir)
    };
    render_pdf(&doc)
}

pub fn markdown_file_to_svg_with_options(path: &Path, options: &ConvertOptions) -> crate::error::Result<Vec<String>> {
    let (markdown, base_dir) = read_markdown_file(path)?;
    let user_css = resolve_user_css(options)?;
    let doc = if options.strict {
        markdown_to_document_with_css_and_base_dir_strict(&markdown, &user_css, base_dir)
            .map_err(crate::error::Error::CssParseError)?
    } else {
        markdown_to_document_with_css_and_base_dir(&markdown, &user_css, base_dir)
    };
    Ok(render_svg(&doc))
}

pub fn markdown_file_to_png_with_options(path: &Path, options: &ConvertOptions) -> crate::error::Result<Vec<Vec<u8>>> {
    let (markdown, base_dir) = read_markdown_file(path)?;
    let user_css = resolve_user_css(options)?;
    let doc = if options.strict {
        markdown_to_document_with_css_and_base_dir_strict(&markdown, &user_css, base_dir)
            .map_err(crate::error::Error::CssParseError)?
    } else {
        markdown_to_document_with_css_and_base_dir(&markdown, &user_css, base_dir)
    };
    render_png(&doc)
}