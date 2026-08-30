# 代码审查记录（2026-08-30）

> 对 `liepress`（Markdown/HTML → PDF 转换器）做的一轮健壮性 & 架构专项审查。
> 覆盖 `src/` 全量（约 1 万行）与 `tests/`，基线 `cargo test` 218 passed / 0 failed。
>
> 记录内容分两部分：**已修复**（本轮直接改掉并回归验证）与 **待办/已评估**（暂不修，附原因与建议）。

---

## 一、审查结论摘要

| 类别 | 数量 | 状态 |
|---|---|---|
| 可由外部输入触发的 panic（P0） | 3 | ✅ 已修复 |
| 正确性问题（P1，内容溢出/丢失） | 2 | ✅ 已修复 |
| 代码质量小问题（P3，panic 隐患/死代码） | 2 | ✅ 已修复 |
| 已评估暂不修（安全/架构/功能缺失） | 4 | ⏳ 待办，见第四节 |

---

## 二、已修复问题

### P0-1 空表格导致分页切片越界 panic

- **位置**：`src/document/from_ast.rs` 的 `compute_table_layout`；`src/output/pdf.rs` 的 `paginate_table`
- **根因**：`compute_table_layout` 在 `n_cols == 0 || cell_nodes.is_empty()` 时返回**长度恒为 1** 的 `row_heights`，而 `rows` 长度 = `cell_nodes.len()`（可为 0 或 >1），二者不等长。`paginate_table` 用裸切片 `row_heights[i..i+fit]` 索引 → panic。
- **触发输入**：
  - `<table></table>` → `rows.len()==0` vs `row_heights.len()==1` → `range end index 1 out of range`
  - `<table><tr></tr><tr></tr></table>` → `rows.len()==2` vs `row_heights.len()==1` → `range end index 2 out of range`
- **修复**：
  1. `compute_table_layout` 空表改返回 `(Vec::new(), Vec::new())`（与 rows 等长的空向量）。
  2. `paginate_table` 开头加 `if rows.is_empty() { return; }` 防护。
  3. 行高切片改为 `(i..i+fit).map(|ri| row_heights.get(ri).copied().unwrap_or(header_h))`，彻底消除裸切片越界。
- **验证**：两个 PoC 均 `EXIT=0`（修复前 `EXIT=101`）。

### P0-2 `<ol start>` 整数溢出 panic

- **位置**：`src/document/from_ast.rs` 的 `NodeKind::List` 分支（`convert_node` 内）
- **根因**：`start` 完全由用户输入（`<ol start="...">`，`to_ast` 解析为 `u32`，无上限校验），`idx += 1` 无保护。debug 下溢出 panic，release 下静默回绕成 `0.`。
- **触发输入**：`<ol start="4294967295"><li>a</li><li>b</li></ol>`（Markdown `4294967295. a` 因 pulldown 对 >9 位序号不识别为列表，实际较难触发，但 HTML 路径 100% 触发）。
- **修复**：改用 `u64` 累加 + `idx.saturating_add(1)`，永不 panic / 回绕。
- **验证**：SVG 输出序号正确为 `4294967295.` / `4294967296.`。

### P0-3 CSS 十六进制颜色按字节切片 panic

- **位置**：`src/css/engine.rs` 的 `parse_color`
- **根因**：`hex.len()` 返回**字节数**，`hex[0..1]` 是**字节切片**。长度恰为 3/6 字节但含多字节字符的值（如 `#é1`）会切在非 UTF-8 边界 panic。且内联样式在 lightningcss 解析失败时会走裸字符串回退分支，把原始值透传进 `parse_color`，路径可达。
- **触发输入**：`<p style="color:#é1">`。
- **修复**：切片前先校验 `hex.bytes().all(|b| b.is_ascii_hexdigit())`，确认纯 ASCII 后才做字节切片。
- **验证**：PoC `EXIT=0`。

### P1-1 超高图片溢出单页被裁剪（内容丢失）

