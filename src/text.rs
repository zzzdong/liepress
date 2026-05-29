use crate::error::Error;
use crate::visual::Color;
use parley::fontique::GenericFamily;
use parley::style::{FontFamily, FontFamilyName, FontStyle as ParleyFontStyle, FontWeight, StyleProperty};
use parley::{Alignment, AlignmentOptions, FontContext, LayoutContext};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use vello_cpu::kurbo::Rect;


/// 文本布局包装类型
pub type TextLayout = parley::Layout<Color>;

/// 字形信息 - 与 parley 的 Glyph 一一对应
#[derive(Clone, Debug)]
pub struct Glyph {
    /// 字形 ID
    pub id: u32,
    /// X 坐标（相对 VisualElement::TextRun.position 的偏移）
    pub x: f32,
    /// Y 坐标（相对 VisualElement::TextRun.position 的偏移）
    pub y: f32,
    /// 前进宽度
    pub advance: f32,
}

/// 文本 Run - 与 parley 的 GlyphRun 一一对应
///
/// 一个 Run 是一段具有相同样式（字体、字号、颜色）的连续字形序列。
/// 它是文本渲染的最小单元。
///
/// 坐标系设计：
/// - `TextLine.bounds.origin`: 行在页面上的位置（绝对坐标）
/// - `glyphs[].x/y`: 相对 `bounds.origin` 的偏移量
///
/// 渲染时：glyph 页面坐标 = bounds.origin + glyph.x/y
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
}

/// 文本行 - 包含一行中的所有 Run
///
/// 分页以 TextLine 为单位，确保行不会被截断。
///
/// 坐标系设计：
/// - `bounds`: 行在页面上的绝对坐标和尺寸
/// - `runs[].glyphs[].x/y`: 相对 `bounds.origin` 的偏移量
#[derive(Clone, Debug)]
pub struct TextLine {
    /// 该行的所有 Run
    pub runs: Vec<TextRun>,
    /// 行的边界框（页面绝对坐标）
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

/// 注册自定义字体到全局字体上下文。
///
/// 适用于不需要创建 `LieChart` 实例即可加载字体的场景。
/// 加载后的字体可以通过 `font_family` 名称在图表的文本样式中使用。
///
/// # 示例
///
/// ```ignore
/// // 从内存加载（例如从 CDN 下载的字节）
/// liecharts::register_font(liecharts::FontSource::Memory(font_bytes), Some("MyFont")).unwrap();
/// ```
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

    let override_info = family_name_override.map(|name| parley::fontique::FontInfoOverride {
        family_name: Some(name),
        ..Default::default()
    });

    crate::text::with_font_context(|font_cx| {
        font_cx.collection.register_fonts(data, override_info);
    });

    if let Some(family) = family_name_override {
        FONT_BYTES.with(|cache| {
            cache.borrow_mut().insert(family.to_string(), arc_bytes);
        });
    }

    Ok(())
}

/// 创建文本布局
///
/// 使用 parley 以 **左对齐** 排版文本，返回布局以获取自然宽度/高度。
/// 组件的对齐（居中、右对齐等）应在拿到 layout 尺寸后手动计算位置偏移。
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
    // 创建布局构建器
    let mut builder = layout_cx.ranged_builder(font_cx, text, 1.0, true);

    // 应用样式
    let font_stack = to_parley_font_family(&style.font_family);
    builder.push_default(StyleProperty::FontFamily(font_stack));
    builder.push_default(StyleProperty::FontSize(style.font_size as f32));
    builder.push_default(StyleProperty::Brush(style.color));
    builder.push_default(StyleProperty::FontStyle(to_parley_font_style(&style.font_style)));
    builder.push_default(StyleProperty::FontWeight(to_parley_font_weight(&style.font_weight)));

    // 构建布局
    let mut layout = builder.build(text);

    // 断行
    layout.break_all_lines(max_width.map(|w| w as f32));

    // 始终左对齐：parley 不做居中/右对齐，组件的对齐由 compute_text_offset 或手动计算实现
    layout.align(Alignment::Start, AlignmentOptions::default());

    layout
}

