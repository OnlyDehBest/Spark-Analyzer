use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use tiny_http::{Header, Method, Request, Response, Server};

use crate::i18n::{self, Lang};
use crate::loader;
use crate::pipeline;
use crate::report::html::icon;
use crate::webui;

const MAX_UPLOAD: usize = 512 * 1024 * 1024;
const MAX_STORED: usize = 30;
const MAX_STAGED: usize = 12;

struct Staged {
    id: String,
    bytes: Arc<Vec<u8>>,
    name: String,
    key: Option<String>,
}

struct Stored {
    id: String,
    title: String,
    detail: String,
    bad: bool,
    bytes: Arc<Vec<u8>>,
    name: String,
    key: Option<String>,
    second: Option<(Arc<Vec<u8>>, String)>,
    second_key: Option<String>,
    cache: HashMap<(Lang, String), Arc<Vec<u8>>>,
}

struct State {
    bytebin: String,
    reports: Mutex<Vec<Stored>>,
    staged: Mutex<Vec<Staged>>,
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
        staged: Mutex::new(Vec::new()),
        // id unici anche dopo un riavvio: le pagine /r/ restano in cache del browser
        counter: AtomicU64::new(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |d| d.as_secs()),
        ),
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

type Resp = Response<std::io::Cursor<Vec<u8>>>;

fn gzip(b: &[u8]) -> Vec<u8> {
    let mut e = GzEncoder::new(Vec::new(), Compression::default());
    let _ = e.write_all(b);
    e.finish().unwrap_or_default()
}

fn text_resp(data: Vec<u8>, gz: bool) -> Resp {
    let r = Response::from_data(data)
        .with_header(header("Content-Type", "text/html; charset=utf-8"))
        .with_header(header("Vary", "Accept-Encoding"));
    if gz {
        r.with_header(header("Content-Encoding", "gzip"))
    } else {
        r
    }
}

fn html_resp(body: String, gz: bool) -> Resp {
    text_resp(
        if gz {
            gzip(body.as_bytes())
        } else {
            body.into_bytes()
        },
        gz,
    )
}

fn page_resp(cached: &[u8], gz: bool) -> Resp {
    let mut plain = Vec::new();
    if !gz {
        let _ = GzDecoder::new(cached).read_to_end(&mut plain);
    }
    text_resp(if gz { cached.to_vec() } else { plain }, gz)
}

fn final_page(html: &str) -> Arc<Vec<u8>> {
    Arc::new(gzip(with_actions(html).as_bytes()))
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
    let gz = req
        .headers()
        .iter()
        .any(|h| h.field.equiv("Accept-Encoding") && h.value.as_str().contains("gzip"));
    let resp = match (method, path.as_str()) {
        (Method::Get, "/") => {
            html_resp(index_page(st), gz).with_header(header("Cache-Control", "no-cache"))
        }
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
            let result = read_upload(&mut req).and_then(|bytes| store(st, bytes, name, None));
            api_result(result)
        }
        (Method::Post, "/api/heap/link") => {
            let mut body = String::new();
            let _ = req.as_reader().take(8192).read_to_string(&mut body);
            let result = loader::fetch_link(body.trim(), &st.bytebin)
                .and_then(|(bytes, key)| stage(st, bytes, key.clone(), Some(key)));
            staged_result(result)
        }
        (Method::Post, "/api/heap/file") => {
            let name = query_param(&url, "name").unwrap_or_else(|| "heap.sparkheap".into());
            let result = read_upload(&mut req).and_then(|bytes| stage(st, bytes, name, None));
            staged_result(result)
        }
        (Method::Post, "/api/heap/compare") => {
            let a = query_param(&url, "a").unwrap_or_default();
            let b = query_param(&url, "b").unwrap_or_default();
            api_result(store_comparison(st, &a, &b))
        }
        (m @ (Method::Get | Method::Post), p) if p.starts_with("/api/share/") => {
            let id = p["/api/share/".len()..].to_string();
            let windows = query_param(&url, "w").unwrap_or_default();
            let upload = m == Method::Post;
            match share_link(st, &id, &windows, upload) {
                Ok(link) => json_resp(200, serde_json::json!({ "url": link })),
                Err(e) => json_resp(400, serde_json::json!({ "error": format!("{e:#}") })),
            }
        }
        (Method::Get, p) if p.starts_with("/r/") => {
            let id = p[3..].trim_end_matches('/').to_string();
            match report_html(st, &id, lang, &query_param(&url, "w").unwrap_or_default()) {
                Some(Ok(h)) => {
                    page_resp(&h, gz).with_header(header("Cache-Control", "private, max-age=3600"))
                }
                Some(Err(e)) => html_resp(
                    format!("<pre>{}</pre>", crate::report::html::esc(&format!("{e:#}"))),
                    gz,
                )
                .with_status_code(500),
                None => html_resp(not_found(), gz).with_status_code(404),
            }
        }
        _ => html_resp(not_found(), gz).with_status_code(404),
    };
    let _ = req.respond(resp);
}

