use crate::heap::HeapReport;
use crate::report::html::{ANIM_JS, CSS, FAVICON, LOGO_SVG, esc, icon};
use crate::report::{Report, fmt_duration, fmt_num};

pub struct Recent {
    pub href: String,
    pub title: String,
    pub detail: String,
    pub bad: bool,
}

pub struct Rendered {
    pub html: String,
    pub markdown: String,
    pub title: String,
    pub detail: String,
    pub bad: bool,
}

pub fn render_report(r: &Report, heap_report: Option<&HeapReport>) -> Rendered {
    let mut html = crate::report::html::render(r);
    let mut markdown = crate::report::markdown::render(r);
    if let Some(h) = heap_report {
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
        html = html.replacen("</style>", &format!("{HEAP_CSS}</style>"), 1);
        markdown = format!("```\n{text}\n```\n\n{markdown}");
    }
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
    Rendered {
        html,
        markdown,
        title,
        detail: detail.join(" · "),
        bad,
    }
}

const HEAP_CSS: &str = "pre.heap{margin:0;background:var(--code);border:1px solid var(--line);border-radius:6px;padding:12px;overflow:auto;font:12px ui-monospace,Menlo,Consolas,monospace}";

pub fn with_static_actions(html: &str, home: &str) -> String {
    let buttons = format!(
        r#"<div class="actions"><a class="btn sec" href="{}">{}{}</a>"#,
        esc(home),
        icon("plus"),
        crate::i18n::t("Nuova analisi", "New analysis")
    );
    html.replacen(r#"<div class="actions">"#, &buttons, 1)
}

pub struct Page<'a> {
    pub crumb: &'a str,
    pub lang_switch: &'a str,
    pub title: &'a str,
    pub side_extra: &'a str,
    pub main: &'a str,
    pub script: &'a str,
    pub module_script: &'a str,
    pub home: &'a str,
    pub footer: &'a str,
}

