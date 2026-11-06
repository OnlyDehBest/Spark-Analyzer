use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::i18n::{self, Lang};
use crate::loader;
use crate::pipeline;
use crate::report::html::icon;
use crate::webui;

const MAX_UPLOAD: usize = 512 * 1024 * 1024;
const MAX_STORED: usize = 30;

struct Stored {
    id: String,
    title: String,
    detail: String,
    bad: bool,
    bytes: Arc<Vec<u8>>,
    name: String,
    key: Option<String>,
    cache: HashMap<Lang, String>,
}

struct State {
    bytebin: String,
    reports: Mutex<Vec<Stored>>,
    counter: AtomicU64,
}

pub struct ServeOptions {
    pub host: String,
    pub port: u16,
    pub open: bool,
    pub bytebin: String,
}

pub fn serve(o: ServeOptions) -> Result<()> {
    let mut server = None;
    let mut port = o.port;
    for p in o.port..o.port.saturating_add(20) {
        if let Ok(s) = Server::http((o.host.as_str(), p)) {
            server = Some(s);
            port = p;
            break;
        }
    }
    let server =
        server.ok_or_else(|| anyhow!("nessuna porta libera tra {} e {}", o.port, o.port + 19))?;
    let shown_host = if o.host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        o.host.as_str()
    };
    let url = format!("http://{shown_host}:{port}/");
    eprintln!("Spark-Analyzer è in esecuzione: apri {url}  (running: open {url})");
    eprintln!("Lascia aperta questa finestra; chiudila (o premi Ctrl+C) per fermarlo.");
    if o.host != "127.0.0.1" && o.host != "localhost" {
        eprintln!(
            "Attenzione: il sito è raggiungibile anche da altri dispositivi della rete su {}:{port}",
            o.host
        );
    }
    if o.open {
        let _ = webbrowser::open(&url);
    }
    let state = Arc::new(State {
        bytebin: o.bytebin,
        reports: Mutex::new(Vec::new()),
        counter: AtomicU64::new(1),
    });
    for req in server.incoming_requests() {
        let st = Arc::clone(&state);
        std::thread::spawn(move || handle(req, &st));
    }
    Ok(())
}

fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).expect("header valido")
}

fn html_resp(body: String) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(body.into_bytes())
        .with_header(header("Content-Type", "text/html; charset=utf-8"))
}

fn json_resp(code: u16, v: serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(v.to_string().into_bytes())
        .with_status_code(code)
        .with_header(header("Content-Type", "application/json"))
}

