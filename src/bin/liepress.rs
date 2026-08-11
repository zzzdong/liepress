use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};
use liepress::{
    ConvertOptions, PageConfig, html_file_to_pdf, html_file_to_png, html_file_to_svg,
    markdown_file_to_pdf, markdown_file_to_png, markdown_file_to_svg,
};

#[derive(ValueEnum, Clone, Debug)]
enum Format {
    Pdf,
    Svg,
    Png,
    Html,
}

#[derive(Clone, Debug)]
enum InputFormat {
    Markdown,
    Html,
}

/// 从输出文件扩展名推断格式
fn infer_format_from_ext(path: &Path) -> Option<Format> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("pdf") => Some(Format::Pdf),
        Some("svg") => Some(Format::Svg),
        Some("png") => Some(Format::Png),
        Some("html") | Some("htm") => Some(Format::Html),
        _ => None,
    }
}

/// 从输入文件扩展名推断格式
fn infer_input_format_from_ext(path: &Path) -> Option<InputFormat> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("md") | Some("markdown") => Some(InputFormat::Markdown),
        Some("html") | Some("htm") => Some(InputFormat::Html),
        _ => None,
    }
}

/// Markdown/HTML to PDF/SVG/PNG/HTML converter
#[derive(Parser, Debug)]
#[command(name = "liepress")]
#[command(about = "Convert Markdown or HTML to PDF, SVG, PNG or HTML")]
struct Args {
    /// Input file path (Markdown: .md, .markdown; HTML: .html, .htm)
    #[arg(short, long, value_name = "FILE")]
    input: PathBuf,

    /// Output file path (format inferred from extension: .pdf, .svg, .png, .html)
    #[arg(short, long, value_name = "FILE")]
    output: PathBuf,

    /// Output format (overrides extension-based inference)
    #[arg(short, long, value_enum)]
    format: Option<Format>,

    /// Optional CSS stylesheet file to override default styles
    #[arg(short = 's', long = "style", value_name = "CSS_FILE")]
    style: Option<PathBuf>,

    /// Document title (used in HTML <title>; defaults to first <h1>)
    #[arg(short = 't', long = "title", value_name = "TITLE")]
    title: Option<String>,

    /// Strict mode: fail on CSS parsing errors instead of ignoring them
    #[arg(short = 'S', long = "strict", default_value_t = false)]
    strict: bool,

    /// Page size preset (A3, A4, A5, A6, Letter, Legal, Tabloid)
    #[arg(short = 'p', long = "page-size", value_name = "SIZE")]
    page_size: Option<String>,

    /// Custom page width with unit (e.g. "210mm", "8.5in", "595pt")
    #[arg(long = "page-width", value_name = "WIDTH")]
    page_width: Option<String>,

    /// Custom page height with unit (e.g. "297mm", "11in", "842pt")
    #[arg(long = "page-height", value_name = "HEIGHT")]
    page_height: Option<String>,

    /// Landscape orientation (swaps width and height)
    #[arg(short = 'L', long = "landscape", default_value_t = false)]
    landscape: bool,

    /// Portrait orientation (explicit, default)
    #[arg(short = 'P', long = "portrait", default_value_t = false)]
    portrait: bool,

    /// Uniform page margin with unit (e.g. "36pt", "0.5in", "10mm")
    #[arg(long = "margin", value_name = "MARGIN")]
    margin: Option<String>,

    /// Top margin (overrides --margin)
    #[arg(long = "margin-top", value_name = "MARGIN")]
    margin_top: Option<String>,

    /// Bottom margin (overrides --margin)
    #[arg(long = "margin-bottom", value_name = "MARGIN")]
    margin_bottom: Option<String>,

    /// Left margin (overrides --margin)
    #[arg(long = "margin-left", value_name = "MARGIN")]
    margin_left: Option<String>,

    /// Right margin (overrides --margin)
    #[arg(long = "margin-right", value_name = "MARGIN")]
    margin_right: Option<String>,

    /// Disable auto font detection (enabled by default).
    /// When enabled, the font-family is chosen based on the detected
    /// language of the document (e.g. Chinese → FangSong 仿宋).
    #[arg(long = "no-auto-font", default_value_t = false)]
    no_auto_font: bool,

    /// Page header text (supports {page} and {total} templates).
    /// Empty string disables the header.
    #[arg(long = "header", value_name = "TEXT")]
    header: Option<String>,

    /// Page footer text (supports {page} and {total} templates).
    /// By default shows page number. Empty string disables the footer.
    #[arg(long = "footer", value_name = "TEXT")]
    footer: Option<String>,

    /// Remove the default page number from the footer.
    /// Equivalent to --footer "".
    #[arg(long = "no-page-number", default_value_t = false)]
    no_page_number: bool,

    /// Enable unlimited height mode (single page, height adapts to content)
    #[arg(long = "height-unlimited", default_value_t = false)]
    height_unlimited: bool,
}

