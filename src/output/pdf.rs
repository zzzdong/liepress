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

use crate::ast::PageBreak;
use crate::document::layout::{Block, BlockKind, DefinitionItemBlock, Document, TableRow};
use crate::document::text::{
    LineMetrics, TextAlign as LayoutAlign, TextDecoration, TextLine, TextRun, layout_text,
};
use crate::document::types::page::PageSettings;
use crate::document::types::{ResolvedStyle, TextAlign};
use crate::error::{Error, Result};
use lievisual::{Color, geometry::Point};

use super::common::{
    BQ_BAR_WIDTH, BQ_PAD_X, BQ_PAD_Y, block_height, blockquote_content_height,
    lines_visual_height, text_style, text_style_from_resolved,
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

/// lievisual `Color`（0–255 u8）→ krilla 的 `RgbColor`（0–255 u8）。
fn to_krilla_color(color: lievisual::Color) -> krilla::color::rgb::Color {
    krilla::color::rgb::Color::new(color.r, color.g, color.b)
}

/// 计算 EXIF Orientation 对应的绘制参数：(绘制宽, 高, 附加仿射矩阵)。
///
/// 显示盒为 `(w, h)`（方向 5–8 时已是交换后的显示方向尺寸）；图像以
/// **存储态**宽高比矩形 `(rw, rh)` 绘制，矩阵把该矩形映射到显示盒：
/// - 1：无变换；
/// - 2/4：水平/垂直镜像；3：旋转 180°；
/// - 5/7：转置系（存储宽高与显示盒互换）；6：旋转 90° 顺时针；8：逆时针。
///
/// 矩阵为 tiny-skia 行序 `[sx, ky, kx, sy, tx, ty]`。
fn exif_draw_params(orientation: u8, w: f64, h: f64) -> (f64, f64, Option<[f32; 6]>) {
    let (fw, fh) = (w as f32, h as f32);
    match orientation {
        2 => (w, h, Some([-1.0, 0.0, 0.0, 1.0, fw, 0.0])),
        3 => (w, h, Some([-1.0, 0.0, 0.0, -1.0, fw, fh])),
        4 => (w, h, Some([1.0, 0.0, 0.0, -1.0, 0.0, fh])),
        5 => (h, w, Some([0.0, 1.0, 1.0, 0.0, 0.0, 0.0])),
        6 => (h, w, Some([0.0, 1.0, -1.0, 0.0, fw, 0.0])),
        7 => (h, w, Some([0.0, -1.0, -1.0, 0.0, fw, fh])),
        8 => (h, w, Some([0.0, -1.0, 1.0, 0.0, 0.0, fh])),
        _ => (w, h, None),
    }
}

/// PDF 后端分页后的单页（仅 PDF 后端内部使用，不污染 document 层）。
#[derive(Clone, Debug, Default)]
struct PdfPage {
    blocks: Vec<PositionedBlock>,
    header: Option<String>,
    footer: Option<String>,
    /// 本页内容区实际占用高度（pt）。
    ///
    /// 无限高度模式（`height_unlimited`）下据此确定最终页面高度
    /// （页高 = 上边距 + used_h + 下边距），否则固定为配置页高。
    used_h: f64,
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
}

impl PositionedBlock {
    fn new(block: Block, x: f64, y: f64, height: f64) -> Self {
        Self { block, x, y, height }
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
            // 无限高度模式：页高 = 上边距 + 内容实际占用 + 下边距（按页自适应），
            // 而非固定 A4 高度——否则内容下方会留出大片空白。
            let height = if self.settings.height_unlimited {
                (self.settings.margin_top_pt + page.used_h as f32 + self.settings.margin_bottom_pt)
                    .max(1.0)
            } else {
                self.settings.height_pt
            };
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
                    renderer.draw_block(&pb.block, pb.x, pb.y);
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
                    paint: Paint::from(to_krilla_color(c)),
                    opacity: NormalizedF32::new(c.a as f32 / 255.0).unwrap_or(NormalizedF32::ONE),
                    rule: FillRule::NonZero,
                };
                self.surface.set_fill(Some(fill));
            } else {
                self.surface.set_fill(None);
            }
            if let Some((c, wt)) = stroke {
                let stroke = KrillaStroke {
                    paint: Paint::from(to_krilla_color(c)),
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
                paint: Paint::from(to_krilla_color(color)),
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
                paint: Paint::from(to_krilla_color(color)),
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
                        paint: Paint::from(to_krilla_color(color)),
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
                self.draw_text_run(run, Point::new(x, y), &tl.metrics);
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
            self.draw_text_run(run, Point::new(tx, y), &line.metrics);
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

    fn draw_image(
        &mut self,
        data: &[u8],
        format: &str,
        (x, y, w, h): (f64, f64, f64, f64),
        orientation: u8,
    ) {
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

        // EXIF 方向校正：krilla 不读 EXIF，按 orientation 施加旋转/翻转矩阵。
        // 绘制矩形（rw, rh）：方向 5–8 时图像存储态宽高与显示盒互换。
        let (rw, rh, row) = exif_draw_params(orientation, w, h);
        if let Some([sx, ky, kx, sy, tx, ty]) = row {
            self.surface
                .push_transform(&KrillaTransform::from_row(sx, ky, kx, sy, tx, ty));
        }

        let Some(image_size) = Size::from_wh(rw as f32, rh as f32) else {
            self.surface.pop();
            if row.is_some() {
                self.surface.pop();
            }
            return;
        };
        self.surface.draw_image(image, image_size);
        if row.is_some() {
            self.surface.pop();
        }
        self.surface.pop();
    }

    /// 绘制文本 run（接受 [`TextRun`]，自闭环类型，可直接消费）。
    ///
    /// `metrics` 为所属行的 [`LineMetrics`]（行内背景矩形的垂直定位需要
    /// ascent / descent，使背景对齐文本 em 盒而非行顶）。
    fn draw_text_run(&mut self, run: &TextRun, position: Point, metrics: &LineMetrics) {
        use krilla::text::GlyphId;

        if run.glyphs.is_empty() {
            return;
        }

        if let Some(bg) = run.background_color {
            let pad = run.font_size * 0.1;
            let rx = position.x as f32 + run.baseline_x - pad;
            // 背景矩形垂直对齐文本 em 盒：em 盒顶 = 基线 - ascent。
            // 行高 > 字号时行顶到 em 盒顶有半个 leading，若从行顶起画
            // 背景会整体偏上、底部盖不住字形。
            let ry = position.y as f32 + run.baseline_y - metrics.ascent - pad * 0.5;
            let rw = run.advance + pad * 2.0;
            let rh = metrics.ascent + metrics.descent + pad;
            let mut pb = PathBuilder::new();
            pb.move_to(rx, ry);
            pb.line_to(rx + rw, ry);
            pb.line_to(rx + rw, ry + rh);
            pb.line_to(rx, ry + rh);
            pb.close();
            if let Some(path) = pb.finish() {
                let fill = Fill {
                    paint: Paint::from(to_krilla_color(bg)),
                    // krilla 要求 0..=1：alpha 为 0-255，须除以 255
                    //（漏除会使半透明背景变不透明、alpha=0 之外的区域溢出为 1）。
                    opacity: NormalizedF32::new(bg.a as f32 / 255.0).unwrap_or(NormalizedF32::ONE),
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

        let krilla_color = to_krilla_color(run.color);
        let paint = Paint::from(krilla_color);
        // krilla 要求 0..=1：alpha 为 0-255，须除以 255（同 draw_rect/draw_line）。
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
                self.draw_text_run(run, Point::new(line_x, line_y), &line.metrics);
            }
        }
    }

    /// 绘制单个 `Block`（递归）。`x`/`y` 为内容区绝对坐标（pt）。
    fn draw_block(&mut self, block: &Block, x: f64, y: f64) {
        let style = &block.style;
        match &block.kind {
            BlockKind::Heading { level: _, children } => {
                // 标题字号/颜色已由 from_ast 按节点 CSS 样式排版进 `lines`
                //（与 PNG/SVG 端一致）。不再用 `heading_font_size` 硬编码覆盖——
                // 旧覆盖使 PDF 与 CSS/PNG/SVG 字号不一致，且用户 CSS 修改标题
                // 字号在 PDF 中完全失效（行框按 CSS 排版、字形被改成 22pt）。
                for child in children {
                    if let BlockKind::Paragraph { lines } = &child.kind {
                        self.draw_doc_lines(lines, x, y);
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
                // 背景高度取末行 bounds.y1（含空行占位与每行真实行高），
                // 与 block_height 一致；`行数 × 行高` 会少计空行导致末行溢出背景。
                let h = lines_visual_height(lines, lh) + 8.0;
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
                    self.draw_image(
                        &img.data,
                        &img.format,
                        (align_x + dx, y + dy, dw, dh),
                        img.orientation,
                    );
                } else if !img.alt.is_empty() {
                    let style =
                        text_style(Color::rgb(120, 120, 120), "serif", 11.0, "normal", "normal");
                    let segments = [(img.alt.as_str(), &style)];
                    let layout = layout_text(&segments, None, LayoutAlign::Left);
                    if let Some(tl) = layout.lines.last() {
                        for run in &tl.runs {
                            self.draw_text_run(run, Point::new(x, y), &tl.metrics);
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
                    self.draw_block(child, inner_x, cy);
                    cy += block_height(child, &self.settings, inner_x);
                }
            }
            BlockKind::List { children, .. } => {
                let mut cy = y;
                for child in children {
                    self.draw_block(child, x, cy);
                    cy += block_height(child, &self.settings, x);
                }
            }
            BlockKind::ListItem { marker, children } => {
                // marker 在缩进槽左缘单独绘制（矢量圆点 / 有序数字），正文整体缩进。
                let indent = crate::output::common::list_item_indent(marker, style);
                self.draw_list_marker(marker, false, false, x, y, style);
                let mut cy = y;
                for child in children {
                    self.draw_block(child, x + indent, cy);
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
                    self.draw_block(child, x + indent, cy);
                    cy += block_height(child, &self.settings, x + indent);
                }
            }
            BlockKind::Container { children, .. } => {
                let mut cy = y;
                for child in children {
                    self.draw_block(child, x, cy);
                    cy += block_height(child, &self.settings, x);
                }
            }
            BlockKind::DefinitionList { items } => {
                // 术语（dt）正常绘制；定义（dd）整体缩进，与列表项视觉一致。
                const DD_INDENT: f64 = 18.0;
                let mut cy = y;
                for item in items {
                    for child in &item.term {
                        self.draw_block(child, x, cy);
                        cy += block_height(child, &self.settings, x);
                    }
                    for child in &item.definition {
                        self.draw_block(child, x + DD_INDENT, cy);
                        cy += block_height(child, &self.settings, x + DD_INDENT);
                    }
                }
            }
            BlockKind::FootnoteDef { children, .. } => {
                let mut cy = y;
                for child in children {
                    self.draw_block(child, x, cy);
                    cy += block_height(child, &self.settings, x);
                }
            }
            BlockKind::Table {
                rows,
                col_widths,
                row_heights,
                ..
            } => {
                self.draw_table(rows, col_widths, row_heights, style, x, y);
            }
            BlockKind::Document { children, .. } => {
                let mut cy = y;
                for child in children {
                    self.draw_block(child, x, cy);
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

    /// 绘制表格。
    ///
    /// 列宽与行高来自 `compute_table_layout` 的预计算值（真实度量）：
    /// - `col_widths`：每列真实宽度（pt）。
    /// - `row_heights`：每行真实高度（pt，按列宽折行后）。
    ///
    /// 约定：`rows[0]` **恒为表头行**（整表首片段即真实表头；跨页续页片段由
    /// [`paginate_table`] 在头部补回原表头行），body 从 `rows[1..]` 开始，
    /// 每行恰好绘制一次，避免首行既当表头画一遍、又在 body 循环再画一遍。
    #[allow(clippy::too_many_arguments)]
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
        let body: &[TableRow] = &rows[1.min(rows.len())..];

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
                    r.draw_block(child, cell_x, ccy);
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
        let body_start = 1usize;
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

    {
        let mut ctx = PaginateCtx {
            pages: &mut pages,
            cur: &mut cur,
            used: &mut used,
            header: &header,
            footer: &footer,
        };

        for block in &document.blocks {
            let h = block_height(block, settings, content_x);

            // P2-5：`page-break-before: always` —— 当前页已有内容时在该块前强制换页。
            let force_break_before = block.style.page_break_before == PageBreak::Always
                && (*ctx.used > 0.0 || !ctx.cur.blocks.is_empty());

            if let BlockKind::Table {
                rows,
                column_align,
                col_widths,
                row_heights,
            } = &block.kind
            {
                if force_break_before {
                    ctx.push_page();
                }
                paginate_table(
                    rows,
                    column_align,
                    col_widths,
                    row_heights,
                    &block.style,
                    &mut ctx,
                    content_x,
                    content_y,
                    content_h,
                );
                // 表格后有自身下边距（与下一个块形成垂直间距）。
                // 注意：表格与其上方块的间距由上一个块的 margin_bottom 提供
                // （本布局系统采用"相邻块间距 = 上一块 margin_bottom"约定），
                // 故此处不再额外加表格的 margin_top，否则会与上方间距叠加导致过大。
                *ctx.used += block.style.margin_bottom as f64;
                if block.style.page_break_after == PageBreak::Always {
                    ctx.push_page();
                }
                continue;
            }

            // 段落 / 代码块：可分割且整块高于一页时按行分页（其余走下方普通块逻辑）。
            // 代码块与段落同构（语法高亮后的 `lines: Vec<TextLine>`），本应像普通文本一样
            // 自然跨页；仅当确实放不下一页时才按行切分，避免对短块引入克隆开销。
            if block.splittable && h > content_h {
                let line_block = match &block.kind {
                    BlockKind::Paragraph { lines } if lines.len() > 1 => {
                        Some((lines.as_slice(), 0.0))
                    }
                    BlockKind::CodeBlock { lines, .. } if lines.len() > 1 => {
                        Some((lines.as_slice(), 4.0))
                    }
                    _ => None,
                };
                if let Some((lines, pad_v)) = line_block {
                    if force_break_before {
                        ctx.push_page();
                    }
                    paginate_lines_block(
                        block, lines, pad_v, &mut ctx, content_x, content_y, content_h,
                    );
                    if block.style.page_break_after == PageBreak::Always {
                        ctx.push_page();
                    }
                    continue;
                }
            }

            // 容器块（List/ListItem/Blockquote/DefinitionList/Container）高过一页时
            // 按子块边界切分：整块塞进一页会让超出页底的内容被静默裁剪（内容丢失）。
            // 高度记用处见 `fragment_container`：片段高度与实际绘制严格一致。
            if h > content_h && is_fragmentable_container(&block.kind) {
                if force_break_before {
                    ctx.push_page();
                }
                for frag in fragment_container(block, settings, content_x, content_h) {
                    let fh = block_height(&frag, settings, content_x);
                    let empty_page = ctx.cur.blocks.is_empty() && *ctx.used <= 0.0;
                    if !empty_page && fh > content_h - *ctx.used {
                        ctx.push_page();
                    }
                    let y = content_y + *ctx.used;
                    ctx.cur
                        .blocks
                        .push(PositionedBlock::new(frag, content_x, y, fh));
                    *ctx.used += fh;
                }
                if block.style.page_break_after == PageBreak::Always {
                    ctx.push_page();
                }
                continue;
            }

            if (*ctx.used + h > content_h && *ctx.used > 0.0) || force_break_before {
                ctx.push_page();
            }
            let y = content_y + *ctx.used;
            ctx.cur
                .blocks
                .push(PositionedBlock::new(block.clone(), content_x, y, h));
            *ctx.used += h;
            if block.style.page_break_after == PageBreak::Always {
                ctx.push_page();
            }
        }
    }

    // 末页：仅在仍有内容（或整个文档为空）时收尾，避免
    // 「最后一个块 page-break-after 触发换页」后附加一张空白页。
    if !cur.blocks.is_empty() || pages.is_empty() {
        cur.header = header;
        cur.footer = footer;
        cur.used_h = used;
        pages.push(cur);
    }
    pages
}

/// 分页累积状态（跨块/跨页共享）。
struct PaginateCtx<'a> {
    pages: &'a mut Vec<PdfPage>,
    cur: &'a mut PdfPage,
    used: &'a mut f64,
    header: &'a Option<String>,
    footer: &'a Option<String>,
}

impl PaginateCtx<'_> {
    /// 结束当前页并开新页，同时写入页眉页脚（否则跨页的中间页会缺页眉页脚）。
    fn push_page(&mut self) {
        let header = self.header.clone();
        let footer = self.footer.clone();
        self.cur.header = header;
        self.cur.footer = footer;
        self.cur.used_h = *self.used;
        self.pages.push(std::mem::take(self.cur));
        *self.used = 0.0;
    }
}

/// 表格分页：按整行切分，续页片段在头部补回真实表头行。
///
/// 行高取自 `compute_table_layout` 预计算的 `row_heights`（真实度量，非固定值）；
/// 续页片段透传原表格列宽与对应行的行高，避免续页表格配色/内边距退回默认值。
///
/// 约定：片段的 `rows[0]` 恒为表头行——首片段即真实表头，续页片段把原表头
/// 克隆到片段头部，与 [`PdfRenderer::draw_table`] 的绘制约定一致。
/// 高度记账（`used` 增量）恒等于片段实际绘制高度（表头 + body 行高之和），
/// 避免表头高被双计（表格从空页开始时）或漏计导致续页表格与后续块重叠/留缝。
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
    // 空表格（无任何行）不生成块，直接返回，避免后续按行切片越界。
    if rows.is_empty() {
        return;
    }
    // 表头行高 = 第一行行高（若无预计算值则退回固定值）。
    let header_h = row_heights.first().copied().unwrap_or(18.0).max(8.0);
    let n = rows.len();
    // `i`：下一个待排 body 行的绝对索引（首片段为 1：rows[0] 是表头）。
    let mut i = 1usize;

    loop {
        let avail = (content_h - *ctx.used).max(0.0);
        let empty_page = ctx.cur.blocks.is_empty() && *ctx.used <= 0.0;

        // 本页可容纳的 body 行数：表头始终占一个 header_h，body 行从 i 起贪心装填。
        let mut fit = 0usize;
        let mut need = header_h;
        while i + fit < n {
            let add = row_heights
                .get(i + fit)
                .copied()
                .unwrap_or(header_h)
                .max(8.0);
            if need + add <= avail {
                need += add;
                fit += 1;
            } else {
                break;
            }
        }

        if fit == 0 && !empty_page {
            // 当前页连「表头 + 下一行」都放不下：换页后重试。
            ctx.push_page();
            continue;
        }
        if fit == 0 && i < n {
            // 空页仍放不下「表头 + 一行」（行本身高过整页）：强制放一行保证前进。
            fit = 1;
        }

        // 片段行集合：续页片段（i > 1）在头部补上真实表头行 rows[0]。
        let end = (i + fit).min(n);
        let page_rows: Vec<TableRow> = if i <= 1 {
            rows[..end].to_vec()
        } else {
            let mut v = Vec::with_capacity(1 + (end - i));
            v.push(rows[0].clone());
            v.extend(rows[i..end].iter().cloned());
            v
        };
        // 防御：row_heights 若与 rows 不等长（异常输入），缺省退回 header_h，避免越界。
        let page_row_heights: Vec<f64> = if i <= 1 {
            (0..end)
                .map(|ri| row_heights.get(ri).copied().unwrap_or(header_h).max(8.0))
                .collect()
        } else {
            let mut v = Vec::with_capacity(1 + (end - i));
            v.push(header_h);
            v.extend(
                (i..end)
                    .map(|ri| row_heights.get(ri).copied().unwrap_or(header_h).max(8.0)),
            );
            v
        };
        // 片段实际绘制高度（表头 + body 行），used 增量与之严格一致。
        let page_h: f64 = page_row_heights.iter().sum();
        let pb = PositionedBlock::new(
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
        ctx.cur.blocks.push(pb);
        *ctx.used += page_h;

        i = end;
        if i >= n {
            break;
        }
        ctx.push_page();
    }
}

/// 判断块是否为可按子块边界切分的容器块。
fn is_fragmentable_container(kind: &BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::List { .. }
            | BlockKind::ListItem { .. }
            | BlockKind::TaskListItem { .. }
            | BlockKind::Blockquote { .. }
            | BlockKind::DefinitionList { .. }
            | BlockKind::Container { .. }
    )
}

/// 把一个高过一页的容器块按子块边界切分为若干片段。
///
/// 每个片段复用原块的结构与样式（List/Blockquote/ListItem 等），使绘制端的
/// 缩进/竖条/marker 逻辑无需感知分页；容器自身的 `margin_top`/`margin_bottom`
/// 只保留在首/末片段上，中间片段清零，避免片段之间出现多余间距。
///
/// 子块若自身高过 `max_h` 且仍是容器块则递归切分；不可再分的子块（如超大图片、
/// 超高标题）独占片段并溢出页面（与旧行为一致，内容不丢，仅可能超出页底）。
fn fragment_container(
    block: &Block,
    settings: &PageSettings,
    x: f64,
    max_h: f64,
) -> Vec<Block> {
    let style = &block.style;
    // 分组预算扣除容器自身上下外边距，保证首/末片段（含 margin）不超过 max_h。
    let inner_max = (max_h - style.margin_top as f64 - style.margin_bottom as f64).max(1.0);

    let mut frags: Vec<Block> = match &block.kind {
        BlockKind::List {
            ordered, start, ..
        } => group_children(
            block.kind.children(),
            settings,
            x,
            inner_max,
            |kids| {
                Block::new(
                    BlockKind::List {
                        ordered: *ordered,
                        start: *start,
                        children: kids,
                    },
                    style.clone(),
                    block.splittable,
                )
            },
        ),
        BlockKind::ListItem { marker, .. } => group_children(
            block.kind.children(),
            settings,
            x,
            inner_max,
            |kids| {
                Block::new(
                    BlockKind::ListItem {
                        marker: marker.clone(),
                        children: kids,
                    },
                    style.clone(),
                    block.splittable,
                )
            },
        ),
        BlockKind::TaskListItem {
            marker, checked, ..
        } => group_children(
            block.kind.children(),
            settings,
            x,
            inner_max,
            |kids| {
                Block::new(
                    BlockKind::TaskListItem {
                        marker: marker.clone(),
                        checked: *checked,
                        children: kids,
                    },
                    style.clone(),
                    block.splittable,
                )
            },
        ),
        BlockKind::Blockquote { .. } => group_children(
            block.kind.children(),
            settings,
            x,
            // Blockquote 高度含上下 BQ_PAD_Y，一并预留。
            (inner_max - 2.0 * BQ_PAD_Y).max(1.0),
            |kids| {
                Block::new(
                    BlockKind::Blockquote { children: kids },
                    style.clone(),
                    block.splittable,
                )
            },
        ),
        BlockKind::Container { .. } => group_children(
            block.kind.children(),
            settings,
            x,
            inner_max,
            |kids| {
                Block::new(
                    BlockKind::Container { children: kids },
                    style.clone(),
                    block.splittable,
                )
            },
        ),
        BlockKind::DefinitionList { items } => {
            // 定义列表以 (term, definition) 项为单位切分。
            let mut groups: Vec<Vec<DefinitionItemBlock>> = Vec::new();
            let mut cur: Vec<DefinitionItemBlock> = Vec::new();
            let mut cur_h = 0.0_f64;
            for item in items {
                let ih: f64 = item
                    .term
                    .iter()
                    .chain(item.definition.iter())
                    .map(|c| block_height(c, settings, x))
                    .sum();
                if !cur.is_empty() && cur_h + ih > inner_max {
                    groups.push(std::mem::take(&mut cur));
                    cur_h = 0.0;
                }
                cur_h += ih;
                cur.push(item.clone());
            }
            if !cur.is_empty() {
                groups.push(cur);
            }
            groups
                .into_iter()
                .map(|items| {
                    Block::new(
                        BlockKind::DefinitionList { items },
                        style.clone(),
                        block.splittable,
                    )
                })
                .collect()
        }
        _ => return vec![block.clone()],
    };

    // 首片段保留 margin_top、末片段保留 margin_bottom，中间片段清零。
    let last = frags.len().saturating_sub(1);
    for (fi, f) in frags.iter_mut().enumerate() {
        if fi != 0 {
            f.style.margin_top = 0.0;
        }
        if fi != last {
            f.style.margin_bottom = 0.0;
        }
    }
    frags
}

/// 把子块序列按高度贪心分组为若干 ≤ `max_h` 的片段。
///
/// `make` 负责把一组子块装配为原容器类型的片段块。子块自身高过 `max_h` 且仍是
/// 容器块时递归切分，其片段作为独立分组单元参与装填。
fn group_children<F>(
    children: &[Block],
    settings: &PageSettings,
    x: f64,
    max_h: f64,
    make: F,
) -> Vec<Block>
where
    F: Fn(Vec<Block>) -> Block,
{
    let mut frags: Vec<Block> = Vec::new();
    let mut cur: Vec<Block> = Vec::new();
    let mut cur_h = 0.0_f64;

    for child in children {
        let units: Vec<Block> = {
            let ch = block_height(child, settings, x);
            if ch > max_h && is_fragmentable_container(&child.kind) {
                fragment_container(child, settings, x, max_h)
            } else {
                vec![child.clone()]
            }
        };
        for unit in units {
            let uh = block_height(&unit, settings, x);
            if !cur.is_empty() && cur_h + uh > max_h {
                frags.push(make(std::mem::take(&mut cur)));
                cur_h = 0.0;
            }
            cur_h += uh;
            cur.push(unit);
        }
    }
    if !cur.is_empty() {
        frags.push(make(cur));
    }
    frags
}

/// 把一段文本行的坐标重新基准化到新原点（首行 y 归零），用于分页片段。
///
/// `TextLine.bounds` / `ink_bounds` 是相对整段文本 layout origin 的坐标；按行切分到
/// 不同页面后，每个片段需从自身原点开始绘制，故把 `bounds` / `ink_bounds` 的 y 分量
/// 减去片段首行的偏移 `base_y`。
fn rebase_lines(lines: &[TextLine], base_y: f64) -> Vec<TextLine> {
    lines
        .iter()
        .map(|line| {
            let mut nl = line.clone();
            nl.bounds.y0 -= base_y;
            nl.bounds.y1 -= base_y;
            nl.ink_bounds.y0 -= base_y;
            nl.ink_bounds.y1 -= base_y;
            nl
        })
        .collect()
}

/// 对带 `lines` 的可分割块（段落 / 代码块）按行分页。
///
/// 与 [`paginate_table`] 同理，但以「文本行」为切分单位：每页能容纳的行数按行的
/// 真实 `bounds` 跨度计算（空行不产出 `TextLine`，但其占位已体现在相邻行的
/// `bounds` 间距中，不能按 `行数 × line_height` 估高），首片段预留 `margin_top`、
/// 末片段追加 `margin_bottom`，中间片段零外边距。每个页面片段通过 [`rebase_lines`]
/// 重新基准化，保证跨页后各片段仍从自身原点正确绘制。
#[allow(clippy::too_many_arguments)]
fn paginate_lines_block(
    block: &Block,
    lines: &[TextLine],
    pad_v: f64,
    ctx: &mut PaginateCtx,
    content_x: f64,
    content_y: f64,
    content_h: f64,
) {
    let style = &block.style;
    let margin_top = style.margin_top as f64;
    let margin_bottom = style.margin_bottom as f64;
    // 上下内边距之和（代码块背景 padding；段落为 0）。
    let overhead = 2.0 * pad_v;
    let n = lines.len();
    let mut i = 0usize;

    while i < n {
        // 首片段额外预留上外边距。
        let leading = if i == 0 { margin_top } else { 0.0 };
        let avail = (content_h - *ctx.used).max(0.0);
        let usable = (avail - leading - overhead).max(0.0);
        // 以首行 bounds.y0 为基准，找能放进 usable 的最大行数（按真实 bounds 跨度）。
        let mut fit = 0usize;
        while i + fit < n {
            let span = lines[i + fit].bounds.y1 - lines[i].bounds.y0;
            if span > usable {
                break;
            }
            fit += 1;
        }

        if fit == 0 {
            // 当前页连一行（含外边距/内边距）都放不下：换页重试。
            if *ctx.used > 0.0 || !ctx.cur.blocks.is_empty() {
                ctx.push_page();
            }
            // 换页后仍放不下（单行比整页还高）时至少放一行，宁可溢出也不丢内容。
            let usable = (content_h - leading - overhead).max(0.0);
            fit = 1;
            while i + fit < n {
                let span = lines[i + fit].bounds.y1 - lines[i].bounds.y0;
                if span > usable {
                    break;
                }
                fit += 1;
            }
        }

        let seg = rebase_lines(&lines[i..i + fit], lines[i].bounds.y0);
        // 片段高度按重定基后的末行底边计算（含空行占位），与 block_height 一致。
        let seg_h = lines_visual_height(&seg, 0.0) + overhead;
        let kind = match &block.kind {
            BlockKind::Paragraph { .. } => BlockKind::Paragraph { lines: seg },
            BlockKind::CodeBlock { code, lang, .. } => BlockKind::CodeBlock {
                code: code.clone(),
                lang: lang.clone(),
                lines: seg,
            },
            _ => unreachable!("paginate_lines_block 只接受 Paragraph/CodeBlock"),
        };
        ctx.cur.blocks.push(PositionedBlock::new(
            Block::new(kind, style.clone(), block.splittable),
            content_x,
            content_y + *ctx.used + leading,
            seg_h,
        ));
        *ctx.used += leading + seg_h;
        i += fit;

        if i >= n {
            *ctx.used += margin_bottom;
            break;
        }
        ctx.push_page();
    }
}

#[cfg(test)]
mod pagination_tests {
    use super::*;
    use crate::document::text::LineMetrics;
    use lievisual::geometry::Rect;

    /// 构造一行文本（runs 留空即可，分页逻辑只关心 `bounds` 与行数）。
    fn make_line(y: f64) -> TextLine {
        TextLine {
            runs: Vec::new(),
            bounds: Rect::new(0.0, y, 100.0, y + 14.0),
            metrics: LineMetrics {
                ascent: 0.0,
                descent: 0.0,
                baseline: 0.0,
                line_height: 14.0,
            },
            ink_bounds: Rect::new(0.0, y, 100.0, y + 14.0),
        }
    }

    fn make_lines(n: usize) -> Vec<TextLine> {
        (0..n).map(|i| make_line(i as f64 * 14.0)).collect()
    }

    #[test]
    fn rebase_lines_shifts_to_new_origin() {
        let lines = make_lines(3);
        let rebased = rebase_lines(&lines[1..], lines[1].bounds.y0);
        assert_eq!(rebased[0].bounds.y0, 0.0, "片段首行应归零");
        assert_eq!(rebased[1].bounds.y0, 14.0, "后续行应保持相对间距");
    }

    #[test]
    fn long_code_block_paginates_without_losing_lines() {
        let n = 60;
        // ResolvedStyle::default 的 line_height_pt = 15，60 行 ≈ 908pt > 一页高。
        let block = Block::new(
            BlockKind::CodeBlock {
                code: String::new(),
                lang: None,
                lines: make_lines(n),
            },
            ResolvedStyle::default(),
            true,
        );
        let doc = Document {
            blocks: vec![block],
        };
        let pages = paginate_layout(&doc, &PageSettings::default());

        let mut total = 0usize;
        for page in &pages {
            for pb in &page.blocks {
                if let BlockKind::CodeBlock { lines, .. } = &pb.block.kind {
                    total += lines.len();
                    // 每个片段都应重新基准化到自身原点。
                    assert_eq!(lines[0].bounds.y0, 0.0, "片段首行 bounds.y0 应归零");
                }
            }
        }
        assert_eq!(total, n, "跨页后代码行不应丢失");
        assert!(
            pages.len() >= 2,
            "60 行代码块应跨多页，实际 {} 页",
            pages.len()
        );
    }

    // ─── 表格分页（H3/H5 回归）────────────────────────────────

    fn text_block(t: &str) -> Block {
        Block::new(
            BlockKind::Text {
                text: t.to_string(),
            },
            ResolvedStyle::default(),
            true,
        )
    }

    fn labeled_row(label: &str) -> TableRow {
        TableRow {
            cells: vec![crate::document::layout::TableCell {
                children: vec![text_block(label)],
            }],
        }
    }

    /// 按 `row_heights` 构造表格并运行分页，返回（页面列表，used 总量）。
    fn paginate_rows(row_heights: &[f64], content_h: f64) -> (Vec<PdfPage>, f64) {
        let n = row_heights.len();
        let rows: Vec<TableRow> = (0..n)
            .map(|i| labeled_row(&format!("row{i}")))
            .collect();
        let style = ResolvedStyle::default();
        let mut pages = Vec::new();
        let mut cur = PdfPage::default();
        let mut used = 0.0_f64;
        let mut ctx = PaginateCtx {
            pages: &mut pages,
            cur: &mut cur,
            used: &mut used,
            header: &None,
            footer: &None,
        };
        paginate_table(
            &rows,
            &[],
            &vec![50.0; n],
            row_heights,
            &style,
            &mut ctx,
            0.0,
            0.0,
            content_h,
        );
        // 模拟 paginate_layout 的收尾：把最后一张未满页的 cur 推入 pages 并记账。
        drop(ctx);
        if !cur.blocks.is_empty() || pages.is_empty() {
            cur.used_h = used;
            pages.push(cur);
        }
        (pages, used)
    }

    #[test]
    fn table_pagination_repeats_real_header_and_matches_height() {
        // content_h=100，行高 [20(表头),30,40,50,60]：
        // 页1 = 表头20+30+40=90；页2 = 表头20+50=70；页3 = 表头20+60=80。
        let (pages, _used) = paginate_rows(&[20.0, 30.0, 40.0, 50.0, 60.0], 100.0);
        assert_eq!(pages.len(), 3, "应切为 3 页，实际 {:?}", pages.len());
        // 记账总量 = 表头 + 全部 body 行（无双计/漏计），且等于全部片段绘制高度。
        let all_drawn: f64 = pages
            .iter()
            .flat_map(|p| p.blocks.iter())
            .map(|b| b.height)
            .sum();
        let total_used: f64 = pages.iter().map(|p| p.used_h).sum();
        assert!((total_used - 240.0).abs() < 1e-9, "used 总量应 240，实际 {total_used}");
        assert!(
            (total_used - all_drawn).abs() < 1e-9,
            "used 总量 {total_used} != 片段绘制总高 {all_drawn}"
        );
        // 每页记账 == 该页片段实际绘制高度，且不得超过页高。
        for p in &pages {
            let drawn: f64 = p.blocks.iter().map(|b| b.height).sum();
            assert!(
                (p.used_h - drawn).abs() < 1e-9,
                "页记账 {} != 绘制高度 {}",
                p.used_h,
                drawn
            );
            assert!(p.used_h <= 100.0 + 1e-9, "页占用 {} 超过页高", p.used_h);
        }
        // 续页片段头部应补回真实表头 row0（且每行只出现一次，不重复绘制）。
        let p2 = &pages[1];
        assert_eq!(p2.blocks.len(), 1);
        if let BlockKind::Table { rows, row_heights, .. } = &p2.blocks[0].block.kind {
            assert_eq!(rows.len(), row_heights.len(), "行数与行高数应一致");
            assert_eq!(rows[0].cells_text(), "row0", "续页片段首行应为真实表头");
            assert_eq!(rows[1].cells_text(), "row3");
            assert_eq!(row_heights[0], 20.0, "续页表头高应取原表头行高");
        } else {
            panic!("续页片段应为 Table 块");
        }
    }

    #[test]
    fn table_starting_on_fresh_page_does_not_double_count_header() {
        // H5 回归：表格从空页开始时 used 增量不得双计表头高。
        // 单表头行 20 + 两行 30/40，content_h=100 → 单页 90，无分页。
        let (pages, used) = paginate_rows(&[20.0, 30.0, 40.0], 100.0);
        assert_eq!(pages.len(), 1);
        assert!((used - 90.0).abs() < 1e-9, "used 应 90（无表头双计），实际 {used}");
        if let BlockKind::Table { rows, .. } = &pages[0].blocks[0].block.kind {
            assert_eq!(rows.len(), 3);
            assert_eq!(rows[0].cells_text(), "row0");
        }
    }

    #[test]
    fn header_only_table_paginates_without_forced_row() {
        // 只有表头一行的表格：不得触发「强制放一行」的越界/死循环路径。
        let (pages, used) = paginate_rows(&[20.0], 100.0);
        assert_eq!(pages.len(), 1);
        assert!((used - 20.0).abs() < 1e-9);
    }

    #[test]
    fn table_row_taller_than_page_still_progresses() {
        // 某一行高过整页：不得死循环，该行独占一页（溢出但内容保留）。
        let (pages, used) = paginate_rows(&[20.0, 30.0, 500.0, 40.0], 100.0);
        // 500pt 行独占一页；前后行正常切分。
        let total_used: f64 = pages.iter().map(|p| p.used_h).sum();
        assert!(
            total_used >= 20.0 + 30.0 + 500.0 + 40.0,
            "所有行高都应计入 used（含超高行），实际 {total_used}"
        );
        let _ = used;
        let mut placed_rows = 0usize;
        for p in &pages {
            for pb in &p.blocks {
                if let BlockKind::Table { rows, .. } = &pb.block.kind {
                    placed_rows += rows.len() - 1.min(rows.len()); // 扣除片段表头
                }
            }
        }
        assert_eq!(placed_rows, 3, "全部 body 行都应被放置");
    }

    // ─── 容器块分页（H4 回归）────────────────────────────────

    fn list_item(height: f64) -> Block {
        let mut para_style = ResolvedStyle::default();
        para_style.line_height_pt = height as f32;
        Block::new(
            BlockKind::ListItem {
                marker: "1.".to_string(),
                children: vec![Block::new(
                    BlockKind::Paragraph { lines: vec![] },
                    para_style,
                    true,
                )],
            },
            ResolvedStyle::default(),
            true,
        )
    }

    #[test]
    fn long_list_fragments_at_item_boundaries() {
        // 3 个各 60pt 的列表项，页高 100：应切成 3 个片段而不是整块溢出。
        let list = Block::new(
            BlockKind::List {
                ordered: true,
                start: None,
                children: vec![list_item(60.0), list_item(60.0), list_item(60.0)],
            },
            ResolvedStyle::default(),
            true,
        );
        let settings = PageSettings::default();
        let frags = fragment_container(&list, &settings, 0.0, 100.0);
        assert_eq!(frags.len(), 3, "每个列表项应独占一个片段");
        // 片段保持 List > ListItem 结构（绘制端缩进/marker 逻辑无需感知分页）。
        assert!(matches!(&frags[0].kind, BlockKind::List { children, .. } if children.len() == 1));
        // 中间片段上下外边距清零，首/末片段保留。
        assert_eq!(frags[1].style.margin_top, 0.0);
        assert_eq!(frags[1].style.margin_bottom, 0.0);
    }

    #[test]
    fn long_list_paginates_into_pages() {
        // 端到端：60 项 × 每项约 18pt 的列表应跨多页且所有项都被放置。
        let list = Block::new(
            BlockKind::List {
                ordered: false,
                start: None,
                children: (0..60).map(|_| list_item(18.0)).collect(),
            },
            ResolvedStyle::default(),
            true,
        );
        let doc = Document { blocks: vec![list] };
        let pages = paginate_layout(&doc, &PageSettings::default());
        assert!(pages.len() >= 2, "60 项列表应跨页，实际 {} 页", pages.len());
        let mut placed = 0usize;
        for p in &pages {
            for pb in &p.blocks {
                if let BlockKind::List { children, .. } = &pb.block.kind {
                    placed += children.len();
                }
            }
        }
        assert_eq!(placed, 60, "所有列表项都应被放置（内容不丢）");
    }

    #[test]
    fn definition_list_fragments_at_item_boundaries() {
        let dl = |h: f64| {
            let mut s = ResolvedStyle::default();
            s.line_height_pt = h as f32;
            DefinitionItemBlock {
                term: vec![Block::new(BlockKind::Paragraph { lines: vec![] }, s.clone(), true)],
                definition: vec![],
            }
        };
        let list = Block::new(
            BlockKind::DefinitionList {
                items: vec![dl(60.0), dl(60.0), dl(60.0)],
            },
            ResolvedStyle::default(),
            true,
        );
        let settings = PageSettings::default();
        let frags = fragment_container(&list, &settings, 0.0, 100.0);
        assert_eq!(frags.len(), 3, "定义列表应按项切分");
    }

    // ─── EXIF 方向绘制参数（2026-09-04 审查） ───

    #[test]
    fn exif_draw_params_rotation_swaps_draw_rect() {
        // 显示盒 200×100，方向 6（顺时针旋转 90°）：绘制矩形应为存储态 100×200，
        // 矩阵把存储左上角映射到显示右上角（u=0,v=0 → x=w）。
        let (rw, rh, row) = exif_draw_params(6, 200.0, 100.0);
        assert_eq!(rw, 100.0);
        assert_eq!(rh, 200.0);
        let [sx, ky, kx, sy, tx, ty] = row.unwrap();
        assert_eq!((sx, ky, kx, sy, tx, ty), (0.0, 1.0, -1.0, 0.0, 200.0, 0.0));
        // 验证角点映射：存储 (0,0) → (200,0)；存储 (100,200) → (0,100)。
        let map = |u: f32, v: f32| (sx * u + kx * v + tx, ky * u + sy * v + ty);
        assert_eq!(map(0.0, 0.0), (200.0, 0.0));
        assert_eq!(map(100.0, 200.0), (0.0, 100.0));
    }

    #[test]
    fn exif_draw_params_identity_and_180() {
        let (rw, rh, row) = exif_draw_params(1, 200.0, 100.0);
        assert_eq!((rw, rh), (200.0, 100.0));
        assert!(row.is_none());

        // 方向 3（旋转 180°）：绘制矩形不变，中心对称。
        let (rw, rh, row) = exif_draw_params(3, 200.0, 100.0);
        assert_eq!((rw, rh), (200.0, 100.0));
        let [sx, ky, kx, sy, tx, ty] = row.unwrap();
        let map = |u: f32, v: f32| (sx * u + kx * v + tx, ky * u + sy * v + ty);
        assert_eq!(map(0.0, 0.0), (200.0, 100.0));
        assert_eq!(map(200.0, 100.0), (0.0, 0.0));
    }
}