fn query_param(url: &str, key: &str) -> Option<String> {
    let q = url.split_once('?')?.1;
    for pair in q.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k == key {
            return Some(percent_decode(v));
        }
    }
    None
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let hex = if c == b'%' && i + 2 < bytes.len() {
            std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        } else {
            None
        };
        match (c, hex) {
            (_, Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b'+', None) => {
                out.push(b' ');
                i += 1;
            }
            (c, None) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn handle(mut req: Request, st: &State) {
    let url = req.url().to_string();
    let lang = Lang::parse(&query_param(&url, "lang").unwrap_or_default());
    let _guard = i18n::scoped(lang);
    let path = url.split('?').next().unwrap_or("/").to_string();
    let method = req.method().clone();
    let resp = match (method, path.as_str()) {
        (Method::Get, "/") => html_resp(index_page(st)),
        (Method::Get, "/favicon.ico") => Response::from_data(Vec::new()).with_status_code(204),
        (Method::Post, "/api/link") => {
            let mut body = String::new();
            let _ = req.as_reader().take(8192).read_to_string(&mut body);
            let link = body.trim().to_string();
            let result = loader::fetch_link(&link, &st.bytebin)
                .and_then(|(bytes, key)| store(st, bytes, key.clone(), Some(key)));
            api_result(result)
        }
        (Method::Post, "/api/file") => {
            let name = query_param(&url, "name").unwrap_or_else(|| "profile".into());
            let mut bytes = Vec::new();
            let read = req
                .as_reader()
                .take(MAX_UPLOAD as u64 + 1)
                .read_to_end(&mut bytes);
            let result = match read {
                Err(e) => Err(anyhow!(
                    "{}",
                    tf!("upload interrotto: {e}", "upload interrupted: {e}")
                )),
                Ok(_) if bytes.len() > MAX_UPLOAD => Err(anyhow!(
                    "{}",
                    i18n::t(
                        "file troppo grande (massimo 512 MB)",
                        "file too large (max 512 MB)"
                    )
                )),
                Ok(_) if bytes.is_empty() => Err(anyhow!(
                    "{}",
                    i18n::t("il file è vuoto", "the file is empty")
                )),
                Ok(_) => store(st, bytes, name, None),
            };
            api_result(result)
        }
        (Method::Get, p) if p.starts_with("/r/") => {
            let id = p[3..].trim_end_matches('/').to_string();
            match report_html(st, &id, lang) {
                Some(Ok(h)) => html_resp(with_actions(&h)),
                Some(Err(e)) => html_resp(format!(
                    "<pre>{}</pre>",
                    crate::report::html::esc(&format!("{e:#}"))
                ))
                .with_status_code(500),
                None => html_resp(not_found()).with_status_code(404),
            }
        }
        _ => html_resp(not_found()).with_status_code(404),
    };
    let _ = req.respond(resp);
}

fn render(bytes: &[u8], name: &str, key: Option<&str>) -> Result<webui::Rendered> {
    let mut loaded = loader::load_bytes(bytes, name)?;
    if let Some(k) = key {
        loaded.origin = format!("https://spark.lucko.me/{k}");
        loaded.viewer_url = Some(format!("https://spark.lucko.me/{k}"));
    }
    let (r, _, heap_report) = pipeline::build(loaded, None)?;
    Ok(webui::render_report(&r, heap_report.as_ref()))
}

fn report_html(st: &State, id: &str, lang: Lang) -> Option<Result<String>> {
    let (bytes, name, key) = {
        let reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
        let r = reports.iter().find(|r| r.id == id)?;
        if let Some(h) = r.cache.get(&lang) {
            return Some(Ok(h.clone()));
        }
        (Arc::clone(&r.bytes), r.name.clone(), r.key.clone())
    };
    let out = match render(&bytes, &name, key.as_deref()) {
        Ok(o) => o,
        Err(e) => return Some(Err(e)),
    };
    let mut reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = reports.iter_mut().find(|r| r.id == id) {
        r.cache.insert(lang, out.html.clone());
    }
    Some(Ok(out.html))
}

fn api_result(r: Result<String>) -> Response<std::io::Cursor<Vec<u8>>> {
    match r {
        Ok(id) => json_resp(200, serde_json::json!({ "id": id })),
        Err(e) => json_resp(400, serde_json::json!({ "error": format!("{e:#}") })),
    }
}

fn store(st: &State, bytes: Vec<u8>, name: String, key: Option<String>) -> Result<String> {
    let out = render(&bytes, &name, key.as_deref())?;
    let n = st.counter.fetch_add(1, Ordering::Relaxed);
    let id = format!(
        "{n}{:x}",
        (bytes.len() as u64 ^ n.wrapping_mul(2_654_435_761)) & 0xffff
    );
    let mut cache = HashMap::new();
    cache.insert(i18n::lang(), out.html);
    let mut reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
    reports.insert(
        0,
        Stored {
            id: id.clone(),
            title: out.title,
            detail: out.detail,
            bad: out.bad,
            bytes: Arc::new(bytes),
            name,
            key,
            cache,
        },
    );
    reports.truncate(MAX_STORED);
    Ok(id)
}

fn index_page(st: &State) -> String {
    let reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
    let recent: Vec<webui::Recent> = reports
        .iter()
        .map(|r| webui::Recent {
            href: if i18n::is_en() {
                format!("/r/{}?lang=en", r.id)
            } else {
                format!("/r/{}", r.id)
            },
            title: r.title.clone(),
            detail: r.detail.clone(),
            bad: r.bad,
        })
        .collect();
    webui::local_index(&recent)
}

fn not_found() -> String {
    webui::local_not_found()
}

fn with_actions(html: &str) -> String {
    html.replacen(
        r#"<div class="actions">"#,
        &format!(
            r#"<div class="actions"><a class="btn sec" href="{}">{}{}</a>"#,
            if i18n::is_en() { "/?lang=en" } else { "/" },
            icon("plus"),
            i18n::t("Nuova analisi", "New analysis")
        ),
        1,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn decode() {
        assert_eq!(super::percent_decode("a%20b%2Fc+d"), "a b/c d");
        assert_eq!(super::percent_decode("100%"), "100%");
    }
}
