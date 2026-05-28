pub mod error;
pub mod generator;
pub mod render;
pub mod ast;
pub mod text;
pub mod visual;

use std::path::Path;
use std::fs;

pub use render::{PixmapDocumentGenerator, PixmapRenderer, SvgDocumentGenerator, SvgRenderer, PdfDocumentGenerator, PdfRenderer, PageRenderer};

use generator::{markdown_to_document, markdown_to_document_with_base_dir};

pub fn markdown_to_pdf(markdown: &str) -> crate::error::Result<Vec<u8>> {
    let document = markdown_to_document(markdown);
    let mut pdf_gen = PdfDocumentGenerator::new("output".to_string());
    for page in &document.pages {
        pdf_gen.render_page(page)?;
    }
    pdf_gen.finalize()
}

/// 从 Markdown 文件路径生成 PDF，自动使用文件所在目录作为图片基础路径
pub fn markdown_file_to_pdf(path: &Path) -> crate::error::Result<Vec<u8>> {
    let markdown = fs::read_to_string(path)?;
    let base_dir = path.parent().map(|p| p.to_path_buf());
    let document = markdown_to_document_with_base_dir(&markdown, base_dir);
    let mut pdf_gen = PdfDocumentGenerator::new("output".to_string());
    for page in &document.pages {
        pdf_gen.render_page(page)?;
    }
    pdf_gen.finalize()
}

pub fn markdown_to_svg(markdown: &str) -> crate::error::Result<Vec<String>> {
    let document = markdown_to_document(markdown);
    let mut svgs = Vec::new();
    for page in &document.pages {
        let mut renderer = SvgRenderer::new(page.width, page.height);
        renderer.render_elements(&page.elements);
        svgs.push(renderer.finalize());
    }
    Ok(svgs)
}

pub fn markdown_file_to_svg(path: &Path) -> crate::error::Result<Vec<String>> {
    let markdown = fs::read_to_string(path)?;
    let base_dir = path.parent().map(|p| p.to_path_buf());
    let document = markdown_to_document_with_base_dir(&markdown, base_dir);
    let mut svgs = Vec::new();
    for page in &document.pages {
        let mut renderer = SvgRenderer::new(page.width, page.height);
        renderer.render_elements(&page.elements);
        svgs.push(renderer.finalize());
    }
    Ok(svgs)
}

pub fn markdown_to_png(markdown: &str) -> crate::error::Result<Vec<Vec<u8>>> {
    let document = markdown_to_document(markdown);
    let mut pngs = Vec::new();
    for page in &document.pages {
        let mut renderer = PixmapRenderer::new_default_dpi(page.width, page.height);
        renderer.render_elements(&page.elements);
        pngs.push(renderer.render_to_png()?);
    }
    Ok(pngs)
}

pub fn markdown_file_to_png(path: &Path) -> crate::error::Result<Vec<Vec<u8>>> {
    let markdown = fs::read_to_string(path)?;
    let base_dir = path.parent().map(|p| p.to_path_buf());
    let document = markdown_to_document_with_base_dir(&markdown, base_dir);
    let mut pngs = Vec::new();
    for page in &document.pages {
        let mut renderer = PixmapRenderer::new_default_dpi(page.width, page.height);
        renderer.render_elements(&page.elements);
        pngs.push(renderer.render_to_png()?);
    }
    Ok(pngs)
}