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

// ─── 图片高度钳制（与 PDF clamp_by_height 对齐，2026-09-04 审查） ─────────

/// 生成纯色 PNG（w×h px）并编码为 data URI。
fn png_data_uri(w: u32, h: u32) -> String {
    use base64::Engine;
    let img = image::DynamicImage::new_rgb8(w, h);
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(buf.get_ref())
    )
}

/// 提取 document.xml 中首个 `<wp:extent>` 的 (cx, cy)（EMU）。
fn docx_first_extent(xml: &str) -> (u64, u64) {
    let marker = "<wp:extent cx=\"";
    let idx = xml.find(marker).expect("嵌入图片应有 wp:extent");
    let rest = &xml[idx + marker.len()..];
    let cx_end = rest.find('"').expect("cx 应有结束引号");
    let cx: u64 = rest[..cx_end].parse().expect("cx 应为数字");
    let rest2 = &rest[cx_end + 1..];
    let cy_marker = " cy=\"";
    let cy_idx = rest2.find(cy_marker).expect("extent 应含 cy");
    let rest3 = &rest2[cy_idx + cy_marker.len()..];
    let cy_end = rest3.find('"').expect("cy 应有结束引号");
    let cy: u64 = rest3[..cy_end].parse().expect("cy 应为数字");
    (cx, cy)
}

#[test]
fn docx_tall_image_clamped_to_page_height() {
    // 100×2000px 竖长图：96dpi 下自然高 1500pt，远超 A4 内容高（≈698pt）。
    // 未经钳制时 Word 会按完整高度嵌入并截断超出页高的部分。
    let md = format!("![tall]({})\n", png_data_uri(100, 2000));
    let bytes = markdown_to_docx(&md, &opts()).expect("含竖长图的转换应成功");
    let xml = docx_entry(&bytes, "word/document.xml");
    let (cx, cy) = docx_first_extent(&xml);
    // 1pt = 12700 EMU；页内容高上限 = A4(841.89) − 2×36pt（内置 @page 边距）≈ 769.9pt
    let max_h_emu = 772.0 * 12700.0;
    assert!(
        (cy as f64) <= max_h_emu,
        "图片显示高应钳制到页内容高，实际 cy={cy} EMU（≈{} pt）",
        cy as f64 / 12700.0
    );
    // 宽度应随高度等比缩小，保持原宽高比 100/2000 = 0.05
    let w_pt = cx as f64 / 12700.0;
    let h_pt = cy as f64 / 12700.0;
    let aspect = w_pt / h_pt;
    assert!(
        (aspect - 0.05).abs() < 0.005,
        "钳制后应保持宽高比 0.05，实际 w={w_pt}pt h={h_pt}pt"
    );
}

#[test]
fn docx_small_image_not_upscaled() {
    // 100×200px 小图：自然宽 75pt < 内容宽，不应被放大，也不受高度钳制影响。
    let md = format!("![small]({})\n", png_data_uri(100, 200));
    let bytes = markdown_to_docx(&md, &opts()).expect("含小图的转换应成功");
    let xml = docx_entry(&bytes, "word/document.xml");
    let (cx, cy) = docx_first_extent(&xml);
    let (w_pt, h_pt) = (cx as f64 / 12700.0, cy as f64 / 12700.0);
    assert!((w_pt - 75.0).abs() < 1.0, "小图宽度应保持自然尺寸 75pt，实际 {w_pt}");
    assert!((h_pt - 150.0).abs() < 1.0, "小图高度应保持自然尺寸 150pt，实际 {h_pt}");
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
