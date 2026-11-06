use wasm_bindgen::prelude::*;

use sparkscope::i18n::{self, Lang};
use sparkscope::{loader, pipeline, webui};

pub fn analyze_to_json(
    bytes: &[u8],
    name: &str,
    key: Option<String>,
    home: &str,
    lang: &str,
) -> Result<String, String> {
    let _guard = i18n::scoped(Lang::parse(lang));
    let mut loaded = loader::load_bytes(bytes, name).map_err(|e| format!("{e:#}"))?;
    if let Some(k) = &key {
        loaded.origin = format!("https://spark.lucko.me/{k}");
        loaded.viewer_url = Some(format!("https://spark.lucko.me/{k}"));
    }
    let (report, _, heap) = pipeline::build(loaded, None).map_err(|e| format!("{e:#}"))?;
    let out = webui::render_report(&report, heap.as_ref());
    let html = webui::with_static_actions(&out.html, home);
    Ok(serde_json::json!({
        "html": html,
        "title": out.title,
        "detail": out.detail,
        "bad": out.bad,
    })
    .to_string())
}

#[wasm_bindgen]
pub fn analyze(
    bytes: &[u8],
    name: &str,
    key: Option<String>,
    home: String,
    lang: String,
) -> Result<String, JsValue> {
    analyze_to_json(bytes, name, key, &home, &lang).map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
