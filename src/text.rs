use crate::error::Error;
use crate::visual::Color;
use parley::fontique::GenericFamily;
use parley::style::{
    FontFamily, FontFamilyName, FontStyle as ParleyFontStyle, FontWeight, StyleProperty,
};
use parley::{Alignment, AlignmentOptions, FontContext, LayoutContext};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use vello_cpu::kurbo::Rect;

// 重新导出 TextDecoration 供 renderer 使用
pub use crate::ast::TextDecoration;

/// 文本布局 - 包含排版后的行集合
///
/// 由 [`layout_text`] / [`layout_text_with_contexts`] 生成。
/// TextLayout 中的坐标全部相对 layout 原点（段落左上角），
/// 分页和绝对定位由 generator 模块负责。
#[derive(Clone, Debug)]
pub struct TextLayout {
    /// 排版后的所有行
    pub lines: Vec<TextLine>,
    /// 布局总宽度（pt）
    pub width: f64,
    /// 布局总高度（pt）
    pub height: f64,
}

/// 字形信息 - 与 parley 的 Glyph 一一对应
#[derive(Clone, Debug)]
pub struct Glyph {
    /// 字形 ID
    pub id: u32,
    /// X 坐标（相对所属 TextLine.bounds.origin 的偏移）
    pub x: f32,
    /// Y 坐标（相对所属 TextLine.bounds.origin 的偏移）
    pub y: f32,
    /// 前进宽度
    pub advance: f32,
}

/// 文本 Run - 与 parley 的 GlyphRun 一一对应
///
/// 一个 Run 是一段具有相同样式（字体、字号、颜色）的连续字形序列。
/// 它是文本渲染的最小单元。
#[derive(Clone, Debug)]
pub struct TextRun {
    /// 该 Run 的文本内容
    pub text: String,
    /// 该 Run 在段落中的文本范围
    pub text_range: std::ops::Range<usize>,
    /// 字体数据（指向 parley FontData 的引用）
    pub font_data: parley::FontData,
    /// 字体大小
    pub font_size: f32,
    /// 文本颜色
    pub color: Color,
    /// 总前进宽度
    pub advance: f32,
    /// 字形列表（坐标相对 TextLine.bounds.origin 偏移）
    pub glyphs: Vec<Glyph>,
    /// 是否从右到左
    pub is_rtl: bool,
    /// 第一个字符的基线 X 坐标（相对 layout 原点的绝对坐标）
    pub baseline_x: f32,
    /// 该行的基线 Y 坐标（相对行顶 row_top_rel 的偏移）
    /// 同一行内所有 run 共享此值
    pub baseline_y: f32,
    /// 超链接 URL（如果有）
    pub url: Option<String>,
    /// 文本修饰（none / underline / line-through）
    pub decoration: TextDecoration,
    /// 基线偏移（pt，使上下标相对行内位置上下移动）
    pub baseline_shift: f32,
    /// 行内背景色（用于行内代码、高亮等，None 表示无背景）
    pub background_color: Option<Color>,
}

/// 文本行 - 包含一行中的所有 Run
///
/// 坐标系设计（相对 layout 原点）：
/// - `bounds.origin`: 行在 layout 中的位置
/// - `runs[].glyphs[].x/y`: 相对 `bounds.origin` 的偏移量
///
/// 绝对定位时：glyph 页面坐标 = 页面偏移 + bounds.origin + glyph
#[derive(Clone, Debug)]
pub struct TextLine {
    /// 该行的所有 Run
    pub runs: Vec<TextRun>,
    /// 行的边界框（相对 layout 原点）
    pub bounds: Rect,
    /// 该行的高度（来自 LineMetrics.line_height）
    pub line_height: f32,
}

