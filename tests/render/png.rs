//! PNG 渲染测试

use liepress::markdown_to_png;
use crate::common::{test_output_dir, save_test_output};
use crate::common::samples;

#[test]
fn test_png_generation_basic() {
    let output_dir = test_output_dir("png_basic");

    let pngs = markdown_to_png(samples::BASIC).expect("PNG generation should succeed");

    // Should have at least one page
    assert!(!pngs.is_empty(), "Should have at least one PNG page");

    for (i, png_data) in pngs.iter().enumerate() {
        // PNG should have some content
        assert!(!png_data.is_empty(), "PNG page {} should not be empty", i);

        // Should start with PNG signature
        assert_eq!(
            &png_data[0..8],
            &[137, 80, 78, 71, 13, 10, 26, 10],
            "Should be a valid PNG file"
        );

        // Save for manual inspection
        let output_path = output_dir.join(format!("test_basic_page_{}.png", i));
        save_test_output(&output_path, png_data);
    }
}

#[test]
fn test_png_with_various_elements() {
    let output_dir = test_output_dir("png_various");

    let pngs = markdown_to_png(samples::FULL_FEATURED).expect("PNG generation should succeed");

    for (i, png_data) in pngs.iter().enumerate() {
        assert_eq!(
            &png_data[0..8],
            &[137, 80, 78, 71, 13, 10, 26, 10],
            "Page {} should be valid PNG",
            i
        );

        let output_path = output_dir.join(format!("test_various_page_{}.png", i));
        save_test_output(&output_path, png_data);
    }
}
