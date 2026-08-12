//! 文档中间表示：Skeleton（结构化、带布局信息的文档树）。
//!
//! Skeleton 是文档层的核心 IR：**不分页**的块树，每个节点携带
//! [`crate::document::types::ResolvedStyle`]。按方案 §4.1「Document 不知道页」，
//! 分页在消费侧（各输出后端）进行，因此 `DocumentSkeleton` 不含页面。
//!
//! - S0：定义类型（本节）。
//! - S1：`from_ast` 从 [`crate::ast::Node`] 构建 `DocumentSkeleton`（源 IR，不分页）。
//!
//! 分页（切页、跨页表格 MultiSpill 等）由各输出后端实现，例如
//! `render::pdf::paginate_skeleton`。

use crate::document::types::{DocImage, DocTextLine, ResolvedStyle, TextAlign};

/// 不分页的 Skeleton 文档根（源 IR）。
///
/// 与方案 `StructuredDocument { blocks: Vec<Block>, page_config }` 对应：
/// 块树保持嵌套（Table/List/Blockquote 内含 blocks）。分页在各输出后端进行。
#[derive(Clone, Debug, Default)]
pub struct DocumentSkeleton {
    /// 文档顶层块（树形，不分页）
    pub blocks: Vec<SkeletonBlock>,
}

/// 页眉/页脚模板（源 IR 持有模板文本，分页后端据页号/总页数替换 `{page}`/`{total}`）。
#[derive(Clone, Debug)]
pub struct SkeletonHeaderFooter {
    /// 文本内容（支持 {page} / {total} 模板变量）
    pub text: String,
    /// 字体大小（pt）
    pub font_size_pt: f32,
    /// 文本对齐
    pub align: crate::document::types::TextAlign,
}

/// Skeleton 内容块。
///
/// 块是源 IR 的基本单元：标题、表格、图片等不可分割；段落、列表可分割。
/// 是否可跨页分割由 [`SkeletonBlock::splittable`] 标记，供输出后端分页时参考。
#[derive(Clone, Debug)]
pub struct SkeletonBlock {
    pub kind: BlockKind,
    /// 该块的已解析样式
    pub style: ResolvedStyle,
    /// 是否可在页面间分割（true=可分割，false=不可分割）
    pub splittable: bool,
}

impl SkeletonBlock {
    pub fn new(kind: BlockKind, style: ResolvedStyle, splittable: bool) -> Self {
        Self {
            kind,
            style,
            splittable,
        }
    }

    /// 获取该块文本内容的拼接值（便于目录/搜索）。
    pub fn text_content(&self) -> String {
        self.kind.text_content()
    }
}

/// Skeleton 块类型。
#[derive(Clone, Debug)]
pub enum BlockKind {
    /// 文档根容器（S2 可能被展平，这里保留以承载整体）
    Document { children: Vec<SkeletonBlock> },

    /// 标题（level 1-6）
    Heading { level: u8, children: Vec<SkeletonBlock> },

    /// 段落（含已排版文本行）
    Paragraph { lines: Vec<DocTextLine> },

    /// 列表
    List {
        ordered: bool,
        start: Option<u32>,
        children: Vec<SkeletonBlock>,
    },

    /// 列表项
    ///
    /// `marker` 为预生成的标记字符串（方案 §3.6 `Listitem { marker }`），
    /// 有序列表为 `"1."`/`"2."` 等，无序列表为 `"•"` 等，由 `from_ast` 注入。
    ListItem {
        marker: String,
        children: Vec<SkeletonBlock>,
    },

    /// 任务列表项（GFM 复选框）
    ///
    /// `marker` 同 [`BlockKind::ListItem`]，通常含复选框符号（`"☐ "`/`"☑ "`）。
    TaskListItem {
        marker: String,
        checked: bool,
        children: Vec<SkeletonBlock>,
    },

    /// 引用块
    Blockquote { children: Vec<SkeletonBlock> },

    /// 代码块
    CodeBlock { code: String, lang: Option<String> },

    /// 分隔线
    ThematicBreak,

    /// 图片
    Image(DocImage),

    /// 表格
    Table {
        rows: Vec<TableRow>,
        column_align: Vec<TextAlign>,
    },

    /// 表格行
    TableRow { cells: Vec<TableCell> },

    /// 表格单元格
    TableCell { children: Vec<SkeletonBlock> },

    /// 纯文本（叶节点）
    Text { text: String },

    /// 行内代码
    InlineCode { code: String },

    /// 链接（叶节点，children 为空时显示 title/url）
    Link {
        url: String,
        title: Option<String>,
        children: Vec<SkeletonBlock>,
    },

    /// 行内换行
    LineBreak,

    /// 块级/行内容器（div/span/center 等）
    Container { children: Vec<SkeletonBlock> },
}

impl BlockKind {
    pub fn text_content(&self) -> String {
        match self {
            BlockKind::Text { text } => text.clone(),
            BlockKind::InlineCode { code } => code.clone(),
            BlockKind::LineBreak => "\n".to_string(),
            BlockKind::Link { url, title, .. } => title.clone().unwrap_or_else(|| url.clone()),
            BlockKind::CodeBlock { code, .. } => code.clone(),
            BlockKind::Document { children }
            |             BlockKind::Heading { children, .. }
            | BlockKind::List { children, .. }
            | BlockKind::ListItem { children, .. }
            | BlockKind::TaskListItem { children, .. }
            | BlockKind::Blockquote { children }
            | BlockKind::Container { children } => {
                let mut s = String::new();
                for c in children {
                    s.push_str(&c.text_content());
                }
                s
            }
            BlockKind::Paragraph { lines } => {
                let mut s = String::new();
                for l in lines {
                    for run in &l.runs {
                        s.push_str(&run.text);
                    }
                }
                s
            }
            BlockKind::Table { rows, .. } => {
                let mut s = String::new();
                for r in rows {
                    s.push_str(&r.cells_text());
                }
                s
            }
            BlockKind::TableRow { cells } => {
                let mut s = String::new();
                for c in cells {
                    for b in &c.children {
                        s.push_str(&b.text_content());
                    }
                }
                s
            }
            BlockKind::TableCell { children } => {
                let mut s = String::new();
                for c in children {
                    s.push_str(&c.text_content());
                }
                s
            }
            BlockKind::Image(img) => img.alt.clone(),
            BlockKind::ThematicBreak => String::new(),
        }
    }
}

/// 表格行（扁平结构，便于分页与投影）。
#[derive(Clone, Debug)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
}

/// 表格单元格。
#[derive(Clone, Debug)]
pub struct TableCell {
    pub children: Vec<SkeletonBlock>,
}

impl TableRow {
    /// 拼接整行所有单元格的文本（用于目录/搜索）。
    pub fn cells_text(&self) -> String {
        let mut s = String::new();
        for c in &self.cells {
            for b in &c.children {
                s.push_str(&b.text_content());
            }
        }
        s
    }
}
