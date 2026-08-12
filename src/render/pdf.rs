//! PDF 渲染器 - 使用 krilla。
//!
//! 本模块是 PDF 输出后端，**直接消费 [`crate::document::skeleton::DocumentSkeleton`]**
//! （方案 §5.1：后端直接消费 DocumentSkeleton，不经 VisualElement 投影）。
//!
//! 分页（切页、跨页表格 MultiSpill）在 [`paginate_skeleton`] 内完成，符合方案
//! §1.1/§1.3.1/§4.1「Document 不知道页，分页是各输出后端职责」。

use std::collections::HashMap;
use std::sync::Arc;

use krilla::action::{Action, LinkAction};
use krilla::annotation::{Annotation, LinkAnnotation, Target};
use krilla::color::rgb::Color as RgbColor;
use krilla::document::Document;
use krilla::geom::{PathBuilder, Point as KrillaPoint, Rect as KrillaRect, Size, Transform as KrillaTransform};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings as KrillaPageSettings;
use krilla::paint::{Fill, FillRule, LineCap, LineJoin, Paint, Stroke as KrillaStroke};
use krilla::surface::Surface;
use krilla::text::Font;
use vello_cpu::kurbo::Point;

use crate::color::Color;
use crate::document::skeleton::{BlockKind, DocumentSkeleton, SkeletonBlock, TableRow};
use crate::document::types::page::PageSettings;
use crate::document::types::{DocColor, DocTextLine, DocTextRun, ResolvedStyle, TextAlign};
use crate::error::{Error, Result};
use crate::text::{Glyph, TextAlign as LayoutAlign, TextDecoration, TextRun, TextStyle, layout_text};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FontCacheKey {
    data_ptr: *const u8,
    data_len: usize,
    index: u32,
}

impl FontCacheKey {
    fn from_font_data(font_data: &parley::FontData) -> Self {
        let data = font_data.data.as_ref();
        Self {
            data_ptr: data.as_ptr(),
            data_len: data.len(),
            index: font_data.index,
        }
    }
}

/// PDF 后端分页后的单页（仅 PDF 后端内部使用，不污染 document 层）。
#[derive(Clone, Debug, Default)]
struct PdfPage {
    blocks: Vec<PositionedBlock>,
    header: Option<String>,
    footer: Option<String>,
}

/// PDF 后端分页后的绝对定位块。
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct PositionedBlock {
    block: SkeletonBlock,
    /// 内容区左上角为原点的绝对坐标（pt）
    x: f64,
    y: f64,
    height: f64,
    /// 若该块是跨页表格的续页片段，需重复表头
    repeat_table_header: bool,
}

impl PositionedBlock {
    fn new(block: SkeletonBlock, x: f64, y: f64, height: f64) -> Self {
        Self {
            block,
            x,
            y,
            height,
            repeat_table_header: false,
        }
    }
}

/// PDF 文档生成器：消费 [`DocumentSkeleton`]，内部分页并渲染为 PDF 字节。
pub struct PdfDocumentGenerator {
    skeleton: DocumentSkeleton,
    settings: PageSettings,
}

impl PdfDocumentGenerator {
    /// 从源 IR（不分页的 [`DocumentSkeleton`]）构造生成器。
    pub fn from_skeleton(skeleton: DocumentSkeleton, settings: PageSettings) -> Self {
        Self { skeleton, settings }
    }

