//! liepress CLI - Markdown to PDF/SVG converter

use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use liepress::{markdown_file_to_pdf, markdown_file_to_svg, markdown_file_to_png};

#[derive(ValueEnum, Clone, Debug)]
enum Format {
    Pdf,
    Svg,
    Png,
}

/// Markdown to PDF/SVG converter
#[derive(Parser, Debug)]
#[command(name = "liepress")]
#[command(about = "Convert Markdown to PDF or SVG")]
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
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    match args.format {
        Format::Pdf => {
            let pdf_bytes = markdown_file_to_pdf(&args.input)?;
            std::fs::write(&args.output, pdf_bytes)?;
            println!("PDF saved to: {}", args.output.display());
        }
        Format::Svg => {
            let svgs = markdown_file_to_svg(&args.input)?;
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
            let pngs = markdown_file_to_png(&args.input)?;
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