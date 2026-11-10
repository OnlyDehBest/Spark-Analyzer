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
    owner: String,
    bytes: Arc<Vec<u8>>,
    name: String,
    key: Option<String>,
}

struct Stored {
    id: String,
    owner: String,
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
    loopback_only: bool,
    reports: Mutex<Vec<Stored>>,
    staged: Mutex<Vec<Staged>>,
    uploads: Mutex<HashMap<String, Arc<UploadSlot>>>,
    counter: AtomicU64,
}

/// A file sent in pieces (so it passes reverse proxies with a request size limit).
/// Heap dumps are parsed while the pieces arrive and never kept whole in memory.
/// Pieces may travel in parallel: each one names its byte range in the original file
/// and the ones that arrive early wait in `early` until the gap before them is filled.
struct UploadSlot {
    owner: String,
    up: Mutex<Upload>,
}

struct Upload {
    name: String,
    data: UploadData,
    /// End of the contiguous prefix of the original file received so far.
    next: u64,
    early: std::collections::BTreeMap<u64, (u64, Vec<u8>)>,
    early_bytes: usize,
    touched: std::time::Instant,
}

enum UploadData {
    Bytes(Vec<u8>),
    Dump(Box<crate::hprof::HprofStream>),
}

const MAX_CHUNK: usize = 64 * 1024 * 1024;
/// Pieces held while waiting for an earlier one (a few in flight per upload).
const MAX_EARLY: usize = 256 * 1024 * 1024;
const MAX_UPLOADS_PER_VISITOR: usize = 4;
const UPLOAD_IDLE: std::time::Duration = std::time::Duration::from_secs(15 * 60);

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
    eprintln!("Spark Resource Analyzer è in esecuzione: apri {url}  (running: open {url})");
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
    let loopback_only = is_loopback_host(&o.host);
    let state = Arc::new(State {
        bytebin: o.bytebin,
        loopback_only,
        reports: Mutex::new(Vec::new()),
        staged: Mutex::new(Vec::new()),
        uploads: Mutex::new(HashMap::new()),
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

fn is_loopback_host(host: &str) -> bool {
    let h = host.trim_start_matches('[');
    let h = match h.split_once(']') {
        Some((inner, _)) => inner,
        None => h.rsplit_once(':').map_or(h, |(name, port)| {
            if port.chars().all(|c| c.is_ascii_digit()) && !name.contains(':') {
                name
            } else {
                h
            }
        }),
    };
    matches!(h, "127.0.0.1" | "localhost" | "::1")
}

fn authority_of(origin: &str) -> Option<&str> {
    let rest = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))?;
    Some(rest.split('/').next().unwrap_or(rest))
}

fn request_allowed(
    is_post: bool,
    host: Option<&str>,
    origin: Option<&str>,
    fetch_site: Option<&str>,
    loopback_only: bool,
) -> bool {
    if loopback_only && !host.is_some_and(is_loopback_host) {
        return false;
    }
    if !is_post {
        return true;
    }
    if matches!(fetch_site, Some("cross-site" | "same-site")) {
        return false;
    }
    match origin {
        None | Some("null") => fetch_site.is_none_or(|f| f == "same-origin" || f == "none"),
        Some(o) => authority_of(o).is_some_and(|a| host.is_some_and(|h| h.eq_ignore_ascii_case(a))),
    }
}

