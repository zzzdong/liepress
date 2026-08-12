# LiePress 重构方案：Document 中间层 + 多输出后端并列

> 本文档取代 `docs/new-world.md` 的架构方向。
> `new-world.md` 设想的是「Markdown → HTML → Styled HTML → Document(pages) → Output」，其中 HTML 仍居于 PDF 之前、是唯一中间表示；本方案将其收口为：**Document 作为唯一中间层，PDF / DOCX / HTML / SVG / PNG 并列作为输出后端**，各自从 Document 派生，分页由各输出后端独立负责。

- 文档版本：1.4
- 最后更新：2026-08-12
- 状态：方案定稿（已批准），待 S0 实现（已并入三轮评审修订 + typst 布局借鉴 + 终审 Pro-tips，见 §9）

---

## 1. 目标与原则

### 1.1 一句话目标

确立一个**格式无关、已完成布局度量、但未分页**的 `Document` 中间层，让 PDF、DOCX、HTML、SVG、PNG 五个输出后端**并列消费它**。分页（尤其是 A4 排版）是各后端自己的职责，不放在中间层。

### 1.2 四项决策（已与用户确认）

| 决策点 | 结论 |
|---|---|
| **A. HTML 来源** | 选项 1：HTML 输出层**从 `Document` 生成**语义化 HTML，各后端对称、内容一致。 |
| **B. PDF 旧像素代码** | 选项 2：PDF 后端**重写为直接消费 `Document`**，删除旧的 `generator::Document`/`VisualElement` 像素中间层（分阶段收口）。 |
| **分页责任** | 各输出后端各自负责独特分页逻辑：PDF 按 A4 内部 paginate；DOCX 仅把 A4 写入 `sectPr` 由 Word 分页；HTML 不分页。 |
| **首要目标** | A4 页面排版是首要目标，分页算法主写在 PDF 后端。 |

### 1.3 核心原则

1. `Document` 只存**语义块 + 未分页的布局度量**（断行结果、行高、列宽、缩进、间距），绝不预先切片到页。
2. 任何输出后端都**不反向**从 PDF 像素还原结构；它们都正向消费 `Document`。
3. `Document` 不持有 parley 字体句柄 / 字形——只存字体名、字号、颜色等纯样式描述，保证 DOCX/HTML 可消费、可序列化。
4. 过渡期内允许旧 `generator::Document` 与新 `Document` 并存，但 S6 收口时删除旧层（B=选项2 的终点）。

---

## 2. 架构对比

### 2.1 当前架构（问题所在）

```
Markdown
  └─[markdown crate]─▶ MDAST
        └─[ast]─▶ Styled AST (Node + Style)
              └─[generator]─▶ Document { pages: Vec<Page> }
                                      Page { elements: Vec<VisualElement> }   ← 带绝对坐标 (x,y) 的像素产物
                                            └─[render::pdf/svg/png]─▶ 输出
```

问题：
- `generator::Document` 是**已分页、带绝对坐标的像素产物**。表格在 generator 内就被切成矩形 + 文本；段落被切成绝对定位 `TextRun`。
- 分页逻辑散落在 generator（`block.rs` / `table.rs` / `place_text_lines`），PDF/SVG/PNG 共用同一份已分页数据。
- HTML 当前只是「解析链路更上游的步骤」，并非一个可独立消费 `Document` 的输出层。
- DOCX 无从生成：无法从「一堆 (x,y) 矩形」还原出段落 / 表格语义。

### 2.2 目标架构

```
Markdown / HTML 源
   │  (HTML 解析链路保持不动)
   ▼
HtmlDocument ──▶ Styled Node (Node + 已解析 Style)          ← Layer 2：语义 + 样式
   │  generator 布局（只测算度，不分页）
   ▼
┌──────────────────────────────────────────┐
│            Document（唯一中间层）           │
│  blocks: Vec<Block>        ← 未分页、有序   │
│  page_config: DocumentPageConfig ← A4 等「意图」 │
│  outline / root_style                   │
└──────────────────────────────────────────┘
        │                │                 │
   ┌────┴────┐      ┌────┴────┐       ┌────┴────┐
   ▼          ▼      ▼          ▼       ▼          ▼
 render::pdf   render::docx   render::html / svg / png
 (A4 内部      (写 A4 sectPr,   (连续流,不分页，共用
  paginate      Word 自行分页)    非分页展平逻辑)
  + 绘制)
```

要点：
- **Document 是汇合点**：Markdown 与 HTML 两种源都先汇入 Styled Node，再由 generator 产出统一的 `Document`。
- **五后端并列**：`PDF / DOCX / HTML / SVG / PNG` 地位平等，都从 `Document` 读数据。HTML/SVG/PNG 同为连续流、共用非分页展平逻辑（评审意见 3）。
- **分页下沉**：`Document` 不含页序列；PDF 后端内做 `paginate(doc, A4)`，DOCX 只写 `sectPr`，HTML/SVG/PNG 直接展平。

---

## 3. Document 中间层设计（S0 核心）

### 3.1 模块位置

新增 `src/document/`：

```
src/document/
├── mod.rs          # StructuredDocument 入口 + 重导出
├── block.rs        # Block 枚举及各类块结构
├── text.rs         # DocTextRun / DocLine / 纯样式文本描述
├── table.rs        # TableBlock / TableRow / TableCell
└── style.rs        # ResolvedStyle（从 ast::Style 投影）
```

### 3.2 顶层结构

```rust
// src/document/mod.rs
pub struct StructuredDocument {
    /// 未分页、有序的块序列（HTML/PDF/DOCX 都从这里读）
    pub blocks: Vec<Block>,
    /// 页面「意图」：尺寸、边距、页眉页脚。未分页。
    pub page_config: DocumentPageConfig,
    /// 大纲（由标题块收集，供 PDF 书签 / DOCX 导航）。中间层不含页号，见 §3.2.1。
    pub outline: Vec<OutlineEntry>,
    /// 根样式（body 解析结果），供继承与默认映射
    pub root_style: ResolvedStyle,
}

pub struct DocumentPageConfig {
    pub size: PageSize,          // A4 / Letter / 自定义
    pub margin_top: f32,         // pt
    pub margin_bottom: f32,
    pub margin_left: f32,
    pub margin_right: f32,
    pub header: Option<DecoratedText>,
    pub footer: Option<DecoratedText>,
}
```

> **命名（评审意见 1）**：不用 `PageConfig`，避免与既有 `ast::style::PageConfig`（`src/ast/style.rs:506`）撞名。新 `PageConfig` 与原 `ast` 类型字段几乎完全重叠，仅把 `Option` 展开为具体值；`Document` 层不依赖 `ast` 层，故单独命名 `DocumentPageConfig`。`PageSize` 枚举（A4/Letter/自定义）为 `ast` 所无，作为新增。

#### 3.2.1 大纲与页眉页脚：分页产物字段不下沉（评审意见 5、10）

`Document` 不分页，因此与"分页结果"强相关的字段**一律不在中间层填充**：

- **大纲（outline）**：中间层的 `OutlineEntry` 只携带 `level / title / children` 与锚点；`page_number / x_position / y_position`（见 `generator/context.rs`）由 **PDF 后端在 `paginate` 后回填**。DOCX 用 `<w:bookmarkStart>` 导航，不依赖页号。
- **页眉页脚**：`DocumentPageConfig.header/footer` 只存**模板字符串 + 样式**（`{page}`/`{total}` 原样保留）。`{total}`（总页数）与 `{page}` 页号均须在分页完成后才能解析，因此**实际解析与绘制完全下沉到 PDF 后端分页收尾之后**（沿用现 `generator/header_footer.rs` 的时序，但移到 PDF 后端）；DOCX 的 `sectPr` 页眉页脚在打包时按 `{page}` 模板展开。

上述两条与现有 `types.rs` / `header_footer.rs` 的"分页后注入"时序保持一致，不产生行为回退。

### 3.3 块枚举（语义 + 度量，未分页）

```rust
pub enum Block {
    Heading(HeadingBlock),
    Paragraph(ParagraphBlock),
    List(ListBlock),
    Table(TableBlock),          // 完整数据，绝不切片
    CodeBlock(CodeBlock),
    Image(ImageBlock),
    BlockQuote(QuoteBlock),
    ThematicBreak,
    PageBreak,                  // 显式分页符（可选）
}

pub struct HeadingBlock {
    pub level: u8,              // 1..=6
    pub runs: Vec<DocTextRun>,
    pub anchor: Option<String>, // 用于 outline / 锚点
}

pub struct ParagraphBlock {
    pub runs: Vec<DocTextRun>,  // 语义富文本（DOCX/HTML 用此重排），来源见 §3.3.1
    pub lines: Vec<DocLine>,    // 断行度量（PDF 用此精确绘制，DOCX/HTML 忽略）
    pub align: Align,
    pub indent: f32,            // 首行缩进（pt，已换算）
    pub spacing_before: f32,    // 相邻间距合并后的结果
    pub spacing_after: f32,
    pub bg: Option<BlockBg>,    // 段落底色 / 边框（PDF 画矩形，DOCX→w:shd）
}
```

