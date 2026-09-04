//! 代码块语法高亮（基于 syntect），作用于 **AST 层**。
//!
//! 高亮属于「文档内容」而非「渲染细节」，因此统一在 AST 富化阶段（[`crate::enrich`]）
//! 完成，产物以 [`CodeSpan`] 形式挂在 `NodeKind::CodeBlock::spans` 上。
//! 这样 PDF / SVG / PNG / HTML / DOCX 五个后端共享同一份着色结果：
//! - PDF/SVG/PNG：`document::from_ast` 把 spans 排版成 `TextLine`；
//! - HTML：每段一个 `<span style="color:...">`；
//! - DOCX：每段一个带颜色的 `Run`。
//!
//! 设计取舍：
//! - 主题固定使用 syntect 内置的**浅色主题**（`InspiredGitHub`），与内置样式表
//!   `pre { background-color: #f6f8fa }` 的浅底配色匹配；此前使用的暗色主题
//!   （`base16-ocean.dark`）在浅底上对比度不足，且无法适配 DOCX/HTML 的浅色背景。
//! - 字体族/字号/字重不属于高亮范畴，由各后端按 CSS 投影样式决定；syntect 仅贡献
//!   每段的前景色与粗/斜体标记。
//! - 语法集与主题集为进程级单例（[`assets`]），避免每次高亮重复解析数百个语法定义。

use crate::ast::{CodeSpan, Node, NodeKind, walk_mut};
use lievisual::Color;

use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color as SynColor, FontStyle as SynFontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

/// 懒加载的共享语法集与主题集（进程级单例）。
struct HighlightAssets {
    syntax_set: SyntaxSet,
    theme: syntect::highlighting::Theme,
}

static ASSETS: OnceLock<HighlightAssets> = OnceLock::new();

fn assets() -> &'static HighlightAssets {
    ASSETS.get_or_init(|| {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        // 浅色主题优先（与代码块浅灰底匹配）；逐级回退，避免键不存在时 panic。
        let themes = ThemeSet::load_defaults();
        let theme = themes
            .themes
            .get("InspiredGitHub")
            .or_else(|| themes.themes.get("base16-ocean.light"))
            .or_else(|| themes.themes.get("base16-ocean.dark"))
            .cloned()
            .unwrap_or_default();
        HighlightAssets { syntax_set, theme }
    })
}

/// 把 syntect 颜色转成内部 [`Color`]。
fn to_color(c: SynColor) -> Color {
    Color::rgb(c.r, c.g, c.b)
}

/// 主题默认前景色（未知语言/无语法定义时的兜底色）。
fn default_foreground() -> Color {
    let a = assets();
    match a.theme.settings.foreground {
        Some(c) => to_color(c),
        None => Color::rgb(36, 41, 46),
    }
}

/// 根据语言名查找语法定义（支持常见别名，如 `js`/`sh`/`cpp`）。
fn find_syntax<'a>(ss: &'a SyntaxSet, lang: &str) -> Option<&'a syntect::parsing::SyntaxReference> {
    if lang.is_empty() {
        return None;
    }
    ss.find_syntax_by_token(lang)
        .or_else(|| ss.find_syntax_by_extension(lang))
}

/// 把一段代码按语法切分为「行 → 着色片段」。
///
/// 纯语义切分，不含任何排版信息。`lang` 为空或语法未知时退化为每行单色片段
/// （使用主题默认前景色），保证下游各后端行为一致。
pub fn tokenize(code: &str, lang: &str) -> Vec<Vec<CodeSpan>> {
    let a = assets();
    let syntax = find_syntax(&a.syntax_set, lang);

    // 行切分：与 `document::from_ast` 重建全文的方式严格对应 ——
    // 末尾换行会产生一个额外的空行占位（保持空行不被吞掉）。
    let mut lines: Vec<&str> = code.split('\n').collect();
    if code.ends_with('\n') {
        lines.push("");
    }

    match syntax {
        Some(syntax) => {
            let mut h = HighlightLines::new(syntax, &a.theme);
            let fallback_color = default_foreground();
            lines
                .into_iter()
                .map(|line| {
                    let hl = h.highlight_line(line, &a.syntax_set);
                    match hl {
                        Ok(ranges) => ranges
                            .into_iter()
                            .map(|(style, text)| CodeSpan {
                                bold: style.font_style.contains(SynFontStyle::BOLD),
                                italic: style.font_style.contains(SynFontStyle::ITALIC),
                                color: to_color(style.foreground),
                                text: text.to_string(),
                            })
                            // 丢弃空片段，减少下游无意义的 Run/span。
                            .filter(|s| !s.text.is_empty())
                            .collect(),
                        Err(_) => vec![CodeSpan {
                            text: line.to_string(),
                            color: fallback_color,
                            bold: false,
                            italic: false,
                        }],
                    }
                })
                .collect()
        }
        None => lines
            .into_iter()
            .map(|line| {
                vec![CodeSpan {
                    text: line.to_string(),
                    color: default_foreground(),
                    bold: false,
                    italic: false,
                }]
            })
            .collect(),
    }
}

/// AST 富化 pass：为树中所有代码块填充语法高亮结果。
///
/// 幂等：`spans` 已有值（已高亮）的节点会被跳过，重复调用不产生额外开销。
pub fn highlight_code_blocks(node: &mut Node) {
    walk_mut(node, &mut |n| {
        if let NodeKind::CodeBlock {
            code,
            lang,
            spans: spans @ None,
        } = &mut n.kind
        {
            *spans = Some(tokenize(code, lang.as_deref().unwrap_or("")));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parse_markdown;

    fn span_colors(lines: &[Vec<CodeSpan>]) -> Vec<Color> {
        let mut out = Vec::new();
        for line in lines {
            for s in line {
                if !out.contains(&s.color) {
                    out.push(s.color);
                }
            }
        }
        out
    }

    #[test]
    fn tokenize_rust_produces_multiple_colors() {
        let lines = tokenize("fn main() {\n    let x: i32 = 42;\n}\n", "rust");
        assert!(!lines.is_empty());
        let colors = span_colors(&lines);
        assert!(
            colors.len() > 1,
            "语法高亮应产生多种颜色，实际 {} 种",
            colors.len()
        );
    }

    #[test]
    fn tokenize_unknown_lang_falls_back_to_single_color() {
        let lines = tokenize("echo hello\n", "zzz-no-such-lang");
        assert!(!lines.is_empty());
        assert_eq!(span_colors(&lines).len(), 1, "未知语言应退化为单色");
    }

    #[test]
    fn tokenize_roundtrip_preserves_text() {
        // 片段文本按行拼接后必须能还原「下游重建的全文」
        // （末尾换行的代码额外带一个空行占位，见 `tokenize` 的行切分规则）。
        let code = "fn main() {\n    let x = 1;\n}\n";
        let lines = tokenize(code, "rust");
        let joined: Vec<String> = lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect();
        assert_eq!(joined.join("\n"), format!("{code}\n"));

        // 无末尾换行的代码不应凭空多出一行。
        let lines = tokenize("let x = 1;", "rust");
        let joined: String = lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(joined, "let x = 1;");
    }

    #[test]
    fn ast_pass_fills_spans_for_code_block() {
        let mut node = parse_markdown("```rust\nfn main() {}\n```").unwrap();
        highlight_code_blocks(&mut node);

        let mut found = 0;
        walk_mut(&mut node, &mut |n| {
            if let NodeKind::CodeBlock { spans, .. } = &n.kind {
                assert!(spans.is_some(), "高亮 pass 后 spans 应已填充");
                found += 1;
            }
        });
        assert_eq!(found, 1);
    }
}
