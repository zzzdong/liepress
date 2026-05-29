use liepress::visual::VisualElement;
use liepress::generator::markdown_to_document;

#[test]
fn diag_trace_url() {
    // 简单链接
    let md1 = r#"[Example](http://example.com)"#;
    let doc1 = markdown_to_document(md1);
    println!("=== Simple link ===");
    for page in &doc1.pages {
        for elem in &page.elements {
            if let VisualElement::TextLine { runs, bounds, .. } = elem {
                println!("  TextLine bounds={:?}", bounds);
                for (ri, run) in runs.iter().enumerate() {
                    println!("    Run[{}]: text={:?}, url={:?}, baseline_x={}, advance={}, font_size={}", 
                        ri, run.text, run.url, run.baseline_x, run.advance, run.font_size);
                }
            }
        }
    }

    // 段落中的链接
    let md2 = r#"This is a paragraph with a [link](http://example.com) in the middle."#;
    let doc2 = markdown_to_document(md2);
    println!("=== Link in paragraph ===");
    for page in &doc2.pages {
        for elem in &page.elements {
            if let VisualElement::TextLine { runs, bounds, .. } = elem {
                println!("  TextLine bounds={:?}", bounds);
                for (ri, run) in runs.iter().enumerate() {
                    println!("    Run[{}]: text={:?}, url={:?}, baseline_x={}, advance={}, font_size={}", 
                        ri, run.text, run.url, run.baseline_x, run.advance, run.font_size);
                }
            }
        }
    }
}