pub fn shell(p: &Page) -> String {
    let module = if p.module_script.is_empty() {
        String::new()
    } else {
        format!(r#"<script type="module">{}</script>"#, p.module_script)
    };
    format!(
        r#"<!doctype html><html lang="{}" data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">{}<title>{}</title><style>{}{}</style></head><body><div class="shell"><aside class="side"><a class="brand" href="{}" title="Home"><span class="logo">{}</span><span>Spark-Analyzer</span></a><nav><a class="on" href="{}">{}<span>{}</span></a></nav>{}<div class="sidefoot"><div class="srv"><b>Spark-Analyzer</b><span>v{}</span></div><button id="langbtn" class="ghost lang" data-to="{}" data-href="{}" title="Lingua / Language">{}</button><button id="theme" class="ghost" title="{}">{}</button></div></aside><main class="content">{}{}</main></div><script>{}{}{}{}</script>{}</body></html>"#,
        crate::i18n::lang().code(),
        FAVICON,
        esc(p.title),
        CSS,
        EXTRA_CSS,
        esc(p.home),
        LOGO_SVG,
        esc(p.home),
        icon("plus"),
        crate::i18n::t("Nuova analisi", "New analysis"),
        p.side_extra,
        env!("CARGO_PKG_VERSION"),
        crate::report::html::lang_to(),
        esc(p.lang_switch),
        crate::report::html::lang_to().to_ascii_uppercase(),
        crate::i18n::t("Tema chiaro/scuro", "Light/dark theme"),
        icon("moon"),
        crate::report::html::topbar(p.crumb, None),
        p.main,
        THEME_JS,
        ANIM_JS,
        messages_js(),
        p.script,
        module
    )
}

pub fn recent_html(recent: &[Recent]) -> (String, String) {
    let mut panel = String::new();
    let mut side = String::new();
    if !recent.is_empty() {
        side.push_str(&format!(
            r#"<div class="recent"><h5>{}</h5>"#,
            crate::i18n::t("Analisi recenti", "Recent analyses")
        ));
        panel.push_str(&format!(
            r#"<section class="panel"><h2><span class="hi">{}</span>{}</h2><div class="rlist">"#,
            icon("history"),
            crate::i18n::t("Analisi recenti", "Recent analyses")
        ));
        for r in recent {
            let cls = if r.bad { "bad" } else { "ok" };
            side.push_str(&format!(
                r#"<a href="{}"><i class="{cls}"></i><span>{}</span></a>"#,
                esc(&r.href),
                esc(&r.title)
            ));
            panel.push_str(&format!(
                r#"<a class="ritem {cls}" href="{}"><div><b>{}</b><span>{}</span></div><span class="go">{}</span></a>"#,
                esc(&r.href),
                esc(&r.title),
                esc(&r.detail),
                crate::i18n::t("Apri →", "Open →")
            ));
        }
        side.push_str("</div>");
        panel.push_str("</div></section>");
    }
    (side, panel)
}

pub fn index_main(subtitle: &str, recent_panel: &str) -> String {
    let t = crate::i18n::t;
    format!(
        r#"<div class="top"><div><h1>{h1}</h1><p class="sub">{}</p></div></div>
<section class="panel"><h2><span class="hi">{link}</span>{link_t}</h2>
<form id="lform" class="lrow"><input id="link" autocomplete="off" placeholder="{ph}"><button class="btn" type="submit">{bolt}{go}</button></form>
<p class="note">{note1} <code>/spark profiler stop</code>.</p></section>
<section class="panel"><h2><span class="hi">{upload}</span>{file_t}</h2>
<label id="drop" class="drop"><input id="file" type="file" accept=".sparkprofile,.sparkheap,.sparkhealth" hidden><span class="dicon">{upload}</span><b>{drop}</b><span>{click}</span><small>{saved} <code>/spark profiler stop --save-to-file</code> {in_dir} <code>plugins/spark/</code></small></label></section>
<div id="busy" class="busy" hidden><span class="spin"></span><span id="busytext">{busy}</span></div>
<div id="err" class="errbox" hidden></div>
<div id="recentbox">{recent_panel}</div>"#,
        esc(subtitle),
        h1 = t("Nuova analisi", "New analysis"),
        link = icon("link"),
        link_t = t("Link del profilo", "Profile link"),
        ph = t(
            "https://spark.lucko.me/AbCd1234 oppure solo il codice",
            "https://spark.lucko.me/AbCd1234 or just the code"
        ),
        bolt = icon("bolt"),
        go = t("Analizza", "Analyze"),
        note1 = t(
            "È il link che spark scrive in chat dopo",
            "It is the link spark prints in chat after"
        ),
        upload = icon("upload"),
        file_t = t("File del profilo", "Profile file"),
        drop = t("Trascina qui il file", "Drop the file here"),
        click = t("oppure clicca per sceglierlo", "or click to choose it"),
        saved = t("I file salvati con", "Files saved with"),
        in_dir = t("sono in", "are in"),
        busy = t("Analisi in corso…", "Analyzing…"),
    )
}

fn messages_js() -> String {
    let t = crate::i18n::t;
    let m = serde_json::json!({
        "lang": crate::i18n::lang().code(),
        "unknown": t("Errore sconosciuto", "Unknown error"),
        "pasteFirst": t("Incolla prima un link o un codice spark.", "Paste a spark link or code first."),
        "downloading": t("Scarico e analizzo il profilo…", "Downloading and analyzing the profile…"),
        "downloadingKey": t("Scarico il profilo {k} da spark…", "Downloading profile {k} from spark…"),
        "analyzing": t("Analizzo il profilo…", "Analyzing the profile…"),
        "analyzingFile": t("Analizzo {f} ({s} MB)…", "Analyzing {f} ({s} MB)…"),
        "lost": t("Connessione a Spark-Analyzer persa: ", "Lost connection to Spark-Analyzer: "),
        "wasm": t("Impossibile caricare il modulo di analisi: ", "Could not load the analysis module: "),
        "notFound": t("Profilo non trovato: il link potrebbe essere scaduto (spark conserva i profili per un periodo limitato).", "Profile not found: the link may have expired (spark keeps profiles for a limited time)."),
        "httpFail": t("Download fallito (HTTP {s})", "Download failed (HTTP {s})"),
        "badLink": t("Link o codice spark non valido. Esempio: https://spark.lucko.me/AbCd1234", "Invalid spark link or code. Example: https://spark.lucko.me/AbCd1234"),
        "recent": t("Analisi recenti", "Recent analyses"),
        "open": t("Apri →", "Open →"),
    });
    format!("var M={m};")
}

pub fn local_index(recent: &[Recent]) -> String {
    let t = crate::i18n::t;
    let (side, panel) = recent_html(recent);
    let main = index_main(
        t(
            "Incolla il link del profilo spark oppure carica un file .sparkprofile, .sparkheap o .sparkhealth. L'analisi avviene sul tuo computer.",
            "Paste the spark profile link or upload a .sparkprofile, .sparkheap or .sparkhealth file. The analysis runs on your computer.",
        ),
        &panel,
    );
    let switch = format!("/?lang={}", crate::report::html::lang_to());
    shell(&Page {
        crumb: t("Nuova analisi", "New analysis"),
        lang_switch: &switch,
        title: t(
            "Spark-Analyzer — nuova analisi",
            "Spark-Analyzer — new analysis",
        ),
        side_extra: &side,
        main: &main,
        script: LOCAL_JS,
        module_script: "",
        home: if crate::i18n::is_en() {
            "/?lang=en"
        } else {
            "/"
        },
        footer: t("in esecuzione sul tuo computer", "running on your computer"),
    })
}

pub fn local_not_found() -> String {
    let t = crate::i18n::t;
    let main = format!(
        r#"<div class="top"><div><h1>{}</h1><p class="sub">{} <a href="/">{}</a>.</p></div></div>"#,
        t("Report non trovato", "Report not found"),
        t(
            "I report restano disponibili finché Spark-Analyzer è aperto.",
            "Reports stay available while Spark-Analyzer is running."
        ),
        t("Torna alla pagina iniziale", "Back to the home page")
    );
    shell(&Page {
        crumb: t("Report non trovato", "Report not found"),
        lang_switch: "/",
        title: t("Spark-Analyzer — non trovato", "Spark-Analyzer — not found"),
        side_extra: "",
        main: &main,
        script: "",
        module_script: "",
        home: "/",
        footer: t("in esecuzione sul tuo computer", "running on your computer"),
    })
}

pub fn static_page_name(l: crate::i18n::Lang) -> &'static str {
    match l {
        crate::i18n::Lang::It => "index.html",
        crate::i18n::Lang::En => "en.html",
    }
}

