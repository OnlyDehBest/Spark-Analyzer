use crate::heap::HeapReport;
use crate::report::html::{CSS, esc, icon};
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

pub fn with_static_actions(html: &str, markdown: &str, home: &str) -> String {
    let buttons = format!(
        r#"<div class="actions"><a class="btn sec" href="{}">{}Nuova analisi</a><button class="btn sec" id="dlh">{}HTML</button><button class="btn sec" id="dlm">{}Markdown</button>"#,
        esc(home),
        icon("plus"),
        icon("download"),
        icon("download")
    );
    let md = markdown.replace("</", "<\\/");
    let script = format!(
        r#"<script type="text/plain" id="mdsrc">{md}</script><script>(function(){{function dl(n,t,c){{var a=document.createElement('a');a.href=URL.createObjectURL(new Blob([c],{{type:t}}));a.download=n;document.body.appendChild(a);a.click();setTimeout(function(){{URL.revokeObjectURL(a.href);a.remove()}},500)}}var h=document.getElementById('dlh'),m=document.getElementById('dlm');if(h)h.addEventListener('click',function(){{dl('sparkscope-report.html','text/html','<!doctype html>'+document.documentElement.outerHTML)}});if(m)m.addEventListener('click',function(){{dl('sparkscope-report.md','text/markdown',document.getElementById('mdsrc').textContent.replace(/<\\\//g,'</'))}})}})();</script></body>"#
    );
    html.replacen(r#"<div class="actions">"#, &buttons, 1)
        .replacen("</body>", &script, 1)
}

pub struct Page<'a> {
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
        r#"<!doctype html><html lang="it" data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{}</title><style>{}{}</style></head><body><div class="shell"><aside class="side"><div class="brand"><span class="logo">{}</span><span>sparkscope</span></div><nav><a class="on" href="{}">{}<span>Nuova analisi</span></a></nav>{}<div class="sidefoot"><div class="srv"><b>sparkscope</b><span>v{}</span></div><button id="theme" class="ghost" title="Tema chiaro/scuro">{}</button></div></aside><main class="content">{}<footer><span>sparkscope</span> v{} · {}</footer></main></div><script>{}{}</script>{}</body></html>"#,
        esc(p.title),
        CSS,
        EXTRA_CSS,
        icon("bolt"),
        esc(p.home),
        icon("plus"),
        p.side_extra,
        env!("CARGO_PKG_VERSION"),
        icon("moon"),
        p.main,
        env!("CARGO_PKG_VERSION"),
        p.footer,
        THEME_JS,
        p.script,
        module
    )
}

