use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use liepress::{
    markdown_file_to_pdf_with_options, markdown_file_to_svg_with_options,
    markdown_file_to_png_with_options, ConvertOptions, PageConfig,
};

#[derive(ValueEnum, Clone, Debug)]
enum Format {
    Pdf,
    Svg,
    Png,
}

/// Markdown to PDF/SVG converter
#[derive(Parser, Debug)]
#[command(name = "liepress")]
#[command(about = "Convert Markdown to PDF, SVG or PNG")]
struct Args {
    /// Input Markdown file path
    #[arg(short, long, value_name = "FILE")]
    input: PathBuf,

    /// Output file path
    #[arg(short, long, value_name = "FILE")]
    output: PathBuf,

    /// Output format
    #[arg(short, long, value_enum, default_value = "pdf")]
    format: Format,

    /// Optional CSS stylesheet file to override default styles
    #[arg(short = 's', long = "style", value_name = "CSS_FILE")]
    style: Option<PathBuf>,

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
    /// language of the document (e.g. Chinese → Noto Serif SC, SimSun).
    #[arg(long = "no-auto-font", default_value_t = false)]
    no_auto_font: bool,
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

/// Build a PageConfig from CLI args (returns None if no page args are given)
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
        || args.margin_right.is_some();

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
        && let (Some(w), Some(h)) = (config.width, config.height) {
            config.width = Some(w.min(h));
            config.height = Some(w.max(h));
        }

    // Margins
    if let Some(m) = &args.margin
        && let Some(v) = parse_length(m) {
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

    Some(config)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

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

    match args.format {
        Format::Pdf => {
            let pdf_bytes = markdown_file_to_pdf_with_options(&args.input, &opts)?;
            std::fs::write(&args.output, pdf_bytes)?;
            println!("PDF saved to: {}", args.output.display());
        }
        Format::Svg => {
            let svgs = markdown_file_to_svg_with_options(&args.input, &opts)?;
            if svgs.len() == 1 {
                std::fs::write(&args.output, &svgs[0])?;
                println!("SVG saved to: {}", args.output.display());
            } else {
                let stem = args.output.file_stem().unwrap_or_default().to_string_lossy();
                let ext = args.output.extension().unwrap_or_default().to_string_lossy();
                let parent = args.output.parent().unwrap_or(std::path::Path::new("."));
                for (i, svg) in svgs.iter().enumerate() {
                    let filename = format!("{}_{}.{}", stem, i + 1, ext);
                    let path = parent.join(&filename);
                    std::fs::write(&path, svg)?;
                    println!("SVG saved to: {}", path.display());
                }
            }
        }
        Format::Png => {
            let pngs = markdown_file_to_png_with_options(&args.input, &opts)?;
            if pngs.len() == 1 {
                std::fs::write(&args.output, &pngs[0])?;
                println!("PNG saved to: {}", args.output.display());
            } else {
                let stem = args.output.file_stem().unwrap_or_default().to_string_lossy();
                let ext = args.output.extension().unwrap_or_default().to_string_lossy();
                let parent = args.output.parent().unwrap_or(std::path::Path::new("."));
                for (i, png) in pngs.iter().enumerate() {
                    let filename = format!("{}_{}.{}", stem, i + 1, ext);
                    let path = parent.join(&filename);
                    std::fs::write(&path, png)?;
                    println!("PNG saved to: {}", path.display());
                }
            }
        }
    }

    Ok(())
}