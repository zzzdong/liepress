# 重构工作日志：文档层重构（refactor-document-layer.md）

> 本日志记录重构过程中的每一步操作、临时调整、以及**与方案不一致**或需要后续澄清的地方。
> 每次开工/变更后追加新条目，保持时间倒序（最新在上）或正序（最新在末尾）。本文采用**正序**（最新追加到末尾）。
>
> 关联分支：`refactor/document-layer-s0`（S0 起）
> 方案文档：`docs/refactor-document-layer.md`

---

## 2026-08-12 — S0 类型骨架搭建

### 分支
- 新建并切换到分支 `refactor/document-layer-s0`（基于 `main`）。

### 新增文件
- `src/document/mod.rs`：文档层模块根，声明 `types` 与 `skeleton` 子模块，说明跨层单向依赖策略与阶段进度（S0/S1/S2/S3）。
- `src/document/types/mod.rs`：逻辑类型模块声明，重新导出 `DocColor`/`DocImage`/`ResolvedStyle`/`TextDecoration`/`TextAlign`/`WhiteSpace`/`ObjectFit`/`DocTextLine`/`DocTextRun`/`DocGlyph`。
- `src/document/types/color.rs`：`DocColor`（RGBA），与 `visual::Color` 双向 `From`。
- `src/document/types/style.rs`：
  - `ResolvedStyle`：投影自 `ast::Style` 的布局/分页稳定子集（字体、margin/padding/border、尺寸、对齐、装饰等）。
  - 投影枚举 `TextAlign`/`WhiteSpace`/`ObjectFit`/`TextDecoration`，与 `ast` 同名类型双向 `From`。
  - 实现 `From<ast::Style> for ResolvedStyle`（仅取文档层需要的字段）。
- `src/document/types/text.rs`：`DocTextRun`/`DocTextLine`/`DocGlyph`，从 `text::TextRun`/`TextLine`/`Glyph` 投影；字体字节以 `Vec<u8>` 自包含提取。
- `src/document/types/image.rs`：`DocImage`，持有图片原始字节 `Vec<u8>`，替代 `VisualElement::Image` 的加载路径依赖（仅接受 `Image` 变体）。
- `src/document/skeleton/mod.rs`：`DocumentSkeleton`/`SkeletonPage`/`SkeletonBlock`/`BlockKind`/`TableRow`/`TableCell`/`SkeletonHeaderFooter`，覆盖方案表 5.1 全部节点类型，带 `text_content()`。

### 修改文件
- `src/lib.rs`：在 `css`/`error`/`generator` 之间新增 `pub mod document;`（含注释指向方案文档）。

### 与方案不一致 / 临时调整（重要）
1. **未引入 `bytes` crate**：方案中 Skeleton 图片/字体提到"可作为 Bytes 或路径"，实际采用 `Vec<u8>` 表示二进制（不新增依赖）。如后续确需 `bytes::Bytes`，可整体替换。
2. **`From<ast::Style>` 投影的是"稳定子集"**：`ResolvedStyle` 未 1:1 搬运 `ast::Style` 全部字段（如 `display`、`font_weight` 枚举本身、`font_style` 枚举本身被化简为 bool/保留需要的派生值）。若 S1/S3 投影阶段需要原始枚举值，需扩展 `ResolvedStyle` 或新增字段。
3. **`DocTextRun.font_name` 暂为 `None`**：S0 仅从 `TextRun.font_data`（parley `FontData`）提取字节，无法反推字体族名。S3 投影到 `VisualElement` 时若需按族名回退，需结合 `text::FONT_BYTES` 映射补全（当前 `from` 实现未在 `with_font_context` 中查名）。
4. **`DocImage.object_fit` 在 `From<&VisualElement>` 中硬编码为 `Contain`**：`VisualElement::Image` 本身未携带 `object_fit`（该信息在 ast/style 层），投影时先给 `Contain`，S3 应改由所属块样式填入。
5. **`ResolvedStyle` 中的 `background_color` 对于 `VisualElement::Image` 未填充**（保持 `None`）；方案中 `DocImage.background` 也暂留 `None`。图片边框/背景色在 S3 由样式注入。
6. **`SkeletonBlock.splittable` 由构造时显式传入**（默认构造只给 `false`/`true` 语义），S0 未自动推导；S2 分页需结合 `BlockKind` 自动判定（如 Paragraph/List 可分割，Table/Image/Heading 不可）。
7. **`SkeletonHeaderFooter` 仅存文本模板与字号/对齐**，实际坐标在 S2 计算；模板变量 `{page}`/`{total}` 在 S2 替换。
8. **`ResolvedStyle` 新增 `baseline_shift`**：方案中 ast 已含该字段（上下标），这里一并投影，确保行内上下标在 S3 可还原。

### 验证
- `cargo build --lib`：通过。
- `cargo test --lib`：78 passed，0 failed（无回归）。
- `cargo clippy --lib`：无警告。

### 下一步
- **S1**：实现 `from_ast`，从 `ast::Node` 构建 `DocumentSkeleton`（AST → Skeleton 转换链）。
- **S2**：分页与绝对坐标计算。
- **S3**：Skeleton → `VisualElement` 投影，并迁移 `generator` 使用新层。

---

## 2026-08-12 — S1 AST → Skeleton 转换

### 新增文件
- `src/document/from_ast.rs`：实现 `ast_to_layout(root: &Node, settings: &PageSettings) -> DocumentSkeleton`。
  - 递归 `convert_node`：遍历 `ast::Node` 树，映射为 `SkeletonBlock` / `BlockKind`。
  - `layout_inline(children, style, settings) -> Vec<DocTextLine>`：**复用** `generator::text::collect_inline_segments`、`text::layout_text_with_contexts`（在 `FONT_CONTEXT`/`LAYOUT_CONTEXT` 中）、`generator::text::annotate_runs_with_urls`，将内联排版结果投影为 `DocTextLine`。
  - 单元测试：`test_ast_to_layout_structure`（Document>Heading/Paragraph/CodeBlock 结构 + text_content）、`test_ast_to_layout_nested_list`（List>ListItem>Paragraph）。

