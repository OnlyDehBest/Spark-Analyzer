use crate::archive::{Archive, ArchivedReport, now_secs};
use crate::usage::UsageStats;
use crate::wasm_bundle::WasmBundle;
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
/// Reports kept with their data and rendered pages in memory; older ones are read
/// from disk when opened again.
const MAX_HOT: usize = 30;
const SWEEP_EVERY: std::time::Duration = std::time::Duration::from_secs(3600);
const MAX_STAGED: usize = 12;

struct Staged {
    id: String,
    owner: String,
    bytes: Arc<Vec<u8>>,
    name: String,
    key: Option<String>,
}

struct Stored {
    meta: ArchivedReport,
    payload: Option<Payload>,
    cache: HashMap<(Lang, String), Arc<Vec<u8>>>,
}

#[derive(Clone)]
struct Payload {
    first: Arc<Vec<u8>>,
    second: Option<Arc<Vec<u8>>>,
}

struct State {
    bytebin: String,
    loopback_only: bool,
    reports: Mutex<Vec<Stored>>,
    archive: Option<Archive>,
    retention: std::time::Duration,
    usage: UsageStats,
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
    Summary(Vec<u8>),
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
    pub data_dir: Option<std::path::PathBuf>,
    pub retention_days: u64,
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
    let retention = std::time::Duration::from_secs(o.retention_days.max(1) * 24 * 3600);
    let archive = open_archive(o.data_dir, retention);
    let reports = archive.as_ref().map_or_else(Vec::new, |a| {
        a.prune(now_secs());
        a.load_all(now_secs())
            .into_iter()
            .map(|meta| Stored {
                meta,
                payload: None,
                cache: HashMap::new(),
            })
            .collect()
    });
    let usage = match &archive {
        Some(a) => UsageStats::load(
            a.dir().join("usage.json"),
            reports.iter().map(|r| r.meta.owner.as_str()),
        ),
        None => UsageStats::in_memory(),
    };
    let state = Arc::new(State {
        bytebin: o.bytebin,
        loopback_only,
        reports: Mutex::new(reports),
        archive,
        retention,
        usage,
        staged: Mutex::new(Vec::new()),
        uploads: Mutex::new(HashMap::new()),
        // id unici anche dopo un riavvio: le pagine /r/ restano in cache del browser
        counter: AtomicU64::new(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |d| d.as_secs()),
        ),
    });
    let sweeper = Arc::clone(&state);
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(SWEEP_EVERY);
            drop_expired(&sweeper);
        }
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
        .with_header(header("Vary", "Accept-Encoding"))
        .with_header(header("X-Content-Type-Options", "nosniff"))
        .with_header(header("Referrer-Policy", "strict-origin-when-cross-origin"))
        .with_header(header("X-Frame-Options", "SAMEORIGIN"))
        .with_header(header(
            "Permissions-Policy",
            "camera=(), microphone=(), geolocation=(), payment=(), usb=()",
        ))
        .with_header(header(
            "Content-Security-Policy",
            "frame-ancestors 'self'; base-uri 'self'; object-src 'none'; form-action 'self'",
        ));
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

fn site_asset(file: &str) -> Option<(&'static str, &'static [u8])> {
    Some(match file {
        "favicon.svg" => (
            "image/svg+xml",
            include_bytes!("../web/static/favicon.svg").as_slice(),
        ),
        "favicon.ico" => (
            "image/x-icon",
            include_bytes!("../web/static/favicon.ico").as_slice(),
        ),
        "apple-touch-icon.png" => (
            "image/png",
            include_bytes!("../web/static/apple-touch-icon.png").as_slice(),
        ),
        "og-image.png" => (
            "image/png",
            include_bytes!("../web/static/og-image.png").as_slice(),
        ),
        _ => return None,
    })
}

/// Public address of the site (`https://host/`), or `None` when it runs on this computer.
fn public_base(host: Option<&str>) -> Option<String> {
    let host = host?.trim();
    let name = host.rsplit_once(':').map_or(host, |(h, p)| {
        if p.bytes().all(|b| b.is_ascii_digit()) {
            h
        } else {
            host
        }
    });
    let local = name.is_empty()
        || name == "localhost"
        || name.ends_with(".localhost")
        || name.starts_with("127.")
        || name.starts_with('[')
        || name == "0.0.0.0"
        || name.parse::<std::net::IpAddr>().is_ok();
    let valid = host
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b':');
    (!local && valid).then(|| format!("https://{}/", host.to_ascii_lowercase()))
}

