//! 文档类型定义
//!
//! 定义 Document、Page、PageContext 等核心数据结构。

use crate::ast::PageConfig;
use crate::generator::constants::{
    PAGE_HEIGHT_PT, PAGE_MARGIN_BOTTOM_PT, PAGE_MARGIN_LEFT_PT, PAGE_MARGIN_RIGHT_PT,
    PAGE_MARGIN_TOP_PT, PAGE_WIDTH_PT,
};
use crate::generator::context::OutlineEntry;
use crate::visual::VisualElement;

/// 页面设置 - 可配置的页面尺寸和边距
#[derive(Debug, Clone)]
pub struct PageSettings {
    pub width_pt: f32,
    pub height_pt: f32,
    pub margin_top_pt: f32,
    pub margin_bottom_pt: f32,
    pub margin_left_pt: f32,
    pub margin_right_pt: f32,

    // ─── 无限高度模式 ──────────────────────────────────────
    /// 仅限定宽度，高度无限（不分页，所有内容连续排列在一个页面上）
    ///
    /// 启用后：
    /// - `content_height()` 返回 `f32::MAX`，所有分页检查永不触发
    /// - `start_new_page()` / `finalize_current_page()` 变为空操作
    /// - 最终输出单页文档，页面高度 = 实际内容高度
    pub height_unlimited: bool,

    // ─── 页眉页脚 ──────────────────────────────────────────
    /// 页眉文本（支持 {page} 和 {total} 模板变量）
    pub header: Option<String>,
    /// 页脚文本（支持 {page} 和 {total} 模板变量）
    pub footer: Option<String>,
    /// 页眉字体大小（pt），默认 9pt
    pub header_font_size: f32,
    /// 页脚字体大小（pt），默认 9pt
    pub footer_font_size: f32,
}

impl Default for PageSettings {
    fn default() -> Self {
        Self {
            width_pt: PAGE_WIDTH_PT,
            height_pt: PAGE_HEIGHT_PT,
            margin_top_pt: PAGE_MARGIN_TOP_PT,
            margin_bottom_pt: PAGE_MARGIN_BOTTOM_PT,
            margin_left_pt: PAGE_MARGIN_LEFT_PT,
            margin_right_pt: PAGE_MARGIN_RIGHT_PT,
            height_unlimited: false,
            header: None,
            footer: Some("- {page} -".to_string()),
            header_font_size: 9.0,
            footer_font_size: 9.0,
        }
    }
}

impl PageSettings {
    /// A4 页面（默认）
    pub fn a4() -> Self {
        Self::default()
    }

    /// 自定义页面尺寸和边距
    pub fn new(width_pt: f32, height_pt: f32) -> Self {
        Self {
            width_pt,
            height_pt,
            ..Default::default()
        }
    }

    /// 设置边距
    pub fn with_margins(mut self, top: f32, bottom: f32, left: f32, right: f32) -> Self {
        self.margin_top_pt = top;
        self.margin_bottom_pt = bottom;
        self.margin_left_pt = left;
        self.margin_right_pt = right;
        self
    }

    /// 启用无限高度模式（仅限定宽度，高度自适应内容）
    pub fn with_height_unlimited(mut self, unlimited: bool) -> Self {
        self.height_unlimited = unlimited;
        self
    }

    /// 内容区左上角 X 坐标
    pub fn content_x(&self) -> f32 {
        self.margin_left_pt
    }

    /// 内容区左上角 Y 坐标
    pub fn content_y(&self) -> f32 {
        self.margin_top_pt
    }

    /// 内容区宽度
    pub fn content_width(&self) -> f32 {
        self.width_pt - self.margin_left_pt - self.margin_right_pt
    }

    /// 内容区高度（无限高度模式下返回 f32::MAX）
    pub fn content_height(&self) -> f32 {
        if self.height_unlimited {
            f32::MAX
        } else {
            self.height_pt - self.margin_top_pt - self.margin_bottom_pt
        }
    }
}

impl From<PageConfig> for PageSettings {
    fn from(config: PageConfig) -> Self {
        Self {
            width_pt: config.width.unwrap_or(PAGE_WIDTH_PT),
            height_pt: config.height.unwrap_or(PAGE_HEIGHT_PT),
            margin_top_pt: config.margin_top.unwrap_or(PAGE_MARGIN_TOP_PT),
            margin_bottom_pt: config.margin_bottom.unwrap_or(PAGE_MARGIN_BOTTOM_PT),
            margin_left_pt: config.margin_left.unwrap_or(PAGE_MARGIN_LEFT_PT),
            margin_right_pt: config.margin_right.unwrap_or(PAGE_MARGIN_RIGHT_PT),
            height_unlimited: config.height_unlimited.unwrap_or(false),
            header: config.header,
            footer: config.footer,
            header_font_size: config.header_font_size.unwrap_or(9.0),
            footer_font_size: config.footer_font_size.unwrap_or(9.0),
        }
    }
}

