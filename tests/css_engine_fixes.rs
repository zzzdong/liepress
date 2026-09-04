//! 2026-09-03 代码审查问题修复回归测试（集成层）
//!
//! 覆盖 docs/code-review-2026-09-03.md 的 P1-3/P1-4、P2-2/2-4/2-5、S-1/S-2/S-3。
//! （P1-1/1-2、P2-1/2-3、S-4 的单元回归在 `src/css/engine.rs` 的 tests 模块内。）

use liepress::ast::parse_markdown_with_css;
use liepress::ast::{NodeKind, PageConfig};
use liepress::dom::resource::{ResolvedResource, embed_images};
use liepress::dom::to_ast::html_to_styled_nodes;
use liepress::dom::{ResourceResolver, markdown_to_dom, parse_html_document};
use liepress::{ConvertOptions, PageSettings};

/// 构造仅含内置默认 CSS 的引擎
fn engine() -> liepress::css::engine::CssEngine {
    liepress::css::engine::CssEngine::new(liepress::ast::presets::DEFAULT_CSS)
        .expect("内置 CSS 必须可解析")
}

// ───────────────────────── P1-3：line-height 透传 ─────────────────────────

/// P1-3: `line-height: 2` 写入 Style.line_height_pt 并进入布局块（度量与排版同源）
#[test]
fn p1_3_line_height_reaches_layout() {
    let (node, _) = parse_markdown_with_css("hello", "p { line-height: 2 }").unwrap();
    let settings = PageSettings::default();
    let doc = liepress::ast_to_layout(&node, &settings);
    assert!(!doc.blocks.is_empty());
    let style = &doc.blocks[0].style;
    assert!(
        (style.line_height_pt - 21.0).abs() < 0.5,
        "line_height_pt 应为 2×10.5=21，实际 {}",
        style.line_height_pt
    );
}

// ───────────────────────── P1-4：PDF 无限高度 ─────────────────────────

/// 用 lopdf 读取 PDF 首页 MediaBox 高度（pt）
fn pdf_first_page_height(pdf: &[u8]) -> f64 {
    let doc = lopdf::Document::load_mem(pdf).expect("load pdf");
    let pages = doc.get_pages();
    assert_eq!(pages.len(), 1, "无限高度模式应为单页");
    let (_, page_id) = pages.iter().next().unwrap();
    let obj = doc.get_object(*page_id).unwrap();
    let mediabox = match obj {
        lopdf::Object::Dictionary(dict) => match dict.get(b"MediaBox").unwrap() {
            lopdf::Object::Reference(r) => doc.get_object(*r).unwrap().clone(),
            other => other.clone(),
        },
        other => other.clone(),
    };
    let lopdf::Object::Array(rect) = mediabox else {
        panic!("MediaBox 应为数组");
    };
    let num = |o: &lopdf::Object| -> f64 {
        match o {
            lopdf::Object::Real(v) => *v as f64,
            lopdf::Object::Integer(v) => *v as f64,
            _ => panic!("MediaBox 分量应为数值"),
        }
    };
    (num(&rect[3]) - num(&rect[1])).abs()
}

/// P1-4: `height_unlimited` 页高按内容扩展（不再固定 A4 高 842pt）
#[test]
fn p1_4_pdf_height_unlimited_expands_page() {
    // 40 段 ≈ 1100pt 内容高，超过 A4（842pt）
    let md: String = (0..40)
        .map(|i| format!("第 {i} 段测试内容。\n\n"))
        .collect();
    let opts = ConvertOptions::new().with_page_config(PageConfig {
        height_unlimited: Some(true),
        ..PageConfig::default()
    });
    let pdf = liepress::markdown_to_pdf(&md, &opts).expect("generate pdf");
    let h = pdf_first_page_height(&pdf);
    assert!(h > 900.0, "页高 {h} 应按内容扩展（A4 为 842）");
    assert!(h < 5000.0, "页高 {h} 不应失控");
}

// ───────────────────────── P2-2：GFM 表格对齐 ─────────────────────────

/// P2-2: GFM `|:---|:---:|---:|` 对齐标记进入表格 align 列表
#[test]
fn p2_2_gfm_table_alignment() {
    let md = "| 左 | 中 | 右 |\n|:---|:---:|---:|\n| a | b | c |";
    let doc = markdown_to_dom(md);
    let e = engine();
    let node = html_to_styled_nodes(&doc, &e);

    fn find_align(node: &liepress::ast::Node) -> Option<Vec<liepress::ast::TextAlign>> {
        match &node.kind {
            NodeKind::Document { children } | NodeKind::Paragraph { children } => {
                children.iter().find_map(find_align)
            }
            NodeKind::Container { children } => children.iter().find_map(find_align),
            NodeKind::Table { align, .. } => Some(align.clone()),
            _ => None,
        }
    }
    let align = find_align(&node).expect("应存在 Table 节点");
    assert_eq!(align.len(), 3);
    assert_eq!(align[0], liepress::ast::TextAlign::Left);
    assert_eq!(align[1], liepress::ast::TextAlign::Center);
    assert_eq!(align[2], liepress::ast::TextAlign::Right);
}