fn header_value(req: &Request, name: &'static str) -> Option<String> {
    req.headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str().to_string())
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
    let allowed = request_allowed(
        method == Method::Post,
        header_value(&req, "Host").as_deref(),
        header_value(&req, "Origin").as_deref(),
        header_value(&req, "Sec-Fetch-Site").as_deref(),
        st.loopback_only,
    );
    if !allowed {
        let _ = req.respond(json_resp(
            403,
            serde_json::json!({ "error": i18n::t(
                    "richiesta bloccata: arriva da un altro sito",
                    "request blocked: it comes from another site"
                ) }),
        ));
        return;
    }
    let known_visitor = visitor_id(&req);
    let new_visitor = known_visitor.is_none();
    let owner = known_visitor.unwrap_or_else(|| random_hex(16));
    let owner = owner.as_str();
    let resp = match (method, path.as_str()) {
        (Method::Get, "/") => match open_shared(st, owner, &url) {
            None => html_resp(index_page(st, owner), gz)
                .with_header(header("Cache-Control", "no-cache")),
            Some(Ok(id)) => {
                let w: String = query_param(&url, "w")
                    .unwrap_or_default()
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == ',')
                    .collect();
                let mut loc = format!("/r/{id}?w={w}");
                if !i18n::is_en() {
                    loc.push_str("&lang=it");
                }
                Response::from_data(Vec::new())
                    .with_status_code(303)
                    .with_header(header("Location", &loc))
            }
            Some(Err(e)) => html_resp(
                format!("<pre>{}</pre>", crate::report::html::esc(&format!("{e:#}"))),
                gz,
            )
            .with_status_code(400),
        },
        (Method::Get, "/favicon.ico") => Response::from_data(Vec::new()).with_status_code(204),
        (Method::Post, "/api/link") => {
            let mut body = String::new();
            let _ = req.as_reader().take(8192).read_to_string(&mut body);
            let link = body.trim().to_string();
            let result = loader::fetch_link(&link, &st.bytebin)
                .and_then(|(bytes, key)| store(st, owner, bytes, key.clone(), Some(key)));
            api_result(result)
        }
        (Method::Post, "/api/file") => {
            let name = query_param(&url, "name").unwrap_or_else(|| "profile".into());
            let result =
                read_upload(&mut req, &name).and_then(|bytes| store(st, owner, bytes, name, None));
            api_result(result)
        }
        (Method::Post, "/api/heap/link") => {
            let mut body = String::new();
            let _ = req.as_reader().take(8192).read_to_string(&mut body);
            let result = loader::fetch_link(body.trim(), &st.bytebin)
                .and_then(|(bytes, key)| stage(st, owner, bytes, key.clone(), Some(key)));
            staged_result(result)
        }
        (Method::Post, "/api/heap/file") => {
            let name = query_param(&url, "name").unwrap_or_else(|| "heap.sparkheap".into());
            let result =
                read_upload(&mut req, &name).and_then(|bytes| stage(st, owner, bytes, name, None));
            staged_result(result)
        }
        (Method::Post, "/api/upload/start") => {
            let name = query_param(&url, "name").unwrap_or_else(|| "profile".into());
            let dump = query_param(&url, "dump").as_deref() == Some("1");
            match upload_start(st, owner, name, dump) {
                Ok(id) => json_resp(200, serde_json::json!({ "upload": id })),
                Err(e) => json_resp(400, serde_json::json!({ "error": format!("{e:#}") })),
            }
        }
        (Method::Post, "/api/upload/chunk") => {
            let id = query_param(&url, "id").unwrap_or_default();
            let num = |k| query_param(&url, k).and_then(|v| v.parse::<u64>().ok());
            let range = num("off").zip(num("end"));
            match upload_chunk(st, owner, &id, range, &mut req) {
                Ok(n) => json_resp(200, serde_json::json!({ "received": n })),
                Err(e) => json_resp(400, serde_json::json!({ "error": format!("{e:#}") })),
            }
        }
        (Method::Post, "/api/upload/finish") => {
            let id = query_param(&url, "id").unwrap_or_default();
            let heap = query_param(&url, "to").as_deref() == Some("heap");
            match upload_finish(st, owner, &id) {
                Ok((bytes, name)) if heap => staged_result(stage(st, owner, bytes, name, None)),
                Ok((bytes, name)) => api_result(store(st, owner, bytes, name, None)),
                Err(e) => json_resp(400, serde_json::json!({ "error": format!("{e:#}") })),
            }
        }
        (Method::Post, "/api/heap/compare") => {
            let a = query_param(&url, "a").unwrap_or_default();
            let b = query_param(&url, "b").unwrap_or_default();
            api_result(store_comparison(st, owner, &a, &b))
        }
        (m @ (Method::Get | Method::Post), p) if p.starts_with("/api/share/") => {
            let id = p["/api/share/".len()..].to_string();
            let windows = query_param(&url, "w").unwrap_or_default();
            let upload = m == Method::Post;
            let site = share_site(header_value(&req, "Host").as_deref());
            match share_link(st, &id, &windows, upload, &site) {
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
    let resp = if new_visitor {
        resp.with_header(header(
            "Set-Cookie",
            &format!("{VISITOR_COOKIE}={owner}; Path=/; Max-Age=31536000; HttpOnly; SameSite=Lax"),
        ))
    } else {
        resp
    };
    let _ = req.respond(resp);
}

fn read_upload(req: &mut Request, name: &str) -> Result<Vec<u8>> {
    let mut head = vec![0u8; 64 * 1024];
    let mut got = 0;
    {
        let r = req.as_reader();
        while got < head.len() {
            match r.read(&mut head[got..]) {
                Ok(0) => break,
                Ok(n) => got += n,
                Err(e) => {
                    return Err(anyhow!(
                        "{}",
                        tf!("upload interrotto: {e}", "upload interrupted: {e}")
                    ));
                }
            }
        }
    }
    head.truncate(got);
    let gz = head.len() > 2 && head[0] == 0x1f && head[1] == 0x8b;
    if crate::hprof::is_hprof(&head) || (gz && crate::hprof::is_hprof_name(name)) {
        let mut all = std::io::Cursor::new(head).chain(req.as_reader());
        let h = crate::hprof::parse_reader(&mut all, |_| {})?;
        return Ok(prost::Message::encode_to_vec(&h));
    }
    let mut bytes = head;
    let read = req
        .as_reader()
        .take(MAX_UPLOAD as u64 + 1 - bytes.len() as u64)
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

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    if getrandom::fill(&mut buf).is_err() {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        for (i, b) in buf.iter_mut().enumerate() {
            let h = std::hash::BuildHasher::hash_one(
                &std::collections::hash_map::RandomState::new(),
                (seed, i),
            );
            *b = h as u8;
        }
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn next_id(st: &State, _len: usize) -> String {
    let _ = st.counter.fetch_add(1, Ordering::Relaxed);
    random_hex(12)
}

const VISITOR_COOKIE: &str = "sa_vid";

fn visitor_id(req: &Request) -> Option<String> {
    let cookies = header_value(req, "Cookie")?;
    cookies.split(';').find_map(|c| {
        let (k, v) = c.trim().split_once('=')?;
        (k == VISITOR_COOKIE && v.len() == 32 && v.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| v.to_string())
    })
}

struct StageInfo {
    id: String,
    name: String,
    detail: String,
    size: usize,
}

fn stage(
    st: &State,
    owner: &str,
    bytes: Vec<u8>,
    name: String,
    key: Option<String>,
) -> Result<StageInfo> {
    let info = crate::heapdiff::describe_bytes(&bytes, &name)?;
    let id = next_id(st, bytes.len());
    let size = bytes.len();
    let mut staged = st.staged.lock().unwrap_or_else(|e| e.into_inner());
    staged.insert(
        0,
        Staged {
            id: id.clone(),
            owner: owner.to_string(),
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

fn staged_bytes(
    st: &State,
    owner: &str,
    id: &str,
) -> Result<(Arc<Vec<u8>>, String, Option<String>)> {
    let staged = st.staged.lock().unwrap_or_else(|e| e.into_inner());
    staged
        .iter()
        .find(|s| s.id == id && s.owner == owner)
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

fn store_comparison(st: &State, owner: &str, a: &str, b: &str) -> Result<String> {
    let (bytes_a, name_a, key_a) = staged_bytes(st, owner, a)?;
    let (bytes_b, name_b, key_b) = staged_bytes(st, owner, b)?;
    let out = render_comparison(&bytes_a, &name_a, &bytes_b, &name_b)?;
    let id = next_id(st, bytes_a.len() + bytes_b.len());
    let mut cache = HashMap::new();
    cache.insert((i18n::lang(), String::new()), final_page(&out.html));
    let mut reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
    reports.insert(
        0,
        Stored {
            id: id.clone(),
            owner: owner.to_string(),
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

/// Links point back to the public host serving this page (e.g. sparkanalyzer.me behind nginx);
/// loopback or LAN `serve` instances fall back to the static site, which others can reach.
fn share_site(host: Option<&str>) -> String {
    match host {
        Some(h)
            if !is_loopback_host(h)
                && h.contains('.')
                && h.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
                && !h.chars().all(|c| c.is_ascii_digit() || c == '.') =>
        {
            format!("https://{}/", h.to_ascii_lowercase())
        }
        _ => crate::report::html::PUBLIC_SITE.to_string(),
    }
}

/// Opens `/?p=KEY` or `/?a=KEY&b=KEY` share links (the same format the static site reads).
fn open_shared(st: &State, owner: &str, url: &str) -> Option<Result<String>> {
    let fetch = |k: &str| loader::fetch_link(k, &st.bytebin);
    if let Some(p) = query_param(url, "p") {
        return Some(
            fetch(&p).and_then(|(bytes, key)| store(st, owner, bytes, key.clone(), Some(key))),
        );
    }
    let (a, b) = (query_param(url, "a")?, query_param(url, "b")?);
    let staged =
        |k: &str| fetch(k).and_then(|(bytes, key)| stage(st, owner, bytes, key.clone(), Some(key)));
    Some(
        staged(&a)
            .and_then(|sa| staged(&b).and_then(|sb| store_comparison(st, owner, &sa.id, &sb.id))),
    )
}

fn public_link(site: &str, key: &str, second: Option<&str>, windows: &str) -> String {
    let enc = |v: &str| v.replace('%', "%25").replace(',', "%2C");
    match second {
        Some(b) => format!("{site}?a={key}&b={b}"),
        None if windows.is_empty() => format!("{site}?p={key}"),
        None => format!("{site}?p={key}&w={}", enc(windows)),
    }
}

fn share_link(
    st: &State,
    id: &str,
    windows: &str,
    upload: bool,
    site: &str,
) -> Result<Option<String>> {
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
    Ok(Some(public_link(
        site,
        &key,
        second_key.as_deref(),
        windows,
    )))
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

fn upload_start(st: &State, owner: &str, name: String, dump: bool) -> Result<String> {
    let mut ups = st.uploads.lock().unwrap_or_else(|e| e.into_inner());
    ups.retain(|_, u| {
        u.up.try_lock()
            .map_or(true, |u| u.touched.elapsed() < UPLOAD_IDLE)
    });
    if ups.values().filter(|u| u.owner == owner).count() >= MAX_UPLOADS_PER_VISITOR {
        anyhow::bail!(
            "{}",
            i18n::t(
                "troppi caricamenti in corso: attendi che finiscano",
                "too many uploads in progress: wait for them to finish"
            )
        );
    }
    let id = random_hex(12);
    let data = if dump {
        UploadData::Dump(Box::new(crate::hprof::HprofStream::new()))
    } else {
        UploadData::Bytes(Vec::new())
    };
    ups.insert(
        id.clone(),
        Arc::new(UploadSlot {
            owner: owner.to_string(),
            up: Mutex::new(Upload {
                name,
                data,
                next: 0,
                early: std::collections::BTreeMap::new(),
                early_bytes: 0,
                touched: std::time::Instant::now(),
            }),
        }),
    );
    Ok(id)
}

fn upload_gone() -> anyhow::Error {
    anyhow!(
        "{}",
        i18n::t(
            "caricamento scaduto o sconosciuto: riprova",
            "upload expired or unknown: try again"
        )
    )
}

fn upload_slot(st: &State, owner: &str, id: &str) -> Result<Arc<UploadSlot>> {
    let ups = st.uploads.lock().unwrap_or_else(|e| e.into_inner());
    ups.get(id)
        .filter(|u| u.owner == owner)
        .cloned()
        .ok_or_else(upload_gone)
}

/// Adds one piece. `range` is its position in the original file (absent for clients
/// that send the pieces one after the other).
fn upload_chunk(
    st: &State,
    owner: &str,
    id: &str,
    range: Option<(u64, u64)>,
    req: &mut Request,
) -> Result<u64> {
    let mut chunk = Vec::new();
    req.as_reader()
        .take(MAX_CHUNK as u64 + 1)
        .read_to_end(&mut chunk)
        .map_err(|e| {
            anyhow!(
                "{}",
                tf!("upload interrotto: {e}", "upload interrupted: {e}")
            )
        })?;
    if chunk.len() > MAX_CHUNK {
        anyhow::bail!("chunk too large");
    }
    let slot = upload_slot(st, owner, id)?;
    let mut up = slot.up.lock().unwrap_or_else(|e| e.into_inner());
    up.touched = std::time::Instant::now();
    let (off, end) = range.unwrap_or((up.next, up.next + chunk.len() as u64));
    if end < off {
        anyhow::bail!("bad range");
    }
    let result = if end <= up.next {
        // piece sent again after a lost reply: already counted
        Ok(())
    } else if off > up.next {
        up.early_bytes += chunk.len();
        if up.early_bytes > MAX_EARLY {
            Err(anyhow!("too many pieces out of order"))
        } else {
            up.early.insert(off, (end, chunk));
            Ok(())
        }
    } else if off < up.next {
        Err(anyhow!("overlapping piece"))
    } else {
        let mut r = append(&mut up, &chunk);
        up.next = end;
        while r.is_ok() {
            let next = up.next;
            let Some((end, piece)) = up.early.remove(&next) else {
                break;
            };
            up.early_bytes -= piece.len();
            r = append(&mut up, &piece);
            up.next = end;
        }
        r
    };
    let received = up.next;
    drop(up);
    if let Err(e) = result {
        st.uploads
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        return Err(e);
    }
    Ok(received)
}

fn append(up: &mut Upload, chunk: &[u8]) -> Result<()> {
    match &mut up.data {
        UploadData::Dump(stream) => stream.feed(chunk),
        UploadData::Bytes(buf) => {
            buf.extend_from_slice(chunk);
            let gz = buf.len() > 2 && buf[0] == 0x1f && buf[1] == 0x8b;
            if buf.len() >= 12
                && (crate::hprof::is_hprof(buf) || (gz && crate::hprof::is_hprof_name(&up.name)))
            {
                let mut stream = crate::hprof::HprofStream::new();
                let fed = stream.feed(buf);
                up.data = UploadData::Dump(Box::new(stream));
                fed
            } else if buf.len() > MAX_UPLOAD {
                Err(anyhow!(
                    "{}",
                    i18n::t(
                        "file troppo grande (massimo 512 MB)",
                        "file too large (max 512 MB)"
                    )
                ))
            } else {
                Ok(())
            }
        }
    }
}

fn upload_finish(st: &State, owner: &str, id: &str) -> Result<(Vec<u8>, String)> {
    let slot = {
        let mut ups = st.uploads.lock().unwrap_or_else(|e| e.into_inner());
        match ups.get(id) {
            Some(u) if u.owner == owner => ups.remove(id),
            _ => None,
        }
    }
    .ok_or_else(upload_gone)?;
    let up = std::mem::replace(
        &mut *slot.up.lock().unwrap_or_else(|e| e.into_inner()),
        Upload {
            name: String::new(),
            data: UploadData::Bytes(Vec::new()),
            next: 0,
            early: std::collections::BTreeMap::new(),
            early_bytes: 0,
            touched: std::time::Instant::now(),
        },
    );
    if !up.early.is_empty() {
        anyhow::bail!(
            "{}",
            i18n::t(
                "caricamento incompleto: mancano dei pezzi, riprova",
                "incomplete upload: some pieces are missing, try again"
            )
        );
    }
    let bytes = match up.data {
        UploadData::Bytes(b) => {
            if b.is_empty() {
                anyhow::bail!("{}", i18n::t("il file è vuoto", "the file is empty"));
            }
            b
        }
        UploadData::Dump(stream) => prost::Message::encode_to_vec(&stream.finish()?),
    };
    Ok((bytes, up.name))
}

fn api_result(r: Result<String>) -> Response<std::io::Cursor<Vec<u8>>> {
    match r {
        Ok(id) => json_resp(200, serde_json::json!({ "id": id })),
        Err(e) => json_resp(400, serde_json::json!({ "error": format!("{e:#}") })),
    }
}

fn store(
    st: &State,
    owner: &str,
    bytes: Vec<u8>,
    name: String,
    key: Option<String>,
) -> Result<String> {
    let out = render(&bytes, &name, key.as_deref(), "")?;
    let id = next_id(st, bytes.len());
    let mut cache = HashMap::new();
    cache.insert((i18n::lang(), String::new()), final_page(&out.html));
    let mut reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
    reports.insert(
        0,
        Stored {
            id: id.clone(),
            owner: owner.to_string(),
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

fn index_page(st: &State, owner: &str) -> String {
    let reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
    let recent: Vec<webui::Recent> = reports
        .iter()
        .filter(|r| r.owner == owner)
        .map(|r| webui::Recent {
            href: if i18n::is_en() {
                format!("/r/{}", r.id)
            } else {
                format!("/r/{}?lang=it", r.id)
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
            if i18n::is_en() { "/" } else { "/?lang=it" },
            icon("plus"),
            i18n::t("Nuova analisi", "New analysis")
        ),
        1,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn loopback_hosts() {
        for h in [
            "127.0.0.1:8085",
            "localhost:8085",
            "[::1]:8085",
            "localhost",
            "127.0.0.1",
        ] {
            assert!(super::is_loopback_host(h), "{h}");
        }
        for h in [
            "evil.example:8085",
            "192.168.1.5:8085",
            "127.0.0.1.evil.com",
            "0.0.0.0",
        ] {
            assert!(!super::is_loopback_host(h), "{h}");
        }
    }

    #[test]
    fn report_ids_and_visitor_ids_are_random() {
        let a = super::random_hex(12);
        let b = super::random_hex(12);
        assert_eq!(a.len(), 24);
        assert_ne!(a, b);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(super::random_hex(16).len(), 32);
    }

    #[test]
    fn blocks_cross_site_and_rebinding() {
        let ok = super::request_allowed;
        let host = Some("127.0.0.1:8085");
        assert!(ok(false, host, None, None, true));
        assert!(ok(
            true,
            host,
            Some("http://127.0.0.1:8085"),
            Some("same-origin"),
            true
        ));
        assert!(ok(true, host, None, None, true));
        assert!(!ok(false, Some("evil.example:8085"), None, None, true));
        assert!(!ok(
            true,
            host,
            Some("https://evil.example"),
            Some("cross-site"),
            true
        ));
        assert!(!ok(true, host, Some("https://evil.example"), None, true));
        assert!(!ok(true, host, Some("http://localhost:8085"), None, true));
        assert!(!ok(true, host, None, Some("cross-site"), true));
        assert!(!ok(true, host, Some("null"), Some("cross-site"), true));
        assert!(ok(
            true,
            Some("192.168.1.5:8085"),
            Some("http://192.168.1.5:8085"),
            Some("same-origin"),
            false
        ));
        assert!(!ok(
            true,
            Some("192.168.1.5:8085"),
            Some("https://evil.example"),
            Some("cross-site"),
            false
        ));
    }

    #[test]
    fn public_links() {
        let site = crate::report::html::PUBLIC_SITE;
        assert_eq!(
            super::public_link(site, "AbCd1234", None, ""),
            format!("{site}?p=AbCd1234")
        );
        assert_eq!(
            super::public_link(site, "AbCd1234", None, "2,5"),
            format!("{site}?p=AbCd1234&w=2%2C5")
        );
        assert_eq!(
            super::public_link(site, "Aaaa1", Some("Bbbb2"), ""),
            format!("{site}?a=Aaaa1&b=Bbbb2")
        );
    }

    #[test]
    fn share_site_follows_public_host() {
        let fallback = crate::report::html::PUBLIC_SITE;
        assert_eq!(
            super::share_site(Some("SparkAnalyzer.me")),
            "https://sparkanalyzer.me/"
        );
        for h in [
            None,
            Some(""),
            Some("127.0.0.1:8085"),
            Some("localhost:8085"),
            Some("192.168.1.5:8085"),
            Some("10.0.0.2"),
            Some("evil.me\r\nX: y"),
            Some("intranet"),
        ] {
            assert_eq!(super::share_site(h), fallback, "{h:?}");
        }
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
