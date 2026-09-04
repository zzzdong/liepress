use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};
use liepress::{ConvertOptions, PageConfig, html_file_to_pdf, markdown_file_to_pdf};

#[derive(ValueEnum, Clone, Debug)]
enum Format {
    Pdf,
    Html,
    Svg,
    Png,
    Docx,
}

#[derive(ValueEnum, Clone, Debug)]
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
        Some("html") | Some("htm") => Some(Format::Html),
        Some("svg") => Some(Format::Svg),
        Some("png") => Some(Format::Png),
        Some("docx") => Some(Format::Docx),
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

/// Markdown/HTML to PDF/HTML converter
#[derive(Parser, Debug)]
#[command(name = "liepress")]
#[command(about = "Convert Markdown or HTML to PDF or HTML")]
struct Args {
    /// Input file path (Markdown: .md, .markdown; HTML: .html, .htm).
    /// Use `-` to read from stdin.
    #[arg(short, long, value_name = "FILE")]
    input: PathBuf,

    /// Output file path (format inferred from extension: .pdf, .html, .svg, .png, .docx).
    /// Use `-` to write to stdout.
    #[arg(short, long, value_name = "FILE")]
    output: PathBuf,

    /// Output format (overrides extension-based inference)
    #[arg(short, long, value_enum)]
    format: Option<Format>,

    /// Input format (required when reading from stdin via `-`, otherwise inferred from extension)
    #[arg(short = 'F', long = "from", value_enum)]
    input_format: Option<InputFormat>,

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

/// Parse a length string with unit (pt, mm, cm, in) into points (pt).
///
/// Rejects non-finite values (`NaN`/`inf`) 与负值——页面几何（尺寸/边距）
/// 均无负值语义，非有限值/负值进入几何会使输出 PDF 尺寸/坐标异常（S-4）。
fn parse_length(value: &str) -> Option<f32> {
    let value = value.trim();

    if value == "0" {
        return Some(0.0);
    }

    let parse_finite = |s: &str| -> Option<f32> {
        s.trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 0.0)
    };

    if let Some(v) = value.strip_suffix("pt") {
        parse_finite(v)
    } else if let Some(v) = value.strip_suffix("mm") {
        Some(parse_finite(v)? * 72.0 / 25.4)
    } else if let Some(v) = value.strip_suffix("cm") {
        Some(parse_finite(v)? * 72.0 / 2.54)
    } else if let Some(v) = value.strip_suffix("in") {
        Some(parse_finite(v)? * 72.0)
    } else if let Some(v) = value.strip_suffix("px") {
        parse_finite(v)
    } else {
        parse_finite(value)
    }
}

/// Resolve a named page size to (width_pt, height_pt)。
///
/// 未知预设返回错误（用户拼错预设名不应静默回退默认页尺寸）。
fn resolve_page_size(name: &str) -> Result<(Option<f32>, Option<f32>), String> {
    match name.trim().to_ascii_lowercase().as_str() {
        "a3" => Ok((Some(841.890), Some(1190.551))),
        "a4" => Ok((Some(595.276), Some(841.890))),
        "a5" => Ok((Some(419.528), Some(595.276))),
        "a6" => Ok((Some(297.638), Some(419.528))),
        "letter" => Ok((Some(612.0), Some(792.0))),
        "legal" => Ok((Some(612.0), Some(1008.0))),
        "tabloid" | "ledger" => Ok((Some(792.0), Some(1224.0))),
        other => Err(format!(
            "Unknown page size preset '{other}'. Supported: A3, A4, A5, A6, Letter, Legal, Tabloid"
        )),
    }
}

/// 解析带单位的长度参数；解析失败时返回带参数名的错误（不静默吞掉）。
fn parse_length_arg(arg_name: &str, value: &str) -> Result<f32, String> {
    parse_length(value)
        .ok_or_else(|| format!("Invalid {arg_name} value '{value}'. Use e.g. 36pt, 10mm, 0.5in"))
}

