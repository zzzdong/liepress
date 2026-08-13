//! 文档模型层（方案 5.2 Layer 5）。
//!
//! 承载从 [`crate::ast`] 转换后的、与渲染后端无关的文档结构（[`layout::Document`]）。
//! **本层只做不分页的源 IR**：分页（切页、跨页表格）由各输出后端负责
//! （方案 §1.1/§1.3.1/§4.1），例如 [`crate::output::pdf`] 内部对
//! `Document` 做 `paginate`。
//!
//! ## 本层定位（与方案/旧管线的演进，记录于 docs/refactor-log.md）
//! - 旧像素层 `visual` 已在重构中删除，本层不再投影到 `VisualElement`；
//!   输出后端（PDF/HTML）直接消费 `ast::Node` 或 `layout::Document`。
//! - `layout::Document` 是**统一文档中间层**：供需要精确布局的后端（PDF 等）消费，
//!   `Paragraph` 直接内嵌 parley 断行的 [`crate::document::text::TextLine`]
//!   （含字形坐标与字体字节）。HTML/DOCX 等流式输出走独立路线，直接消费
//!   `ast::Node`（不消费 `layout::Document`）。
//! - 跨层依赖单向：document → ast/text，不反向。
//! - 二进制内容（图片/字体）使用 `Vec<u8>`；图片字节在 `from_ast` 解码 data URI 后持有。

/// 文档逻辑类型（重新投影，避免上层直接依赖渲染类型）
pub mod types;

/// 文档排版模块：文本布局引擎 + 文本类型（Layout/Line/Run/Glyph/TextStyle）。
/// 排版是文档生成的组成部分，故从独立的 `crate::text` 收归到此模块。
pub mod text;

/// 文档中间表示：`Document`（不分页的源 IR 块树）
pub mod layout;

/// AST → `Document` 转换（源 IR 构建）
pub mod from_ast;
