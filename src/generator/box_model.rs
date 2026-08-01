//! Box Model 布局基础设施
//!
//! 提供块级元素的盒模型布局方法：
//! - `begin_box` / `end_box`：管理 margin、padding、背景、边框
//! - `draw_box_border` / `draw_border`：绘制边框（支持直角和圆角）

use crate::ast::Style;
use crate::ast::style::BoxSides;
use crate::visual::{Color, FillStrokeStyle, Stroke, VisualElement};
use vello_cpu::kurbo::Rect;

use super::PageContext;

/// 背景样式（用于 `place_text_lines` 等方法）
pub(crate) struct BgStyle {
    pub color: Color,
    pub padding: BoxSides,
    pub margin: BoxSides,
}

// ─── Box Model 入口 / 出口 ─────────────────────────────────────

/// 开始块级元素的 Box Model 布局。
///
/// 处理顺序：margin.top → padding.top → 设置水平偏移。
/// 返回 `(content_indent, content_width, content_start_y, prev_indent)`，
/// 内容布局完成后必须调用 `end_box` 收尾。
pub(crate) fn begin_box(
    ctx: &mut PageContext,
    current_indent: f32,
    style: &Style,
) -> (f32, f32, f32, f32) {
    // 消耗 margin.top
    ctx.consume_height(style.margin.top);

    // 计算水平偏移和可用宽度
    let content_indent = current_indent + style.margin.left + style.padding.left;
    let content_width = ctx.settings.content_width()
        - current_indent
        - style.margin.left
        - style.padding.left
        - style.margin.right
        - style.padding.right;

    // 记录内容起始位置
    let content_start_y = ctx.current_y;

    // 消耗 padding.top
    ctx.consume_height(style.padding.top);

    (
        content_indent,
        content_width,
        content_start_y,
        current_indent,
    )
}

/// 结束块级元素的 Box Model 布局（仅内容区收尾，不绘制背景/边框）
///
/// 处理顺序：padding.bottom → margin.bottom。
/// 适用于跨页内容（段落、代码块等），背景/边框由上层按页处理。
pub(crate) fn end_box_content(ctx: &mut PageContext, style: &Style) {
    ctx.consume_height(style.padding.bottom);
    ctx.consume_height(style.margin.bottom);
}

/// 结束块级元素的 Box Model 布局。
///
/// 处理顺序：padding.bottom → 绘制背景 → 绘制边框 → margin.bottom。
/// 适用于不跨页的内容（标题、图片、容器等），背景和边框一次性绘制。
pub(crate) fn end_box(
    ctx: &mut PageContext,
    style: &Style,
    content_start_y: f32,
    prev_indent: f32,
) {
    // 消耗 padding.bottom
    ctx.consume_height(style.padding.bottom);

    // 绘制背景矩形
    draw_box_background(ctx, style, content_start_y, prev_indent);

    // 绘制边框
    let indent = prev_indent + style.margin.left + style.padding.left;
    let available_width = ctx.settings.content_width()
        - prev_indent
        - style.margin.left
        - style.padding.left
        - style.margin.right
        - style.padding.right;
    draw_box_border(ctx, style, content_start_y, indent, available_width);

    // 消耗 margin.bottom
    ctx.consume_height(style.margin.bottom);
}

// ─── 背景绘制 ──────────────────────────────────────────────────

/// 绘制块级元素的背景矩形（单页，从 content_start_y 到当前 y）
pub(crate) fn draw_box_background(
    ctx: &mut PageContext,
    style: &Style,
    content_start_y: f32,
    prev_indent: f32,
) {
    if let Some(bg_color) = style.background_color {
        let content_x = ctx.settings.content_x();
        let content_y = ctx.settings.content_y();
        let content_width = ctx.settings.content_width();
        let bg_top = content_y + content_start_y - style.padding.top;
        let bg_bottom = content_y + ctx.current_y + style.padding.bottom;
        let bg_left = content_x + prev_indent + style.margin.left;
        let bg_right = content_x + content_width - style.margin.right;
        ctx.add_element_before_text(VisualElement::Rect {
            rect: Rect::new(
                bg_left as f64,
                bg_top as f64,
                bg_right as f64,
                bg_bottom as f64,
            ),
            style: FillStrokeStyle {
                fill: Some(bg_color),
                stroke: None,
            },
        });
    }
}

// ─── 边框绘制 ──────────────────────────────────────────────────

/// 绘制块级元素的边框。
///
/// 根据内容起始位置和当前 y 位置计算边框矩形。
pub(crate) fn draw_box_border(
    ctx: &mut PageContext,
    style: &Style,
    content_start_y: f32,
    indent: f32,
    available_width: f32,
) {
    if !style.border.is_any_visible() {
        return;
    }
    let content_x = ctx.settings.content_x();
    let content_y = ctx.settings.content_y();
    let bw = style.border.max_width();
    let border_color = style.border.top.color;
    let box_x = content_x + indent - style.padding.left - bw;
    let box_y = content_y + content_start_y - style.padding.top - bw;
    let box_w = available_width + style.padding.left + style.padding.right + 2.0 * bw;
    let box_h =
        ctx.current_y - content_start_y + style.padding.top + style.padding.bottom + 2.0 * bw;
    draw_border(
        ctx,
        box_x,
        box_y,
        box_w,
        box_h,
        border_color,
        bw,
        style.border.radius,
    );
}

/// 绘制边框矩形（四边），支持圆角
#[allow(clippy::too_many_arguments)]
fn draw_border(
    ctx: &mut PageContext,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    border_color: Color,
    border_width: f32,
    radius: f32,
) {
    if radius > 0.0 {
        // 圆角边框：使用 RoundedRect
        let r = radius as f64;
        ctx.add_element(VisualElement::RoundedRect {
            rect: Rect::new(x as f64, y as f64, (x + width) as f64, (y + height) as f64),
            radii: (r, r, r, r),
            style: FillStrokeStyle {
                fill: None,
                stroke: Some(Stroke::new(border_color, border_width as f64)),
            },
        });
    } else {
        // 直角边框：四条矩形
        let x = x as f64;
        let y = y as f64;
        let w = width as f64;
        let h = height as f64;
        let bw = border_width as f64;
        // 上边
        ctx.add_element(VisualElement::Rect {
            rect: Rect::new(x, y, x + w, y + bw),
            style: FillStrokeStyle {
                fill: Some(border_color),
                stroke: None,
            },
        });
        // 下边
        ctx.add_element(VisualElement::Rect {
            rect: Rect::new(x, y + h - bw, x + w, y + h),
            style: FillStrokeStyle {
                fill: Some(border_color),
                stroke: None,
            },
        });
        // 左边
        ctx.add_element(VisualElement::Rect {
            rect: Rect::new(x, y, x + bw, y + h),
            style: FillStrokeStyle {
                fill: Some(border_color),
                stroke: None,
            },
        });
        // 右边
        ctx.add_element(VisualElement::Rect {
            rect: Rect::new(x + w - bw, y, x + w, y + h),
            style: FillStrokeStyle {
                fill: Some(border_color),
                stroke: None,
            },
        });
    }
}