/// 将多段不同样式的文本合并在一个 TextLayout 中。
///
/// 每段文本可以有自己的 TextStyle（字体、字号、颜色）。
/// 所有文本按顺序直接拼接，通过 parley 的 RangedBuilder 为各段应用不同样式。
/// 需要换行时，请在文本段中自行包含 `\n`。
/// 最终返回单一的 TextLayout，支持断行和多行对齐。
///
/// # 参数
/// - `texts`: 文本段列表，每项为 `(文本内容, 文本样式)`。至少包含一段。
/// - `max_width`: 最大行宽，`None` 表示不断行。
/// - `align`: 多行对齐方式。`Left`、`Center` 或 `Right`。
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
///
/// 这是 `layout_text` 的低级版本，适用于需要复用上下文的场景。
/// 直接拼接所有文本段（不带额外分隔符），以第一段的样式为默认样式，
/// 其余各段通过 ranged_builder 的 `push` 方法覆盖特定范围的样式属性。
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

    // 1. 直接拼接所有文本（不带额外分隔符，用户可在文本中自行添加 \n）
    let mut combined = String::new();
    let mut ranges: Vec<(usize, usize)> = Vec::with_capacity(texts.len());
    for (text, _) in texts {
        let start = combined.len();
        combined.push_str(text);
        let end = combined.len();
        ranges.push((start, end));
    }

    // 2. 创建 ranged_builder，以第一段样式作为默认
    let mut builder = layout_cx.ranged_builder(font_cx, &combined, 1.0, true);

    let first_style = &texts[0].1;
    let default_font_stack = to_parley_font_family(&first_style.font_family);
    builder.push_default(StyleProperty::FontFamily(default_font_stack));
    builder.push_default(StyleProperty::FontSize(first_style.font_size as f32));
    builder.push_default(StyleProperty::Brush(first_style.color));
    builder.push_default(StyleProperty::FontStyle(to_parley_font_style(&first_style.font_style)));
    builder.push_default(StyleProperty::FontWeight(to_parley_font_weight(&first_style.font_weight)));

    // 3. 后续各段覆盖样式（直接推送所有样式，不做条件判断）
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
        builder.push(StyleProperty::FontStyle(to_parley_font_style(&style.font_style)), start..end);
        builder.push(StyleProperty::FontWeight(to_parley_font_weight(&style.font_weight)), start..end);
    }

    // 4. 构建布局
    let mut layout = builder.build(&combined);

    // 5. 断行
    layout.break_all_lines(max_width.map(|w| w as f32));

    // 6. 对齐：映射 TextAlign → parley::Alignment
    let paragraph_align = match align {
        TextAlign::Left => Alignment::Start,
        TextAlign::Center => Alignment::Center,
        TextAlign::Right => Alignment::End,
    };
    layout.align(paragraph_align, AlignmentOptions::default());

    layout
}


/// 将字体家族列表（Vec<String>）转换为 parley 的 FontFamily::List。
///
/// 自动识别 CSS 通用家族关键字（serif、sans-serif、monospace 等），
/// 将其映射为 GenericFamily 以便 parley 使用系统字体回退。
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
            // 尝试解析为数字
            if let Ok(val) = weight.parse::<f32>() {
                FontWeight::new(val)
            } else {
                FontWeight::NORMAL
            }
        }
    }
}

// pub fn layout_text_with_contexts(
//     texts: &[(&str, &TextStyle)],
//     max_width: Option<f64>,
//     max_height: Option<f64>,
//     align: TextAlign,
//     font_cx: &mut FontContext,
//     layout_cx: &mut LayoutContext<Color>,
// ) -> Vec<TextRun> {
//     unimplemented!()
// }