#### 3.3.1 `runs` 的来源必须是"样式切分"，而非 parley 断行结果（评审意见 2）

现有 `text.rs::TextRun` 是 parley `GlyphRun` 的产物——一个 run 是**已断行、已连字的字形序列**（`extract_lines_from_parley` 里文本会因换行被切碎）。若 `DocTextRun` 直接由它投影，**DOCX/HTML 用 `runs` 重排时会把被换行切断的两段硬拼回同一行**，产出错误。

因此 `ParagraphBlock.runs` **必须来自 `html::styled` 层按 `Node + Style` 切分的语义 run**（源文本的样式区间序列），而非 generator 断行后的 run。`lines`（断行度量）才由 parley 在 generator 阶段按 `runs` 计算得到。S1 产出 `ParagraphBlock` 时，需同时保留"语义 runs"与"断行 lines"两个视图，二者共享同一文本但切分维度不同。

#### 3.3.2 `lines` 是"参考度量"，非通用事实（外部评审建议 1）

`ParagraphBlock.lines`（断行结果）**强依赖可用内容宽度**，因此它不是格式无关的通用度量，而是**绑定 `DocumentPageConfig` 的参考度量（Reference Metrics）**：

- 断行在 generator 阶段按 `DocumentPageConfig` 的 A4 尺寸与边距计算，`lines` 仅对**同一尺寸/边距**严格有效。
- **契约声明**：
  - **PDF 后端**：若渲染尺寸与 `page_config` 一致，直接消费 `lines`（精确绘制）；若需支持动态改变纸张（如 A4→Letter），**丢弃 `lines`，基于 `runs` 重新调 parley 断行**（降级路径现成可用，`text.rs::create_text_layout` 已支持 `max_width` 参数）。
  - **DOCX/HTML/SVG/PNG 后端**：**忽略 `lines`**，仅消费 `runs` 与语义结构，由各自消费侧（Word / 浏览器 / 直接绘制）断行。
- **推论**：`lines` 本质是"PDF 后端的缓存度量"，而非 `Document` 的通用数据。若 S2 后 PDF 的 `paginate` 反复改动纸张尺寸，可考虑把 `lines` 从 `ParagraphBlock` 移出、改为 PDF 后端按需计算；S0 阶段暂保留在 `ParagraphBlock`，但语义上须理解为"参考度量"。

### 3.4 纯样式文本（关键：不持有字体句柄）

```rust
// src/document/text.rs
pub struct DocTextRun {
    pub text: Arc<str>,             // 文本共享，避免跨行重复拷贝（评审意见 3）
    pub font_family: Vec<String>,   // 来自 Style.font_family
    pub east_asia_font: Option<String>, // 中文字体名，映射 DOCX w:eastAsia
    pub font_size: f32,             // pt
    pub weight: u16,                // 400 / 700
    pub style: FontStyle,           // Normal / Italic
    pub color: Option<Color>,
    pub decoration: TextDecoration, // none / underline / line-through
    pub background_color: Option<Color>,
    pub url: Option<String>,
}

/// 对 `ParagraphBlock.runs` 的引用视图：某行内的某段 run 子区间。
/// 通过索引 + 字节区间引用 `runs`，**不持有独立文本**，避免内存重复拷贝。
pub struct RunRef {
    pub run_idx: usize,             // 指向 runs 的索引
    pub byte_start: usize,          // 该行内的字节起始
    pub byte_end: usize,            // 该行内的字节结束（开区间）
}

pub struct DocLine {
    pub run_refs: Vec<RunRef>,      // 该行引用的 runs 子区间（评审意见 3）
    pub line_height: f32,           // pt
    pub baseline_y: f32,            // 相对段落原点的基线 y
}
```

> 与现有 `text::TextRun` 的区别：现有 `TextRun` 持有 `parley::FontData` 与 `glyphs`（PDF 绘制专用）。`DocTextRun` **只描述样式**，不绑定任何渲染后端，因此可被 HTML/DOCX 安全消费，也可序列化。PDF 后端在绘制时再据此查询/缓存真实字体。
>
> **内存契约（评审意见 3）**：一个跨行的 run 在 `ParagraphBlock.runs` 只存一份文本，`DocLine` 通过 `RunRef`（索引 + 字节区间）引用它，**不复制 String**。`DocTextRun.text` 用 `Arc<str>` 共享底层字符串。这套视图设计使 `Document` 在处理大型 Markdown 时保持内存紧凑，避免文本数据在 `runs` 与 `lines` 两处重复。
>
> **`RunRef` 生命周期与并发（最终评审 Pro-tip 1）**：`DocLine` 因持有索引，生命周期与 `ParagraphBlock` 强绑定，无法脱离 Block 独立借用。曾有人建议直接把 `RunRef` 改为持 `Arc<str>` 的 Owned 类型以规避借用检查——**但保留 `RunRef` 作为核心模型**，理由有二：
> 1. **子区间语义**：一个跨行 run 在不同行是不同字节区间（§3.3.1），`RunRef` 的 `byte_start/end` 精确表达这一事实；若改成 Owned `Arc<str>`，仍需额外存字节区间，反而退化为"带 Arc 的 RunRef"。
> 2. **PDF 分页是单线程推进**：typst 的 `layout_flow` 是串行 `compose`（§10.1 `Work` 状态机），并非多页并发布局；liepress 的 `StructuredDocument` 分页同理，`DocLine` 不需要跨线程传递。
>
> 为兼顾"独立/并发使用"的诉求，S0 为 `ParagraphBlock` 提供**便捷解析方法**（如 `fn resolve_run_ref(&self, r: &RunRef) -> Arc<str>`），把 `RunRef` 解析为 Owned `Arc<str>` 片段，供 DOCX 输出、调试、子文档等需要独立文本的场景使用。

### 3.5 表格块（完整数据，分页意图外置）

```rust
// src/document/table.rs
pub struct TableBlock {
    pub rows: Vec<TableRow>,        // 完整数据，不切片
    pub header_row_count: usize,   // 通常 1
    pub repeat_header: bool,        // 跨页重复表头（PDF 重复 / DOCX→w:tblHeader）
    pub col_widths: Vec<f32>,      // 列宽（pt）
    pub align: TableAlign,
    pub caption: Option<String>,
    pub border_color: Color,
    pub border_width: f32,
    pub cell_padding_h: f32,
    pub cell_padding_v: f32,
    pub header_bg: Option<Color>,
    pub alt_row_bg: Option<Color>,
}

pub struct TableRow {
    pub cells: Vec<TableCell>,
    pub is_header: bool,
}

pub struct TableCell {
    pub blocks: Vec<Block>,        // 单元格内可含段落 / 列表等
    pub align: CellAlign,
    pub colspan: usize,
    pub rowspan: usize,
}
```

> **核心收益**：表格在中间层始终以 `Vec<Vec<Cell>>` 完整存在。分页（整行下移 / 重复表头 / 极端长单元格内拆）完全发生在 PDF 后端的 `paginate` 阶段，生成每页的 `Vec<RowRange>` + 是否重复表头。DOCX 直接读 `repeat_header` 输出 `<w:tblHeader/>`，HTML 输出 `<table>`。

#### 3.5.1 单元格断行的列宽依赖（评审意见 9）

单元格内段落高度 = 断行行数 × 行高，而**断行必须在已知列宽（`col_widths`）后才能进行**。`TableCell.blocks` 里的 `ParagraphBlock` 无法在纯 `Document` 层一次性算好 `lines`——因为列宽本身是表格级信息，渲染期才可能调整。

**决策**：单元格内 `ParagraphBlock` **不预存 `lines`**（`lines` 留空）。PDF 后端在 `paginate` 表格时，按 `TableBlock.col_widths` 对单元格内 `runs` 临时断行测高；DOCX/HTML 本就用 `runs` 重排，不受影响。`Document` 层对单元格只保证"语义完整（runs）+ 表格级度量（col_widths）"，不含单元格级断行结果。

> **列宽意图（外部建议 9，P1）**：`col_widths: Vec<f32>`（pt）是绝对值。S5 做 HTML 时如需响应式 `<colgroup>`（`%` 宽度），需补充 `col_widths_intent: Vec<ColumnWidthIntent>`（`Auto`/`Percentage`/`Fixed`）。**S0 不引入该字段**，保持 `Vec<f32>` 绝对值起步，避免 S0 阶段增加类型复杂度；S5 HTML 后端落地时再补。

### 3.6 其他块

- `ListBlock { ordered: bool, start: usize, items: Vec<Listitem> }`，`Listitem { blocks: Vec<Block>, marker: String }`。
- `CodeBlock { lang: Option<String>, lines: Vec<String>, bg: Color }`，PDF/DOCX 用等宽字体 + 底色；HTML 用 `<pre><code>`。
- `ImageBlock { data: bytes::Bytes, format: String, alt: Option<String>, caption: Option<String>, width: Option<f32>, height: Option<f32> }`。
- `QuoteBlock { blocks: Vec<Block>, side_color: Color }`。
- `BlockBg { color: Color, padding: f32, radius: f32 }` 用于段落 / 行内代码底色。