- **位置**：`src/document/from_ast.rs` 的 `resolve_image_size`（及 `convert_image_node` / `convert_code_block` 两处调用）
- **根因**：图片自适应页宽时（`(None,None)` 与 `(Some(w),None)` 分支）高度无上限。细长图（如 100×2000 长截图）高度可达数千 pt，而 `paginate_layout` 对页首超高块直接放入当前页 → 超出页面高度被裁剪，下方内容丢失。
- **修复**：`resolve_image_size` 新增 `max_h` 参数（传入 `settings.content_height()`），自适应场景下高度超限时按高度等比缩小（保持宽高比）。
- **验证**：新增回归测试 `test_resolve_image_size_clamps_oversized_height`；长图 PoC 不再溢出。

### P1-2 超长代码块/段落跨页分割（内容丢失）

- **位置**：`src/dom/to_ast.rs` 的 `is_splittable`；`src/output/pdf.rs` 的 `paginate_layout` / `paginate_lines_block`
- **根因**：`is_splittable()` 未包含 `NodeKind::CodeBlock`（代码块 `splittable=false`），且 `paginate_layout` 只对 `BlockKind::Table` 做按行分页，段落/代码块这类「带 `lines` 的可分割块」被整块放进单页 → 高度超出一页时溢出裁剪，尾部内容丢失。
- **触发输入**：60 行以上的代码块、超长段落。
- **修复**：
  1. `is_splittable()` 加入 `CodeBlock`（代码块与段落同构，语法高亮后即 `lines: Vec<TextLine>`，本应自然跨页）。
  2. 新增 `paginate_lines_block`：按「文本行」切分 `Paragraph`/`CodeBlock`，每页片段通过 `rebase_lines` 把 `bounds`/`ink_bounds` 重新基准化（首行 y 归零），首片段预留 `margin_top`、末片段追加 `margin_bottom`。
  3. 顺带修复 `PaginateCtx` 切页时不写 `header/footer` 导致表格跨页中间页缺页眉页脚的问题（抽出 `push_page()` 统一处理）。
- **验证**：新增单元测试 `rebase_lines_shifts_to_new_origin`、`long_code_block_paginates_without_losing_lines`（60 行代码跨页后行数不丢、片段首行归零）；集成测试 `tests/pagination.rs`（200 行代码块 ≥ 3 页、超长段落 ≥ 2 页）。

### P3-1 语法高亮主题 fallback 潜在 panic + 重复解析

- **位置**：`src/document/highlight.rs` 的 `assets()`
- **根因**：fallback 用 `themes["InspiredGitHub"]` 索引（键缺失会 panic），且 `ThemeSet::load_defaults()` 被调用两次（重复解析全部内置主题，慢）。
- **修复**：复用一份 `ThemeSet`，`.get("base16-ocean.dark").or_else(.get("InspiredGitHub")).cloned().unwrap_or_default()`。

### P3-2 死代码清理

- **位置**：`src/document/highlight.rs` 的 `highlight_code`
- **根因**：`let line_len = line.len();` 及其 `let _ = line_len;`（用 `_` 抑制未使用告警）为无意义死代码。
- **修复**：直接删除。

---

## 三、回归验证结果

- `cargo build`：通过，无警告。
- `cargo test`：**222 passed / 0 failed**（原 218 + 新增 4 个：`resolve_image_size` 高度限制、`rebase_lines` 坐标重基准、`long_code_block_paginates_without_losing_lines`、集成测试 `tests/pagination.rs` 2 个）。
- 4 个 P0 PoC + 长图 PoC：全部 `EXIT=0`，输出正确。
- 复现样例保留在 `target/poc/`（已被 gitignore）：`poc1_empty_table.html`、`poc2_empty_rows.html`、`poc3_ol_start.html`、`poc4_color.html`、`poc7_tall_img.md`（含 `tall.png`）、`poc8_long_code.md`。

---

## 四、待办 / 已评估暂不修（附原因与建议）

### 1. 本地图片读取无路径穿越限制（安全，视部署形态而定）

