use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use liepress::{
    markdown_file_to_pdf, markdown_file_to_svg, markdown_file_to_png,
    markdown_file_to_pdf_with_options, markdown_file_to_svg_with_options,
    markdown_file_to_png_with_options, ConvertOptions,
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
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let mut opts = ConvertOptions::default();
    if let Some(css_path) = &args.style {
        opts.css_file = Some(css_path.clone());
    }
    opts.strict = args.strict;

    match args.format {
        Format::Pdf => {
            let pdf_bytes = if opts.css_file.is_some() || opts.strict {
                markdown_file_to_pdf_with_options(&args.input, &opts)?
            } else {
                markdown_file_to_pdf(&args.input)?
            };
            std::fs::write(&args.output, pdf_bytes)?;
            println!("PDF saved to: {}", args.output.display());
        }
        Format::Svg => {
            let svgs = if opts.css_file.is_some() || opts.strict {
                markdown_file_to_svg_with_options(&args.input, &opts)?
            } else {
                markdown_file_to_svg(&args.input)?
            };
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
            let pngs = if opts.css_file.is_some() || opts.strict {
                markdown_file_to_png_with_options(&args.input, &opts)?
            } else {
                markdown_file_to_png(&args.input)?
            };
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