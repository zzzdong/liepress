//! 代码块语法高亮（基于 syntect）。
//!
//! 在文档层把代码块预排版成带颜色的 [`TextLine`]，供 PDF/SVG/PNG 后端直接消费。
//! 后端无需任何改动即可获得着色效果：只需迭代 `BlockKind::CodeBlock.lines`。
//!
//! 设计取舍：
//! - 高亮在 `from_ast` 阶段完成（而非后端），因为着色本质是「文档内容」而非「渲染细节」。
//! - 字体/字号/字重由 CSS 投影的 [`ResolvedStyle`] 决定（通过 `base` 传入），
//!   syntect 仅贡献每段的前景色（`Color`）。
//! - 主题固定使用 syntect 内置的 `base16-ocean.dark`（暗色背景 + 亮色代码，与代码块
//!   245/245/245 浅灰背景对比度不足，因此这里单独采用暗色主题，配合后端绘制时的
//!   深色背景）。

use crate::ast::TextAlign;
use crate::document::text::{
    StyleRange, TextLine, TextStyle, css_text_style, layout_text_with_ranges,
};
use crate::document::types::ResolvedStyle;
use lievisual::Color;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Color as SynColor, FontStyle as SynFontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

/// 懒加载的共享语法集与主题集（进程级单例，避免每次高亮重复解析几百个语法定义）。
struct HighlightAssets {
    syntax_set: SyntaxSet,
    theme: syntect::highlighting::Theme,
}

static ASSETS: std::sync::OnceLock<HighlightAssets> = std::sync::OnceLock::new();

fn assets() -> &'static HighlightAssets {
    ASSETS.get_or_init(|| {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        // 优先暗色主题，回退 InspiredGitHub，二者都缺失（理论不会）退化为默认主题。
        // 原来的回退用 `themes["InspiredGitHub"]` 索引（键不存在会 panic），且
        // `ThemeSet::load_defaults()` 被重复解析了两次，这里统一复用一份。
        let themes = ThemeSet::load_defaults();
        let theme = themes
            .themes
            .get("base16-ocean.dark")
            .or_else(|| themes.themes.get("InspiredGitHub"))
            .cloned()
            .unwrap_or_default();
        HighlightAssets { syntax_set, theme }
    })
}

/// 把 syntect 颜色转成内部 [`Color`]。
fn to_color(c: SynColor) -> Color {
    Color::rgb(c.r, c.g, c.b)
}

/// 从文档层投影样式构造基础排版样式（字体族/字号/字重来自 CSS，颜色作为兜底）。
///
/// 代码块语义上必须使用等宽字体，因此 `font_family` 强制覆盖为 `["monospace"]`，
/// 不受正文 CSS 字体族影响（否则比例字体会让代码错位、视觉上「挤在一起」）。
/// 字号/字重/颜色仍来自投影样式，syntect 仅在其上覆盖各 token 前景色。
fn base_style(style: &ResolvedStyle) -> TextStyle {
    css_text_style(
        style.color,
        &["monospace".to_string()],
        style.font_size_pt as f64,
        if style.font_weight_bold {
            "bold"
        } else {
            "normal"
        },
        if style.font_style_italic {
            "italic"
        } else {
            "normal"
        },
        TextAlign::Left,
        None,
        style.text_decoration,
        0.0,
        None,
    )
}

/// 根据语言名查找语法定义（支持常见别名，如 `js`/`sh`/`cpp`）。
fn find_syntax<'a>(ss: &'a SyntaxSet, lang: &str) -> Option<&'a syntect::parsing::SyntaxReference> {
    if lang.is_empty() {
        return None;
    }
    // 优先精确匹配（syntect 已含大量别名映射），失败再尝试手动归一化。
    ss.find_syntax_by_token(lang)
        .or_else(|| ss.find_syntax_by_extension(lang))
}

