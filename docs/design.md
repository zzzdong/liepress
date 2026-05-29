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
│         (mdast::Node - 符合 CommonMark + GFM 规范)              │
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

负责 Markdown 到带样式 AST 的转换。使用 `markdown` crate 以 GFM (GitHub Flavored Markdown) 模式解析。

#### 1.1 核心类型

- **`Node`** ([src/ast/node.rs](../src/ast/node.rs)): 带样式的 AST 节点
  - 包含 `NodeKind`（内容类型）和 `Style`（样式信息）
  - 支持嵌套结构（如列表项包含段落）
  - 包含 `splittable` 标志，控制是否允许跨页分割

- **`Style`** ([src/ast/style.rs](../src/ast/style.rs)): 样式定义
  - 字体家族（支持多字体回退列表）
  - 字体大小、粗细、样式
  - 颜色、行高、边距、对齐方式
  - 表格相关样式（边框、填充、表头/交替行背景）
  - 链接 URL

#### 1.2 节点类型 (NodeKind)

| 类型 | 说明 | 支持状态 |
|------|------|---------|
| `Document` | 文档根节点 | ✅ |
| `Paragraph` | 段落 | ✅ |
| `Heading { level }` | 标题 H1-H6 | ✅ |
| `List { ordered, start }` | 列表（有序/无序） | ✅ |
| `ListItem` | 列表项 | ✅ |
| `CodeBlock { lang, code }` | 代码块 | ✅ |
| `Blockquote` | 引用块 | ✅ |
| `ThematicBreak` | 分隔线 | ✅ |
| `Image { src, alt }` | 图片 | ✅ |
| `Table { align }` | 表格 | ✅ |
| `TableRow` | 表格行 | ✅ |
| `Text` | 纯文本（叶节点） | ✅ |
| `Strong` | 粗体 | ✅ |
| `Emphasis` | 斜体 | ✅ |
| `InlineCode` | 行内代码 | ✅ |
| `Link { url }` | 超链接 | ✅ |
| `Delete` | 删除线 | ⚠️ 已定义，未实现样式渲染 |

#### 1.3 样式属性 ([src/ast/style.rs](../src/ast/style.rs))

| 属性 | 类型 | 说明 | 支持状态 |
|------|------|------|---------|
| `font_family` | `Vec<String>` | 字体家族列表（优先级从高到低） | ✅ |
| `font_size_pt` | `f32` | 字号（pt） | ✅ |
| `font_weight` | `FontWeight` | 字重（Normal/Bold） | ✅ |
| `font_style` | `FontStyle` | 字体样式（Normal/Italic） | ✅ |
| `color` | `Color` | 文本颜色（RGBA） | ✅ |
| `line_height_pt` | `f32` | 行高（pt） | ✅ |
| `margin_top_pt` | `f32` | 上边距（pt） | ✅ |
| `margin_bottom_pt` | `f32` | 下边距（pt） | ✅ |
| `text_align` | `TextAlign` | 对齐方式（Left/Center/Right/Justify） | ✅（Justify 回退为 Left） |
| `display` | `Display` | 显示类型（Block/Inline/InlineBlock） | ✅ |
| `width` | `Option<f32>` | 显式宽度（pt） | ✅（图片使用） |
| `object_fit` | `ObjectFit` | 图片适应方式（Contain/Cover/Fill/None） | ✅ |
| `table_border_color` | `Color` | 表格边框颜色 | ✅ |
| `table_border_width_pt` | `f32` | 表格边框宽度（pt） | ✅ |
| `table_cell_padding_h_pt` | `f32` | 单元格水平内边距（pt） | ✅ |
| `table_cell_padding_v_pt` | `f32` | 单元格垂直内边距（pt） | ✅ |
| `table_header_bg` | `Option<Color>` | 表头背景色 | ✅ |
| `table_alt_row_bg` | `Option<Color>` | 交替行背景色 | ✅ |
| `link_url` | `Option<String>` | 链接 URL | ✅ |

#### 1.4 样式预设 ([src/ast/presets.rs](../src/ast/presets.rs))

