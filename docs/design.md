# LiePress 设计文档

## 项目概述

LiePress 是一个 Rust 实现的 Markdown 到多格式文档生成器，支持将 Markdown 转换为 PDF、SVG 和 PNG 格式。

## 核心设计原则

1. **三层 AST 架构**：清晰的转换管道，每层负责单一职责
2. **纯数据描述**：视觉元素（VisualElement）与渲染后端解耦
3. **统一渲染接口**：PageRenderer trait 支持多种输出格式
4. **流式布局引擎**：支持分页、断行等复杂排版需求

## 系统架构

```
┌─────────────────────────────────────────────────────────────────┐
│                          输入层                                  │
│                    Markdown 文本 / 文件                          │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Layer 1: MDAST                             │
│              markdown crate 解析的原始 AST                       │
│         (mdast::Node - 符合 CommonMark 规范)                    │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Layer 2: Styled AST                        │
│              带样式的抽象语法树 (ast 模块)                        │
│              Node + Style = 内容与样式分离                        │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Layer 3: Layout AST                        │
│              布局后的文档结构 (generator 模块)                    │
│              Document → Page → VisualElement                     │
└─────────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│   PDF 渲染器     │ │   SVG 渲染器     │ │   PNG 渲染器     │
│  (krilla)       │ │  (字符串拼接)     │ │  (vello_cpu)    │
└─────────────────┘ └─────────────────┘ └─────────────────┘
```

## 模块详解

### 1. AST 模块 (`src/ast/`)

负责 Markdown 到带样式 AST 的转换。

#### 1.1 核心类型

- **`Node`** (`node.rs`): 带样式的 AST 节点
  - 包含 `NodeKind`（内容类型）和 `Style`（样式信息）
  - 支持嵌套结构（如列表项包含段落）

- **`Style`** (`style.rs`): 样式定义
  - 字体家族（支持多字体回退）
  - 字体大小、粗细、样式
  - 颜色、行高、边距

#### 1.2 节点类型 (NodeKind)

| 类型 | 说明 | 示例 |
|-----|------|------|
| `Paragraph` | 段落 | 普通文本段落 |
| `Heading { level }` | 标题 | # H1, ## H2 |
| `ListItem` | 列表项 | - item, 1. item |
| `CodeBlock { lang, code }` | 代码块 | ```rust ... ``` |
| `InlineCode` | 行内代码 | `code` |
| `Text` | 纯文本 | 普通文字 |
| `Strong` | 粗体 | **bold** |
| `Emphasis` | 斜体 | *italic* |
| `Link { url }` | 链接 | [text](url) |
| `Image { url, alt }` | 图片 | ![alt](url) |
| `ThematicBreak` | 分隔线 | --- |
| `Blockquote` | 引用块 | > quote |

#### 1.3 样式预设 (`presets.rs`)

提供各元素的默认样式：

```rust
paragraph_style()     // 正文: 10.5pt 衬线字体
heading_style(level)  // 标题: 随层级递减
code_style()          // 代码块: 9pt 等宽字体
list_item_style()     // 列表项: 10.5pt 衬线字体
inline_code_style()   // 行内代码: 等宽字体
link_style()          // 链接: 蓝色
blockquote_style()    // 引用: 灰色左边框
```

### 2. Generator 模块 (`src/generator/`)

将 Styled AST 转换为布局后的 VisualElement。

#### 2.1 核心组件

- **`DocumentGenerator`**: 文档生成器主入口
- **`PageContext`**: 页面上下文，管理当前页状态
  - 剩余高度跟踪
  - 元素收集
  - 分页控制

#### 2.2 布局流程

```
parse_markdown()
    │
    ▼
build_ast() ──► Node (Styled AST)
    │
    ▼
markdown_to_document()
    │
    ├──► layout_node() ──► 根据 NodeKind 分发
    │       │
    │       ├──► layout_paragraph()
    │       ├──► layout_heading()
    │       ├──► layout_list()
    │       │       └──► layout_list_with_indent()
    │       │               └──► layout_list_item_first_child()
    │       ├──► layout_codeblock()
    │       ├──► layout_image()
    │       └──► ...
    │
    ▼
Document { pages: Vec<Page> }
```

#### 2.3 关键布局算法

**列表布局**:
- 无序列表: 固定标记区域 (10pt)
- 有序列表: 根据最大编号动态计算标记区域宽度
- 缩进: 基于字体大小的 2 个空格宽度

**图片布局**:
1. 尝试原始大小放入当前页
2. 如放不下，按宽度缩放后尝试
3. 仍放不下则换页
4. 新页面按页面尺寸比例缩放

**代码块分页**:
- 背景框跟随分页
- 每页单独绘制背景

### 3. Visual 模块 (`src/visual.rs`)

纯数据描述的视觉元素，与渲染后端解耦。

#### 3.1 元素类型

```rust
enum VisualElement {
    // 基础图形
    Rect { rect, style },
    Circle { center, radius, style },
    Line { start, end, style },
    Polyline { points, style },
    Path { path, style },
    
    // 渐变
    GradientPath { path, gradient, stroke },
    
    // 文本
    TextLine { runs, bounds, line_height },
    
    // 图片
    Image { position, size, pixel_size, data, format, alt },
    
    // 组合
    Group { children, transform },
    ZGroup { z_index, children },  // z-index 支持
}
```