- **位置**：`src/dom/resource.rs` 的 `ResourceResolver::load_local`（`base.join(src)` 直接 `fs::read`，支持相对/绝对路径，无 `..` 拒绝、无扩展名/类型限制）。
- **现状**：这是「嵌入本地相对/绝对路径图片」的**设计功能**，且 `http(s)://`、`//`、`#`、`data:` 均已排除。
- **风险**：仅当把本库用于**服务端接收不可信 Markdown** 时才构成本地文件泄露（LFD，如 `![](../../etc/passwd)`）。纯 CLI 本地转换场景不构成威胁。
- **建议**：若未来面向服务端/WASM 部署，在 `load_local` 增加 `..` 组件拒绝，或对 `base.join(src)` 结果做 `canonicalize` 后校验仍在 `base_dir` 内；绝对路径读取可改为可配置开关。

### 2. 表格 `margin_top` 与普通块不一致（经评估为有意设计，不改）

- **位置**：`src/output/pdf.rs` 的 `paginate_layout`（Table 分支只 `used += margin_bottom`，普通块经 `block_height` 叠加 `margin_top + margin_bottom`）。
- **结论**：这是「margin 不折叠」的简化模型（代码注释已说明），对默认样式（相邻块通常一个 `top=0`）恰好近似 CSS margin-collapse 结果。补 `margin_top` 反而会让表格上方间距翻倍（12→24pt）。**边缘 case**（两个相邻块都带非零 `margin-top`）存在偏差，但当前收益不抵风险，故保留。

### 3. 架构层重复与「写无读」字段（重构项）

- **入口重复**：`src/lib.rs` 约 16 个 `*_to_pdf/svg/png/docx` 入口是同一五步模板（`resolve_user_css → ResourceResolver::new → parse → html_to_layout → render`），合计约 204 行重复；建议抽 `enum InputSource { Markdown(&str), Html(&str), File(Path, Kind) }` + 内部 `run<T>(src, options, sink)` 收敛，16 个入口退化为一行转发。
- **`Style` vs `ResolvedStyle` 字段不同步**：`src/ast/style.rs`（31 字段）经 `src/document/types/style.rs` 手写 `From` 投影为子集（33 字段），新增字段不会触发编译错误。实证「写无读」：`page_break_before/after`（仅 `css/engine.rs` 写入，全仓零读取 → `page-break-*` CSS 静默失效）、`list_indent_pt`（仅写入零读取）、`link_url`（生产后端不读）。
- **超大函数**：`apply_declaration`（`css/engine.rs`，约 238 行）、`convert_node`（`from_ast.rs`，约 252 行）、`draw_block`（`to_scene.rs` / `pdf.rs`，两处同构）建议拆分或提取块分派表。
- **性能小点**：`CssEngine::resolve_style` 对每个元素遍历全部规则并分配 `Vec`（`all_rules()` + `matches.sort_by_key`）；`css_text_style` 每次 `font_family.join(", ")` 分配 String（排版热路径）。可预合并规则、缓存 join 结果。

### 4. 疑似死代码（待确认后清理）

- 全仓零调用、建议确认后删除：`document/text.rs` 的 `get_font_bytes`/`with_text_contexts`/`create_text_layout`；`ast/presets.rs` 的 `list_marker_style`/`LIST_INDENT_PT`/`calculate_list_indent`；`ast/style.rs` 的 `BoxSides::all/vertical/horizontal`、`horizontal_sum/vertical_sum`、`BoxBorders::uniform/is_any_visible/max_width`；`dom/mod.rs` 的 `is_supported`/`as_element_mut`；`ast/mod.rs` 的 `parse_markdown_with_css_strict`。
- 另有若干「仅对外 API」（`unescape_html`、`register_font` 等）应保留，勿误删。

---

## 五、审查方法备注

- 采用「静态扫描（unwrap/panic/切片/算术溢出/路径处理）+ 构造最小 PoC 实测」相结合，所有 P0 结论均以 `target/debug/liepress.exe` 实测退出码（`101` → 修复后 `0`）佐证。
- 架构层结论由只读子代理扫描（选择器匹配、重复度统计、死代码检索）产出，未做改动。
