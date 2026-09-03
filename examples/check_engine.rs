use liepress::css::engine::CssEngine;
use liepress::ast::presets::DEFAULT_CSS;
use liepress::ast::style::Style;

fn main() {
    let css = ".highlight { background-color: #ffffcc; padding: 2pt 4pt; }\n\
               .warning { font-weight: bold; color: #cc0000; }\n\
               .tagline { font-style: italic; color: #4a90d9; }\n";
    let mut engine = CssEngine::new(DEFAULT_CSS).expect("new");
    engine = engine.with_user_css(css).expect("user");

    let parent = Style::default();
    let classes = vec!["highlight".to_string()];
    let s = engine.resolve_style("span", &classes, None, &[], &parent);
    println!("span.highlight bg = {:?} color = {:?}", s.background_color, s.color);

    let classes2 = vec!["warning".to_string()];
    let s2 = engine.resolve_style("span", &classes2, None, &[], &parent);
    println!("span.warning bg = {:?} color = {:?}", s2.background_color, s2.color);

    let classes3 = vec!["tagline".to_string()];
    let s3 = engine.resolve_style("span", &classes3, None, &[], &parent);
    println!("span.tagline bg = {:?} color = {:?}", s3.background_color, s3.color);
}