thread_local! {
    /// 全局字体上下文 - 线程本地存储
    pub static FONT_CONTEXT: RefCell<FontContext> = RefCell::new(FontContext::default());
    /// 全局布局上下文 - 线程本地存储
    pub static LAYOUT_CONTEXT: RefCell<LayoutContext<Color>> = RefCell::new(LayoutContext::default());
    /// 字体字节缓存 - 映射字体族名到原始字体字节
    /// 供 PdfRenderer 等需要直接访问字体数据的渲染后端使用
    pub static FONT_BYTES: RefCell<HashMap<String, Arc<Vec<u8>>>> = RefCell::new(HashMap::new());
}

/// 获取已注册字体的原始字节
pub fn get_font_bytes(family: &str) -> Option<Arc<Vec<u8>>> {
    FONT_BYTES.with(|cache| cache.borrow().get(family).cloned())
}

/// 访问字体上下文的便捷函数
pub fn with_font_context<R, F: FnOnce(&mut FontContext) -> R>(f: F) -> R {
    FONT_CONTEXT.with(|cx| f(&mut cx.borrow_mut()))
}

/// 访问布局上下文的便捷函数
pub fn with_layout_context<R, F: FnOnce(&mut LayoutContext<Color>) -> R>(f: F) -> R {
    LAYOUT_CONTEXT.with(|cx| f(&mut cx.borrow_mut()))
}

/// 同时访问两个上下文的便捷函数
pub fn with_text_contexts<R, F: FnOnce(&mut FontContext, &mut LayoutContext<Color>) -> R>(
    f: F,
) -> R {
    FONT_CONTEXT.with(|font_cx| {
        LAYOUT_CONTEXT.with(|layout_cx| f(&mut font_cx.borrow_mut(), &mut layout_cx.borrow_mut()))
    })
}

/// 文本对齐方式
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
pub struct TextStyle {
    pub color: Color,
    /// 字体家族列表（优先级从高到低）
    pub font_family: Vec<String>,
    pub font_size: f64,
    pub font_weight: String,
    pub font_style: String,
    pub align: TextAlign,
    /// 超链接 URL（如果有）
    pub url: Option<String>,
    /// 文本修饰（none / underline / line-through）
    pub decoration: TextDecoration,
    /// 基线偏移（pt，正数=上移用于上标，负数=下移用于下标）
    pub baseline_shift: f32,
    /// 行内背景色（用于行内代码、高亮等，None 表示无背景）
    pub background_color: Option<Color>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            color: Color::BLACK,
            font_family: vec!["sans-serif".to_string()],
            font_size: 10.5,
            font_weight: "normal".to_string(),
            font_style: "normal".to_string(),
            align: TextAlign::Left,
            url: None,
            decoration: TextDecoration::None,
            baseline_shift: 0.0,
            background_color: None,
        }
    }
}

/// 字体来源
#[derive(Debug, Clone)]
pub enum FontSource {
    /// 从文件路径加载
    Path(std::path::PathBuf),
    /// 从内存数据加载
    Memory(Vec<u8>),
}

/// 解析通用字体名称到 GenericFamily
fn parse_generic_family_name(name: &str) -> Option<GenericFamily> {
    match name.to_lowercase().as_str() {
        "serif" => Some(GenericFamily::Serif),
        "sans-serif" | "sansserif" => Some(GenericFamily::SansSerif),
        "monospace" => Some(GenericFamily::Monospace),
        "cursive" => Some(GenericFamily::Cursive),
        "fantasy" => Some(GenericFamily::Fantasy),
        "system-ui" | "systemui" => Some(GenericFamily::SystemUi),
        "ui-serif" | "uiserif" => Some(GenericFamily::UiSerif),
        "ui-sans-serif" | "uisansserif" => Some(GenericFamily::UiSansSerif),
        "ui-monospace" | "uimonospace" => Some(GenericFamily::UiMonospace),
        "ui-rounded" | "uirounded" => Some(GenericFamily::UiRounded),
        "emoji" => Some(GenericFamily::Emoji),
        "math" => Some(GenericFamily::Math),
        "fangsong" => Some(GenericFamily::FangSong),
        _ => None,
    }
}

