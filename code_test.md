## 5. 代码块语法高亮

无语言标识的代码块（退化为单色等宽）：

```
echo "Hello, liepress!"
```

Rust 代码块（高亮）：

```rust
use std::fs;

use liepress::{ConvertOptions, PageConfig, markdown_to_pdf};

fn main() {
    let md = "# Hello\nThis is **liepress**.";
    let options = ConvertOptions::default().with_page_config(PageConfig {
        width: Some(210.0),
        height: Some(297.0), // A4 纵向
        ..PageConfig::default()
    });

    let pdf = markdown_to_pdf(md, &options).unwrap();
    fs::write("output.pdf", pdf).unwrap();
}
```
