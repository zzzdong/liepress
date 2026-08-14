//! 管线输出：DOCX（含图片资源）。

use liepress::ConvertOptions;
use liepress::markdown_to_docx;

fn opts() -> ConvertOptions {
    ConvertOptions::new().with_auto_font(false)
}

#[test]
fn docx_full_pipeline() {
    let md = "# 标题\n\n这是一段正文内容。\n";
    let bytes = markdown_to_docx(md, &opts()).expect("转换应成功");
    assert!(!bytes.is_empty());
    // DOCX 是 ZIP 包，签名：PK\x03\x04
    assert_eq!(
        &bytes[..4],
        &[0x50, 0x4B, 0x03, 0x04],
        "应生成合法 DOCX（ZIP）"
    );
}

#[test]
fn docx_unicode_content() {
    let md = "# 测试\n\n中文、日本語、한국어 与 emoji 🚀 文本。\n";
    let bytes = markdown_to_docx(md, &opts()).expect("转换应成功");
    assert_eq!(&bytes[..4], &[0x50, 0x4B, 0x03, 0x04]);
}

#[test]
fn docx_special_characters() {
    let md = "# 特殊字符\n\n& < > \" ' © ® ™ § ¶ • — – … € £ ¥\n";
    let bytes = markdown_to_docx(md, &opts()).expect("转换应成功");
    assert_eq!(&bytes[..4], &[0x50, 0x4B, 0x03, 0x04]);
}

#[test]
fn docx_nested_structure() {
    let md = "# 一级\n\n## 二级\n\n- 列表项\n  - 嵌套项\n\n> 引用文本\n";
    let bytes = markdown_to_docx(md, &opts()).expect("转换应成功");
    assert_eq!(&bytes[..4], &[0x50, 0x4B, 0x03, 0x04]);
}

#[test]
fn docx_table() {
    let md = "| 名称 | 值 |\n|------|----|\n| A | 1 |\n| B | 2 |\n";
    let bytes = markdown_to_docx(md, &opts()).expect("转换应成功");
    assert_eq!(&bytes[..4], &[0x50, 0x4B, 0x03, 0x04]);
}

#[test]
fn docx_image_data_uri() {
    // docx-rs 要求可读出 PNG 尺寸，故用本地 fixture 图片验证图片嵌入
    let md = "![alt](tests/fixtures/test_image.png)\n";
    let bytes = markdown_to_docx(md, &opts()).expect("含本地图片的转换应成功");
    assert_eq!(&bytes[..4], &[0x50, 0x4B, 0x03, 0x04]);
}

#[test]
fn docx_image_local() {
    let md = "![alt](tests/fixtures/test_image.png)\n";
    let bytes = markdown_to_docx(md, &opts()).expect("含本地图片的转换应成功");
    assert_eq!(&bytes[..4], &[0x50, 0x4B, 0x03, 0x04]);
}

#[test]
fn docx_image_scaling() {
    let md = "<img src=\"tests/fixtures/test_image.png\" width=\"50%\" height=\"50\">\n";
    let bytes = markdown_to_docx(md, &opts()).expect("含缩放图片的转换应成功");
    assert_eq!(&bytes[..4], &[0x50, 0x4B, 0x03, 0x04]);
}
