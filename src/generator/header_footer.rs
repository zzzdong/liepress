//! 页眉页脚注入
//!
//! 在所有页面中注入页眉和页脚视觉元素。
//! 支持 `{page}`（当前页码）和 `{total}`（总页数）模板变量。

use crate::generator::types::DocumentLayout;
use crate::generator::types::PageSettings;
use crate::text::{TextAlign, TextStyle};
use crate::visual::VisualElement;
use vello_cpu::kurbo::{Point, Rect};

/// 在所有页面中注入页眉和页脚视觉元素。
///
/// 页眉放置在内容区上方（顶部边距区域），页脚放置在内容区下方（底部边距区域）。
pub(crate) fn inject_header_footer(doc: &mut DocumentLayout, settings: &PageSettings) {
    let total_pages = doc.pages.len();
    if total_pages == 0 {
        return;
    }

    let content_x = settings.content_x();
    let content_y = settings.content_y();
    let content_width = settings.content_width();
    let content_height = settings.content_height();

    for page in &mut doc.pages {
        let page_num = page.index + 1;

        // ── 页眉 ──
        if let Some(ref header_template) = settings.header {
            let text = header_template
                .replace("{page}", &page_num.to_string())
                .replace("{total}", &total_pages.to_string());

            let header_style = TextStyle {
                color: crate::visual::Color::new(100, 100, 100),
                font_family: vec!["sans-serif".to_string()],
                font_size: settings.header_font_size as f64,
                font_weight: "normal".to_string(),
                font_style: "normal".to_string(),
                align: TextAlign::Center,
                url: None,
                decoration: crate::text::TextDecoration::None,
                baseline_shift: 0.0,
                background_color: None,
            };

            let layout = crate::text::layout_text(
                &[(text.as_str(), &header_style)],
                Some(content_width as f64),
                TextAlign::Center,
            );

            let header_total_height: f32 = layout.lines.iter().map(|l| l.line_height).sum();
            let header_y = content_y - header_total_height - 4.0;

            let mut rel_y = 0.0_f32;
            for line in &layout.lines {
                let line_width = line.bounds.width() as f32;
                let x_offset = ((content_width - line_width) / 2.0).max(0.0);

                let abs_left = content_x + x_offset;
                let abs_top = header_y + rel_y;

                let bounds = Rect::new(
                    abs_left as f64,
                    abs_top as f64,
                    (abs_left + line_width) as f64,
                    (abs_top + line.line_height) as f64,
                );

                page.elements.push(VisualElement::TextLine {
                    runs: line.runs.clone(),
                    bounds,
                    line_height: line.line_height,
                });

                rel_y += line.line_height;
            }

            // 页眉分隔线（文字下方 2pt）
            let line_y = header_y + rel_y + 2.0;
            page.elements.push(VisualElement::Line {
                start: Point::new(content_x as f64, line_y as f64),
                end: Point::new((content_x + content_width) as f64, line_y as f64),
                style: crate::visual::StrokeStyle {
                    color: crate::visual::Color::new(200, 200, 200),
                    width: 0.5,
                },
            });
        }

        // ── 页脚 ──
        if let Some(ref footer_template) = settings.footer {
            let text = footer_template
                .replace("{page}", &page_num.to_string())
                .replace("{total}", &total_pages.to_string());

            let footer_style = TextStyle {
                color: crate::visual::Color::new(100, 100, 100),
                font_family: vec!["sans-serif".to_string()],
                font_size: settings.footer_font_size as f64,
                font_weight: "normal".to_string(),
                font_style: "normal".to_string(),
                align: TextAlign::Center,
                url: None,
                decoration: crate::text::TextDecoration::None,
                baseline_shift: 0.0,
                background_color: None,
            };

            let layout = crate::text::layout_text(
                &[(text.as_str(), &footer_style)],
                Some(content_width as f64),
                TextAlign::Center,
            );

            let footer_y = content_y + content_height + 4.0;

            let mut rel_y = 0.0_f32;
            for line in &layout.lines {
                let line_width = line.bounds.width() as f32;
                let x_offset = ((content_width - line_width) / 2.0).max(0.0);

                let abs_left = content_x + x_offset;
                let abs_top = footer_y + rel_y;

                let bounds = Rect::new(
                    abs_left as f64,
                    abs_top as f64,
                    (abs_left + line_width) as f64,
                    (abs_top + line.line_height) as f64,
                );

                page.elements.push(VisualElement::TextLine {
                    runs: line.runs.clone(),
                    bounds,
                    line_height: line.line_height,
                });

                rel_y += line.line_height;
            }
        }
    }
}
