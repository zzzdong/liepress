# LiePress 设计文档

## 项目概述

LiePress 是一个 Rust 实现的 Markdown / HTML 到 PDF 的文档生成器，支持 CSS 样式定制。输入经统一 HTML AST（DOM）后，按「双路线」输出：

- **PDF 精确布局路线**：`HtmlDocument → ast::Node（Styled）→ document::Document（不分页源 IR）→ output::pdf 内部 paginate + draw`。
- **HTML 流式路线**：`HtmlDocument → ast::Node（Styled）→ output::html 序列化`（直接消费 `ast::Node`，文本断行交由浏览器）。

## 核心设计原则

1. **HTML AST 作为输入汇合点**：Markdown（pulldown-cmark 事件流直连）与纯 HTML（html5ever）都归一为同一棵 `HtmlDocument`，后续管线不区分来源。
2. **双中间层（双路线）**：
   - `ast::Node`（Styled AST）是**语义真源**：已套 CSS、与输出格式无关，流式输出（HTML）与精确布局（PDF）共享。
   - `document::Document` 是**精确布局中间层**：仅服务需要精确坐标/断行的后端（当前为 PDF），不是通用格式无关 IR。
3. **Document 不知道页**：`Document` 只承载**不分页**的块树；分页（切页、跨页表格）是各输出后端的职责。
4. **单一样式真源**：样式统一由 `CssEngine`（基于 Lightning CSS）从内置 CSS + 用户 CSS + `<style>` 标签解析，输出到 `ast::Node`，各后端消费同一份样式。
5. **纯数据描述**：渲染中间结构（`Document` / `TextLayout`）只描述「画什么」，不含渲染逻辑，便于多后端复用与测试。

## 系统架构

```
┌────────────────────────────────────────────────────────────────┐
│                      输入层                                    │
│   Markdown（pulldown-cmark 事件流）   HTML 字符串（html5ever）   │
└────────────────────────────────────────────────────────────────┘
                          │  汇合
                          ▼
┌────────────────────────────────────────────────────────────────┐
│                 Layer 1: HtmlDocument（dom 模块）                │
│           HTML AST（HtmlElement / HtmlNode / HtmlTag）          │
│           + 从 <style> 提取的 style_sheets                      │
└────────────────────────────────────────────────────────────────┘
                          │  CssEngine（Lightning CSS）+ 套样式
                          ▼
┌────────────────────────────────────────────────────────────────┐
│                 Layer 2: Styled AST（ast 模块）                  │
│                Node + NodeKind + Style（语义真源）               │
└────────────────────────────────────────────────────────────────┘
                          │
          ┌───────────────┴────────────────┐
          ▼                                ▼
┌────────────────────────┐   ┌──────────────────────────────┐
│  双路线 A：精确布局       │   │  双路线 B：流式输出            │
│  document::Document     │   │  output::html::node_to_html  │
│  （不分页源 IR）          │   │  （直接消费 ast::Node）        │
└────────────┬───────────┘   └──────────────────────────────┘
             ▼
┌────────────────────────┐
│  output::pdf            │
│  from_layout →          │
│  paginate_layout →      │
│  PdfRenderer.draw       │
│  （krilla → PDF 字节）    │
└────────────────────────┘
```

## 模块详解

### 1. DOM 模块（`src/dom/`）— 输入层 Layer 1

将 Markdown / HTML 统一为 `HtmlDocument`。

| 文件 | 职责 |
|------|------|
| `mod.rs` | `HtmlDocument` / `HtmlElement` / `HtmlNode` / `HtmlTag` 定义、序列化、查询、空白折叠 |
| `parser.rs` | 纯 HTML 字符串 → `HtmlDocument`（html5ever + 自定义 TreeSink） |
| `markdown.rs` | pulldown-cmark `Event` 流**直连** `HtmlDocument`，不经过字符串往返；`inline_local_images` 内联本地图片为 data URI |
| `md_converter.rs` | 兼容入口：Markdown 源 → HTML 字符串 / `HtmlDocument`（含 `markdown_to_html`、`embed_local_images`） |
| `style_resolver.rs` | 从 `HtmlDocument` 解析 / 合并样式表 |
| `to_ast.rs` | `HtmlDocument` + `CssEngine` → 带样式的 `ast::Node`（`html_to_styled_nodes`） |

**关键点**：Markdown 输入不先序列化为 HTML 字符串再二次解析，而是把 pulldown-cmark 的 `Event` 流直接映射为 HTML AST 节点，既消除无意义的往返，又提升文本保真度。

### 2. AST 模块（`src/ast/`）— 语义真源 Layer 2

