//! 端到端管线测试。
//!
//! 组织方式：
//! - `pipeline/` 按管线步骤与功能点拆分：
//!   - `dom`    Layer 1：Markdown/HTML -> DOM 结构
//!   - `styled` Layer 2：DOM -> Styled AST（脚注、CSS 样式）
//!   - `layout` Layer 3：Styled -> Layout（空格、居中、定义列表、图片）
//!   - `pdf` / `svg` / `png` / `docx` 输出格式
//!   - `html_input` 以 HTML 作为管线入口
//! - `pdf_validation` PDF 深度校验（字体、对象、资源）

mod pdf_validation;
mod pipeline;