### 修改文件
- `src/document/mod.rs`：新增 `pub mod from_ast;`。

### 与方案不一致 / 临时调整（重要）
1. **行内语义节点不生成独立 SkeletonBlock**：`Strong`/`Emphasis`/`Delete`/`Subscript`/`Superscript` 的语义完全由排版后的 `DocTextRun`（font_weight/italic/decoration/baseline_shift）承载，与旧管线一致；未新增对应 `BlockKind`。这是 S0 `BlockKind` 表 5.1 未列这些节点的延续。
2. **`<center>`/`<div>`/`<span>` 统一映射 `BlockKind::Container`**：`center` 的居中 alignment 体现在该块 `ResolvedStyle.text_align`，未额外标记（方案中表 5.1 的 "center" 未单列）。若需后续区分，可加 `Container` 的 `align_hint` 字段。
3. **列表项 marker 与缩进未处理**：`ListItem`/`TaskListItem` 在 S1 仅递归子块，未生成有序序号/圆点 marker，也未做左缩进。旧管线在 `layout_list_item_first_child` 处理；S2 分页阶段需补全（记录为 S2 待办）。
4. **块级 `Image` 不加载字节**：`DocImage` 的 `data` 为空、`pixel_size=(0,0)`、`format="png"` 占位，仅从 `style.width/height` 取显示尺寸。实际字节加载推迟到 S3 投影阶段（仍需外部加载器，可能新增 `image_loader` 依赖或复用现有加载逻辑）。
5. **可用宽度取整页内容宽度**：`layout_inline` 用 `settings.content_width()`。该值已扣除 `margin_left_pt + margin_right_pt`（`generator::types::PageSettings::content_width` 定义），即为纯文本可排版宽度，与方案"collect 阶段只依赖内容宽度断行"一致。S2 仅按 `content_height()`（页面高度）切页，**不重新排版**，故此处无返工风险。块级 padding/border 在 S2 计入占用时不改变文本区宽度（已在 content_width 之外）。
6. **`align=Justify` 降级为 `Left`**：与旧管线 `layout_paragraph` 一致（vello parley 不支持两端对齐），在 `layout_inline` 内映射。
7. **根 Document 直接展平到单页**：`ast_to_layout` 把根 `Document` 的 children 直接放入 `SkeletonPage.blocks`，`SkeletonPage` 的 `header`/`footer` 留 `None`，分页与页眉页脚在 S2 处理。
8. **`SkeletonBlock.anchor` 暂置 `None`**：S2 目录/交叉引用阶段按 heading 等填充。
9. **`convert_node` 对内联节点顶层出现兜底**：内联节点（含 `TableRow`）若作为块级顶层出现（理论上不应），包裹成 `Paragraph`。`TableRow` 仅作为 `Table` 子节点在 `filter_map` 中处理，不会走到此分支（仅满足 match 完备性）。

### 验证
- `cargo build --lib`：通过。
- `cargo test --lib`：80 passed（新增 2 个 from_ast 测试，无回归）。
- `cargo clippy --lib`：无警告。

### 下一步
- **S2**：分页与绝对坐标计算（含列表 marker/缩进、box model 宽度收缩、页眉页脚、anchor 填充）。需先评估 S1 提前排版导致的宽度重排返工风险（见上）。
- **S3**：Skeleton → `VisualElement` 投影，并迁移 `generator` 使用新层。

> **S0/S1 设计修正（在 S2 实施时对齐方案）**：S0 最初把 `DocumentSkeleton { pages: Vec<SkeletonPage> }` 定义为"已分页"形态、S1 也直接把根 children 塞进单页 `SkeletonPage`。这与方案 §4.1「Document 不知道页、分页在消费侧」冲突。S2 时将其修正为：
> - 源 IR：`DocumentSkeleton { blocks: Vec<SkeletonBlock> }`（不分页）。
> - 分页产物：新增 `PaginatedDocument { pages: Vec<PositionedPage> }`、`PositionedPage { blocks: Vec<PositionedBlock>, header, footer }`、`PositionedBlock { block, x, y, height }`。
> - 原 `SkeletonPage` 类型重命名为 `PositionedPage` 复用。S1 的 `ast_to_layout` 同步改为返回 `{ blocks }`。相关测试已更新。

---

## 2026-08-12 — S2 分页与坐标计算

### 新增文件
- `src/document/paginate.rs`：实现 `paginate(skeleton: &DocumentSkeleton, settings: &PageSettings) -> PaginatedDocument`。
  - `flatten`：递归展平块树为线性 `FlatItem`（带 `indent`）。按方案 §4.2.1，Table 为**不透明叶子**（不展开内部 `TableCell`），List/Blockquote/Container/Document 深入。
  - `measure`：按块自带度量（段落=行高和；CodeBlock=行数×行高；Image=size.1；Table=行数×行高；ThematicBreak=边框+余白）。
  - 分页主循环（带 `VecDeque` 队列处理段落按行切的回溯）：线性推进 `content_height`，不可分块整块下移，可分页的 `Paragraph` 按 `DocTextLine` 切（`split_paragraph_lines`）。
  - `height_unlimited` 模式：单页顺序放置（HTML/DOCX 用）。
  - 页眉/页脚：从 `settings.header/footer` 模板替换 `{page}`/`{total}`。
  - 单元测试：`test_split_paragraph_lines_basic`、`test_measure_paragraph`、`test_paginate_height_unlimited_single_page`、`test_paginate_multi_page_and_line_split`、`test_paginate_header_footer_template`。