#### 3.6.1 图片字节的内存策略（评审意见 14、外部建议 10）

`ImageBlock.data` 内嵌图片字节，直接决定 `Document` 的内存形态与各后端的加载时机：

- **DOCX** 必须把图片字节写入 OOXML `word/media/`，**必须内嵌**。
- **PDF** 只需引用（字体/图片可延迟加载），内嵌会造成 `Document` 拷贝大块内存。
- `Document` 若要保持可序列化（§3.4 声明 `DocTextRun` 可序列化），内嵌字节会使快照体积膨胀。

**决策**：`ImageBlock.data` 使用 **`bytes::Bytes`**（零拷贝共享字节切片，支持 `Bytes::slice` 子切片共享），而非 `Arc<Vec<u8>>`——后者无法优雅表达"共享一段大字节的局部视图"，`bytes::Bytes` 是 Rust 生态对此场景的标准方案。若 S0 不希望引入 `bytes` 依赖，可用 `Arc<[u8]>` 作为等价替代。`Document` 序列化场景下由序列化层决定是否剥离 `data`（快照用路径引用）。图片**懒加载**（延迟到 PDF 绘制时解码）作为可选优化，S2 之后评估。

### 4.1 总原则

`Document` 不知道「页」。分页算法写在消费侧：

| 后端 | 分页做法 | 表格分页 |
|---|---|---|
| **PDF** | `render::pdf::paginate(doc, A4)`：按内容区高度把 `blocks` 切成多页；标题 `splittable=false`；段落按 `DocLine` 切；代码块背景跨页连续 | 整行下移；`repeat_header` 时每页重绘表头；单格超过一页才允许行内拆 |
| **DOCX** | 不 paginate；把 `DocumentPageConfig.size/margin` 写入 `word/sectPr.xml` 的 `<w:pgSz>`/`<w:pgMar>`，由 Word 自行分页 | `<w:tblHeader/>` 由 `repeat_header` 决定；Word 默认整行下移 |
| **HTML** | 不分页，直接顺序展平 `blocks` 为语义标签 | `<table>` 原生响应式 |

### 4.2 PDF 分页伪流程（S2 实现）

```
# 前置：pdf 后端把 doc.blocks（树形）递归展平为带层级/缩进的扁平序列
# （见 §4.2.1），paginate 在此扁平序列上做线性推进。
paginate(doc, page_config):
    content_area = page_height - margin_top - margin_bottom
    pages = []
    cur = new_page()
    for block in flatten(doc.blocks):        # PDF 后端内递归展平
        if block is PageBreak:
            if cur.used == 0: continue       # 防呆：已在页首则跳过，避免空页（评审建议 11）
            flush(cur); cur = new_page(); continue
        h = measure(block)                   # 用 block 自带的度量（行高/列宽/图片尺寸）
        if h <= content_area - cur.used:
            place(block, cur); cur.used += h
        else:
            if block.splittable == false:    # 标题等
                flush(cur); cur = new_page(); place(block, cur)
            elif block is Table:
                split_table(block, cur, pages)   # 整行下移 + 重复表头
            elif block is Paragraph:
                split_paragraph_by_lines(block, cur, pages)
            else:
                flush(cur); cur = new_page(); place(block, cur)
    flush(cur)
```

> 这与 typst 的 `flow/mod.rs` + `flow/distribute.rs` 思路一致：外层 `distribute` 把内容切片到多个 `regions`，块自身只声明「可否拆」。区别在于 liepress 把 region 驱动简化为「A4 内容区高度 + 剩余高度」的线性推进，不引入多分栏 region 复杂度。

#### 4.2.1 块树展平：保留树形，PDF 内展平（评审意见 8、外部建议 2）

`Document` 的 `Table/List/Quote` 内含递归 `blocks`，构成任意深度的块树。**决策（经外部评审修订）：`StructuredDocument` 保留树形 `blocks`（原始嵌套结构），展平仅在 PDF 后端的 `paginate` 内进行。**

- **`StructuredDocument.blocks` 是树形**：嵌套结构天然存在，DOCX/HTML 后端直接递归消费即可重建 `<w:tbl>/<w:list>`、`<ul>/<blockquote>`，无需 `container_id`/`nesting_path` 额外标识（避免给每个块背额外内存）。
- **PDF 后端在 `paginate` 时递归展平**为扁平序列（带缩进/层级信息），在该序列上做线性高度推进与分页。
- 表格的整行下移/重复表头仍作为 Table 块内部的原子分页逻辑（§4.3），不影响外层扁平推进。

> **`Table` 是不透明叶子（最终评审 Pro-tip 3）**：PDF 的 `flatten` 递归函数遇到 `List`/`BlockQuote` 会深入其子 `blocks`，但**遇到 `Table` 绝不能继续展平 `TableCell` 内部的 `blocks`**——必须把 `TableBlock` 当作**不透明叶子节点**（对应 typst 的 `MultiChild`，§5.5.6），整体作为一个原子推入扁平序列。单元格内的断行、跨页 `MultiSpill`、重复表头，必须由 `TableBlock` 自己的 `layout_table` 函数在收到"内容区剩余高度"后**内部闭环处理**（§4.3），外层 `paginate` 只把表格当"一个可拆项"调用其 layout、取回 `spill`。这避免了表格被错误切开导致的结构错乱。

> **修正记录**：初版决策曾把展平放在 S1 生成器侧（扁平 `Vec<Block>` + 层级信息）。外部评审指出，仅靠 `indent` 很难让 DOCX/HTML 完美重建嵌套 DOM，而背 `container_id` 又增额外内存。故改为"保留树形 + PDF 内展平"——DOCX/HTML 用树最自然，PDF 用展平最线性，各取所需。

### 4.3 表格分页细节（回答用户 Q2）

- **默认 `repeat_header = true`**：跨页时每页表格顶部重绘表头行（与 typst、Word 默认行为一致）。
- **整行下移**：某行放不下当前页剩余高度时，整行移到下一页，不拦腰截断（修复现有 `table.rs` 把行切成矩形+文本的问题）。
- **极端长单元格**：仅当单行高度 > 一整页内容区时，才允许单元格内容行内拆分，并标记「此行被拆分」以便后续可能的续行提示。
- **DOCX 受益**：`<w:tbl>` 天然支持跨页重复表头，直接读 `repeat_header`；PDF 与 DOCX 共享同一套表格语义与分页意图。

---

## 5. 输出后端改造

### 5.1 PDF 后端（B=选项2，重写消费 Document）

- 入口改为 `render_pdf(doc: &StructuredDocument, opts) -> Vec<u8>`（或返回页数结构）。
- 内部：`paginate` 得到多页布局 → 对每个块调用绘制原语（矩形 / 文本 / 图片 / 表格）。
- **删除**：旧的 `generator::Document { pages, Page { elements: Vec<VisualElement> } }` 与 `render::pdf` 中消费 `VisualElement` 的分发（S6 收口）。
- 字体：保留 `text` 模块的 parley 布局能力，但改为「在 paginate 阶段按 `DocTextRun` 的 `font_family/size` 做 parley 布局 → 得到字形 → 绘制」。即 parley 从「generator 内」移到「PDF 后端内」。
- 坐标系：保持现有「左上角原点 → PDF 左下角翻转」逻辑。

### 5.2 DOCX 后端（新增，S4）

- 新增 `src/render/docx.rs`，地图 `StructuredDocument` → OOXML：
  - `Block::Heading` → `<w:p><w:pPr><w:OutlineLvl/></w:pPr><w:r>...` + 标题样式
  - `Block::Paragraph` → `<w:p>`，每行/每段内 `DocTextRun` → `<w:r><w:rPr>...</w:rPr><w:t>`
  - `DocTextRun` → `RunProperty`：`b`/`i`/`color`/`sz`/`u`；`background_color` → `<w:shd>`；`url` → `<w:hyperlink>`
  - `Block::Table` → `<w:tbl>` + `<w:tblPr><w:tblHeader/>`(若 repeat_header) + 列宽；单元格 `<w:tc>`
  - `Block::CodeBlock` → 等宽 `<w:r>` + `<w:shd>` 底色
  - `Block::Image` → `<w:drawing>` 嵌入图片
  - `DocumentPageConfig` → `word/sectPr.xml` 的 `<w:pgSz w:w=.. w:h=..>`(A4=11906×16838 twips) + `<w:pgMar>`
- 打包：手写 minimal OOXML（zip + XML），或引入 `zip` crate；产物 `.docx` 含 `[Content_Types].xml`、`_rels/.rels`、`word/document.xml`、`word/styles.xml`、`word/sectPr`（可内联于 document.xml）。
- 行内重排：DOCX **忽略 `ParagraphBlock.lines`**，让 Word 自行断行（保留 `runs` 语义即可）。

