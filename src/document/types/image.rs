//! 文档图片类型（自包含二进制，不依赖渲染层）。

use crate::document::types::ObjectFit;
use lievisual::Color;

/// 文档图片（自包含二进制）。
///
/// 此处直接持有原始字节，使文档模型不依赖具体加载路径。
#[derive(Clone, Debug)]
pub struct DocImage {
    /// 图片左上角相对父块的坐标（pt）
    pub position: (f64, f64),
    /// 显示尺寸（pt）
    pub size: (f64, f64),
    /// 原始像素尺寸（宽, 高）。
    ///
    /// EXIF 方向为 5–8（需旋转 90°）的图片存储**显示方向**的尺寸
    /// （即已交换宽高），与 [`Self::orientation`] 配套。
    pub pixel_size: (u32, u32),
    /// EXIF Orientation（1–8，默认 1 = 正常方向）。
    ///
    /// 5–8 需旋转 90° 渲染；各渲染端据此校正（PDF 用变换矩阵，
    /// PNG/SVG 解码位图后 `apply_orientation`）。
    pub orientation: u8,
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