| 预设函数 | 字体 | 字号 | 特殊样式 |
|---------|------|------|---------|
| `paragraph_style()` | serif | 10.5pt | 默认正文 |
| `heading_style(level)` | serif | 24~10.5pt | 逐级递减，粗体 |
| `code_style()` | monospace | 9pt | 深灰文字 #333 |
| `image_style()` | serif | 10.5pt | ObjectFit::Contain |
| `list_item_style()` | serif | 10.5pt | 较小下边距 4pt |
| `list_marker_style()` | serif | 10.5pt | 右对齐，Inline |
| `unordered_list_style()` | serif | 10.5pt | 列表容器 |
| `ordered_list_style()` | serif | 10.5pt | 列表容器 |
| `inline_code_style()` | monospace | 10.5pt | 深灰文字 #333，Inline |
| `link_style()` | serif | 10.5pt | 蓝色 #0000FF，Inline |
| `blockquote_style()` | serif | 10.5pt | 上下边距 12pt |
| `table_style()` | serif | 10.5pt | 表头浅灰背景 #F0F0F0，交替行 #F8F8F8 |
| `thematic_break_style()` | serif | 10.5pt | 大上下边距 18pt |

### 2. Generator 模块 (`src/generator/`)

将 Styled AST 转换为布局后的 VisualElement。这是布局引擎的核心。

#### 2.1 核心组件 ([src/generator/types.rs](../src/generator/types.rs))

- **`DocumentGenerator`**: 文档生成器主入口
- **`PageContext`**: 页面上下文，管理当前页状态
  - 剩余高度跟踪（`remaining_height()`）
  - 元素收集（`add_element()`）
  - 分页控制（`start_new_page()`）
- **`PageSettings`**: 页面尺寸和边距配置（默认 A4）
- **`Document`**: 最终文档结构，包含 `Vec<Page>` 和页面尺寸
- **`Page`**: 单页结构，包含 `Vec<VisualElement>`

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
    │       ├──► layout_paragraph_with_indent()
    │       ├──► layout_heading()
    │       ├──► layout_list()
    │       │       └──► layout_list_with_indent()
    │       │               └──► layout_list_item_first_child()
    │       ├──► layout_code()
    │       ├──► layout_image()
    │       ├──► layout_blockquote()
    │       ├──► layout_thematic_break()
    │       └──► layout_table()
    │
    ▼
Document { pages: Vec<Page> }
```

#### 2.3 关键布局算法

**段落布局** ([src/generator/mod.rs](../src/generator/mod.rs)):
1. 收集所有内联子节点（Text/Strong/Emphasis/InlineCode/Link）的文本段和样式
2. 使用 parley 的 RangedBuilder 拼接文本，各段应用不同样式
3. 断行后提取 `TextLineRel`（相对坐标的行数据）
4. 使用 `annotate_runs_with_urls` 将超链接 URL 回填到对应的 run 中
5. `place_text_lines` 负责将行放置到页面上，处理自动分页

**列表布局**:
- 无序列表: 固定标记区域 (10pt)
- 有序列表: 根据最大编号动态计算标记区域宽度（最小12pt，最大30pt）
- 缩进: 基于字体大小的 2 个空格宽度（`font_size * 1.2`，范围10~20pt）
- 标记右对齐，内容左对齐

**图片布局** ([src/generator/mod.rs](../src/generator/mod.rs)):
1. 尝试原始大小放入当前页
2. 如放不下，按宽度缩放后尝试
3. 仍放不下则换页
4. 新页面按页面尺寸比例缩放
5. 支持图片标题（使用 alt 文本，9pt 灰色居中）

**代码块分页** ([src/generator/mod.rs](../src/generator/mod.rs)):
- 浅灰背景 (#F5F5F5)
- 背景框跟随分页，每页单独绘制背景
- 左偏移 8pt，使用内容区全宽

**标题布局**:
- 不可跨页分割（splittable = false）
- 高度超过页面内容区时 panic 报错

**引用块布局**:
- 左侧背景色块标记（使用 Rect 元素）
- 内容在色块右侧显示

**分隔线布局**:
- 居中水平线，宽度为内容区 60%

#### 2.4 表格布局引擎 ([src/generator/table.rs](../src/generator/table.rs))

两阶段设计：
1. **`compute_layout_info()`**: 纯计算，返回列宽、行高等布局数据
   - 测量每个单元格的理想宽度和最小宽度
   - 启发式列宽分配
   - 基于换行的行高计算
2. **`generate_rows()`**: 按行区间生成视觉元素，支持跨页分割
   - 表头背景填充
   - 交替行背景填充
   - 单元格文本渲染
   - 表格边框绘制

#### 2.5 内联文本处理 ([src/generator/text.rs](../src/generator/text.rs))

核心函数：

- **`collect_inline_segments()`**: 递归收集内联子节点（Strong、Emphasis、Link、InlineCode、Text）的文本段和样式
- **`build_text_lines_rel()`**: 从 parley Layout 提取行列表，将字形坐标转换为相对行左上角的偏移量
- **`annotate_runs_with_urls()`**: 通过 run 的 `text_range` 匹配对应的 segment，将超链接 URL 回填到 TextRun 的 `url` 字段

关键数据结构：

- **`TextLineRel`**: 相对段落原点的行数据，包含 runs、min_x、width、line_height、row_top_rel
- **`GlyphRaw`**: 字形原始数据（相对 layout 原点的坐标）

### 3. Visual 模块 ([src/visual.rs](../src/visual.rs))

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
    ZGroup { z_index, children },
}
```