/// 文档结构 - 包含页面列表和大纲
#[derive(Debug, Clone)]
pub struct Document {
    pub pages: Vec<Page>,
    pub page_width: f32,
    pub page_height: f32,
    /// 文档大纲（标题层级结构）
    pub outline: Vec<OutlineEntry>,
}

/// 布局完成的文档——布局阶段的显式输出
///
/// `DocumentLayout` 是 `DocumentGenerator` 的产出物，代表经过完整布局计算后的文档。
/// 它包含的内存布局布局与 `Document` 相同，但语义上明确属于"布局层"而非"渲染层"。
/// 通过 `From` / `Into` 转换可轻松得到 `Document` 供渲染器使用。
#[derive(Debug, Clone)]
pub struct DocumentLayout {
    pub pages: Vec<Page>,
    pub page_width: f32,
    pub page_height: f32,
    pub outline: Vec<OutlineEntry>,
}

impl From<DocumentLayout> for Document {
    fn from(layout: DocumentLayout) -> Self {
        Document {
            pages: layout.pages,
            page_width: layout.page_width,
            page_height: layout.page_height,
            outline: layout.outline,
        }
    }
}

/// 页面结构 - 包含视觉元素列表
#[derive(Debug, Clone)]
pub struct Page {
    pub elements: Vec<VisualElement>,
    pub width: f32,
    pub height: f32,
    pub index: usize,
}

impl Page {
    pub fn new(width: f32, height: f32, index: usize) -> Self {
        Self {
            elements: Vec::new(),
            width,
            height,
            index,
        }
    }

    pub fn add_element(&mut self, element: VisualElement) {
        self.elements.push(element);
    }

    /// 在元素列表开头插入元素（用于背景矩形，确保在文本下方绘制）
    pub fn prepend_element(&mut self, element: VisualElement) {
        self.elements.insert(0, element);
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

/// 页面上下文 - 维护当前页的布局状态
#[derive(Debug, Clone)]
pub struct PageContext {
    pub pages: Vec<Page>,
    pub current_page: Page,
    pub current_y: f32,
    pub settings: PageSettings,
}

impl PageContext {
    pub fn new(settings: PageSettings) -> Self {
        Self {
            pages: Vec::new(),
            current_page: Page::new(settings.width_pt, settings.height_pt, 0),
            current_y: 0.0,
            settings,
        }
    }

    pub fn add_element(&mut self, element: VisualElement) {
        self.current_page.add_element(element);
    }

    /// 在当前页元素列表开头插入元素（用于背景矩形，确保在文本下方绘制）
    pub fn add_element_before_text(&mut self, element: VisualElement) {
        self.current_page.prepend_element(element);
    }

    pub fn is_empty(&self) -> bool {
        self.current_page.is_empty()
    }

    pub fn remaining_height(&self) -> f32 {
        self.settings.content_height() - self.current_y
    }

    pub fn consume_height(&mut self, height: f32) {
        self.current_y += height;
    }

    pub fn finalize_current_page(&mut self) {
        if self.settings.height_unlimited {
            return; // 无限高度模式：不分页，页面持续增长
        }
        if !self.current_page.is_empty() {
            let width = self.current_page.width;
            let height = self.current_page.height;
            let index = self.current_page.index;
            self.pages.push(std::mem::replace(
                &mut self.current_page,
                Page::new(width, height, index + 1),
            ));
        }
        self.current_y = 0.0;
    }

    pub fn start_new_page(&mut self) {
        self.finalize_current_page();
    }

    pub fn finish(mut self) -> DocumentLayout {
        let page_width = self.current_page.width;
        let page_height = self.current_page.height;

        if self.settings.height_unlimited {
            // 单页模式：动态设置页面高度 = 实际内容高度 + 上下边距
            let actual_height =
                self.current_y + self.settings.margin_bottom_pt + self.settings.margin_top_pt;
            self.current_page.height = actual_height;
            if !self.current_page.is_empty() {
                self.pages.push(self.current_page);
            }
            return DocumentLayout {
                pages: self.pages,
                page_width,
                page_height: actual_height,
                outline: Vec::new(),
            };
        }

        self.finalize_current_page();
        DocumentLayout {
            pages: self.pages,
            page_width,
            page_height,
            outline: Vec::new(),
        }
    }
}
