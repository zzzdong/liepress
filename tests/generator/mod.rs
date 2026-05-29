//! Generator 模块测试
//!
//! 测试 Layer 2 (Styled AST) → Layer 3 (Layout Document) 的转换

use liepress::generator::{markdown_to_document, Document, Page, constants::*};

mod document;
mod layout;
mod pagination;
mod table;