#### 2.1 核心类型

- **`Node`**（`node.rs`）：带样式的 AST 节点，含 `NodeKind`（内容类型）、`Style`、`splittable`（是否允许跨页分割）。
- **`NodeKind`**：`Document` / `Paragraph` / `Heading` / `List` / `ListItem` / `TaskListItem` / `CodeBlock` / `Blockquote` / `ThematicBreak` / `Image` / `Table` / `TableRow` / `TableCell` / `Text` / `Strong` / `Emphasis` / `InlineCode` / `Link` / `Delete` / `Sub` / `Sup` / `LineBreak` / `Container`。
- **`Style`**（`style.rs`）：字体、字号、颜色、行高、对齐、边距、宽高、`object_fit`、表格样式、链接 URL 等。`Style::default()` 与 `Style::inherit_from()` 的 `object_fit` 均默认为 `Contain`（图片默认不拉伸）。
- **`presets.rs`**：内置默认 CSS（`DEFAULT_CSS`）。

#### 2.2 内置样式表（`presets.rs`）

内置默认 CSS 覆盖 `body` / `h1`–`h6` / `p` / `ul,ol` / `li` / `pre` / `blockquote` / `hr` / `table` / `th` / `tr:nth-child(even)` / `code` / `a` / `strong` / `em` / `span`，提供字体、颜色、间距、表格样式等基础排版。

### 3. CSS 模块（`src/css/`）

基于 **Lightning CSS** 实现浏览器级 CSS 解析与匹配，替代旧的手写解析器。

- **`CssEngine`**：合并内置 CSS + 用户 CSS + 内联 `<style>`；解析 `@page` 规则到 `PageConfig`；支持严格 / 非严格模式。
- 样式解析统一在 `dom::to_ast::html_to_styled_nodes` 中完成，将 `Style` 落到每个 `ast::Node`。

### 4. 文档层（`src/document/`）— 精确布局中间层

| 文件 | 职责 |
|------|------|
| `mod.rs` | 文档层模块根，声明 `types` / `text` / `layout` / `from_ast` 子模块，说明跨层单向依赖与双路线定位 |
| `types/` | 文档逻辑类型投影（`ResolvedStyle` / `DocImage` / `PageSettings` / `ObjectFit` / `TextAlign`），避免上层直接依赖渲染类型；含页面常量 |
| `text.rs` | 文本排版引擎 + 文本类型（`TextLayout` / `TextLine` / `TextRun` / `Glyph` / `TextStyle`），基于 parley |
| `layout/` | 文档中间表示：`Document`（不分页源 IR）+ `Block` / `BlockKind` / `TableRow` / `TableCell` / `HeaderFooter` |
| `from_ast.rs` | `ast_to_layout`：从 `ast::Node` 构建 `Document`（源 IR，不分页） |

#### 4.1 `layout::Document`（源 IR）

```rust
pub struct Document { pub blocks: Vec<Block> }
```

- **不分页**的块树，每个 `Block` 携带 `BlockKind` + `ResolvedStyle` + `splittable`。
- 分页（切页、跨页表格）由输出后端（`output::pdf::paginate_layout`）完成。
- **定位说明**：`Document` 是 PDF 精确布局层，`Paragraph` 直接内嵌 parley 断行的 `TextLine`（含字形坐标与字体字节）。HTML / DOCX 等流式输出直接消费 `ast::Node`，**不消费 `Document`**。

#### 4.2 `from_ast::ast_to_layout`

从 `ast::Node` 递归构建 `Document`：

- 文本行（`TextLine`）复用文档排版模块的 `layout_text_with_contexts` 完成断行。
- 行内语义节点（Strong / Emphasis / Delete / Sub / Super）不生成独立块，由 `TextRun` 样式承载。
- 列表项 marker（有序序号 / 无序圆点 / 任务复选框）在 `List` 边界注入。
- 图片：若 `src` 为 data URI，解码字节并探测原始像素尺寸，按宽高比解析显示尺寸（未指定时「适合页宽」）；纯图片段落提升为独立图片块并默认居中。

#### 4.3 文本排版（`text.rs`）

基于 parley，职责：接收文本 + 样式，返回已断行的 `TextLayout`。分页、绝对定位由输出后端负责。

**线程安全**：使用 `thread_local` 存储字体与布局上下文（`FONT_CONTEXT` / `LAYOUT_CONTEXT`），并用 `Arc<Vec<u8>>` 缓存字体字节去重。

### 5. 输出层（`src/output/`）