    /// 生成 PDF 字节数据。
    pub fn generate(&self) -> Result<Vec<u8>> {
        let pages = paginate_skeleton(&self.skeleton, &self.settings);

        let mut krilla_doc = Document::new();
        for (idx, page) in pages.iter().enumerate() {
            let width = self.settings.width_pt;
            let height = self.settings.height_pt;
            let size = Size::from_wh(width, height).ok_or_else(|| Error::RenderError("invalid page size".into()))?;
            let mut krilla_page = krilla_doc.start_page_with(KrillaPageSettings::new(size));
            let mut surface = krilla_page.surface();

            Self::draw_white_background(&mut surface, width, height);

            let content_w = self.settings.content_width() as f64;
            let links = {
                let mut renderer = PdfRenderer::new(&mut surface, content_w, self.settings.clone());
                if let Some(h) = &page.header {
                    renderer.draw_text(
                        h,
                        self.settings.content_x() as f64,
                        (self.settings.margin_top_pt - 6.0).max(2.0) as f64,
                        9.0,
                        LayoutAlign::Center,
                        width as f64,
                    );
                }
                if let Some(f) = &page.footer {
                    let footer_text = f
                        .replace("{page}", &format!("{}", idx + 1))
                        .replace("{total}", &format!("{}", pages.len()));
                    renderer.draw_text(
                        &footer_text,
                        self.settings.content_x() as f64,
                        (height - self.settings.margin_bottom_pt + 4.0) as f64,
                        9.0,
                        LayoutAlign::Center,
                        width as f64,
                    );
                }
                for pb in &page.blocks {
                    renderer.draw_block(&pb.block, pb.x, pb.y, pb.repeat_table_header);
                }
                renderer.take_links()
            };

            surface.finish();

            for (lx, ly, lw, lh, url) in links {
                if let Some(rect) = KrillaRect::from_xywh(lx, ly, lw, lh) {
                    let action = Action::from(LinkAction::new(url.clone()));
                    let link_annotation = LinkAnnotation::new(rect, Target::Action(action));
                    krilla_page.add_annotation(Annotation::new_link(link_annotation, None));
                }
            }

            krilla_page.finish();
        }

        krilla_doc
            .finish()
            .map_err(|e| Error::RenderError(format!("{:?}", e)))
    }

    fn draw_white_background(surface: &mut Surface, width: f32, height: f32) {
        let mut pb = PathBuilder::new();
        pb.move_to(0.0, 0.0);
        pb.line_to(width, 0.0);
        pb.line_to(width, height);
        pb.line_to(0.0, height);
        pb.close();
        if let Some(path) = pb.finish() {
            let fill = Fill {
                paint: Paint::from(RgbColor::new(255, 255, 255)),
                opacity: NormalizedF32::ONE,
                rule: FillRule::NonZero,
            };
            surface.set_fill(Some(fill));
            surface.draw_path(&path);
        }
    }
}

/// PDF 绘制器：在单个 krilla Surface 上绘制 SkeletonBlock。
struct PdfRenderer<'a, 's> {
    surface: &'s mut Surface<'a>,
    font_cache: HashMap<FontCacheKey, Font>,
    links: Vec<(f32, f32, f32, f32, String)>,
    content_w: f64,
    settings: PageSettings,
}

impl<'a, 's> PdfRenderer<'a, 's> {
    fn new(surface: &'s mut Surface<'a>, content_w: f64, settings: PageSettings) -> Self {
        Self {
            surface,
            font_cache: HashMap::new(),
            links: Vec::new(),
            content_w,
            settings,
        }
    }

    fn take_links(&mut self) -> Vec<(f32, f32, f32, f32, String)> {
        std::mem::take(&mut self.links)
    }

    // ─── 基础绘制原语 ──────────────────────────────────────

    fn draw_rect(&mut self, x: f64, y: f64, w: f64, h: f64, fill: Option<DocColor>, stroke: Option<(DocColor, f64)>) {
        let mut pb = PathBuilder::new();
        pb.move_to(x as f32, y as f32);
        pb.line_to((x + w) as f32, y as f32);
        pb.line_to((x + w) as f32, (y + h) as f32);
        pb.line_to(x as f32, (y + h) as f32);
        pb.close();
        if let Some(path) = pb.finish() {
            if let Some(c) = fill {
                let fill = Fill {
                    paint: Paint::from(RgbColor::new(c.r, c.g, c.b)),
                    opacity: NormalizedF32::new(c.a as f32 / 255.0).unwrap_or(NormalizedF32::ONE),
                    rule: FillRule::NonZero,
                };
                self.surface.set_fill(Some(fill));
            } else {
                self.surface.set_fill(None);
            }
            if let Some((c, wt)) = stroke {
                let stroke = KrillaStroke {
                    paint: Paint::from(RgbColor::new(c.r, c.g, c.b)),
                    opacity: NormalizedF32::new(c.a as f32 / 255.0).unwrap_or(NormalizedF32::ONE),
                    width: wt as f32,
                    miter_limit: 4.0,
                    line_cap: LineCap::default(),
                    line_join: LineJoin::default(),
                    dash: None,
                };
                self.surface.set_stroke(Some(stroke));
            } else {
                self.surface.set_stroke(None);
            }
            self.surface.draw_path(&path);
        }
    }

