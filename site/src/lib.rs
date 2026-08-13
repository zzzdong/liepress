use liepress::markdown_to_pdf;
use liepress::{ConvertOptions, document};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn register_font_bytes(family_name: &str, bytes: &[u8]) -> Result<(), JsValue> {
    document::text::register_font(document::text::FontSource::Memory(bytes.to_vec()), Some(family_name))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

fn into_options(font_family: &str, css: &str) -> ConvertOptions {
    let mut opts = ConvertOptions::new();
    let families: Vec<&str> = if font_family.trim().is_empty() {
        Vec::new()
    } else {
        font_family.split(',').map(|s| s.trim()).collect()
    };
    if !families.is_empty() {
        opts = opts.with_font_family(&families);
    }
    if !css.is_empty() {
        opts = opts.with_css(css);
    }
    opts
}

#[wasm_bindgen]
pub fn markdown_to_pdf_base64(md: &str, font_family: &str, css: &str) -> Result<String, JsValue> {
    let opts = into_options(font_family, css);
    let bytes = markdown_to_pdf(md, &opts).map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &bytes,
    ))
}
