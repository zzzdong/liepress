//! 文档图片类型（自包含二进制，不依赖渲染层）。

use crate::color::Color;
use crate::document::types::ObjectFit;

/// 文档图片（自包含二进制）。
///
/// 此处直接持有原始字节，使文档模型不依赖具体加载路径。
#[derive(Clone, Debug)]
pub struct DocImage {
    /// 图片左上角相对父块的坐标（pt）
    pub position: (f64, f64),
    /// 显示尺寸（pt）
    pub size: (f64, f64),
    /// 原始像素尺寸（宽, 高）
    pub pixel_size: (u32, u32),
    /// 图片字节
    pub data: Vec<u8>,
    /// 格式（如 "png", "jpeg"）
    pub format: String,
    /// 替代文本
    pub alt: String,
    /// 适应方式
    pub object_fit: ObjectFit,
    /// 可选背景色（占位/边框用）
    pub background: Option<Color>,
}
