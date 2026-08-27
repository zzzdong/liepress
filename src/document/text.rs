//! 文本排版（统一委托给 lievisual）。
//!
//! liepress 不再维护自有的排版类型/引擎，直接复用 lievisual 的文本排版：
//! - [`TextLayout`] / [`TextLine`] / [`TextRun`] / [`Glyph`] / [`TextStyle`] /
//!   [`FontStyle`] / [`TextAlign`] / [`TextBaseline`] / [`TextDecoration`] 均为
//!   `lievisual::text` 的**同一类型**（此处 `pub use` 转发），消除重复定义。
//! - 排版 / 测量统一走 `lievisual::text::layout_text` / `measure_text`，liepress 不再
//!   直接触达 parley。
//!
//! 保留的 liepress 侧内容仅为：
//! - 字体注册与上下文桥接（[`FontSource`] / [`register_font`] / `with_*_context` /
//!   [`FONT_BYTES`] / [`get_font_bytes`]），供 PDF 等需要嵌入字体的后端使用。
//! - CSS → lievisual 样式的构造桥接（[`css_text_style`]）与语法高亮区间
//!   （[`StyleRange`] / [`layout_text_with_ranges`]）。

/// 文本排版类型直接复用 `lievisual::text`，`crate::ast` 已 `pub use` 转发，
/// 故下游既可写 `crate::ast::TextAlign` 也可写 `lievisual::text::TextAlign`，
/// 二者为同一类型。
pub use crate::ast::TextAlign;
pub use lievisual::text::{
    FontStyle, FontWeight, Glyph, LineMetrics, TextBaseline, TextDecoration, TextLayout, TextLine,
    TextRun, TextStyle,
};
use lievisual::{FontWidth, geometry::Rect as LRect};
use std::sync::Arc;

