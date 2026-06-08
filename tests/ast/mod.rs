//! AST 模块测试
//!
//! 测试 Layer 1 (Markdown) → Layer 2 (Styled AST) 的转换

use liepress::ast::{NodeKind, parse_markdown};

mod parsing;
mod structure;
mod style;