/// 注册自定义字体到全局字体上下文。
///
/// 如果 `family_name_override` 是通用字体名称（如 `monospace`、`serif`、`sans-serif`），
/// 则字体会被注册到对应的通用字体族，这样当 CSS 使用 `font-family: monospace` 时，
/// 会使用注册的字体而不是系统默认字体。
pub fn register_font(
    source: FontSource,
    family_name_override: Option<&str>,
) -> crate::error::Result<()> {
    use parley::fontique::Blob;

    let (data, arc_bytes) = match source {
        FontSource::Path(path) => {
            let bytes = std::fs::read(&path)
                .map_err(|e| Error::FontLoadError(format!("读取字体文件失败: {e}")))?;
            let arc = Arc::new(bytes);
            (Blob::new(arc.clone()), arc)
        }
        FontSource::Memory(bytes) => {
            let arc = Arc::new(bytes);
            (Blob::new(arc.clone()), arc)
        }
    };

    // 检查是否是通用字体族名称
    let generic_family = family_name_override.and_then(parse_generic_family_name);

    // 如果是通用字体族，不覆盖家族名称，让字体使用原始名称
    let override_info = if generic_family.is_some() {
        None
    } else {
        family_name_override.map(|name| parley::fontique::FontInfoOverride {
            family_name: Some(name),
            ..Default::default()
        })
    };

    crate::text::with_font_context(|font_cx| {
        let registered = font_cx.collection.register_fonts(data, override_info);

        // 如果是通用字体族，将注册的字体族关联到对应的 GenericFamily
        if let Some(generic) = generic_family {
            let family_ids: Vec<_> = registered.into_iter().map(|(id, _)| id).collect();
            font_cx
                .collection
                .append_generic_families(generic, family_ids.into_iter());
        }
    });

    // 缓存字体数据（用于后续嵌入 PDF）
    if let Some(family) = family_name_override {
        FONT_BYTES.with(|cache| {
            cache.borrow_mut().insert(family.to_string(), arc_bytes);
        });
    }

    Ok(())
}

// ─── 内部辅助：从 parley Layout 提取行 ─────────────────────

/// 字形原始数据（相对 layout 原点的坐标），仅在提取过程中使用
struct GlyphRaw {
    id: u32,
    x: f32,
    y: f32,
    advance: f32,
}

