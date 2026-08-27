//! PDF 渲染器 - 使用 krilla。
//!
//! 本模块是 PDF 输出后端，**直接消费 [`crate::document::layout::Document`]**
//! （方案 §5.1：后端直接消费 `Document`，不经 VisualElement 投影）。
//!
//! 分页（切页、跨页表格 MultiSpill）在 [`paginate_layout`] 内完成，符合方案
//! §1.1/§1.3.1/§4.1「Document 不知道页，分页是各输出后端职责」。

use std::collections::HashMap;

use krilla::action::{Action, LinkAction};
use krilla::annotation::{Annotation, LinkAnnotation, Target};
use krilla::color::rgb::Color as RgbColor;
use krilla::destination::{Destination, XyzDestination};
use krilla::document::Document as KrillaDocument;
use krilla::geom::{
    PathBuilder, Point as KrillaPoint, Rect as KrillaRect, Size, Transform as KrillaTransform,
};
use krilla::num::NormalizedF32;
use krilla::outline::{Outline, OutlineNode};
use krilla::page::PageSettings as KrillaPageSettings;
use krilla::paint::{Fill, FillRule, LineCap, LineJoin, Paint, Stroke as KrillaStroke};
use krilla::surface::Surface;
use krilla::text::Font;
use vello_cpu::kurbo::Point;

use crate::document::layout::{Block, BlockKind, Document, TableRow};
use crate::document::text::{
    TextAlign as LayoutAlign, TextDecoration, TextLine, TextRun, layout_text,
};
use crate::document::types::page::PageSettings;
use crate::document::types::{ResolvedStyle, TextAlign};
use crate::error::{Error, Result};
use lievisual::Color;

