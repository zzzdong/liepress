//! CSS 样式引擎模块
//!
//! 基于 Lightning CSS 实现的 CSS 样式解析和匹配引擎。
//! 替代原有的手写 CSS 解析器，提供浏览器级的 CSS 支持。

pub mod engine;

pub use engine::CssEngine;
