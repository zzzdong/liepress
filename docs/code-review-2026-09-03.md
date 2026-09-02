# 代码审查记录（2026-09-03）

> 对 `liepress`（Markdown/HTML → PDF 转换器）的第二轮健壮性 & 样式链路专项审查。
> 覆盖 `src/` 全量（约 1.1 万行）与 `tests/`。方法：静态扫描 + **临时探测程序实测验证**
> （针对 CSS 引擎行为编译运行后删除）+ 源码级交叉核对（lievisual 0.2.0 / parley）。
>
> 与 2026-08-30 一轮相比：**未发现新的 P0（崩溃）级问题**（上轮修复均带回归测试、未复发）；
> 本轮核心发现集中在 **CSS 样式链路的高影响功能缺陷**——多为「解析成功但静默丢样式」形态，
> 不易被现有测试暴露，均以引擎实测输出佐证。

---

## 一、P1 — 高影响正确性缺陷（已实测证实）

### P1-1 CSS 简写属性大面积静默失效（`margin` / `padding` / `border`）

- **位置**：`src/css/engine.rs:469-706` `apply_declaration`
- **根因**：引擎只处理 `margin-top/bottom/left/right` 等单边属性，没有裸 `margin` / `padding` /
  `border`（颜色/样式合并）分支；`lightningcss` 解析出的简写属性名在 match 中落入 `_ => {}` 丢弃。
- **实测**（引擎直接 resolve，父样式 `Style::default()`）：
  - `p { margin: 10pt }` → `margin.top=0, bottom=0`（期望 10/10）
  - `p { padding: 5pt }` → `padding.top=0`（期望 5）
  - `p { border: 1pt solid #ff0000 }` → 宽度 0、无边框
- **附带影响**：内置 `default.css:8` 的 `body { margin: 0 }` 也因此失效（body 保持默认 12pt 下边距）。
- **影响**：`margin: 0 auto`、`padding: 8px`、`border: 1px solid #ccc` 等最常见外部 CSS 写法全部不生效。

### P1-2 百分比长度一律按「字号」换算（`%` 语义错误）

- **位置**：`src/ast/style.rs:315` `CssLength::Percent` → `v / 100.0 * font_size`
- **根因**：`width/height/margin/padding` 的 `%` 应相对**包含块宽度**，但 `apply_declaration`
  （`engine.rs:546-554` 等）统一 `len.resolve(font_size, root)` 乘当前字号；仅 `font-size`/`line-height`
  才应如此。
- **实测**：`div { width: 50% }` → `width = Some(5.25)`（= 0.5 × 10.5pt 字号）。任何用 `%` 的外部布局样式都会得到一个「一个字符宽」的盒子。

### P1-3 `line-height` CSS 不参与实际行排版（度量与绘制分叉）

- **位置**：`src/document/text.rs:204`（`css_text_style` 硬编码 `line_height: None`）+
  `src/ast/presets.rs:75-93`（`computed_style_to_text_style` 未传行高）
- **根因**：CSS `line-height` 解析进 `Style.line_height_pt` 且被 `block_height`
  （`output/common.rs:29-31`）与 `paginate_lines_block` / `paginate_table`（`pdf.rs:1190-1198`）
  用作**高度度量**；但真实排版 `layout_text` 时 `TextStyle.line_height = None` → parley 采用
  **字体固有行高**（lievisual 源码 `text/engine.rs:218-223` 注释 "left unset (the font's default is used)"）。
- **后果**：① 用户设 `line-height` 不改变文字行距；② 字体固有行高 ≠ `line_height_pt` 时
  **块高度量与文本实际占位不一致**（代码块背景、段落间空隙、分页切点与文字错位）。
  默认字体下二者接近而掩盖问题，大行高（如 1.8 / 20pt）即显形。

### P1-4 PDF `--height-unlimited` 模式不支持内容超高（输出被截断）

- **位置**：`src/output/pdf.rs:192-225`（`generate` 每页固定 `settings.height_pt`）
- **根因**：`height_unlimited` 仅令 `content_height() = f32::MAX`（`page.rs:102-108`）使
  `paginate_layout` 把所有内容排进**一页**，但 PDF 页面仍是固定 A4 高度。SVG/PNG 经
  `to_scene.rs:56-58` 会把画布扩到 `total_h`，唯独 PDF 无扩展机制 → 超高内容落入页面框外。