#### 3.2 样式类型

- **`FillStrokeStyle`**: 填充 + 描边样式
- **`StrokeStyle`**: 纯描边样式
- **`GradientDef`**: 渐变定义（线性/径向）
- **`Transform`**: 2D 变换（平移、旋转、缩放）
- **`Color`**: RGBA 颜色

### 4. Text 模块 ([src/text.rs](../src/text.rs))

文本排版核心，基于 parley 库。

#### 4.1 核心类型

- **`TextLayout`**: parley `Layout<Color>` 的包装类型
- **`TextLine`**: 一行文本，包含 `runs`、`bounds`、`line_height`
- **`TextRun`**: 具有相同样式的文本片段
  - `text`: 文本内容
  - `text_range`: 在段落中的字节范围
  - `font_data`: parley FontData（用于渲染）
  - `font_size`: 字号
  - `color`: 颜色
  - `glyphs`: 字形列表（坐标相对行顶偏移）
  - `baseline_x/y`: 基线位置
  - `url`: 超链接 URL（可选）
- **`Glyph`**: 单个字形信息（id, x, y, advance）
- **`TextStyle`**: 样式输入（color, font_family, font_size, font_weight, font_style, align, url）

#### 4.2 排版流程

```
文本内容 + TextStyle
    │
    ▼
parley RangedBuilder (多段样式拼接)
    │
    ▼
Layout.break_all_lines(max_width)
    │
    ▼
Layout.align(Alignment)
    │
    ▼
遍历 positioned_glyphs()，提取行和字形
    │
    ▼
转换为 TextLine (包含 TextRun 列表)
    │
    ▼
annotate_runs_with_urls() 回填 URL
    │
    ▼
渲染时遍历 TextRun，逐个绘制字形
```

#### 4.3 多段样式布局

`layout_text_with_contexts()` 支持将多段不同样式的文本合并为一个 Layout：
1. 拼接所有文本段
2. 以第一段样式为默认样式
3. 后续各段通过 `RangedBuilder.push()` 覆盖特定范围的样式属性
4. 支持字体家族、字号、颜色、字重、字体样式

#### 4.4 样式映射

| TextStyle 字段 | Parley StyleProperty | 说明 |
|---------------|---------------------|------|
| `font_family` | `FontFamily(List)` | 自动识别 CSS 通用家族关键字 |
| `font_size` | `FontSize` | pt 值 |
| `color` | `Brush` | RGBA |
| `font_weight` | `FontWeight` | 支持 full range (100-900) |
| `font_style` | `FontStyle` | Normal/Italic/Oblique |

#### 4.5 线程安全

使用 thread_local 存储字体上下文：