fn plain_resp(body: String, ctype: &str) -> Resp {
    let len = body.len();
    Response::new(
        tiny_http::StatusCode(200),
        vec![
            header("Content-Type", ctype),
            header("Cache-Control", "public, max-age=3600"),
        ],
        std::io::Cursor::new(body.into_bytes()),
        Some(len),
        None,
    )
}

fn robots_txt(base: Option<&str>) -> String {
    match base {
        Some(b) => format!(
            "User-agent: *\nAllow: /\nDisallow: /api/\nDisallow: /r/\n\nSitemap: {b}sitemap.xml\n"
        ),
        None => "User-agent: *\nDisallow: /\n".to_string(),
    }
}

fn sitemap_xml(base: &str) -> String {
    let url = |loc: &str, alt_en: &str, alt_it: &str, prio: &str| {
        format!(
            "<url><loc>{base}{loc}</loc><xhtml:link rel=\"alternate\" hreflang=\"en\" href=\"{base}{alt_en}\"/><xhtml:link rel=\"alternate\" hreflang=\"it\" href=\"{base}{alt_it}\"/><changefreq>weekly</changefreq><priority>{prio}</priority></url>"
        )
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">{}{}{}{}</urlset>\n",
        url("", "", "?lang=it", "1.0"),
        url("?lang=it", "", "?lang=it", "0.9"),
        url("privacy.html", "privacy.html", "privacy-it.html", "0.3"),
        url("privacy-it.html", "privacy.html", "privacy-it.html", "0.3"),
    )
}

fn static_resp(path: &str, gz: bool, base: Option<&str>) -> Option<Resp> {
    let (dir, file) = path.rsplit_once('/')?;
    if dir.ends_with("/fonts") || dir == "/fonts" {
        let body = crate::fonts::font(file)?.to_vec();
        let len = body.len();
        return Some(Response::new(
            tiny_http::StatusCode(200),
            vec![
                header("Content-Type", "font/woff2"),
                header("Cache-Control", "public, max-age=31536000, immutable"),
            ],
            std::io::Cursor::new(body),
            Some(len),
            None,
        ));
    }
    if dir.is_empty()
        && let Some((ctype, body)) = site_asset(file)
    {
        return Some(Response::new(
            tiny_http::StatusCode(200),
            vec![
                header("Content-Type", ctype),
                header("Cache-Control", "public, max-age=86400"),
            ],
            std::io::Cursor::new(body.to_vec()),
            Some(body.len()),
            None,
        ));
    }
    let lang = match file {
        "privacy.html" => Lang::En,
        "privacy-it.html" => Lang::It,
        _ => return None,
    };
    let _guard = i18n::scoped(lang);
    let html = crate::webui::privacy_page_public(base);
    Some(html_resp(html, gz).with_header(header("Cache-Control", "public, max-age=3600")))
}

