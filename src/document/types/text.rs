//! 文档文本类型（投影自 [`crate::text::TextRun`] / [`crate::text::TextLine`]）。
//!
//! 渲染层用 parley 的 `FontData` 引用持有字形数据；文档层改为持有
//! 二进制字体字节（`Vec<u8>`），使文档模型自包含、与 parley 生命周期解耦。
//! 后续投影到 VisualElement 时再重建 `FontData`。

use crate::document::types::{DocColor, TextDecoration};
// TextDecoration 定义于 super::style，经 mod.rs 再导出，故此处可引用。

/// 文档文本 Run（同一样式连续字形序列）。
#[derive(Clone, Debug)]
pub struct DocTextRun {
    /// 该 Run 的文本内容
    pub text: String,
    /// 该 Run 在段落中的文本范围
    pub text_range: std::ops::Range<usize>,
    /// 字体原始字节（解析自 parley FontData 的 blob）
    pub font_data: Vec<u8>,
    /// 字体名称（用于回退/调试，可选）
    pub font_name: Option<String>,
    /// 字体大小（pt）
    pub font_size: f32,
    /// 文本颜色
    pub color: DocColor,
    /// 总前进宽度（pt）
    pub advance: f32,
    /// 字形列表（坐标相对所属 DocTextLine 原点的偏移）
    pub glyphs: Vec<DocGlyph>,
    /// 第一个字符的基线 X 坐标（相对行原点）
    pub baseline_x: f32,
    /// 该行的基线 Y 坐标（相对行顶偏移）
    pub baseline_y: f32,
    /// 超链接 URL（如果有）
    pub url: Option<String>,
    /// 文本修饰
    pub decoration: TextDecoration,
    /// 基线偏移（pt，使上下标相对行内位置上下移动）
    pub baseline_shift: f32,
    /// 行内背景色（None 表示无背景）
    pub background_color: Option<DocColor>,
}

/// 单个字形（投影自 [`crate::text::Glyph`]）。
#[derive(Clone, Debug)]
pub struct DocGlyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub advance: f32,
    /// 字节簇偏移（相对该 Run 自身的 text）
    pub cluster: u32,
}

/// 文档文本行（投影自 [`crate::text::TextLine`]）。
#[derive(Clone, Debug)]
pub struct DocTextLine {
    /// 该行的所有 Run
    pub runs: Vec<DocTextRun>,
    /// 行的边界框（相对段落原点；绝对坐标由布局/分页阶段赋予）
    pub bounds: (f64, f64, f64, f64), // (x0, y0, x1, y1)
    /// 该行的高度（来自 LineMetrics.line_height）
    pub line_height: f32,
}

impl From<&crate::text::Glyph> for DocGlyph {
    fn from(g: &crate::text::Glyph) -> Self {
        Self {
            id: g.id,
            x: g.x,
            y: g.y,
            advance: g.advance,
            cluster: g.cluster,
        }
    }
}

impl From<&crate::text::TextRun> for DocTextRun {
    fn from(r: &crate::text::TextRun) -> Self {
        // parley FontData 可取其底层字节缓存；此处提取原始字体字节，
        // 使文档模型自包含（不持有 parley 生命周期）。font_name 在 S0 暂留空，
        // 投影阶段可结合 FONT_BYTES 映射补全。
        let font_data: Vec<u8> = r.font_data.data.as_ref().to_vec();
        Self {
            text: r.text.clone(),
            text_range: r.text_range.clone(),
            font_data,
            font_name: None,
            font_size: r.font_size,
            color: DocColor::from(r.color),
            advance: r.advance,
            glyphs: r.glyphs.iter().map(DocGlyph::from).collect(),
            baseline_x: r.baseline_x,
            baseline_y: r.baseline_y,
            url: r.url.clone(),
            decoration: TextDecoration::from(r.decoration),
            baseline_shift: r.baseline_shift,
            background_color: r.background_color.map(DocColor::from),
        }
    }
}

impl From<&crate::text::TextLine> for DocTextLine {
    fn from(l: &crate::text::TextLine) -> Self {
        Self {
            runs: l.runs.iter().map(DocTextRun::from).collect(),
            bounds: (l.bounds.x0, l.bounds.y0, l.bounds.x1, l.bounds.y1),
            line_height: l.line_height,
        }
    }
}