pub fn static_index() -> String {
    let t = crate::i18n::t;
    let main = index_main(
        t(
            "Incolla il link del profilo spark oppure carica un file .sparkprofile, .sparkheap o .sparkhealth. L'analisi avviene nel tuo browser: il file non viene inviato a nessun server.",
            "Paste the spark profile link or upload a .sparkprofile, .sparkheap or .sparkhealth file. The analysis runs in your browser: the file is not sent to any server.",
        ),
        "",
    );
    let other = if crate::i18n::is_en() {
        "./"
    } else {
        "./en.html"
    };
    let home = if crate::i18n::is_en() {
        "./en.html"
    } else {
        "./"
    };
    shell(&Page {
        crumb: t("Nuova analisi", "New analysis"),
        lang_switch: other,
        title: t(
            "Spark-Analyzer — analisi profili spark",
            "Spark-Analyzer — spark profile analysis",
        ),
        side_extra: r#"<div id="recentside"></div>"#,
        main: &main,
        script: "",
        module_script: STATIC_JS,
        home,
        footer: t(
            "analisi eseguita nel tuo browser",
            "analysis runs in your browser",
        ),
    })
}

pub const EXTRA_CSS: &str = r#"
.lrow{display:flex;gap:8px;flex-wrap:wrap}.lrow input{flex:1;min-width:220px;padding:10px 12px;border:1px solid var(--line);border-radius:5px;background:var(--code);color:var(--fg);font:inherit;outline:none}.lrow input:focus{border-color:var(--acc)}
.drop{display:flex;flex-direction:column;align-items:center;justify-content:center;gap:4px;text-align:center;border:2px dashed var(--line);border-radius:8px;padding:34px 16px;cursor:pointer;background:var(--card2);transition:border-color .15s,background .15s}.drop:hover,.drop.over{border-color:var(--acc);background:var(--accl)}.drop b{color:var(--hd);font-size:15px}.drop span{color:var(--muted)}.drop small{color:var(--muted);margin-top:8px}.dicon{display:grid;place-items:center;width:48px;height:48px;border-radius:8px;background:var(--acc);color:#fff!important;margin-bottom:6px}.dicon svg{width:24px;height:24px}
.busy{display:flex;align-items:center;gap:12px;background:var(--card);border:1px solid var(--line);border-left:3px solid var(--acc);border-radius:8px;padding:14px 16px;margin-bottom:14px;color:var(--hd)}.busy[hidden],.errbox[hidden]{display:none}.spin{width:18px;height:18px;border:2px solid var(--line);border-top-color:var(--acc);border-radius:50%;animation:sp 0.8s linear infinite}@keyframes sp{to{transform:rotate(360deg)}}
.errbox{background:rgba(250,82,82,.1);border:1px solid var(--crit);border-radius:8px;padding:12px 16px;margin-bottom:14px;color:var(--crit);white-space:pre-wrap}
.rlist{display:flex;flex-direction:column;gap:8px}.ritem{display:flex;align-items:center;justify-content:space-between;gap:12px;background:var(--card2);border:1px solid var(--line);border-left:3px solid var(--ok);border-radius:6px;padding:10px 14px;color:var(--fg)}.ritem.bad{border-left-color:var(--crit)}.ritem:hover{text-decoration:none;border-color:var(--muted)}.ritem div{display:flex;flex-direction:column}.ritem b{color:var(--hd)}.ritem div span{color:var(--muted);font-size:12px}.go{color:var(--acc);font-size:13px;white-space:nowrap}
.recent{margin-top:10px;padding-top:10px;border-top:1px solid var(--line2);overflow:auto}.recent h5{margin:0 8px 6px}.recent a{display:flex;align-items:center;gap:8px;padding:6px 8px;border-radius:5px;color:var(--fg);font-size:12px}.recent a:hover{background:var(--card2);text-decoration:none}.recent a span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.recent i{width:8px;height:8px;border-radius:50%;flex:none;background:var(--ok)}.recent i.bad{background:var(--crit)}
.ghost.lang{font-size:11px;font-weight:700;letter-spacing:.04em;color:var(--muted)}
pre.heap{margin:0;background:var(--code);border:1px solid var(--line);border-radius:6px;padding:12px;overflow:auto;font:12px ui-monospace,Menlo,Consolas,monospace}
@media (max-width:900px){.recent{display:none}}
"#;

pub const THEME_JS: &str = r#"
(function(){var d=document.documentElement;try{var t=localStorage.getItem('sparkscope-theme');if(t)d.dataset.theme=t}catch(e){}var b=document.getElementById('theme');if(b)b.addEventListener('click',function(){var n=d.dataset.theme==='dark'?'light':'dark';d.dataset.theme=n;try{localStorage.setItem('sparkscope-theme',n)}catch(e){}})})();
"#;

pub const LOCAL_JS: &str = r#"
(function(){
var LQ=M.lang==='en'?'lang=en':'';
try{var pref=localStorage.getItem('sa-lang');if(pref&&pref!==M.lang&&!/[?&]lang=/.test(location.search)){location.replace('/?lang='+pref);return}}catch(e){}
var lb=document.getElementById('langbtn');if(lb)lb.addEventListener('click',function(){try{localStorage.setItem('sa-lang',lb.dataset.to)}catch(e){}location.href=lb.dataset.href});
var busy=document.getElementById('busy'),bt=document.getElementById('busytext'),err=document.getElementById('err');
function start(t){err.hidden=true;bt.textContent=t;busy.hidden=false}
function fail(m){busy.hidden=true;err.textContent=m;err.hidden=false}
function done(r){return r.json().then(function(j){if(j.id){location.href='/r/'+j.id+(LQ?'?'+LQ:'')}else{fail(j.error||M.unknown)}})}
document.getElementById('lform').addEventListener('submit',function(e){e.preventDefault();var v=document.getElementById('link').value.trim();if(!v){fail(M.pasteFirst);return}start(M.downloading);fetch('/api/link?'+LQ,{method:'POST',body:v}).then(done).catch(function(x){fail(M.lost+x)})});
function send(f){if(!f)return;start(M.analyzingFile.replace('{f}',f.name).replace('{s}',(f.size/1048576).toFixed(1)));fetch('/api/file?name='+encodeURIComponent(f.name)+'&'+LQ,{method:'POST',body:f}).then(done).catch(function(x){fail(M.lost+x)})}
var drop=document.getElementById('drop'),file=document.getElementById('file');
file.addEventListener('change',function(){send(file.files[0])});
['dragenter','dragover'].forEach(function(t){drop.addEventListener(t,function(e){e.preventDefault();drop.classList.add('over')})});
['dragleave','drop'].forEach(function(t){drop.addEventListener(t,function(e){e.preventDefault();drop.classList.remove('over')})});
drop.addEventListener('drop',function(e){send(e.dataTransfer.files[0])});
document.addEventListener('dragover',function(e){e.preventDefault()});document.addEventListener('drop',function(e){e.preventDefault();if(e.target.closest&&!e.target.closest('#drop'))send(e.dataTransfer.files[0])});
document.getElementById('link').addEventListener('paste',function(){var i=this;setTimeout(function(){if(/spark\.lucko\.me\/|^[A-Za-z0-9]{6,}$/.test(i.value.trim()))document.getElementById('lform').requestSubmit()},50)});
})();
"#;

const STATIC_JS: &str = r#"
import init, { analyze } from './pkg/sparkscope_web.js';
var OTHER=document.getElementById('langbtn').dataset.href;
var REDIR=false;try{var pref=localStorage.getItem('sa-lang');if(pref&&pref!==M.lang){REDIR=true;location.replace(OTHER+location.search)}}catch(e){}
if(REDIR)await new Promise(function(){});
document.getElementById('langbtn').addEventListener('click',function(){try{localStorage.setItem('sa-lang',this.dataset.to)}catch(e){}location.href=OTHER+location.search});
var busy=document.getElementById('busy'),bt=document.getElementById('busytext'),err=document.getElementById('err');
function start(t){err.hidden=true;bt.textContent=t;busy.hidden=false}
function fail(m){busy.hidden=true;err.textContent=m;err.hidden=false}
var ready=init().catch(function(e){fail(M.wasm+e);throw e});
function extractKey(v){v=v.trim();var m=v.match(/^(?:https?:\/\/)?[^\/\s]+\/([A-Za-z0-9]+)/);if(m)return m[1];return /^[A-Za-z0-9]{4,64}$/.test(v)?v:null}
var RKEY='sparkscope-recent';
function loadRecent(){try{return JSON.parse(localStorage.getItem(RKEY)||'[]')}catch(e){return []}}
function saveRecent(item){try{var l=loadRecent().filter(function(x){return x.key!==item.key});l.unshift(item);localStorage.setItem(RKEY,JSON.stringify(l.slice(0,15)))}catch(e){}}
function esc(s){return String(s).replace(/[&<>"']/g,function(c){return {'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]})}
function showRecent(){var l=loadRecent();if(!l.length)return;var side='<div class="recent"><h5>'+M.recent+'</h5>',p='<section class="panel"><h2>'+M.recent+'</h2><div class="rlist">';l.forEach(function(x){var c=x.bad?'bad':'ok';var h='?p='+encodeURIComponent(x.key);side+='<a href="'+h+'"><i class="'+c+'"></i><span>'+esc(x.title)+'</span></a>';p+='<a class="ritem '+c+'" href="'+h+'"><div><b>'+esc(x.title)+'</b><span>'+esc(x.detail)+' · '+esc(x.key)+'</span></div><span class="go">'+M.open+'</span></a>'});document.getElementById('recentside').innerHTML=side+'</div>';document.getElementById('recentbox').innerHTML=p+'</div></section>'}
var HOME=location.pathname;
var last=null;
var CUR=M.lang;
function render(bytes,name,key,lang,win){lang=lang||CUR;CUR=lang;last={bytes:bytes,name:name,key:key,win:win||null};return ready.then(function(){var res=JSON.parse(analyze(bytes,name,key||undefined,HOME,lang,win||undefined));if(key){try{var u=new URL(location.href);u.searchParams.set('p',key);if(win)u.searchParams.set('w',win);else u.searchParams.delete('w');history.replaceState(null,'',u.toString())}catch(e){}}if(key)saveRecent({key:key,title:res.title,detail:res.detail,bad:res.bad});setTimeout(function(){document.open();document.write(res.html);document.close();window.scrollTo(0,0)},0)})}
window.__saRender=function(l){if(!last)return;render(last.bytes,last.name,last.key,l,last.win).catch(function(e){alert(e.message||String(e))})};
window.__saFilter=function(w){if(!last)return;render(last.bytes,last.name,last.key,CUR,w||null).catch(function(e){alert(e.message||String(e))})};
function runKey(key){start(M.downloadingKey.replace('{k}',key));fetch('https://spark-usercontent.lucko.me/'+encodeURIComponent(key)).then(function(r){if(r.status===404)throw new Error(M.notFound);if(!r.ok)throw new Error(M.httpFail.replace('{s}',r.status));return r.arrayBuffer()}).then(function(b){bt.textContent=M.analyzing;return render(new Uint8Array(b),key,key,null,new URLSearchParams(location.search).get('w'))}).catch(function(e){fail(e.message||String(e))})}
document.getElementById('lform').addEventListener('submit',function(e){e.preventDefault();var k=extractKey(document.getElementById('link').value);if(!k){fail(M.badLink);return}location.search='?p='+encodeURIComponent(k)});
function send(f){if(!f)return;start(M.analyzingFile.replace('{f}',f.name).replace('{s}',(f.size/1048576).toFixed(1)));f.arrayBuffer().then(function(b){return render(new Uint8Array(b),f.name,null)}).catch(function(e){fail(e.message||String(e))})}
var drop=document.getElementById('drop'),file=document.getElementById('file');
file.addEventListener('change',function(){send(file.files[0])});
['dragenter','dragover'].forEach(function(t){drop.addEventListener(t,function(e){e.preventDefault();drop.classList.add('over')})});
['dragleave','drop'].forEach(function(t){drop.addEventListener(t,function(e){e.preventDefault();drop.classList.remove('over')})});
drop.addEventListener('drop',function(e){send(e.dataTransfer.files[0])});
document.addEventListener('dragover',function(e){e.preventDefault()});document.addEventListener('drop',function(e){e.preventDefault();if(e.target.closest&&!e.target.closest('#drop'))send(e.dataTransfer.files[0])});
document.getElementById('link').addEventListener('paste',function(){var i=this;setTimeout(function(){if(extractKey(i.value))document.getElementById('lform').requestSubmit()},50)});
showRecent();
var q=new URLSearchParams(location.search).get('p');if(q){var k=extractKey(q);if(k){document.getElementById('link').value='https://spark.lucko.me/'+k;runKey(k)}}
"#;
