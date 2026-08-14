//! 管线输出：PNG（含图片资源）。

use liepress::ConvertOptions;
use liepress::markdown_to_png;

fn opts() -> ConvertOptions {
    ConvertOptions::new().with_auto_font(false)
}

#[test]
fn png_full_pipeline() {
    let md = "# 标题\n\n这是一段正文内容。\n";
    let bytes = markdown_to_png(md, &opts()).expect("转换应成功");
    assert!(!bytes.is_empty());
    // PNG 文件签名: 89 50 4E 47
    assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47], "应生成合法 PNG");
}

#[test]
fn png_unicode_content() {
    let md = "# 测试\n\n中文、日本語、한국어 与 emoji 🚀 文本。\n";
    let bytes = markdown_to_png(md, &opts()).expect("转换应成功");
    assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
}

#[test]
fn png_special_characters() {
    let md = "# 特殊字符\n\n& < > \" ' © ® ™ § ¶ • — – … € £ ¥\n";
    let bytes = markdown_to_png(md, &opts()).expect("转换应成功");
    assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
}

#[test]
fn png_nested_structure() {
    let md = "# 一级\n\n## 二级\n\n- 列表项\n  - 嵌套项\n\n> 引用文本\n";
    let bytes = markdown_to_png(md, &opts()).expect("转换应成功");
    assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
}

#[test]
fn png_image_data_uri() {
    let md = "![alt](data:image/png;base64,iVBORw0KGgo=)\n";
    let bytes = markdown_to_png(md, &opts()).expect("含 data URI 图片的转换应成功");
    assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
}

#[test]
fn png_image_local() {
    let md = "![alt](tests/fixtures/test_image.png)\n";
    let bytes = markdown_to_png(md, &opts()).expect("含本地图片的转换应成功");
    assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
}

#[test]
fn png_image_scaling() {
    // 缩放属性不应导致转换失败，且仍输出 PNG
    let md = "<img src=\"tests/fixtures/test_image.png\" width=\"50%\" height=\"50\">\n";
    let bytes = markdown_to_png(md, &opts()).expect("含缩放图片的转换应成功");
    assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47]);
}