> **列表降级（外部建议 4，S4 范围裁剪）**：手写 minimal OOXML 时，**不要**在 S4 生成 Word 原生多级列表（`<w:num>`/`<w:abstractNum>`/`<w:numPr>`）——它极易写崩或格式错乱，是 OOXML 里最深的坑之一。**S4 优先实现"视觉模拟"列表**：用段落缩进 + 文本符号（`•` / `1.`）伪装列表（与 §4.2.1 的"展平 + 缩进"天然契合）。**原生 Word 多级列表推迟到 P2/P3**，以免 S4 进度失控。

> **图片"两遍扫描"（最终评审 Pro-tip 2）**：OOXML 要求图片必须在 `word/_rels/document.xml.rels` 注册 `rId`，且 `[Content_Types].xml` 需声明 `png/jpeg` 扩展。若在生成 XML 的"同一遍"里动态发现图片并改 `.rels`，会导致流式写入混乱/回溯。**S4 的 `render::docx` 用两遍扫描**：
> - **Pass 1**：遍历 `Document`，收集所有 `ImageBlock`，为每个分配递增 `rId`，将 `(rId, bytes::Bytes)` 存入 `ImageContext`（`HashMap`）。
> - **Pass 2**：再次遍历 `Document` 生成 `document.xml`，遇到图片时从 `ImageContext` 查 `rId` 写 `<w:drawing>`；`<w:hyperlink>` 的 `rId` 同理。
> - 收尾：统一把 `ImageContext` 的图片字节写入 zip `word/media/`，并生成 `.rels` 与 `[Content_Types].xml`。此模式与 §3.6.1 的 `bytes::Bytes`（零拷贝共享）天然配合——Pass 1/2 共享同一字节视图，不复制。

### 5.3 HTML 输出层（S5，从 Document 生成）

- 新增 `src/render/html.rs`（或从现有 `html/md_converter.rs` 转向）：遍历 `doc.blocks` 输出语义化标签：
  - `Heading` → `<h1>`..`<h6>`
  - `Paragraph` → `<p>`（内联 `DocTextRun` → `<strong>`/`<em>`/`<a>`/`<code>`/`<span style="background">`）
  - `List` → `<ul>`/`<ol><li>`
  - `Table` → `<table><thead><tbody><tr><th>/<td>`
  - `CodeBlock` → `<pre><code>`
  - `Image` → `<img>`
  - `BlockQuote` → `<blockquote>`
- 样式（评审意见 6）：`Document` 持有的是**解析后**的 `ResolvedStyle`，而 HTML 输出层有两种取舍：
  - **纯语义 HTML + 外部 CSS**：HTML 只输出标签结构，样式靠外部 `DEFAULT_CSS` + 用户 CSS 呈现。优点：体积小、可读、与现有 CSS 驱动一致。缺点：`ResolvedStyle` 里解析出的部分样式若不在 CSS 中，会丢失。
  - **内联样式**：把 `ResolvedStyle` 的差异部分写成 `style="..."`（如 `background`、`color`）。优点：自包含。缺点：HTML 膨胀、难读。
  - **决策**：默认输出**纯语义 HTML + 外部 CSS**（与现状一致），`ResolvedStyle` 仅用于从 `Document` 反推语义标签的分类（如 `bg`→`<span style="background">`）；HTML 不再额外内联全部解析样式。保留 CSS 驱动（DEFAULT_CSS + 用户 CSS）作为浏览器侧呈现。
- **样式降级兜底（外部建议 5）**：对 `ResolvedStyle` 中**无法映射到语义标签**（如 `<strong>`/`<em>`）**且外部 CSS 无法表达**的样式（如行内自定义 `color`/`background-color`），HTML 后端**自动回退为内联 `style="..."`**，而非直接丢弃。这是"纯语义 + 外部 CSS"主策略的兜底，不是全部内联，仅处理语义映射覆盖不到的差异样式。
- 分页：无（连续流）。

### 5.4 派发（S6）

```rust
pub enum OutputFormat { Pdf, Html, Docx, Svg, Png }

// lib.rs 主入口
pub fn convert(md: &str, opts: &ConvertOptions) -> Result<Vec<u8>, Error> {
    let doc = build_document(md, opts)?;   // → StructuredDocument
    match opts.format {
        OutputFormat::Pdf  => render::pdf::render(&doc, opts),
        OutputFormat::Html => render::html::render(&doc, opts),
        OutputFormat::Docx => render::docx::render(&doc, opts),
        OutputFormat::Svg  => render::svg::render(&doc, opts),
        OutputFormat::Png  => render::png::render(&doc, opts),
    }
}
```

> **SVG/PNG 处置（评审意见 3、外部建议 6）**：现有 `render/{svg,pixmap}.rs` 与 PDF 一样消费 `VisualElement`。SVG/PNG 同为「连续流、不分页」，应与 HTML 一并并入 **S5**，共用同一套"非分页展平"逻辑（遍历展平后的 `blocks` 直接绘制/序列化），不需要 `paginate`。S6 删除旧 `generator::Document`/`VisualElement` 时，三者（HTML/SVG/PNG）都已迁移完成，不存在遗留消费方。
>
> **产物形态界定（外部建议 6）**：SVG/PNG 的产物形态明确为**单张长图（long-strip）**（高度 = 内容总高，不分页）。若未来需要"多页 PNG"（如逐页缩略图/幻灯片预览），须**复用 PDF 的 `paginate` 逻辑**，而非当前的连续流展平——该需求当前不在 S0–S6 范围内，标注为后续扩展。

---

### 5.5 借鉴 typst 的 layout 算法（生成 `StructuredDocument`）

生成 `StructuredDocument` 的布局算法，借鉴 `typst-layout`（`typst-0.15.1/crates/typst-layout/`）的**两阶段布局**思想。核心洞见：**typst 把布局拆成「与页面无关的预处理」和「与页面相关的分页」两段**，`collect` 对应前者（产出 `StructuredDocument`），`distribute` 对应后者（产出 PDF 的 `paginate`）。

#### 5.5.1 两阶段模型：`collect` → `distribute`（关键借鉴）

typst 的 `layout_flow`（`flow/mod.rs`）并不是直接分页，而是维护一个 `Work` 状态机：

```text
                 ┌─── collect.rs（与页面宽度无关的预处理）───┐
  content 元素 ──▶  Child::Line(已断行的段落行)             │
                  │  Child::Single(不可拆块)                │
                  │  Child::Multi(可拆块)                   │──▶ Child 序列
                  │  Child::Rel/Fr(间距, 含 weakness)       │   = liepress 的
                  │  Child::Placed(浮动)                    │     StructuredDocument
                 └──────────────────────────────────────────┘
                                        │
                                        ▼
                 ┌─── distribute.rs（region 驱动分页）───┐
                 │  Work { children:&[Child], spill }    │
                 │  compose(regions) → 每页 frame        │──▶ PagedDocument
                 └────────────────────────────────────────┘
```

**映射到 liepress**：
- **`collect` 阶段 = S1 生成器产出 `StructuredDocument`**。这个阶段**只依赖内容宽度（列宽）**，不依赖页面高度——段落行（`ParagraphBlock.lines`）在此预计算，块按"可拆/不可拆"预分类，间距（`spacing_before/after` + weakness）在此收集。
- **`distribute` 阶段 = S2 的 PDF `paginate`**。它消费 `StructuredDocument`（类比 `Child` 序列），按 region 高度推进分页，通过 `spill` 处理可拆块的续页。
- **`Regions` = 页面内容区**。`layout_page_run`（`pages/run.rs`）用 `Regions::repeat(area)` 把内容区作为**无限重复的 region 序列** feed 给布局，对产出的每个 fragment 帧再单独 layout 页眉页脚。

> **对 §3.3.2 的印证**：typst 的 `collect` 阶段段落断行也用 `regions.size.x`（列宽），与 liepress"`lines` 绑定内容宽度"一致。这确认了 §3.3.2"参考度量"契约的成立——`lines` 在 collect 阶段预计算、绑定宽度，正是 typst 的做法。

#### 5.5.2 Child 分类模型 → liepress 的 Block 分类

typst `collect` 的 Child 预分类，直接对应 liepress 分页时需要的"可否拆"信息：

| typst Child | 含义 | liepress 对应 | 分页行为 |
|---|---|---|---|
| `Line` | 已断行的段落行，带 `need`（孤儿/寡行抑制高度） | `ParagraphBlock.lines`（含首/末行高度） | 按行拆，孤儿/寡行抑制 |
| `Single` | 不可拆块（标题、图片、代码块整体） | `Heading/Image/CodeBlock/ThematicBreak/PageBreak` | 整块下移新页 |
| `Multi` | 可拆块（表格、列表），layout 返回 `(frame, spill)` | `TableBlock`（§4.3） | 拆出片段 + `spill` 续页 |
| `Rel`/`Fr` | 间距（含 `weakness` 折叠） | `spacing_before/after`（§7.2 P1 合并） | 页尾 trim、相邻取 max |
| `Placed` | 浮动元素 | （暂无） | — |

**借鉴落地（S1 必须产出）**：`StructuredDocument` 的每个 `Block` 应携带一个**"可否拆"标识**（`splittable: bool`），以及段落行的**首行/末行/前两行/后两行高度**（对应 typst `LineChild.need`），用于 PDF `paginate` 做孤儿/寡行抑制。这是 S1 相比当前方案 §4.2 需要补强的点。

