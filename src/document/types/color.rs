//! 文档颜色类型（与 [`crate::color::Color`] 同源，但去除渲染层依赖）。

/// 文档层颜色（RGBA）。
///
/// 与渲染层中立的 [`crate::color::Color`] 结构一致，二者可双向转换，
/// 使文档模型可独立存在。
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct DocColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl DocColor {
    pub const BLACK: Self = Self::new(0, 0, 0);
    pub const WHITE: Self = Self::new(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn with_alpha(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}