/// Build a PageConfig from CLI args。
///
/// 返回 `None` 表示未提供任何页面相关参数；参数非法（未知预设/无法解析的
/// 长度）时返回 `Err`，由 main 转为退出错误——不静默回退默认值。
fn build_page_config(args: &Args) -> Result<Option<PageConfig>, String> {
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
        return Ok(None);
    }

    let mut config = PageConfig::default();

    // Resolve page size from preset
    if let Some(size) = &args.page_size {
        let (w, h) = resolve_page_size(size)?;
        config.width = w;
        config.height = h;
    }

    // Custom dimensions override preset
    if let Some(w) = &args.page_width {
        config.width = Some(parse_length_arg("--page-width", w)?);
    }
    if let Some(h) = &args.page_height {
        config.height = Some(parse_length_arg("--page-height", h)?);
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
    if let Some(m) = &args.margin {
        let v = parse_length_arg("--margin", m)?;
        config.margin_top = Some(v);
        config.margin_bottom = Some(v);
        config.margin_left = Some(v);
        config.margin_right = Some(v);
    }
    if let Some(v) = &args.margin_top {
        config.margin_top = Some(parse_length_arg("--margin-top", v)?);
    }
    if let Some(v) = &args.margin_bottom {
        config.margin_bottom = Some(parse_length_arg("--margin-bottom", v)?);
    }
    if let Some(v) = &args.margin_left {
        config.margin_left = Some(parse_length_arg("--margin-left", v)?);
    }
    if let Some(v) = &args.margin_right {
        config.margin_right = Some(parse_length_arg("--margin-right", v)?);
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

    Ok(Some(config))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // stdin 输入：输入路径为 `-`
    let from_stdin = args.input.as_os_str() == "-";
    // stdout 输出：输出路径为 `-`
    let to_stdout = args.output.as_os_str() == "-";

    // 推断输入格式（stdin 时需显式 --from，否则默认 Markdown）
    let input_format = if from_stdin {
        args.input_format.clone().unwrap_or(InputFormat::Markdown)
    } else {
        infer_input_format_from_ext(&args.input).ok_or_else(|| {
            format!(
                "Cannot determine input format from extension '{}'. Supported: .md, .markdown, .html, .htm",
                args.input.extension().and_then(|e| e.to_str()).unwrap_or("(none)")
            )
        })?
    };

    // 推断输出格式
    let format = args.format.clone().or_else(|| infer_format_from_ext(&args.output))
        .ok_or_else(|| format!(
            "Cannot determine output format from extension '{}'. Use -f/--format or a supported extension (.pdf, .html, .svg, .png, .docx).",
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

    if let Some(page_config) = build_page_config(&args)? {
        opts.page_config = Some(page_config);
    }

    // 统一输出：stdout 时写标准输出，否则写文件
    let emit = |bytes: &[u8]| -> Result<(), Box<dyn std::error::Error>> {
        if to_stdout {
            use std::io::Write;
            let mut out = std::io::stdout().lock();
            out.write_all(bytes)?;
            out.flush()?;
        } else {
            std::fs::write(&args.output, bytes)?;
            println!("Saved to: {}", args.output.display());
        }
        Ok(())
    };

    // stdin 输入：提前读取全部内容（字符串版 API 无法解析相对路径本地图片，
    // 图片需以 data URI 形式提供，符合「stdin 无文件路径上下文」的约束）。
    // 非 stdin 输入时恒为空串——各分支仅在 from_stdin 为真时消费它。
    use std::io::Read;
    let stdin_content: String = if from_stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("Failed to read stdin: {e}"))?;
        buf
    } else {
        String::new()
    };

    // 读取本地输入文本（文件输入用）；stdin 输入直接复用 stdin_content
    let read_input = || -> Result<String, Box<dyn std::error::Error>> {
        if from_stdin {
            Ok(stdin_content.clone())
        } else {
            Ok(std::fs::read_to_string(&args.input)?)
        }
    };
    match (input_format, format) {
        // Markdown → PDF
        (InputFormat::Markdown, Format::Pdf) => {
            let pdf_bytes = if from_stdin {
                liepress::markdown_to_pdf(&stdin_content, &opts)?
            } else {
                markdown_file_to_pdf(&args.input, &opts)?
            };
            emit(&pdf_bytes)?;
        }
        // Markdown → HTML
        (InputFormat::Markdown, Format::Html) => {
            let md_content = read_input()?;

            // 读取用户 CSS 文件
            let user_css = if let Some(css_path) = &args.style {
                Some(std::fs::read_to_string(css_path)?)
            } else {
                None
            };

            // 文件输入时从文件名提取 fallback title（去掉扩展名）；stdin 时不提供
            let fallback_title = if from_stdin {
                None
            } else {
                args.input
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            };

            let html = liepress::markdown_to_html_document(
                &md_content,
                user_css.as_deref(),
                args.title.as_deref(),
                fallback_title.as_deref(),
            );
            emit(html.as_bytes())?;
        }

        // HTML → PDF
        (InputFormat::Html, Format::Pdf) => {
            let pdf_bytes = if from_stdin {
                liepress::html_to_pdf(stdin_content.as_str(), &opts)?
            } else {
                html_file_to_pdf(&args.input, &opts)?
            };
            emit(&pdf_bytes)?;
        }
        // HTML → HTML（直接复制）
        (InputFormat::Html, Format::Html) => {
            let html_content = read_input()?;
            emit(html_content.as_bytes())?;
        }

        // Markdown → SVG / PNG / DOCX
        (InputFormat::Markdown, Format::Svg) => {
            let svg = if from_stdin {
                liepress::markdown_to_svg(stdin_content.as_str(), &opts)?
            } else {
                liepress::markdown_file_to_svg(&args.input, &opts)?
            };
            emit(svg.as_bytes())?;
        }
        (InputFormat::Markdown, Format::Png) => {
            let png = if from_stdin {
                liepress::markdown_to_png(stdin_content.as_str(), &opts)?
            } else {
                liepress::markdown_file_to_png(&args.input, &opts)?
            };
            emit(&png)?;
        }
        (InputFormat::Markdown, Format::Docx) => {
            // 用字符串版本（stdin 无相对路径图片上下文）；文件版为兼容保留
            let docx = if from_stdin {
                liepress::markdown_to_docx(stdin_content.as_str(), &opts)?
            } else {
                liepress::markdown_file_to_docx(&args.input, &opts)?
            };
            emit(&docx)?;
        }

        // HTML → SVG / PNG / DOCX
        (InputFormat::Html, Format::Svg) => {
            let svg = if from_stdin {
                liepress::html_to_svg(stdin_content.as_str(), &opts)?
            } else {
                liepress::html_file_to_svg(&args.input, &opts)?
            };
            emit(svg.as_bytes())?;
        }
        (InputFormat::Html, Format::Png) => {
            let png = if from_stdin {
                liepress::html_to_png(stdin_content.as_str(), &opts)?
            } else {
                liepress::html_file_to_png(&args.input, &opts)?
            };
            emit(&png)?;
        }
        (InputFormat::Html, Format::Docx) => {
            // 与其余分支一致：stdin 用字符串版（无相对路径图片上下文），
            // 文件输入用文件版（可内嵌相对路径图片）；输出统一走 emit()
            // （此前该分支把 `-o -` 写成名为 `-` 的文件、stdin 输入直接报错）。
            let docx = if from_stdin {
                liepress::html_to_docx(&stdin_content, &opts)?
            } else {
                liepress::html_file_to_docx(&args.input, &opts)?
            };
            emit(&docx)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    fn args(extra: &[&str]) -> Args {
        let mut argv = vec!["liepress", "-i", "in.md", "-o", "out.pdf"];
        argv.extend_from_slice(extra);
        Args::parse_from(argv)
    }

    // ─── parse_length ───

    #[test]
    fn parse_length_accepts_units_and_zero() {
        assert_eq!(parse_length("36pt"), Some(36.0));
        assert_eq!(parse_length("10mm"), Some((10.0 * 72.0 / 25.4)));
        assert!((parse_length("0.5in").unwrap() - 36.0).abs() < 1e-4);
        assert_eq!(parse_length("0"), Some(0.0));
        assert_eq!(parse_length("595"), Some(595.0));
    }

    #[test]
    fn parse_length_rejects_invalid_negative_and_non_finite() {
        assert_eq!(parse_length("abc"), None);
        assert_eq!(parse_length("NaN"), None);
        assert_eq!(parse_length("inf"), None);
        assert_eq!(parse_length("-5pt"), None, "页面几何不接受负值");
        assert_eq!(parse_length("-2in"), None);
    }

    // ─── resolve_page_size ───

    #[test]
    fn resolve_page_size_known_presets() {
        let (w, h) = resolve_page_size("A4").unwrap();
        assert_eq!((w, h), (Some(595.276), Some(841.890)));
        let (w, _) = resolve_page_size("letter").unwrap();
        assert_eq!(w, Some(612.0));
    }

    #[test]
    fn resolve_page_size_unknown_is_error() {
        assert!(resolve_page_size("A7").is_err(), "未知预设应报错而非静默回退");
    }

    // ─── build_page_config ───

    #[test]
    fn build_page_config_none_without_args() {
        assert!(build_page_config(&args(&[])).unwrap().is_none());
    }

    #[test]
    fn build_page_config_rejects_bad_margin() {
        let e = build_page_config(&args(&["--margin-top", "abc"]))
            .expect_err("非法边距应报错而非静默回退默认值");
        assert!(e.contains("--margin-top"), "错误信息应包含参数名: {e}");
    }

    #[test]
    fn build_page_config_rejects_bad_page_size() {
        assert!(build_page_config(&args(&["--page-size", "A7"])).is_err());
    }

    #[test]
    fn build_page_config_rejects_negative_width() {
        assert!(
            build_page_config(&args(&["--page-width=-5pt"])).is_err(),
            "负页面宽度应报错"
        );
    }

    #[test]
    fn build_page_config_merges_size_and_margins() {
        let config = build_page_config(&args(&["--page-size", "a5", "--margin", "24pt"]))
            .unwrap()
            .expect("有页面参数时应返回 Some");
        assert_eq!(config.width, Some(419.528));
        assert_eq!(config.height, Some(595.276));
        assert_eq!(config.margin_top, Some(24.0));
    }

    #[test]
    fn build_page_config_landscape_swaps() {
        let config = build_page_config(&args(&["--page-size", "a4", "--landscape"]))
            .unwrap()
            .expect("Some");
        assert_eq!(config.width, Some(841.890));
        assert_eq!(config.height, Some(595.276));
    }
}
