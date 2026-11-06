use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::loader;
use crate::pipeline;
use crate::report::html::{CSS, esc, icon};
use crate::report::{fmt_duration, fmt_num};

const MAX_UPLOAD: usize = 512 * 1024 * 1024;
const MAX_STORED: usize = 30;

struct Stored {
    id: String,
    title: String,
    detail: String,
    bad: bool,
    html: String,
    markdown: String,
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
    eprintln!("sparkscope è in esecuzione: apri {url}");
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
    let path = url.split('?').next().unwrap_or("/").to_string();
    let method = req.method().clone();
    let resp = match (method, path.as_str()) {
        (Method::Get, "/") => html_resp(index_page(st)),
        (Method::Get, "/favicon.ico") => Response::from_data(Vec::new()).with_status_code(204),
        (Method::Post, "/api/link") => {
            let mut body = String::new();
            let _ = req.as_reader().take(8192).read_to_string(&mut body);
            let link = body.trim().to_string();
            let result = loader::load_link(&link, &st.bytebin).and_then(|l| store(st, l));
            api_result(result)
        }
        (Method::Post, "/api/file") => {
            let name = query_param(&url, "name").unwrap_or_else(|| "profilo".into());
            let mut bytes = Vec::new();
            let read = req
                .as_reader()
                .take(MAX_UPLOAD as u64 + 1)
                .read_to_end(&mut bytes);
            let result = match read {
                Err(e) => Err(anyhow!("upload interrotto: {e}")),
                Ok(_) if bytes.len() > MAX_UPLOAD => {
                    Err(anyhow!("file troppo grande (massimo 512 MB)"))
                }
                Ok(_) if bytes.is_empty() => Err(anyhow!("il file è vuoto")),
                Ok(_) => loader::load_bytes(&bytes, &name).and_then(|l| store(st, l)),
            };
            api_result(result)
        }
        (Method::Get, p) if p.starts_with("/r/") => {
            let rest = &p[3..];
            let (id, kind) = match rest.split_once('/') {
                Some((i, k)) => (i, k),
                None => (rest, ""),
            };
            let reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
            match reports.iter().find(|r| r.id == id) {
                Some(r) => match kind {
                    "" => html_resp(with_actions(&r.html, &r.id)),
                    "html" => Response::from_data(r.html.clone().into_bytes())
                        .with_header(header("Content-Type", "text/html; charset=utf-8"))
                        .with_header(header(
                            "Content-Disposition",
                            &format!("attachment; filename=\"sparkscope-{}.html\"", r.id),
                        )),
                    "md" => Response::from_data(r.markdown.clone().into_bytes())
                        .with_header(header("Content-Type", "text/markdown; charset=utf-8"))
                        .with_header(header(
                            "Content-Disposition",
                            &format!("attachment; filename=\"sparkscope-{}.md\"", r.id),
                        )),
                    _ => html_resp(not_found()).with_status_code(404),
                },
                None => html_resp(not_found()).with_status_code(404),
            }
        }
        _ => html_resp(not_found()).with_status_code(404),
    };
    let _ = req.respond(resp);
}

fn api_result(r: Result<String>) -> Response<std::io::Cursor<Vec<u8>>> {
    match r {
        Ok(id) => json_resp(200, serde_json::json!({ "id": id })),
        Err(e) => json_resp(400, serde_json::json!({ "error": format!("{e:#}") })),
    }
}