use super::common::{
    BQ_BAR_WIDTH, BQ_PAD_X, BQ_PAD_Y, apply_heading_style, block_height, blockquote_content_height,
    heading_font_size, text_style, text_style_from_resolved,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FontCacheKey {
    data_ptr: *const u8,
    data_len: usize,
    index: u32,
}

impl FontCacheKey {
    fn from_font_data(font_data: &[u8], index: u32) -> Self {
        Self {
            data_ptr: font_data.as_ptr(),
            data_len: font_data.len(),
            index,
        }
    }
}

/// lievisual `Color`（0–1 f64）→ krilla 的 `RgbColor`（0–255 u8）。
fn rgb_u8(c: lievisual::Color) -> RgbColor {
    let to_u8 = |v: f64| -> u8 { (v.clamp(0.0, 1.0) * 255.0).round() as u8 };
    RgbColor::new(to_u8(c.r), to_u8(c.g), to_u8(c.b))
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
    block: Block,
    /// 内容区左上角为原点的绝对坐标（pt）
    x: f64,
    y: f64,
    height: f64,
    /// 若该块是跨页表格的续页片段，需重复表头
    repeat_table_header: bool,
}

impl PositionedBlock {
    fn new(block: Block, x: f64, y: f64, height: f64) -> Self {
        Self {
            block,
            x,
            y,
            height,
            repeat_table_header: false,
        }
    }
}

/// PDF 文档生成器：消费 [`Document`]，内部分页并渲染为 PDF 字节。
pub struct PdfDocumentGenerator {
    document: Document,
    settings: PageSettings,
}

impl PdfDocumentGenerator {
    /// 从源 IR（不分页的 [`Document`]）构造生成器。
    pub fn from_layout(document: Document, settings: PageSettings) -> Self {
        Self { document, settings }
    }

    /// 生成 PDF 字节数据。
    pub fn generate(&self) -> Result<Vec<u8>> {
        let pages = paginate_layout(&self.document, &self.settings);

        // 收集脚注定义位置：id → (页索引, x, y)，供正文引用生成内部跳转。
        // 坐标原点为内容区左上角；XyzDestination 的 point 同样以内容区左上角为原点，
        // 由 krilla 自动转换为 PDF 底部原点。
        let footnote_targets: HashMap<String, (usize, f64, f64)> = {
            let mut m = HashMap::new();
            for (idx, page) in pages.iter().enumerate() {
                for pb in &page.blocks {
                    if let BlockKind::FootnoteDef { id, .. } = &pb.block.kind {
                        m.insert(id.clone(), (idx, pb.x, pb.y));
                    }
                }
            }
            m
        };

        // 收集脚注引用位置：正文里 url 形如 `#fn-def-<label>` 的链接所在块坐标，
        // 作为返回引用目标，key 取 `fn-ref-<label>`。使脚注定义区的 ↩ 能页内跳回引用处。
        let footnote_ref_targets: HashMap<String, (usize, f64, f64)> = {
            fn walk(
                block: &Block,
                page_idx: usize,
                x: f64,
                y: f64,
                out: &mut HashMap<String, (usize, f64, f64)>,
            ) {
                // 段落内的链接以 TextRun.url 形式存在（脚注引用即在此），需单独收集。
                if let BlockKind::Paragraph { lines } = &block.kind {
                    for l in lines {
                        for run in &l.runs {
                            if let Some(u) = run.url.as_deref()
                                && let Some(label) = u.strip_prefix("#fn-def-")
                            {
                                out.entry(format!("fn-ref-{}", label))
                                    .or_insert((page_idx, x, y));
                            }
                        }
                    }
                }
                if let BlockKind::Link { url, .. } = &block.kind
                    && let Some(label) = url.strip_prefix("#fn-def-")
                {
                    out.entry(format!("fn-ref-{}", label))
                        .or_insert((page_idx, x, y));
                }
                for child in block.kind.children() {
                    walk(child, page_idx, x, y, out);
                }
            }
            let mut m = HashMap::new();
            for (idx, page) in pages.iter().enumerate() {
                for pb in &page.blocks {
                    walk(&pb.block, idx, pb.x, pb.y, &mut m);
                }
            }
            m
        };

        // 收集目录条目：标题（Heading）块 → (层级, 文本, 页索引, y)，用于生成 PDF outline。
        // 同时建立「标题 slug → 跳转目标」映射，供正文/TOC 中的内部锚点链接（#anchor）
        // 跳转到对应标题（与 outline 行为一致，而非退化为外部 URL 动作）。
        let mut outline_entries: Vec<OutlineEntry> = Vec::new();
        let mut heading_targets: HashMap<String, (usize, f64, f64)> = HashMap::new();
        for (idx, page) in pages.iter().enumerate() {
            for pb in &page.blocks {
                if let BlockKind::Heading { level, .. } = &pb.block.kind {
                    let title = pb.block.text_content().trim().to_string();
                    if !title.is_empty() {
                        outline_entries.push(OutlineEntry {
                            level: *level,
                            title: title.clone(),
                            page_index: idx,
                            y: pb.y,
                        });
                        // 以 GitHub 风格 slug 作为锚点 key。
                        heading_targets.insert(github_slug(&title), (idx, 0.0, pb.y));
                    }
                }
            }
        }

        let mut krilla_doc = KrillaDocument::new();
        for (idx, page) in pages.iter().enumerate() {
            let width = self.settings.width_pt;
            let height = self.settings.height_pt;
            let size = Size::from_wh(width, height)
                .ok_or_else(|| Error::RenderError("invalid page size".into()))?;
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
                        content_w,
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
                        content_w,
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
                    // 内部锚点（#fn-def-<id> 或 #heading-slug）→ 目标 destination；
                    // 否则视为外部 URL 动作。
                    let target = if let Some(internal) = url.strip_prefix('#') {
                        if let Some((page_idx, tx, ty)) = footnote_targets.get(internal) {
                            Target::Destination(Destination::Xyz(XyzDestination::new(
                                *page_idx,
                                KrillaPoint::from_xy(*tx as f32, *ty as f32),
                            )))
                        } else if let Some((page_idx, tx, ty)) = footnote_ref_targets.get(internal)
                        {
                            // 命中脚注返回引用（#fn-ref-<label>）：页内跳回正文引用处。
                            Target::Destination(Destination::Xyz(XyzDestination::new(
                                *page_idx,
                                KrillaPoint::from_xy(*tx as f32, *ty as f32),
                            )))
                        } else if let Some((page_idx, tx, ty)) = heading_targets.get(internal) {
                            // 命中标题锚点：与 outline 一致的页内跳转。
                            Target::Destination(Destination::Xyz(XyzDestination::new(
                                *page_idx,
                                KrillaPoint::from_xy(*tx as f32, *ty as f32),
                            )))
                        } else {
                            let action = Action::from(LinkAction::new(url.clone()));
                            Target::Action(action)
                        }
                    } else {
                        let action = Action::from(LinkAction::new(url.clone()));
                        Target::Action(action)
                    };
                    let link_annotation = LinkAnnotation::new(rect, target);
                    krilla_page.add_annotation(Annotation::new_link(link_annotation, None));
                }
            }

            krilla_page.finish();
        }

        // 按层级把标题条目组织为嵌套 outline，并挂到文档。
        if !outline_entries.is_empty() {
            let outline = build_krilla_outline(&outline_entries);
            krilla_doc.set_outline(outline);
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

/// PDF 绘制器：在单个 krilla Surface 上绘制 `Block`。
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

    fn draw_rect(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        fill: Option<Color>,
        stroke: Option<(Color, f64)>,
    ) {
        let mut pb = PathBuilder::new();
        pb.move_to(x as f32, y as f32);
        pb.line_to((x + w) as f32, y as f32);
        pb.line_to((x + w) as f32, (y + h) as f32);
        pb.line_to(x as f32, (y + h) as f32);
        pb.close();
        if let Some(path) = pb.finish() {
            if let Some(c) = fill {
                let fill = Fill {
                    paint: Paint::from(to_krilla_color(&c)),
                    opacity: NormalizedF32::new(c.a as f32 / 255.0).unwrap_or(NormalizedF32::ONE),
                    rule: FillRule::NonZero,
                };
                self.surface.set_fill(Some(fill));
            } else {
                self.surface.set_fill(None);
            }
            if let Some((c, wt)) = stroke {
                let stroke = KrillaStroke {
                    paint: Paint::from(to_krilla_color(&c)),
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

    fn draw_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: Color, width: f64) {
        let mut pb = PathBuilder::new();
        pb.move_to(x1 as f32, y1 as f32);
        pb.line_to(x2 as f32, y2 as f32);
        if let Some(path) = pb.finish() {
            self.surface.set_fill(None);
            let stroke = KrillaStroke {
                paint: Paint::from(to_krilla_color(&color)),
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

    /// 填充实心圆（用 4 段三次贝塞尔逼近真圆，任何字体下样式一致、缩放不模糊）。
    fn draw_circle_fill(&mut self, cx: f64, cy: f64, r: f64, color: Color) {
        if r <= 0.0 {
            return;
        }
        let r = r as f32;
        let k = 0.552_284_8 * r; // 三次贝塞尔逼近圆的 magic constant
        let (cx, cy) = (cx as f32, cy as f32);
        let mut pb = PathBuilder::new();
        pb.move_to(cx + r, cy);
        pb.cubic_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r);
        pb.cubic_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy);
        pb.cubic_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r);
        pb.cubic_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy);
        pb.close();
        if let Some(path) = pb.finish() {
            let fill = Fill {
                paint: Paint::from(to_krilla_color(&color)),
                opacity: NormalizedF32::new(color.a as f32 / 255.0).unwrap_or(NormalizedF32::ONE),
                rule: FillRule::NonZero,
            };
            self.surface.set_fill(Some(fill));
            self.surface.set_stroke(None);
            self.surface.draw_path(&path);
            self.surface.set_fill(None);
        }
    }

    /// 在列表项缩进槽**左缘**（`x`）单独绘制列表 marker（矢量绘图）。
    ///
    /// - 任务列表：矢量画复选框外框，勾选时加对勾路径。
    /// - 无序列表（marker="●"）：矢量画实心圆点。
    /// - 有序列表（marker="N."）：以正文同字号/基线绘制数字文本。
    ///
    /// 正文（children）由调用方以 `x + list_item_indent` 排起，故 marker 落在
    /// 空白缩进槽内，与正文天然错位、互不重叠。
    #[allow(clippy::too_many_arguments)]
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
            self.draw_rect(x0, y0, size, size, None, Some((color, 1.2)));
            if checked {
                let s = size;
                let mut pb = PathBuilder::new();
                pb.move_to((x0 + s * 0.2) as f32, (y0 + s * 0.52) as f32);
                pb.line_to((x0 + s * 0.42) as f32, (y0 + s * 0.72) as f32);
                pb.line_to((x0 + s * 0.8) as f32, (y0 + s * 0.28) as f32);
                if let Some(path) = pb.finish() {
                    let stroke = KrillaStroke {
                        paint: Paint::from(to_krilla_color(&color)),
                        opacity: NormalizedF32::new(color.a as f32 / 255.0)
                            .unwrap_or(NormalizedF32::ONE),
                        width: 1.6,
                        miter_limit: 4.0,
                        line_cap: LineCap::default(),
                        line_join: LineJoin::default(),
                        dash: None,
                    };
                    self.surface.set_fill(None);
                    self.surface.set_stroke(Some(stroke));
                    self.surface.draw_path(&path);
                    self.surface.set_stroke(None);
                }
            }
            return;
        }

        if marker == "●" {
            // 实心圆点：半径 = 0.18em，垂直居中于首行，略偏左缘。
            let r = fs * 0.18;
            let cx = x + r + fs * 0.1;
            let cy = y + lh * 0.5;
            self.draw_circle_fill(cx, cy, r, color);
            return;
        }

        // 有序数字 marker：与正文同字号/基线绘制（x = 行顶）。
        let ts = text_style_from_resolved(style);
        let segments = [(marker, &ts)];
        let layout =
            crate::document::text::layout_text(&segments, None, crate::ast::TextAlign::Left);
        if let Some(tl) = layout.lines.last() {
            for run in &tl.runs {
                self.draw_text_run(run, Point::new(x, y));
            }
        }
    }

    /// 绘制单行纯文本（用于页眉/页脚等），居中于 [x, x+width]。
    fn draw_text(
        &mut self,
        text: &str,
        x: f64,
        y: f64,
        font_size: f64,
        align: LayoutAlign,
        width: f64,
    ) {
        if text.is_empty() {
            return;
        }
        let style = text_style(
            Color::rgb(120, 120, 120),
            "serif",
            font_size as f32,
            "normal",
            "normal",
        );
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

    /// 按 `object_fit` 计算图片在 box `(w, h)` 内的实际绘制区域（x, y, w, h）。
    ///
    /// - `Fill`/`None`：拉伸铺满整个 box（默认行为）。
    /// - `Contain`：保持宽高比缩放，完整放入 box 内并居中。
    /// - `Cover`：保持宽高比缩放，铺满 box（超出部分被裁剪）。
    fn image_draw_rect(
        &self,
        img: &crate::document::types::DocImage,
        (w, h): (f64, f64),
    ) -> (f64, f64, f64, f64) {
        let (pw, ph) = (img.pixel_size.0 as f64, img.pixel_size.1 as f64);
        if pw <= 0.0
            || ph <= 0.0
            || img.object_fit == crate::ast::ObjectFit::Fill
            || img.object_fit == crate::ast::ObjectFit::None
        {
            return (0.0, 0.0, w, h);
        }
        let box_aspect = w / h;
        let img_aspect = pw / ph;
        let (dw, dh) = match img.object_fit {
            crate::ast::ObjectFit::Contain => {
                if img_aspect > box_aspect {
                    (w, w / img_aspect)
                } else {
                    (h * img_aspect, h)
                }
            }
            crate::ast::ObjectFit::Cover => {
                if img_aspect > box_aspect {
                    (h * img_aspect, h)
                } else {
                    (w, w / img_aspect)
                }
            }
            _ => (w, h),
        };
        // 居中
        let dx = (w - dw) / 2.0;
        let dy = (h - dh) / 2.0;
        (dx, dy, dw, dh)
    }

    fn draw_image(&mut self, data: &[u8], format: &str, (x, y, w, h): (f64, f64, f64, f64)) {
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
            .push_transform(&KrillaTransform::from_translate(x as f32, y as f32));

        let Some(image_size) = Size::from_wh(w as f32, h as f32) else {
            self.surface.pop();
            return;
        };
        self.surface.draw_image(image, image_size);
        self.surface.pop();
    }

    /// 绘制文本 run（接受 [`TextRun`]，自闭环类型，可直接消费）。
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
                    paint: Paint::from(rgb_u8(bg)),
                    opacity: NormalizedF32::new(bg.a as f32).unwrap_or(NormalizedF32::ONE),
                    rule: FillRule::NonZero,
                };
                self.surface.set_fill(Some(fill));
                self.surface.draw_path(&path);
            }
        }

        // `run.font_data` 是 lievisual 共享的 `Arc<Vec<u8>>`，以 Arc 克隆交给 krilla
        // （`Data: From<Arc<Vec<u8>>>`），避免 `.to_vec()` 深拷贝整份字体。
        // `run.font_index` 用于在 ttc/otc 集合中定位具体字体实例，须同时参与缓存 key。
        let font_key = FontCacheKey::from_font_data(run.font_data.as_slice(), run.font_index);
        if let std::collections::hash_map::Entry::Vacant(e) = self.font_cache.entry(font_key) {
            if let Some(font) = Font::new(krilla::Data::from(run.font_data.clone()), run.font_index)
            {
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
            self.links
                .push((link_x, link_y, link_w, link_h, url.clone()));
        }

        let mut krilla_glyphs = Vec::new();
        let run_len = run.text.len();
        // glyph 的 cluster 在 text 层提取时已被归一化为相对本 run 切片后 `run.text` 的
        // **局部**偏移（自闭环），因此这里可直接用作 `run.text` 的索引，无需任何
        // 外部信息（不再依赖 text_range / 所属 Line / 原始 full_text）。
        for (i, g) in run.glyphs.iter().enumerate() {
            if g.id == 0 {
                continue;
            }
            // 相邻字形的 cluster 单调递增，范围取 [glyph.cluster, next_glyph.cluster)，
            // 末字形到 run.text 末尾。
            let start = g.cluster as usize;
            let end = if i + 1 < run.glyphs.len() {
                run.glyphs[i + 1].cluster as usize
            } else {
                run_len
            };
            let range = start.min(run_len)..end.min(run_len);
            if range.is_empty() {
                continue;
            }
            krilla_glyphs.push(krilla::text::KrillaGlyph::new(
                GlyphId::new(g.id),
                0.0,
                (g.x - run.baseline_x) / run.font_size,
                (g.y - run.baseline_y) / run.font_size,
                0.0,
                range,
                None,
            ));
        }

        if krilla_glyphs.is_empty() {
            return;
        }

        self.surface.set_stroke(None);

        let krilla_color = rgb_u8(run.color);
        let paint = Paint::from(krilla_color);
        let opacity = NormalizedF32::new(run.color.a as f32).unwrap_or(NormalizedF32::ONE);
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

    // ─── Block 绘制 ─────────────────────────────────────────

    /// 绘制已排版的文档文本行（[`TextLine`]）。
    ///
    /// `TextLine`/`TextRun` 已自闭环（字体以 `Vec<u8>` 内嵌、glyph.cluster 为相对
    /// `text` 的局部偏移），可直接消费，无需任何重建或还原。
    fn draw_doc_lines(&mut self, lines: &[TextLine], x: f64, y: f64) {
        for line in lines {
            let line_x = x + line.bounds.x0;
            let line_y = y + line.bounds.y0;
            for run in &line.runs {
                self.draw_text_run(run, Point::new(line_x, line_y));
            }
        }
    }

    /// 绘制单个 `Block`（递归）。`x`/`y` 为内容区绝对坐标（pt）。
    fn draw_block(&mut self, block: &Block, x: f64, y: f64, repeat_table_header: bool) {
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
            BlockKind::CodeBlock { lines, .. } => {
                // 语法高亮（暗色主题）已预排版进 `lines`，每段自带前景色；
                // 背景统一使用深色以匹配高亮配色（CSS 未声明时兜底深灰）。
                let bg = style.background_color.unwrap_or(Color::rgb(40, 44, 52));
                let lh = if style.line_height_pt > 0.0 {
                    style.line_height_pt as f64
                } else {
                    18.0
                };
                let n = lines.len().max(1);
                let h = n as f64 * lh + 8.0;
                self.draw_rect(x, y, self.content_w, h, Some(bg), None);
                self.draw_doc_lines(lines, x + 4.0, y + 4.0);
            }
            BlockKind::ThematicBreak => {
                // 横线颜色来自 CSS 的 hr 规则（border-top + border-color），
                // 投影到 ResolvedStyle.border_color；无声明时兜底为灰。
                let color = style.border_color.unwrap_or(Color::rgb(180, 180, 180));
                let w = if style.border_width_top > 0.0 {
                    style.border_width_top as f64
                } else {
                    0.8
                };
                self.draw_line(x, y + w / 2.0, x + self.content_w, y + w / 2.0, color, w);
            }
            BlockKind::Image(img) => {
                let (w, h) = if img.size.0 > 0.0 && img.size.1 > 0.0 {
                    (img.size.0, img.size.1)
                } else {
                    (self.content_w, 120.0)
                };
                if !img.data.is_empty() {
                    // 按 text_align 水平居中：图片块默认随内容区左边缘 `x` 定位；
                    // 居中时把图片整体右移 (content_w - w)/2（若图片比内容窄）。
                    let align_x = match style.text_align {
                        LayoutAlign::Center => x + ((self.content_w - w) / 2.0).max(0.0),
                        LayoutAlign::Right => x + (self.content_w - w).max(0.0),
                        _ => x,
                    };
                    // 按 object_fit 计算图片在 box 内的实际绘制区域（避免强制尺寸时拉伸变形）。
                    let (dx, dy, dw, dh) = self.image_draw_rect(img, (w, h));
                    self.draw_image(&img.data, &img.format, (align_x + dx, y + dy, dw, dh));
                } else if !img.alt.is_empty() {
                    let style =
                        text_style(Color::rgb(120, 120, 120), "serif", 11.0, "normal", "normal");
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
                let inner_x = x + BQ_BAR_WIDTH + BQ_PAD_X;
                // 先量文本 box（已扣子块 margin），引用块高度由文本 box 决定：文本高 + 上下对称内边距。
                let text_h = blockquote_content_height(children, &self.settings, inner_x);
                let content_h = text_h + 2.0 * BQ_PAD_Y;
                // 左侧竖条颜色来自 CSS 的 blockquote 规则（border-left + border-color），
                // 投影到 ResolvedStyle.border_color；无声明时兜底为灰。
                let bar_color = style.border_color.unwrap_or(Color::rgb(200, 200, 200));
                self.draw_rect(x, y, BQ_BAR_WIDTH, content_h, Some(bar_color), None);
                // 文本在引用块内真正垂直居中：上下均分剩余空间。
                let offset = ((content_h - text_h) / 2.0).max(0.0);
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
            BlockKind::ListItem { marker, children } => {
                // marker 在缩进槽左缘单独绘制（矢量圆点 / 有序数字），正文整体缩进。
                let indent = crate::output::common::list_item_indent(marker, style);
                self.draw_list_marker(marker, false, false, x, y, style);
                let mut cy = y;
                for child in children {
                    self.draw_block(child, x + indent, cy, false);
                    cy += block_height(child, &self.settings, x + indent);
                }
            }
            BlockKind::TaskListItem {
                marker,
                checked,
                children,
            } => {
                let indent = crate::output::common::list_item_indent(marker, style);
                self.draw_list_marker(marker, true, *checked, x, y, style);
                let mut cy = y;
                for child in children {
                    self.draw_block(child, x + indent, cy, false);
                    cy += block_height(child, &self.settings, x + indent);
                }
            }
            BlockKind::Container { children, .. } => {
                let mut cy = y;
                for child in children {
                    self.draw_block(child, x, cy, false);
                    cy += block_height(child, &self.settings, x);
                }
            }
            BlockKind::DefinitionList { items } => {
                // 术语（dt）正常绘制；定义（dd）整体缩进，与列表项视觉一致。
                const DD_INDENT: f64 = 18.0;
                let mut cy = y;
                for item in items {
                    for child in &item.term {
                        self.draw_block(child, x, cy, false);
                        cy += block_height(child, &self.settings, x);
                    }
                    for child in &item.definition {
                        self.draw_block(child, x + DD_INDENT, cy, false);
                        cy += block_height(child, &self.settings, x + DD_INDENT);
                    }
                }
            }
            BlockKind::FootnoteDef { children, .. } => {
                let mut cy = y;
                for child in children {
                    self.draw_block(child, x, cy, false);
                    cy += block_height(child, &self.settings, x);
                }
            }
            BlockKind::Table {
                rows,
                col_widths,
                row_heights,
                ..
            } => {
                self.draw_table(
                    rows,
                    col_widths,
                    row_heights,
                    style,
                    x,
                    y,
                    repeat_table_header,
                );
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
    ///
    /// 列宽与行高来自 `compute_table_layout` 的预计算值（真实度量）：
    /// - `col_widths`：每列真实宽度（pt）。
    /// - `row_heights`：每行真实高度（pt，按列宽折行后）。
    #[allow(clippy::too_many_arguments)]
    fn draw_table(
        &mut self,
        rows: &[TableRow],
        col_widths: &[f64],
        row_heights: &[f64],
        style: &ResolvedStyle,
        x: f64,
        y: f64,
        repeat_header: bool,
    ) {
        if rows.is_empty() {
            return;
        }
        let ncols = col_widths.len().max(1);
        let content_w: f64 = col_widths.iter().sum();

        // 行高：优先用预计算值；缺省退回行高。
        let row_height_at = |idx: usize| -> f64 {
            row_heights
                .get(idx)
                .copied()
                .unwrap_or((style.line_height_pt as f64).max(8.0))
                .max(8.0)
        };

        let header = rows.first();
        let body: &[TableRow] = if repeat_header {
            rows
        } else {
            &rows[1.min(rows.len())..]
        };

        let border = style.table_border_color;
        let border_w = style.table_border_width_pt as f64;
        let pad_h = style.table_cell_padding_h_pt;
        let pad_v = style.table_cell_padding_v_pt;
        let header_bg = style.table_header_bg.unwrap_or(Color::rgb(230, 230, 230));
        let alt_row_bg = style.table_alt_row_bg;
        // `row_idx` 为该行在整表中的索引（用于取行高）。
        let draw_row = |r: &mut PdfRenderer<'_, '_>,
                        row: &TableRow,
                        cy: f64,
                        row_h: f64,
                        is_header: bool,
                        alt: bool| {
            let bg = if is_header {
                header_bg
            } else if alt {
                alt_row_bg.unwrap_or(Color::rgb(255, 255, 255))
            } else {
                Color::rgb(255, 255, 255)
            };
            r.draw_rect(x, cy, content_w, row_h, Some(bg), Some((border, border_w)));
            // 列间分隔竖线：在每列交界处画一条垂直边框线。
            let mut sep = x;
            for (ci, cw) in col_widths.iter().enumerate() {
                sep += cw;
                if ci + 1 < ncols {
                    r.draw_line(sep, cy, sep, cy + row_h, border, border_w);
                }
            }
            let mut cx = x;
            for (ci, cell) in row.cells.iter().enumerate() {
                let cell_w = col_widths.get(ci).copied().unwrap_or(0.0);
                let cell_x = cx + pad_h as f64;
                let mut ccy = cy + pad_v as f64;
                for child in &cell.children {
                    r.draw_block(child, cell_x, ccy, false);
                    ccy += block_height(child, &r.settings, cell_x) + 2.0;
                }
                cx += cell_w;
            }
        };

        if let Some(h) = header {
            draw_row(self, h, y, row_height_at(0), true, false);
        }
        let mut cy = y;
        if header.is_some() {
            cy += row_height_at(0);
        }
        let body_start = if repeat_header { 0 } else { 1 };
        for (i, row) in body.iter().enumerate() {
            let row_idx = body_start + i;
            let row_h = row_height_at(row_idx);
            draw_row(self, row, cy, row_h, false, i % 2 == 1);
            cy += row_h;
        }
    }
}

// ─── 分页（PDF 后端职责）───────────────────────────────────

/// 目录条目：一个标题块的位置与层级。
struct OutlineEntry {
    /// 标题层级（1..=6）
    level: u8,
    /// 标题文本
    title: String,
    /// 所在页索引（0 起）
    page_index: usize,
    /// 页内 y 坐标（内容区左上角为原点，pt）
    y: f64,
}

/// 把标题文本转换为 GitHub 风格锚点 slug，用于内部链接跳转匹配。
///
/// 规则（与 GitHub/常见 Markdown TOC 生成器一致）：
/// - 字母数字与字母（含 CJK）保留；ASCII 字母转小写。
/// - 空白与标点（`.`, `,`, `!` 等）折叠为单个 `-`。
/// - 去除首尾 `-`。
///
/// 例：`"1. 安装与快速开始"` → `"1-安装与快速开始"`，与手动 TOC 的 `#1-安装与快速开始` 匹配。
fn github_slug(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c.is_alphabetic() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if (c.is_whitespace() || ".,!?;:'\"()[]{}|/\\<>#&*+=@$%^~`".contains(c))
            && !prev_dash
            && !out.is_empty()
        {
            out.push('-');
            prev_dash = true;
        }
        // 其它字符（如中文标点）跳过，不计入 slug。
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// 将扁平标题条目列表组织为层级 krilla `Outline`。
///
/// 条目已按文档顺序排列，层级递增即嵌套；`XyzDestination` 会自动将坐标从
/// 「顶部原点」转换为 PDF 的「底部原点」，故直接传 `y` 即可。
fn build_krilla_outline(entries: &[OutlineEntry]) -> Outline {
    fn push_children(
        node: &mut OutlineNode,
        entries: &[OutlineEntry],
        i: &mut usize,
        parent_level: u8,
    ) {
        while *i < entries.len() {
            let e = &entries[*i];
            if e.level <= parent_level {
                break;
            }
            let dest = XyzDestination::new(e.page_index, KrillaPoint::from_xy(0.0, e.y as f32));
            let mut child = OutlineNode::new(e.title.clone(), dest);
            *i += 1;
            push_children(&mut child, entries, i, e.level);
            node.push_child(child);
        }
    }

    let mut outline = Outline::new();
    let mut i = 0usize;
    while i < entries.len() {
        let e = &entries[i];
        let dest = XyzDestination::new(e.page_index, KrillaPoint::from_xy(0.0, e.y as f32));
        let mut node = OutlineNode::new(e.title.clone(), dest);
        i += 1;
        push_children(&mut node, entries, &mut i, e.level);
        outline.push_child(node);
    }
    outline
}

/// 把不分页的 [`Document`] 切分为多页绝对定位块（PDF 后端内部分页）。
fn paginate_layout(document: &Document, settings: &PageSettings) -> Vec<PdfPage> {
    let content_x = settings.content_x() as f64;
    let content_y = settings.content_y() as f64;
    let content_h = settings.content_height() as f64;

    let mut pages: Vec<PdfPage> = Vec::new();
    let mut cur = PdfPage::default();
    let mut used = 0.0_f64;

    let header = settings.header.clone();
    let footer = settings.footer.clone();

    for block in &document.blocks {
        let h = block_height(block, settings, content_x);
        if let BlockKind::Table {
            rows,
            column_align,
            col_widths,
            row_heights,
        } = &block.kind
        {
            paginate_table(
                rows,
                column_align,
                col_widths,
                row_heights,
                &block.style,
                &mut PaginateCtx {
                    pages: &mut pages,
                    cur: &mut cur,
                    used: &mut used,
                },
                content_x,
                content_y,
                content_h,
            );
            // 表格后有自身下边距（与下一个块形成垂直间距）。
            // 注意：表格与其上方块的间距由上一个块的 margin_bottom 提供
            // （本布局系统采用"相邻块间距 = 上一块 margin_bottom"约定），
            // 故此处不再额外加表格的 margin_top，否则会与上方间距叠加导致过大。
            used += block.style.margin_bottom as f64;
            continue;
        }
        if used + h > content_h && used > 0.0 {
            cur.header = header.clone();
            cur.footer = footer.clone();
            pages.push(std::mem::take(&mut cur));
            used = 0.0;
        }
        cur.blocks.push(PositionedBlock::new(
            block.clone(),
            content_x,
            content_y + used,
            h,
        ));
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
///
/// 行高取自 `compute_table_layout` 预计算的 `row_heights`（真实度量，非固定值）；
/// 续页片段透传原表格列宽与对应行的行高，避免续页表格配色/内边距退回默认值。
#[allow(clippy::too_many_arguments)]
fn paginate_table(
    rows: &[TableRow],
    column_align: &[TextAlign],
    col_widths: &[f64],
    row_heights: &[f64],
    style: &ResolvedStyle,
    ctx: &mut PaginateCtx,
    content_x: f64,
    content_y: f64,
    content_h: f64,
) {
    // 表头行高 = 第一行行高（若无预计算值则退回行高）。
    let header_h = row_heights.first().copied().unwrap_or(18.0);
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
            // 首行当作表头，续页首行作为表头重复（仍需 header_h 高度）。
            let add = if i + fit == 0 {
                header_h
            } else {
                row_heights
                    .get(i + fit)
                    .copied()
                    .unwrap_or(header_h)
                    .max(8.0)
            };
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
        let page_row_heights: Vec<f64> = row_heights[i..i + fit].to_vec();
        let is_continuation = i > 0;
        let page_h: f64 = page_row_heights.iter().sum();
        let mut pb = PositionedBlock::new(
            Block::new(
                BlockKind::Table {
                    rows: page_rows,
                    column_align: column_align.to_vec(),
                    col_widths: col_widths.to_vec(),
                    row_heights: page_row_heights,
                },
                style.clone(),
                false,
            ),
            content_x,
            content_y + *ctx.used,
            page_h,
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

fn to_krilla_color(color: &lievisual::Color) -> krilla::color::rgb::Color {
    krilla::color::rgb::Color::new(color.r(), color.g(), color.b())
}
