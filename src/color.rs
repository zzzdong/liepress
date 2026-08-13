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

    /// 不透明黑色常量（与旧 `DocColor::BLACK` 兼容）。
    pub const BLACK: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    /// 序列化为 `#rrggbb` 形式的 CSS 颜色字符串（忽略 alpha 通道）。
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}