### 修改文件
- `src/document/skeleton/mod.rs`：`DocumentSkeleton` 由 `{ pages }` 改为 `{ blocks }`（不分页源 IR）；`SkeletonPage` 重命名为 `PositionedPage`（分页产物），新增 `PositionedBlock`、`PaginatedDocument`；注释更新为「源 IR 不分页」语义。
- `src/document/from_ast.rs`：`ast_to_layout` 返回 `DocumentSkeleton { blocks: root children }`（不再造 `pages`）。测试断言 `pages[0].blocks` 改为 `blocks`。
- `src/document/mod.rs`：新增 `pub mod paginate;`。

### 与方案不一致 / 临时调整（重要）
1. **S0 的 premature 分页结构在 S2 已修正**（见上方 S0/S1 修正说明），现源 IR 不分页，符合方案。
2. **表格作为不透明叶子整块放置**：方案 §4.2.1 要求 Table 不透明，但分页时若单张表格超过一整页，当前直接强制放置并换页（无"整行下移 + 重复表头"逻辑）。表格内跨页切分（表头重复）标记为 **S2 未覆盖**，留待 S3 或更后补全。
3. **列表 marker/缩进未生成**：S2 仅对 List/Blockquote 做了 `indent` 缩进（LIST_INDENT=20pt、QUOTE_INDENT=16pt），但**未生成有序序号/圆点 marker**，也未在 `PositionedBlock` 上标记 marker。旧管线在 `layout_list_item_first_child` 处理；此项为 S2 已知缺口（记录，S3 投影时需补全或回 S2 补）。
4. **`anchor` 未填充**：`SkeletonBlock.anchor` 仍为 `None`（S0 预留），目录/交叉引用在更后阶段填充，本阶段未涉及。
5. **box model 宽度已在 S1 解决**：`content_width()` 已扣 margin，S2 仅用 `content_x()` 作起始 x，不重新排版（与方案一致，无返工）。
6. **`PositionedBlock.height` 为测量高度**：用于调试/后续，实际绘制在 S3；当前未校验 `x + width` 是否溢出右边界（文本行宽由 S1 排版保证 < content_width，故不溢出）。
7. **`measure` 的段落高度用 `DocTextLine.line_height` 之和**：与 S1 排版产出一致；若后续 S1 改变行高来源，measure 需同步。

### 验证
- `cargo build --lib`：通过。
- `cargo test --lib`：85 passed（新增 5 个 paginate 测试，无回归）。
- `cargo clippy --lib`：无警告。

### 下一步
- **S3（按方案 §7 实际定义）**：表格分页改 MultiSpill（整行下移 + spill 续页），落在 `document::paginate` 的表格分支。
  - 需补全 S2 缺口：表格跨页切分 + 表头重复、列表 marker 生成（见上 #2/#3）。
  - 投影时加载块级 `Image` 字节（S1 占位 `data` 为空）。

---

## 2026-08-12 — S3 表格分页 MultiSpill（按方案 §5.5.6 / §7）

> **范围澄清（用户确认）**：经与用户核对方案 §7 阶段表，S3 的正式定义是
> 「表格分页改 MultiSpill（返回片段 + spill 续页，整行下移 + 重复表头）」，
> 落在 PDF 后端的 paginate 内。我此前 S2 日志末尾的「下一步」误将 S3 记为
> 「Skeleton→VisualElement 投影 + 迁移 generator」（那实为方案 §5.1/§6 的更大范围，
> 非 S3）。本次按方案原意落地：在 `document::paginate` 中把 Table 从「整体下移」
> 升级为「按整行切分 + spill 续页」，续页片段用 `is_continuation` 标记，
> 重复表头由 PDF 渲染阶段据 `is_continuation` 决定（符合方案 §3.5.1）。

### 新增 / 修改文件
- `src/document/skeleton/mod.rs`：
  - `BlockKind` 新增 `TableFragment { header_rows, body_rows, column_align, is_continuation }`，
    承载表格跨页 spill 续页片段；`is_continuation=true` 表示续页（非首片段）。
  - `text_content()` 补充 `TableFragment` 分支（拼接 header+body 单元格文本）。
  - 模块顶部阶段说明 S3 改为「表格 MultiSpill」。
- `src/document/paginate.rs`：
  - `measure` 对 `Table` / `TableFragment` 改为**逐行累加**（`rows.len() * row_h`，并 `.max(1.0)` 兜底行高，避免未初始化 style 行高为 0 导致整表被误判「放得下」）。
  - 主循环新增 **Table 分支**：逐行累积高度，当前页放不下的行生成 `TableFragment { is_continuation:true, header_rows: 表头首行候选 }` 续页；spill 时先 `pages.push(take(cur))` 定稿当前页再重置 `used=0`（**关键修复**：原代码只 `cur.blocks.push` 从不换页，导致多页内容全堆到同一页）。
  - 主循环新增 **TableFragment 分支**：与 Table 同构，按行切分续页片段，`is_continuation` 透传，spill 同样定稿换页。
  - 新增测试辅助 `table(n)` / `fragment_body_rows(page)` / `page_has_continuation(page)`，以及 2 个测试：
    - `test_paginate_table_multispill`：12 行表格在 `content_height=50pt`（5 行/页）下分页为 3 页（5+5+2），续页片段 `is_continuation=true`。
    - `test_paginate_table_followed_by_paragraph`：表格占满页1后段落排到页2。

### 与方案不一致 / 偏离（需后续澄清）
1. **表格切分下推到 Document 层**：方案 §4.2.1（Pro-tip 30）+ §3.5.1 的原意是
   「Table 是不透明叶子，由 `layout_table` 收到『内容区剩余高度』后内部闭环 MultiSpill，
   列宽计算与单元格断行在 PDF paginate 临时进行」。但由于当前 `document::paginate` 是
   **唯一的 paginate 实现**（PDF 后端尚未消费 `PaginatedDocument`），S3 把「按整行切分 +
   spill」这一粗粒度分页点建模到了 `document::paginate`，`is_continuation` 标记供 PDF 渲染阶段
   决定重复表头。**这是临时中间态**：待方案 §5.1/§6 的 PDF 后端重写消费 `PaginatedDocument` 时，
   真实列宽计算 + 单元格断行 + 整行下移 + 重复表头 应由 PDF paginate 按剩余高度临时完成，
   届时 `document::paginate` 的表格行级切分可作为粗粒度分页点保留或被接管。
