//! PDF 渲染测试

use liepress::markdown_to_pdf;
use crate::common::{test_output_dir, save_test_output};
use crate::common::samples;

#[test]
fn test_pdf_generation_basic() {
    let output_dir = test_output_dir("pdf_basic");

    let pdf_data = markdown_to_pdf(samples::BASIC).expect("PDF generation should succeed");

    // PDF should have some content
    assert!(!pdf_data.is_empty(), "PDF should not be empty");

    // Should start with PDF header
    assert_eq!(&pdf_data[0..4], b"%PDF", "Should be a valid PDF file");

    // Save for manual inspection
    let output_path = output_dir.join("test_basic.pdf");
    save_test_output(&output_path, &pdf_data);
}

#[test]
fn test_pdf_with_various_elements() {
    let output_dir = test_output_dir("pdf_various");

    let pdf_data = markdown_to_pdf(samples::FULL_FEATURED).expect("PDF generation should succeed");

    assert!(!pdf_data.is_empty(), "PDF should not be empty");
    assert_eq!(&pdf_data[0..4], b"%PDF", "Should be a valid PDF file");

    let output_path = output_dir.join("test_various.pdf");
    save_test_output(&output_path, &pdf_data);
}

#[test]
fn test_pdf_code_block() {
    let output_dir = test_output_dir("pdf_code");

    let pdf_data = markdown_to_pdf(samples::CODE_BLOCK).expect("PDF generation should succeed");

    let output_path = output_dir.join("test_code.pdf");
    save_test_output(&output_path, &pdf_data);
}

#[test]
fn test_pdf_list() {
    let output_dir = test_output_dir("pdf_list");

    let pdf_data = markdown_to_pdf(samples::NESTED_LIST).expect("PDF generation should succeed");

    let output_path = output_dir.join("test_list.pdf");
    save_test_output(&output_path, &pdf_data);
}

#[test]
fn test_pdf_ordered_list() {
    let output_dir = test_output_dir("pdf_ordered");

    let pdf_data = markdown_to_pdf(samples::ORDERED_LIST).expect("PDF generation should succeed");

    let output_path = output_dir.join("test_ordered.pdf");
    save_test_output(&output_path, &pdf_data);
}