    fn draw_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: DocColor, width: f64) {
        let mut pb = PathBuilder::new();
        pb.move_to(x1 as f32, y1 as f32);
        pb.line_to(x2 as f32, y2 as f32);
        if let Some(path) = pb.finish() {
            self.surface.set_fill(None);
            let stroke = KrillaStroke {
                paint: Paint::from(RgbColor::new(color.r, color.g, color.b)),
                opacity: NormalizedF32::new(color.a as f32 / 255.0).unwrap_or(NormalizedF32::ONE),
                width: width as f32,
                miter_limit: 4.0,
                line_cap: LineCap::default(),
                line_join: LineJoin::default(),
                dash: None,
            };
            self.surface.set_stroke(Some(stroke));
            self.surface.draw_path(&path);
        }
    }

    /// 绘制单行纯文本（用于页眉/页脚等），居中于 [x, x+width]。
    fn draw_text(&mut self, text: &str, x: f64, y: f64, font_size: f64, align: LayoutAlign, width: f64) {
        if text.is_empty() {
            return;
        }
        let style = text_style(Color::new(120, 120, 120), "serif", font_size as f32, "normal", "normal");
        let segments = [(text, &style)];
        let layout = layout_text(&segments, None, align);
        let line = match layout.lines.last() {
            Some(l) => l,
            None => return,
        };
        let tx = match align {
            LayoutAlign::Center => x + (width - line.bounds.width()) / 2.0,
            LayoutAlign::Right => x + (width - line.bounds.width()),
            _ => x,
        };
        for run in &line.runs {
            self.draw_text_run(run, Point::new(tx, y));
        }
    }

    fn draw_image(&mut self, data: &[u8], format: &str, position: Point, size: (f64, f64)) {
        use krilla::image::Image;

        let image = match format.to_lowercase().as_str() {
            "png" => Image::from_png(krilla::Data::from(data.to_vec()), true),
            "jpeg" | "jpg" => Image::from_jpeg(krilla::Data::from(data.to_vec()), true),
            "gif" => Image::from_gif(krilla::Data::from(data.to_vec()), true),
            "webp" => Image::from_webp(krilla::Data::from(data.to_vec()), true),
            _ => return,
        };

        let image = match image {
            Ok(img) => img,
            Err(_) => return,
        };

        self.surface
            .push_transform(&KrillaTransform::from_translate(position.x as f32, position.y as f32));

        let Some(image_size) = Size::from_wh(size.0 as f32, size.1 as f32) else {
            self.surface.pop();
            return;
        };
        self.surface.draw_image(image, image_size);
        self.surface.pop();
    }

    /// 绘制 parley 文本 run（接受 [`TextRun`]，由 `DocTextRun` 重建而来）。
    fn draw_text_run(&mut self, run: &TextRun, position: Point) {
        use krilla::text::GlyphId;

        if run.glyphs.is_empty() {
            return;
        }

        if let Some(bg) = run.background_color {
            let pad = run.font_size * 0.1;
            let rx = position.x as f32 + run.baseline_x - pad;
            let ry = position.y as f32;
            let rw = run.advance + pad * 2.0;
            let rh = run.font_size * 1.25;
            let mut pb = PathBuilder::new();
            pb.move_to(rx, ry);
            pb.line_to(rx + rw, ry);
            pb.line_to(rx + rw, ry + rh);
            pb.line_to(rx, ry + rh);
            pb.close();
            if let Some(path) = pb.finish() {
                let fill = Fill {
                    paint: Paint::from(RgbColor::new(bg.r, bg.g, bg.b)),
                    opacity: NormalizedF32::new(bg.a as f32 / 255.0).unwrap_or(NormalizedF32::ONE),
                    rule: FillRule::NonZero,
                };
                self.surface.set_fill(Some(fill));
                self.surface.draw_path(&path);
            }
        }

        let font_key = FontCacheKey::from_font_data(&run.font_data);
        if let std::collections::hash_map::Entry::Vacant(e) = self.font_cache.entry(font_key) {
            let data = run.font_data.data.as_ref();
            if let Some(font) = Font::new(krilla::Data::from(data.to_vec()), run.font_data.index) {
                e.insert(font);
            } else {
                return;
            }
        }
        let krilla_font = match self.font_cache.get(&font_key) {
            Some(f) => f.clone(),
            None => return,
        };

        let krilla_origin = KrillaPoint::from_xy(
            position.x as f32 + run.baseline_x,
            position.y as f32 + run.baseline_y,
        );

        if let Some(ref url) = run.url {
            let link_x = position.x as f32 + run.baseline_x;
            let link_y = position.y as f32;
            let link_w = run.advance;
            let link_h = run.font_size * 1.4;
            self.links.push((link_x, link_y, link_w, link_h, url.clone()));
        }

        let mut krilla_glyphs = Vec::new();
        for g in &run.glyphs {
            if g.id == 0 {
                continue;
            }
            krilla_glyphs.push(krilla::text::KrillaGlyph::new(
                GlyphId::new(g.id),
                0.0,
                (g.x - run.baseline_x) / run.font_size,
                (g.y - run.baseline_y) / run.font_size,
                0.0,
                run.text_range.clone(),
                None,
            ));
        }

        if krilla_glyphs.is_empty() {
            return;
        }

        self.surface.set_stroke(None);

        let krilla_color = RgbColor::new(run.color.r, run.color.g, run.color.b);
        let paint = Paint::from(krilla_color);
        let opacity = NormalizedF32::new(run.color.a as f32 / 255.0).unwrap_or(NormalizedF32::ONE);
        let fill = Fill {
            paint,
            opacity,
            rule: FillRule::NonZero,
        };

        self.surface.set_fill(Some(fill.clone()));
        self.surface.draw_glyphs(
            krilla_origin,
            &krilla_glyphs,
            krilla_font,
            &run.text,
            run.font_size,
            false,
        );

        if run.decoration == TextDecoration::LineThrough {
            let strike_y = position.y as f32 + run.baseline_y - run.font_size * 0.3;
            let line_x1 = position.x as f32 + run.baseline_x;
            let line_x2 = line_x1 + run.advance;
            let stroke_w = (run.font_size * 0.06).max(0.5);

            self.surface.set_fill(None);
            let stroke = KrillaStroke {
                paint: Paint::from(krilla_color),
                opacity,
                width: stroke_w,
                miter_limit: 4.0,
                line_cap: LineCap::default(),
                line_join: LineJoin::default(),
                dash: None,
            };
            self.surface.set_stroke(Some(stroke));

            let mut pb = PathBuilder::new();
            pb.move_to(line_x1, strike_y);
            pb.line_to(line_x2, strike_y);
            if let Some(path) = pb.finish() {
                self.surface.draw_path(&path);
            }

            self.surface.set_stroke(None);
            self.surface.set_fill(Some(fill));
        }
    }

    // ─── DocTextRun → TextRun 重建 ─────────────────────────

    /// 把文档层投影的 [`DocTextRun`] 还原为 parley 的 [`TextRun`]，供 `draw_text_run` 使用。
    ///
    /// `DocTextRun` 是 `TextRun` 的精确投影（含 `font_data` 原始字节与字形），
    /// 直接重建即可正确嵌入子字体与绘制，符合方案 §5.1（PDF 直接消费 Skeleton）。
    fn doc_run_to_text_run(&self, dr: &DocTextRun) -> TextRun {
        TextRun {
            text: dr.text.clone(),
            text_range: dr.text_range.clone(),
            font_data: parley::FontData::new(parley::fontique::Blob::new(Arc::new(dr.font_data.clone())), 0),
            font_size: dr.font_size,
            color: Color::from(dr.color),
            advance: dr.advance,
            glyphs: dr
                .glyphs
                .iter()
                .map(|g| Glyph {
                    id: g.id,
                    x: g.x,
                    y: g.y,
                    advance: g.advance,
                    cluster: g.cluster,
                })
                .collect(),
            is_rtl: false,
            baseline_x: dr.baseline_x,
            baseline_y: dr.baseline_y,
            url: dr.url.clone(),
            decoration: crate::text::TextDecoration::from(dr.decoration),
            baseline_shift: dr.baseline_shift,
            background_color: dr.background_color.map(Color::from),
        }
    }

    // ─── SkeletonBlock 绘制 ─────────────────────────────────

    /// 绘制已排版的文档文本行（DocTextLine）。
    fn draw_doc_lines(&mut self, lines: &[DocTextLine], x: f64, y: f64) {
        for line in lines {
            let line_x = x + line.bounds.0;
            let line_y = y + line.bounds.1;
            for run in &line.runs {
                let tr = self.doc_run_to_text_run(run);
                self.draw_text_run(&tr, Point::new(line_x, line_y));
            }
        }
    }

    /// 绘制单个 SkeletonBlock（递归）。`x`/`y` 为内容区绝对坐标（pt）。
    fn draw_block(&mut self, block: &SkeletonBlock, x: f64, y: f64, repeat_table_header: bool) {
        let style = &block.style;
        match &block.kind {
            BlockKind::Heading { level, children } => {
                let size = heading_font_size(*level);
                let color = style.color;
                for child in children {
                    if let BlockKind::Paragraph { lines } = &child.kind {
                        let styled_lines = apply_heading_style(lines, size, color);
                        self.draw_doc_lines(&styled_lines, x, y);
                    }
                }
            }
            BlockKind::Paragraph { lines } => {
                self.draw_doc_lines(lines, x, y);
            }
            BlockKind::CodeBlock { code, .. } => {
                let lh = 18.0;
                let n = code.lines().count().max(1);
                let h = n as f64 * lh + 8.0;
                self.draw_rect(
                    x,
                    y,
                    self.content_w,
                    h,
                    Some(DocColor {
                        r: 245,
                        g: 245,
                        b: 245,
                        a: 255,
                    }),
                    None,
                );
                let mono = text_style(
                    Color::new(30, 30, 30),
                    "monospace",
                    13.0,
                    "normal",
                    "normal",
                );
                let mut cy = y + 4.0;
                for raw in code.lines() {
                    let segments = [(raw, &mono)];
                    let layout = layout_text(&segments, None, LayoutAlign::Left);
                    if let Some(tl) = layout.lines.last() {
                        for run in &tl.runs {
                            self.draw_text_run(run, Point::new(x + 4.0, cy));
                        }
                    }
                    cy += lh;
                }
            }
            BlockKind::ThematicBreak => {
                self.draw_line(
                    x,
                    y + 1.0,
                    x + self.content_w,
                    y + 1.0,
                    DocColor {
                        r: 180,
                        g: 180,
                        b: 180,
                        a: 255,
                    },
                    0.8,
                );
            }
            BlockKind::Image(img) => {
                let (w, h) = if img.size.0 > 0.0 && img.size.1 > 0.0 {
                    (img.size.0, img.size.1)
                } else {
                    (self.content_w, 120.0)
                };
                if !img.data.is_empty() {
                    self.draw_image(&img.data, &img.format, Point::new(x, y), (w, h));
                } else if !img.alt.is_empty() {
                    let style = text_style(
                        Color::new(120, 120, 120),
                        "serif",
                        11.0,
                        "normal",
                        "normal",
                    );
                    let segments = [(img.alt.as_str(), &style)];
                    let layout = layout_text(&segments, None, LayoutAlign::Left);
                    if let Some(tl) = layout.lines.last() {
                        for run in &tl.runs {
                            self.draw_text_run(run, Point::new(x, y));
                        }
                    }
                }
            }
            BlockKind::Blockquote { children } => {
                let pad = 8.0;
                let inner_x = x + pad + 4.0;
                // 内容高度 = 子块高度之和 + 底部内边距
                let content_h = measure_block_recursive(children, &self.settings, inner_x) + 4.0;
                // 该块在父级中分配的总高度（含引用块自身的上下 margin）
                let total_h = block_height(block, &self.settings, x);
                // 左侧竖条跨越整个块（含 margin 区），内容在块内垂直居中
                self.draw_rect(
                    x,
                    y,
                    2.0,
                    total_h,
                    Some(DocColor {
                        r: 200,
                        g: 200,
                        b: 200,
                        a: 255,
                    }),
                    None,
                );
                let offset = ((total_h - content_h) / 2.0).max(0.0);
                let mut cy = y + offset;
                for child in children {
                    self.draw_block(child, inner_x, cy, false);
                    cy += block_height(child, &self.settings, inner_x);
                }
            }
            BlockKind::List { children, .. } => {
                let mut cy = y;
                for child in children {
                    self.draw_block(child, x, cy, false);
                    cy += block_height(child, &self.settings, x);
                }
            }
            BlockKind::ListItem { marker, children, .. } => {
                let mstyle = text_style(
                    Color::from(style.color),
                    style.font_family.first().map(|s| s.as_str()).unwrap_or("serif"),
                    style.font_size_pt,
                    if style.font_style_italic { "italic" } else { "normal" },
                    if style.font_weight_bold { "bold" } else { "normal" },
                );
                let mseg = [(marker.as_str(), &mstyle)];
                let mlayout = layout_text(&mseg, None, LayoutAlign::Left);
                if let Some(tl) = mlayout.lines.last() {
                    for run in &tl.runs {
                        self.draw_text_run(run, Point::new(x, y));
                    }
                }
                let mut cy = y;
                for child in children {
                    self.draw_block(child, x + 18.0, cy, false);
                    cy += block_height(child, &self.settings, x + 18.0);
                }
            }
            BlockKind::TaskListItem { checked, children, .. } => {
                let box_color = if *checked {
                    DocColor {
                        r: 40,
                        g: 120,
                        b: 220,
                        a: 255,
                    }
                } else {
                    DocColor {
                        r: 150,
                        g: 150,
                        b: 150,
                        a: 255,
                    }
                };
                self.draw_rect(x, y + 2.0, 10.0, 10.0, None, Some((box_color, 1.0)));
                let mut cy = y;
                for child in children {
                    self.draw_block(child, x + 18.0, cy, false);
                    cy += block_height(child, &self.settings, x + 18.0);
                }
            }
            BlockKind::Container { children, .. } => {
                let mut cy = y;
                for child in children {
                    self.draw_block(child, x, cy, false);
                    cy += block_height(child, &self.settings, x);
                }
            }
            BlockKind::Table { rows, .. } => {
                self.draw_table(rows, x, y, repeat_table_header);
            }
            BlockKind::Document { children, .. } => {
                let mut cy = y;
                for child in children {
                    self.draw_block(child, x, cy, false);
                    cy += block_height(child, &self.settings, x);
                }
            }
            // 以下变体不会作为顶层块出现（仅作为其它块的组成部分），
            // 穷尽 match 所需，绘制时不作处理。
            BlockKind::TableRow { .. }
            | BlockKind::TableCell { .. }
            | BlockKind::Text { .. }
            | BlockKind::InlineCode { .. }
            | BlockKind::Link { .. }
            | BlockKind::LineBreak => {}
        }
    }

    /// 绘制表格（含跨页续页重复表头）。
    fn draw_table(&mut self, rows: &[TableRow], x: f64, y: f64, repeat_header: bool) {
        if rows.is_empty() {
            return;
        }
        let content_w = self.content_w;
        let ncols = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0).max(1);
        let col_w = content_w / ncols as f64;

        let header = rows.first();
        let body: &[TableRow] = if repeat_header { rows } else { &rows[1.min(rows.len())..] };

        let row_h = 18.0;
        let border = DocColor {
            r: 200,
            g: 200,
            b: 200,
            a: 255,
        };
        let draw_row = |r: &mut PdfRenderer<'_, '_>, row: &TableRow, cy: f64, is_header: bool| {
            r.draw_rect(
                x,
                cy,
                content_w,
                row_h,
                Some(if is_header {
                    DocColor {
                        r: 230,
                        g: 230,
                        b: 230,
                        a: 255,
                    }
                } else {
                    DocColor {
                        r: 255,
                        g: 255,
                        b: 255,
                        a: 255,
                    }
                }),
                Some((border, 0.5)),
            );
            // 列间分隔竖线：在每列交界处画一条垂直边框线。
            for col in 1..ncols {
                let sep_x = x + col as f64 * col_w;
                r.draw_line(sep_x, cy, sep_x, cy + row_h, border, 0.5);
            }
            let mut cx = x;
            for cell in row.cells.iter() {
                let cell_x = cx + 3.0;
                let mut ccy = cy + 3.0;
                for child in &cell.children {
                    r.draw_block(child, cell_x, ccy, false);
                    ccy += block_height(child, &r.settings, cell_x) + 2.0;
                }
                cx += col_w;
            }
        };

        if let Some(h) = header {
            draw_row(self, h, y, true);
            let _ = row_h;
        }
        let mut cy = y;
        if header.is_some() {
            cy += row_h;
        }
        for row in body {
            draw_row(self, row, cy, false);
            cy += row_h;
        }
    }
}