fn store(st: &State, loaded: loader::Loaded) -> Result<String> {
    let (r, _, heap_report) = pipeline::build(loaded, None)?;
    let mut html = crate::report::html::render(&r);
    let mut markdown = crate::report::markdown::render(&r);
    if let Some(h) = &heap_report {
        let text = crate::heap::render_text(h);
        let section = format!(
            r#"<section id="heap" class="panel"><h2><span class="hi">{}</span>Heap summary</h2><pre class="heap">{}</pre></section>"#,
            icon("cube"),
            esc(&text)
        );
        html = html.replacen(
            r#"<section id="diagnosi""#,
            &format!(r#"{section}<section id="diagnosi""#),
            1,
        );
        markdown = format!("```\n{text}\n```\n\n{markdown}");
    }
    let n = st.counter.fetch_add(1, Ordering::Relaxed);
    let id = format!(
        "{n}{:x}",
        (r.meta.start_time.unsigned_abs() ^ (n.wrapping_mul(2_654_435_761))) & 0xffff
    );
    let title = if r.meta.platform.is_empty() {
        r.meta.kind.clone()
    } else {
        r.meta.platform.clone()
    };
    let mut detail = vec![r.meta.kind.clone()];
    if let Some(t) = r.meta.tps {
        detail.push(format!("TPS {}", fmt_num(t[0])));
    }
    if let Some(b) = r.analysis.main.busy_per_tick {
        detail.push(format!("{} ms/tick", fmt_num(b)));
    }
    if r.meta.duration_s > 0.0 {
        detail.push(fmt_duration(r.meta.duration_s));
    }
    let bad = r
        .findings
        .iter()
        .any(|f| f.severity <= crate::rules::Severity::Alto);
    let mut reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
    reports.insert(
        0,
        Stored {
            id: id.clone(),
            title,
            detail: detail.join(" · "),
            bad,
            html,
            markdown,
        },
    );
    reports.truncate(MAX_STORED);
    Ok(id)
}

fn with_actions(html: &str, id: &str) -> String {
    html.replacen(
        r#"<div class="actions">"#,
        &format!(
            r#"<div class="actions"><a class="btn sec" href="/">{}Nuova analisi</a><a class="btn sec" href="/r/{id}/html">{}HTML</a><a class="btn sec" href="/r/{id}/md">{}Markdown</a>"#,
            icon("plus"),
            icon("download"),
            icon("download")
        ),
        1,
    )
}

fn shell(title: &str, side_extra: &str, main: &str, script: &str) -> String {
    format!(
        r#"<!doctype html><html lang="it" data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{}</title><style>{}{}</style></head><body><div class="shell"><aside class="side"><div class="brand"><span class="logo">{}</span><span>sparkscope</span></div><nav><a class="on" href="/">{}<span>Nuova analisi</span></a></nav>{}<div class="sidefoot"><div class="srv"><b>sparkscope</b><span>v{}</span></div><button id="theme" class="ghost" title="Tema chiaro/scuro">{}</button></div></aside><main class="content">{}<footer><span>sparkscope</span> v{} · in esecuzione sul tuo computer</footer></main></div><script>{}{}</script></body></html>"#,
        esc(title),
        CSS,
        EXTRA_CSS,
        icon("bolt"),
        icon("plus"),
        side_extra,
        env!("CARGO_PKG_VERSION"),
        icon("moon"),
        main,
        env!("CARGO_PKG_VERSION"),
        THEME_JS,
        script
    )
}

fn index_page(st: &State) -> String {
    let reports = st.reports.lock().unwrap_or_else(|e| e.into_inner());
    let mut recent = String::new();
    let mut side = String::new();
    if !reports.is_empty() {
        side.push_str(r#"<div class="recent"><h5>Analisi recenti</h5>"#);
        recent.push_str(&format!(r#"<section class="panel"><h2><span class="hi">{}</span>Analisi recenti</h2><div class="rlist">"#, icon("history")));
        for r in reports.iter() {
            let cls = if r.bad { "bad" } else { "ok" };
            side.push_str(&format!(
                r#"<a href="/r/{}"><i class="{cls}"></i><span>{}</span></a>"#,
                r.id,
                esc(&r.title)
            ));
            recent.push_str(&format!(
                r#"<a class="ritem {cls}" href="/r/{}"><div><b>{}</b><span>{}</span></div><span class="go">Apri →</span></a>"#,
                r.id,
                esc(&r.title),
                esc(&r.detail)
            ));
        }
        side.push_str("</div>");
        recent.push_str("</div></section>");
    }
    let main = format!(
        r#"<div class="top"><div><h1>Nuova analisi</h1><p class="sub">Incolla il link del profilo spark oppure carica un file .sparkprofile, .sparkheap o .sparkhealth. L'analisi avviene sul tuo computer.</p></div></div>
<section class="panel"><h2><span class="hi">{link}</span>Link del profilo</h2>
<form id="lform" class="lrow"><input id="link" autocomplete="off" placeholder="https://spark.lucko.me/AbCd1234 oppure solo il codice"><button class="btn" type="submit">{bolt}Analizza</button></form>
<p class="note">È il link che spark scrive in chat dopo <code>/spark profiler stop</code>.</p></section>
<section class="panel"><h2><span class="hi">{upload}</span>File del profilo</h2>
<label id="drop" class="drop"><input id="file" type="file" accept=".sparkprofile,.sparkheap,.sparkhealth" hidden><span class="dicon">{upload}</span><b>Trascina qui il file</b><span>oppure clicca per sceglierlo</span><small>I file salvati con <code>/spark profiler stop --save-to-file</code> sono in <code>plugins/spark/</code></small></label></section>
<div id="busy" class="busy" hidden><span class="spin"></span><span id="busytext">Analisi in corso…</span></div>
<div id="err" class="errbox" hidden></div>
{recent}"#,
        link = icon("link"),
        bolt = icon("bolt"),
        upload = icon("upload"),
    );
    shell("sparkscope — nuova analisi", &side, &main, INDEX_JS)
}

fn not_found() -> String {
    shell(
        "sparkscope — non trovato",
        "",
        r#"<div class="top"><div><h1>Report non trovato</h1><p class="sub">I report restano disponibili finché sparkscope è aperto. <a href="/">Torna alla pagina iniziale</a>.</p></div></div>"#,
        "",
    )
}

const EXTRA_CSS: &str = r#"
.lrow{display:flex;gap:8px;flex-wrap:wrap}.lrow input{flex:1;min-width:220px;padding:10px 12px;border:1px solid var(--line);border-radius:5px;background:var(--code);color:var(--fg);font:inherit;outline:none}.lrow input:focus{border-color:var(--acc)}
.drop{display:flex;flex-direction:column;align-items:center;justify-content:center;gap:4px;text-align:center;border:2px dashed var(--line);border-radius:8px;padding:34px 16px;cursor:pointer;background:var(--card2);transition:border-color .15s,background .15s}.drop:hover,.drop.over{border-color:var(--acc);background:var(--accl)}.drop b{color:var(--hd);font-size:15px}.drop span{color:var(--muted)}.drop small{color:var(--muted);margin-top:8px}.dicon{display:grid;place-items:center;width:48px;height:48px;border-radius:8px;background:var(--acc);color:#fff!important;margin-bottom:6px}.dicon svg{width:24px;height:24px}
.busy{display:flex;align-items:center;gap:12px;background:var(--card);border:1px solid var(--line);border-left:3px solid var(--acc);border-radius:8px;padding:14px 16px;margin-bottom:14px;color:var(--hd)}.busy[hidden],.errbox[hidden]{display:none}.spin{width:18px;height:18px;border:2px solid var(--line);border-top-color:var(--acc);border-radius:50%;animation:sp 0.8s linear infinite}@keyframes sp{to{transform:rotate(360deg)}}
.errbox{background:rgba(250,82,82,.1);border:1px solid var(--crit);border-radius:8px;padding:12px 16px;margin-bottom:14px;color:var(--crit);white-space:pre-wrap}
.rlist{display:flex;flex-direction:column;gap:8px}.ritem{display:flex;align-items:center;justify-content:space-between;gap:12px;background:var(--card2);border:1px solid var(--line);border-left:3px solid var(--ok);border-radius:6px;padding:10px 14px;color:var(--fg)}.ritem.bad{border-left-color:var(--crit)}.ritem:hover{text-decoration:none;border-color:var(--muted)}.ritem div{display:flex;flex-direction:column}.ritem b{color:var(--hd)}.ritem div span{color:var(--muted);font-size:12px}.go{color:var(--acc);font-size:13px;white-space:nowrap}
.recent{margin-top:10px;padding-top:10px;border-top:1px solid var(--line2);overflow:auto}.recent h5{margin:0 8px 6px}.recent a{display:flex;align-items:center;gap:8px;padding:6px 8px;border-radius:5px;color:var(--fg);font-size:12px}.recent a:hover{background:var(--card2);text-decoration:none}.recent a span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.recent i{width:8px;height:8px;border-radius:50%;flex:none;background:var(--ok)}.recent i.bad{background:var(--crit)}
pre.heap{margin:0;background:var(--code);border:1px solid var(--line);border-radius:6px;padding:12px;overflow:auto;font:12px ui-monospace,Menlo,Consolas,monospace}
@media (max-width:900px){.recent{display:none}}
"#;

const THEME_JS: &str = r#"
(function(){var d=document.documentElement;try{var t=localStorage.getItem('sparkscope-theme');if(t)d.dataset.theme=t}catch(e){}var b=document.getElementById('theme');if(b)b.addEventListener('click',function(){var n=d.dataset.theme==='dark'?'light':'dark';d.dataset.theme=n;try{localStorage.setItem('sparkscope-theme',n)}catch(e){}})})();
"#;

const INDEX_JS: &str = r#"
(function(){
var busy=document.getElementById('busy'),bt=document.getElementById('busytext'),err=document.getElementById('err');
function start(t){err.hidden=true;bt.textContent=t;busy.hidden=false}
function fail(m){busy.hidden=true;err.textContent=m;err.hidden=false}
function done(r){return r.json().then(function(j){if(j.id){location.href='/r/'+j.id}else{fail(j.error||'Errore sconosciuto')}})}
document.getElementById('lform').addEventListener('submit',function(e){e.preventDefault();var v=document.getElementById('link').value.trim();if(!v){fail('Incolla prima un link o un codice spark.');return}start('Scarico e analizzo il profilo…');fetch('/api/link',{method:'POST',body:v}).then(done).catch(function(x){fail('Connessione a sparkscope persa: '+x)})});
function send(f){if(!f)return;start('Analizzo '+f.name+' ('+(f.size/1048576).toFixed(1)+' MB)…');fetch('/api/file?name='+encodeURIComponent(f.name),{method:'POST',body:f}).then(done).catch(function(x){fail('Connessione a sparkscope persa: '+x)})}
var drop=document.getElementById('drop'),file=document.getElementById('file');
file.addEventListener('change',function(){send(file.files[0])});
['dragenter','dragover'].forEach(function(t){drop.addEventListener(t,function(e){e.preventDefault();drop.classList.add('over')})});
['dragleave','drop'].forEach(function(t){drop.addEventListener(t,function(e){e.preventDefault();drop.classList.remove('over')})});
drop.addEventListener('drop',function(e){send(e.dataTransfer.files[0])});
document.addEventListener('dragover',function(e){e.preventDefault()});document.addEventListener('drop',function(e){e.preventDefault();if(e.target.closest&&!e.target.closest('#drop'))send(e.dataTransfer.files[0])});
document.getElementById('link').addEventListener('paste',function(){var i=this;setTimeout(function(){if(/spark\.lucko\.me\/|^[A-Za-z0-9]{6,}$/.test(i.value.trim()))document.getElementById('lform').requestSubmit()},50)});
})();
"#;

#[cfg(test)]
mod tests {
    #[test]
    fn decode() {
        assert_eq!(super::percent_decode("a%20b%2Fc+d"), "a b/c d");
        assert_eq!(super::percent_decode("100%"), "100%");
    }
}
