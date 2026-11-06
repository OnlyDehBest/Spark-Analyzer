use wasm_bindgen::prelude::*;

use sparkscope::i18n::{self, Lang};
use sparkscope::{loader, pipeline, webui};

pub fn parse_windows(spec: &str) -> Option<Vec<usize>> {
    let v: Vec<usize> = spec
        .split(',')
        .filter_map(|x| x.trim().parse().ok())
        .collect();
    if v.is_empty() { None } else { Some(v) }
}

pub fn analyze_to_json(
    bytes: &[u8],
    name: &str,
    key: Option<String>,
    home: &str,
    lang: &str,
) -> Result<String, String> {
    analyze_windows_to_json(bytes, name, key, home, lang, "")
}

pub fn analyze_windows_to_json(
    bytes: &[u8],
    name: &str,
    key: Option<String>,
    home: &str,
    lang: &str,
    windows: &str,
) -> Result<String, String> {
    let _guard = i18n::scoped(Lang::parse(lang));
    let mut loaded = loader::load_bytes(bytes, name).map_err(|e| format!("{e:#}"))?;
    if let Some(k) = &key {
        loaded.origin = format!("https://spark.lucko.me/{k}");
        loaded.viewer_url = Some(format!("https://spark.lucko.me/{k}"));
    }
    let win = parse_windows(windows);
    let (report, _, heap) =
        pipeline::build_with(loaded, None, win.as_deref()).map_err(|e| format!("{e:#}"))?;
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
    windows: Option<String>,
) -> Result<String, JsValue> {
    analyze_windows_to_json(
        bytes,
        name,
        key,
        &home,
        &lang,
        windows.as_deref().unwrap_or(""),
    )
    .map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