- **后果**：与文档声明「最终输出单页文档，页面高度 = 实际内容高度」不符；
  `--height-unlimited -f pdf` 是可用 CLI 参数，属真实功能缺陷。

---

## 二、P2 — 功能缺失 / 语义不一致

### P2-1 颜色子集太窄，常见值静默失效

- **位置**：`src/css/engine.rs:756-807` `parse_color`
- **现状**：仅 11 个命名色 + `#rgb/#rrggbb` + 逗号 `rgb()`。
- **实测**：`rgba(255,0,0,.5)` → 黑；`transparent` → 不生效；`hsl()` 因 lightningcss 预转
  `rgb(...)` 碰巧可用；`#rrggbbaa`、`navy` 等其余命名色全部丢弃。
- **影响**：`background-color: transparent` 等极常见写法失效。

### P2-2 GFM 表格列对齐标记被丢弃

- **位置**：`src/dom/markdown.rs:268`（`Tag::Table(_alignments)` 忽略列对齐）
- **根因**：主 DOM 管线（PDF/SVG/PNG）生成的 `<td>` 不携带对齐，`to_ast.rs:699`
  `extract_table_align` 只能从 inline `style="text-align:…"` 读取 → **实测** `|:---|---:|` 表格
  `align: []`，全部退化为左对齐。
- **影响**：HTML 手写 `<td style="text-align:center">` 的表格不受影响，两输入路径行为不一致。

### P2-3 `!important` 无法压过更高特异性普通声明

- **位置**：`src/css/engine.rs:160-168` + `:332-345`
- **根因**：`extract_declarations` 仅把 important 追加到同规则声明末尾，跨规则仍按特异性升序应用。
- **实测逻辑**：`p{color:red!important}` 会被 `.x{color:blue}`（更高特异性）覆盖。

### P2-4 `div` / `section` / `article` 等容器被降级为 `Paragraph`

- **位置**：`src/dom/to_ast.rs:376-406`（`convert_tag_block`）
- **根因**：`<div>` 内嵌块级子元素（`<h1>`/`<table>`/多个 `<p>`）时，块级子节点被塞进
  `NodeKind::Paragraph`；布局层 `collect_inline_segments`（`from_ast.rs:742-747`）对块级子节点走
  `text_content()` 降级 → 结构与样式被压平成一串文本。
- **注**：`test_div_as_paragraph` 表明 div→段落是有意简化；「div 内嵌块级」被破坏是副作用。

### P2-5 `page-break-before/after` 等「写后无读」字段

- **位置**：`ast/style.rs:376-377`、`engine.rs:562-567`
- **现状**：`page-break-before/after` 全仓零消费（分页输入 / `ResolvedStyle` 均不含该字段）→
  CSS 完全无效且无报错。`list-indent`、`list-style-type`、`text-indent` 同类。

---

## 三、安全与健壮性

### S-1 本地图片读取无路径限制（P1 · 沿用上轮待办，本轮新增「两管线不一致」证据）

- **位置**：`src/dom/resource.rs:77-86` `load_local`
- **现状**：`resolve_image` 只排除 `http(s)`/`//`/`#`，**不排除 Unix 绝对路径 `/` 与 Windows
  盘符路径**；`base.join(src)` 遇绝对路径会覆盖 base，任意本地文件被读取并 base64 内嵌。
- **新发现**：两条 Markdown 管线不一致——降级字符串管线 `md_converter.rs:149-155`
  `should_embed` 明确跳过 `/` 开头，而主 DOM 管线（PDF/SVG/PNG/DOCX）不跳。同一份 md 里
  `![](/etc/passwd)`：HTML 输出保留原样，PDF 输出却读文件内嵌。
- **处置建议**：服务端接收不可信 md 前补 `..` 组件拒绝 + 绝对路径开关 + 图片扩展名白名单。

### S-2 `markdown.rs` 事件栈 `expect`（P3 · 未构造成功）

- **位置**：`src/dom/markdown.rs:90` `stack.pop().expect(...)`
- **现状**：依赖 pulldown 事件严格配对，未见可构造崩溃路径；建议改 `if let` 容错。