#### 5.5.3 间距弱折叠（`weak spacing`）→ spacing 合并

typst `distribute` 里 `keep_weak_rel_spacing`/`trim_spacing` 处理相邻间距：**间距在页边界处折叠/trim**，`weakness` 决定是否可被抑制。

- **借鉴**：liepress 的 `ParagraphBlock.spacing_before/after` 在 generator 收尾阶段合并（相邻取 max），且页尾/页首的间距由 PDF `paginate` **trim**（避免页顶空出一段）。这一点已在 §7.2 P1 标注，这里明确其算法来源。

#### 5.5.4 `need` 与孤儿/寡行抑制

typst `LineChild.need` = 单行高度或"前两行/后两行/三行"高度（`block_rows`/`single_row` 等），用于**防止孤行/寡行**（段首行留在页尾、段末行甩到页首）。

- **借鉴**：PDF `paginate` 在拆段落时，若剩余高度不足以放下 `need`（首行或末行），则整段移动，不做行内拆分。这对应 §7.2 P2"避孤行（orphan/widow）"，S2 即可低成本内置。

#### 5.5.5 sticky 跟随（keep-with-next）

typst `sticky` 机制让标题（sticky block）在页尾时能跟随到下一页（标题与正文不分离）。

- **借鉴**：liepress 的 `Heading` 分页时应带 `keep_with_next` 语义——页尾若放不下标题+首行正文，则标题整体移下页。这在 S2 的 PDF `paginate` 里实现，作为 `Heading` 块的默认分页规则。

#### 5.5.6 `MultiSpill` 与表格/列表续页（核心借鉴，S3）

typst 可拆块（表格/列表）的 layout 返回 `(frame, spill)`：**frame 是当前 region 能放下的部分，spill 是放不下的剩余部分**，下个 region 继续 layout。

```text
layout(table, region_height):
    loop:
        if region_height 已满: return (当前部分, 剩余部分)   # spill
        else: 继续填表格行
```

- **借鉴**：liepress §4.3 的表格分页（整行下移 + 重复表头）本质就是 typst `MultiSpill`。**S3 应把表格 layout 改为"返回 `(当前页表格片段, spill)`"**：每次 `paginate` 给表格一个"内容区剩余高度"，表格自己决定放下几行、是否重复表头、返回溢出部分续页。这比外层线性推进 + 内层重算更符合 typst 的架构，也是 S3 的实现目标。

#### 5.5.7 对 S1/S2/S3 的改动汇总

借鉴 typst 后，S1–S3 相比原方案需补强：

- **S1**：`Block` 增 `splittable: bool`；`ParagraphBlock` 增首/末行高度（供孤儿/寡行）；`spacing_before/after` 在 generator 收尾做相邻取 max 合并。产出即 typst 的 `collect`。
- **S2**：`paginate` 用 `Regions`（内容区高度序列）+ `Work`（块序列 + spill）推进；内置孤儿/寡行抑制 + `keep_with_next`（标题跟随）；页边界 trim 间距。产出即 typst 的 `distribute`。
- **S3**：表格 layout 改为 `MultiSpill`（返回片段 + spill 续页）。

> **对照 §6**：本节的 `collect/distribute` 划分是本方案对 §6 typst 对照表第 2 行（`layout_flow + Regions + distribute`）的细化落地。

---

## 6. 与 typst 的对照（验证方向）

| typst | liepress 重构后 |
|---|---|
| `PagedDocument`（带 `Frame`） | `StructuredDocument`（不分页时） |
| `layout_flow` + `Regions` + `distribute`（分页） | `render::pdf::paginate(doc)`（分页上移） |
| `flow/distribute.rs` 相邻 spacing 取 max | `ParagraphBlock.spacing_before/after` 在 generator 收尾阶段做合并 |
| `inline/cjk_punct_style` 避头尾 | generator 度量阶段做 CJK 避头尾（后续 P1） |
| `typst-pdf` / `typst-html` 多后端消费 `PagedDocument` | `render::{pdf,docx,html,svg,png}` 多后端消费 `StructuredDocument` |
| 无 DOCX | 新增 `render::docx` |

---

## 7. 分阶段实施计划

| 阶段 | 任务 | 改动范围 | 风险 |
|---|---|---|---|
| **S0** | 新增 `src/document/`：`StructuredDocument`/`Block`/`DocTextRun`/`TableBlock`/`ResolvedStyle` 类型骨架 | 纯新增 | 低 |
| **S1** | generator 新增产出 `StructuredDocument`（未分页、保留树形）路径：语义 `runs`（来自 styled 层）+ 断行 `lines`（参考度量）+ `RunRef` 视图 + `splittable`/首末行高度（§5.5.2）；spacing 相邻取 max 合并（§5.5.3）。旧 `finish()` 路径仅以 `legacy-generator` feature **过渡期隔离**（S1–S5 并存，S6 彻底删除，不保留） | generator 重构（输出目标抽象，见 §7.3） | **中**（非并行路径） |
| **S2** | 新增 `render::pdf::paginate(doc, A4)`：`Regions` + `Work` 推进（§5.5.1），内置孤儿/寡行抑制（§5.5.4）+ 标题 keep-with-next（§5.5.5）+ 页边界 trim 间距（§5.5.3）；回填 outline 页号、解析页眉页脚 `{total}` | PDF 后端重写 | 高（核心） |
| **S3** | table 分页改 `MultiSpill`（返回片段 + spill 续页，§5.5.6）：整行下移 + 重复表头（在 PDF paginate 内，按 `col_widths` 临时断行测高） | PDF paginate | 中 |
| **S4** | 新增 `render::docx`：OOXML 生成；`DocTextRun`→`RunProperty`（含 `eastAsia`）、`bg`→`w:shd`、`TableBlock`→`<w:tbl>`+`tblHeader`、图片→`word/media/` | Cargo 加 `zip` | 中 |
| **S5** | HTML / SVG / PNG 三个非分页后端从 `StructuredDocument` 生成（共用非分页展平逻辑） | 新增 html 后端 + 改造 svg/png | 低 |
| **S6** | `ConvertOptions` 增加 `OutputFormat::{Pdf,Html,Docx,Svg,Png}` 派发；删除旧 `generator::Document`/`VisualElement`（B=选项2 收口） | lib.rs + 删旧层 | 低 |
| **S7** | 中间层契约测试：fixture 快照 + 多后端一致性断言（见 §7.4） | tests/ | 低 |

### 7.1 过渡期约定（S0–S6）

- S0–S1 阶段，旧 `generator::Document(pages/VisualElement)` **仅作过渡期隔离**（不追求兼容、不保证回归）：S1 起 `legacy-generator` feature 让旧路径仍能编译运行，作为新路径开发时的对照与渐进替换，但**它不是目标，S6 必须删除**。
- S1 需要**输出目标抽象**（见 §7.3）：冻结旧 `finish()` 路径，在其旁新增独立的未分页产出方法，不并行改动核心 `place_text_lines` 以降低风险。
- **双轨制用 feature flag（外部建议 8）**：S1 起旧路径与新路径以 Rust `#[cfg(feature = "legacy-generator")]` 编译期开关隔离，而非运行时条件分支。S6 删旧层时**直接去掉该 feature 及其代码**，干净彻底，避免死代码残留。
- **feature 彻底清理（最终评审 Pro-tip 4）**：S6 删旧层不止删 `src/` 下的 `.rs` 代码，还要**同步清理 `Cargo.toml` 的 `[features]` 定义与 CI 编译矩阵**（如 GitHub Actions 中 `--features legacy-generator` 的 job）。S7 加一条静态检查断言 `Cargo.toml` 不再含 `legacy-generator` 关键字，确保技术债物理级清零、无"幽灵 feature"残留。
- S2 把 PDF 完整迁移到消费 `StructuredDocument` 后，SVG/PNG **在 S5 一并迁移**（同为连续流、无需 paginate），不再依赖旧链路。
- S6 才删除旧层（`legacy-generator` feature 移除），完成 B=选项2 的终点；S7 为贯穿性的契约测试，可随各阶段并行补充，不需单独排期到末尾。

### 7.2 优先级建议（投入产出比）

1. **P0**：S0 + S1 + S2 的「PDF 走 Document」最小闭环 —— 验证架构主线（S6 单独执行，见下）。
2. **P0**：S4 DOCX 后端 —— 用户明确要的新功能，**首期仅"视觉模拟"列表**（外部建议 4）。
3. **P1**：S5 SVG/PNG/HTML 非分页后端统一。
4. **P1**：S3 表格 `MultiSpill` 分页（整行下移 + 重复表头，借鉴 typst §5.5.6）。
5. **P1**：generator 度量阶段做相邻 spacing 合并（借鉴 typst `distribute.rs`，§5.5.3）。
6. **P1**：`col_widths_intent` 列宽意图（HTML `<colgroup>` 响应式宽度，外部建议 9）。
7. **P2**：CJK 避头尾 + 标点挤压（借鉴 typst `inline/shaping.rs` `cjk_punct_style`）。
8. **P2**：首行缩进（孤儿/寡行抑制已随 S2 内置，见 §5.5.4）。
9. **P2**：DOCX **原生多级列表**（`<w:num>`/`<w:abstractNum>`，外部建议 4，S4 之后）。
10. **P3**：全局 Knuth-Plass 断行（仅当中英文混排两端对齐为刚需时）。