fn wasm_asset_resp(path: &str, gz: bool) -> Option<Response<std::io::Cursor<&'static [u8]>>> {
    let bundle = WasmBundle::embedded();
    let compressed = if gz { bundle.asset_gzip(path) } else { None };
    let encoded = compressed.is_some();
    let asset = compressed.or_else(|| bundle.asset(path))?;
    let mut headers = vec![
        header("Content-Type", asset.content_type),
        header("Cache-Control", "public, max-age=31536000, immutable"),
        header("Vary", "Accept-Encoding"),
    ];
    if encoded {
        headers.push(header("Content-Encoding", "gzip"));
    }
    Some(Response::new(
        tiny_http::StatusCode(200),
        headers,
        std::io::Cursor::new(asset.body),
        Some(asset.body.len()),
        None,
    ))
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
    let base = public_base(header_value(&req, "Host").as_deref());
    if method == Method::Get && (path == "/robots.txt" || path == "/sitemap.xml") {
        let resp = if path == "/robots.txt" {
            plain_resp(robots_txt(base.as_deref()), "text/plain; charset=utf-8")
        } else if let Some(b) = base.as_deref() {
            plain_resp(sitemap_xml(b), "application/xml; charset=utf-8")
        } else {
            html_resp(not_found(), gz).with_status_code(404)
        };
        let _ = req.respond(resp);
        return;
    }
    if method == Method::Get
        && let Some(resp) = static_resp(&path, gz, base.as_deref())
    {
        let _ = req.respond(resp);
        return;
    }
    if method == Method::Get
        && let Some(asset) = wasm_asset_resp(&path, gz)
    {
        let _ = req.respond(asset);
        return;
    }
    let known_visitor = visitor_id(&req);
    let new_visitor = known_visitor.is_none();
    let owner = known_visitor.unwrap_or_else(|| random_hex(16));
    let owner = owner.as_str();
    let resp = match (method, path.as_str()) {
        (Method::Get, "/") => match open_shared(st, owner, &url) {
            None => html_resp(index_page(st, owner, base.as_deref()), gz)
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
            let body = if UploadKind::from_query(&url) == UploadKind::Summary {
                read_summary(&mut req)
            } else {
                read_upload(&mut req, &name)
            };
            let result = body.and_then(|bytes| store(st, owner, bytes, name, None));
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
            let body = if UploadKind::from_query(&url) == UploadKind::Summary {
                read_summary(&mut req)
            } else {
                read_upload(&mut req, &name)
            };
            let result = body.and_then(|bytes| stage(st, owner, bytes, name, None));
            staged_result(result)
        }
        (Method::Post, "/api/upload/start") => {
            let name = query_param(&url, "name").unwrap_or_else(|| "profile".into());
            match upload_start(st, owner, name, UploadKind::from_query(&url)) {
                Ok(id) => json_resp(200, serde_json::json!({ "upload": id })),
                Err(e) => json_resp(400, serde_json::json!({ "error": format!("{e:#}") })),
            }
        }
        (Method::Post, "/api/upload/chunk") => {
            let id = query_param(&url, "id").unwrap_or_default();
            let num = |k| query_param(&url, k).and_then(|v| v.parse::<u64>().ok());
            let range = num("off").zip(num("end"));
            let sent = num("len").and_then(|n| usize::try_from(n).ok());
            match upload_chunk(st, owner, &id, range, sent, &mut req) {
                Ok(n) => json_resp(200, serde_json::json!({ "received": n })),
                Err(e) if e.downcast_ref::<PieceIncomplete>().is_some() => {
                    json_resp(503, serde_json::json!({ "error": format!("{e:#}") }))
                }
                Err(e) => json_resp(400, serde_json::json!({ "error": format!("{e:#}") })),
            }
        }
        (Method::Post, "/api/upload/finish") => {
            let id = query_param(&url, "id").unwrap_or_default();
            let heap = query_param(&url, "to").as_deref() == Some("heap");
            let size = query_param(&url, "size").and_then(|v| v.parse::<u64>().ok());
            match upload_finish(st, owner, &id, size) {
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
                Some(Ok(h)) => page_resp(&h, gz)
                    .with_header(header("Cache-Control", "private, max-age=3600"))
                    .with_header(header("X-Robots-Tag", "noindex")),
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
    let meta = ArchivedReport {
        id: id.clone(),
        owner: owner.to_string(),
        title: out.title,
        detail: out.detail,
        bad: out.bad,
        name: name_a,
        key: key_a,
        second_name: Some(name_b),
        second_key: key_b,
        created: now_secs(),
    };
    let payload = Payload {
        first: bytes_a,
        second: Some(bytes_b),
    };
    keep(st, meta, payload, final_page(&out.html));
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
    let (meta, payload) = report_data(st, id)?.ok_or_else(|| {
        anyhow!(
            "{}",
            i18n::t(
                "report non più disponibile: rifai l'analisi",
                "report no longer available: run the analysis again"
            )
        )
    })?;
    let bytes = payload.first;
    let name = meta.name.clone();
    let key = meta.key.clone();
    let second = payload.second.zip(meta.second_name.clone());
    let second_key = meta.second_key.clone();
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
        if let Some(r) = reports.iter_mut().find(|r| r.meta.id == id) {
            r.meta.key = Some(key.clone());
            r.meta.second_key.clone_from(&second_key);
            if let Some(a) = &st.archive
                && let Err(e) = a.update(&r.meta)
            {
                eprintln!("report {id}: {e}");
            }
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
    {
        let reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
        let r = reports.iter().find(|r| r.meta.id == id)?;
        if let Some(h) = r.cache.get(&ck) {
            return Some(Ok(Arc::clone(h)));
        }
    }
    let (meta, payload) = match report_data(st, id) {
        Ok(Some(found)) => found,
        Ok(None) => return None,
        Err(e) => return Some(Err(e)),
    };
    let rendered = match (&payload.second, &meta.second_name) {
        (Some(b), Some(nb)) => render_comparison(&payload.first, &meta.name, b, nb),
        _ => render(&payload.first, &meta.name, meta.key.as_deref(), windows),
    };
    let out = match rendered {
        Ok(o) => o,
        Err(e) => return Some(Err(e)),
    };
    let page = final_page(&out.html);
    let mut reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = reports.iter_mut().find(|r| r.meta.id == id) {
        r.cache.insert(ck, Arc::clone(&page));
    }
    Some(Ok(page))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UploadKind {
    File,
    Dump,
    Summary,
}

impl UploadKind {
    fn from_query(url: &str) -> UploadKind {
        if query_param(url, "summary").as_deref() == Some("1") {
            UploadKind::Summary
        } else if query_param(url, "dump").as_deref() == Some("1") {
            UploadKind::Dump
        } else {
            UploadKind::File
        }
    }
}

#[derive(Debug)]
struct PieceIncomplete;

impl std::fmt::Display for PieceIncomplete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(i18n::t(
            "pezzo arrivato incompleto",
            "piece arrived incomplete",
        ))
    }
}

impl std::error::Error for PieceIncomplete {}

fn decode_summary(bytes: Vec<u8>) -> Result<Vec<u8>> {
    let received = bytes.len();
    let plain = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::with_capacity(received.saturating_mul(4));
        flate2::read::MultiGzDecoder::new(bytes.as_slice())
            .take(MAX_UPLOAD as u64 + 1)
            .read_to_end(&mut out)
            .map_err(|e| summary_broken(received, &e))?;
        out
    } else {
        bytes
    };
    if plain.is_empty() || plain.len() > MAX_UPLOAD {
        return Err(summary_broken(received, &"empty or too large"));
    }
    <crate::proto::HeapData as prost::Message>::decode(plain.as_slice())
        .map_err(|e| summary_broken(received, &e))?;
    Ok(plain)
}

fn summary_broken(received: usize, cause: &dyn std::fmt::Display) -> anyhow::Error {
    anyhow!(
        "{}",
        tf!(
            "riepilogo dell'heap dump incompleto o danneggiato ({received} byte ricevuti): {cause}",
            "heap dump summary incomplete or damaged ({received} bytes received): {cause}"
        )
    )
}

fn read_summary(req: &mut Request) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    req.as_reader()
        .take(MAX_UPLOAD as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| {
            anyhow!(
                "{}",
                tf!("upload interrotto: {e}", "upload interrupted: {e}")
            )
        })?;
    if let Some(expected) =
        header_value(req, "Content-Length").and_then(|v| v.parse::<usize>().ok())
        && expected != bytes.len()
    {
        return Err(summary_broken(bytes.len(), &PieceIncomplete));
    }
    decode_summary(bytes)
}

