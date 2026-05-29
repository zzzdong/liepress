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

    /// 简单表格
    pub const SIMPLE_TABLE: &str = r#"| Header 1 | Header 2 |
|----------|----------|
| Cell A1  | Cell B1  |
| Cell A2  | Cell B2  |"#;

    /// 多列表格
    pub const WIDE_TABLE: &str = r#"| Name   | Age | City      | Country   |
|--------|-----|-----------|-----------|
| Alice  | 30  | New York  | USA       |
| Bob    | 25  | London    | UK        |
| Charlie| 35  | Beijing   | China     |"#;

    /// 各种对齐的表格
    pub const ALIGNED_TABLE: &str = r#"| Left   | Center | Right |
|:-------|:------:|------:|
| L1     | C1     | R1    |
| L2     | C2     | R2    |"#;

    /// 大表格（用于测试跨页）
    pub const LARGE_TABLE: &str = r#"| #  | Name        | Description                              |
|----|-------------|------------------------------------------|
| 1  | Item One    | This is the first item with a longer description that wraps |
| 2  | Item Two    | The second item description goes here and might wrap too |
| 3  | Item Three  | Short description                        |
| 4  | Item Four   | Another item with some details here      |
| 5  | Item Five   | Yet another item with description text that could wrap |
| 6  | Item Six    | Short                                     |
| 7  | Item Seven  | A longer description for item seven here |
| 8  | Item Eight  | Eighth item with description              |
| 9  | Item Nine   | Ninth item description goes here         |
|10  | Item Ten    | Tenth and final item description          |"#;

    /// 带内联格式的表格
    pub const FORMATTED_TABLE: &str = r#"| Feature        | Status |
|----------------|--------|
| **Bold text**  | ✅ Done |
| *Italic text*  | ✅ Done |
| `inline code`  | ⏳ WIP  |"#;

    /// 空表格
    pub const EMPTY_TABLE: &str = r#"| H1 | H2 |
|----|----|"#;
}
