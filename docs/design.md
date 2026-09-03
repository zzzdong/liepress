# Liepress 设计说明

> 本文档描述 Liepress 当前（main 分支）的实际架构、模块职责与设计原则，作为开发、扩展与排错的权威参考。
> 所有内容以源码为准；如与代码不一致，以代码为最终事实。

---

## 1. 概述

Liepress 是一个 Rust 实现、**支持 CSS 样式**的文档生成器：将 Markdown / HTML 转换为 PDF / HTML / SVG / PNG / DOCX 五种产物。

### 1.1 支持的能力

- **输入**：Markdown（通过 `pulldown_cmark`）、HTML（通过 `html5ever`）。
- **输出**：PDF、HTML、SVG、PNG、DOCX。
- **CSS 样式**：内置样式表 + 外部 CSS 文件 + Markdown 内联 `<style>`；支持选择器、简写展开、`!important`、百分比/字体相对单位、命名/十六进制/rgb/rgba 颜色。
- **GFM**：表格、任务列表、删除线、带语言标签的代码块。
- **图表与图示**（默认开启，可经 `--no-default-features` 关闭）：`mermaid` 代码块（经 `liemermaid`）渲染为图；`liecharts` 代码块（ECharts 风格 JSON，经 `liecharts`）渲染为图表。
- **自动字体检测**：按文档语言（中文 / 日文 / 韩文 / 拉丁）推荐字体并回退。
- **页面布局**：默认 A4，可通过 `@page` 与 CLI 配置页面尺寸与边距；自动分页，含孤行/寡行（widow/orphan）控制。
- **页眉页脚**：可配置，支持 `{page}` / `{total}` 模板变量。
- **超链接**：PDF 中可点击，跨换行连续。
- **表格**：列宽自适应、跨页拆分、表头/隔行样式。
- **图片**：自动缩放，支持 PNG/JPEG/GIF/WebP（本地或远程，远程需显式开启）。
- **代码块**：等宽字体、语法高亮、跨页。

### 1.2 设计核心原则

1. **Document 不知道页（Document is page-agnostic）**
   文档中间表示 `Document` 是一棵**不分页**的块树，每个节点携带已解析样式。分页是**各输出后端**的职责，不在文档层进行。这一原则让 PDF / SVG / PNG / HTML / DOCX 共用同一份 `Document`，各自决定如何切页。
2. **三层 IR 流水线**
   输入文本 → DOM（结构）→ 简化 AST（带样式节点）→ Document/Block（排版盒）→ 输出后端。每一层只关心自己那一级的语义，互不越层。
3. **样式与结构分离**
   CSS 引擎独立解析样式表并级联，生成 `ResolvedStyle` 附着在节点上；排版逻辑只读 `ResolvedStyle`，不解析 CSS。
4. **输出后端可插拔**
   每个后端直接消费 `Document`，互不共享分页状态。SVG/PNG 进一步共用 `to_scene` 把 `Document` 投影成 `lievisual::Scene`。

---

## 2. 总体架构

```
  输入 (.md / .html)
        │
        ▼
  ┌──────────────────────── dom ────────────────────────┐
  │  markdown.rs / parser.rs(html5ever) / md_converter.rs  │
  │  → HtmlElement 树（DOM）                                │
  │  resource.rs: 图片解析/加载/远程开关                    │
  │  to_ast.rs:   html_to_styled_nodes → 简化 AST          │
  └───────────────────────────┬───────────────────────────┘
                               │  HtmlElement 树 + 选择器
                               ▼
                  ┌─────────── css ────────────┐
                  │  engine.rs: 解析/级联/简写  │
                  │  presets.rs: 内置样式表       │
                  │  → ResolvedStyle 注入节点    │
                  └───────────────┬─────────────┘
                               │  带样式节点
                               ▼
                    ┌───────── ast ──────────┐
                    │  node.rs: Node/NodeKind │
                    │  style.rs: Style        │
                    │  Layer 2：简化带样式 AST │
                    └────────────┬─────────────┘
                               │  Node 树（ast::Node）
                               │
          ┌────────────────────┴─────────────────────┐
          │                                          │
          ▼ 路径 A（经 document 层，分页/投影后端）    │ 路径 B（直接消费 ast::Node）
  ┌──────────── document ────────────┐                │
  │ from_ast.rs: ast_to_layout         │                │
  │ layout/mod.rs: Document/Block      │                │
  │ text.rs: 文本排版/换行              │                │
  │ types/: ResolvedStyle/PageSettings │                │
  │ highlight.rs: 语法高亮             │                │
  └──────────────┬─────────────────────┘                │
                 │  Document（不分页块树）               │
        ┌────────┼────────┐                             │
        ▼        ▼        ▼                             ▼
   output/pdf  output/svg  output/png            output/html / output/docx
   (krilla)  (to_scene→  (to_scene→            （node_to_html / docx-rs，
             lievisual)   lievisual)            不经 document 层）
```