/// 从 parley Layout 提取行列表，返回相对 layout 原点的 TextLine 集合。
///
/// 每个 TextLine.bounds.origin 表示该行在 layout 中的位置：
/// - x = 该行最左字形相对于 layout 左侧的偏移
/// - y = 该行顶部相对于 layout 顶部的累积偏移
///
/// runs[].glyphs[].x/y = glyph 坐标 - 行原点，即相对行左上角的偏移
fn extract_lines_from_parley(
    layout: &parley::Layout<Color>,
    full_text: &str,
    decoration_map: &[(std::ops::Range<usize>, TextDecoration)],
    baseline_shift_map: &[(std::ops::Range<usize>, f32)],
) -> Vec<TextLine> {
    let mut lines = Vec::new();
    let mut row_top_rel = 0.0_f32;

    for line in layout.lines() {
        let metrics = line.metrics();
        let line_height = metrics.line_height;
        let baseline_y = metrics.baseline - row_top_rel;

        let mut glyph_data: Vec<(GlyphRaw, usize)> = Vec::new();
        let mut run_infos: Vec<(
            Color,
            parley::FontData,
            f32,
            parley::layout::Run<'_, Color>,
            f32,
        )> = Vec::new();

        let mut next_run_idx = 0;
        for item in line.items() {
            if let parley::layout::PositionedLayoutItem::GlyphRun(glyph_run) = item {
                let run = glyph_run.run();
                let color = glyph_run.style().brush;
                let font_data = run.font().clone();
                let font_size = run.font_size();

                let first_glyph_x = glyph_run
                    .positioned_glyphs()
                    .next()
                    .map(|g| g.x)
                    .unwrap_or(0.0);

                run_infos.push((color, font_data, font_size, *run, first_glyph_x));

                for g in glyph_run.positioned_glyphs() {
                    glyph_data.push((
                        GlyphRaw {
                            id: g.id,
                            x: g.x,
                            y: g.y,
                            advance: g.advance,
                        },
                        next_run_idx,
                    ));
                }
                next_run_idx += 1;
            }
        }

        if glyph_data.is_empty() {
            row_top_rel += line_height;
            continue;
        }

        let min_x = glyph_data
            .iter()
            .map(|(g, _)| g.x)
            .fold(f32::INFINITY, f32::min);
        let max_x = glyph_data
            .iter()
            .map(|(g, _)| g.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let width = max_x - min_x;

        let mut runs = Vec::new();

        let mut run_glyph_ranges: Vec<(usize, usize)> = Vec::new();
        let mut current_start = 0;
        let mut last_run_idx = glyph_data[0].1;

        for (i, (_, run_idx)) in glyph_data.iter().enumerate() {
            if *run_idx != last_run_idx {
                run_glyph_ranges.push((current_start, i));
                current_start = i;
                last_run_idx = *run_idx;
            }
        }
        run_glyph_ranges.push((current_start, glyph_data.len()));

        for (start_idx, end_idx) in run_glyph_ranges.iter() {
            let run_idx = glyph_data[*start_idx].1;
            let (color, font_data, font_size, run, first_glyph_x) = &run_infos[run_idx];

            // 从 parley run 获取文本范围
            let run_text_range = run.text_range();
            let text_start = run_text_range.start;
            let text_end = run_text_range.end;
            let text_range = text_start..text_end;

            // 提取该 run 的文本内容
            let run_text = if text_end <= full_text.len() {
                full_text[text_start..text_end].to_string()
            } else {
                String::new()
            };

            let relative_glyphs: Vec<Glyph> = glyph_data[*start_idx..*end_idx]
                .iter()
                .map(|(g, _)| Glyph {
                    id: g.id,
                    x: g.x - min_x,
                    y: g.y - row_top_rel,
                    advance: g.advance,
                })
                .collect();

            let baseline_x = *first_glyph_x - min_x;

            // 查找该 run 的基线偏移
            let shift = lookup_baseline_shift(text_range.start, baseline_shift_map);
            let adjusted_baseline_y = baseline_y - shift;

            // 调整 glyph y 坐标：需同时考虑 baseline_y 的变化
            let adjusted_glyphs: Vec<Glyph> = relative_glyphs
                .iter()
                .map(|g| Glyph {
                    id: g.id,
                    x: g.x,
                    y: g.y + shift, // 上标：shift>0 → y 值增大 → 视觉上移（parley y 轴向下为正）
                    advance: g.advance,
                })
                .collect();

            let advance = adjusted_glyphs.iter().map(|g| g.advance).sum();

            runs.push(TextRun {
                text: run_text,
                text_range: text_range.clone(),
                font_data: font_data.clone(),
                font_size: *font_size,
                color: *color,
                advance,
                glyphs: adjusted_glyphs,
                is_rtl: false,
                baseline_x,
                baseline_y: adjusted_baseline_y,
                url: None,
                decoration: lookup_decoration(text_range.start, decoration_map),
                baseline_shift: shift,
                background_color: None,
            });
        }

        let bounds = Rect::new(
            min_x as f64,
            row_top_rel as f64,
            (min_x + width) as f64,
            (row_top_rel + line_height) as f64,
        );

        lines.push(TextLine {
            runs,
            bounds,
            line_height,
        });

        row_top_rel += line_height;
    }

    lines
}

/// 查找 byte position 对应的文本修饰
fn lookup_decoration(
    pos: usize,
    map: &[(std::ops::Range<usize>, TextDecoration)],
) -> TextDecoration {
    for (range, dec) in map {
        if range.contains(&pos) {
            return *dec;
        }
    }
    TextDecoration::None
}

/// 查找 byte position 对应的基线偏移
fn lookup_baseline_shift(pos: usize, map: &[(std::ops::Range<usize>, f32)]) -> f32 {
    for (range, shift) in map {
        if range.contains(&pos) {
            return *shift;
        }
    }
    0.0
}

// ─── 公开布局函数 ────────────────────────────────────────

/// 创建文本布局
///
/// 使用 parley 排版文本，返回包含行集合的 TextLayout。
pub fn create_text_layout(
    text: &str,
    font_config: &TextStyle,
    max_width: Option<f64>,
) -> TextLayout {
    with_text_contexts(|font_cx, layout_cx| {
        create_text_layout_with_contexts(text, font_config, max_width, font_cx, layout_cx)
    })
}

/// 使用指定的上下文创建文本布局
pub fn create_text_layout_with_contexts(
    text: &str,
    style: &TextStyle,
    max_width: Option<f64>,
    font_cx: &mut FontContext,
    layout_cx: &mut LayoutContext<Color>,
) -> TextLayout {
    let mut builder = layout_cx.ranged_builder(font_cx, text, 1.0, true);

    let font_stack = to_parley_font_family(&style.font_family);
    builder.push_default(StyleProperty::FontFamily(font_stack));
    builder.push_default(StyleProperty::FontSize(style.font_size as f32));
    builder.push_default(StyleProperty::Brush(style.color));
    builder.push_default(StyleProperty::FontStyle(to_parley_font_style(
        &style.font_style,
    )));
    builder.push_default(StyleProperty::FontWeight(to_parley_font_weight(
        &style.font_weight,
    )));

    let mut layout = builder.build(text);
    layout.break_all_lines(max_width.map(|w| w as f32));
    layout.align(Alignment::Start, AlignmentOptions::default());

    let width = layout.width() as f64;
    let height = layout.height() as f64;
    let decoration_map = [(0..text.len(), style.decoration)];
    let baseline_shift_map = if style.baseline_shift != 0.0 {
        vec![(0..text.len(), style.baseline_shift)]
    } else {
        vec![]
    };
    let lines = extract_lines_from_parley(&layout, text, &decoration_map, &baseline_shift_map);

    TextLayout {
        lines,
        width,
        height,
    }
}

/// 将多段不同样式的文本合并在一个 TextLayout 中。
pub fn layout_text(
    texts: &[(&str, &TextStyle)],
    max_width: Option<f64>,
    align: TextAlign,
) -> TextLayout {
    with_text_contexts(|font_cx, layout_cx| {
        layout_text_with_contexts(texts, max_width, align, font_cx, layout_cx)
    })
}

/// 使用指定的 FontContext 和 LayoutContext 创建多段样式文本布局。
pub fn layout_text_with_contexts(
    texts: &[(&str, &TextStyle)],
    max_width: Option<f64>,
    align: TextAlign,
    font_cx: &mut FontContext,
    layout_cx: &mut LayoutContext<Color>,
) -> TextLayout {
    if texts.is_empty() {
        return layout_text_with_contexts(
            &[("", &TextStyle::default())],
            max_width,
            align,
            font_cx,
            layout_cx,
        );
    }

    let mut combined = String::new();
    let mut ranges: Vec<(usize, usize)> = Vec::with_capacity(texts.len());
    for (text, _) in texts {
        let start = combined.len();
        combined.push_str(text);
        let end = combined.len();
        ranges.push((start, end));
    }

    let mut builder = layout_cx.ranged_builder(font_cx, &combined, 1.0, true);

    let first_style = &texts[0].1;
    let default_font_stack = to_parley_font_family(&first_style.font_family);
    builder.push_default(StyleProperty::FontFamily(default_font_stack));
    builder.push_default(StyleProperty::FontSize(first_style.font_size as f32));
    builder.push_default(StyleProperty::Brush(first_style.color));
    builder.push_default(StyleProperty::FontStyle(to_parley_font_style(
        &first_style.font_style,
    )));
    builder.push_default(StyleProperty::FontWeight(to_parley_font_weight(
        &first_style.font_weight,
    )));

    for (i, (_, style)) in texts.iter().enumerate().skip(1) {
        let (start, end) = ranges[i];
        if start >= end {
            continue;
        }
        builder.push(
            StyleProperty::FontFamily(to_parley_font_family(&style.font_family)),
            start..end,
        );
        builder.push(StyleProperty::FontSize(style.font_size as f32), start..end);
        builder.push(StyleProperty::Brush(style.color), start..end);
        builder.push(
            StyleProperty::FontStyle(to_parley_font_style(&style.font_style)),
            start..end,
        );
        builder.push(
            StyleProperty::FontWeight(to_parley_font_weight(&style.font_weight)),
            start..end,
        );
    }

    let mut layout = builder.build(&combined);
    layout.break_all_lines(max_width.map(|w| w as f32));

    let paragraph_align = match align {
        TextAlign::Left => Alignment::Start,
        TextAlign::Center => Alignment::Center,
        TextAlign::Right => Alignment::End,
    };
    layout.align(paragraph_align, AlignmentOptions::default());

    let width = layout.width() as f64;
    let height = layout.height() as f64;

    // 构建 decoration map 和 baseline_shift map
    let mut decoration_map: Vec<(std::ops::Range<usize>, TextDecoration)> = Vec::new();
    let mut baseline_shift_map: Vec<(std::ops::Range<usize>, f32)> = Vec::new();
    for (i, (_, style)) in texts.iter().enumerate() {
        let (start, end) = ranges[i];
        decoration_map.push((start..end, style.decoration));
        if style.baseline_shift != 0.0 {
            baseline_shift_map.push((start..end, style.baseline_shift));
        }
    }
    let lines = extract_lines_from_parley(&layout, &combined, &decoration_map, &baseline_shift_map);

    TextLayout {
        lines,
        width,
        height,
    }
}

/// 将字体家族列表（Vec<String>）转换为 parley 的 FontFamily::List。
fn to_parley_font_family(families: &[String]) -> FontFamily<'static> {
    let names: Vec<FontFamilyName<'static>> = families
        .iter()
        .map(|f| match GenericFamily::parse(f) {
            Some(generic) => FontFamilyName::Generic(generic),
            None => FontFamilyName::Named(Cow::Owned(f.clone())),
        })
        .collect();
    FontFamily::List(Cow::Owned(names))
}

