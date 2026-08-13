//! PNG 输出后端。
//!
//! 消费 [`crate::document::layout::Document`]（已布局块树），用 `vello_cpu`
//! 光栅化为**不分页**的长图 PNG。文本用字形轮廓（从 `TextRun.font_data` 加载
//! 字体，`glyph_run` 绘制），保证与 PDF 视觉一致。

use crate::color::Color;
use crate::document::layout::{Block, BlockKind, Document, TableCell, TableRow};
use crate::document::text::{TextLine, TextRun};
use crate::document::types::page::PageSettings;
use crate::document::types::ResolvedStyle;
use crate::error::Result;

use super::common::{
    BQ_BAR_WIDTH, BQ_PAD_X, BQ_PAD_Y, apply_heading_style, block_height, heading_font_size,
    table_border_segments, table_row_height,
};

use vello_cpu::kurbo::{Affine, BezPath, Point, Rect};
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
        images: Vec::new(),
    };
    r.draw_blocks(&document.blocks, margin_left, margin_top);

    let mut pixmap = vello_cpu::Pixmap::new(pw16, ph16);
    r.ctx.render(&mut pixmap, &mut r.resources);
    // 白色背景：vello 输出默认透明，这里把完全透明像素铺为白色（premultiplied）。
    fill_transparent_white(&mut pixmap);
    // 合成嵌入图片（vello_cpu 0.2 的 Image paint 有缺陷，改为 image crate 后合成）
    composite_images(&mut pixmap, &r.images, scale as f64);
    let png = pixmap
        .into_png()
        .map_err(|e| crate::error::Error::RenderError(format!("{}", e)))?;
    Ok(png)
}

/// 把 pixmap 中完全透明（alpha=0）的像素填充为白色。
fn fill_transparent_white(pixmap: &mut vello_cpu::Pixmap) {
    let data = pixmap.data_mut();
    for px in data.iter_mut() {
        if px.a == 0 {
            px.r = 255;
            px.g = 255;
            px.b = 255;
            px.a = 255;
        }
    }
}

/// 将嵌入图片合成到 pixmap 的目标区域（vello_cpu 0.2 的 Image paint 有缺陷，
/// 改用 `image` crate 解码 + 缩放 + 直接写像素）。
///
/// 需要先调用 [`fill_transparent_white`] 铺白背景，图片按其自身 alpha 覆盖。
fn composite_images(
    pixmap: &mut vello_cpu::Pixmap,
    images: &[PendingImage],
    scale: f64,
) {
    if images.is_empty() {
        return;
    }
    let (pix_w, pix_h) = (pixmap.width() as usize, pixmap.height() as usize);
    let data = pixmap.data_mut();
    for img in images {
        let Ok(dyn_img) = image::load_from_memory(&img.data) else {
            continue;
        };
        let target_w = ((img.w * scale).ceil() as u32).max(1);
        let target_h = ((img.h * scale).ceil() as u32).max(1);
        let scaled = dyn_img.resize_exact(target_w, target_h, image::imageops::FilterType::Triangle);
        let rgba = scaled.to_rgba8();
        let raw = rgba.as_raw();
        let dx = (img.x * scale).round() as i64;
        let dy = (img.y * scale).round() as i64;
        for py in 0..target_h as i64 {
            let yy = dy + py;
            if yy < 0 || yy >= pix_h as i64 {
                continue;
            }
            for px in 0..target_w as i64 {
                let xx = dx + px;
                if xx < 0 || xx >= pix_w as i64 {
                    continue;
                }
                let idx = yy as usize * pix_w + xx as usize;
                if idx >= data.len() {
                    continue;
                }
                let si = (py as usize * target_w as usize + px as usize) * 4;
                let a = raw[si + 3];
                if a == 0 {
                    continue;
                }
                // 预乘 alpha
                let premul = |c: u8| ((a as u16 * c as u16) / 255) as u8;
                data[idx].r = premul(raw[si]);
                data[idx].g = premul(raw[si + 1]);
                data[idx].b = premul(raw[si + 2]);
                data[idx].a = a;
            }
        }
    }
}

/// 待合成的嵌入图片：原始字节 + 目标区域（pt 坐标）。
struct PendingImage {
    data: Vec<u8>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

struct PngRenderer {
    ctx: RenderContext,
    resources: Resources,
    content_w: f64,
    scale: f64,
    settings: PageSettings,
    /// 待合成的嵌入图片（vello_cpu 0.2 的 Image paint 有缺陷，改用后处理合成）
    images: Vec<PendingImage>,
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
        self.ctx.set_stroke(vello_cpu::kurbo::Stroke::new(self.px(width)));
        self.ctx.stroke_path(&path);
    }

    fn draw_doc_lines(&mut self, lines: &[TextLine], x: f64, y: f64) {
        for line in lines {
            let line_x = x + line.bounds.x0 as f64;
            let line_y = y + line.bounds.y0 as f64;
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
        let font_data =
            vello_cpu::peniko::FontData::new(vello_cpu::peniko::Blob::new(blob), 0);
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
                    let layout = crate::document::text::layout_text(&segments, None, crate::ast::TextAlign::Left);
                    if let Some(tl) = layout.lines.last() {
                        self.draw_doc_lines(&[tl.clone()], x + 4.0, cy);
                    }
                    cy += lh;
                }
            }
            BlockKind::ThematicBreak => {
                self.stroke_line(x, y + 2.0, x + self.content_w, y + 2.0, &Color::new(0, 0, 0), 0.75);
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
            BlockKind::ListItem { children, .. } | BlockKind::TaskListItem { children, .. } => {
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
        let header_bg = style.table_header_bg.unwrap_or(Color::new(230, 230, 230));
        let header_h = table_row_height(style, row_heights, 0);
        if let Some(h) = rows.first() {
            self.fill_rect(x, y, content_w, header_h, &header_bg);
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
                self.fill_rect(x, cy, content_w, rh, &alt);
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
        // 注意：竖线压住横线，横线压住外框，交点处视觉更连贯。
        for seg in table_border_segments(rows, col_widths, row_heights, style, x, y) {
            self.stroke_line(seg.x1, seg.y1, seg.x2, seg.y2, &seg.color, seg.width);
        }
    }

    /// 记录嵌入图片（vello_cpu 0.2 的 Image paint 有缺陷，渲染后由
    /// [`composite_images`] 用 `image` crate 合成到 pixmap）。
    fn draw_image(&mut self, data: &[u8], x: f64, y: f64, w: f64, h: f64) {
        if data.is_empty() {
            return;
        }
        self.images.push(PendingImage {
            data: data.to_vec(),
            x,
            y,
            w,
            h,
        });
    }
}