> 说明：
> 路径 A（PDF/SVG/PNG）经 `document` 层生成不分页的 `Document`，再交由后端消费（PDF后端自己做分页；SVG/PNG 不分页）。
> 路径 B（HTML/DOCX）直接消费`ast::Node`，不经 `document` 层，也不参与分页（HTML 流式、DOCX 由 Word 自行排版）。


### 2.1 模块清单

| 模块 | 职责 |
| --- | --- |
| `src/lib.rs` | 公共 API 入口（`markdown_to_pdf`、`html_file_to_pdf`、`ConvertOptions`、`PageConfig` 等），串联流水线，注入页面度量到 CSS 引擎。 |
| `src/dom/` | 解析输入为 DOM（`HtmlElement` 树），并把 DOM+CSS 转换为简化 AST。 |
| `src/css/` | CSS 解析、级联、`ResolvedStyle` 计算、内置样式表。 |
| `src/ast/` | 简化带样式 AST（Layer 2）：`Node` / `NodeKind` / `Style`。 |
| `src/document/` | 把简化 AST 排版为 `Document`（块树 + 文本行），含文本排版、样式类型、高亮、场景投影。 |
| `src/output/` | 五个输出后端 + 共享几何工具 `common.rs`。 |
| `src/bin/liepress.rs` | CLI（基于 `clap`）。 |
| `src/error.rs` | 统一错误类型 `Error` / `Result`。 |

---

## 3. 核心依赖

| 依赖 | 用途 |
| --- | --- |
| `lievisual` | 自研渲染基元（几何、文本、颜色、`Scene`）；**SVG 与 PNG 后端均先经 `to_scene::document_to_scene` 转成 `lievisual::Scene`，再分别由 `SvgRenderer`（Scene→XML）与 `VelloPixmapRenderer`（Scene→像素，底层基于 `vello_cpu`）渲染**。 |
| `lightningcss` | CSS 解析与原子化（值解析、单位、颜色）。 |
| `pulldown_cmark` | Markdown → 事件流。 |
| `html5ever` / `markup5ever` | HTML 解析为 DOM。 |
| `krilla` | PDF 生成（PDF 对象与页面写入）。 |
| `docx-rs` | DOCX 生成（从 `ast::Node` 重建语义化 Word 文档）。 |
| `syntect` | 代码块语法高亮。 |
| `clap` | CLI 参数解析。 |

> 图表（ECharts 风格，经 `liecharts`）与 Mermaid 图（经 `liemermaid`）的渲染位于 `src/document/ext_render/`，分别由 `charts` / `mermaid` feature 启用。

---

## 4. 模块详解

### 4.1 `dom` — 解析与 DOM 层

- **`markdown.rs`**：用 `pulldown_cmark` 把 Markdown 事件转换为 `HtmlElement` 树；处理 GFM 表格对齐（为 `<td>` 写入 `text-align` 内联样式）、任务列表复选框、脚注聚合、`HtmlBlock` 透明处理（不破坏块级语义）。
- **`parser.rs`**：基于 `html5ever` 的 HTML 解析；`collect_children_with_opts` 采用**显式栈的迭代式**递归（避免极端深嵌套导致栈溢出），并配合 `to_ast` 层的深度守卫。
- **`resource.rs`**：图片资源解析——`has_image_extension`、`is_absolute_src`（Unix/Windows/UNC）、`has_parent_component`；`load_local` 拒绝越界路径；远程加载需 `with_allow_absolute_paths` 显式开启（安全默认）。
- **`md_converter.rs`**：Markdown 资源引用与 `should_embed` 判定，与 `resource.rs` 的安全语义对齐。
- **`to_ast.rs`**：`html_to_styled_nodes` —— 把 `HtmlElement` 树 + CSS 解析结果转换为 `ast::Node` 树（简化 AST）。关键职责：
  - `contains_block_child` / `is_block_tag` 决定容器是否映射为块级 `Container`；
  - `extract_table_align` 兼容 pulldown 的 `TableHead → Td`（无 `Tr`）与 `TableHead → Tr → Td` 两种结构；
  - `DepthGuard`（thread_local，上限 `MAX_HTML_TO_AST_DEPTH`）防止深嵌套栈溢出；
  - 把 CSS `ResolvedStyle` 注入每个 `Node`。