2. **重复表头由 PDF 阶段负责**：S3 未在 Document 层实际绘制重复表头行，仅通过
   `TableFragment.header_rows`（携带表头首行候选）+ `is_continuation` 提供数据钩子。
   方案明确重复表头是 PDF 渲染责任，本阶段不越层。
3. **单元格多行文本未预存（符合方案）**：`measure` 用「行数×估计行高」粗粒度度量，
   单元格内真实断行（parley）在 PDF 阶段按列宽临时进行（方案 §3.5.1），故表格分页点
   为「整行」粒度，非「单元格内行」粒度。超大单元格（单行超整页）未做 break-glass 保护，
   存在理论死循环（k==0 且 used==0），待真实度量阶段处理。
4. **不跨页表格保留为 `BlockKind::Table`**：整表能放入一页时走主循环「放得下」分支，
   直接 push 原 `Table`（非 `TableFragment`）。消费端需同时兼容 `Table` 与 `TableFragment` 两种表示。

### 验证
- `cargo test --lib document::`：9 passed（新增 2 个表格 spill 测试，无回归）。
- `cargo test`：全量 lib(85) + 集成(10) + doctest(9) 全部通过。
- `cargo clippy --all-targets`：仅剩 3 个 `field_reassign_with_default` 风格警告（既有测试同风格，非错误）。

### 下一步
- **S2 缺口现状（2026-08-12 已部分闭环）**：列表 marker 已在本次 S2 收尾补全（见下）；
  块级 `Image` 字节与 `anchor` 因 AST 层限制（Image 仅含 `src` 路径无字节、Node 无 anchor 字段）
  **无法在 `document` 层补**，推迟到渲染后端（PDF/DOCX/HTML 按 `src` 加载字节）与目录功能
  （需先扩展 AST 增加 anchor）阶段。
- **S3 完成后真正的下一阶段（方案 §5.1/§6）**：让 PDF/HTML/SVG 等后端消费 `PaginatedDocument`，
  建立 `Skeleton/PaginatedDocument → VisualElement` 投影，迁移 `generator`，并在此过程中
  用真实度量（parley 列宽/断行）接管表格 MultiSpill 的精细分页与重复表头。

---

## 2026-08-12 — S2 收尾：列表 marker 注入（方案 §3.6）

> 范围：S0–S3 完成后，补全 `document` 层中唯一可在本层闭合的 S2 缺口——列表项 marker。
> 依据方案 §3.6「`Listitem { blocks, marker: String }`」，marker 应预存在 Block 里。

### 新增 / 修改文件
- `src/document/skeleton/mod.rs`：
  - `BlockKind::ListItem` 增加 `marker: String` 字段；`BlockKind::TaskListItem` 增加 `marker: String`（复选框符号 `"☐ "`/`"☑ "`）。
  - `text_content()` 的 match 给 `ListItem`/`TaskListItem` 补 `..`（不影响文本拼接）。
- `src/document/from_ast.rs`：
  - `List` 分支在转换 children 时按 `ordered/start` 注入 marker：有序 `"N."`（`start` 默认 1，逐项 +1），无序 `"•"`；嵌套列表由内层 List 节点递归重新计数。
  - 新增 `convert_list_item(node, marker, settings)` 辅助：仅对 `ListItem`/`TaskListItem` 填充给定 marker，其余类型透传 `convert_node`（保证嵌套段落/内联节点行为不变）。
  - 顶部模块注释更正：列表 marker 已在 S1 注入（原注释误写「S2 补全」）；图片字节说明改为「AST 仅含 `src` 路径，真实字节由渲染后端加载（方案 §3.5.1）」。
  - 新增测试 `test_ast_to_layout_list_marker`：验证有序 `1.`/`2.` 与无序 `•` 注入正确。

### 与方案不一致 / 偏离
1. **marker 生成位置**：方案 §3.6 仅说明 IR 含 `marker`，未指定由 `from_ast` 还是渲染端生成。
   本次选择在 `document` 层（`from_ast`）预生成（符合 §3.6「预存在 Block 里」的语义），
   渲染端（PDF 缩进+符号模拟 / HTML `<ol><ul>` / DOCX `w:numPr`）直接消费该字符串。
2. **图片字节 / anchor 明确推迟**：经核查 AST 数据模型，`NodeKind::Image` 仅含 `src: String` 路径、
   `Node` 无 anchor 字段，故二者**不能在 `document` 层补**——
   - 图片字节：由渲染后端按 `src` 加载（方案 §3.5.1）。
   - anchor：需先扩展 AST（增加 anchor 字段）再在 `from_ast` 提取，属目录功能（方案 §5.x），本次不做。

### 验证
- `cargo test --lib document::`：10 passed（新增 marker 测试，无回归）。
- `cargo test`：全量 lib(85) + 集成(10) + doctest(9) 全部通过。
- `cargo clippy --all-targets`：仅剩 3 个 `field_reassign_with_default` 既有风格警告，无新增、无错误。

### 下一步
- `document` 层 IR 现已符合方案 §3.6 完整定义（含 marker）。
- 按方案 §7 阶段表，S0–S3 已完成，`document` 层数据与分页闭环就绪。
- **下一个正式阶段为 S4（DOCX 后端）**：需新增 `zip` + XML 生成依赖，让 DOCX 后端直接消费
  `DocumentSkeleton`/`PaginatedDocument`（不投影到 `VisualElement`，符合方案 §5.2「DOCX 走原生结构」）。
  是否启动 S4，需用户确认（涉及新增 crate 依赖与较大后端实现）。

---

## 2026-08-12（续）— 方向修正：Document 层只做源 IR，分页下放到输出后端；删除旧 generator/visual 像素链路