/// 将一段代码按语法高亮排版为带颜色的文本行。
///
/// - `code`：完整代码文本。
/// - `lang`：代码块语言标识（空字符串表示不高亮，退化为单色 mono）。
/// - `style`：文档层投影样式（字体族/字号/字重来自 CSS），各着色段在此基础上覆盖颜色。
///
/// 返回 `Vec<TextLine>`：每行一个 [`TextLine`]，行内可能含多个不同颜色的 [`TextRun`]。
///
/// 排版策略（parley 为主、syntect 仅着色）：
/// 1. 把**完整的代码文本**（行间以 `\n` 连接）作为一份字符串交给 parley 排版一次，
///    行高与垂直偏移完全由 parley 计算，不再逐行排版后手工补 `bounds.y0`。
/// 2. syntect 对**同一份完整文本**逐行 `highlight_line`，把每个着色段映射为全局
///    字节区间 `[start, end)`，收集成 [`StyleRange`]。
/// 3. 通过 parley 的 `ranged_builder` 把这些区间的颜色/粗体/斜体施加到完整文本上。
pub fn highlight_code(code: &str, lang: &str, style: &ResolvedStyle) -> Vec<TextLine> {
    let base = base_style(style);
    let a = assets();
    let syntax = find_syntax(&a.syntax_set, lang);

    // 完整文本：行间以 \n 连接，parley 与 syntect 都消费这一份。
    // 注意保留末尾换行：若 code 以 \n 结束，补一个空段，使最后一行空行也能占位。
    let full = if code.ends_with('\n') {
        let mut s = String::with_capacity(code.len() + 1);
        s.push_str(code);
        s.push('\n');
        s
    } else {
        code.to_string()
    };

    let ranges: Vec<StyleRange> = match syntax {
        Some(syntax) => {
            let mut h = HighlightLines::new(syntax, &a.theme);
            let fallback_foreground = a.theme.settings.foreground.unwrap_or(SynColor {
                r: 200,
                g: 200,
                b: 200,
                a: 255,
            });
            let fallback = syntect::highlighting::Style {
                foreground: fallback_foreground,
                ..Default::default()
            };
            let mut out_ranges = Vec::new();
            let mut cursor = 0usize;
            for line in full.split('\n') {
                // highlight_line 针对该行（不含换行符）。
                let hl = h
                    .highlight_line(line, &a.syntax_set)
                    .unwrap_or_else(|_| vec![(fallback, line)]);
                for (ranged, text) in hl {
                    let start = cursor;
                    let end = cursor + text.len();
                    out_ranges.push(StyleRange {
                        start,
                        end,
                        color: to_color(ranged.foreground),
                        font_weight: if ranged.font_style.contains(SynFontStyle::BOLD) {
                            "bold".to_string()
                        } else {
                            "normal".to_string()
                        },
                        font_style: if ranged.font_style.contains(SynFontStyle::ITALIC) {
                            "italic".to_string()
                        } else {
                            "normal".to_string()
                        },
                    });
                    cursor = end;
                }
                // 跳过行尾的换行符（\n 占 1 字节），保持 cursor 与 `full` 对齐。
                cursor += 1;
                // 防御：若 split 得到的行 + 换行符长度与剩余不符，已超出则停止。
                if cursor > full.len() {
                    break;
                }
            }
            out_ranges
        }
        None => {
            // 无匹配语法：退化为单色 mono（整段使用 base 颜色，无区间覆盖）。
            Vec::new()
        }
    };

    let layout = layout_text_with_ranges(&full, &base, &ranges, None, TextAlign::Left);
    layout.lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::types::ResolvedStyle;

    fn plain_style() -> ResolvedStyle {
        // 用默认投影样式即可；高亮只关心前景色覆盖，基础字段不影响测试断言。
        ResolvedStyle::default()
    }

    #[test]
    fn highlight_rust_produces_multiple_colors() {
        let code = "fn main() {\n    let x: i32 = 42;\n}\n";
        let lines = highlight_code(code, "rust", &plain_style());
        assert!(!lines.is_empty(), "应至少产出一个文本行");

        // 收集所有 run 的颜色（lievisual f64 颜色），语法高亮应产生不止一种颜色
        // （关键字/类型/数字等）。
        let mut colors: Vec<lievisual::Color> = Vec::new();
        for line in &lines {
            for run in &line.runs {
                if !colors.contains(&run.color) {
                    colors.push(run.color);
                }
            }
        }
        assert!(
            colors.len() > 1,
            "语法高亮后代码块应包含多种颜色，实际只有 {} 种",
            colors.len()
        );
    }

    #[test]
    fn unknown_lang_falls_back_to_single_color() {
        let code = "echo hello\n";
        let lines = highlight_code(code, "zzz-no-such-lang", &plain_style());
        assert!(!lines.is_empty());
        let mut colors: Vec<lievisual::Color> = Vec::new();
        for line in &lines {
            for run in &line.runs {
                if !colors.contains(&run.color) {
                    colors.push(run.color);
                }
            }
        }
        assert_eq!(colors.len(), 1, "未知语言应退化为单色");
    }
}
