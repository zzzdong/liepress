//! 文档类型定义
//!
//! 定义 Document、Page、PageContext 等核心数据结构。

use crate::visual::VisualElement;
use crate::generator::constants::{
    PAGE_WIDTH_PT, PAGE_HEIGHT_PT,
    PAGE_MARGIN_TOP_PT, PAGE_MARGIN_BOTTOM_PT,
    PAGE_MARGIN_LEFT_PT, PAGE_MARGIN_RIGHT_PT,
};
use crate::ast::PageConfig;

/// 页面设置 - 可配置的页面尺寸和边距
#[derive(Debug, Clone, Copy)]
pub struct PageSettings {
    pub width_pt: f32,
    pub height_pt: f32,
    pub margin_top_pt: f32,
    pub margin_bottom_pt: f32,
    pub margin_left_pt: f32,
    pub margin_right_pt: f32,
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

    /// 内容区高度
    pub fn content_height(&self) -> f32 {
        self.height_pt - self.margin_top_pt - self.margin_bottom_pt
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
        }
    }
}

/// 文档结构 - 包含页面列表
#[derive(Debug, Clone)]
pub struct Document {
    pub pages: Vec<Page>,
    pub page_width: f32,
    pub page_height: f32,
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

    pub fn finish(mut self) -> Document {
        let page_width = self.current_page.width;
        let page_height = self.current_page.height;
        self.finalize_current_page();
        Document {
            pages: self.pages,
            page_width,
            page_height,
        }
    }
}