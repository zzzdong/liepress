//! 测试公共模块
//!
//! 提供测试共享的工具函数和类型

use std::fs;
use std::path::PathBuf;

/// 获取测试输出目录
pub fn test_output_dir(test_name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push("liepress_tests");
    path.push(test_name);
    let _ = fs::create_dir_all(&path);
    path
}

/// 获取诊断输出目录
pub fn diag_output_dir(subdir: &str) -> PathBuf {
    let dir = PathBuf::from("target/diag_output").join(subdir);
    fs::create_dir_all(&dir).expect("Should create output directory");
    dir
}

/// 保存测试输出文件
pub fn save_test_output(path: &PathBuf, data: &[u8]) {
    fs::write(path, data).expect("Should write output file");
}

/// 测试用的 Markdown 样本
pub mod samples {
    /// 基础文档
    pub const BASIC: &str = r#"# Test Document

This is a test paragraph."#;

    /// 完整功能展示
    pub const FULL_FEATURED: &str = r#"# Heading 1

This is a paragraph with **bold** and *italic* text.

## Heading 2

- List item 1
- List item 2
- List item 3

```rust
fn main() {
    println!("Hello, world!");
}
```

> This is a blockquote.

---

[Link to example](https://example.com)"#;

    /// 代码块
    pub const CODE_BLOCK: &str = r#"# Code Example

```rust
fn main() {
    println!("hello");
}
```"#;

    /// 嵌套列表
    pub const NESTED_LIST: &str = r#"# Nested List

- Item 1
  - Sub item 1.1
  - Sub item 1.2
- Item 2
  - Sub item 2.1"#;

    /// 有序列表
    pub const ORDERED_LIST: &str = r#"1. First item
2. Second item
3. Third item"#;
}
