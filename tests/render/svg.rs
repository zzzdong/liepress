//! SVG 渲染测试

use crate::common::samples;
use crate::common::{save_test_output, test_output_dir};
use liepress::markdown_to_svg;

#[test]
fn test_svg_generation_basic() {
    let output_dir = test_output_dir("svg_basic");

    let svgs = markdown_to_svg(samples::BASIC).expect("SVG generation should succeed");

    // Should have at least one page
    assert!(!svgs.is_empty(), "Should have at least one SVG page");

    for (i, svg_data) in svgs.iter().enumerate() {
        // SVG should have some content
        assert!(!svg_data.is_empty(), "SVG page {} should not be empty", i);

        // Should contain SVG tags
        assert!(svg_data.contains("<svg"), "Should contain SVG opening tag");
        assert!(
            svg_data.contains("</svg>"),
            "Should contain SVG closing tag"
        );

        // Save for manual inspection
        let output_path = output_dir.join(format!("test_basic_page_{}.svg", i));
        save_test_output(&output_path, svg_data.as_bytes());
    }
}

#[test]
fn test_svg_with_various_elements() {
    let output_dir = test_output_dir("svg_various");

    let svgs = markdown_to_svg(samples::FULL_FEATURED).expect("SVG generation should succeed");

    for (i, svg_data) in svgs.iter().enumerate() {
        assert!(svg_data.contains("<svg"), "Page {} should contain SVG", i);

        let output_path = output_dir.join(format!("test_various_page_{}.svg", i));
        save_test_output(&output_path, svg_data.as_bytes());
    }
}

#[test]
fn test_svg_multipage() {
    // Create a long document that should span multiple pages
    let mut md = String::new();
    for i in 0..50 {
        md.push_str(&format!(
            "# Heading {}\n\nParagraph {} with some text content.\n\n",
            i, i
        ));
    }

    let output_dir = test_output_dir("svg_multipage");
    let svgs = markdown_to_svg(&md).expect("SVG generation should succeed");

    // Should have multiple pages
    assert!(
        svgs.len() > 1,
        "Long document should span multiple SVG pages, got {} pages",
        svgs.len()
    );

    for (i, svg_data) in svgs.iter().enumerate() {
        let output_path = output_dir.join(format!("page_{}.svg", i));
        save_test_output(&output_path, svg_data.as_bytes());
    }
}
