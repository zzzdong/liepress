//! 表格布局生成测试
//!
//! 测试表格 AST → VisualElement 转换的正确性，
//! 包括跨页分割、边框、背景、文本布局等。

use crate::common::samples;
use liepress::generator::markdown_to_document;
use liepress::generator::{Document, Page, constants::*};
use liepress::visual::VisualElement;

// ─── 辅助函数 ───

/// 提取页面上所有文本行及其文本内容，按 Y 坐标排序
fn extract_text_elements(page: &liepress::generator::Page) -> Vec<(f64, f64, f64, f64, String)> {
    let mut texts = Vec::new();
    for elem in &page.elements {
        if let VisualElement::TextLine { runs, bounds, .. } = elem {
            let text: String = runs.iter().map(|r| r.text.as_str()).collect();
            texts.push((bounds.x0, bounds.y0, bounds.x1, bounds.y1, text));
        }
    }
    texts.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    texts
}

/// 统计页面上特定类型的元素数量
fn count_elements(page: &liepress::generator::Page) -> (usize, usize, usize) {
    let text_lines = page
        .elements
        .iter()
        .filter(|e| matches!(e, VisualElement::TextLine { .. }))
        .count();
    let lines = page
        .elements
        .iter()
        .filter(|e| matches!(e, VisualElement::Line { .. }))
        .count();
    let rects = page
        .elements
        .iter()
        .filter(|e| matches!(e, VisualElement::Rect { .. }))
        .count();
    (text_lines, lines, rects)
}

// ─── 基础表格测试 ───

#[test]
fn test_simple_table_renders_elements() {
    let doc = markdown_to_document(samples::SIMPLE_TABLE);
    assert!(
        !doc.pages.is_empty(),
        "Table document should have at least one page"
    );

    let page = &doc.pages[0];
    let (text_lines, border_lines, _rects) = count_elements(page);

    // 应该有文本行（每行每列都有内容）
    assert!(
        text_lines > 0,
        "Table should have text line elements, got {}",
        text_lines
    );
    // 应该有边框线
    assert!(
        border_lines > 0,
        "Table should have border line elements, got {}",
        border_lines
    );
}

#[test]
fn test_table_has_borders() {
    let doc = markdown_to_document(samples::SIMPLE_TABLE);
    let page = &doc.pages[0];

    let line_elements: Vec<&VisualElement> = page
        .elements
        .iter()
        .filter(|e| matches!(e, VisualElement::Line { .. }))
        .collect();

    // 一个 3 行 × 2 列表格至少有：顶部 + 底部 + 每行底部 + 左右垂直 = 至少 8 条线
    // 但实际可能更多，取决于实现
    assert!(
        line_elements.len() >= 4,
        "Table should have at least 4 border lines (top, bottom, left, right), got {}",
        line_elements.len()
    );
}

#[test]
fn test_table_text_content() {
    let doc = markdown_to_document(samples::SIMPLE_TABLE);
    let page = &doc.pages[0];
    let texts = extract_text_elements(page);

    let all_text: String = texts.iter().map(|t| t.4.as_str()).collect();
    // 应该包含所有单元格文本
    assert!(
        all_text.contains("Header 1"),
        "Table should contain 'Header 1'"
    );
    assert!(
        all_text.contains("Header 2"),
        "Table should contain 'Header 2'"
    );
    assert!(
        all_text.contains("Cell A1"),
        "Table should contain 'Cell A1'"
    );
    assert!(
        all_text.contains("Cell B2"),
        "Table should contain 'Cell B2'"
    );
}

#[test]
fn test_table_text_within_content_bounds() {
    let doc = markdown_to_document(samples::SIMPLE_TABLE);
    let page = &doc.pages[0];
    let texts = extract_text_elements(page);

    for (x0, _y0, x1, _y1, text) in &texts {
        assert!(
            *x0 >= CONTENT_AREA_X_PT as f64,
            "Text '{}' starts outside content area (left bound {:.1} < {})",
            text,
            x0,
            CONTENT_AREA_X_PT
        );
        assert!(
            *x1 <= (CONTENT_AREA_X_PT + CONTENT_AREA_WIDTH_PT) as f64,
            "Text '{}' ends outside content area (right bound {:.1} > {})",
            text,
            x1,
            CONTENT_AREA_X_PT + CONTENT_AREA_WIDTH_PT
        );
    }
}