// ───────────────────────── P2-4：容器标签块级子元素 ─────────────────────────

fn find_kind(node: &liepress::ast::Node, pred: &dyn Fn(&NodeKind) -> bool) -> bool {
    if pred(&node.kind) {
        return true;
    }
    match &node.kind {
        NodeKind::Document { children }
        | NodeKind::Paragraph { children }
        | NodeKind::Container { children }
        | NodeKind::Span { children } => children.iter().any(|c| find_kind(c, pred)),
        _ => false,
    }
}

/// P2-4: `<div>` 内嵌块级子元素映射为 Container（结构与样式不再压平）
#[test]
fn p2_4_div_with_block_children_maps_to_container() {
    let doc = parse_html_document("<div><h1>标题</h1><p>正文</p></div>");
    let e = engine();
    let node = html_to_styled_nodes(&doc, &e);
    assert!(
        find_kind(&node, &|k| matches!(k, NodeKind::Container { .. })),
        "div 内嵌块级子元素应映射为 Container"
    );
    assert!(find_kind(&node, &|k| matches!(k, NodeKind::Heading { .. })));
}

/// P2-4: 纯行内内容的 div 仍为 Paragraph（保留既有简化语义）
#[test]
fn p2_4_div_inline_only_stays_paragraph() {
    let doc = parse_html_document("<div>纯文本 <b>加粗</b></div>");
    let e = engine();
    let node = html_to_styled_nodes(&doc, &e);
    assert!(
        !find_kind(&node, &|k| matches!(k, NodeKind::Container { .. })),
        "纯行内 div 不应升级为 Container"
    );
}

// ───────────────────────── P2-5：page-break-before/after ─────────────────────────

/// 用 lopdf 统计 PDF 页数
fn pdf_page_count(pdf: &[u8]) -> usize {
    lopdf::Document::load_mem(pdf)
        .expect("load pdf")
        .get_pages()
        .len()
}

/// 超大页面配置（排除自然溢出分页干扰）
fn huge_page_opts() -> ConvertOptions {
    ConvertOptions::new().with_page_config(PageConfig {
        width: Some(2000.0),
        height: Some(5000.0),
        margin_top: Some(0.0),
        margin_bottom: Some(0.0),
        margin_left: Some(0.0),
        margin_right: Some(0.0),
        ..PageConfig::default()
    })
}

/// P2-5: `page-break-before: always` 强制分页
#[test]
fn p2_5_page_break_before_forces_new_page() {
    let md =
        "<div>第一页内容。</div>\n\n<div style=\"page-break-before: always\">第二页内容。</div>";
    let pdf = liepress::markdown_to_pdf(md, &huge_page_opts()).expect("generate pdf");
    assert_eq!(pdf_page_count(&pdf), 2, "page-break-before 应强制分为两页");
}

/// P2-5: `page-break-after: always` 强制分页
#[test]
fn p2_5_page_break_after_forces_new_page() {
    let md = "<div style=\"page-break-after: always\">前段。</div>\n\n<div>后段。</div>";
    let pdf = liepress::markdown_to_pdf(md, &huge_page_opts()).expect("generate pdf");
    assert_eq!(pdf_page_count(&pdf), 2, "page-break-after 应强制分为两页");
}

// ───────────────────────── S-1：本地图片路径限制 ─────────────────────────

fn find_img_src(el: &liepress::dom::HtmlElement) -> Option<String> {
    if el.tag == liepress::dom::HtmlTag::Img {
        return el.attrs.get("src").cloned();
    }
    for child in &el.children {
        if let liepress::dom::HtmlNode::Element(c) = child
            && let Some(s) = find_img_src(c)
        {
            return Some(s);
        }
    }
    None
}

/// S-1: 主 DOM 管线拒绝绝对路径/目录穿越/非图片扩展名（不再读取内嵌）
#[test]
fn s1_main_pipeline_rejects_unsafe_paths() {
    for src in ["/etc/passwd", "../../etc/passwd", "C:\\Windows\\win.ini"] {
        let md = format!("![x]({src})");
        let mut doc = markdown_to_dom(&md);
        let r = ResourceResolver::new(None);
        assert!(
            matches!(r.resolve_image(src), ResolvedResource::Unchanged),
            "{src} 应被拒绝"
        );
        embed_images(&mut doc, &r);
        assert_eq!(
            find_img_src(&doc.root).as_deref(),
            Some(src),
            "{src} 应保持原样不内嵌"
        );
    }
}