fn upload_start(st: &State, owner: &str, name: String, kind: UploadKind) -> Result<String> {
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
    let data = match kind {
        UploadKind::Dump => UploadData::Dump(Box::default()),
        UploadKind::Summary => UploadData::Summary(Vec::new()),
        UploadKind::File => UploadData::Bytes(Vec::new()),
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
    sent: Option<usize>,
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
    if sent.is_some_and(|n| n != chunk.len()) {
        return Err(PieceIncomplete.into());
    }
    let slot = upload_slot(st, owner, id)?;
    let mut up = slot.up.lock().unwrap_or_else(|e| e.into_inner());
    up.touched = std::time::Instant::now();
    let (off, end) = range.unwrap_or((up.next, up.next + chunk.len() as u64));
    if end < off {
        anyhow::bail!("bad range");
    }
    let result = accept_piece(&mut up, off, end, chunk);
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

fn accept_piece(up: &mut Upload, off: u64, end: u64, chunk: Vec<u8>) -> Result<()> {
    if end <= up.next {
        // piece sent again after a lost reply: already counted
        Ok(())
    } else if off > up.next {
        up.early_bytes += chunk.len();
        if up.early_bytes > MAX_EARLY {
            Err(anyhow!("too many pieces out of order"))
        } else {
            // a piece sent again replaces the copy already waiting
            if let Some((_, old)) = up.early.insert(off, (end, chunk)) {
                up.early_bytes -= old.len();
            }
            Ok(())
        }
    } else if off < up.next {
        Err(anyhow!("overlapping piece"))
    } else {
        let mut r = append(up, &chunk);
        up.next = end;
        while r.is_ok() {
            // copies of pieces that were also sent again and already added
            while let Some((&k, _)) = up.early.first_key_value() {
                if k >= up.next {
                    break;
                }
                if let Some((_, old)) = up.early.remove(&k) {
                    up.early_bytes -= old.len();
                }
            }
            let next = up.next;
            let Some((end, piece)) = up.early.remove(&next) else {
                break;
            };
            up.early_bytes -= piece.len();
            r = append(up, &piece);
            up.next = end;
        }
        r
    }
}

fn append(up: &mut Upload, chunk: &[u8]) -> Result<()> {
    match &mut up.data {
        UploadData::Dump(stream) => stream.feed(chunk),
        UploadData::Summary(buf) => {
            buf.extend_from_slice(chunk);
            if buf.len() > MAX_UPLOAD {
                Err(summary_broken(buf.len(), &"too large"))
            } else {
                Ok(())
            }
        }
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

fn upload_finish(
    st: &State,
    owner: &str,
    id: &str,
    size: Option<u64>,
) -> Result<(Vec<u8>, String)> {
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
    if !up.early.is_empty() || size.is_some_and(|n| n != up.next) {
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
        UploadData::Summary(b) => decode_summary(b)?,
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
    let meta = ArchivedReport {
        id: id.clone(),
        owner: owner.to_string(),
        title: out.title,
        detail: out.detail,
        bad: out.bad,
        name,
        key,
        second_name: None,
        second_key: None,
        created: now_secs(),
    };
    let payload = Payload {
        first: Arc::new(bytes),
        second: None,
    };
    keep(st, meta, payload, final_page(&out.html));
    Ok(id)
}

fn open_archive(
    dir: Option<std::path::PathBuf>,
    retention: std::time::Duration,
) -> Option<Archive> {
    let candidates = match dir {
        Some(d) => vec![d],
        None => Archive::candidate_dirs(),
    };
    let mut last_error = None;
    for dir in candidates {
        match Archive::open(dir.clone(), retention) {
            Ok(a) => {
                eprintln!(
                    "{}",
                    tf!(
                        "Report salvati in {} (eliminati dopo {} giorni)",
                        "Reports saved in {} (deleted after {} days)",
                        dir.display(),
                        retention.as_secs() / 86_400
                    )
                );
                return Some(a);
            }
            Err(e) => last_error = Some(format!("{}: {e}", dir.display())),
        }
    }
    eprintln!(
        "{}",
        tf!(
            "Impossibile salvare i report ({}): restano solo in memoria.",
            "Cannot save reports ({}): they stay in memory only.",
            last_error.unwrap_or_default()
        )
    );
    None
}

/// Adds a new report: it is written to disk and the oldest reports in memory give
/// back their data and pages (they stay listed and are read again from disk).
fn keep(st: &State, meta: ArchivedReport, payload: Payload, page: Arc<Vec<u8>>) {
    st.usage.record(&meta.owner);
    let saved = match &st.archive {
        Some(a) => match a.save(
            &meta,
            &payload.first,
            payload.second.as_deref().map(Vec::as_slice),
        ) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("report {}: {e}", meta.id);
                false
            }
        },
        None => false,
    };
    let mut cache = HashMap::new();
    cache.insert((i18n::lang(), String::new()), page);
    let mut reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
    reports.insert(
        0,
        Stored {
            meta,
            payload: Some(payload),
            cache,
        },
    );
    if st.archive.is_none() || !saved {
        let mut hot = 0;
        reports.retain(|r| {
            let keep = r.payload.is_some() && hot < MAX_HOT;
            if keep {
                hot += 1;
            }
            keep
        });
    }
    for r in reports.iter_mut().skip(MAX_HOT) {
        r.payload = None;
        r.cache.clear();
    }
}

fn report_data(st: &State, id: &str) -> Result<Option<(ArchivedReport, Payload)>> {
    let meta = {
        let reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
        let Some(r) = reports.iter().find(|r| r.meta.id == id) else {
            return Ok(None);
        };
        if let Some(p) = &r.payload {
            return Ok(Some((r.meta.clone(), p.clone())));
        }
        r.meta.clone()
    };
    let Some(archive) = &st.archive else {
        return Ok(None);
    };
    let (first, second) = archive.payload(id).map_err(|e| {
        anyhow!(
            "{}",
            tf!(
                "report non leggibile dal disco: {e}",
                "report cannot be read from disk: {e}"
            )
        )
    })?;
    Ok(Some((
        meta,
        Payload {
            first: Arc::new(first),
            second: second.map(Arc::new),
        },
    )))
}

fn drop_expired(st: &State) {
    let now = now_secs();
    if let Some(a) = &st.archive {
        a.prune(now);
    }
    let limit = st.retention.as_secs();
    st.reports
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|r| now.saturating_sub(r.meta.created) <= limit);
}

fn index_page(st: &State, owner: &str, base: Option<&str>) -> String {
    let reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
    let recent: Vec<webui::Recent> = reports
        .iter()
        .filter(|r| r.meta.owner == owner)
        .map(|r| webui::Recent {
            href: if i18n::is_en() {
                format!("/r/{}", r.meta.id)
            } else {
                format!("/r/{}?lang=it", r.meta.id)
            },
            title: r.meta.title.clone(),
            detail: r.meta.detail.clone(),
            bad: r.meta.bad,
        })
        .collect();
    drop(reports);
    webui::local_index_public(
        &recent,
        WasmBundle::embedded().query(),
        Some(st.usage.snapshot()),
        base,
    )
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
    fn sample_summary() -> Vec<u8> {
        let heap = crate::proto::HeapData {
            entries: vec![crate::proto::HeapEntry {
                r#type: "java.lang.String".into(),
                instances: 3,
                size: 72,
                ..Default::default()
            }],
            ..Default::default()
        };
        prost::Message::encode_to_vec(&heap)
    }

    #[test]
    fn summary_from_the_browser_is_accepted_plain_or_gzipped() {
        let plain = sample_summary();
        assert_eq!(super::decode_summary(plain.clone()).unwrap(), plain);
        assert_eq!(super::decode_summary(super::gzip(&plain)).unwrap(), plain);
    }

    #[test]
    fn truncated_summary_is_rejected_with_a_clear_error() {
        let plain = sample_summary();
        let err = super::decode_summary(plain[..plain.len() - 3].to_vec()).unwrap_err();
        assert!(
            format!("{err:#}").contains("incomplete") || format!("{err:#}").contains("incompleto")
        );
        let gz = super::gzip(&plain);
        assert!(super::decode_summary(gz[..gz.len() - 8].to_vec()).is_err());
        assert!(super::decode_summary(Vec::new()).is_err());
    }

    fn summary_upload() -> super::Upload {
        super::Upload {
            name: "dump.hprof".into(),
            data: super::UploadData::Summary(Vec::new()),
            next: 0,
            early: std::collections::BTreeMap::new(),
            early_bytes: 0,
            touched: std::time::Instant::now(),
        }
    }

    #[test]
    fn pieces_in_any_order_and_repeated_rebuild_the_original() {
        let original: Vec<u8> = (0..10_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let ranges: Vec<(usize, usize)> = (0..original.len())
            .step_by(700)
            .map(|a| (a, (a + 700).min(original.len())))
            .collect();
        let order = [
            3usize, 1, 0, 1, 5, 2, 14, 4, 3, 6, 9, 8, 7, 13, 12, 11, 10, 14, 0,
        ];
        let mut up = summary_upload();
        for &i in &order {
            let (a, b) = ranges[i];
            super::accept_piece(&mut up, a as u64, b as u64, original[a..b].to_vec()).unwrap();
        }
        assert_eq!(up.next, original.len() as u64);
        assert!(up.early.is_empty());
        assert_eq!(up.early_bytes, 0);
        let super::UploadData::Summary(got) = up.data else {
            panic!("summary expected");
        };
        assert_eq!(got, original);
    }

    #[test]
    fn overlapping_piece_is_refused() {
        let mut up = summary_upload();
        super::accept_piece(&mut up, 0, 10, vec![1; 10]).unwrap();
        assert!(super::accept_piece(&mut up, 5, 15, vec![2; 10]).is_err());
    }

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