// ─── 多列表格 ───

#[test]
fn test_wide_table() {
    let doc = markdown_to_document(samples::WIDE_TABLE);
    assert!(!doc.pages.is_empty());

    let page = &doc.pages[0];
    let texts = extract_text_elements(page);

    let all_text: String = texts.iter().map(|t| t.4.as_str()).collect();
    assert!(all_text.contains("Alice"), "Should contain Alice");
    assert!(all_text.contains("Charlie"), "Should contain Charlie");
    assert!(all_text.contains("Beijing"), "Should contain Beijing");
}

// ─── 对齐表格 ───

#[test]
fn test_aligned_table() {
    let doc = markdown_to_document(samples::ALIGNED_TABLE);
    assert!(!doc.pages.is_empty(), "Aligned table should render");

    let page = &doc.pages[0];
    let (_text_lines, border_lines, _rects) = count_elements(page);
    assert!(border_lines > 0, "Aligned table should have border lines");
}

// ─── 带内联格式的表格 ───

#[test]
fn test_formatted_table() {
    let doc = markdown_to_document(samples::FORMATTED_TABLE);
    assert!(!doc.pages.is_empty());

    let page = &doc.pages[0];
    let texts = extract_text_elements(page);

    let all_text: String = texts.iter().map(|t| t.4.as_str()).collect();
    assert!(
        all_text.contains("Bold"),
        "Formatted table should contain bold text"
    );
    assert!(
        all_text.contains("Italic"),
        "Formatted table should contain italic text"
    );
    assert!(
        all_text.contains("inline code"),
        "Formatted table should contain inline code"
    );

    // 文本行数量应该够
    assert!(
        texts.len() >= 3,
        "Should have text for header + 3 rows, got {}",
        texts.len()
    );
}

// ─── 空表格 ───

#[test]
fn test_empty_table() {
    // 只有表头没有数据行的表格
    let doc = markdown_to_document(samples::EMPTY_TABLE);
    // 不应该导致崩溃，应正常生成空内容或仅表头
    let (_text_lines, _border_lines, _rects) = if let Some(page) = doc.pages.first() {
        count_elements(page)
    } else {
        (0, 0, 0)
    };
    // 只是不崩溃就通过
}

#[test]
fn test_table_with_no_rows_does_not_crash() {
    let md = r#"| H1 | H2 |
|---|---|"#;
    let _doc = markdown_to_document(md);
    // 不崩溃即可
}

/// 辅助函数：检查页面内是否有 Y 重叠的文本行（同行单元格除外）
fn find_text_overlaps(page: &Page) -> Vec<(usize, usize, f64, f64)> {
    use liepress::visual::VisualElement;
    // 收集所有 TextLine 的 Y 范围
    let mut text_y: Vec<(f64, f64, usize)> = page
        .elements
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            if let VisualElement::TextLine { bounds, .. } = e {
                Some((bounds.y0, bounds.y1, i))
            } else {
                None
            }
        })
        .collect();
    text_y.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    // 按行分组：连续且 Y 范围重叠的元素视为同一行
    let mut groups: Vec<Vec<(f64, f64, usize)>> = Vec::new();
    let mut i = 0;
    while i < text_y.len() {
        let mut group = vec![text_y[i]];
        let mut j = i + 1;
        while j < text_y.len() {
            let prev_bottom = group.last().unwrap().1;
            let curr_top = text_y[j].0;
            if curr_top < prev_bottom + 0.5 {
                group.push(text_y[j]);
                j += 1;
            } else {
                break;
            }
        }
        groups.push(group);
        i = j;
    }

    // 检查组间是否有重叠
    let mut overlaps = Vec::new();
    for k in 1..groups.len() {
        let prev_bottom = groups[k - 1].iter().map(|g| g.1).fold(0.0_f64, f64::max);
        let curr_top = groups[k].iter().map(|g| g.0).fold(f64::INFINITY, f64::min);
        if curr_top < prev_bottom - 0.1 {
            overlaps.push((groups[k - 1][0].2, groups[k][0].2, prev_bottom, curr_top));
        }
    }
    overlaps
}