> **重要方向修正（用户确认）**：此前 S0–S3 日志中把「分页（PaginatedDocument / TableFragment / 行级切分）」
> 放在 `document` 层，这与方案 §4.1「Document 不知道页、分页在消费侧」以及用户明确的
> 「document 层做 IR，不做分页；分页是各自的输出后端（PDF/DOCX）的责任，它们不一样」诉求冲突。
> 本次据此**推翻并重做**前述 S2/S3 的分页实现，确立最终架构：
>
> - **`document` 层只产出不分页的源 IR**（`DocumentSkeleton { blocks }`）。
> - **分页是输出后端的责任**：PDF 后端 `render::pdf::paginate_skeleton` 内部自管分页；
>   DOCX 后端（后续）同样自管，两者策略可不同。
> - **删除旧像素链路**：`generator/`（block/box_model/table/image/text/...）、`visual.rs`
>   （含 `VisualElement`/`Color`）、`render/svg.rs`、`render/pixmap.rs` 一并删除，
>   PDF 后端直接消费 `DocumentSkeleton` + 已投影的 `DocTextLine`（parley 字形坐标）。

### 删除的文件 / 模块
- `src/generator/`（整目录）：`block.rs` `box_model.rs` `constants.rs` `context.rs`
  `header_footer.rs` `image.rs` `mod.rs` `table.rs` `text.rs` `types.rs`。
- `src/visual.rs`：旧像素布局类型（`VisualElement`/`Color`/矩形坐标），已被 `document` 层 + `color.rs` 取代。
- `src/render/svg.rs` `src/render/pixmap.rs`：旧 SVG/PNG 像素渲染后端。
- `src/document/paginate.rs`：`DocumentSkeleton → PaginatedDocument` 的分页实现（改由后端负责，删除）。
- `src/document/skeleton/mod.rs` 中 `PaginatedDocument`/`PositionedPage`/`PositionedBlock`/`TableFragment`
  及 `SkeletonBlock.anchor`、`BlockKind::TableFragment` 一并移除（源 IR 不再含分页概念）。

### 新增 / 重写文件
- `src/color.rs`：**中立** `Color { r,g,b,a }`，替代被删的 `visual::Color`（避免与文档层语义耦合）。
  - `new(r,g,b)` 3 参（不透明）、`with_alpha`、`black()`、`From<DocColor>`/`From<Color>`。
- `src/document/types/page.rs`：`PageSettings`（分页后端输入），含 `From<PageConfig>` 与页面常量
  （A4 `PAGE_WIDTH_PT=595.276` / `PAGE_HEIGHT_PT=841.89` 等）。`document/types/mod.rs` 重新导出。
- `src/document/types/color.rs`：`DocColor` 改为使用 `lievisual::Color`；`From<Color>` 保留在此，
  `From<DocColor>` 仅在 `color.rs` 实现（避免 E0119 冲突）。`DocColor` 派生 `Default`。
- `src/document/types/style.rs`：手写 `impl Default for ResolvedStyle`（font_size_pt=10.5、line_height_pt=15.0 等）。
- `src/document/types/text.rs`：`DocGlyph` 增加 `cluster: u32`（字节簇偏移，相对该 Run 自身 text）；
  `From<&text::Glyph>` 保留 cluster。
- `src/document/from_ast.rs`：去除分页残留（无 `anchor`、无 `TableFragment`）；
  内联 `collect_inline_segments` + `annotate_runs_with_urls`（原来自 `generator::text`）；
  `ast_to_layout(root, settings)` 返回不分页 `{ blocks }`。`column_align` 经 `DocTextAlign::from` 投影。
- `src/text.rs`：
  - `Glyph` 增加 `cluster: u32`（相对 run_text 的字节簇）。
  - 字形采集改用 `glyph_run.run().visual_clusters()` 配对 `Cluster::text_range().start` 获取簇字节偏移，
    再与 `positioned_glyphs()` 坐标一一配对（parley 0.11 `Glyph` 无 cluster 字段，必须这样取）。
  - **关键修复**：`TextRun.text` 保留**整段** `full_text`（而非 `run_text` 切片），使 `glyph.cluster`
    的全局字节偏移可直接用于 krilla（避免「cluster 越界 / string of length N」panic）。
  - `Color::BLACK` → `Color::black()`（改用中立 `color::Color`）。
- `src/render/pdf.rs`：**完全重写**为消费 `DocumentSkeleton` 的后端。
  - `PdfRenderer`：`new(surface, content_w, settings)`；持有 `font_cache` 与 `links`。
  - `doc_run_to_text_run`：从 `DocTextRun`（已含 parley 字形坐标）投影回 krilla `TextRun`
    （重建 `parley::FontData` 从字体字节、glyph 坐标/cluser/color 直接复用，不重新排版）。
  - `draw_doc_lines` / `draw_block`：按 `BlockKind` 绘制（Heading/Paragraph/List/TaskListItem/Blockquote/
    CodeBlock/ThematicBreak/Image/Table/...）。`Table` 走 `draw_table`（含跨页续页 `repeat_table_header`）。
  - `paginate_skeleton(skeleton, settings) -> Vec<PdfPage>`：后端自管分页；
    `paginate_table` 抽成 `PaginateCtx`（聚合 `pages/cur/used`，消除 clippy 参数过多告警）。
  - `PdfDocumentGenerator::from_skeleton(skeleton, settings).generate() -> Vec<u8>`：对外入口。
  - `PositionedBlock` 保留 `height` 字段（`#[allow(dead_code)]`，供调试/后续）。
- `src/render/mod.rs`：仅 `pub mod pdf; pub use pdf::PdfDocumentGenerator;`（SVG/PNG 后端已删）。
- `src/bin/liepress.rs`：移除 `Format::Svg`/`Png` 及对应分支，仅保留 `Pdf`/`Html`（旧像素链路删除）。
- `src/lib.rs`：`pub use render::PdfDocumentGenerator`；`html_to_document` → `html_to_layout`；
  重写 pipeline（`markdown_to_pdf` 等）走 `html_to_layout → from_ast::ast_to_layout → render_pdf`；
  移除所有 svg/png API 入口。新增 `HtmlError`/`RenderError` 到 `error.rs`。

