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
    let content_type = loader::spark_content_type(&loaded.payload);
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
        "ctype": content_type,
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

pub fn describe_heap_json(bytes: &[u8], name: &str, lang: &str) -> Result<String, String> {
    let _guard = i18n::scoped(Lang::parse(lang));
    let info = sparkscope::heapdiff::describe_bytes(bytes, name).map_err(|e| format!("{e:#}"))?;
    Ok(serde_json::json!({
        "name": name,
        "detail": sparkscope::heapdiff::describe_label(&info),
        "size": bytes.len(),
    })
    .to_string())
}

pub fn compare_heaps_json(
    a: &[u8],
    name_a: &str,
    b: &[u8],
    name_b: &str,
    home: &str,
    lang: &str,
) -> Result<String, String> {
    let _guard = i18n::scoped(Lang::parse(lang));
    let c =
        sparkscope::heapdiff::compare_bytes(a, name_a, b, name_b).map_err(|e| format!("{e:#}"))?;
    let out = webui::render_comparison(&c);
    Ok(serde_json::json!({
        "html": webui::with_static_actions(&out.html, home),
        "title": out.title,
        "detail": out.detail,
        "bad": out.bad,
    })
    .to_string())
}

#[wasm_bindgen]
pub fn describe_heap(bytes: &[u8], name: &str, lang: String) -> Result<String, JsValue> {
    describe_heap_json(bytes, name, &lang).map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn compare_heaps(
    a: &[u8],
    name_a: &str,
    b: &[u8],
    name_b: &str,
    home: String,
    lang: String,
) -> Result<String, JsValue> {
    compare_heaps_json(a, name_a, b, name_b, &home, &lang).map_err(|e| JsValue::from_str(&e))
}

/// Streaming heap dump (.hprof) reader: feed the file in chunks, then `finish`
/// returns the class histogram encoded as a spark heap summary.
#[wasm_bindgen]
pub struct HprofReader {
    inner: Option<sparkscope::hprof::HprofStream>,
}

impl Default for HprofReader {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl HprofReader {
    #[wasm_bindgen(constructor)]
    pub fn new() -> HprofReader {
        HprofReader {
            inner: Some(sparkscope::hprof::HprofStream::new()),
        }
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Result<(), JsValue> {
        match self.inner.as_mut() {
            Some(s) => s
                .feed(chunk)
                .map_err(|e| JsValue::from_str(&format!("{e:#}"))),
            None => Err(JsValue::from_str("finished")),
        }
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, JsValue> {
        let s = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("finished"))?;
        let h = s
            .finish()
            .map_err(|e| JsValue::from_str(&format!("{e:#}")))?;
        Ok(prost::Message::encode_to_vec(&h))
    }
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