### S-3 深嵌套 HTML 递归无深度保护（P3）

- **位置**：`to_ast.rs` / `from_ast.rs` 的递归转换
- **现状**：数万层嵌套 div 可致栈溢出 abort（abort 无法被 catch）。低概率但不可恢复。

### S-4 `parse_length` 接受 `NaN`/`inf`（P3）

- **位置**：`css/engine.rs:809-840` 与 CLI `bin/liepress.rs:154-177`
- **现状**：`NaN`/`inf` 传播进几何坐标造成异常输出（不 panic）。

---

## 四、架构 / 质量观察

1. **双解析架构是样式缺陷的共同温床**：`engine.rs` 先经 lightningcss 类型化解析，再把每个声明
   `to_css_string` 字符串化，最后用**手工弱解析器**（`parse_color`/`parse_length`/
   `parse_font_weight` 等）重解析。类型信息在字符串往返中丢失，简写展开、单位归一化、
   `!important` 元数据均拿不到。建议直接消费 `lightningcss::properties::Property` 枚举，
   删除 `property_to_string_pair` + 手工解析器。
2. **`Style` 双轨投影**：`ast::Style`（写满）→ `document::types::ResolvedStyle`（子集投影）→
   布局；新增字段不触发编译错误，`page_break_*`/`list_*` 的「写后无读」由此产生。
3. **16 个 `*_to_pdf/svg/png/docx` 公共入口**（`lib.rs:494-841`）是同一模板的五份复制
   （约 200 行重复），上轮建议的 `InputSource` + `run()` 收敛仍未做。
4. **死代码复查（仍存在）**：`ast/presets.rs` 的 `list_marker_style()` / `LIST_INDENT_PT` /
   `calculate_list_indent()` 零调用；`ResolvedRule.max_specificity`（`engine.rs:44`）写后无读
   （`#[allow(dead_code)]`）。
5. **正向确认**：PDF 表格跨页（`paginate_table` + `rebase_lines` + `push_page` 统一页眉页脚）、
   按行分页（`paginate_lines_block`）、`u64 + saturating_add` 列表序号、`is_ascii_hexdigit`
   预校验等防护扎实且有回归测试；SVG/PNG 统一经 `to_scene` 与 lievisual，文本行定位三后端一致
   （未发现 PDF 与 SVG 行距漂移）。

---

## 五、验证方法备注

- 针对 CSS 引擎行为（P1-1/1-2、P2-1、GFM 表格对齐）构造**临时探测 example** 直接调用
  `CssEngine::resolve_style` / `markdown_to_dom` / `html_to_styled_nodes` 打印结果，实测佐证
  后已删除探测文件。
- `line-height` 结论对照 lievisual 0.2.0 源码 `text/engine.rs`（`line_height: None` 时 parley
  用字体默认行高）。
- `height_unlimited` 结论来自 `pdf.rs generate()` / `page.rs content_height()` /
  `to_scene.rs` 画布计算三方对照。

---

## 六、修复建议（按优先级）

1. **P1**：`apply_declaration` 增加 `margin`/`padding`（1-4 值展开）与 `border`/`background`
   简写分支；`CssLength::Percent` 按属性区分基准（宽度类相对包含块）。
2. **P1**：`css_text_style` 增 `line_height: Option<f64>` 参数并透传 `Style.line_height_pt`
   （`presets.rs`/`output/common.rs` 一并改），消除度量/绘制分叉。
3. **P1**：PDF `height_unlimited` 在 `generate` 按实际内容总高设置单页高度，与 SVG/PNG 对齐。
4. **P2**：`parse_color` 扩命名色 + 支持 `rgba()`/`transparent`/`#rrggbbaa`（或直接消费
   lightningcss 类型化颜色值）。
5. **P2**：`markdown.rs` 把 `Tag::Table(alignments)` 写进 `<th>/<td>` 的
   `style="text-align:…"`，一处小改即恢复 GFM 表格对齐。
6. **P2**：`extract_declarations` 携带 `!important` 标记参与排序；`ResolvedStyle` 补
   `page_break_*` 并接入分页（或明确移除避免误导）。
7. **安全**：`resource.rs::load_local` 补 `..`/绝对路径拒绝与扩展名白名单，统一两管线语义。