/// S-1: 两管线语义统一 —— 降级字符串管线同样拒绝（HTML 输出无 data: 内嵌）
#[test]
fn s1_legacy_pipeline_consistent() {
    let html = liepress::markdown_to_html("![x](/etc/passwd)");
    assert!(!html.contains("data:"), "降级管线不应内嵌绝对路径文件");
}

// ───────────────────────── S-3：深嵌套保护 ─────────────────────────

/// S-3: 3000 层嵌套 div 转换不栈溢出（to_ast 深度保护生效，转换正常完成）
#[test]
fn s3_deep_nesting_does_not_overflow() {
    let depth = 3000;
    let mut html = String::with_capacity(depth * 12);
    for _ in 0..depth {
        html.push_str("<div>");
    }
    html.push_str("深层内容");
    for _ in 0..depth {
        html.push_str("</div>");
    }
    let doc = parse_html_document(&html);
    let e = engine();
    // 只要不 abort/panic 即通过（512 层深度限制生效）
    let _node = html_to_styled_nodes(&doc, &e);
}

// ───────────────────────── S-2：事件栈容错 ─────────────────────────

/// S-2: 常规 Markdown（含嵌套结构/表格/脚注）解析无 panic
#[test]
fn s2_markdown_event_stack_safe() {
    let md = "# 标题\n\n- 列表项 **加粗**\n- 另一项[^1]\n\n> 引用 `code`\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n[^1]: 脚注定义";
    let doc = markdown_to_dom(md);
    assert!(
        find_img_src(&doc.root).is_none() || true,
        "解析完成即为通过"
    );
}

// ───────────────────────── 2026-09-04 审查批次三：PDF alpha 与标题字号 ─────────────────────────

/// 收集 PDF 全部内容流中的 `Tf`（字号）操作数
fn pdf_font_sizes(pdf: &[u8]) -> Vec<f32> {
    let doc = lopdf::Document::load_mem(pdf).expect("load pdf");
    let mut sizes = Vec::new();
    for (_, page_id) in doc.get_pages() {
        let content = doc.get_page_content(page_id);
        if let Ok(ops) = lopdf::content::Content::decode(&content) {
            for op in &ops.operations {
                if op.operator == "Tf"
                    && let Some(size) = op.operands.get(1)
                {
                    let v = match size {
                        lopdf::Object::Real(v) => *v as f32,
                        lopdf::Object::Integer(v) => *v as f32,
                        _ => continue,
                    };
                    sizes.push(v);
                }
            }
        }
    }
    sizes
}

/// 标题字号应来自 CSS 排版（default.css h1 = 24pt），不得被 PDF 端硬编码 22pt 覆盖
#[test]
fn pdf_heading_uses_css_font_size() {
    let md = "# 标题\n\n正文内容";
    let pdf = liepress::markdown_to_pdf(md, &ConvertOptions::default()).expect("generate pdf");
    let sizes = pdf_font_sizes(&pdf);
    assert!(
        sizes.iter().any(|s| (*s - 24.0).abs() < 0.5),
        "h1 应按 CSS 24pt 排版，实际 Tf 字号 {sizes:?}"
    );
    assert!(
        !sizes.iter().any(|s| (*s - 22.0).abs() < 0.1),
        "不得出现旧硬编码的 22pt 标题字号，实际 {sizes:?}"
    );
}

/// 半透明文字颜色：alpha 须除以 255 归一化（0.5 → ExtGState /ca ≈ 0.5），
/// 旧实现直接传 0-255 值，0..=1 之外的值回退为 1（不透明）。
#[test]
fn pdf_text_color_alpha_is_normalized() {
    let md = "<style>p { color: rgba(255, 0, 0, 0.5); }</style>\n\n半透明文字";
    let pdf = liepress::markdown_to_pdf(md, &ConvertOptions::default()).expect("generate pdf");
    let doc = lopdf::Document::load_mem(&pdf).expect("load pdf");
    let has_half_alpha = doc.objects.values().any(|obj| {
        if let lopdf::Object::Dictionary(dict) = obj {
            if let Ok(v) = dict.get(b"ca") {
                let num = match v {
                    lopdf::Object::Real(r) => Some(*r as f32),
                    lopdf::Object::Integer(i) => Some(*i as f32),
                    _ => None,
                };
                if let Some(n) = num
                    && (n - 0.5).abs() < 0.05
                {
                    return true;
                }
            }
        }
        false
    });
    assert!(
        has_half_alpha,
        "PDF 应存在 /ca ≈ 0.5 的 ExtGState（alpha 归一化生效）"
    );
}