// ─── 分页（PDF 后端职责）───────────────────────────────────

/// 把不分页的 [`DocumentSkeleton`] 切分为多页绝对定位块（PDF 后端内部分页）。
fn paginate_skeleton(skeleton: &DocumentSkeleton, settings: &PageSettings) -> Vec<PdfPage> {
    let content_x = settings.content_x() as f64;
    let content_y = settings.content_y() as f64;
    let content_h = settings.content_height() as f64;

    let mut pages: Vec<PdfPage> = Vec::new();
    let mut cur = PdfPage::default();
    let mut used = 0.0_f64;

    let header = settings.header.clone();
    let footer = settings.footer.clone();

    for block in &skeleton.blocks {
        let h = block_height(block, settings, content_x);
        if let BlockKind::Table { rows, column_align } = &block.kind {
            paginate_table(
                rows,
                column_align,
                &mut PaginateCtx {
                    pages: &mut pages,
                    cur: &mut cur,
                    used: &mut used,
                },
                content_x,
                content_y,
                content_h,
            );
            continue;
        }
        if used + h > content_h && used > 0.0 {
            cur.header = header.clone();
            cur.footer = footer.clone();
            pages.push(std::mem::take(&mut cur));
            used = 0.0;
        }
        cur.blocks.push(PositionedBlock::new(block.clone(), content_x, content_y + used, h));
        used += h;
    }
    cur.header = header;
    cur.footer = footer;
    pages.push(cur);
    pages
}

