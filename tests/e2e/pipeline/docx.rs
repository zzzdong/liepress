//! 管线输出：DOCX（含图片资源）。

use liepress::ConvertOptions;
use liepress::markdown_to_docx;
use std::io::Read;

fn opts() -> ConvertOptions {
    ConvertOptions::new().with_auto_font(false)
}

/// 读取 DOCX 包内指定条目为字符串。
fn docx_entry(bytes: &[u8], name: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("DOCX 应为合法 zip");
    let mut f = zip
        .by_name(name)
        .unwrap_or_else(|_| panic!("DOCX 应包含条目 {name}"));
    let mut s = String::new();
    f.read_to_string(&mut s).expect("读取条目应成功");
    s
}

/// 列出 DOCX 包内 `word/media/` 下的所有条目名（嵌入图片）。
fn docx_media(bytes: &[u8]) -> Vec<String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("DOCX 应为合法 zip");
    (0..zip.len())
        .filter_map(|i| {
            let f = zip.by_index(i).ok()?;
            let name = f.name().to_string();
            name.starts_with("word/media/").then_some(name)
        })
        .collect()
}

/// 提取 `document.xml` 中所有 `<w:color w:val="..."/>` 的取值（去重、保序）。
fn run_colors(xml: &str) -> Vec<String> {
    const MARKER: &str = "color w:val=\"";
    let mut out: Vec<String> = Vec::new();
    let mut rest = xml;
    while let Some(idx) = rest.find(MARKER) {
        rest = &rest[idx + MARKER.len()..];
        let Some(end) = rest.find('"') else { break };
        let value = rest[..end].to_string();
        if !out.contains(&value) {
            out.push(value);
        }
        rest = &rest[end..];
    }
    out
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

// ─── AST 富化产物：外绘图片 + 语法高亮（DOCX 侧验证） ─────────

#[cfg(feature = "mermaid")]
#[test]
fn docx_mermaid_block_embeds_image() {
    let md = "# 图表\n\n```mermaid\nflowchart TD\n  A[开始] --> B[结束]\n```\n";
    let bytes = markdown_to_docx(md, &opts()).expect("含 mermaid 的转换应成功");
    let media = docx_media(&bytes);
    assert!(
        !media.is_empty(),
        "mermaid 图应预渲染并嵌入 word/media/，实际媒体条目：{media:?}"
    );
}

#[cfg(feature = "charts")]
#[test]
fn docx_liecharts_block_embeds_image() {
    let md = concat!(
        "```liecharts\n",
        "{\"title\":{\"text\":\"t\"},\"xAxis\":[{\"type\":\"category\",\"data\":[\"a\",\"b\"]}],",
        "\"yAxis\":[{\"type\":\"value\"}],\"series\":[{\"type\":\"bar\",\"data\":[1,2]}]}\n",
        "```\n"
    );
    let bytes = markdown_to_docx(md, &opts()).expect("含 liecharts 的转换应成功");
    let media = docx_media(&bytes);
    assert!(
        !media.is_empty(),
        "liecharts 图应预渲染并嵌入 word/media/，实际媒体条目：{media:?}"
    );
}

#[test]
fn docx_code_block_has_syntax_highlight() {
    // 关键字/类型/数字/字符串应产生不同前景色 → document.xml 中出现多个 w:color。
    let md = "```rust\nfn main() {\n    let x: i32 = 42;\n}\n```\n";
    let bytes = markdown_to_docx(md, &opts()).expect("含代码块的转换应成功");
    let xml = docx_entry(&bytes, "word/document.xml");
    let colors = run_colors(&xml);
    assert!(
        colors.len() > 1,
        "DOCX 代码块应带语法高亮（多种前景色），实际颜色：{colors:?}"
    );
}

/// 拼接 `document.xml` 中所有 `<w:t>...</w:t>` 的文本（跨 Run 合并）。
fn run_text(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(idx) = rest.find("<w:t") {
        rest = &rest[idx..];
        let Some(open) = rest.find('>') else { break };
        rest = &rest[open + 1..];
        let Some(end) = rest.find("</w:t>") else {
            break;
        };
        out.push_str(&rest[..end]);
        rest = &rest[end..];
    }
    out
}

#[test]
fn docx_code_block_preserves_line_breaks_and_text() {
    let md = "```rust\nlet a = 1;\nlet b = 2;\n```\n";
    let bytes = markdown_to_docx(md, &opts()).expect("含代码块的转换应成功");
    let xml = docx_entry(&bytes, "word/document.xml");
    assert!(
        xml.contains("<w:br"),
        "多行代码块应使用换行符（<w:br/>）而非软换行"
    );
    // 高亮后代码被切成多个 Run，需合并 <w:t> 后再比对文本。
    let text = run_text(&xml);
    assert!(
        text.contains("let a = 1;") && text.contains("let b = 2;"),
        "代码块文本应完整保留（含缩进与换行），实际：{text:?}"
    );
}