/// 将 font_style 字符串转换为 parley 的 FontStyle
fn to_parley_font_style(style: &str) -> ParleyFontStyle {
    match style.to_lowercase().as_str() {
        "italic" => ParleyFontStyle::Italic,
        "oblique" => ParleyFontStyle::Oblique(None),
        _ => ParleyFontStyle::Normal,
    }
}

/// 将 font_weight 字符串或数值转换为 parley 的 FontWeight
fn to_parley_font_weight(weight: &str) -> FontWeight {
    match weight.to_lowercase().as_str() {
        "normal" => FontWeight::NORMAL,
        "bold" => FontWeight::BOLD,
        "thin" | "100" => FontWeight::THIN,
        "extra_light" | "200" => FontWeight::EXTRA_LIGHT,
        "light" | "300" => FontWeight::LIGHT,
        "semi_light" | "350" => FontWeight::SEMI_LIGHT,
        "medium" | "500" => FontWeight::MEDIUM,
        "semi_bold" | "600" => FontWeight::SEMI_BOLD,
        "extra_bold" | "800" => FontWeight::EXTRA_BOLD,
        "black" | "900" => FontWeight::BLACK,
        _ => {
            if let Ok(val) = weight.parse::<f32>() {
                FontWeight::new(val)
            } else {
                FontWeight::NORMAL
            }
        }
    }
}