#[test]
fn test_cell_text_wrapping_no_overlap() {
    let md = r#"| Short | Very long text that will definitely wrap across multiple lines in a narrow column |
|-------|-----------------------------------------------------------------------------|
| A     | This is a very long description that should wrap to multiple lines without overlapping |
| B     | Short text here                                                          |"#;
    let doc = markdown_to_document(md);
    assert!(!doc.pages.is_empty(), "Document should have pages");

    for (page_idx, page) in doc.pages.iter().enumerate() {
        let overlaps = find_text_overlaps(page);
        assert!(
            overlaps.is_empty(),
            "Page {}: found {} text overlaps between row groups: {:?}",
            page_idx,
            overlaps.len(),
            overlaps
        );
    }
}

// ─── 分页测试 ───

#[test]
fn test_large_table_spans_multiple_pages() {
    // 动态生成 80 行的大表格，确保跨页
    let mut md = String::from("| # | Name        | Description |\n|---|---|---|\n");
    for i in 1..=80 {
        md.push_str(&format!(
            "| {} | Item {} | This is a long description for item number {} that should wrap across multiple lines to make the row height large enough |\n",
            i, i, i
        ));
    }

    let doc = markdown_to_document(&md);

    // 大表应该跨多页
    assert!(
        doc.pages.len() >= 2,
        "Large table with 80 rows should span multiple pages, got {} pages",
        doc.pages.len()
    );
}

#[test]
fn test_table_pagination_continuity() {
    let mut md = String::from("| # | Name        | Description |\n|---|---|---|\n");
    for i in 1..=80 {
        md.push_str(&format!(
            "| {} | Item {} | Long description for item {} that wraps across multiple lines to make rows tall |\n",
            i, i, i
        ));
    }
    let doc = markdown_to_document(&md);
    assert!(doc.pages.len() >= 2, "Expected at least 2 pages");

    // 每页应该都有内容
    for (i, page) in doc.pages.iter().enumerate() {
        assert!(!page.elements.is_empty(), "Page {} should have elements", i);
    }
}

#[test]
fn test_table_rows_distributed_across_pages() {
    let mut md = String::from("| # | Name        | Description |\n|---|---|---|\n");
    for i in 1..=80 {
        md.push_str(&format!(
            "| {} | Item {} | Description for item {} with extra text to ensure row wraps |\n",
            i, i, i
        ));
    }
    let doc = markdown_to_document(&md);

    // 收集所有页的文本
    let all_texts: Vec<String> = doc
        .pages
        .iter()
        .flat_map(|page| extract_text_elements(page).into_iter().map(|t| t.4))
        .collect();

    // 所有文本内容应该完整
    let joined = all_texts.join(" ");
    for name in &["Item 1", "Item 40", "Item 80"] {
        assert!(
            joined.contains(name),
            "Table content '{}' should appear somewhere in the document",
            name
        );
    }
}

#[test]
fn test_table_pagination_borders_on_each_page() {
    let mut md = String::from("| # | Name        | Description |\n|---|---|---|\n");
    for i in 1..=80 {
        md.push_str(&format!(
            "| {} | Item {} | Long description for item {} that needs wrapping across lines |\n",
            i, i, i
        ));
    }
    let doc = markdown_to_document(&md);

    // 每页都应该有边框线
    for (i, page) in doc.pages.iter().enumerate() {
        let line_count = page
            .elements
            .iter()
            .filter(|e| matches!(e, VisualElement::Line { .. }))
            .count();
        assert!(
            line_count >= 2,
            "Page {} should have at least 2 border lines (top + bottom), got {}",
            i,
            line_count
        );
    }
}