### 测试
- 删除 `tests/stage2_generator/`、`tests/stage3_render/`（针对旧像素链路）、
  `tests/stage2_generator_tests.rs`、`tests/stage3_render_tests.rs`、`tests/e2e/diagnostic.rs`
  （依赖 `markdown_to_svg`/`markdown_to_png`/`debug_renderer`）。
- 重写 `tests/e2e/pipeline.rs`：仅测 PDF 全链路（`markdown_to_pdf` + `assert_valid_pdf` + 页数），
  覆盖富文本、Unicode、特殊字符、嵌套列表、表格、引用、代码块等。
- 保留 `tests/stage1_ast/`、`tests/e2e/pdf_validation.rs`、`tests/e2e/mod.rs`（去除 diagnostic 模块声明）。

### 验证
- `cargo build`：通过，零 warning。
- `cargo test`：lib 80 + 集成 21 + 单元测试 28 + doctest 9，**全部通过**。
- `cargo clippy --all-targets`：零 warning。
- 实际产物：`liepress --input sample.md --output out.pdf` 生成 1 页合法 PDF（26KB）。
  （parley ICU4X 对日语 `ja` 缺分词模型仅告警、不影响英文/中文输出。）

### 结论
- **PDF 全链路已打通**：`Markdown/HTML → AST → DocumentSkeleton（源 IR，不分页）→ PDF 后端内部 paginate + draw → PDF 字节`。
- **旧像素链路（generator/visual/svg/pixmap）已彻底删除**，架构回归方案 §4.1 本意。
- **DOCX 后端按用户要求暂缓**（不在此 PR 范围）；其分页同样由 DOCX 后端自管，复用同一 `DocumentSkeleton`。

### 下一步（用户决策）
- 是否启动 DOCX 后端（S4）：需新增 `zip` + OOXML/quick-xml 依赖，消费 `DocumentSkeleton` 自管分页与原生结构。
- 可选增强（不影响 PDF 链路）：列表 marker 缩进绘制、表格列宽真实度量（parley）、单元格内断行、图片字节加载。

---

## 2026-08-13 — 管线重组：按 pipeline 阶段拆分模块 + Markdown 直连 HTML AST

> **背景 / 计划 vs 实际**：此前架构已确立 `Document` 层只做源 IR、分页在输出后端（见 2026-08-12 续）。
> 但上游「输入→HTML AST→语义树」这一段仍堆在 `src/html`（含 `ast.rs` 解析、`parser.rs` 解析、
> `styled.rs` 语义树、`style_resolver.rs` 样式、`md_converter.rs` 转换、**职责过多**），用户指出
> `src/html` 让人困惑，应按 **pipeline 阶段** 重组；并进一步评估「Markdown 应**直接**到 HTML AST，
> 而不是先序列化成 HTML 字符串再二次解析」。本次**一步到位**执行了目录重组 + 直连 DOM 两条改动。

### 模块重组（src/html → src/dom + src/output）
- 删除 `src/html/`（整目录）：`mod.rs` `ast.rs` `parser.rs` `style_resolver.rs` `styled.rs` `md_converter.rs`。
- 新增 `src/dom/`（管线 Layer 1：HTML AST 层）：
  - `mod.rs`：原 `src/html/ast.rs` 重命名为 `src/dom/mod.rs`，`HtmlDocument`/`HtmlElement`/`HtmlNode`/`HtmlTag` 定义与操作方法全部迁入。
  - `parser.rs`：原 `src/html/parser.rs`（html5ever 纯字符串 → `HtmlDocument`）。
  - `style_resolver.rs`：原 `src/html/style_resolver.rs`。
  - `to_ast.rs`：原 `src/html/styled.rs`（`HtmlDocument + CssEngine → ast::Node` 语义树）。
  - `markdown.rs`（**新增**）：pulldown-cmark 事件流**直接**构建 `HtmlDocument`，不经过中间 HTML 字符串。
- 新增 `src/output/`（管线输出侧）：
  - `mod.rs`：声明 `html` / `md_converter` / `pdf`。
  - `html.rs`：原 `src/html/node_to_html.rs`（`ast::Node` → 自包含内联样式 HTML）。
  - `md_converter.rs`：原 `src/html/md_converter.rs`，但逻辑已改为走 styled AST（见下）。
  - `pdf.rs`：原 `src/render/pdf.rs` 迁移（消费 `DocumentSkeleton`，内部自管分页）。
- 删除 `src/render/`：仅含 `pdf.rs`/`mod.rs`，已整体并入 `src/output/`。
- `src/lib.rs`：模块声明改为 `pub mod dom; pub mod output;`（移除 `html`/`render`）；相应 re-export 路径更新。

### Markdown 直连 HTML AST（消除字符串往返）
- 新增 `src/dom/markdown.rs::markdown_to_dom(md) -> HtmlDocument`：
  - pulldown-cmark `Event` 流直接映射为 `HtmlElement`/`HtmlNode`（栈式写入），仅遍历一次。
  - 等价于 `parse_html(&markdown_to_html(md))` 但无序列化/反序列化开销、文本保真度更高。
  - `inline_local_images(doc, base_dir)`：就地把本地图片内嵌为 base64 data URI（供 PDF 字节自包含）。
- `src/ast/mod.rs`：`build_engine_and_parse` 与 `parse_markdown_with_resolver` 改为先 `dom::markdown_to_dom` 再 `dom::to_ast::html_to_styled_nodes`，**不再** `markdown_to_html` → `parse_html` 往返。
- `src/lib.rs` 入口：`markdown_to_pdf`/`markdown_file_to_pdf` 走 `markdown_to_dom` + `inline_local_images`；
  `html_to_pdf`/`html_file_to_pdf` 走 `parse_html` 后再 `html_to_layout`（输入二选一，汇合到同一 `HtmlDocument`）。