| 文件 | 职责 |
|------|------|
| `mod.rs` | 输出层模块根 |
| `common.rs` | 各后端共享的块测量（`block_height` 等）与样式工具 |
| `pdf.rs` | PDF 输出后端：消费 `Document`，内部 `paginate_layout` 分页 + `PdfRenderer` 用 krilla 绘制 |
| `html.rs` | `ast::Node` → 自包含 HTML 序列化（流式路线） |
| `svg.rs` | SVG 输出后端：消费 `Document`，输出不分页长图；文本用 `<text>`（系统字体），行内代码用 `<rect>` 画背景 |
| `png.rs` | PNG 输出后端：消费 `Document`，用 `vello_cpu` 光栅化为不分页长图；文本用真实字形 |
| `docx.rs` | DOCX 输出后端：消费 `ast::Node`（保留语义），用 `docx-rs` 生成可编辑 Word 文档 |

#### 5.1 PDF 输出后端（`pdf.rs`）

1. **`PdfDocumentGenerator`**：`from_layout(Document, PageSettings)` → `generate()` 输出 PDF 字节。
2. **`paginate_layout`**：把不分页的 `Document` 切分为多页绝对定位块（`PdfPage` / `PositionedBlock`），跨页表格重复表头。
3. **`PdfRenderer`**：在单个 krilla Surface 上绘制 `Block`；字体缓存（`FontCacheKey` 去重）；图片按 `object_fit`（Contain / Cover / Fill）与 `text_align`（居中）计算绘制区域；超链接收集为 `LinkAnnotation`。
4. **内部链接跳转**：`generate` 先扫描 `paginate_layout` 结果收集脚注定义位置（`id → 页索引/坐标`），正文脚注引用生成 `Target::Destination(XyzDestination)` 实现 PDF 内点击跳转；外部 URL 用 `Target::Action(LinkAction)`。两者皆由 krilla 支持。
5. 支持图片格式：PNG / JPEG / GIF / WebP。

#### 5.2 HTML 输出（`html.rs`）

`ast::Node` 语义树 → HTML 序列化，文本断行交给浏览器（流式 / 响应式）。与 PDF 共享同一棵 `ast::Node`，保证语义一致性。

#### 5.3 SVG 输出（`svg.rs`，不分页长图）

消费 `Document`（已布局块树），计算总高度作为长图画布，输出单张 SVG。文本用 `<text>` 元素（依赖阅读器系统字体）；图形用 `<rect>`/`<line>`；图片用 `<image href="data:...">`；行内代码检测 `TextRun.background_color` 先画 `<rect>` 背景。

**限制**：因 SVG 文本用系统字体渲染而 `Document` 坐标来自 parley 嵌入字体，多 run 并排时可能轻微错位；未采用 `<foreignObject>` 内嵌 HTML（保持 SVG 独立性与可移植性）。

#### 5.4 PNG 输出（`png.rs`，不分页长图）

消费 `Document`，用 `vello_cpu` 光栅化为长图 PNG。文本用 `glyph_run(&resources, &font_data)` 绘制**真实字形**（从 `TextRun.font_data` 加载字体），与 PDF 视觉一致；支持 DPI 参数（默认 96）。图形用 `fill_rect`/`stroke_path`；表格背景/边框/隔行色、代码块背景、引用块竖条、行内代码背景框均支持。

#### 5.5 DOCX 输出（`docx.rs`）

消费 `ast::Node`（Styled AST，保留语义），用 `docx-rs` 生成可编辑 Word 文档。**不消费 `Document`**（其 `Paragraph` 绑定 parley 字形坐标），而是从 `ast::Node` 重建。支持标题（Heading1-6）、段落、加粗/斜体、行内代码（等宽字体）、有序/无序/任务列表、代码块、表格、定义列表、分隔线。用 `Docx::pack` 打包为完整 .docx zip。

### 6. 其他模块

- `color.rs`：颜色类型（RGBA）。旧像素层 `visual::Color` 已删除，此处为重新投影。
- `error.rs`：统一错误类型（`VisualElementError` / `FontLoadError` / `LayoutError` / `CssParseError` / `HtmlError` / `RenderError` / `IoError`）。
- `lib.rs`：库 API 入口（`markdown_to_pdf` / `html_to_pdf` 等）与 `ConvertOptions`。
- `bin/liepress.rs`：基于 clap 的 CLI。

## 公开 API（`src/lib.rs`）

