//! SVG 输出后端。
//!
//! 消费 [`crate::document::layout::Document`]（已布局块树），输出**不分页**的
//! 长图 SVG。文本用 `<text>` 元素（依赖阅读器系统字体，不嵌入字形）。
//!
//! 注意：`Document` 的 `Paragraph.lines` 已按整页宽度折行；SVG 直接复用这些
//! 行坐标（不分页、单长画布）。

use std::fmt::Write;

use crate::color::Color;
use crate::document::layout::{Block, BlockKind, Document, TableCell, TableRow};
use crate::document::text::{TextLine, TextRun};
use crate::document::types::ResolvedStyle;
use crate::document::types::page::PageSettings;
use crate::dom::resource::base64_encode;

use super::common::{
    BQ_BAR_WIDTH, BQ_PAD_X, BQ_PAD_Y, apply_heading_style, block_height, heading_font_size,
    table_border_segments, table_row_height,
};

/// 输出层：把 [`Document`] 渲染为不分页 SVG 字符串。
///
/// 画布尺寸参考 PDF：宽度取整页宽（含左右边距），高度取内容高 + 上下边距。
/// 坐标原点为页面左上角，内容从 `(margin_left, margin_top)` 开始绘制。
pub fn document_to_svg(document: &Document, settings: &PageSettings) -> String {
    let content_h = document
        .blocks
        .iter()
        .map(|b| block_height(b, settings, settings.margin_left_pt as f64))
        .sum::<f64>();
    let page_w = settings.width_pt as f64;
    let total_h = settings.margin_top_pt as f64 + content_h + settings.margin_bottom_pt as f64;
    let content_w = settings.content_width() as f64;
    let mut r = SvgRenderer {
        out: String::new(),
        content_w,
        margin_left: settings.margin_left_pt as f64,
        margin_top: settings.margin_top_pt as f64,
        settings: settings.clone(),
    };
    let _ = writeln!(
        r.out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{:.2}" height="{:.2}" viewBox="0 0 {:.2} {:.2}">"#,
        page_w, total_h, page_w, total_h
    );
    // 白色背景（整页，含边距）
    let _ = writeln!(
        r.out,
        r##"<rect x="0" y="0" width="{:.2}" height="{:.2}" fill="#ffffff" />"##,
        page_w, total_h
    );
    r.draw_blocks(&document.blocks, r.margin_left, r.margin_top);
    r.out.push_str("</svg>");
    r.out
}

struct SvgRenderer {
    out: String,
    content_w: f64,
    /// 左边距（页面原点 → 内容区 x 偏移）
    margin_left: f64,
    /// 上边距（页面原点 → 内容区 y 偏移）
    margin_top: f64,
    settings: PageSettings,
}