pub fn recent_html(recent: &[Recent]) -> (String, String) {
    let mut panel = String::new();
    let mut side = String::new();
    if !recent.is_empty() {
        side.push_str(r#"<div class="recent"><h5>Analisi recenti</h5>"#);
        panel.push_str(&format!(r#"<section class="panel"><h2><span class="hi">{}</span>Analisi recenti</h2><div class="rlist">"#, icon("history")));
        for r in recent {
            let cls = if r.bad { "bad" } else { "ok" };
            side.push_str(&format!(
                r#"<a href="{}"><i class="{cls}"></i><span>{}</span></a>"#,
                esc(&r.href),
                esc(&r.title)
            ));
            panel.push_str(&format!(
                r#"<a class="ritem {cls}" href="{}"><div><b>{}</b><span>{}</span></div><span class="go">Apri →</span></a>"#,
                esc(&r.href),
                esc(&r.title),
                esc(&r.detail)
            ));
        }
        side.push_str("</div>");
        panel.push_str("</div></section>");
    }
    (side, panel)
}

pub fn index_main(subtitle: &str, recent_panel: &str) -> String {
    format!(
        r#"<div class="top"><div><h1>Nuova analisi</h1><p class="sub">{}</p></div></div>
<section class="panel"><h2><span class="hi">{link}</span>Link del profilo</h2>
<form id="lform" class="lrow"><input id="link" autocomplete="off" placeholder="https://spark.lucko.me/AbCd1234 oppure solo il codice"><button class="btn" type="submit">{bolt}Analizza</button></form>
<p class="note">È il link che spark scrive in chat dopo <code>/spark profiler stop</code>.</p></section>
<section class="panel"><h2><span class="hi">{upload}</span>File del profilo</h2>
<label id="drop" class="drop"><input id="file" type="file" accept=".sparkprofile,.sparkheap,.sparkhealth" hidden><span class="dicon">{upload}</span><b>Trascina qui il file</b><span>oppure clicca per sceglierlo</span><small>I file salvati con <code>/spark profiler stop --save-to-file</code> sono in <code>plugins/spark/</code></small></label></section>
<div id="busy" class="busy" hidden><span class="spin"></span><span id="busytext">Analisi in corso…</span></div>
<div id="err" class="errbox" hidden></div>
<div id="recentbox">{recent_panel}</div>"#,
        esc(subtitle),
        link = icon("link"),
        bolt = icon("bolt"),
        upload = icon("upload"),
    )
}

pub fn local_index(recent: &[Recent]) -> String {
    let (side, panel) = recent_html(recent);
    let main = index_main(
        "Incolla il link del profilo spark oppure carica un file .sparkprofile, .sparkheap o .sparkhealth. L'analisi avviene sul tuo computer.",
        &panel,
    );
    shell(&Page {
        title: "sparkscope — nuova analisi",
        side_extra: &side,
        main: &main,
        script: LOCAL_JS,
        module_script: "",
        home: "/",
        footer: "in esecuzione sul tuo computer",
    })
}

pub fn local_not_found() -> String {
    shell(&Page {
        title: "sparkscope — non trovato",
        side_extra: "",
        main: r#"<div class="top"><div><h1>Report non trovato</h1><p class="sub">I report restano disponibili finché sparkscope è aperto. <a href="/">Torna alla pagina iniziale</a>.</p></div></div>"#,
        script: "",
        module_script: "",
        home: "/",
        footer: "in esecuzione sul tuo computer",
    })
}

pub fn static_index() -> String {
    let main = index_main(
        "Incolla il link del profilo spark oppure carica un file .sparkprofile, .sparkheap o .sparkhealth. L'analisi avviene nel tuo browser: il file non viene inviato a nessun server.",
        "",
    );
    shell(&Page {
        title: "sparkscope — analisi profili spark",
        side_extra: r#"<div id="recentside"></div>"#,
        main: &main,
        script: "",
        module_script: STATIC_JS,
        home: "./",
        footer: "analisi eseguita nel tuo browser",
    })
}

pub const EXTRA_CSS: &str = r#"
.lrow{display:flex;gap:8px;flex-wrap:wrap}.lrow input{flex:1;min-width:220px;padding:10px 12px;border:1px solid var(--line);border-radius:5px;background:var(--code);color:var(--fg);font:inherit;outline:none}.lrow input:focus{border-color:var(--acc)}
.drop{display:flex;flex-direction:column;align-items:center;justify-content:center;gap:4px;text-align:center;border:2px dashed var(--line);border-radius:8px;padding:34px 16px;cursor:pointer;background:var(--card2);transition:border-color .15s,background .15s}.drop:hover,.drop.over{border-color:var(--acc);background:var(--accl)}.drop b{color:var(--hd);font-size:15px}.drop span{color:var(--muted)}.drop small{color:var(--muted);margin-top:8px}.dicon{display:grid;place-items:center;width:48px;height:48px;border-radius:8px;background:var(--acc);color:#fff!important;margin-bottom:6px}.dicon svg{width:24px;height:24px}
.busy{display:flex;align-items:center;gap:12px;background:var(--card);border:1px solid var(--line);border-left:3px solid var(--acc);border-radius:8px;padding:14px 16px;margin-bottom:14px;color:var(--hd)}.busy[hidden],.errbox[hidden]{display:none}.spin{width:18px;height:18px;border:2px solid var(--line);border-top-color:var(--acc);border-radius:50%;animation:sp 0.8s linear infinite}@keyframes sp{to{transform:rotate(360deg)}}
.errbox{background:rgba(250,82,82,.1);border:1px solid var(--crit);border-radius:8px;padding:12px 16px;margin-bottom:14px;color:var(--crit);white-space:pre-wrap}
.rlist{display:flex;flex-direction:column;gap:8px}.ritem{display:flex;align-items:center;justify-content:space-between;gap:12px;background:var(--card2);border:1px solid var(--line);border-left:3px solid var(--ok);border-radius:6px;padding:10px 14px;color:var(--fg)}.ritem.bad{border-left-color:var(--crit)}.ritem:hover{text-decoration:none;border-color:var(--muted)}.ritem div{display:flex;flex-direction:column}.ritem b{color:var(--hd)}.ritem div span{color:var(--muted);font-size:12px}.go{color:var(--acc);font-size:13px;white-space:nowrap}
.recent{margin-top:10px;padding-top:10px;border-top:1px solid var(--line2);overflow:auto}.recent h5{margin:0 8px 6px}.recent a{display:flex;align-items:center;gap:8px;padding:6px 8px;border-radius:5px;color:var(--fg);font-size:12px}.recent a:hover{background:var(--card2);text-decoration:none}.recent a span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.recent i{width:8px;height:8px;border-radius:50%;flex:none;background:var(--ok)}.recent i.bad{background:var(--crit)}
pre.heap{margin:0;background:var(--code);border:1px solid var(--line);border-radius:6px;padding:12px;overflow:auto;font:12px ui-monospace,Menlo,Consolas,monospace}
@media (max-width:900px){.recent{display:none}}
"#;

pub const THEME_JS: &str = r#"
(function(){var d=document.documentElement;try{var t=localStorage.getItem('sparkscope-theme');if(t)d.dataset.theme=t}catch(e){}var b=document.getElementById('theme');if(b)b.addEventListener('click',function(){var n=d.dataset.theme==='dark'?'light':'dark';d.dataset.theme=n;try{localStorage.setItem('sparkscope-theme',n)}catch(e){}})})();
"#;

pub const LOCAL_JS: &str = r#"
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

const STATIC_JS: &str = r#"
import init, { analyze } from './pkg/sparkscope_web.js';
var busy=document.getElementById('busy'),bt=document.getElementById('busytext'),err=document.getElementById('err');
function start(t){err.hidden=true;bt.textContent=t;busy.hidden=false}
function fail(m){busy.hidden=true;err.textContent=m;err.hidden=false}
var ready=init().catch(function(e){fail('Impossibile caricare il modulo di analisi: '+e);throw e});
function extractKey(v){v=v.trim();var m=v.match(/^(?:https?:\/\/)?[^\/\s]+\/([A-Za-z0-9]+)/);if(m)return m[1];return /^[A-Za-z0-9]{4,64}$/.test(v)?v:null}
var RKEY='sparkscope-recent';
function loadRecent(){try{return JSON.parse(localStorage.getItem(RKEY)||'[]')}catch(e){return []}}
function saveRecent(item){try{var l=loadRecent().filter(function(x){return x.key!==item.key});l.unshift(item);localStorage.setItem(RKEY,JSON.stringify(l.slice(0,15)))}catch(e){}}
function esc(s){return String(s).replace(/[&<>"']/g,function(c){return {'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]})}
function showRecent(){var l=loadRecent();if(!l.length)return;var side='<div class="recent"><h5>Analisi recenti</h5>',p='<section class="panel"><h2>Analisi recenti</h2><div class="rlist">';l.forEach(function(x){var c=x.bad?'bad':'ok';var h='?p='+encodeURIComponent(x.key);side+='<a href="'+h+'"><i class="'+c+'"></i><span>'+esc(x.title)+'</span></a>';p+='<a class="ritem '+c+'" href="'+h+'"><div><b>'+esc(x.title)+'</b><span>'+esc(x.detail)+' · '+esc(x.key)+'</span></div><span class="go">Apri →</span></a>'});document.getElementById('recentside').innerHTML=side+'</div>';document.getElementById('recentbox').innerHTML=p+'</div></section>'}
function home(){return location.pathname}
function render(bytes,name,key){return ready.then(function(){var res=JSON.parse(analyze(bytes,name,key||undefined,home()));if(key)saveRecent({key:key,title:res.title,detail:res.detail,bad:res.bad});setTimeout(function(){document.open();document.write(res.html);document.close();window.scrollTo(0,0)},0)})}
function runKey(key){start('Scarico il profilo '+key+' da spark…');fetch('https://spark-usercontent.lucko.me/'+encodeURIComponent(key)).then(function(r){if(r.status===404)throw new Error('Profilo non trovato: il link potrebbe essere scaduto (spark conserva i profili per un periodo limitato).');if(!r.ok)throw new Error('Download fallito (HTTP '+r.status+')');return r.arrayBuffer()}).then(function(b){bt.textContent='Analizzo il profilo…';return render(new Uint8Array(b),key,key)}).catch(function(e){fail(e.message||String(e))})}
document.getElementById('lform').addEventListener('submit',function(e){e.preventDefault();var k=extractKey(document.getElementById('link').value);if(!k){fail('Link o codice spark non valido. Esempio: https://spark.lucko.me/AbCd1234');return}location.search='?p='+encodeURIComponent(k)});
function send(f){if(!f)return;start('Analizzo '+f.name+' ('+(f.size/1048576).toFixed(1)+' MB)…');f.arrayBuffer().then(function(b){return render(new Uint8Array(b),f.name,null)}).catch(function(e){fail(e.message||String(e))})}
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