> 原方案把 S6 并入最小闭环（P0），会造成 S2 一完成就删旧层、砍掉 SVG/PNG 过渡期。**修订：最小闭环为 S0+S1+S2，S6 单独在 S5 之后执行**（评审意见 3、7）。

### 7.3 S1 复杂度说明（评审意见 12）

现有 `DocumentGenerator` 的 `layout_paragraph`/`layout_table` 等方法**直接往 `page_context` 写 `VisualElement`（带绝对坐标的像素产物）**，`place_text_lines` 更是直接做分页（`finalize_current_page`/`start_new_page`）。因此 S1 **不是"加一条并行路径"**，而是对 generator 核心循环做**输出目标抽象**：同一套度量计算分叉为「写 `VisualElement`」与「写 `Block`」两个目标。故 S1 风险上调为**中**，并在 §7.1 采用「冻结旧路径 + 新增独立未分页产出方法」策略，避免 S1 就动核心 `place_text_lines`。

### 7.4 中间层契约测试（S7，评审意见 13）

三个后端各自独立演进，`Document` 结构演化（加字段/改类型）时极易漏改某一后端。新增契约测试：

- **fixture 快照**：定义覆盖 标题/段落/表格/列表/引用/代码块/图片/嵌套列表 的代表性文档，对 `StructuredDocument` 做序列化快照，`Block`/`DocTextRun` 等类型显式派生 `Debug`/`Clone`/`PartialEq` 以便断言。
- **后端一致性冒烟**：同一 fixture 跑 `Pdf/Html/Docx/Svg/Png` 五个后端，断言均正常生成、不 panic、产物非空。
- **feature 清理静态检查（最终评审 Pro-tip 4）**：S7 增加一条测试，读取并断言 `Cargo.toml` 不再包含 `legacy-generator` 关键字（S6 之后），确保旧层删除时 `[features]`/CI 矩阵也被清干净，无"幽灵 feature"残留。

#### 7.4.1 现有测试的分层处置（不兼容优先，评审修订）

重构将删除旧 `generator::Document`/`VisualElement` 像素层，且**不保证新旧输出兼容**。现有 `tests/` 按阶段分层，处置策略如下（已逐一核对断言层级）：

