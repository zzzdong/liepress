# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.2.3]

### Changed

- **BREAKING (dependency):** 适配 `lievisual` 0.3，迁移依赖并改用 `NodeKind` 容器模型（`to_scene.rs` 通过 `SceneNode::new(el).with_z(z)` 构造，避免新增字段破坏编译）。
- 收敛转换入口为单一管线（`Source` + `prepare_styled` + `html_to_styled_node`），删除 PDF/DOCX 两条路径重复的 CSS 合并逻辑；`infer_font_family` 改为消费已解析的 `HtmlDocument`，删除手写 Markdown 状态机，并修正 CSS 优先级（字体兜底 < 文档 `style` 标签 < 用户 CSS）与严格模式顺序。
- 升级依赖并适配 `zip` 9 API。

### Fixed

- PDF 字体缓存：条目连同 `Arc<Vec<u8>>` 一起持有字体字节，消除指针键地址复用导致的错误命中；命中时 `debug_assert` 校验内容一致，未命中时按内容比对只注册一份 `krilla` 字体。

## [0.2.2]

- 完善页眉页脚排版与字体配置。
- 升级并锁定 `liecharts` 0.2.2、`liemermaid` 0.2.1。

## [0.2.1]

- 修复 base64 图片在行内、表格与非常见格式下被静默丢弃。
- 将 DOCX/HTML/SVG/PNG 输出改为生成器模式。