/// 分页累积状态（跨块/跨页共享）。
struct PaginateCtx<'a> {
    pages: &'a mut Vec<PdfPage>,
    cur: &'a mut PdfPage,
    used: &'a mut f64,
}

/// 表格分页：按整行切分，续页重复表头。
fn paginate_table(
    rows: &[TableRow],
    column_align: &[TextAlign],
    ctx: &mut PaginateCtx,
    content_x: f64,
    content_y: f64,
    content_h: f64,
) {
    let row_h = 18.0_f64;
    let header_h = row_h;
    let n = rows.len();
    let mut i = 0usize;

    loop {
        let mut fit = 0usize;
        let mut need = if ctx.cur.blocks.is_empty() && *ctx.used == 0.0 {
            header_h
        } else {
            0.0
        };
        while i + fit < n {
            let add = if i + fit == 0 { header_h } else { row_h };
            if need + add <= content_h - *ctx.used {
                need += add;
                fit += 1;
            } else {
                break;
            }
        }

        if fit == 0 {
            if !ctx.cur.blocks.is_empty() || *ctx.used > 0.0 {
                ctx.pages.push(std::mem::take(ctx.cur));
                *ctx.used = 0.0;
            }
            fit = 1;
            need = header_h;
        }

        let page_rows: Vec<TableRow> = rows[i..i + fit].to_vec();
        let is_continuation = i > 0;
        let mut pb = PositionedBlock::new(
            SkeletonBlock::new(
                BlockKind::Table {
                    rows: page_rows,
                    column_align: column_align.to_vec(),
                },
                ResolvedStyle::default(),
                false,
            ),
            content_x,
            content_y + *ctx.used,
            fit as f64 * row_h,
        );
        pb.repeat_table_header = is_continuation;
        ctx.cur.blocks.push(pb);
        *ctx.used += need;

        i += fit;
        if i >= n {
            break;
        }
        ctx.pages.push(std::mem::take(ctx.cur));
        *ctx.used = 0.0;
    }
}