### 4.2 `css` — 样式引擎

- **`engine.rs`**：核心 `CssEngine`。
  - `CssEngine::new(builtin_css)`：解析内置样式表为 `ResolvedRule` 列表（含选择器特定性、是否 `!important`）；用户 CSS 经 `with_user_css` 合并。
  - **级联**：两遍遍历——第一遍应用普通声明，第二遍仅应用 `!important` 声明，保证 `!important` 胜出。
  - **简写展开**：`margin`/`padding`（1–4 值）、`border`（含 `border-style`）、`background` 等展开为具体 longhand。
  - **单位解析**：`ResolveCtx { font_size, root_font_size, containing_block_width }` 决定相对单位的基准：
    - 盒模型属性（`width`/`height`/`margin`/`padding`/`border-width`）的百分比相对于**包含块宽度**；
    - 字体相关（`font-size` 的 `em`/`rem`/`ex`/`ch`、行高等）相对于**当前/根字体大小**；
    - `vw`/`vh` 相对于**页面尺寸**。
  - **颜色解析**：`#rgb`/`#rgba`/`#rrggbb`/`#rrggbbaa`、`rgb()`/`rgba()`（逗号或空格+斜杠）、`transparent`、约 150 个命名颜色。
  - **健壮性**：`parse_length` / `parse_finite_f32` 拒绝 `NaN`/`inf`；非法值回退安全默认值，避免排版崩溃。
  - `containing_block_width` 字段 + `set_containing_block_width()`：由 `lib.rs` 依据 `PageConfig`/`@page` 注入。
- **`ast/presets.rs`**：内置默认样式表 `DEFAULT_CSS`（标题、段落、列表、表格、代码块、引用、页眉页脚等的默认表现，由 `CssEngine::new` 加载），并提供 `list_marker_style()`。

### 4.3 `ast` — 简化带样式 AST（Layer 2）

- **`node.rs`**：`Node { kind, style, splittable }` 与 `NodeKind` 枚举（标题、段落、列表/列表项/任务列表、定义列表、脚注、图片、代码块、引用、分隔线、表格/行、文本/加粗/斜体/行内代码/链接/删除线/上下标、Span/Center/Container/LineBreak）。
  - `splittable` 标记该节点是否可在页间拆分（段落、列表项可拆；标题、表格行不可拆）。
  - `walk` / `collect_text` 提供遍历与文本提取工具。
- **`style.rs`**：`Style`（CSS 引擎解析得到的**最终样式值**，所有单位已解析为 pt，含字体、`color`、`line-height`、`page-break-*`、盒模型、表格字段等）与 `PageBreak` 枚举（`Auto`/`Always`/`Avoid`/`Left`/`Right`）。
- **`PageBreak`**：`page-break-before` / `page-break-after` 的枚举，由 `output::pdf` 的分页器消费。

### 4.4 `document` — 排版与中间表示

- **`layout/mod.rs`**：核心 IR `Document { blocks: Vec<Block> }`（**不分页**）。`Block { kind, style, splittable }` 与 `BlockKind`（对应 `NodeKind`，但段落持 `Vec<TextLine>`、列表项持预生成 `marker`、代码块持高亮后 `lines` 等）、`HeaderFooter`、`DefinitionItemBlock`。
- **`from_ast.rs`**：`ast_to_layout` —— `Node` 树 → `Document`。
  - `convert_node_depth`（上限 `MAX_CONVERT_DEPTH`）把深度传递给所有递归调用，防止深树栈溢出。
  - 注入列表标记（`marker`：有序 `"1."` / 无序 `"●"` / 任务 `"☐ "`）；聚合脚注到正文末尾。