// 已注册字体的原始字节缓存（供 PdfRenderer 等需要嵌入字体的后端使用）。
thread_local! {
    pub static FONT_BYTES: std::cell::RefCell<std::collections::HashMap<String, Arc<Vec<u8>>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// 获取已注册字体的原始字节。
pub fn get_font_bytes(family: &str) -> Option<Arc<Vec<u8>>> {
    FONT_BYTES.with(|cache| cache.borrow().get(family).cloned())
}

/// 访问 lievisual 的线程本地字体上下文。
pub fn with_font_context<R, F: FnOnce(&mut lievisual::parley::FontContext) -> R>(f: F) -> R {
    use std::cell::RefCell;
    thread_local! {
        static FC: RefCell<lievisual::parley::FontContext> =
            RefCell::new(lievisual::parley::FontContext::new());
    }
    FC.with(|cx| f(&mut cx.borrow_mut()))
}

/// 访问 lievisual 的线程本地布局上下文。
pub fn with_layout_context<
    R,
    F: FnOnce(&mut lievisual::parley::LayoutContext<lievisual::Color>) -> R,
>(
    f: F,
) -> R {
    use std::cell::RefCell;
    thread_local! {
        static LC: RefCell<lievisual::parley::LayoutContext<lievisual::Color>> =
            RefCell::new(lievisual::parley::LayoutContext::new());
    }
    LC.with(|cx| f(&mut cx.borrow_mut()))
}

/// 同时访问两个上下文的便捷函数。
pub fn with_text_contexts<
    R,
    F: FnOnce(
        &mut lievisual::parley::FontContext,
        &mut lievisual::parley::LayoutContext<lievisual::Color>,
    ) -> R,
>(
    f: F,
) -> R {
    with_font_context(|fc| with_layout_context(|lc| f(fc, lc)))
}

/// 字体来源。
#[derive(Debug, Clone)]
pub enum FontSource {
    /// 从文件路径加载。
    Path(std::path::PathBuf),
    /// 从内存数据加载。
    Memory(Vec<u8>),
}

/// 注册自定义字体到全局字体上下文（委托给 lievisual，并缓存原始字节供 PDF 嵌入）。
pub fn register_font(
    source: FontSource,
    family_name_override: Option<&str>,
) -> crate::error::Result<()> {
    let (lv_source, arc_bytes) = match source {
        FontSource::Path(path) => {
            let bytes = std::fs::read(&path).map_err(|e| {
                crate::error::Error::FontLoadError(format!("读取字体文件失败: {e}"))
            })?;
            let arc = Arc::new(bytes);
            (lievisual::FontSource::Path(path), arc)
        }
        FontSource::Memory(bytes) => {
            let arc = Arc::new(bytes);
            (lievisual::FontSource::Memory(arc.to_vec()), arc)
        }
    };

    if let Some(g) = family_name_override.and_then(parse_generic_family_name) {
        lievisual::register_font_generic(lv_source, None, Some(g))
            .map_err(crate::error::Error::FontLoadError)?;
    } else {
        lievisual::register_font(lv_source, family_name_override)
            .map_err(crate::error::Error::FontLoadError)?;
    }

    if let Some(family) = family_name_override {
        FONT_BYTES.with(|cache| {
            cache.borrow_mut().insert(family.to_string(), arc_bytes);
        });
    }
    Ok(())
}

/// 解析通用字体名称到 lievisual 的 GenericFamily。
fn parse_generic_family_name(name: &str) -> Option<lievisual::parley::fontique::GenericFamily> {
    lievisual::parse_generic_family(name)
}

/// 按 liepress 语义的 [`TextDecoration`] 设置 lievisual `TextStyle` 的
/// `underline` / `strikethrough` 标志（lievisual 样式没有单一 `decoration` 字段）。
///
/// `TextDecoration` 已直接是 `lievisual::text::TextDecoration`（见文件顶部
/// `pub use`），此处仅作语义桥接到 lievisual 的 `underline` / `strikethrough` 标志位。
#[inline]
pub fn set_decoration(style: &mut TextStyle, dec: TextDecoration) {
    match dec {
        TextDecoration::None => {
            style.underline = false;
            style.strikethrough = false;
        }
        TextDecoration::Underline => {
            style.underline = true;
            style.strikethrough = false;
        }
        TextDecoration::LineThrough => {
            style.underline = false;
            style.strikethrough = true;
        }
    }
}

/// 将 CSS 字重字符串/数值转换为字重数值（100–900）。
pub fn weight_to_f32(weight: &str) -> f32 {
    match weight.to_lowercase().as_str() {
        "normal" => 400.0,
        "bold" => 700.0,
        "thin" | "100" => 100.0,
        "extra_light" | "200" => 200.0,
        "light" | "300" => 300.0,
        "medium" | "500" => 500.0,
        "semi_bold" | "600" => 600.0,
        "extra_bold" | "800" => 800.0,
        "black" | "900" => 900.0,
        _ => weight.parse::<f32>().unwrap_or(400.0).clamp(100.0, 900.0),
    }
}

/// 将 CSS 字体风格字符串转换为 lievisual 的 [`FontStyle`]。
fn style_to_fontstyle(style: &str) -> FontStyle {
    match style.to_lowercase().as_str() {
        "italic" => FontStyle::Italic,
        "oblique" => FontStyle::Oblique,
        _ => FontStyle::Normal,
    }
}

/// 将 liepress 侧习惯的 CSS 字面量（`Vec<String>` 字体族、字符串字重/风格、
/// `TextDecoration` 修饰）构造为一个 lievisual [`TextStyle`]。
///
/// liepress 下游（`ast::presets`、`output::common`、`from_ast`）以 CSS 字符串形态
/// 持有样式，这里统一桥接到 lievisual 的数值/枚举字段；布局类型本身即是 lievisual 的。
#[must_use]
pub fn css_text_style(
    color: lievisual::Color,
    font_family: &[String],
    font_size: f64,
    font_weight: &str,
    font_style: &str,
    align: TextAlign,
    url: Option<String>,
    decoration: TextDecoration,
    baseline_shift: f32,
    background_color: Option<lievisual::Color>,
) -> TextStyle {
    let (underline, strikethrough) = match decoration {
        TextDecoration::None => (false, false),
        TextDecoration::Underline => (true, false),
        TextDecoration::LineThrough => (false, true),
    };
    TextStyle {
        color: color,
        font_family: font_family.join(", "),
        font_size,
        font_weight: FontWeight::parse(font_weight).unwrap_or_default(),
        font_style: style_to_fontstyle(font_style),
        font_width: FontWidth::Normal,
        line_height: None,
        letter_spacing: 0.0,
        underline,
        underline_color: None,
        strikethrough,
        strikethrough_color: None,
        baseline_shift: baseline_shift as f64,
        background_color: background_color,
        url,
        rotation: 0.0,
        max_width: None,
        align,
        baseline: lievisual::text::TextBaseline::Top,
    }
}

// ─── 排版函数（直接委托 lievisual，无需转换） ──────────────────────

/// 把 lievisual 返回的 `Arc<TextLayout>` 转为拥有所有权（引用计数为 1 时零拷贝）。
fn into_owned(layout: std::sync::Arc<TextLayout>) -> TextLayout {
    std::sync::Arc::try_unwrap(layout).unwrap_or_else(|a| (*a).clone())
}

/// 构造一个默认的 lievisual [`TextStyle`]（liepress 语义：黑色 10.5pt sans-serif 左对齐）。
#[must_use]
pub fn default_text_style() -> TextStyle {
    TextStyle::new(lievisual::Color::BLACK, 10.5, "sans-serif")
}

/// 创建文本布局（委托给 lievisual）。
#[must_use]
pub fn create_text_layout(text: &str, style: &TextStyle, max_width: Option<f64>) -> TextLayout {
    let span = lievisual::RichSpan::new(text.to_string(), style.clone());
    let spans = [span];
    into_owned(lievisual::text::layout_text(&spans, max_width))
}

/// 将多段不同样式的文本合并在一个 TextLayout 中（委托给 lievisual）。
///
/// `align` 即 `lievisual::text::TextAlign`（与 `crate::ast::TextAlign` 为同一类型），
/// 直接用于每个 span 的对齐，无需额外映射。
#[must_use]
pub fn layout_text(
    texts: &[(&str, &TextStyle)],
    max_width: Option<f64>,
    align: TextAlign,
) -> TextLayout {
    let spans: Vec<lievisual::RichSpan> = texts
        .iter()
        .map(|(text, style)| {
            let mut s = (*style).clone();
            s.align = align;
            lievisual::RichSpan::new((*text).to_string(), s)
        })
        .collect();
    into_owned(lievisual::text::layout_text(&spans, max_width))
}

/// 排版时按字节区间覆盖样式的区间（供语法高亮等按 token 区间着色的场景）。
#[derive(Debug, Clone)]
pub struct StyleRange {
    /// 字节起始（相对全文）。
    pub start: usize,
    /// 字节结束（不含，相对全文）。
    pub end: usize,
    /// 区间文本颜色。
    pub color: lievisual::Color,
    /// 区间字重（"normal" / "bold" 或数值）。
    pub font_weight: String,
    /// 区间字体风格（"normal" / "italic"）。
    pub font_style: String,
}

/// 对一段全文，基于基础样式 `base` 施加若干 `ranges` 区间样式后排版
/// （委托给 lievisual 的富文本排版）。用于代码高亮等「整段排一次、区间覆盖颜色」场景。
#[must_use]
pub fn layout_text_with_ranges(
    full: &str,
    base: &TextStyle,
    ranges: &[StyleRange],
    max_width: Option<f64>,
    align: TextAlign,
) -> TextLayout {
    if full.is_empty() {
        return TextLayout {
            lines: Vec::new(),
            width: 0.0,
            height: 0.0,
            ink_bounds: LRect::new(0.0, 0.0, 0.0, 0.0),
        };
    }
    let lv_align = align;

    let mut spans: Vec<lievisual::RichSpan> = Vec::new();
    let mut pos = 0usize;
    let mut ranges: Vec<StyleRange> = ranges.to_vec();
    ranges.sort_by_key(|r| r.start);
    for r in &ranges {
        let s = r.start.min(full.len());
        let e = r.end.min(full.len()).max(s);
        if s < pos || s >= e {
            continue;
        }
        if s > pos {
            spans.push(lievisual::RichSpan::new(
                full[pos..s].to_string(),
                base.clone(),
            ));
        }
        let mut over = base.clone();
        over.color = r.color;
        over.font_weight = FontWeight::from_value(weight_to_f32(&r.font_weight));
        over.font_style = style_to_fontstyle(&r.font_style);
        spans.push(lievisual::RichSpan::new(full[s..e].to_string(), over));
        pos = e;
    }
    if pos < full.len() {
        spans.push(lievisual::RichSpan::new(
            full[pos..].to_string(),
            base.clone(),
        ));
    }
    if spans.is_empty() {
        spans.push(lievisual::RichSpan::new(full.to_string(), base.clone()));
    }
    // 对齐：以首 span 承载 align。
    if let Some(first) = spans.first_mut() {
        first.style.align = lv_align;
    }
    into_owned(lievisual::text::layout_text(&spans, max_width))
}