/// 测量单个块高度（递归）。
///
/// 返回的块高**包含该块的上下 margin**（`style.margin_top` + `margin_bottom`），
/// 使不同元素之间产生垂直间距。容器块（List/Blockquote/Document 等）先累加
/// 子块高度（子块已含自身 margin），再加上容器自身的 margin。
fn block_height(block: &SkeletonBlock, settings: &PageSettings, x: f64) -> f64 {
    let style = &block.style;
    let base = match &block.kind {
        BlockKind::Heading { children, .. } => children.iter().map(|c| block_height(c, settings, x)).sum(),
        BlockKind::Paragraph { lines } => (lines.len().max(1) as f64) * style.line_height_pt as f64,
        BlockKind::CodeBlock { code, .. } => (code.lines().count().max(1) as f64) * 18.0 + 8.0,
        BlockKind::ThematicBreak => 4.0,
        BlockKind::Image(img) => {
            if img.size.1 > 0.0 {
                img.size.1
            } else {
                120.0
            }
        }
        BlockKind::Blockquote { children } => {
            children.iter().map(|c| block_height(c, settings, x + 12.0)).sum::<f64>() + 4.0
        }
        BlockKind::List { children, .. } => children.iter().map(|c| block_height(c, settings, x)).sum(),
        BlockKind::ListItem { children, .. } => children
            .iter()
            .map(|c| block_height(c, settings, x + 18.0))
            .sum::<f64>()
            .max(style.line_height_pt as f64),
        BlockKind::TaskListItem { children, .. } => children
            .iter()
            .map(|c| block_height(c, settings, x + 18.0))
            .sum::<f64>()
            .max(style.line_height_pt as f64),
        BlockKind::Container { children, .. } => children.iter().map(|c| block_height(c, settings, x)).sum(),
        BlockKind::Table { rows, .. } => (rows.len().max(1) as f64) * 18.0,
        BlockKind::TableRow { cells } => {
            cells.iter().map(|c| measure_block_recursive(&c.children, settings, x)).sum::<f64>().max(18.0)
        }
        BlockKind::TableCell { children } => children.iter().map(|c| block_height(c, settings, x)).sum(),
        BlockKind::Text { .. } => style.line_height_pt as f64,
        BlockKind::InlineCode { .. } => style.line_height_pt as f64,
        BlockKind::Link { children, .. } => {
            if children.is_empty() {
                style.line_height_pt as f64
            } else {
                children.iter().map(|c| block_height(c, settings, x)).sum()
            }
        }
        BlockKind::LineBreak => style.line_height_pt as f64 / 2.0,
        BlockKind::Document { children, .. } => children.iter().map(|c| block_height(c, settings, x)).sum(),
    };
    base + style.margin_top as f64 + style.margin_bottom as f64
}