#### 3.2 样式类型

- **`FillStrokeStyle`**: 填充 + 描边样式
- **`StrokeStyle`**: 纯描边样式
- **`GradientDef`**: 渐变定义（线性/径向）
- **`Transform`**: 2D 变换（平移、旋转、缩放）

### 4. Render 模块 (`src/render/`)

多后端渲染实现。

#### 4.1 PageRenderer Trait

统一渲染接口：

```rust
trait PageRenderer {
    fn draw_rect(&mut self, rect: Rect, style: &FillStrokeStyle);
    fn draw_circle(&mut self, center: Point, radius: f64, style: &FillStrokeStyle);
    fn draw_line(&mut self, start: Point, end: Point, style: &StrokeStyle);
    fn draw_path(&mut self, path: &BezPath, style: &FillStrokeStyle);
    fn draw_text_run(&mut self, run: &TextRun, position: Point);
    fn draw_image(&mut self, data: &[u8], format: &str, position: Point, size: (f64, f64));
}
```

#### 4.2 渲染后端

| 后端 | 实现 | 用途 |
|-----|------|------|
| PDF | krilla | 高质量文档输出 |
| SVG | 字符串拼接 | 矢量图形、Web 展示 |
| PNG | vello_cpu | 位图渲染、预览 |

#### 4.3 坐标系处理

- **SVG**: 左上角原点 (y 向下)
- **PDF**: 左下角原点 (y 向上)
- 转换: `y_pdf = page_height - y_svg`

### 5. Text 模块 (`src/text.rs`)

文本排版核心，基于 parley 库。

#### 5.1 核心类型

- **`TextLayout`**: 文本布局结果
- **`TextLine`**: 一行文本
- **`TextRun`**: 具有相同样式的文本片段
- **`GlyphInfo`**: 单个字形信息

#### 5.2 排版流程

```
文本内容 + TextStyle
    │
    ▼
parley 布局引擎
    │
    ▼
Layout (每行包含多个 GlyphRun)
    │
    ▼
转换为 TextLine (包含 TextRun 列表)
    │
    ▼
渲染时遍历 TextRun，逐个绘制字形
```

#### 5.3 线程安全

使用 thread_local 存储字体上下文：

```rust
thread_local! {
    static FONT_CONTEXT: RefCell<FontContext> = ...;
    static LAYOUT_CONTEXT: RefCell<LayoutContext<Brush>> = ...;
}
```

### 6. CLI 模块 (`src/bin/liepress.rs`)

命令行接口，基于 clap。

```bash
liepress --input input.md -o output.pdf
```

## 关键设计决策

### 1. 三层 AST 架构

**决策**: 将转换过程分为 MDAST → Styled AST → Layout AST 三层。

**理由**:
- 每层职责单一，易于维护
- 样式与结构分离，支持主题定制
- 布局与渲染分离，支持多后端

### 2. VisualElement 纯数据设计

**决策**: VisualElement 不包含任何渲染逻辑，仅描述"画什么"。

**理由**:
- 渲染后端可独立演进
- 易于测试（可序列化比较）
- 支持新后端时无需修改核心逻辑

### 3. 分页策略

**决策**: 以 TextLine 为单位分页，确保行不被截断。

**理由**:
- 避免文字被截断的可读性问题
- 代码块背景需要跨页连续绘制
- 图片作为整体处理

### 4. 字体回退机制

**决策**: 支持 CSS 风格的字体家族列表（如 `["SimSun", "serif"]`）。

**理由**:
- 跨平台兼容性
- 中文字体优先，西文字体回退
- 与 Web 字体机制一致

## 扩展指南

### 添加新的 NodeKind

1. 在 `ast/node.rs` 添加新的 NodeKind 变体
2. 在 `ast/presets.rs` 添加默认样式（如需要）
3. 在 `generator/mod.rs` 添加对应的布局函数
4. 更新文档

### 添加新的渲染后端

1. 实现 `PageRenderer` trait
2. 处理所有 VisualElement 类型
3. 注意坐标系转换
4. 添加测试

### 自定义样式

通过修改 `presets.rs` 中的样式函数，或在未来支持外部主题文件。

## 性能考虑

1. **字体缓存**: 使用 thread_local 缓存 FontContext
2. **布局缓存**: 文本布局结果可复用
3. **流式处理**: 支持大文档的分页处理
4. **图片优化**: 根据显示尺寸选择合适的 DPI

## 依赖说明

| 依赖 | 用途 |
|-----|------|
| markdown | MDAST 解析 |
| parley | 文本布局 |
| krilla | PDF 生成 |
| vello_cpu | 2D 图形渲染 |
| image | 图片解码 |
| clap | CLI 参数解析 |

## 未来方向

1. **表格支持**: 实现 Table NodeKind 和布局
2. **数学公式**: 支持 LaTeX 公式渲染
3. **主题系统**: 外部主题文件支持
4. **增量渲染**: 大文档的流式输出
5. **Web 服务**: HTTP API 接口