```rust
thread_local! {
    static FONT_CONTEXT: RefCell<FontContext>;
    static LAYOUT_CONTEXT: RefCell<LayoutContext<Color>>;
    static FONT_BYTES: RefCell<HashMap<String, Arc<Vec<u8>>>>;
}
```

提供便捷函数：`with_font_context()`, `with_layout_context()`, `with_text_contexts()`

#### 4.6 字体注册

- `register_font()`: 支持从文件路径或内存数据加载字体
- 自动缓存字体字节供 PDF 渲染器使用

### 5. 渲染模块 (`src/render/`)

多后端渲染实现。

#### 5.1 PageRenderer Trait

```rust
trait PageRenderer {
    fn draw_rect(&mut self, rect: Rect, style: &FillStrokeStyle);
    fn draw_circle(&mut self, center: Point, radius: f64, style: &FillStrokeStyle);
    fn draw_line(&mut self, start: Point, end: Point, style: &StrokeStyle);
    fn draw_polyline(&mut self, points: &[Point], style: &StrokeStyle);
    fn draw_path(&mut self, path: &BezPath, style: &FillStrokeStyle);
    fn draw_gradient_path(&mut self, path: &BezPath, gradient: &GradientDef, stroke: Option<&Stroke>);
    fn begin_group(&mut self, transform: Option<&Transform>);
    fn end_group(&mut self);
    fn draw_text_run(&mut self, run: &TextRun, position: Point);
    fn draw_image(&mut self, data: &[u8], format: &str, position: Point, size: (f64, f64));
}
```

#### 5.2 渲染后端

| 后端 | 实现 | 用途 | 状态 |
|-----|------|------|------|
| PDF | krilla | 高质量文档输出 | ✅ |
| SVG | 字符串拼接 | 矢量图形、Web 展示 | ✅ |
| PNG | vello_cpu | 位图渲染、预览 | ✅ |

#### 5.3 PDF 渲染器 ([src/render/pdf.rs](../src/render/pdf.rs))

- 使用 krilla 库生成 PDF
- **字体缓存**: 通过 `FontCacheKey`（data_ptr + data_len + index）去重字体
- **超链接注释**: 在绘制文本 run 时，收集有 URL 的 run 的位置和尺寸，渲染完成后统一添加 `LinkAnnotation`
- 坐标系: PDF 左下角原点 (y 向上)，布局系统使用左上角原点，渲染时翻转 y 坐标
- 支持图片格式: PNG、JPEG、GIF、WebP

#### 5.4 坐标系处理

- **布局引擎**: 左上角原点 (y 向下)
- **SVG**: 左上角原点 (y 向下)
- **PDF**: 左下角原点 (y 向上)
- 转换: `y_pdf = page_height - y_svg`

### 6. 超链接处理

#### 6.1 数据流

```
Markdown: [text](url)
    │
    ▼
NodeKind::Link { url, children: [Text("text")] }
    │  link_style() 设置 color=#0000FF, link_url=url
    │  子节点继承父节点的 link_url
    ▼
collect_inline_segments() → ("text", TextStyle { url: Some(url), ... })
    │
    ▼
layout_text_with_contexts() → parley 为不同样式创建独立 GlyphRun
    │
    ▼
build_text_lines_rel() → 提取 TextLineRel（含 text_range）
    │
    ▼
annotate_runs_with_urls() → 匹配 text_range 中点 → 回填 run.url
    │
    ▼
draw_text_run() → 收集有 url 的 run 的矩形区域
    │
    ▼
render_page() → 为每个区域创建 LinkAnnotation + Action(URI)
```

#### 6.2 断行超链接

当超链接文本被断行时，Parley 会为每行中属于该链接的部分创建独立的 GlyphRun，每个 run 保留正确的 `text_range`。`annotate_runs_with_urls` 通过 `text_range` 中点匹配，为每个断行 run 正确关联 URL。渲染时每行中的链接片段会生成独立的 LinkAnnotation，确保断行文本的每个部分都可点击。

### 7. CLI 模块 ([src/bin/liepress.rs](../src/bin/liepress.rs))

命令行接口，基于 clap。