- **`text.rs`**：文本排版核心。`css_text_style` 生成 `TextStyle`（含 `line_height`），`TextLine` 为已换行文本行；`layout_text` 负责折行与行高。
- **`types/`**：
  - `mod.rs`：`ResolvedStyle`、`TextAlign`、默认内容宽度 `default_content_width()`。
  - `style.rs`：`ResolvedStyle` 的 `page_break_before/after`、尺寸/颜色字段。
  - `page.rs`：`PageSettings`（页面尺寸、边距、`content_width()` 等），由 `PageConfig` 与 `@page` 构建。
  - 表格样式字段（`ResolvedStyle` 上的 `table_border_*` 边框、`table_alt_row_bg` 隔行、`table_header_bg` 表头底纹等）。
- **`highlight.rs`**：基于 `syntect` 的代码块语法高亮，产出带颜色/粗体的 `TextLine`。
- **`to_scene.rs`**：`document_to_scene(document, settings, dpi)` —— 把 `Document` 投影成 `lievisual::Scene`（图元 IR），供 SVG/PNG 后端共用。
  - 全程在 **pt** 坐标系计算，末尾乘 `scale = dpi / 72` 归一到像素；`DEFAULT_DPI = 144`。
  - `Scene.scale` 让 SVG 后端把 `viewBox` 还原到 pt 单位，PNG 后端按 `scale` 设画布像素。

### 4.5 `output` — 输出后端

- **`mod.rs`**：统一输出入口与各后端调度。
- **`common.rs`**：后端共享几何工具——`block_height`、`blockquote_content_height`、`table_row_height`、`table_border_segments`、`list_item_indent`、`text_style` / `text_style_from_resolved`、`heading_font_size`、`apply_heading_style` 等。避免各后端重复排版逻辑。
- **`pdf.rs`**：PDF 后端（krilla）。
  - 直接消费 `Document`（不经额外投影）。
  - `paginate_layout` 完成切页：内部 `PaginateCtx` 携带 `force_break_before` / `page_break_after`，遇 `page-break-*` 时 `push_page()`。
  - `PdfPage { blocks, header, footer, used_h }`：无限高度模式（`height_unlimited`）下，页面高度 = 上边距 + `used_h` + 下边距。
  - 跨页表格：续页通过 `repeat_table_header` 重复表头。
  - 超链接：`Annotation` + `LinkAction` + `Destination`，支持跨换行连续。
  - 字体缓存 `FontCacheKey`（按数据指针/长度/索引）避免重复加载。
- **`html.rs`**：把 `ast::Node` 序列化为 HTML 文档（`node_to_html`，保留结构与样式）。
- **`svg.rs` / `png.rs`**：经 `to_scene` 得到 `lievisual::Scene`，分别委托 `SvgRenderer` / `VelloPixmapRenderer` 生成产物。
- **`docx.rs`**：经 `docx-rs` 生成 DOCX（从 `ast::Node` 重建语义化元素）。
- **`ext_render/`**：`liecharts` / `liemermaid` 的嵌入渲染（由 `charts` / `mermaid` feature 启用），在外部分页前把代码块替换为图片节点。

---

## 5. 分页策略

- **原则**：`Document` 不分页（仅 PDF/SVG/PNG 走此层）；分页由各输出后端在消费 `Document` 时进行。HTML/DOCX 直接消费 `ast::Node`，不经 `document` 层，因此不参与分页（HTML 为流式输出、DOCX 交予 Word 自行排版）。
- **PDF**（`paginate_layout`）：
  - 顺序遍历块，累加高度；超过当前页可用高度则开新页。
  - `splittable=false` 的块（标题、表格行、图片等）整体保持在同一页（若单块超过整页则允许强制放入并溢出续页）。
  - `page-break-before: always` / `page-break-after: always` 通过 `force_break_before` / `page_break_after` 强制换页。
  - 表格跨页：续页通过 `repeat_table_header` 重复表头。
  - 孤行/寡行控制：避免段落首行孤立在页尾或末行孤立在页首（依据 `splittable` 与最小行数）。
  - 无限高度模式：用于单页/海报场景，最终页高由实际 `used_h` 决定。
