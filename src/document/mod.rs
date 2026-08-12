//! 文档模型层（方案 5.2 Layer 5）。
//!
//! 这是重构文档层的第一阶段（S0）骨架。当前仅定义文档中间表示（IR）的类型，
//! 用于承载从 [`crate::ast`] 转换后的、与渲染后端无关的文档结构。
//!
//! 跨层依赖策略（遵循方案 5.3）：
//! - 本层只做**类型投影**：逻辑类型在 [`crate::document::types`] 中重新定义，
//!   运行期通过 `From<crate::...>` 从既有类型转换，保持单向依赖
//!   （document → ast/text/visual），不反向。
//! - 二进制内容（图片/字体）使用 `Vec<u8>`，Skeleton 阶段持有原始字节，
//!   后续阶段（渲染）再转换为 [`crate::visual::VisualElement`]。
//!
//! 后续阶段（S1）在此追加：从 AST 到本层模型的转换（`from_ast`）。
//!
//! 本层**只做不分页的源 IR**（`DocumentSkeleton` + `SkeletonBlock`）。
//! 分页（切页、跨页表格）由各输出后端负责（方案 §1.1/§1.3.1/§4.1），
//! 例如 `render::pdf` 内部对 `DocumentSkeleton` 做 `paginate`。

/// 文档逻辑类型（重新投影，避免上层直接依赖渲染类型）
pub mod types;

/// 文档中间表示：Skeleton（结构化、带布局信息的文档树）
pub mod skeleton;

/// AST → Skeleton 转换（方案 S1，源 IR 构建）
pub mod from_ast;