impl SvgRenderer {
    fn color(c: &Color) -> String {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    }

    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, fill: &str, stroke: Option<&str>) {
        let sw = if stroke.is_some() {
            " stroke=\"black\" stroke-width=\"0.5\""
        } else {
            ""
        };
        let _ = writeln!(
            self.out,
            r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{}"{} />"#,
            x, y, w, h, fill, sw
        );
    }

    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, stroke: &str, width: f64) {
        let _ = writeln!(
            self.out,
            r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{}" stroke-width="{:.2}" />"#,
            x1, y1, x2, y2, stroke, width
        );
    }

    /// 输出一行已排版的文本（SVG `<text>`，依赖系统字体）。
    /// 若 run 带背景色（行内代码），先画一个灰色背景矩形。
    ///
    /// `x`/`y` 是文本的基线位置（baseline）。背景矩形应覆盖文字整体，
    /// 故 rect 顶部在基线上方约一个字高，底部到基线下方一点。
    fn text(&mut self, run: &TextRun, x: f64, y: f64, font_family: &str) {
        if let Some(bg) = run.background_color {
            let pad = run.font_size as f64 * 0.12;
            let fs = run.font_size as f64;
            let bx = x - pad;
            // 背景宽度：SVG 文本用系统字体渲染，宽度与 parley 嵌入字体的 advance 不一致。
            // 用字符数 × 估算字符宽（等宽/代码场景约 0.6×字号）贴合系统字体实际显示，
            // 避免背景比代码更宽/更窄。
            let char_w = fs * 0.6;
            let text_w = run.text.chars().count() as f64 * char_w;
            let bw = text_w + pad * 2.0;
            let bh = fs * 1.25;
            // 基线在底部：rect 顶部 = 基线 - 字高*0.85，底部 = 顶部 + 字高*1.25
            let by = y - fs * 0.85;
            self.rect(bx, by, bw, bh, &Self::color(&bg), None);
        }
        let fill = Self::color(&run.color);
        // 加粗 / 斜体（用系统字体渲染时显式声明，否则会丢失样式）
        let weight_attr = if run.font_weight_bold {
            r#" font-weight="bold""#
        } else {
            ""
        };
        let style_attr = if run.font_style_italic {
            r#" font-style="italic""#
        } else {
            ""
        };
        let _ = writeln!(
            self.out,
            r#"<text x="{:.2}" y="{:.2}" font-size="{:.2}" font-family="{}"{}{} fill="{}">{}</text>"#,
            x,
            y,
            run.font_size,
            escape_attr(font_family),
            weight_attr,
            style_attr,
            fill,
            escape_text(&run.text)
        );
    }

    fn draw_doc_lines(&mut self, lines: &[TextLine], x: f64, y: f64, family: &str) {
        for line in lines {
            let line_x = x + line.bounds.x0;
            let line_y = y + line.bounds.y0;
            for run in &line.runs {
                let tx = line_x + run.baseline_x as f64;
                let ty = line_y + run.baseline_y as f64;
                self.text(run, tx, ty, family);
            }
        }
    }

    fn draw_blocks(&mut self, blocks: &[Block], x: f64, y: f64) {
        let mut cy = y;
        for b in blocks {
            self.draw_block(b, x, cy);
            cy += block_height(b, &self.settings, x);
        }
    }

    fn draw_block(&mut self, block: &Block, x: f64, y: f64) {
        let style = &block.style;
        // 参考 PDF：使用完整 font_family 列表（CSS 逗号分隔，空格字体加引号），
        // 让 SVG 按系统字体 fallback，与 PDF 的字体选择保持一致。
        let family = css_font_family(&style.font_family);
        match &block.kind {
            BlockKind::Heading { level, children } => {
                let size = heading_font_size(*level);
                let color = style.color;
                for child in children {
                    if let BlockKind::Paragraph { lines } = &child.kind {
                        let styled = apply_heading_style(lines, size, color);
                        self.draw_doc_lines(&styled, x, y, &family);
                    }
                }
            }
            BlockKind::Paragraph { lines } => {
                self.draw_doc_lines(lines, x, y, &family);
            }
            BlockKind::CodeBlock { code, .. } => {
                let bg = style.background_color.unwrap_or(Color::new(245, 245, 245));
                let lh = if style.line_height_pt > 0.0 {
                    style.line_height_pt as f64
                } else {
                    18.0
                };
                let n = code.lines().count().max(1);
                let h = n as f64 * lh + 8.0;
                self.rect(x, y, self.content_w, h, &Self::color(&bg), None);
                let mut cy = y + 4.0;
                for raw in code.lines() {
                    let _ = writeln!(
                        self.out,
                        r#"<text x="{:.2}" y="{:.2}" font-size="{:.2}" font-family="monospace" fill="black">{}</text>"#,
                        x + 4.0,
                        cy + lh * 0.7,
                        style.font_size_pt,
                        escape_text(raw)
                    );
                    cy += lh;
                }
            }
            BlockKind::ThematicBreak => {
                self.line(x, y + 2.0, x + self.content_w, y + 2.0, "black", 0.75);
            }
            BlockKind::Image(img) => {
                let (w, h) = (img.size.0, img.size.1);
                if !img.data.is_empty() {
                    // `DocImage.data` 是解码后的图片字节，需编码为 data URI 供 SVG <image> 引用。
                    let mime = match img.format.as_str() {
                        "png" => "image/png",
                        "jpeg" | "jpg" => "image/jpeg",
                        "gif" => "image/gif",
                        "webp" => "image/webp",
                        "svg" => "image/svg+xml",
                        _ => "image/png",
                    };
                    let b64 = base64_encode(&img.data);
                    let href = format!("data:{};base64,{}", mime, b64);
                    let _ = writeln!(
                        self.out,
                        r#"<image x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" href="{}" />"#,
                        x,
                        y,
                        w,
                        h,
                        escape_attr(&href)
                    );
                } else if !img.alt.is_empty() {
                    let _ = writeln!(
                        self.out,
                        r##"<text x="{:.2}" y="{:.2}" font-size="11" font-family="serif" fill="#787878">{}</text>"##,
                        x,
                        y + 11.0,
                        escape_text(&img.alt)
                    );
                }
            }
            BlockKind::Blockquote { children } => {
                let bar_color = style
                    .border_color
                    .map(|c| Self::color(&c))
                    .unwrap_or_else(|| "#b0b0b0".to_string());
                let inner_x = x + BQ_BAR_WIDTH + BQ_PAD_X;
                let text_h =
                    super::common::blockquote_content_height(children, &self.settings, inner_x);
                let content_h = text_h + 2.0 * BQ_PAD_Y;
                self.rect(x, y, BQ_BAR_WIDTH, content_h, &bar_color, None);
                self.draw_blocks(children, inner_x, y + BQ_PAD_Y);
            }
            BlockKind::List { children, .. } => {
                self.draw_blocks(children, x, y);
            }
            BlockKind::ListItem {
                marker, children, ..
            }
            | BlockKind::TaskListItem {
                marker, children, ..
            } => {
                // 列表 marker（有序数字 / 无序圆点 / 任务框）画在缩进位置之前
                let marker_text = marker.trim_end();
                if !marker_text.is_empty() {
                    let _ = writeln!(
                        self.out,
                        r#"<text x="{:.2}" y="{:.2}" font-size="{:.2}" font-family="{}" fill="black">{}</text>"#,
                        x,
                        y + style.line_height_pt as f64 * 0.7,
                        style.font_size_pt,
                        escape_attr(&family),
                        escape_text(marker_text)
                    );
                }
                self.draw_blocks(children, x + 18.0, y);
            }
            BlockKind::Container { children, .. } => {
                self.draw_blocks(children, x, y);
            }
            BlockKind::DefinitionList { items } => {
                let mut cy = y;
                for item in items {
                    self.draw_blocks(&item.term, x, cy);
                    for b in &item.term {
                        cy += block_height(b, &self.settings, x);
                    }
                    self.draw_blocks(&item.definition, x + 18.0, cy);
                    for b in &item.definition {
                        cy += block_height(b, &self.settings, x + 18.0);
                    }
                }
            }
            BlockKind::FootnoteDef { children, .. } => {
                self.draw_blocks(children, x, y);
            }
            BlockKind::Table {
                rows,
                col_widths,
                row_heights,
                ..
            } => {
                self.draw_table(rows, col_widths, row_heights, style, x, y);
            }
            BlockKind::TableRow { cells } => {
                let mut cx = x;
                for c in cells {
                    self.draw_cell(c, cx, y);
                    cx += 40.0;
                }
            }
            BlockKind::TableCell { children } => {
                self.draw_blocks(children, x, y);
            }
            // 其余内联/叶子块：输出文本
            _ => {
                let t = block.text_content();
                if !t.is_empty() {
                    let _ = writeln!(
                        self.out,
                        r#"<text x="{:.2}" y="{:.2}" font-size="{:.2}" font-family="{}" fill="black">{}</text>"#,
                        x,
                        y + 12.0,
                        style.font_size_pt,
                        escape_attr(&family),
                        escape_text(&t)
                    );
                }
            }
        }
    }

    fn draw_cell(&mut self, cell: &TableCell, x: f64, y: f64) {
        self.draw_blocks(&cell.children, x, y);
    }

    fn draw_table(
        &mut self,
        rows: &[TableRow],
        col_widths: &[f64],
        row_heights: &[f64],
        style: &ResolvedStyle,
        x: f64,
        y: f64,
    ) {
        if rows.is_empty() {
            return;
        }
        let content_w: f64 = col_widths.iter().sum();
        let header_bg = style.table_header_bg.unwrap_or(Color::new(230, 230, 230));
        let header_h = table_row_height(style, row_heights, 0);
        if let Some(h) = rows.first() {
            self.rect(x, y, content_w, header_h, &Self::color(&header_bg), None);
            let mut cx = x;
            for (ci, cell) in h.cells.iter().enumerate() {
                let cw = col_widths.get(ci).copied().unwrap_or(0.0);
                self.draw_cell(cell, cx + 2.0, y + 2.0);
                cx += cw;
            }
        }
        let mut cy = y + header_h;
        for (ri, row) in rows.iter().enumerate().skip(1) {
            let rh = table_row_height(style, row_heights, ri);
            if ri % 2 == 1 {
                let alt = style.table_alt_row_bg.unwrap_or(Color::new(255, 255, 255));
                self.rect(x, cy, content_w, rh, &Self::color(&alt), None);
            }
            let mut cx = x;
            for (ci, cell) in row.cells.iter().enumerate() {
                let cw = col_widths.get(ci).copied().unwrap_or(0.0);
                self.draw_cell(cell, cx + 2.0, cy + 2.0);
                cx += cw;
            }
            cy += rh;
        }
        // 表格边框线（外框 + 列分隔竖线 + 行分隔横线），颜色/宽度取自样式。
        for seg in table_border_segments(rows, col_widths, row_heights, style, x, y) {
            self.line(
                seg.x1,
                seg.y1,
                seg.x2,
                seg.y2,
                &Self::color(&seg.color),
                seg.width,
            );
        }
    }
}

/// 构造 CSS font-family 列表字符串（逗号分隔，空格字体名加引号）。
fn css_font_family(families: &[String]) -> String {
    if families.is_empty() {
        return "sans-serif".to_string();
    }
    families
        .iter()
        .map(|f| {
            let f = f.trim();
            if f.contains(char::is_whitespace) {
                format!("\"{}\"", f)
            } else {
                f.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// 转义 SVG 文本内容（`&` `<` `>` 等）。
fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// 转义 SVG 属性值。
fn escape_attr(s: &str) -> String {
    escape_text(s).replace('"', "&quot;")
}