- **HTML**：天然流式，不分页（由浏览器/CSS 控制）。
- **SVG/PNG**：`to_scene` 生成连续画布；PNG 按 `scale` 决定像素尺寸，SVG 由 `viewBox` 表达整页。
- **DOCX**：由 `docx-rs` 生成，原生分页交由 Word 处理。

---

## 6. CSS 解析与级联细节

- **输入来源**：内置样式表（默认）→ 外部 CSS 文件（`-s`/API 注入）→ Markdown 内联 `<style>`；后者可覆盖前者。
- **选择器**：支持类型、类、ID、后代/子代、属性等基本选择器，特定性（specificity）参与排序。
- **`!important`**：两遍应用，确保最高优先级。
- **简写**：`margin/padding` 按 1/2/3/4 值规则展开；`border` 解析宽度/样式/颜色；`background` 解析颜色与图片（图片通常降级为纯色）。
- **单位与基准**（`ResolveCtx`）：
  - 盒模型百分比 → 包含块宽度（由 `lib.rs` 从页面内容宽度注入）。
  - 字体相对单位 → 当前/根字体大小（`em`/`rem`/`ex`/`ch`）。
  - 视口单位 → 页面尺寸（`vw`/`vh`）。
- **颜色**：十六进制（3/4/6/8 位）、`rgb()`/`rgba()`、`hsl` 视实现、`transparent`、命名颜色。
- **容错**：解析失败（非法长度、NaN/inf、未知属性）在**非严格模式**下忽略并回退默认值；**严格模式**（`--strict`）则报错。

---

## 7. 字体系统

- **自动检测**：按文本字符分布判断主要语言（CJK / 拉丁等），推荐对应字体族。
- **回退链**：`with_font_family` / CSS `font-family` 提供候选列表，按可用性逐层回退。
- **缓存**：PDF 后端以 `FontCacheKey`（数据指针 + 长度 + 字体索引）缓存已加载字体，避免重复解析。

---

## 8. 可选功能与 Feature 开关

`Cargo.toml` 的 `[features]` 段（实际内容）：

```toml
[features]
default = ["charts", "mermaid"]
charts = ["dep:liecharts"]
mermaid = ["dep:liemermaid"]
```

- **`default`**：开启 `charts` 与 `mermaid`，即默认已支持图表（`liecharts`）与 Mermaid 图（`liemermaid`）渲染。
- **`charts`**：启用 `dep:liecharts`，支持 ECharts 风格图表（`liecharts`）代码块渲染。
- **`mermaid`**：启用 `dep:liemermaid`，支持 Mermaid 图（`liemermaid`）代码块渲染。

注意：

- **输出后端（PDF / HTML / SVG / PNG / DOCX）均无条件编译**，不通过 feature 开关控制，五种格式默认全部可用。
- 文档层 `src/document/ext_render/`（图表/图示嵌入）随 `charts`/`mermaid` feature 启用，是内外代码块替换的基础设施。
- 没有 `ext-render`、`render-*`、`web`、`all` 等 feature（此前文档所述为误写）。

> 如需最小构建，可显式关闭默认 feature：`cargo build --no-default-features`（仅保留核心 Markdown→PDF/HTML，不含图表/Mermaid）。

---

## 9. CLI 与 API

### 9.1 CLI（`src/bin/liepress.rs`，基于 `clap`）

```bash
# 基本转换（默认 PDF）
liepress -i document.md -o document.pdf

# 指定格式（扩展名或 -f 均可）
liepress -i document.md -o document.svg -f svg
liepress -i document.md -o document.png -f png
liepress -i document.md -o document.docx -f docx

# 自定义样式
liepress -i document.md -o document.pdf -s style.css

# 页眉/页脚
liepress -i input.md -o output.pdf --header "Project Report"
liepress -i input.md -o output.pdf --footer "Page {page} / {total}"

# 去除默认页码
liepress -i input.md -o output.pdf --no-page-number

# 页面尺寸与边距
liepress -i input.md -o output.pdf -p A5 --margin 24pt
liepress -i input.md -o output.pdf --page-width 210mm --page-height 297mm --landscape

# 严格 CSS 解析（遇错即失败）
liepress -i input.md -o output.pdf -S
```