| 层 | 文件 | 断言性质 | 处置 |
|---|---|---|---|
| **Stage 1 AST** | `stage1_ast/`（3） | 纯 AST/Styled AST 结构断言 | **保留不动**（不碰像素层） |
| **common/** | `common/mod.rs`（1） | lopdf 工具 + Markdown 样本（samples） | **保留不动**（samples 是跨层回归基准） |
| **Stage 2 Generator** | `stage2_generator/`（4） | 大量直接解构 `Page`/`VisualElement` 绝对坐标（`layout.rs` 的 `get_element_bounds`、`pagination.rs` 的坐标重叠断言、`table.rs` 的像素矩形/阅读顺序） | **重写**：把"语义意图"（标题在内容区、文本不重叠、长文档分页、表格有边框、阅读顺序）迁移为基于 `StructuredDocument` 的结构断言 |
| **Stage 3 pdf/png/svg** | `stage3_render/`（3 产物） | 产物级断言（页数/PNG 签名/`<svg>` 标签），仅依赖高层 `markdown_to_*` | **保留**（产物断言不动） |
| **Stage 3 debug_renderer** | `stage3_render/debug_renderer.rs` | 实现 `PageRenderer` trait + 遍历 `page.elements` | **改写**：适配新渲染接口（或删除，其调试价值有限） |
| **E2E pipeline/pdf_validation** | `e2e/`（2） | 行为断言基于 lopdf + 高层 API | **保留**（产物断言不动） |
| **E2E diagnostic** | `e2e/diagnostic.rs` | `test_tasklist_debug_elements`/`diag_trace_url` 直接解构 `Page`/`VisualElement` | **改写或删除**（调试性质，价值低） |
| **lib.rs pipeline_tests** | `src/lib.rs` | 仅依赖高层 `markdown_to_*` | **保留**（不依赖 generator 类型） |

> **关键原则（不兼容优先）**：不再以旧输出为回归基线。借鉴 typst 意味着 PDF 输出会重写，视觉差异是预期内的；旧测试若断言了旧的像素产物（`VisualElement` 绝对坐标），S6 删旧层时**直接删除**，不维护双管线对拍。保留的回归保障是：
> 1. **Stage 2 重写后的语义断言**（在新 `StructuredDocument` 上验证"分页正确、不重叠、顺序正确"等语义意图）。
> 2. **Stage 3 / E2E 产物断言**（页数、文件签名、标签、PDF 文本/链接/页尺寸）作为端到端冒烟。
> 3. **`common/samples` 作为跨层共享样本**，绝不可删。

---

## 8. 对现有文档的影响

- `docs/design.md`：第 2 节「Generator 模块」与第 3 节「Visual 模块」描述的是旧像素产物架构。本方案落地后，需将 `Document` 重定义为「未分页结构化中间层」，并将 `VisualElement` 收敛为 PDF 后端内部绘制原语（不再作为全局中间层）。
- `docs/new-world.md`：其「HTML 作为唯一中间表示、居于 PDF 之前」的方向被本文档取代；该文件可保留为历史参考，但不再作为实现依据。
- 实现过程中，每个阶段完成时应同步更新 `design.md` 的对应小节与「特性支持状态」表（新增 DOCX、表格分页策略变更等）。

---

## 9. 评审意见与修订记录

### 9.1 前两轮评审（14 条，均处置）

| # | 评审意见 | 处置 | 落点 |
|---|---|---|---|
| 1 | `PageConfig` 与 `ast::style::PageConfig` 撞名 | 改名 `DocumentPageConfig` | §3.2 |
| 2 | `ParagraphBlock.runs` 若来自 parley 断行 run，DOCX/HTML 重排会硬拼被切断的行 | 明确 `runs` 来自 styled 层样式切分，`lines` 才是断行结果 | §3.3.1 |
| 3 | SVG/PNG 过渡期处置过轻，S6 会无消费方 | SVG/PNG 并入 S5，与 HTML 共用非分页展平 | §5.4、§7.1、§7.2 |
| 4 | S2 高风险的回归验证手段缺失 | 补 S2 回归基线（后经外部建议 7 改为结构断言） | §7.4 |
| 5 | outline 的 `page_number/x/y` 是分页产物，与"Document 不分页"冲突 | 中间层不带页号，PDF 分页后回填 | §3.2.1 |
| 6 | HTML 输出层与 `ResolvedStyle`/外部 CSS 的关系未澄清 | 决策：纯语义 HTML + 外部 CSS | §5.3 |
| 7 | S2 最小闭环与 S6 捆绑，砍掉 SVG/PNG 过渡期 | 最小闭环改为 S0+S1+S2，S6 单列 | §7.2 |
| 8 | `Table/List/Quote` 递归 `blocks` 与线性 paginate 矛盾 | 决策：保留树形 + PDF 内展平（经外部建议 2 修订） | §4.2.1 |
| 9 | 单元格断行依赖列宽，纯 Document 层无法预计算 | 决策：单元格内不存 `lines`，PDF 按列宽临时断行 | §3.5.1 |
| 10 | 页眉页脚 `{total}` 需分页完成后才可知 | 解析与绘制下沉到 PDF 后端分页收尾后 | §3.2.1 |
| 11 | 中文字体需 `w:eastAsia`，`Vec<String>` 无法区分 | `DocTextRun` 增 `east_asia_font` 字段 | §3.4 |
| 12 | S1"复用 layout 度量"被低估，实为输出目标抽象 | S1 风险调为"中"，冻结旧路径+新增未分页产出 | §7.1、§7.3 |
| 13 | 缺测试策略，三后端易漂移 | 新增 S7 契约测试（fixture 快照+一致性冒烟） | §7.4 |
| 14 | `ImageBlock.data` 内嵌字节的内存策略未定 | 决策：内嵌字节（后经外部建议 10 改用 `bytes::Bytes`） | §3.6.1 |

### 9.2 第三轮外部评审（11 条，均处置）

| # | 评审意见 | 处置 | 落点 |
|---|---|---|---|
| 15 | `lines` 断行强依赖内容宽度，与"格式无关"声明矛盾 | 定义为"参考度量"，绑定 `DocumentPageConfig`；PDF 尺寸变化时基于 `runs` 重排降级 | §3.3.2 |
| 16 | 展平后仅靠 indent 难以重建嵌套 DOM | 改为**保留树形 + PDF 内展平**（DOCX/HTML 用树，PDF 用展平） | §4.2.1 |
| 17 | `DocLine.runs` 与 `runs` 重复拷贝 String，内存膨胀 | `DocTextRun.text` 用 `Arc<str>`；`DocLine` 改 `RunRef`（run_idx+字节区间）视图 | §3.4 |
| 18 | DOCX 原生多级列表（`<w:num>`）极难写且易崩 | S4 降级为"段落缩进+文本符号"视觉模拟；原生列表推 P2 | §5.2、§7.2 |
| 19 | HTML 纯语义 + 外部 CSS 会丢失无法语义映射的样式 | 对无法映射且外部 CSS 无法表达的样式自动回退内联 `style` | §5.3 |
| 20 | SVG/PNG 是否多页语义未界定 | 明确产物为"单张长图"；多页需复用 PDF `paginate` | §5.4 |
| 21 | S2 回归的纯像素对比在 PDF 是 false positive 重灾区 | 改为结构断言为主 + SSIM 95% 阈值 + 指令流对比 | §7.4 |
| 22 | S1 双轨制建议用编译期 feature flag | S1 起用 `#[cfg(feature="legacy-generator")]` 隔离，S6 移除 | §7.1 |
| 23 | 表格列宽 `Vec<f32>` 绝对值无法表达响应式意图 | 补 `col_widths_intent`，标为 P1（S0 不引入） | §3.5 |
| 24 | 图片字节 `Arc<Vec<u8>>` 不如 `bytes::Bytes` | `ImageBlock.data` 改用 `bytes::Bytes`（可选 `Arc<[u8]>`） | §3.6.1 |
| 25 | `PageBreak` 在页首/连续出现会生成空页 | paginate 加防呆：已在页首则跳过 | §4.2 |

### 9.3 架构演进（用户要求，非第三方评审）

| # | 演进 | 处置 | 落点 |
|---|---|---|---|
| 26 | 不考虑兼容，可打破不合适测试 | 删除"旧/新 PDF 管线对拍基线"；按层级处置测试：Stage1/common/Stage3/E2E 保留，Stage2 重写为语义断言，debug_renderer/diagnostic 改写或删除 | §7.4.1 |
| 27 | 生成 `StructuredDocument` 的布局算法借鉴 typst `typst-layout` | 引入两阶段模型：S1 = typst `collect`（产出 Document），S2 = typst `distribute`（PDF paginate）；借鉴 Child 分类/`need` 孤儿寡行/`weak spacing` 折叠/sticky keep-with-next/`MultiSpill` 续页 | §5.5 |

### 9.4 最终评审 Pro-tips（4 条，均已处置）

最终评审批准方案定稿，并提出 4 条落地编码 Pro-tips。

| # | Pro-tip | 处置 | 落点 |
|---|---|---|---|
| 28 | `RunRef` 生命周期与并发：建议改 Owned `Arc<str>` | **保留 `RunRef` 作核心模型**（子区间语义 + PDF 分页单线程推进，无需跨线程）；补 `ParagraphBlock::resolve_run_ref` 便捷解析方法供独立/并发场景 | §3.4 |
| 29 | DOCX 图片用"两遍扫描"：Pass1 收集图片+分配 rId 入 `ImageContext`，Pass2 查 rId 写 `<w:drawing>`，最后统一写 zip+.rels | 采纳，写入 S4 | §5.2 |
| 30 | PDF `flatten` 中 `Table` 是不透明叶子，不继续展平单元格 blocks | 采纳，`TableBlock` 作 `MultiChild`，由 `layout_table` 收到剩余高度后内部闭环 `MultiSpill` | §4.2.1 |
| 31 | feature flag 彻底清理：删旧层时同步清 `Cargo.toml` `[features]` + CI 矩阵；S7 加静态检查断言无 `legacy-generator` | 采纳 | §7.1、§7.4 |

---

## 10. 附录：typst `typst-layout` 详细 layout 实现

本附录逐模块拆解 `typst-0.15.1/crates/typst-layout/` 的布局算法，供实现者对照源码理解 §5.5 的借鉴点。所有文件名均相对于 `crates/typst-layout/src/`。

### 10.1 `flow/mod.rs`：`layout_flow` 与 `Work` 状态机（顶层分页）

typst 分页不是"预先算好所有页"，而是维护一个可推进的状态机，每 `compose` 一页就取走能放下的内容。

```rust
// flow/mod.rs（简化）
struct Work<'a, 'e> {
    children: &'a [Child<'a>],   // 尚未布局的子项（从 collect 来）
    spill: Option<&'a dyn Spillable>, // 上一个可拆块留下的剩余部分
    width: Abs,                   // 已用宽度
    height: Abs,                  // 已用高度
    backlog: ...                  // 用于 sticky/keep 的待定项
    pinned: Vec<PinnedItem>,      // 页眉/页脚/水印锚定
    ...
}

fn layout_flow(ctx, engine, place: bool, regions: Regions, children) -> Fragment {
    let mut work = Work { children, ... };
    let mut output = vec![];
    while let Some(frame) = work.compose(ctx, engine, regions.clone()) {
        output.push(frame);
        if work.is_empty() && !work.has_uncomposed() { break; }  // 终止条件
    }
    Fragment::frames(output)
}
```

**关键机制**：
- **`Work.compose(regions)` 每调用一次产出一页**（一个 `Frame`）。`while let` 循环持续到内容耗尽。
- `children` 切片随着布局不断前移（`advance()`），已布局的子项从切片头部移除。
- **`spill`**：若上一个块是可拆的（`Multi`）且放不下，它 layout 时返回剩余部分存进 `spill`，下一页先处理 spill 再处理 `children`。
- **终止条件**：`is_empty() && !has_uncomposed()`。`Regions` 提供 `may_progress`/`may_break` 标志，防止在无法前进也无法拆分时死循环。

> **对 liepress §4.2 / §5.5.1 的映射**：`Work` = PDF `paginate` 内部的状态（当前页已用高度 + 剩余块序列 + spill），`compose` 每次调用对应"生成一页"。liepress 可直接照搬这个状态机结构。

### 10.2 `flow/collect.rs`：`collect` 预处理（与页面无关）

```rust
// flow/collect.rs（简化）
enum Child<'a> {
    Line(LineChild<'a>),       // 已断行的段落行
    Single(Box<dyn Blocklike>),// 不可拆块（标题、图片等）
    Multi(Box<dyn MultiChild>),// 可拆块（表格、列表）
    Rel(Abs, Weakness),        // 相对间距（含可折叠强度）
    Fr(Fr),                    // 弹性间距（占满剩余空间）
    Placed(Box<PlacedChild>),  // 浮动/绝对定位
}

fn collect<'a>(ctx, engine, regions, flow) -> Vec<Child<'a>> {
    flow.children()
        .filter_map(|child| child.layout(ctx, engine, regions))
        .collect()
}
```

**关键机制**：
- **`LineChild` 在 collect 阶段就 layout 好**：`Par::layout` → `layout_par` → `lines()` 用 `regions.size.x`（列宽）对段落断行，产出 `Vec<LineFragment>`，每个封装为一个 `LineChild`。**断行在 collect 阶段完成，与页面高度无关**。
- `LineChild` 携带 `need`（见 10.3），由行数的 `single_row`/`block_rows` 计算。
- **`Rel(Abs, Weakness)`**：`Weakness`（0..3）表示间距强度，`keep_weak_rel_spacing`/`trim_spacing` 据此决定是否折叠/trim（见 10.4）。
- **`Single` vs `Multi`**：块实现 `Blocklike` trait，`layout` 返回 `(frame, spill)`；`Spill::none()` 表示不可拆（Single），否则可拆（Multi）。

> **对 liepress S1 的映射**：`collect` 就是产出 `StructuredDocument` 的阶段。**`ParagraphBlock.lines` 必须在 S1 生成器里用列宽算好**（对应 `layout_par`/`lines`），块分类（`splittable`）和间距 weakness 也在此收集。这确认了 §5.5.2 的 `splittable` 字段来源。

### 10.3 `flow/distribute.rs`：`distribute`（region 内排版 + 孤儿寡行 + 间距折叠）

```rust
// flow/distribute.rs（简化）
struct distribute(Regions, &mut Work, Frame) -> Vec<Frame>

fn distribute(
    regions: Regions,
    work: &mut Work,
    frame: &mut Frame,
) -> Vec<Frame> {
    let mut next_region = vec![];
    let mut push = true;
    while push {
        push = false;
        for (i, child) in work.children.iter().enumerate() {
            if frame.y + child.height > regions.y_available {
                // 放不下：把后续子项排到下页
                next_region.extend_from_slice(&work.children[i..]);
                work.children = &[];
                break;
            }
            match child {
                Child::Line(line) => {
                    if line.need > regions.y_available - frame.y {
                        // 孤儿/寡行：整行移下页
                        next_region.push(...); work.children = &[];
                        break;
                    }
                    place_line(...);  // 放行
                    work.advance(i + 1);
                }
                Child::Rel(rel, weakness) => {
                    keep_weak_rel_spacing(...);  // 强度折叠
                    place(...);
                }
                Child::Single(block) => {
                    // 不可拆：放不下就整体下移
                    ...
                }
                Child::Multi(block) => {
                    let (part, spill) = block.layout(ctx, engine, Regions::one(...));
                    if spill.is_none() { /* 放得下 */ }
                    else { work.spill = Some(spill); /* 续页 */ }
                    ...
                }
            }
        }
        // 用未用空间放 sticky/页脚等
        if regions.may_progress { ... }
    }
    next_region
}
```

**关键机制**：
- **孤儿/寡行**：`line.need > regions.y_available - frame.y` 时整行下移。`LineChild.need` 由行数决定（见 10.4 的 `need` 计算）。
- **间距弱折叠**：`keep_weak_rel_spacing` 只在 `weakness` 足够强时保留间距，否则在页边界折叠。
- **Single 整体下移**、**Multi 返回 spill 续页**——分页的三种基本动作都在这一个循环里。

### 10.4 `flow/mod.rs`（`Child::layout` 内部）+ `flow/block.rs`：`need` 与孤儿寡行、sticky

**`need`（孤儿/寡行抑制）计算**（`flow/block.rs` 的 `LineChild::new`）：

```rust
// block.rs（简化）
struct LineChild {
    frame: Frame,
    need: Abs,   // 放不下的最小高度 → 整行/整段下移
}

