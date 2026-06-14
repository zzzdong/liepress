//! PDF 渲染测试

use crate::common::samples;
use crate::common::{assert_valid_pdf, pdf_page_count, save_test_output, test_output_dir};
use liepress::ConvertOptions;
use liepress::markdown_to_pdf;

#[test]
fn test_pdf_generation_basic() {
    let output_dir = test_output_dir("pdf_basic");

    let pdf_data = markdown_to_pdf(samples::BASIC, &ConvertOptions::default())
        .expect("PDF generation should succeed");
    let _doc = assert_valid_pdf(&pdf_data);
    let pages = pdf_page_count(&pdf_data);

    assert_eq!(pages, 1, "Basic doc should be 1 page");

    let output_path = output_dir.join("test_basic.pdf");
    save_test_output(&output_path, &pdf_data);
}

#[test]
fn test_pdf_with_various_elements() {
    let output_dir = test_output_dir("pdf_various");

    let pdf_data = markdown_to_pdf(samples::FULL_FEATURED, &ConvertOptions::default())
        .expect("PDF generation should succeed");
    let _doc = assert_valid_pdf(&pdf_data);

    let output_path = output_dir.join("test_various.pdf");
    save_test_output(&output_path, &pdf_data);
}

#[test]
fn test_pdf_code_block() {
    let output_dir = test_output_dir("pdf_code");

    let pdf_data = markdown_to_pdf(samples::CODE_BLOCK, &ConvertOptions::default())
        .expect("PDF generation should succeed");
    let _doc = assert_valid_pdf(&pdf_data);

    let output_path = output_dir.join("test_code.pdf");
    save_test_output(&output_path, &pdf_data);
}

#[test]
fn test_pdf_list() {
    let output_dir = test_output_dir("pdf_list");

    let pdf_data = markdown_to_pdf(samples::NESTED_LIST, &ConvertOptions::default())
        .expect("PDF generation should succeed");
    let _doc = assert_valid_pdf(&pdf_data);

    let output_path = output_dir.join("test_list.pdf");
    save_test_output(&output_path, &pdf_data);
}

#[test]
fn test_pdf_ordered_list() {
    let output_dir = test_output_dir("pdf_ordered");

    let pdf_data = markdown_to_pdf(samples::ORDERED_LIST, &ConvertOptions::default())
        .expect("PDF generation should succeed");
    let _doc = assert_valid_pdf(&pdf_data);

    let output_path = output_dir.join("test_ordered.pdf");
    save_test_output(&output_path, &pdf_data);
}