/// 递归测量子块总高度（供 Blockquote 绘制背景用）。
fn measure_block_recursive(children: &[SkeletonBlock], settings: &PageSettings, x: f64) -> f64 {
    children.iter().map(|c| block_height(c, settings, x)).sum()
}

// ─── 辅助 ────────────────────────────────────────────────

fn heading_font_size(level: u8) -> f32 {
    match level {
        1 => 22.0,
        2 => 18.0,
        3 => 15.0,
        4 => 13.0,
        5 => 12.0,
        _ => 11.0,
    }
}

/// 构造 [`crate::text::TextStyle`](用于 layout_text)。
fn text_style(color: Color, family: &str, size: f32, weight: &str, style: &str) -> TextStyle {
    TextStyle {
        color,
        font_family: vec![family.to_string()],
        font_size: size as f64,
        font_weight: weight.to_string(),
        font_style: style.to_string(),
        align: LayoutAlign::Left,
        url: None,
        decoration: TextDecoration::None,
        baseline_shift: 0.0,
        background_color: None,
    }
}

/// 把标题文本行套用标题字号/颜色（from_ast 产出的 Paragraph 行是正文样式）。
fn apply_heading_style(lines: &[DocTextLine], size: f32, color: DocColor) -> Vec<DocTextLine> {
    lines
        .iter()
        .map(|line| {
            let mut nl = line.clone();
            for r in nl.runs.iter_mut() {
                r.font_size = size;
                r.color = color;
            }
            nl
        })
        .collect()
}
