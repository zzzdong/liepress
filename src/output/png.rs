//! PNG 输出后端。
//!
//! 消费 [`crate::document::layout::Document`]（已布局块树），用 `vello_cpu`
//! 光栅化为**不分页**的长图 PNG。文本用字形轮廓（从 `TextRun.font_data` 加载
//! 字体，`glyph_run` 绘制），保证与 PDF 视觉一致。

use crate::color::Color;
use crate::document::layout::{Block, BlockKind, Document, TableCell, TableRow};
use crate::document::text::{TextLine, TextRun, layout_text};
use crate::document::types::ResolvedStyle;
use crate::document::types::page::PageSettings;
use crate::error::Result;

use super::common::{
    BQ_BAR_WIDTH, BQ_PAD_X, BQ_PAD_Y, apply_heading_style, block_height, heading_font_size,
    table_border_segments, table_row_height, text_style_from_resolved,
};

use vello_cpu::kurbo::{Affine, BezPath, Circle, Point, Rect, Shape};
use vello_cpu::peniko::color::AlphaColor;
use vello_cpu::{RenderContext, Resources};

/// 输出层：把 [`Document`] 渲染为不分页 PNG 字节。
///
/// 画布尺寸参考 PDF：宽度取整页宽（含左右边距），高度取内容高 + 上下边距。
/// 坐标原点为页面左上角，内容从 `(margin_left, margin_top)` 开始绘制。
/// `dpi` 控制分辨率（默认 96）；72 时 1pt = 1px。
pub fn document_to_png(document: &Document, settings: &PageSettings, dpi: f32) -> Result<Vec<u8>> {
    let content_h = document
        .blocks
        .iter()
        .map(|b| block_height(b, settings, settings.margin_left_pt as f64))
        .sum::<f64>();
    let page_w = settings.width_pt as f64;
    let total_h = settings.margin_top_pt as f64 + content_h + settings.margin_bottom_pt as f64;
    let content_w = settings.content_width() as f64;
    let margin_left = settings.margin_left_pt as f64;
    let margin_top = settings.margin_top_pt as f64;
    let scale = dpi / 72.0;
    let pixel_w = ((page_w * scale as f64).ceil() as u32).clamp(1, u32::from(u16::MAX));
    let pixel_h = ((total_h * scale as f64).ceil() as u32).clamp(1, u32::from(u16::MAX));
    if pixel_w == 0 || pixel_h == 0 {
        return Err(crate::error::Error::RenderError(
            "PNG dimensions out of range".to_string(),
        ));
    }
    let (pw16, ph16) = (pixel_w as u16, pixel_h as u16);

    let mut r = PngRenderer {
        ctx: RenderContext::new(pw16, ph16),
        resources: Resources::new(),
        content_w,
        scale: scale as f64,
        settings: settings.clone(),
    };
    // 先铺白色全页底色（不透明），再绘制其它元素。vello 默认画布透明，
    // 由这条全页矩形提供背景，避免渲染后全局后处理铺白的语义问题。
    r.fill_rect(0.0, 0.0, page_w, total_h, &Color::new(255, 255, 255));
    r.draw_blocks(&document.blocks, margin_left, margin_top);

    let mut pixmap = vello_cpu::Pixmap::new(pw16, ph16);
    r.ctx.render(&mut pixmap, &mut r.resources);
    let png = pixmap
        .into_png()
        .map_err(|e| crate::error::Error::RenderError(format!("{}", e)))?;
    Ok(png)
}

struct PngRenderer {
    ctx: RenderContext,
    resources: Resources,
    content_w: f64,
    scale: f64,
    settings: PageSettings,
}

impl PngRenderer {
    fn px(&self, pt: f64) -> f64 {
        pt * self.scale
    }

    fn color(c: &Color) -> AlphaColor<vello_cpu::color::Srgb> {
        AlphaColor::from_rgba8(c.r, c.g, c.b, c.a)
    }

    fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: &Color) {
        let rect = Rect::new(self.px(x), self.px(y), self.px(x + w), self.px(y + h));
        self.ctx.set_paint(Self::color(color));
        self.ctx.fill_rect(&rect);
    }

    fn stroke_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: &Color, width: f64) {
        let mut path = BezPath::new();
        path.move_to(Point::new(self.px(x1), self.px(y1)));
        path.line_to(Point::new(self.px(x2), self.px(y2)));
        self.ctx.set_paint(Self::color(color));
        self.ctx
            .set_stroke(vello_cpu::kurbo::Stroke::new(self.px(width)));
        self.ctx.stroke_path(&path);
    }

    /// 填充实心圆（矢量绘制，任何字体下样式一致、缩放不模糊）。
    fn fill_circle(&mut self, cx: f64, cy: f64, r: f64, color: &Color) {
        if r <= 0.0 {
            return;
        }
        let circle = Circle::new(Point::new(self.px(cx), self.px(cy)), self.px(r));
        let path = circle.to_path(0.1);
        self.ctx.set_paint(Self::color(color));
        self.ctx.fill_path(&path);
    }

    /// 描边空心矩形（复选框外框）。
    fn stroke_rect(&mut self, x: f64, y: f64, w: f64, h: f64, color: &Color, width: f64) {
        let rect = Rect::new(self.px(x), self.px(y), self.px(x + w), self.px(y + h));
        let path = rect.to_path(0.1);
        self.ctx.set_paint(Self::color(color));
        self.ctx
            .set_stroke(vello_cpu::kurbo::Stroke::new(self.px(width)));
        self.ctx.stroke_path(&path);
    }

    /// 在列表项缩进槽**左缘**（`x`）单独绘制列表 marker（矢量绘图）。
    ///
    /// - 任务列表：矢量画复选框外框，勾选时加对勾路径。
    /// - 无序列表（marker="●"）：矢量画实心圆点。
    /// - 有序列表（marker="N."）：以正文同字号/基线绘制数字文本。
    ///
    /// 正文（children）由调用方以 `x + list_item_indent` 排起，故 marker 落在
    /// 空白缩进槽内，与正文天然错位、互不重叠。
    fn draw_list_marker(
        &mut self,
        marker: &str,
        is_task: bool,
        checked: bool,
        x: f64,
        y: f64,
        style: &ResolvedStyle,
    ) {
        let marker = marker.trim();
        if marker.is_empty() {
            return;
        }
        let fs = style.font_size_pt as f64;
        let lh = if style.line_height_pt > 0.0 {
            style.line_height_pt as f64
        } else {
            fs * 1.5
        };
        let color = style.color;

        if is_task {
            // 复选框：外框 + 对勾（矢量）
            let size = fs * 0.55;
            let x0 = x;
            let y0 = y + (lh - size) * 0.5;
            self.stroke_rect(x0, y0, size, size, &color, 1.2);
            if checked {
                let s = size;
                let mut path = BezPath::new();
                path.move_to(Point::new(self.px(x0 + s * 0.2), self.px(y0 + s * 0.52)));
                path.line_to(Point::new(self.px(x0 + s * 0.42), self.px(y0 + s * 0.72)));
                path.line_to(Point::new(self.px(x0 + s * 0.8), self.px(y0 + s * 0.28)));
                self.ctx.set_paint(Self::color(&color));
                self.ctx
                    .set_stroke(vello_cpu::kurbo::Stroke::new(self.px(1.6)));
                self.ctx.stroke_path(&path);
            }
            return;
        }

        if marker == "●" {
            // 实心圆点：半径 = 0.18em，垂直居中于首行，略偏左缘。
            let r = fs * 0.18;
            let cx = x + r + fs * 0.1;
            let cy = y + lh * 0.5;
            self.fill_circle(cx, cy, r, &color);
            return;
        }

        // 有序数字 marker：与正文同字号/基线绘制（x = 行顶）。
        let ts = text_style_from_resolved(style);
        let segments = [(marker, &ts)];
        let layout = layout_text(&segments, None, crate::ast::TextAlign::Left);
        if let Some(tl) = layout.lines.last() {
            for run in &tl.runs {
                self.draw_text_run(run, Point::new(x, y));
            }
        }
    }

    fn draw_doc_lines(&mut self, lines: &[TextLine], x: f64, y: f64) {
        for line in lines {
            let line_x = x + line.bounds.x0;
            let line_y = y + line.bounds.y0;
            for run in &line.runs {
                self.draw_text_run(run, Point::new(line_x, line_y));
            }
        }
    }

    fn draw_text_run(&mut self, run: &TextRun, position: Point) {
        let scaled_pos = Point::new(self.px(position.x), self.px(position.y));
        let transform = Affine::translate((scaled_pos.x, scaled_pos.y));
        let glyphs: Vec<vello_cpu::Glyph> = run
            .glyphs
            .iter()
            .filter(|g| g.id != 0)
            .map(|g| vello_cpu::Glyph {
                id: g.id,
                x: self.px(g.x as f64) as f32,
                y: self.px(g.y as f64) as f32,
            })
            .collect();
        if glyphs.is_empty() {
            return;
        }
        // 行内背景色
        if let Some(bg) = run.background_color {
            let pad = run.font_size as f64 * 0.1;
            let bx = position.x + run.baseline_x as f64 - pad;
            let bw = run.advance as f64 + pad * 2.0;
            let bh = run.font_size as f64 * 1.25;
            self.fill_rect(bx, position.y, bw, bh, &bg);
        }
        self.ctx.set_paint(Self::color(&run.color));
        // Arc<Vec<u8>> → FontData（linebender Blob）
        let blob: std::sync::Arc<dyn AsRef<[u8]> + Send + Sync> = run.font_data.clone();
        let font_data = vello_cpu::peniko::FontData::new(vello_cpu::peniko::Blob::new(blob), 0);
        let font_size_px = self.px(run.font_size as f64) as f32;
        self.ctx
            .glyph_run(&mut self.resources, &font_data)
            .font_size(font_size_px)
            .glyph_transform(transform)
            .fill_glyphs(glyphs.into_iter());
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
        match &block.kind {
            BlockKind::Heading { level, children } => {
                let size = heading_font_size(*level);
                let color = style.color;
                for child in children {
                    if let BlockKind::Paragraph { lines } = &child.kind {
                        let styled = apply_heading_style(lines, size, color);
                        self.draw_doc_lines(&styled, x, y);
                    }
                }
            }
            BlockKind::Paragraph { lines } => {
                self.draw_doc_lines(lines, x, y);
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
                self.fill_rect(x, y, self.content_w, h, &bg);
                let mut cy = y + 4.0;
                let mono = super::common::text_style_from_resolved(style);
                for raw in code.lines() {
                    let segments = [(raw, &mono)];
                    let layout = crate::document::text::layout_text(
                        &segments,
                        None,
                        crate::ast::TextAlign::Left,
                    );
                    if let Some(tl) = layout.lines.last() {
                        self.draw_doc_lines(std::slice::from_ref(tl), x + 4.0, cy);
                    }
                    cy += lh;
                }
            }
            BlockKind::ThematicBreak => {
                self.stroke_line(
                    x,
                    y + 2.0,
                    x + self.content_w,
                    y + 2.0,
                    &Color::new(0, 0, 0),
                    0.75,
                );
            }
            BlockKind::Image(img) => {
                let (w, h) = (img.size.0, img.size.1);
                if !img.data.is_empty() {
                    self.draw_image(&img.data, x, y, w, h);
                } else if !img.alt.is_empty() {
                    // 无字节时回退为占位色块
                    let g = Color::new(200, 200, 200);
                    self.fill_rect(x, y, w, h, &g);
                }
            }
            BlockKind::Blockquote { children } => {
                let bar_color = style.border_color.unwrap_or(Color::new(176, 176, 176));
                let inner_x = x + BQ_BAR_WIDTH + BQ_PAD_X;
                let text_h =
                    super::common::blockquote_content_height(children, &self.settings, inner_x);
                let content_h = text_h + 2.0 * BQ_PAD_Y;
                self.fill_rect(x, y, BQ_BAR_WIDTH, content_h, &bar_color);
                self.draw_blocks(children, inner_x, y + BQ_PAD_Y);
            }
            BlockKind::List { children, .. } => {
                self.draw_blocks(children, x, y);
            }
            BlockKind::ListItem { marker, children } => {
                // marker 在缩进槽左缘单独绘制（矢量圆点 / 有序数字），正文整体缩进。
                let indent = super::common::list_item_indent(marker, style);
                self.draw_list_marker(marker, false, false, x, y, style);
                self.draw_blocks(children, x + indent, y);
            }
            BlockKind::TaskListItem {
                marker,
                checked,
                children,
            } => {
                let indent = super::common::list_item_indent(marker, style);
                self.draw_list_marker(marker, true, *checked, x, y, style);
                self.draw_blocks(children, x + indent, y);
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
            _ => {
                // 其余叶子块：忽略（PNG 以已布局文本为准）
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
        // 单元格内边距取自样式（与 from_ast 行高计算、pdf.rs 绘制保持一致）。
        let pad_h = style.table_cell_padding_h_pt as f64;
        let pad_v = style.table_cell_padding_v_pt as f64;
        let header_bg = style.table_header_bg.unwrap_or(Color::new(230, 230, 230));
        let header_h = table_row_height(style, row_heights, 0);
        if let Some(h) = rows.first() {
            self.fill_rect(x, y, content_w, header_h, &header_bg);
            let mut cx = x;
            for (ci, cell) in h.cells.iter().enumerate() {
                let cw = col_widths.get(ci).copied().unwrap_or(0.0);
                self.draw_cell(cell, cx + pad_h, y + pad_v);
                cx += cw;
            }
        }
        let mut cy = y + header_h;
        for (ri, row) in rows.iter().enumerate().skip(1) {
            let rh = table_row_height(style, row_heights, ri);
            if ri % 2 == 1 {
                let alt = style.table_alt_row_bg.unwrap_or(Color::new(255, 255, 255));
                self.fill_rect(x, cy, content_w, rh, &alt);
            }
            let mut cx = x;
            for (ci, cell) in row.cells.iter().enumerate() {
                let cw = col_widths.get(ci).copied().unwrap_or(0.0);
                self.draw_cell(cell, cx + pad_h, cy + pad_v);
                cx += cw;
            }
            cy += rh;
        }
        // 表格边框线（外框 + 列分隔竖线 + 行分隔横线），颜色/宽度取自样式。
        // 注意：竖线压住横线，横线压住外框，交点处视觉更连贯。
        for seg in table_border_segments(rows, col_widths, row_heights, style, x, y) {
            self.stroke_line(seg.x1, seg.y1, seg.x2, seg.y2, &seg.color, seg.width);
        }
    }

    /// 绘制嵌入图片：用 `image` crate 解码并按目标像素尺寸缩放，构造
    /// `vello_cpu::Pixmap` 后通过 `Resources::register_image` 注册并用
    /// `ImageSource::OpaqueId` 作为 `Image` paint，由 vello 在绘制阶段直接合成
    /// （src-over 到已有的白底之上）。
    ///
    /// 使用 `OpaqueId` + `register_image` 路径（而非 `ImageSource::Pixmap` 内嵌），
    /// 因为后者在 vello_cpu 0.2 的光栅化中对 `Image` paint 存在缺陷，图片无法
    /// 正常绘制；`OpaqueId` 路径在 `RenderContext::set_paint` 文档中明确受支持。
    fn draw_image(&mut self, data: &[u8], x: f64, y: f64, w: f64, h: f64) {
        if data.is_empty() {
            return;
        }
        let dyn_img = match image::load_from_memory(data) {
            Ok(img) => img,
            Err(e) => {
                eprintln!(
                    "[liepress] draw_image: image decode failed ({} bytes): {}",
                    data.len(),
                    e
                );
                // 解码失败：回退为灰色占位块。
                self.fill_rect(x, y, w, h, &Color::new(200, 200, 200));
                return;
            }
        };
        // 按目标像素尺寸缩放，避免超大原图并提升合成质量。
        let target_w = ((w * self.scale).ceil() as u32).max(1).min(u16::MAX as u32);
        let target_h = ((h * self.scale).ceil() as u32).max(1).min(u16::MAX as u32);
        let scaled =
            dyn_img.resize_exact(target_w, target_h, image::imageops::FilterType::Triangle);
        let rgba = scaled.to_rgba8();
        let pixels: Vec<vello_cpu::color::PremulRgba8> = rgba
            .as_raw()
            .chunks_exact(4)
            .map(|p| {
                let [r, g, b, a] = [p[0], p[1], p[2], p[3]];
                vello_cpu::color::PremulRgba8 {
                    r: ((a as u32 * r as u32) / 255) as u8,
                    g: ((a as u32 * g as u32) / 255) as u8,
                    b: ((a as u32 * b as u32) / 255) as u8,
                    a,
                }
            })
            .collect();
        let pixmap = vello_cpu::Pixmap::from_parts(pixels, target_w as u16, target_h as u16);
        let brush = vello_cpu::Image {
            image: vello_cpu::ImageSource::Pixmap(std::sync::Arc::new(pixmap)),
            sampler: vello_cpu::peniko::ImageSampler::default(),
        };
        self.ctx.set_paint(brush);
        // `Image` paint 按 pixmap 的像素坐标 (0,0) 起始绘制，必须用 paint transform
        // 把它平移（必要时缩放）到目标矩形区域；否则图片只会被画在画布原点附近，
        // fill_rect 仅定义允许 paint 显示的裁剪形状，导致图片几乎不可见。
        // 参考 vello_cpu 官方 `paints.rs` 的 pattern 示例（set_paint_transform）。
        let rect = Rect::new(self.px(x), self.px(y), self.px(x + w), self.px(y + h));
        self.ctx
            .set_paint_transform(vello_cpu::kurbo::Affine::translate((rect.x0, rect.y0)));
        self.ctx.fill_rect(&rect);
        self.ctx.reset_paint_transform();
    }
}