| 函数 | 说明 |
|------|------|
| `markdown_to_pdf(md, opts)` | Markdown 字符串 → PDF 字节 |
| `markdown_file_to_pdf(path, opts)` | Markdown 文件 → PDF 字节（自动内联本地图片） |
| `html_to_pdf(html, opts)` | HTML 字符串 → PDF 字节 |
| `html_file_to_pdf(path, opts)` | HTML 文件 → PDF 字节（自动内联本地图片） |
| `node_to_html(node)` | `ast::Node` → HTML 片段（流式路线） |
| `markdown_to_html(md)` | Markdown → HTML 字符串（兼容降级入口） |
| `markdown_to_svg(md, opts)` / `html_to_svg(html, opts)` | Markdown / HTML → SVG 长图 |
| `markdown_to_png(md, opts)` / `html_to_png(html, opts)` | Markdown / HTML → PNG 长图（96 DPI） |
| `markdown_to_png_dpi(md, opts, dpi)` | Markdown → PNG（自定义 DPI） |
| `markdown_to_docx(md, opts)` / `html_to_docx(html, opts)` | Markdown / HTML → DOCX（可编辑） |
| `markdown_file_to_docx(path, opts)` / `html_file_to_docx(path, opts)` | Markdown / HTML 文件 → DOCX（自动内联本地图片） |

### ConvertOptions

支持 builder 风格链式调用：`with_font_family` / `with_css` / `with_css_file` / `with_strict` / `with_auto_font` / `with_page_config` / `with_header` / `with_footer` / `with_header_font_size` / `with_footer_font_size` / `with_height_unlimited`。

### 页面设置（@page）

通过 `@page` at-rule 或 `PageSettings` 配置页面尺寸、边距、页眉页脚：

- 尺寸：`size: A4` / `Letter` / 宽高。
- 边距：`margin-*` 与 `margin` 简写。
- 页眉页脚：`header` / `footer`，支持 `{page}` / `{total}` 模板变量，`header-font-size` / `footer-font-size`（默认 9pt）。

**优先级**（从高到低）：`PageSettings` 传入值 → `ConvertOptions.page_config` → CSS `@page` 规则 → 内置默认。

## 关键设计决策

### 1. 双中间层（双路线）

**决策**：PDF 与 HTML 走两条独立路线，不共用 `Document`。

**理由**：
- `Document` 的 `Paragraph` 已内嵌 parley 断行的字形坐标，是 PDF 精确布局所需。
- HTML 是流式 / 响应式媒介，文本断行应交给浏览器；若 HTML 也走 `Document`，会把固定宽度断行**固化**进 HTML，破坏响应式。
- 两者共享 `ast::Node` 这层语义真源（已套 CSS），而非 `Document`。

### 2. Document 不知道页

**决策**：`Document` 只承载不分页的块树；分页是各输出后端的职责。

**理由**：分页算法强耦合后端度量与坐标（如 PDF 的跨页表格 MultiSpill），抽象成通用层反而徒增复杂度。PDF 后端 `paginate_layout` 自管分页。

### 3. 图片尺寸与居中

**决策**：图片未指定宽高时「适合页宽」（宽度 = 内容宽度，高度按原始宽高比），且默认 `object_fit = Contain`（不拉伸）；Markdown 纯图片段落默认居中。

**理由**：避免固定 `100×100` 造成的失真；`object_fit = Contain` 在 box 内保持比例完整放入。

### 4. CSS 继承机制

**决策**：`body` 作为根元素，通过 `Style::inherit_from()` 实现 CSS 标准继承。

**理由**：在 `<style>body { font-family }</style>` 配置一次，所有元素自动继承；与 Web CSS 行为一致；显式规则（如 `pre { font-family: monospace }`）自然覆盖继承值。

### 5. 超链接 URL 回填

**决策**：文本布局完成后，通过 `text_range` 中点匹配方式将 URL 回填到 `TextRun`。

**理由**：parley 的 `text_range` 在布局后保持正确；中点匹配可靠处理断行场景。

### 6. 行级相对坐标系

**决策**：`TextRun` 中字形坐标相对行左上角偏移，行绝对位置由 `TextLine.bounds` 决定。

**理由**：渲染时只需计算一次行位置；分页时只需更新 bounds。

## 特性支持状态

### Markdown 元素

| 特性 | 状态 |
|------|------|
| 标题 / 段落 / 粗体 / 斜体 / 行内代码 / 超链接 | ✅ |
| 图片（适合页宽、居中、object_fit） | ✅ |
| 无序 / 有序列表（嵌套、起始编号） | ✅ |
| 任务列表（GFM checkbox） | ✅ |
| 代码块（灰色背景、分页） | ✅ |
| 引用块（左侧色块） | ✅ |
| 分隔线 | ✅ |
| 表格（列宽自适应、跨页分割、表头重复） | ✅ |
| CSS 自定义样式（内置 + 用户 + `<style>`） | ✅ |
| 自动字体检测（中 / 日 / 韩 / 拉丁） | ✅ |
| 删除线 | ✅ |
| 定义列表（术语加粗、定义缩进） | ✅ |
| 脚注（上标引用 + 末尾聚合定义区 + PDF 内部点击跳转） | ✅ |
| 数学公式 | ❌ |

