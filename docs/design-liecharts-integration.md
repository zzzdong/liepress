# 设计文档：内置 echarts 风格图表渲染（```` ```liecharts ```` 代码块）

> 状态：可行性评估 + 接口设计（未实现）
> 日期：2026-08-14

## 1. 目标

支持在 Markdown 中用标记代码块声明图表，提取块内 JSON 配置，调用 [`liecharts`](https://github.com/zzzdong/liecharts)（ECharts 风格的 Rust 图表库）渲染为 PNG，并作为图片嵌入 PDF / SVG / PNG 三种输出。

语法示例：

````markdown
```liecharts
{
  "title": { "text": "月度趋势" },
  "xAxis": [{ "type": "category", "data": ["1月", "2月", "3月"] }],
  "yAxis": [{ "type": "value" }],
  "series": [{ "type": "bar", "name": "销售额", "data": [120, 200, 150] }]
}
```
````

期望产物：该代码块被渲染成一张图表图片，居中嵌入文档，与现有 `![...](x.png)` 图片体验一致。

## 2. 可行性评估（结论：可行）

### 2.1 liecharts 侧接口已满足

探查 `../liecharts`（`v0.1.0`，Apache-2.0）确认：

- 公共 API 在 `lib.rs` 导出 `pub use builder::ChartBuilder;` 与 `pub use chart::Chart;`。
- JSON 路径：`ChartBuilder::from_option_json(&str) -> Result<ChartBuilder>`（见 `src/builder.rs:62`）。
- 内存渲染（**无需落盘**）：
  - `ChartBuilder::render_png(self, width: u32, height: u32) -> Result<Vec<u8>>`（见 `src/builder.rs:214`），返回 PNG 字节。
  - `ChartBuilder::render_svg(self, width: u32, height: u32) -> Result<String>`（见 `src/builder.rs:218`），返回 SVG 字符串。
- 内部实现：`build(width,height) -> Chart`，`Chart::render_png()` 走 `vello_cpu` / `image` 编码为 `Vec<u8>` PNG（见 `src/api/chart.rs:586`）。
- 依赖栈：`vello_cpu 0.1`、`parley 0.11`、`image 0.25`、`serde_json`。**注意**：liecharts 用的是 `vello_cpu 0.1`，而 liepress 当前用 `vello_cpu 0.2.0`，需对齐版本（见 §6）。

结论：输入「JSON 字符串 + 宽高」→ 输出「内存 PNG 字节」的链路**已经存在且可用**，无需新增 liecharts 能力。

> **依赖接入方式（用户决策）：使用 `[patch.crates-io]` 而非 `path` 依赖。**
> - liecharts 已发布到 crates.io（`v0.1.0`），liepress 正式依赖写 `liecharts = "0.1"`（取自 crates.io）。
> - 开发期在 liepress 的 `Cargo.toml` 加：
>   ```toml
>   [patch.crates-io]
>   liecharts = { path = "../liecharts" }
>   ```
>   以本地 `../liecharts` 覆盖，便于迭代改动；发布/CI 去掉 `[patch]` 即回到 crates.io 版本。
> - **`vello_cpu` 版本冲突不构成阻塞**：liepress 用 `vello_cpu 0.2.0`、liecharts 用 `0.1`，cargo 允许同一 crate 多版本共存；且 liepress 仅在 liecharts **内部**调用 `render_png()` 并接收 `Vec<u8>`，不在跨 crate 边界传递任何 `vello_cpu` 类型，故无需强行对齐版本。
> - 如需对 liecharts 的小调整（如暴露 theme 入口，见 §6.2），在本地 `../liecharts` 改、通过 patch 生效，调通后由 liecharts 方发新版本、再升级 liepress 的 `= "0.1"` 约束。

### 2.2 liepress 侧图片通路已满足

- 文档层图片类型 `DocImage`（`src/document/types/image.rs`）自包含二进制：`data: Vec<u8>`、`format: String`（"png"）、`pixel_size: (u32,u32)`、`size: (f64,f64)`。
- 三个后端均已消费 `BlockKind::Image`：
  - `src/output/pdf.rs:523` `draw_image` 直接吃 PNG 字节（`Image::from_png`）。
  - `src/output/png.rs:426` `image::load_from_memory(data)`。
  - `src/output/svg.rs:281` 内联为 `<image href="data:image/png;base64,...">`。
- 图片尺寸解析逻辑 `resolve_image_size`（见 `from_ast.rs:533`）可复用于图表块（按像素尺寸 + 页宽自适应）。

结论：只要产出 `DocImage { data: png_bytes, format: "png", pixel_size: (w,h) }`，PDF/SVG/PNG 后端**零改动**即可渲染。

### 2.3 接入点明确

`src/document/from_ast.rs` 中代码块转换当前调用 `highlight_code(...)` 生成 `BlockKind::CodeBlock`。新增一个分支：当 `lang == "liecharts"` 时，不走高亮，而是解析 JSON → 调 liecharts → 生成 `BlockKind::Image`。这一层（document 层）正是「Markdown 语义 → 文档模型」的边界，与图片节点转换（`convert_image_node`，`from_ast.rs:522`）处于同一抽象层级，职责一致。

## 3. 接口设计

### 3.1 可插拔渲染器抽象（参考 mermaid / plantuml 模式）

类似外部工具渲染（mermaid-cli 用 `mmdc` 命令、plantuml 用 `java -jar`）的常见做法是**「渲染器 trait + 注册表」**，而非把每个渲染器硬编码进主流程。这带来两点好处：(a) liecharts 不成为强制依赖；(b) 未来可同构接入 mermaid、vega-lite、plantuml 等。

定义统一 trait（`src/document/ext_render.rs` 草案）：

```rust
/// 外部渲染器统一抽象：输入代码块文本，输出自包含图片字节。
pub trait BlockRenderer {
    /// 该渲染器认领的代码块语言标识（如 "liecharts"、"mermaid"）。
    fn lang(&self) -> &'static str;

    /// 渲染。失败返回 Err，由上层降级为「显示原始代码」或报错。
    fn render(&self, code: &str, opts: &RenderOpts) -> Result<RenderedImage, RenderError>;
}

pub struct RenderOpts {
    /// 建议像素宽（默认取页内容宽 pt 折算，如 720）
    pub width: u32,
    /// 建议像素高（默认按宽高比或 fallback 如 420）
    pub height: u32,
    /// 主题（"light" | "dark"），可映射到 liecharts Theme
    pub theme: String,
}

pub struct RenderedImage {
    pub data: Vec<u8>,
    pub format: String,   // "png" | "svg"
    pub pixel_size: (u32, u32),
}
```

liecharts 实现（`src/document/ext_render/liecharts.rs`）：

```rust
pub struct LieChartsRenderer;

impl BlockRenderer for LieChartsRenderer {
    fn lang(&self) -> &'static str { "liecharts" }
    fn render(&self, code: &str, opts: &RenderOpts) -> Result<RenderedImage, RenderError> {
        let builder = liecharts::ChartBuilder::from_option_json(code)
            .map_err(|e| RenderError::Parse(e.to_string()))?;
        let png = builder.render_png(opts.width, opts.height)
            .map_err(|e| RenderError::Render(e.to_string()))?;
        Ok(RenderedImage { data: png, format: "png".into(), pixel_size: (opts.width, opts.height) })
    }
}
```

注册表（默认空，feature 开启时注册）：

```rust
/// 全局渲染器表。未开启任何 feature 时为空，代码块退化为普通高亮。
pub fn builtin_renderers() -> Vec<Box<dyn BlockRenderer>> {
    #[cfg(feature = "charts")]
    return vec![Box::new(LieChartsRenderer)];
    #[cfg(not(feature = "charts"))]
    return vec![];
}
```

### 3.2 依赖与 feature 门控

`Cargo.toml`（正式来源为 crates.io，本地用 `[patch]` 覆盖）：

```toml
[features]
default = []
charts = ["dep:liecharts"]

[dependencies]
liecharts = { version = "0.1", optional = true }

# 开发期取消注释，用本地 ../liecharts 覆盖 crates.io 版本：
# [patch.crates-io]
# liecharts = { path = "../liecharts" }
```

- 正式依赖取自 crates.io（`v0.1.0`），保证可复现构建与发布可用性。
- 本地开发/调 liecharts 时，在 `Cargo.toml` 加 `[patch.crates-io] liecharts = { path = "../liecharts" }` 即可用本地改动；发布或 CI 去掉 `[patch]` 即回到 crates.io 版本。
- 默认构建**不引入** liecharts，保持轻量；开启 `features = ["charts"]` 才拉入图表渲染栈。
- `vello_cpu` 多版本共存由 cargo 自动处理（见 §2.1），无需对齐。

### 3.3 document 层接入

`src/document/from_ast.rs` 代码块分支改为：

```rust
let renderers = builtin_renderers();
if let Some(r) = renderers.iter().find(|r| r.lang() == lang) {
    match r.render(code, &RenderOpts::from_settings(settings)) {
        Ok(img) => Block::new(BlockKind::Image(DocImage::from_rendered(img, alt)), style, false),
        Err(e) => {
            // 降级：把错误信息 + 原始 JSON 作为 CodeBlock 显示，不中断整篇渲染
            Block::new(BlockKind::CodeBlock { code: format!("// render failed: {e}\n{code}"), lang: Some("json".into()), lines: highlight_code(code, "json", &style) }, style, true)
        }
    }
} else {
    // 现有高亮路径
    let lines = highlight_code(code, lang, &style);
    Block::new(BlockKind::CodeBlock { code: code.into(), lang: lang.clone(), lines }, style, true)
}
```

### 3.4 尺寸与主题

- 宽高：默认 `width = content_width_pt`（约 720pt 折算像素，按 DPI 150 → ~1080px），`height` 默认 `width * 7/12`（≈ 630px，贴近 echarts 4:3 习惯）。支持在代码块 info string 用查询参数覆盖，例如 ` ```liecharts width=900 height=500 `（解析 `lang` 后的附加 token）。
- 主题：默认 `light`（echarts 主题）；info string 写 `theme=dark` 时映射到 `Theme::dark()`。需把 theme 透传给 `liecharts`（当前 `ChartBuilder` 未暴露 theme 设置入口，需确认/补充；见 §6）。

## 4. 多后端行为

| 后端 | 行为 | 改动量 |
|------|------|--------|
| PDF (`pdf.rs`) | `draw_image` 直接吃 PNG 字节 | 0（复用） |
| PNG (`png.rs`) | `image::load_from_memory` | 0（复用） |
| SVG (`svg.rs`) | base64 data URI 内联 | 0（复用） |
| DOCX (`docx.rs`) | 当前图片走 `<img src>` + 文件读取；需确认能否接收内存字节（见 §6） | 小 |
| HTML (`html.rs`) | 内联 data URI | 小/0 |

## 5. 错误处理与降级策略

- JSON 解析失败 / liecharts 渲染失败 → **不 panic、不中断整篇**，降级为「带错误注释的原始代码块」（见 §3.3），保证文档健壮性（与 strict 模式无关，始终软降级）。
- `charts` feature 未开启时，` ```liecharts ` 当作普通代码块按 `json` 高亮显示，用户不会看到神秘空白。

## 6. 风险与待确认项

1. **vello_cpu 多版本共存（非阻塞）**：liepress 用 `vello_cpu 0.2.0`，liecharts 用 `0.1`。采用 `[patch.crates-io]` 覆盖本地 liecharts 后，cargo 会同时存在两个 `vello_cpu` 版本；liepress 仅在 liecharts **内部**调用 `render_png()` 并接收 `Vec<u8>`，跨 crate 边界不传递任何 `vello_cpu` 类型，故不会编译冲突。**无需对齐版本**。若日后希望彻底统一，可待 liecharts 发新版时升级其内部 `vello_cpu` 至 `0.2`。
2. **theme 透传（需 patch 调整 liecharts）**：`ChartBuilder` 当前公开 API 未暴露主题选择入口，但内部已有完整 `Theme` 体系（`echarts()`/`light()`/`dark()` 等，见 `theme.rs`）。需对本地 `../liecharts` 做小改：在 `ChartBuilder` 增加 `with_theme(&str)` 或在 `from_option_json` 读取 JSON 顶层 `theme` 字段（如 `"theme": "dark"`）映射到 `Theme::dark()`。改动经 `[patch]` 生效，调通后由 liecharts 方发新版本、升级 liepress 的 `= "0.1"` 约束。首版若暂不改，则仅支持默认 echarts 主题。
3. **DOCX 内存图片**：`docx.rs` 当前图片依赖 `src` 路径读文件（`emit_image(p, src, alt)`），需改为支持 `DocImage.data` 内存字节（参考 PDF/PNG 后端做法）。属小改动。
4. **parley 版本**：双方均 `parley 0.11.0`，一致，无冲突。
5. **SVG 输出选项**：若希望矢量嵌入（`render_svg` 返回字符串），SVG 后端可内联 `<svg>` 而非 raster PNG；但 PDF/PNG 后端仍需 raster，故首版统一用 PNG 最稳。
6. **渲染性能**：每张图表在布局期同步渲染，文档含大量图表时会增加耗时；可接受（与图片内联同量级）。如需异步可后续加 `spawn_blocking`。

## 7. 实施步骤（状态：已完成 ✅）

1. ✅ 本地 `../liecharts` 的 `ChartBuilder::from_option_json` 已注册内置主题（light/dark/roma），JSON `"theme"` 字段生效，经 `[patch]` 覆盖。
2. ✅ liepress `Cargo.toml`：加 `charts` feature + 可选 `liecharts = "0.1"` 依赖 + 开发期 `[patch.crates-io]` 段（已生效，发布时删除该段即回 crates.io）。
3. ✅ 新增 `src/document/ext_render/mod.rs`（trait + `RenderOpts` + `RenderedImage` + `RenderError` + 注册表 + info-string 解析）与 `ext_render/liecharts.rs`（实现）。
4. ✅ `from_ast.rs`：`convert_code_block` 接入渲染器表，命中则渲染为居中 `DocImage`，失败软降级为带注释的 JSON 代码块；未命中退化为普通高亮。
5. ⬜ `docx.rs` 支持内存图片字节（如需 DOCX 输出图表，待办；PDF/PNG/SVG 已可用）。
6. ✅ 单元测试：`ext_render` 内 info-string 解析 + opts 覆盖；`liecharts.rs` 内合法 JSON → 合法 PNG 字节、非法 JSON → `RenderError::Parse`。
7. ✅ `tests/test.md` 增加 ` ```liecharts ` 示例段（第 10 章），覆盖默认/深色指定尺寸/非法 JSON 降级三种情形。

> 验证：`cargo run --features charts -- -i tests/test.md -o x.pdf` 生成 194KB PDF，含两张渲染图表与一张降级代码块；`cargo test --features charts` 全绿。
> `vello_cpu` 多版本共存方案经实践验证可行（liecharts 0.1 + liepress 0.2 编译通过、运行正常）。

## 8. 可扩展性（mermaid / vega / plantuml 同构）

`trait BlockRenderer` 不加修改即可容纳其他渲染器：

- **mermaid**：`MermaidRenderer` 调外部 `mmdc` CLI（`npx @mermaid-js/mermaid-cli` 或本地二进制），`render` 写临时 `.mmd` → 执行命令 → 读回 `.png`。属「外部命令渲染器」子类。
- **plantuml / vega-lite**：同理，外部进程或 wasm 绑定。
- 注册表 `builtin_renderers()` 随 feature 增减，主流程无需变动。

这正对应业界「文档工具 + 外部渲染引擎」的通用模式：核心只定义契约，具体引擎可插拔、可缺省。