### 统一 HTML 输出路径（含方案偏离）
- `markdown_to_html_document` 改为：先 `parse_markdown_with_css`（得到 styled `ast::Node`）→ `node_to_html`（由 `Style::to_inline_css` 写出 `style="..."`），导出**自包含、带样式**的 HTML。
- `src/ast/style.rs` 新增 `Style::to_inline_css(&self) -> String`（序列化 computed style 为内联 CSS，跳过默认值）。
- `src/output/html.rs` 的 `node_to_html` 每个元素节点均带 `style` 属性（样式真源与 PDF 路径共享同一棵 styled AST）。
- `markdown_to_html`（纯 pulldown → 字符串）降级为 fallback，仅在 `parse_markdown_with_css` 失败时使用。
- `src/color.rs` 新增 `Color::to_hex()` → `#rrggbb`，供内联样式输出。

### 与方案 / 原计划不一致（重要）
1. **`src/html` 被彻底拆散，未保留该名**：原计划讨论"按 pipeline 阶段拆"时曾设想保留 `html` 作为
   某一阶段名，实际落地为新建 `dom`（Layer 1 HTML AST）+ `output`（输出），`html` 概念被 `dom::parser`
   （纯字符串解析）+ `output::html`（ast→HTML 序列化）两个端点瓜分。这是比"单纯拆分"更进一步的重组。
2. **Markdown 直连 DOM 超出原 S0–S4 范围**：原方案 §4 的管线是 `markdown → html string → HtmlDocument`，
   本次**主动消除字符串往返**（用户评估认可）。副作用：`markdown_to_dom` 需自行处理 pulldown 的
   `Tag`/`Event` 语义（CodeBlock 语言丢失问题、Image alt 提取等），这些在 `src/html/styled.rs` 里
   原本由二次 `parse_html` 兜底，现在需在 `dom::markdown` 直接处理（见下 #4）。
3. **`render/pdf.rs` 改名为 `output/pdf.rs` 但**行为未变**：PDF 后端仍消费 `DocumentSkeleton` + 内部分页，
   仅物理位置变化；本次**未**改动其分页/绘制逻辑，也未启动 S4 DOCX。
4. **CodeBlock 语言与 Image alt 的直连实现细节**：
   - 代码块不再嵌套 `<code>`（旧 `to_ast` 假设有 `<code>` 子），改为 `<pre class="language-X">` 单节点；
     `to_ast::extract_code_block` 已加回退：无 `<code>` 时读 `pre` 自身 `class` 取语言 + `pre.children` 文本。
   - Image `alt` 不再用 `title` 充数，改为在 `End(Tag::Image)` 时从 inner text 提取 alt。
   这两处是直连 DOM 带来的额外兼容修复，非原方案要求。
5. **`src/text.rs` 重命名为 `src/document/text.rs`**（git 标记为 R）：文本/字形投影类型已归入 document 层，
   与 S0 `document/types/text.rs` 的 `DocTextRun` 等并存，未合并（物理移动，逻辑未重构）。

### 验证
- `cargo build`：通过。
- `cargo test`：全量 **149** 测试通过（lib + 集成 + 单元测试 + doctest，无回归）。
- e2e PDF 渲染：`liepress --input sample.md --output out.pdf` 生成合法 PDF（21KB），富文本/代码块/表格/引用正常。
- `cargo clippy`：无新增警告。

### 下一步（待定）
- **S4 DOCX / PNG / SVG 后端**：按方案应新增 `output/docx.rs`/`output/png.rs` 等，复用 `DocumentSkeleton`；
  当前 `output/` 仅含 `pdf`/`html`/`md_converter`，DOCX 仍未启动。
- **`ResolvedStyle` 表格样式投影**：`pdf.rs::draw_table` 仍硬编码边框/底色（`ResolvedStyle` 尚无 `table_*` 字段）；
  属样式系统补全，独立于本次管线重组。
- 可选：将 `dom::markdown` 的 pulldown 解析与 `dom::parser`（html5ever）做语义对齐回归测试，防止直连 DOM 偏离。

### 双路线设计原则（HTML 与 PDF 不共用同一中间表示）

经讨论确认并固化为设计原则：**HTML 输出与 PDF 输出走两条独立路线，不共用 `DocumentSkeleton`**。
两条路线在**已套 CSS 的 `ast::Node`（语义树）**这一层汇合、共享样式真源，但在其下游分流。

| 路线 | 管线 | 消费的中间表示 |
|---|---|---|
| **PDF（精确布局）** | `ast::Node → DocumentSkeleton → paginate → Vec<PdfPage>` | `DocumentSkeleton`（含断行/坐标/layout）|
| **HTML（流式）** | `ast::Node → node_to_html` | `ast::Node`（**不做**断行/layout）|

**关键理由：`DocumentSkeleton` 不适合 HTML 流式输出。**
`from_ast.rs::layout_inline` 在生成 `DocumentSkeleton` 时，已调用 `layout_text_with_contexts`
按固定页面内容宽度（A4 ~460pt）做**文本折行**，产出的 `TextLine` 是**已断行、带 x 坐标**的物理布局结果。
若 HTML 也走 `DocumentSkeleton`，会把这套固定宽度断行**固化**进 HTML：
- 丢失浏览器响应式重排能力（宽屏变窄行、窄屏换行点不一致）；
- 宽度假设（A4）被写死，换容器无法自适应。
HTML 作为流式/响应式媒介，文本断行应交给浏览器，因此必须**跳过 `DocumentSkeleton`**，
直接消费尚未断行的 `ast::Node`（文本仍以原始 segments 存在），由 `node_to_html` 输出整段
`<p>…</p>` 让浏览器自行排版。

**结论**：PDF 要 `DocumentSkeleton` 的 layout 信息（精确坐标），HTML 不要（要流式）；
两者共享 `ast::Node` 这层真源，而非 `DocumentSkeleton`。这是有意的双路线对称设计，
非方案文档 §5.3「HTML 从 Document 生成」原始设想的偏离式妥协，而是对流式本质的正确适配。