```bash
liepress --input input.md -o output.pdf
liepress --input input.md -o output.svg
liepress --input input.md -o output.png
```

### 8. 常量定义 ([src/generator/constants.rs](../src/generator/constants.rs))

- 页面尺寸: A4 (595.276 × 841.890 pt)
- 默认边距: 上下 72pt，左右 90pt
- 默认 DPI: 72

## 特性支持状态

### Markdown 元素

| 特性 | 状态 | 说明 |
|------|------|------|
| 标题 (H1-H6) | ✅ | 逐级递减字号，粗体 |
| 段落 | ✅ | 自动换行、分页 |
| 粗体 | ✅ | 字体加粗 |
| 斜体 | ✅ | 字体倾斜 |
| 行内代码 | ✅ | 等宽字体 |
| 超链接 | ✅ | 蓝色文本，PDF 可点击注释 |
| 图片 | ✅ | 自适应缩放，支持标题 |
| 无序列表 | ✅ | 支持嵌套 |
| 有序列表 | ✅ | 自动编号，支持嵌套 |
| 代码块 | ✅ | 灰色背景，支持分页 |
| 引用块 | ✅ | 左侧色块标记 |
| 分隔线 | ✅ | 居中水平线 |
| 表格 | ✅ | 列宽自适应，跨页分割 |
| 删除线 | ⚠️ | NodeKind 已定义，样式未渲染 |
| 任务列表 | ❌ | 未实现 |
| 脚注 | ❌ | 未实现 |
| 定义列表 | ❌ | 未实现 |
| 数学公式 | ❌ | 未实现 |
| Emoji | ✅ | 依赖系统字体支持 |

### 样式属性

| 属性 | 状态 | 说明 |
|------|------|------|
| 字体家族 | ✅ | 支持 fallback 列表，CSS 通用家族关键字 |
| 字号 | ✅ | pt 单位 |
| 字重 | ✅ | Normal/Bold + 完整 100-900 |
| 字体样式 | ✅ | Normal/Italic/Oblique |
| 颜色 | ✅ | RGBA |
| 行高 | ✅ | pt 单位 |
| 边距 | ✅ | 上下边距 |
| 文本对齐 | ✅ | 左/中/右（两端对齐回退为左对齐） |
| 显示类型 | ✅ | Block/Inline/InlineBlock |
| 表格边框 | ✅ | 颜色、宽度 |
| 单元格填充 | ✅ | 水平/垂直 |
| 表头背景 | ✅ | 可配置颜色 |
| 交替行背景 | ✅ | 可配置颜色 |
| 图片适应 | ✅ | Contain/Cover/Fill/None |
| 删除线样式 | ❌ | 未实现 |

### 布局功能

| 功能 | 状态 | 说明 |
|------|------|------|
| A4 页面 | ✅ | 默认 |
| 自定义页面尺寸 | ✅ | 通过 PageSettings |
| 自定义边距 | ✅ | 通过 PageSettings |
| 自动分页 | ✅ | 行级分页，确保行完整 |
| 列表缩进 | ✅ | 基于字体大小动态计算 |
| 有序列表起始编号 | ✅ | 支持 start 属性 |
| 图片缩放 | ✅ | 多策略：原始/适配宽度/适配页面 |
| 图片标题 | ✅ | 使用 alt 文本，灰色小字 |
| 代码块背景 | ✅ | 浅灰色，跟随分页 |
| 表格跨页 | ✅ | 按行区间分割 |
| 表头样式 | ✅ | 背景色 |
| 交替行样式 | ✅ | 背景色 |
| 表格对齐 | ✅ | 列级左/中/右对齐 |
| 超链接注释 | ✅ | PDF 可点击 |
| 断行超链接 | ✅ | 多段注释，各段独立可点击 |
| 字体回退 | ✅ | CSS 风格字体列表 |
| Unicode 支持 | ✅ | 中文、日文、韩文、特殊符号 |
| 分页符控制 | ⚠️ | 仅标题不可分割，无显式分页符 |

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

### 5. 超链接 URL 回填

**决策**: 文本布局完成后，通过 `text_range` 中点匹配方式将 URL 回填到 TextRun。

