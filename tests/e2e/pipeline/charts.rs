//! 管线输出：图表代码块（mermaid / liecharts）在 PDF / SVG / PNG 后端的覆盖。
//!
//! 此前仅 DOCX 与 HTML 有图表用例，PDF/SVG/PNG 三个后端消费
//! `BlockKind::Image` 的路径（PDF 嵌入 XObject、SVG `<image>`、PNG 重采样）
//! 完全没有回归保护（2026-09-04 审查·测试盲区）。

use liepress::{ConvertOptions, markdown_to_pdf, markdown_to_png, markdown_to_svg};

fn opts() -> ConvertOptions {
    ConvertOptions::new().with_auto_font(false)
}

/// 判断 PDF 对象是否为 Image XObject（流对象，其字典 Subtype == /Image）。
fn is_image_xobject(o: &lopdf::Object) -> bool {
    matches!(o, lopdf::Object::Stream(s)
        if s.dict.get(b"Subtype").is_ok_and(|v| matches!(v, lopdf::Object::Name(n) if n.as_slice() == b"Image")))
}

const MERMAID_MD: &str = "# 图表\n\n```mermaid\nflowchart TD\n  A[开始] --> B[结束]\n```\n";

const LIECHARTS_MD: &str = concat!(
    "```liecharts\n",
    "{\"xAxis\":[{\"type\":\"category\",\"data\":[\"a\",\"b\"]}],",
    "\"yAxis\":[{\"type\":\"value\"}],\"series\":[{\"type\":\"bar\",\"data\":[1,2]}]}\n",
    "```\n"
);

// ─── mermaid → 三个位图/矢量后端 ─────────────────────────────

#[cfg(feature = "mermaid")]
#[test]
fn pdf_mermaid_block_renders() {
    let pdf = markdown_to_pdf(MERMAID_MD, &opts()).expect("含 mermaid 的 PDF 应生成成功");
    assert!(pdf.starts_with(b"%PDF"));
    // 图片作为 XObject 嵌入：PDF 中应出现 /XObject 资源。
    let doc = lopdf::Document::load_mem(&pdf).expect("load pdf");
    let has_xobject = doc.objects.values().any(is_image_xobject);
    assert!(has_xobject, "mermaid 图应以 Image XObject 嵌入 PDF");
}

#[cfg(feature = "mermaid")]
#[test]
fn svg_mermaid_block_renders() {
    let svg = markdown_to_svg(MERMAID_MD, &opts()).expect("含 mermaid 的 SVG 应生成成功");
    assert!(svg.contains("<svg"), "应为合法 SVG");
    assert!(
        svg.contains("<image"),
        "mermaid 图应以 <image> 元素嵌入 SVG"
    );
}

#[cfg(feature = "mermaid")]
#[test]
fn png_mermaid_block_renders() {
    let png = markdown_to_png(MERMAID_MD, &opts()).expect("含 mermaid 的 PNG 应生成成功");
    assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
}

// ─── liecharts → 三个位图/矢量后端 ───────────────────────────

#[cfg(feature = "charts")]
#[test]
fn pdf_liecharts_block_renders() {
    let pdf = markdown_to_pdf(LIECHARTS_MD, &opts()).expect("含 liecharts 的 PDF 应生成成功");
    let doc = lopdf::Document::load_mem(&pdf).expect("load pdf");
    let has_xobject = doc.objects.values().any(is_image_xobject);
    assert!(has_xobject, "liecharts 图应以 Image XObject 嵌入 PDF");
}

#[cfg(feature = "charts")]
#[test]
fn svg_liecharts_block_renders() {
    let svg = markdown_to_svg(LIECHARTS_MD, &opts()).expect("含 liecharts 的 SVG 应生成成功");
    assert!(svg.contains("<image"), "liecharts 图应以 <image> 元素嵌入 SVG");
}

#[cfg(feature = "charts")]
#[test]
fn png_liecharts_block_renders() {
    let png = markdown_to_png(LIECHARTS_MD, &opts()).expect("含 liecharts 的 PNG 应生成成功");
    assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
}

// ─── 软降级：渲染失败时代码块保留、不 panic ────────────────────

#[cfg(feature = "mermaid")]
#[test]
fn mermaid_render_failure_degrades_gracefully() {
    use liepress::markdown_to_html_document;
    // 非法 mermaid 源码：渲染器报错 → 软降级为带错误注释的普通代码块。
    let html = markdown_to_html_document(
        "```mermaid\n:::not-a-valid-diagram:::\n```\n",
        None,
        None,
        None,
    );
    assert!(
        html.contains("render failed"),
        "渲染失败应软降级并在代码块中注明，实际：{}",
        &html[..html.len().min(500)]
    );
    assert!(
        !html.contains("data:image/png;base64,"),
        "失败渲染不应产出图片"
    );
}

#[cfg(feature = "charts")]
#[test]
fn liecharts_render_failure_degrades_gracefully() {
    use liepress::markdown_to_html_document;
    let html = markdown_to_html_document(
        "```liecharts\n{\"series\": \"not-an-array\"}\n```\n",
        None,
        None,
        None,
    );
    assert!(
        html.contains("render failed") || html.contains("<pre"),
        "非法 liecharts 应软降级为代码块/错误注释，实际：{}",
        &html[..html.len().min(500)]
    );
}

// ─── info-string 覆盖项端到端传导 ────────────────────────────

#[cfg(feature = "mermaid")]
#[test]
fn info_string_width_override_propagates() {
    use liepress::markdown_to_html_document;
    // `width=900` 应传导到渲染像素尺寸：解码内嵌 PNG 的 IHDR 宽度断言。
    let html = markdown_to_html_document(
        "```mermaid width=900\nflowchart TD\n  A --> B\n```\n",
        None,
        None,
        None,
    );
    let marker = "data:image/png;base64,";
    let idx = html
        .find(marker)
        .expect("mermaid 应渲染为内嵌 PNG");
    let b64 = &html[idx + marker.len()..];
    let b64 = &b64[..b64.find('"').unwrap_or(b64.len())];
    let png = base64_decode(b64);
    // PNG IHDR：8 字节签名 + 4 长度 + "IHDR" + 4 宽度（大端）。
    assert_eq!(&png[12..16], b"IHDR", "内嵌数据应为 PNG");
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    assert_eq!(width, 900, "info-string 的 width=900 应传导到渲染像素宽");
}

/// 极简 base64 解码（测试专用，无外部依赖）。
fn base64_decode(input: &str) -> Vec<u8> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a') as u32 + 26),
            b'0'..=b'9' => Some((c - b'0') as u32 + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u32;
    for &c in input.as_bytes() {
        if let Some(v) = val(c) {
            acc = (acc << 6) | v;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
            }
        }
    }
    out
}