/// Parse a length string with unit (pt, mm, cm, in) into points (pt)
fn parse_length(value: &str) -> Option<f32> {
    let value = value.trim();

    if value == "0" {
        return Some(0.0);
    }

    if let Some(v) = value.strip_suffix("pt") {
        v.trim().parse::<f32>().ok()
    } else if let Some(v) = value.strip_suffix("mm") {
        let mm = v.trim().parse::<f32>().ok()?;
        Some(mm * 72.0 / 25.4)
    } else if let Some(v) = value.strip_suffix("cm") {
        let cm = v.trim().parse::<f32>().ok()?;
        Some(cm * 72.0 / 2.54)
    } else if let Some(v) = value.strip_suffix("in") {
        let inches = v.trim().parse::<f32>().ok()?;
        Some(inches * 72.0)
    } else if let Some(v) = value.strip_suffix("px") {
        v.trim().parse::<f32>().ok()
    } else {
        value.parse::<f32>().ok()
    }
}

/// Resolve a named page size to (width_pt, height_pt)
fn resolve_page_size(name: &str) -> (Option<f32>, Option<f32>) {
    match name.trim().to_ascii_lowercase().as_str() {
        "a3" => (Some(841.890), Some(1190.551)),
        "a4" => (Some(595.276), Some(841.890)),
        "a5" => (Some(419.528), Some(595.276)),
        "a6" => (Some(297.638), Some(419.528)),
        "letter" => (Some(612.0), Some(792.0)),
        "legal" => (Some(612.0), Some(1008.0)),
        "tabloid" | "ledger" => (Some(792.0), Some(1224.0)),
        _ => (None, None),
    }
}