- **输入格式**：`-` 表示标准输入；否则按扩展名（`.md`/`.markdown`、`.html`/`.htm`）推断。
- **输出格式**：按扩展名或 `-f` 推断（pdf/html/svg/png/docx）。
- **页面控制**：`--page-size`（A3/A4/A5/A6/Letter/Legal/Tabloid）、`--page-width/--page-height`（带单位）、`--landscape/--portrait`、`--margin` 及各方向 `--margin-top/...`。
- **其它**：`--title`（HTML `<title>`，默认首个 `<h1>`）、`--strict`。

### 9.2 Rust API

```rust
use liepress::{markdown_to_pdf, ConvertOptions};

let md = "# Hello\n\nThis is a **Markdown** document.";
let pdf = markdown_to_pdf(
    md,
    &ConvertOptions::new()
        .with_font_family(&["Noto Sans CJK SC", "sans-serif"])
        .with_css("h1 { color: #c00; }")
        .with_header("My Document")
        .with_footer("- {page} -"),
)?;
```

主要入口：`markdown_to_pdf` / `markdown_file_to_pdf` / `html_file_to_pdf`；配置体 `ConvertOptions`、`PageConfig`（页面尺寸、边距、`@page` 来源）。

### 9.3 Markdown 内联 CSS

```markdown
<style>
body { font-family: "Noto Sans CJK SC", serif; }
h1 { color: #c00; border-bottom: 1px solid #ccc; }
@page {
    margin: 36pt 54pt;
    header: "Project Report";
    footer: "Page {page} / {total}";
}
</style>

# Title
Document content here...
```

---

## 10. 错误处理

- 统一错误类型 `error::Error` / `Result<T>`。
- CSS 解析在非严格模式下容错回退；严格模式（`--strict`/等价 API）遇非法 CSS 即失败。
- 资源加载遵循安全默认：本地路径越界拒绝；远程资源需显式开启（`with_allow_absolute_paths`）。
- 文本排版与坐标计算对 `NaN`/`inf` 做防御，避免渲染崩溃。

---

## 11. 测试与质量

- 集成回归测试 `tests/css_engine_fixes.rs` 覆盖 `docs/code-review-2026-09-03.md` 的 P1-3/P1-4、P2-2/2-4/2-5、S-1/S-2/S-3（line-height 透传、PDF 无限高度、GFM 表格对齐、深嵌套栈溢出保护、分页高度等）。
- CSS 引擎的单元回归（简写展开、`!important` 胜出、`width%` 使用包含块宽度、扩展颜色、边框等）位于 `src/css/engine.rs` 的测试模块。
- 建议改动后运行 `cargo test` 与构建（`cargo build --all-features`）确认无回归。

---

## 12. 开发指南与设计原则

### 12.1 新增一个块级/行内元素

1. 在 `ast/node.rs` 的 `NodeKind` 增加变体（如需要），保持 `walk` / `text_content` 同步。
2. 在 `dom/to_ast.rs` 增加 `HtmlElement` → `Node` 的映射，并注入 `ResolvedStyle`。
3. 在 `document/layout/mod.rs` 的 `BlockKind`（及 `document/from_ast.rs` 的 `ast_to_layout`）增加对应排版逻辑，决定 `splittable`。
4. 在各输出后端（`output/*`）按 `BlockKind` 实现绘制；SVG/PNG 走 `to_scene.rs`。
5. 如涉及 CSS 默认表现，在 `src/ast/presets.rs` 的 `DEFAULT_CSS`（内嵌 `presets/default.css`）补充样式。

### 12.2 新增一个输出后端

- 直接消费 `document::layout::Document`。
- 优先复用 `output/common.rs` 的几何工具，避免重复排版。
- SVG/PNG 类后端复用 `document::to_scene` 投影到 `lievisual::Scene`。
- 分页逻辑各自实现，遵循"Document 不分页"原则。

### 12.3 改动样式引擎时的注意点

- 任何相对单位都要明确基准（`containing_block_width` 或 `font_size`/`root_font_size`），不要混用。
- 简写展开必须同时更新普通与 `!important` 两遍处理。
- 新颜色/单位解析需在 `parse_*` 中保持对 `NaN`/`inf` 的拒绝。

### 12.4 稳定性红线

- 不在 `document` 层引入分页概念。
- 不在 `ast`/`dom` 层直接绘制。
- 资源加载默认拒绝远程/越界，安全开关需显式开启。
- 深递归路径必须带深度守卫或改为迭代式，防止栈溢出。