---


## 2026-08-13（续）— `<center>` 居中失效排查：确认不合法用法，不做 hack

> **现象**：`tests/test.md`（217–229 行）用 `<center>` 包裹 markdown 块级内容（`# 标题`、`| 表格 |`），
> 生成 PDF 后内容并未居中。
>
> **排查结论**：这是**非法的 CommonMark 用法**，工具行为正确，无需修复。
> - `<center>` 在 pulldown-cmark 中被当作**块级 HTML 标签**；按 CommonMark 规范，块级 HTML 标签内部
>   不会再去解析 markdown 块级语法，其内容（标题/表格）会被**提升到 `<center>` 之外**成为独立块级节点。
> - 实际 DOM 验证：`<center>` 解析为**空容器**（children=0），标题/表格均为其兄弟节点，左对齐。
>   （即便改成合法的纯 HTML 写法 `<center><h1>…</h1></center>`，pulldown-cmark 同样把块级 `<h1>` 提到外面——
>   这是解析器固有行为，非 liepress bug。）
> - 因此"居中"作用在一个空块上，肉眼表现为"失效"。

### 尝试过并已还原的改动（用户确认"不合法就不支持"）
1. `src/document/from_ast.rs`：曾让 `NodeKind::Center` 分支把 `text-align: center` 继承给子节点。
   因 center 实际无子块（children=0），该逻辑无意义，已还原为原 `Center|Container|Span` 合并分支。
2. `src/dom/markdown.rs`：曾试图把 `<center>`/`<center>` 作为"容器压栈/弹栈"吸收后续 markdown 块级内容。
   破坏 pulldown 事件流 Start/End 配对（`<center>` 被包在段落 inline HTML 内，段落 End 会误弹 center），已还原。

### 保留的合法行为
- `src/ast/presets/default.css` 的 `center { text-align: center; }` 规则**保留**：它对 `<center>` 内**行内文本**
  （如 `<center>居中文字</center>`）合法且生效——行内文本段落会继承 `text-align: center` 并正确居中。
- `from_ast` 中 `<center>`/`div`/`span` 统一映射 `BlockKind::Container` 的现状不变（见 S1 日志 #2）。

### 修改文件
- `tests/e2e/pipeline.rs`：将原先针对"center 包裹块级"的测试替换为
  `test_center_inline_text_centers`（验证 `<center>居中文字</center>` 合法行内用法能正常生成 PDF）。

### 验证
- `cargo test --test e2e_tests`：**23 passed，0 failed**（含 `test_center_inline_text_centers`，无回归）。
- `src/document/from_ast.rs`、`src/dom/markdown.rs` 已彻底还原，无 hack 残留、无调试 eprintln。

### 结论 / 给用户的建议
- 用 HTML 块级标签（`<center>`）包裹 markdown 块级语法（标题/表格）**不是合法 CommonMark**，不被支持。
- 若确实需要把整个标题/表格居中，应使用合法机制：例如给内容套带对齐样式的容器（div + class），
  或在 CSS 中直接对标题/表格设 `text-align: center`，而非依赖已废弃的 `<center>` 标签去包 markdown。
- 如需，可后续新增"块级居中方框语法"（如 `::: center` 围栏容器）作为正式支持——需用户确认。

---

## 2026-08-13（续2）— 任务5：`draw_table` 硬编码颜色 → ResolvedStyle 投影 table_* 字段并接入

> **背景**：`src/output/pdf.rs::draw_table` 把表格边框色、表头底色、斑马纹底色、边框宽度、单元格内边距
> 全部硬编码（灰 200 边框 / 0.5 宽 / 230 灰表头 / 255 白底 / 3.0 内边距）。这些本应由 CSS `table_*`
> 计算样式驱动，以便通过 `default.css` 或用户样式定制表格外观。
> `ast::Style` 早已定义 `table_border_color` / `table_border_width_pt` / `table_cell_padding_h_pt` /
> `table_cell_padding_v_pt` / `table_header_bg` / `table_alt_row_bg` 六个字段，但 `ResolvedStyle`
> 未投影它们，`draw_table` 也无从读取。

### 改动
1. `src/document/types/style.rs`：
   - `ResolvedStyle` 新增表格字段：`table_border_color`、`table_border_width_pt`、
     `table_cell_padding_h_pt`、`table_cell_padding_v_pt`、`table_header_bg`(Option)、`table_alt_row_bg`(Option)。
   - `Default` 实现给上述字段赋与 `ast::Style::default()` 一致的值（边框灰 180 / 0.5pt / 内边距 4×2 / 表头&斑马纹 None）。
   - `From<ast::Style>` 投影补充这六个字段，使 CSS 计算后的表格样式能落到文档层。
2. `src/output/pdf.rs`：
   - `draw_table` 新增 `style: &ResolvedStyle` 参数（调用点 `BlockKind::Table` 传入块样式）。
   - 边框色/边框宽/单元格内边距改用 `style.table_border_color` / `table_border_width_pt`(as f64) /
     `table_cell_padding_h_pt` / `table_cell_padding_v_pt`。
   - 表头底色改用 `style.table_header_bg.unwrap_or(230 灰)`；原代码无斑马纹，现接入
     `table_alt_row_bg`：仅当该字段为 `Some` 时，body 奇数行（i%2==1）使用交替底色，默认仍为白底，行为向后兼容。

### 修改文件
- `src/document/types/style.rs`（ResolvedStyle 增字段 + 投影）
- `src/output/pdf.rs`（draw_table 接入 style）

### 验证
- `cargo build` 通过（仅既有 warning，与本次无关）。
- `cargo test --test e2e_tests`：**23 passed，0 failed**（含 `test_table`、`test_pdf_table_structure`，无回归）。
- 默认渲染外观不变（table_* 默认值与旧硬编码一致）；后续可通过 CSS 覆盖 `table_*` 来定制表格配色。