/// Build a PageConfig from CLI args (returns None if no page/header/footer args are given)
fn build_page_config(args: &Args) -> Option<PageConfig> {
    let has_page_args = args.page_size.is_some()
        || args.page_width.is_some()
        || args.page_height.is_some()
        || args.landscape
        || args.portrait
        || args.margin.is_some()
        || args.margin_top.is_some()
        || args.margin_bottom.is_some()
        || args.margin_left.is_some()
        || args.margin_right.is_some()
        || args.header.is_some()
        || args.footer.is_some()
        || args.no_page_number
        || args.height_unlimited;

    if !has_page_args {
        return None;
    }

    let mut config = PageConfig::default();

    // Resolve page size from preset
    if let Some(size) = &args.page_size {
        let (w, h) = resolve_page_size(size);
        config.width = w;
        config.height = h;
    }

    // Custom dimensions override preset
    if let Some(w) = &args.page_width {
        config.width = parse_length(w);
    }
    if let Some(h) = &args.page_height {
        config.height = parse_length(h);
    }

    // Handle orientation
    if args.landscape {
        if let (Some(w), Some(h)) = (config.width, config.height) {
            config.width = Some(w.max(h));
            config.height = Some(w.min(h));
        } else {
            config.width = Some(841.890);
            config.height = Some(595.276);
        }
    } else if args.portrait
        && let (Some(w), Some(h)) = (config.width, config.height)
    {
        config.width = Some(w.min(h));
        config.height = Some(w.max(h));
    }

    // Margins
    if let Some(m) = &args.margin
        && let Some(v) = parse_length(m)
    {
        config.margin_top = Some(v);
        config.margin_bottom = Some(v);
        config.margin_left = Some(v);
        config.margin_right = Some(v);
    }
    if let Some(v) = &args.margin_top {
        config.margin_top = parse_length(v);
    }
    if let Some(v) = &args.margin_bottom {
        config.margin_bottom = parse_length(v);
    }
    if let Some(v) = &args.margin_left {
        config.margin_left = parse_length(v);
    }
    if let Some(v) = &args.margin_right {
        config.margin_right = parse_length(v);
    }

    // ─── 无限高度模式 ──────────────────────────────────
    if args.height_unlimited {
        config.height_unlimited = Some(true);
    }

    // ─── 页眉页脚 ───────────────────────────────────────
    if let Some(header) = &args.header {
        if header.is_empty() {
            config.header = None;
        } else {
            config.header = Some(header.clone());
        }
    }
    if let Some(footer) = &args.footer {
        if footer.is_empty() {
            config.footer = None;
        } else {
            config.footer = Some(footer.clone());
        }
    }
    if args.no_page_number {
        config.footer = None;
    }

    Some(config)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // 推断输入格式
    let input_format = infer_input_format_from_ext(&args.input)
        .ok_or_else(|| format!(
            "Cannot determine input format from extension '{}'. Supported: .md, .markdown, .html, .htm",
            args.input.extension().and_then(|e| e.to_str()).unwrap_or("(none)")
        ))?;

    // 推断输出格式
    let format = args.format.clone().or_else(|| infer_format_from_ext(&args.output))
        .ok_or_else(|| format!(
            "Cannot determine output format from extension '{}'. Use -f/--format or a supported extension (.pdf, .svg, .png, .html).",
            args.output.extension().and_then(|e| e.to_str()).unwrap_or("(none)")
        ))?;

    let mut opts = ConvertOptions::default();
    if let Some(css_path) = &args.style {
        opts.css_file = Some(css_path.clone());
    }
    opts.strict = args.strict;
    if args.no_auto_font {
        opts.auto_font = false;
    }

    if let Some(page_config) = build_page_config(&args) {
        opts.page_config = Some(page_config);
    }

    match (input_format, format) {
        // Markdown → PDF/SVG/PNG
        (InputFormat::Markdown, Format::Pdf) => {
            let pdf_bytes = markdown_file_to_pdf(&args.input, &opts)?;
            std::fs::write(&args.output, pdf_bytes)?;
            println!("PDF saved to: {}", args.output.display());
        }
        (InputFormat::Markdown, Format::Svg) => {
            let svgs = markdown_file_to_svg(&args.input, &opts)?;
            if svgs.is_empty() {
                eprintln!("Warning: no SVG pages generated");
            } else if svgs.len() == 1 {
                std::fs::write(&args.output, &svgs[0])?;
                println!("SVG saved to: {}", args.output.display());
            } else {
                let stem = args
                    .output
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy();
                let ext = args
                    .output
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy();
                let parent = args.output.parent().unwrap_or(std::path::Path::new("."));
                for (i, svg) in svgs.iter().enumerate() {
                    let filename = format!("{}_{}.{}", stem, i + 1, ext);
                    let path = parent.join(&filename);
                    std::fs::write(&path, svg)?;
                    println!("SVG saved to: {}", path.display());
                }
            }
        }
        (InputFormat::Markdown, Format::Png) => {
            let pngs = markdown_file_to_png(&args.input, &opts)?;
            if pngs.len() == 1 {
                std::fs::write(&args.output, &pngs[0])?;
                println!("PNG saved to: {}", args.output.display());
            } else {
                let stem = args
                    .output
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy();
                let ext = args
                    .output
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy();
                let parent = args.output.parent().unwrap_or(std::path::Path::new("."));
                for (i, png) in pngs.iter().enumerate() {
                    let filename = format!("{}_{}.{}", stem, i + 1, ext);
                    let path = parent.join(&filename);
                    std::fs::write(&path, png)?;
                    println!("PNG saved to: {}", path.display());
                }
            }
        }
        // Markdown → HTML
        (InputFormat::Markdown, Format::Html) => {
            let md_content = std::fs::read_to_string(&args.input)?;

            // 读取用户 CSS 文件
            let user_css = if let Some(css_path) = &args.style {
                Some(std::fs::read_to_string(css_path)?)
            } else {
                None
            };

            // 从输入文件名提取 fallback title（去掉扩展名）
            let fallback_title = args
                .input
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string());

            let html = liepress::markdown_to_html_document(
                &md_content,
                user_css.as_deref(),
                args.title.as_deref(),
                fallback_title.as_deref(),
            );
            std::fs::write(&args.output, html)?;
            println!("HTML saved to: {}", args.output.display());
        }

        // HTML → PDF/SVG/PNG
        (InputFormat::Html, Format::Pdf) => {
            let pdf_bytes = html_file_to_pdf(&args.input, &opts)?;
            std::fs::write(&args.output, pdf_bytes)?;
            println!("PDF saved to: {}", args.output.display());
        }
        (InputFormat::Html, Format::Svg) => {
            let svgs = html_file_to_svg(&args.input, &opts)?;
            if svgs.is_empty() {
                eprintln!("Warning: no SVG pages generated");
            } else if svgs.len() == 1 {
                std::fs::write(&args.output, &svgs[0])?;
                println!("SVG saved to: {}", args.output.display());
            } else {
                let stem = args
                    .output
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy();
                let ext = args
                    .output
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy();
                let parent = args.output.parent().unwrap_or(std::path::Path::new("."));
                for (i, svg) in svgs.iter().enumerate() {
                    let filename = format!("{}_{}.{}", stem, i + 1, ext);
                    let path = parent.join(&filename);
                    std::fs::write(&path, svg)?;
                    println!("SVG saved to: {}", path.display());
                }
            }
        }
        (InputFormat::Html, Format::Png) => {
            let pngs = html_file_to_png(&args.input, &opts)?;
            if pngs.len() == 1 {
                std::fs::write(&args.output, &pngs[0])?;
                println!("PNG saved to: {}", args.output.display());
            } else {
                let stem = args
                    .output
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy();
                let ext = args
                    .output
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy();
                let parent = args.output.parent().unwrap_or(std::path::Path::new("."));
                for (i, png) in pngs.iter().enumerate() {
                    let filename = format!("{}_{}.{}", stem, i + 1, ext);
                    let path = parent.join(&filename);
                    std::fs::write(&path, png)?;
                    println!("PNG saved to: {}", path.display());
                }
            }
        }
        // HTML → HTML（直接复制）
        (InputFormat::Html, Format::Html) => {
            let html_content = std::fs::read_to_string(&args.input)?;
            std::fs::write(&args.output, html_content)?;
            println!("HTML saved to: {}", args.output.display());
        }
    }

    Ok(())
}
