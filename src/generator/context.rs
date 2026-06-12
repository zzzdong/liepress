//! 布局上下文
//!
//! 管理文档生成过程中的状态，包括缩进和文档大纲。

/// 大纲条目（树形结构）
///
/// children 包含该条目的子标题（level 更高的相邻条目）。
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineEntry {
    pub level: u8,
    pub title: String,
    pub page_number: usize,
    /// 标题在页面上的位置（pt，从页面左上角算起）
    pub x_position: f32,
    pub y_position: f32,
    /// 子标题（level 更高的条目）
    pub children: Vec<OutlineEntry>,
}

/// 布局上下文，管理文档生成过程中的状态
#[derive(Debug, Clone)]
pub struct LayoutContext {
    /// 当前缩进（pt）
    pub(crate) current_indent: f32,
    /// 文档大纲（树形结构）
    outline: Vec<OutlineEntry>,
}

impl Default for LayoutContext {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutContext {
    pub fn new() -> Self {
        Self {
            current_indent: 0.0,
            outline: Vec::new(),
        }
    }

    /// 在指定缩进上下文中执行闭包，执行完毕后恢复之前的缩进。
    pub fn with_indent<R>(&mut self, indent: f32, f: impl FnOnce(&mut Self) -> R) -> R {
        let prev = self.current_indent;
        self.current_indent = indent;
        let result = f(self);
        self.current_indent = prev;
        result
    }

    /// 在当前缩进基础上增加偏移量，执行闭包后恢复。
    pub fn with_additional_indent<R>(
        &mut self,
        additional: f32,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let prev = self.current_indent;
        self.current_indent += additional;
        let result = f(self);
        self.current_indent = prev;
        result
    }

    /// 记录一个标题到大纲（树形结构）
    ///
    /// 根据 level 自动将新条目插入到树中的正确位置：
    /// - level > 上一个条目的 level：作为上一个条目的子节点
    /// - level <= 上一个条目的 level：作为同级节点
    pub fn record_heading(
        &mut self,
        level: u8,
        title: String,
        page_number: usize,
        x_position: f32,
        y_position: f32,
    ) {
        let entry = OutlineEntry {
            level,
            title,
            page_number,
            x_position,
            y_position,
            children: Vec::new(),
        };
        Self::insert_into_siblings(&mut self.outline, entry);
    }

    /// 递归地将新条目插入到兄弟节点列表中合适的位置
    fn insert_into_siblings(siblings: &mut Vec<OutlineEntry>, new_entry: OutlineEntry) {
        if siblings.is_empty() {
            siblings.push(new_entry);
            return;
        }

        let last_level = siblings.last().unwrap().level;

        if new_entry.level > last_level {
            // 新条目是最后一个兄弟节点的子节点
            Self::insert_into_siblings(&mut siblings.last_mut().unwrap().children, new_entry);
        } else {
            // 新条目是同级（或更高级别），作为兄弟节点
            siblings.push(new_entry);
        }
    }

    /// 获取大纲条目（根级别列表）
    pub fn outline(&self) -> &[OutlineEntry] {
        &self.outline
    }
}
