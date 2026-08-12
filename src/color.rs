//! 颜色类型（中立于渲染后端）。
//!
//! 原位于 `visual::Color`，由于 `visual`（旧像素层）在本次重构中被删除，
//! 颜色作为基础类型迁移到此处供 `text` 与渲染后端共用。

/// RGBA 颜色（每个分量 0-255）。
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// 不透明颜色（alpha 默认 255）。与旧 `visual::Color::new(r,g,b)` 兼容。
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn with_alpha(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b)
    }

    pub fn black() -> Self {
        Self::new(0, 0, 0)
    }
}

impl From<crate::document::types::DocColor> for Color {
    fn from(c: crate::document::types::DocColor) -> Self {
        Self::with_alpha(c.r, c.g, c.b, c.a)
    }
}

impl From<Color> for crate::document::types::DocColor {
    fn from(c: Color) -> Self {
        crate::document::types::DocColor {
            r: c.r,
            g: c.g,
            b: c.b,
            a: c.a,
        }
    }
}
