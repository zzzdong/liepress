//! CSS 样式解析器
//!
//! 封装 CSS 引擎，提供 HTML 上下文感知的样式解析。
//! 将 CSS 解析、祖先跟踪、内联样式应用统一管理。

use crate::ast::style::Style;
use crate::css::engine::{AncestorInfo, CssEngine};
use crate::dom::HtmlElement;

/// HTML 上下文感知的样式解析器
///
/// 职责：
/// - 包装 `CssEngine`，提供简化的样式解析 API
/// - 自动维护祖先跟踪栈
/// - 处理内联样式、display:none 等特殊逻辑
///
/// 用法：
/// ```ignore
/// let resolver = StyleResolver::new(&engine);
/// let style = resolver.resolve(elem, &parent_style);
/// resolver.with_ancestor(elem, |resolver| {
///     // 递归处理子元素，ancestor 信息自动维护
/// });
/// ```
pub struct StyleResolver<'a> {
    engine: &'a CssEngine,
    ancestors: Vec<AncestorInfo>,
}

impl<'a> StyleResolver<'a> {
    /// 创建新的样式解析器
    pub fn new(engine: &'a CssEngine) -> Self {
        Self {
            engine,
            ancestors: Vec::new(),
        }
    }

    /// 解析元素样式
    ///
    /// 结合 CSS 选择器匹配、祖先信息、父元素样式和内联样式
    /// 计算元素的最终样式。
    pub fn resolve(&self, elem: &HtmlElement, parent_style: &Style) -> Style {
        let tag = elem.tag.as_str();
        let classes = elem.classes();
        let id = elem.id();

        let mut style = self
            .engine
            .resolve_style(tag, &classes, id, &self.ancestors, parent_style);

        // 应用内联样式
        if let Some(inline_css) = elem.inline_style() {
            self.engine.apply_inline_style(&mut style, inline_css);
        }

        style
    }

    /// 解析文本节点的样式
    pub fn resolve_text(&self, parent_style: &Style) -> Style {
        self.engine
            .resolve_style("span", &[], None, &self.ancestors, parent_style)
    }

    /// 在当前祖先上下文中执行操作
    ///
    /// 调用 `f` 前将 `elem` 推入祖先栈，完成后弹出。
    /// 适用于递归处理子元素。
    pub fn with_ancestor<F, R>(&mut self, elem: &HtmlElement, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        let tag = elem.tag.as_str();
        let classes = elem.classes();
        let id = elem.id();

        self.ancestors.push(AncestorInfo {
            tag: tag.to_string(),
            classes,
            id: id.map(|s| s.to_string()),
        });

        let result = f(self);

        self.ancestors.pop();

        result
    }

    /// 直接获取祖先栈（只读）
    pub fn ancestors(&self) -> &[AncestorInfo] {
        &self.ancestors
    }

    /// 判断元素是否 display:none
    pub fn is_hidden(&self, elem: &HtmlElement, parent_style: &Style) -> bool {
        self.resolve(elem, parent_style).display == crate::ast::style::Display::None
    }

    /// 访问底层 CSS 引擎
    pub fn engine(&self) -> &'a CssEngine {
        self.engine
    }
}