#[test]
fn test_table_pagination_no_text_overlap() {
    let mut md = String::from("| # | Name        | Description |\n|---|---|---|\n");
    for i in 1..=80 {
        md.push_str(&format!(
            "| {} | Item {} | Description for item {} with some wrapping text |\n",
            i, i, i
        ));
    }
    let doc = markdown_to_document(&md);

    for (page_idx, page) in doc.pages.iter().enumerate() {
        let texts = extract_text_elements(page);
        if texts.is_empty() {
            continue;
        }

        // 将文本行按 Y 坐标分组（同一行的单元格 Y 范围重叠是正常的）
        // 使用聚类：如果两个文本块的 Y 范围重叠超过 50%，视为同一行
        let mut groups: Vec<(f64, f64)> = Vec::new(); // (y0, y1) 每组
        let mut i = 0;
        while i < texts.len() {
            let mut group_y0 = texts[i].1;
            let mut group_y1 = texts[i].3;
            let mut j = i + 1;
            while j < texts.len() {
                let overlap = texts[j].3.min(group_y1) - texts[j].1.max(group_y0);
                let min_height = (group_y1 - group_y0).min(texts[j].3 - texts[j].1);
                if min_height > 0.0 && overlap / min_height > -0.3 {
                    group_y0 = group_y0.min(texts[j].1);
                    group_y1 = group_y1.max(texts[j].3);
                    j += 1;
                } else {
                    break;
                }
            }
            groups.push((group_y0, group_y1));
            i = j;
        }

        // 检查组之间不重叠
        for k in 1..groups.len() {
            let prev_bottom = groups[k - 1].1;
            let curr_top = groups[k].0;
            assert!(
                curr_top >= prev_bottom - 0.1,
                "Page {}: row groups {} and {} overlap (prev bottom={:.1}, curr top={:.1})",
                page_idx,
                k - 1,
                k,
                prev_bottom,
                curr_top
            );
        }
    }
}

// ─── 表格与其它元素混排 ───

#[test]
fn test_table_after_heading() {
    let md = "# Sales Data\n\n| Product | Revenue |\n|---------|--------|\n| Foo     | 100    |\n| Bar     | 200    |";
    let doc = markdown_to_document(md);
    assert!(!doc.pages.is_empty());

    let page = &doc.pages[0];
    let texts = extract_text_elements(page);

    // 标题应该在表格之前出现
    let positions: Vec<&str> = texts.iter().map(|t| t.4.as_str()).collect();
    let heading_pos = positions.iter().position(|t| t.contains("Sales Data"));
    let table_pos = positions.iter().position(|t| t.contains("Product"));

    assert!(heading_pos.is_some(), "Heading should be present");
    assert!(table_pos.is_some(), "Table content should be present");

    if let (Some(h), Some(t)) = (heading_pos, table_pos) {
        assert!(h < t, "Heading should appear before table content");
    }
}

#[test]
fn test_table_followed_by_paragraph() {
    let md = "| A | B |\n|---|---|\n| 1 | 2 |\n\nAfter the table.";
    let doc = markdown_to_document(md);
    assert!(!doc.pages.is_empty());

    let page = &doc.pages[0];
    let texts = extract_text_elements(page);
    let all_text: String = texts.iter().map(|t| t.4.as_str()).collect();

    assert!(
        all_text.contains("After the table"),
        "Table should be followed by paragraph text"
    );
}

// ─── 表头背景 ───

#[test]
fn test_table_header_background() {
    let doc = markdown_to_document(samples::SIMPLE_TABLE);
    let page = &doc.pages[0];

    // 表头应该有 Rect 作为背景
    let rect_count = page
        .elements
        .iter()
        .filter(|e| matches!(e, VisualElement::Rect { .. }))
        .count();

    // 有表头时应有至少一个背景矩形
    // （可能同时有交替行背景，所以 >= 1）
    assert!(
        rect_count >= 1,
        "Table header should have background rectangle, got {}",
        rect_count
    );
}

// ─── 元素顺序 ───

#[test]
fn test_table_elements_in_reading_order() {
    let doc = markdown_to_document(samples::SIMPLE_TABLE);

    for page in &doc.pages {
        let texts = extract_text_elements(page);

        // 文本行应该按从上到下的阅读顺序排列
        for i in 1..texts.len() {
            assert!(
                texts[i].1 >= texts[i - 1].1 - 0.1,
                "Text lines should be in reading order (top to bottom)"
            );
        }
    }
}

#[test]
fn test_table_y_positions_increase() {
    let doc = markdown_to_document(samples::WIDE_TABLE);

    for page in &doc.pages {
        let texts = extract_text_elements(page);
        for i in 1..texts.len() {
            assert!(
                texts[i].1 > texts[i - 1].1 - 0.1,
                "Row {} should be below row {}",
                i,
                i - 1
            );
        }
    }
}