### 布局功能

| 功能 | 状态 |
|------|------|
| A4 页面 / 自定义尺寸 / 边距 | ✅ |
| 页眉页脚（`{page}` / `{total}` 模板） | ✅ |
| 自动分页（行级，确保行完整） | ✅ |
| 跨页表格 + 重复表头 | ✅ |
| 图片缩放（适合页宽、宽高比保持） | ✅ |
| 图片居中 | ✅ |
| 超链接注释（PDF 可点击，断行多段） | ✅ |
| 无限高度模式（`with_height_unlimited`） | ✅ |

### 输出格式

| 格式 | 状态 | 说明 |
|------|------|------|
| PDF | ✅ | 分页、目录（outline）、内部链接跳转、跨页表格 |
| HTML | ✅ | 流式 / 响应式（浏览器重排） |
| SVG | ✅ | 不分页长图；文本用系统字体 `<text>`，行内代码 `<rect>` 背景 |
| PNG | ✅ | 不分页长图；真实字形光栅化（`vello_cpu`） |
| DOCX | ✅ | 可编辑 Word；消费 `ast::Node` 保留语义 |

## 扩展指南

### 添加新的 NodeKind

1. 在 `ast/node.rs` 添加 `NodeKind` 变体。
2. 在 `ast/presets.rs` 添加默认样式（如需要）。
3. 在 `dom/to_ast.rs` 处理该节点到 Styled AST 的映射。
4. 在 `document/from_ast.rs` 的 `convert_node` 添加对应 `BlockKind` 分支（PDF 路线）。
5. 在 `output/pdf.rs` 的 `draw_block` 添加绘制分支。
6. 添加测试。

### 添加新的精确布局输出后端（如 DOCX）

DOCX 等需要精确布局 / 原生结构的后端，可复用 `document::Document`：

1. 消费 `Document`（块树 + `TextLine`），自管分页。
2. 图片字节从 `DocImage.data`（data URI 解码结果）获取。
3. 注意：`Document` 的 `Paragraph` 已绑定 parley 字形坐标，若 DOCX 需要**重新断行**（而非 PDF 固定宽度），需从 `ast::Node` 重建文本，而非复用 `TextLine`。

### 添加流式输出后端

流式 / 响应式输出（如 HTML 扩展）直接消费 `ast::Node`，不经过 `Document`，文本断行交由目标媒介。

## 测试策略

| 层级 | 位置 | 说明 |
|------|------|------|
| 单元测试 | `src/` 各模块内 `#[cfg(test)]` | 内部函数测试 |
| 端到端测试 | `tests/e2e/pipeline.rs` | Markdown → PDF / HTML 全链路 |
| PDF 验证 | `tests/common/` | `assert_valid_pdf` / `pdf_page_count` / 链接提取验证（lopdf） |

## 依赖说明

| 依赖 | 用途 | 版本 |
|------|------|------|
| pulldown-cmark | Markdown 解析（GFM） | 0.13 |
| html5ever | HTML 解析 | 0.39 |
| lightningcss | CSS 解析与匹配 | 1.0 |
| parley | 文本布局引擎 | 0.11 |
| krilla | PDF 生成 | 0.8 |
| image | 图片解码（尺寸探测 / 渲染） | 0.25 |
| base64 | data URI 解码 | 0.23 |
| clap | CLI 参数解析 | 4 |
| thiserror | 错误类型派生 | 2 |
| docx-rs | DOCX 生成（预留） | 0.4 |
| lopdf | 测试中 PDF 解析验证 | 0.44 |

## 未来方向

### 短期

1. 表格列宽 / 行高的真实度量（当前行高取 `line_height_pt`）。
2. 显式分页符（`page-break`）完善。
3. 代码语法高亮。
4. 超大单元格 / 表格分页的 break-guard 完善。

### 中期

1. DOCX 后端（复用 `Document` 或从 `ast::Node` 重建）。
2. 主题系统（外部 YAML / TOML）。
3. 目录生成。
4. 图片对齐（float / 环绕）。

### 长期

1. 数学公式（KaTeX / MathJax）。
2. Web 服务（HTTP API）。
3. 增量渲染（大文档流式输出）。
