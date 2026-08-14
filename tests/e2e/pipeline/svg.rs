//! 管线输出：SVG。

use liepress::ConvertOptions;
use liepress::markdown_to_svg;

fn opts() -> ConvertOptions {
    ConvertOptions::new().with_auto_font(false)
}

#[test]
fn svg_full_pipeline() {
    let md = "# 标题\n\n这是一段正文内容。\n";
    let svg = markdown_to_svg(md, &opts()).expect("转换应成功");
    assert!(!svg.is_empty());
    assert!(
        svg.trim_start().starts_with("<?xml") || svg.contains("<svg"),
        "应输出 SVG 内容"
    );
}

#[test]
fn svg_unicode_content() {
    let md = "# 测试\n\n中文、日本語、한국어 与 emoji 🚀 文本。\n";
    let svg = markdown_to_svg(md, &opts()).expect("转换应成功");
    assert!(!svg.is_empty());
    assert!(svg.contains("<svg"));
}

#[test]
fn svg_special_characters() {
    let md = "# 特殊字符\n\n& < > \" ' © ® ™ § ¶ • — – … € £ ¥\n";
    let svg = markdown_to_svg(md, &opts()).expect("转换应成功");
    assert!(!svg.is_empty());
    assert!(svg.contains("<svg"));
}

#[test]
fn svg_nested_structure() {
    let md = "# 一级\n\n## 二级\n\n- 列表项\n  - 嵌套项\n\n> 引用文本\n";
    let svg = markdown_to_svg(md, &opts()).expect("转换应成功");
    assert!(!svg.is_empty());
    assert!(svg.contains("<svg"));
}