let height = frame.height();
let need = match lines_count {
    1 => height,                       // 单行：整行高度
    2 => height * 2,                   // 两行：两行高度
    _ => height * 3,                   // 多行：三行高度（首+末+中间）
};
```

**sticky（keep-with-next，标题跟随）**：`flow/mod.rs` 里标题实现 `Sticky` trait，布局时把标题"钉"在页底区域，若放不下标题+正文首行，则整个标题+首行移到下页。

```rust
// flow/mod.rs 的 sticky 处理
struct PinnedItem { frame: Frame, size: Size, overlap: bool }
// 页尾：若 title.need（标题高+正文首行高）放不下，则标题整体下移
```

> **对 liepress S2 的映射**：孤儿/寡行（§5.5.4）和标题 keep-with-next（§5.5.5）都在这两个文件，S2 可直接照搬 `need` 的"行数→高度"规则和 sticky 的下移判定。

### 10.5 `flow/compose.rs`：`compose`（region 推进 + 产生页 frame）

```rust
// flow/compose.rs（简化）
fn compose<'a>(&mut self, ctx, engine, regions: Regions) -> Option<Frame> {
    if self.pinned_non_empty() && self.finished() {
        return Some(self.finish_page());   // 产出本页
    }
    let mut frame = self.new_frame(ctx);   // 新空白页（含背景）
    let mut next_regions = self.distribute(ctx, engine, regions.clone(), &mut frame);
    // 若有 spill，下页从 spill 继续；否则从 next_regions 继续
    let mut regions = regions.next().into_next();  // 推进到下一页 region
    // 用 sticky/页脚填充未用空间
    self.pin(ctx, engine, regions.clone(), &mut frame);
    // 若无法前进且无法拆，返回 None 终止
    if !regions.may_progress && !self.regions_may_break {
        return None;
    }
    Some(frame)
}
```

**关键机制**：
- `compose` 每次产出**一页**。`distribute` 填充当前页，`next_regions` 决定下页从哪开始。
- **`regions.next()`**：`Regions` 是 region 序列，`next()` 推进到下一页的尺寸/剩余信息。`may_progress`（能否新增内容）与 `may_break`（能否拆块）双标志防止死循环。
- **页尾填充**：用 sticky/页脚/弹性间距填满当前页未用空间。

> **对 liepress §4.2 的映射**：`compose` 的 `regions.next()` + `may_progress/may_break` 双标志，是 PDF `paginate` 循环的骨架。liepress 需要 `Regions` 这个"下一页"抽象来驱动多页。

### 10.6 `pages/run.rs`：`layout_page_run`（页配置 → 无限 region 序列）

```rust
// pages/run.rs（简化）
fn layout_page_run(ctx, engine, page: &Page) -> Fragment {
    let size = page.size();
    let frame = layout_frame(...);  // 页面外层 frame（含背景、装饰）
    let regions = Regions::repeat(
        frame.size() - frame.size().vary(page.margin.vertical()),  // 内容区高度
        frame.size().x,
        page.margin,
        true,  // last
    );
    let mut fragment = layout_flow(ctx, engine, true, regions, page.content.clone());
    for frame in &mut fragment.frames {
        // 页眉页脚在"每页产出的 frame"上分别 layout
        if let Some(header) = &page.header {
            frame.push_front(layout_header(ctx, engine, header, ...));
        }
        if let Some(footer) = &page.footer {
            frame.push_back(layout_footer(ctx, engine, footer, ...));
        }
    }
    fragment
}
```

**关键机制**：
- **`Regions::repeat(area)`**：把内容区作为**无限重复**的 region 序列 feed 给 `layout_flow`，让它产出一页页内容。
- **页眉页脚**：对 `layout_flow` 产出的每个 frame，**分别** layout 页眉/页脚并 push 到帧顶部/底部。**此时才知道当前是第几页、总页数**——所以 `{page}/{total}` 模板在此展开（印证 §3.2.1"页眉页脚延迟解析"）。

> **对 liepress S2 的映射**：`Regions::repeat` + 逐帧页眉页脚就是 PDF `paginate` 的最终形态。liepress 的 `{total}` 在分页完成后回填，正是这里的做法。

### 10.7 `pages/finalize.rs`：页面 finalize（物理页属性合并）

```rust
// pages/finalize.rs（简化）
fn finalize_frame(ctx, engine, frame: &mut Frame, page, area, target_size, background, ...) {
    // 合并背景、页边距、内容区；仅对"内容" frame 操作，物理页属性在页外层
}
```

**关键机制**：
- finalize 把"内容"与"物理页装饰"（背景、页码、页边距）分开。**页内内容只管布局，页装饰由 `layout_page_run`/finalize 在知道物理页号后合并**。

> **对 liepress §3.2.1 的印证**：这正是"分页产物字段（页号）不下沉到中间层"的 typst 侧佐证。

### 10.8 `inline/`：段落内断行与 CJK 处理

`inline/shaping.rs` 的 `cjk_punct_style`（CJK 避头尾 + 标点挤压）是 §7.2 P2 的借鉴来源：

```rust
// inline/shaping.rs（简化）
fn cjk_punct_style(ctx, engine, ...) {
    // 根据 CJK 标点规则，调整标点两侧的间距（挤压/压缩），
    // 并应用避头尾（不允许行首/行尾出现某些标点）。
}
```

- typst 用 `parley` 做底层断行（`text/shaping.rs`），`cjk_punct_style` 在其上做 CJK 特定的标点间距修正。
- liepress 已用 parley，S2 起可评估直接借鉴该修正逻辑（P2）。

### 10.9 §5.5 借鉴点与 typst 源码映射速查表

| §5.5 借鉴点 | typst 源码 | 算法要点 | liepress 落点 |
|---|---|---|---|
| 两阶段模型 | `flow/mod.rs` `layout_flow` + `flow/collect.rs` + `flow/distribute.rs` | collect（与页无关预处理）→ distribute（region 分页） | S1 产出 Document，S2 做 paginate |
| `Work` 状态机 | `flow/mod.rs` `struct Work` | children 切片前移 + spill + 逐页 compose | §4.2 paginate 骨架 |
| `Regions` 序列 | `flow/mod.rs` `struct Regions` + `pages/run.rs` `Regions::repeat` | `next()` 推进 + `may_progress/may_break` 防死循环 | §4.2 paginate 循环 |
| Child 分类 | `flow/collect.rs` `enum Child` | Line/Single/Multi/Rel/Fr/Placed | §5.5.2 `splittable` + 块分类 |
| 段落断行预计算 | `flow/collect.rs` `layout_par`/`lines` | 用列宽断行，与页高无关 | §3.3 `ParagraphBlock.lines` |
| `need` 孤儿寡行 | `flow/block.rs` `LineChild::new` | 1/2/3 行高度规则 | §5.5.4，S2 内置 |
| sticky keep-with-next | `flow/mod.rs` `Sticky` trait | 标题+首行整体下移 | §5.5.5，S2 内置 |
| 间距弱折叠 | `flow/distribute.rs` `keep_weak_rel_spacing`/`trim_spacing` | `Rel(amount, weakness)` | §5.5.3，spacing 合并 |
| `MultiSpill` 续页 | `flow/block.rs` 可拆块 `layout` 返回 `(frame, spill)` | 放不下返回剩余部分 | §5.5.6，S3 表格 |
| 页眉页脚延迟 | `pages/run.rs` `layout_page_run` + `pages/finalize.rs` | 每页 frame 分别 layout 页眉页脚 | §3.2.1，S2 |
| CJK 标点 | `inline/shaping.rs` `cjk_punct_style` | 避头尾 + 标点挤压 | §7.2 P2 |

---

*文档版本：1.4*
*最后更新：2026-08-12*
*状态：方案定稿（已批准），待 S0 实现（已并入三轮评审修订 + typst 布局借鉴 + 终审 Pro-tips，附录见 §10）*