**理由**:
- parley 的 Run 的 `text_range` 在布局后保持正确
- 中点匹配可靠处理断行场景
- 避免依赖易变的颜色匹配

### 6. 行级相对坐标系

**决策**: TextRun 中的字形坐标使用相对行左上角的偏移量，行的绝对位置由 TextLine.bounds 决定。

**理由**:
- 渲染时只需计算一次行位置
- 字形坐标在线性变换后无需调整
- 分页时只需更新 bounds

## 扩展指南

### 添加新的 NodeKind

1. 在 [ast/node.rs](../src/ast/node.rs) 添加新的 NodeKind 变体
2. 在 [ast/presets.rs](../src/ast/presets.rs) 添加默认样式（如需要）
3. 在 [generator/mod.rs](../src/generator/mod.rs) 的 `layout_node()` 添加对应的布局分支
4. 添加测试
5. 更新此文檔

### 添加新的渲染后端

1. 实现 `PageRenderer` trait
2. 处理所有 VisualElement 类型
3. 注意坐标系转换
4. 添加测试

### 自定义样式

通过修改 `presets.rs` 中的样式函数，或在未来支持外部主题文件。

## 测试策略

### 测试分层

| 层级 | 位置 | 说明 |
|------|------|------|
| 单元测试 | src/ 中各模块 | 测试内部函数 |
| 集成测试 | tests/integration/ | 端到端管道测试 PDF/SVG/PNG |
| PDF 验证测试 | tests/integration/pdf_validation.rs | 使用 lopdf 解析和验证 PDF |
| 渲染测试 | tests/render/pdf.rs | 验证 PDF 基本结构 |

### PDF 验证工具 ([tests/common/mod.rs](../tests/common/mod.rs))

- `load_pdf()`: 加载 PDF 数据
- `assert_valid_pdf()`: 验证 header + 至少一页
- `extract_links()`: 提取所有链接注释
- `assert_has_link()`: 验证指定 URL 存在
- `assert_link_count()`: 验证链接数量
- `validate_pdf_structure()`: 完整结构验证（页数、每页链接）
- `group_links_by_url()`: 按 URL 分组链接矩形（用于断行验证）

## 性能考虑

1. **字体缓存**: 使用 thread_local 缓存 FontContext 和 LayoutContext
2. **字体字节缓存**: 缓存已注册字体的原始字节，供 PDF 渲染器使用
3. **流式处理**: 支持大文档的分页处理
4. **图片优化**: 根据显示尺寸选择合适的 DPI (150 DPI)

## 依赖说明

| 依赖 | 用途 | 版本 |
|-----|------|------|
| markdown | MDAST 解析 (GFM) | 最新 |
| parley | 文本布局引擎 | 0.9 |
| krilla | PDF 生成 | 0.7 |
| vello_cpu | 2D 图形渲染 | 最新 |
| image | 图片解码 | 最新 |
| clap | CLI 参数解析 | 最新 |
| lopdf | 测试中 PDF 解析和验证 | 最新 |

## 未来方向

### 短期

1. **删除线渲染**: 实现 Delete NodeKind 的样式渲染（text-decoration: line-through）
2. **两端对齐**: 实现 TextAlign::Justify 的完整支持
3. **行内代码高亮**: 为 InlineCode 添加背景色和边框
4. **代码语法高亮**: 基于代码块 lang 实现语法着色
5. **显式分页符**: 支持 `\pagebreak` 或 `---` 分页控制

### 中期

6. **主题系统**: 外部主题文件（YAML/TOML）支持
7. **自定义字体**: 改进字体注册 API，支持按文档配置
8. **页眉页脚**: 支持页码、标题、日期等
9. **目录生成**: 自动生成 Table of Contents
10. **图片对齐**: 支持 float、居中、环绕等布局

### 长期

11. **数学公式**: 支持 LaTeX 公式渲染（KaTeX/MathJax）
12. **Web 服务**: HTTP API 接口
13. **增量渲染**: 大文档的流式输出
14. **任务列表**: 支持 GFM 任务列表（checkbox）
15. **脚注**: 支持脚注引用和渲染