fn read_upload(req: &mut Request) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let read = req
        .as_reader()
        .take(MAX_UPLOAD as u64 + 1)
        .read_to_end(&mut bytes);
    match read {
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
        Ok(_) => Ok(bytes),
    }
}

fn next_id(st: &State, len: usize) -> String {
    let n = st.counter.fetch_add(1, Ordering::Relaxed);
    format!(
        "{n}{:x}",
        (len as u64 ^ n.wrapping_mul(2_654_435_761)) & 0xffff
    )
}

struct StageInfo {
    id: String,
    name: String,
    detail: String,
    size: usize,
}

fn stage(st: &State, bytes: Vec<u8>, name: String, key: Option<String>) -> Result<StageInfo> {
    let info = crate::heapdiff::describe_bytes(&bytes, &name)?;
    let id = next_id(st, bytes.len());
    let size = bytes.len();
    let mut staged = st.staged.lock().unwrap_or_else(|e| e.into_inner());
    staged.insert(
        0,
        Staged {
            id: id.clone(),
            bytes: Arc::new(bytes),
            name: name.clone(),
            key,
        },
    );
    staged.truncate(MAX_STAGED);
    Ok(StageInfo {
        id,
        name,
        detail: crate::heapdiff::describe_label(&info),
        size,
    })
}

fn staged_result(r: Result<StageInfo>) -> Response<std::io::Cursor<Vec<u8>>> {
    match r {
        Ok(s) => json_resp(
            200,
            serde_json::json!({ "stage": s.id, "name": s.name, "detail": s.detail, "size": s.size }),
        ),
        Err(e) => json_resp(400, serde_json::json!({ "error": format!("{e:#}") })),
    }
}

fn staged_bytes(st: &State, id: &str) -> Result<(Arc<Vec<u8>>, String, Option<String>)> {
    let staged = st.staged.lock().unwrap_or_else(|e| e.into_inner());
    staged
        .iter()
        .find(|s| s.id == id)
        .map(|s| (Arc::clone(&s.bytes), s.name.clone(), s.key.clone()))
        .ok_or_else(|| {
            anyhow!(
                "{}",
                i18n::t(
                    "file non più disponibile: ricaricalo",
                    "file no longer available: load it again"
                )
            )
        })
}

fn render_comparison(a: &[u8], name_a: &str, b: &[u8], name_b: &str) -> Result<webui::Rendered> {
    let c = crate::heapdiff::compare_bytes(a, name_a, b, name_b)?;
    Ok(webui::render_comparison(&c))
}

fn store_comparison(st: &State, a: &str, b: &str) -> Result<String> {
    let (bytes_a, name_a, key_a) = staged_bytes(st, a)?;
    let (bytes_b, name_b, key_b) = staged_bytes(st, b)?;
    let out = render_comparison(&bytes_a, &name_a, &bytes_b, &name_b)?;
    let id = next_id(st, bytes_a.len() + bytes_b.len());
    let mut cache = HashMap::new();
    cache.insert((i18n::lang(), String::new()), final_page(&out.html));
    let mut reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
    reports.insert(
        0,
        Stored {
            id: id.clone(),
            title: out.title,
            detail: out.detail,
            bad: out.bad,
            bytes: bytes_a,
            name: name_a,
            key: key_a,
            second: Some((bytes_b, name_b)),
            second_key: key_b,
            cache,
        },
    );
    reports.truncate(MAX_STORED);
    Ok(id)
}

fn upload_payload(st: &State, bytes: &[u8], name: &str) -> Result<String> {
    let loaded = loader::load_bytes(bytes, name)?;
    loader::upload(
        bytes,
        loader::spark_content_type(&loaded.payload),
        &st.bytebin,
    )
}

fn public_link(key: &str, second: Option<&str>, windows: &str) -> String {
    let site = crate::report::html::PUBLIC_SITE;
    let enc = |v: &str| v.replace('%', "%25").replace(',', "%2C");
    match second {
        Some(b) => format!("{site}?a={key}&b={b}"),
        None if windows.is_empty() => format!("{site}?p={key}"),
        None => format!("{site}?p={key}&w={}", enc(windows)),
    }
}

