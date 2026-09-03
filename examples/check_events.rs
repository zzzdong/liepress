use pulldown_cmark::{Event, Options, Parser};

fn main() {
    let input = "文本<span class=\"highlight\">高亮文本</span>和<code>x</code>。\n\n<style> .highlight { background: #ffffcc } </style>\n";

    // 复刻 markdown.rs 当前使用的 options
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_FOOTNOTES;

    println!("===== current markdown.rs options =====");
    for ev in Parser::new_ext(input, opts) {
        print_event(&ev);
    }
}

fn print_event(ev: &Event) {
    match ev {
        Event::Html(s) => println!("Html: {:?}", s),
        Event::InlineHtml(s) => println!("InlineHtml: {:?}", s),
        Event::Text(s) => println!("Text: {:?}", s),
        Event::Start(t) => println!("Start: {:?}", t),
        Event::End(t) => println!("End: {:?}", t),
        Event::Code(s) => println!("Code: {:?}", s),
        other => println!("{:?}", other),
    }
}
