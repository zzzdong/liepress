//! 文档中间表示（文档层核心 IR）。
//!
//! `Document` 是文档层的核心 IR：**不分页**的块树，每个节点携带
//! [`crate::document::types::ResolvedStyle`]。按方案 §4.1「Document 不知道页」，
//! 分页在消费侧（各输出后端）进行，因此 `Document` 不含页面。
//!
//! 分页（切页、跨页表格 MultiSpill 等）由各输出后端实现，例如
//! [`crate::output::pdf::paginate_layout`]。

use crate::document::text::TextLine;
use crate::document::types::{DocImage, ResolvedStyle, TextAlign};

/// 不分页的文档根（源 IR）。
///
/// 与方案 `StructuredDocument { blocks: Vec<Block>, page_config }` 对应：
/// 块树保持嵌套（Table/List/Blockquote 内含 blocks）。分页在各输出后端进行。
#[derive(Clone, Debug, Default)]
pub struct Document {
    /// 文档顶层块（树形，不分页）
    pub blocks: Vec<Block>,
}

/// 页眉/页脚模板（源 IR 持有模板文本，分页后端据页号/总页数替换 `{page}`/`{total}`）。
#[derive(Clone, Debug)]
pub struct HeaderFooter {
    /// 文本内容（支持 {page} / {total} 模板变量）
    pub text: String,
    /// 字体大小（pt）
    pub font_size_pt: f32,
    /// 文本对齐
    pub align: crate::document::types::TextAlign,
}

/// 内容块（源 IR 的基本单元）。
///
/// 标题、表格、图片等不可分割；段落、列表可分割。
/// 是否可跨页分割由 [`Block::splittable`] 标记，供输出后端分页时参考。
#[derive(Clone, Debug)]
pub struct Block {
    pub kind: BlockKind,
    /// 该块的已解析样式
    pub style: ResolvedStyle,
    /// 是否可在页面间分割（true=可分割，false=不可分割）
    pub splittable: bool,
}

/// 定义列表项（<dt> 术语 + <dd> 定义）
#[derive(Clone, Debug)]
pub struct DefinitionItemBlock {
    /// 术语（<dt>）块
    pub term: Vec<Block>,
    /// 定义（<dd>）块
    pub definition: Vec<Block>,
}

impl Block {
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

/// 块类型。
#[derive(Clone, Debug)]
pub enum BlockKind {
    /// 文档根容器（承载整棵块树）
    Document { children: Vec<Block> },

    /// 标题（level 1-6）
    Heading { level: u8, children: Vec<Block> },

    /// 段落（含已排版文本行）
    Paragraph { lines: Vec<TextLine> },

    /// 列表
    List {
        ordered: bool,
        start: Option<u32>,
        children: Vec<Block>,
    },

    /// 定义列表（<dl>）：有序的 (术语, 定义) 项序列
    DefinitionList { items: Vec<DefinitionItemBlock> },

    /// 脚注定义（末尾聚合）：携带 label 供 PDF 内部跳转
    FootnoteDef { id: String, children: Vec<Block> },

    /// 列表项
    ///
    /// `marker` 为预生成的标记字符串（方案 §3.6 `Listitem { marker }`），
    /// 有序列表为 `"1."`/`"2."` 等，无序列表为 `"•"` 等，由 `from_ast` 注入。
    ListItem {
        marker: String,
        children: Vec<Block>,
    },

    /// 任务列表项（GFM 复选框）
    ///
    /// `marker` 同 [`BlockKind::ListItem`]，通常含复选框符号（`"☐ "`/`"☑ "`）。
    TaskListItem {
        marker: String,
        checked: bool,
        children: Vec<Block>,
    },

    /// 引用块
    Blockquote { children: Vec<Block> },

    /// 代码块
    ///
    /// `lines` 为语法高亮后的预排版文本行（由 `spans` 经 `from_ast::spans_to_lines`
    /// 排版得到；`spans` 来自富化阶段 [`crate::highlight`]），
    /// 前端/后端应优先消费 `lines` 而非重新排版 `code`。
    CodeBlock {
        code: String,
        lang: Option<String>,
        /// 语法高亮后的带色文本行（逐行一个 [`TextLine`]）
        lines: Vec<TextLine>,
    },

    /// 分隔线
    ThematicBreak,

    /// 图片
    Image(DocImage),

    /// 表格
    Table {
        rows: Vec<TableRow>,
        column_align: Vec<TextAlign>,
        /// 每列宽度（pt，按内容真实度量，见 `from_ast::compute_table_layout`）
        col_widths: Vec<f64>,
        /// 每行高度（pt，按列宽折行后的真实度量）
        row_heights: Vec<f64>,
    },

    /// 表格行
    TableRow { cells: Vec<TableCell> },

    /// 表格单元格
    TableCell { children: Vec<Block> },

    /// 纯文本（叶节点）
    Text { text: String },

    /// 行内代码
    InlineCode { code: String },

    /// 链接（叶节点，children 为空时显示 title/url）
    Link {
        url: String,
        title: Option<String>,
        children: Vec<Block>,
    },

    /// 行内换行
    LineBreak,

    /// 块级/行内容器（div/span/center 等）
    Container { children: Vec<Block> },
}

impl BlockKind {
    /// 返回该块变体包含的子块（无子块则返回空切片）。用于递归遍历（如收集链接）。
    pub fn children(&self) -> &[Block] {
        match self {
            BlockKind::Document { children }
            | BlockKind::Heading { children, .. }
            | BlockKind::List { children, .. }
            | BlockKind::ListItem { children, .. }
            | BlockKind::TaskListItem { children, .. }
            | BlockKind::Blockquote { children }
            | BlockKind::Container { children }
            | BlockKind::FootnoteDef { children, id: _ } => children,
            BlockKind::TableRow { .. } | BlockKind::Table { .. } => {
                // 表格内链接（如脚注引用出现在表格中）极少见，暂不在 children() 中展开，
                // 避免与 TableCell/TableRow 的不同容器类型冲突；不影响脚注主流程。
                &[]
            }
            BlockKind::TableCell { children } => children,
            _ => &[],
        }
    }

    pub fn text_content(&self) -> String {
        match self {
            BlockKind::Text { text } => text.clone(),
            BlockKind::InlineCode { code } => code.clone(),
            BlockKind::LineBreak => "\n".to_string(),
            BlockKind::Link { url, title, .. } => title.clone().unwrap_or_else(|| url.clone()),
            BlockKind::CodeBlock { code, .. } => code.clone(),
            BlockKind::Document { children }
            | BlockKind::Heading { children, .. }
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
            BlockKind::DefinitionList { items } => {
                let mut s = String::new();
                for item in items {
                    for b in &item.term {
                        s.push_str(&b.text_content());
                    }
                    for b in &item.definition {
                        s.push_str(&b.text_content());
                    }
                }
                s
            }
            BlockKind::FootnoteDef { children, .. } => {
                let mut s = String::new();
                for b in children {
                    s.push_str(&b.text_content());
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
    pub children: Vec<Block>,
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