fn share_link(st: &State, id: &str, windows: &str, upload: bool) -> Result<Option<String>> {
    let (bytes, name, key, second, second_key) = {
        let reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
        let r = reports.iter().find(|r| r.id == id).ok_or_else(|| {
            anyhow!(
                "{}",
                i18n::t(
                    "report non più disponibile: rifai l'analisi",
                    "report no longer available: run the analysis again"
                )
            )
        })?;
        (
            Arc::clone(&r.bytes),
            r.name.clone(),
            r.key.clone(),
            r.second.clone(),
            r.second_key.clone(),
        )
    };
    let complete = key.is_some() && (second.is_none() || second_key.is_some());
    if !complete && !upload {
        return Ok(None);
    }
    let key = match key {
        Some(k) => k,
        None => upload_payload(st, &bytes, &name)?,
    };
    let second_key = match (&second, second_key) {
        (Some(_), Some(k)) => Some(k),
        (Some((b, nb)), None) => Some(upload_payload(st, b, nb)?),
        (None, _) => None,
    };
    {
        let mut reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(r) = reports.iter_mut().find(|r| r.id == id) {
            r.key = Some(key.clone());
            r.second_key.clone_from(&second_key);
        }
    }
    Ok(Some(public_link(&key, second_key.as_deref(), windows)))
}

fn render(bytes: &[u8], name: &str, key: Option<&str>, windows: &str) -> Result<webui::Rendered> {
    let mut loaded = loader::load_bytes(bytes, name)?;
    if let Some(k) = key {
        loaded.origin = format!("https://spark.lucko.me/{k}");
        loaded.viewer_url = Some(format!("https://spark.lucko.me/{k}"));
    }
    let win: Vec<usize> = windows
        .split(',')
        .filter_map(|x| x.trim().parse().ok())
        .collect();
    let (r, _, heap_report) =
        pipeline::build_with(loaded, None, if win.is_empty() { None } else { Some(&win) })?;
    Ok(webui::render_report(&r, heap_report.as_ref()))
}

fn report_html(st: &State, id: &str, lang: Lang, windows: &str) -> Option<Result<Arc<Vec<u8>>>> {
    let ck = (lang, windows.to_string());
    let (bytes, name, key, second) = {
        let reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
        let r = reports.iter().find(|r| r.id == id)?;
        if let Some(h) = r.cache.get(&ck) {
            return Some(Ok(Arc::clone(h)));
        }
        (
            Arc::clone(&r.bytes),
            r.name.clone(),
            r.key.clone(),
            r.second.clone(),
        )
    };
    let rendered = match &second {
        Some((b, nb)) => render_comparison(&bytes, &name, b, nb),
        None => render(&bytes, &name, key.as_deref(), windows),
    };
    let out = match rendered {
        Ok(o) => o,
        Err(e) => return Some(Err(e)),
    };
    let page = final_page(&out.html);
    let mut reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = reports.iter_mut().find(|r| r.id == id) {
        r.cache.insert(ck, Arc::clone(&page));
    }
    Some(Ok(page))
}

fn api_result(r: Result<String>) -> Response<std::io::Cursor<Vec<u8>>> {
    match r {
        Ok(id) => json_resp(200, serde_json::json!({ "id": id })),
        Err(e) => json_resp(400, serde_json::json!({ "error": format!("{e:#}") })),
    }
}

fn store(st: &State, bytes: Vec<u8>, name: String, key: Option<String>) -> Result<String> {
    let out = render(&bytes, &name, key.as_deref(), "")?;
    let id = next_id(st, bytes.len());
    let mut cache = HashMap::new();
    cache.insert((i18n::lang(), String::new()), final_page(&out.html));
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
            second: None,
            second_key: None,
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
    fn public_links() {
        let site = crate::report::html::PUBLIC_SITE;
        assert_eq!(
            super::public_link("AbCd1234", None, ""),
            format!("{site}?p=AbCd1234")
        );
        assert_eq!(
            super::public_link("AbCd1234", None, "2,5"),
            format!("{site}?p=AbCd1234&w=2%2C5")
        );
        assert_eq!(
            super::public_link("Aaaa1", Some("Bbbb2"), ""),
            format!("{site}?a=Aaaa1&b=Bbbb2")
        );
    }

    #[test]
    fn decode() {
        assert_eq!(super::percent_decode("a%20b%2Fc+d"), "a b/c d");
        assert_eq!(super::percent_decode("100%"), "100%");
    }

    #[test]
    fn gzip_negotiation() {
        let gz = super::gzip(b"<p>ciao</p>");
        let has = |r: &super::Resp| {
            r.headers()
                .iter()
                .any(|h| h.field.equiv("Content-Encoding"))
        };
        assert!(has(&super::page_resp(&gz, true)));
        let plain = super::page_resp(&gz, false);
        assert!(!has(&plain));
        assert!(
            has(&super::html_resp("x".into(), true)) && !has(&super::html_resp("x".into(), false))
        );
        assert_eq!(plain.data_length(), Some(11));
    }
}
