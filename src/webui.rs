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
        let section = heap_section(h, &text);
        let anchor = if html.contains(r#"<section id="macchina""#) {
            r#"<section id="macchina""#
        } else {
            r#"<section id="mondo""#
        };
        if html.contains(anchor) {
            html = html.replacen(anchor, &format!("{section}{anchor}"), 1);
        } else {
            html = html.replacen(
                r#"<footer class="copy">"#,
                &format!(r#"{section}<footer class="copy">"#),
                1,
            );
        }
        let label = crate::i18n::t("Memoria (heap)", "Memory (heap)");
        html = html.replacen(
            "</nav>",
            &format!(
                r##"<a href="#heap">{}<span>{label}</span></a></nav>"##,
                icon("cube")
            ),
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

fn heap_section(h: &HeapReport, text: &str) -> String {
    use crate::report::fmt_time;
    use std::fmt::Write;
    let t = crate::i18n::t;
    let bytes = |v: i64| fmt_time(v as f64, "byte");
    let mut b = String::new();
    let _ = write!(
        b,
        r#"<section id="heap" class="panel"><h2><span class="hi">{}</span>{}</h2><p class="note">{}</p>"#,
        icon("cube"),
        t("Memoria per plugin", "Memory by plugin"),
        esc(&tf!(
            "Heap totale {} in {} oggetti{}. Le classi sono assegnate al plugin in base al package. La cifra è la memoria degli oggetti del plugin: i dati che tiene in HashMap, liste e array compaiono come classi Java (sotto) e si aggiungono a questa.",
            "Total heap {} in {} objects{}. Classes are assigned to a plugin by their package. The figure is the memory of the plugin's own objects: data kept in HashMaps, lists and arrays shows up as Java classes (below) and comes on top of it.",
            bytes(h.total_size),
            crate::heap::count(h.total_instances),
            if h.players > 0 {
                tf!(", {} giocatori online", ", {} players online", h.players)
            } else {
                String::new()
            }
        ))
    );
    if h.by_plugin.is_empty() {
        let _ = write!(
            b,
            "<p>{}</p>",
            t(
                "Nessuna classe riconducibile ai plugin installati.",
                "No class could be linked to the installed plugins."
            )
        );
    } else {
        let _ = write!(
            b,
            r#"<table class="sortable hp"><thead><tr><th>Plugin</th><th class="n">{}</th><th class="n">% heap</th><th class="n">{}</th><th>{}</th><th>{}</th></tr></thead><tbody>"#,
            t("Memoria", "Memory"),
            t("Oggetti", "Objects"),
            t("Classi più pesanti", "Heaviest classes"),
            t("Segnale", "Signal")
        );
        for p in &h.by_plugin {
            let (cls, tag) = match p.level {
                2 => ("lv2", t("Sospetto leak", "Suspected leak")),
                1 => ("lv1", t("Da tenere d'occhio", "Keep an eye on")),
                _ => ("lv0", t("Normale", "Normal")),
            };
            let classes: Vec<String> = p
                .top
                .iter()
                .take(3)
                .map(|c| {
                    format!(
                        "<code title=\"{}\">{}</code> <span class=\"dim\">{} · {}</span>",
                        esc(&c.kind),
                        esc(crate::heap::short_class(&c.kind)),
                        bytes(c.size),
                        crate::heap::count(c.instances)
                    )
                })
                .collect();
            let why: Vec<String> = p
                .reasons
                .iter()
                .map(|r| format!("<div>{}</div>", esc(r)))
                .collect();
            let _ = write!(
                b,
                r#"<tr><td><b>{}</b> <span class="dim">{}</span></td><td class="n" data-v="{}">{}</td><td class="n" data-v="{:.4}">{:.2}%</td><td class="n" data-v="{}">{}</td><td class="small">{}</td><td class="small"><span class="lv {cls}">{tag}</span>{}</td></tr>"#,
                esc(&p.name),
                esc(&p.version),
                p.size,
                bytes(p.size),
                p.pct,
                p.pct,
                p.instances,
                crate::heap::count(p.instances),
                classes.join("<br>"),
                why.join("")
            );
        }
        b.push_str("</tbody></table>");
    }
    let _ = write!(
        b,
        r#"<p class="note">{}</p>"#,
        t(
            "Un solo heap summary è una fotografia: una cache grande non è per forza un leak. Per confermarlo rifai /spark heapsummary dopo qualche ora e confronta: se i numeri di un plugin salgono sempre e non scendono mai, è un leak.",
            "A single heap summary is a snapshot: a large cache is not necessarily a leak. To confirm it, run /spark heapsummary again a few hours later and compare: if a plugin's numbers keep going up and never come down, it's a leak."
        )
    );
    if !h.notes.is_empty() {
        b.push_str("<ul class=\"hnotes\">");
        for n in &h.notes {
            let _ = write!(b, "<li>{}</li>", esc(n));
        }
        b.push_str("</ul>");
    }
    let _ = write!(
        b,
        r#"<details class="hraw"><summary>{}</summary><pre class="heap">{}</pre></details></section>"#,
        t(
            "Dettaglio completo (package e classi più grandi)",
            "Full detail (largest packages and classes)"
        ),
        esc(text)
    );
    b
}

const HEAP_CSS: &str = ".lv{display:inline-block;border-radius:999px;padding:2px 9px;font-size:11px;font-weight:600;margin-bottom:4px;border:1px solid var(--line)}.lv2{color:var(--crit);border-color:var(--crit)}.lv1{color:var(--high);border-color:var(--high)}.lv0{color:var(--ok)}table.hp td{vertical-align:top}table.hp td:nth-child(5){min-width:260px}table.hp td.small div{margin-top:2px}.hnotes{margin:8px 0 12px;padding-left:18px;color:var(--fg)}.hnotes li{margin:4px 0}details.hraw{margin-top:8px}details.hraw summary{cursor:pointer;color:var(--muted);font-size:13px;margin-bottom:8px}pre.heap{margin:0;background:var(--code);border:1px solid var(--line);border-radius:6px;padding:12px;overflow:auto;font:12px ui-monospace,Menlo,Consolas,monospace}";

pub fn render_comparison(c: &crate::heapdiff::HeapComparison) -> Rendered {
    let t = crate::i18n::t;
    let leaks: Vec<String> = c.leaks().map(|p| p.name.clone()).collect();
    let platform = if c.after.platform.is_empty() {
        "Server".to_string()
    } else {
        c.after.platform.clone()
    };
    let detail = if leaks.is_empty() {
        tf!(
            "nessun leak · {}",
            "no leak · {}",
            fmt_duration(c.hours * 3600.0)
        )
    } else {
        tf!(
            "leak: {} · {}",
            "leak: {} · {}",
            leaks.join(", "),
            fmt_duration(c.hours * 3600.0)
        )
    };
    Rendered {
        html: crate::report::heapdiff_html::render(c),
        markdown: crate::heapdiff::render_text(c),
        title: format!("{} · {platform}", t("Confronto heap", "Heap comparison")),
        detail,
        bad: !leaks.is_empty() || c.restarted,
    }
}

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
        r#"<!doctype html><html lang="{}" data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">{}<title>{}</title><style>{}{}{}</style></head><body><div class="shell"><aside class="side"><a class="brand" href="{}" title="Home"><span class="logo">{}</span><span>Spark Resource Analyzer</span></a><nav><a class="on" href="{}" aria-label="{}">{}<span>{}</span></a></nav>{}<div class="sidefoot"><div class="srv"><b>v{}</b><span>GPL-3.0</span></div><button id="langbtn" class="ghost lang" data-to="{}" data-href="{}" title="Lingua / Language">{}</button><button id="theme" class="ghost" title="{}">{}</button></div></aside><main class="content">{}{}{}</main></div><script>{}{}{}{}{}</script>{}</body></html>"#,
        crate::i18n::lang().code(),
        FAVICON,
        esc(p.title),
        CSS,
        EXTRA_CSS,
        crate::report::polish::POLISH_CSS,
        esc(p.home),
        LOGO_SVG,
        esc(p.home),
        crate::i18n::t("Nuova analisi", "New analysis"),
        icon("plus"),
        crate::i18n::t("Nuova analisi", "New analysis"),
        p.side_extra,
        env!("CARGO_PKG_VERSION"),
        crate::report::html::lang_to(),
        esc(p.lang_switch),
        crate::report::html::lang_to().to_ascii_uppercase(),
        crate::i18n::t("Tema chiaro/scuro", "Light/dark theme"),
        icon("moon"),
        crate::report::html::topbar(p.crumb),
        p.main,
        crate::report::html::copyright_footer(),
        THEME_JS,
        ANIM_JS,
        messages_js(),
        COMPARE_JS,
        p.script,
        module
    )
}

pub fn spark_commands_html() -> String {
    let t = crate::i18n::t;
    let items = [
        (
            t("Profilo CPU (5 minuti)", "CPU profile (5 minutes)"),
            "/spark profiler start --timeout 300",
        ),
        (
            t("Solo i tick lenti", "Only slow ticks"),
            "/spark profiler start --only-ticks-over 100",
        ),
        (t("Memoria", "Memory"), "/spark heapsummary"),
    ];
    let mut out = format!(
        r#"<div class="cmds"><h5>{}</h5>"#,
        t("Comandi spark", "spark commands")
    );
    for (label, cmd) in items {
        out.push_str(&format!(
            r#"<div><span>{}</span><code title="{}">{}</code></div>"#,
            esc(label),
            t("Clic per copiare", "Click to copy"),
            esc(cmd)
        ));
    }
    out.push_str("</div>");
    out
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

fn compare_slot(index: usize) -> String {
    let t = crate::i18n::t;
    let (title, hint) = if index == 0 {
        (
            t("Prima", "Before"),
            t("Il dump più vecchio", "The older dump"),
        )
    } else {
        (
            t("Dopo", "After"),
            t(
                "Fatto qualche ora dopo, senza riavviare il server",
                "Taken a few hours later, without restarting the server",
            ),
        )
    };
    format!(
        r#"<div class="slot" data-slot="{index}"><h3>{title} <span class="tag">{n}</span></h3><p class="h">{hint}</p><div class="schip" hidden><span class="sok">{check}</span><div class="stxt"><b class="sname"></b><span class="sdet"></span></div><button type="button" class="ghost sclear" title="{remove}">{x}</button></div><div class="sinput"><form class="lrow slinkform"><input class="slink" autocomplete="off" placeholder="{ph}"><button class="btn sec" type="submit">{link}{load}</button></form><label class="drop sdrop"><input class="sfile" type="file" hidden><span class="dicon">{upload}</span><b>{drop}</b><span>{click}</span></label></div></div>"#,
        n = index + 1,
        check = icon("check"),
        remove = t("Rimuovi", "Remove"),
        x = icon("x"),
        ph = t(
            "Link spark.lucko.me o codice",
            "spark.lucko.me link or code"
        ),
        link = icon("link"),
        load = t("Carica", "Load"),
        upload = icon("upload"),
        drop = if index == 0 {
            t("Trascina qui il primo file", "Drop the first file here")
        } else {
            t("Trascina qui il secondo file", "Drop the second file here")
        },
        click = t("oppure clicca per sceglierlo", "or click to choose it"),
    )
}

fn compare_panel() -> String {
    let t = crate::i18n::t;
    format!(
        r#"<section class="panel" id="cmppanel"><h2><span class="hi">{cube}</span>{title}</h2><p class="note">{note}</p><div class="cmp2">{a}{b}</div><div class="cmpbar"><button class="btn" type="button" id="cmpgo" disabled>{bolt}{go}</button><span class="dim small" id="cmpmsg">{missing}</span></div><ul class="tips"><li>{tip1}</li><li>{tip2}</li><li>{tip3}</li></ul></section>"#,
        cube = icon("cube"),
        title = t("Confronta due heap summary", "Compare two heap summaries"),
        note = t(
            "Carica due /spark heapsummary dello stesso server fatti a qualche ora di distanza: il report mostra cosa cresce, a che ritmo e quale plugin ha un memory leak.",
            "Load two /spark heapsummary of the same server taken a few hours apart: the report shows what grows, how fast and which plugin has a memory leak.",
        ),
        a = compare_slot(0),
        b = compare_slot(1),
        bolt = icon("bolt"),
        go = t("Confronta", "Compare"),
        missing = t(
            "Carica i due file. L'ordine non conta: li riordino io in base alla data.",
            "Load both files. The order doesn't matter: I sort them by date.",
        ),
        tip1 = t(
            "Fai /spark heapsummary adesso e di nuovo tra 2-3 ore, con giocatori che entrano ed escono",
            "Run /spark heapsummary now and again in 2-3 hours, with players joining and leaving",
        ),
        tip2 = t(
            "Non riavviare il server tra i due dump: se l'uptime non torna, ti avviso",
            "Don't restart the server between the two dumps: if the uptime doesn't add up, I'll warn you",
        ),
        tip3 = t(
            "Puoi incollare due link oppure trascinare due file .sparkheap",
            "You can paste two links or drop two .sparkheap files",
        ),
    )
}

const HERO_WAVE: &str = r#"<svg class="wave" viewBox="0 0 600 120" preserveAspectRatio="none" aria-hidden="true"><polyline points="0,90 60,90 80,70 100,95 130,88 170,88 190,30 205,105 225,80 280,80 300,60 320,92 360,85 420,85 440,20 455,110 470,78 530,78 560,70 600,72"/></svg>"#;

pub fn index_main(recent_panel: &str) -> String {
    index_main_with(recent_panel, None)
}

pub fn index_main_with(recent_panel: &str, usage: Option<(u64, u64)>) -> String {
    let t = crate::i18n::t;
    let usage = usage.map(usage_html).unwrap_or_default();
    format!(
        r#"<section class="hcard">{wave}<div class="hmain"><div class="hrow"><span class="hlogo">{logo}</span><h1>Spark Analyzer</h1></div><p class="htag">{tagline}</p><div class="chips"><span>{chip1}</span><span>{chip2}</span><span>{chip3}</span><span>{chip4}</span></div>{usage}</div>{preview}</section>
<div class="modesel"><div class="seg big" id="modesel"><button type="button" class="on" data-mode="one">{mode_one}</button><button type="button" data-mode="cmp">{mode_cmp}</button></div><span id="modehint" hidden></span></div>
<div id="mode-one">
<section id="drop" class="hero"><span class="dicon">{upload}</span><h2>{hero_t}</h2>
<form id="lform" class="lrow"><label class="lin"><span aria-hidden="true"><b>$</b> <i>spark-analyzer</i></span><input id="link" autocomplete="off" spellcheck="false" placeholder="{ph}"></label><button class="btn" type="submit">{bolt}{go}</button></form>
<div class="or">{or}</div>
<label class="btn sec pick"><input id="file" type="file" hidden>{upload}<span>{pick}</span><span class="fmt">.sparkprofile · .sparkheap · .hprof <i title="{beta_t}">BETA</i></span></label>
<p class="hint"><span>{note1} <code>/spark profiler stop</code></span> <span>{saved} <code>--save-to-file</code> {in_dir} <code>plugins/spark/</code></span></p></section>
</div>
<div id="mode-cmp" hidden>{compare}</div>
<div id="busy" class="busy" hidden><div class="twh" aria-hidden="true"><span>spark-analyzer</span><div class="pvw"><svg viewBox="0 0 10 10"><path d="M2 5h6"/></svg><svg viewBox="0 0 10 10"><rect x="2.2" y="2.2" width="5.6" height="5.6" rx=".8"/></svg><svg viewBox="0 0 10 10"><path d="M2.4 2.4l5.2 5.2M7.6 2.4L2.4 7.6"/></svg></div></div><div class="tlog" id="tlog" aria-hidden="true"></div><span class="spin"></span><span class="pring" id="pring" hidden><svg viewBox="0 0 34 34" aria-hidden="true"><circle class="bg" cx="17" cy="17" r="14"/><circle class="fg" id="pfg" cx="17" cy="17" r="14"/></svg><b id="ppct"></b></span><span id="busytext">{busy}</span><span class="pmeta" id="pmeta" hidden></span></div>
<div id="err" class="errbox" hidden></div>
<div id="recentbox">{recent_panel}</div>
<script>(function(){{var bt=document.getElementById('busytext'),lg=document.getElementById('tlog'),b=document.getElementById('busy');if(!bt||!lg||!b||!window.MutationObserver)return;var cur=bt.textContent;function key(x){{return x.replace(/[0-9.,]+ *(%|[KMG]?B)?/g,'#')}}new MutationObserver(function(){{var n=bt.textContent;if(n===cur)return;if(!b.hidden&&cur&&key(n)!==key(cur)){{var d=document.createElement('div'),k=document.createElement('b');k.textContent='[ok]';d.appendChild(k);d.appendChild(document.createTextNode(' '+cur));lg.appendChild(d)}}cur=n}}).observe(bt,{{childList:true,characterData:true,subtree:true}});new MutationObserver(function(){{if(b.hidden)lg.textContent=''}}).observe(b,{{attributes:true,attributeFilter:['hidden']}})}})();</script>"#,
        wave = HERO_WAVE,
        preview = hero_preview(),
        logo = LOGO_SVG,
        tagline = t(
            "Analisi avanzata dei profili spark per server Minecraft: individua il plugin, la classe e il metodo che causano il lag e ricevi le correzioni consigliate.",
            "Advanced spark profile analysis for Minecraft servers: pinpoint the plugin, class and method causing lag and get recommended fixes.",
        ),
        chip1 = t("Diagnosi del lag", "Lag diagnosis"),
        chip2 = t("Lag spike", "Lag spikes"),
        chip3 = t("Memory leak", "Memory leaks"),
        chip4 = t("Ottimizzazione configurazioni", "Configuration tuning"),
        beta_t = t(
            "Gli heap dump .hprof sono in beta",
            "Heap dumps (.hprof) are in beta"
        ),
        hero_t = t(
            "Incolla il link del profilo spark o carica il file",
            "Paste a spark profile link or upload the file",
        ),
        or = t("oppure", "or"),
        pick = t("Scegli un file", "Choose a file",),
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
        saved = t("I file salvati con", "Files saved with"),
        in_dir = t("sono in", "are in"),
        busy = t("Analisi in corso…", "Analyzing…"),
        mode_one = t("Analizza un profilo", "Analyze a profile"),
        mode_cmp = t("Confronta due heap", "Compare two heaps"),
        compare = compare_panel(),
    )
}

fn hero_preview() -> String {
    let t = crate::i18n::t;
    format!(
        r##"<div class="hprev" aria-hidden="true"><div class="pvh"><i></i><span>{sample}</span><em>Paper 1.21.4 · 10m 00s</em><div class="pvw"><svg viewBox="0 0 10 10"><path d="M2 5h6"/></svg><svg viewBox="0 0 10 10"><rect x="2.2" y="2.2" width="5.6" height="5.6" rx=".8"/></svg><svg viewBox="0 0 10 10"><path d="M2.4 2.4l5.2 5.2M7.6 2.4L2.4 7.6"/></svg></div></div><div class="pvk"><div class="pvr"><svg viewBox="0 0 44 44"><circle cx="22" cy="22" r="18"/><circle class="v" cx="22" cy="22" r="18" pathLength="100" stroke-dasharray="44 100"/></svg><b>44</b></div><div><small>TPS</small><b class="c">17.8</b></div><div><small>MSPT</small><b>44.0<u>ms</u></b></div><div><small>Lag spike</small><b class="h">3</b></div></div><svg class="pvc" viewBox="0 0 420 104" preserveAspectRatio="none"><defs><linearGradient id="pvg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f97316" stop-opacity=".32"/><stop offset="1" stop-color="#f97316" stop-opacity="0"/></linearGradient></defs><path class="g" d="M0 26H420M0 52H420M0 78H420"/><path class="lim" d="M0 64H420"/><path class="ar" d="M0,80 L50,80 C70,80 75,14 90,14 C105,14 110,80 130,80 L170,80 C190,80 195,14 210,14 C225,14 230,80 250,80 L290,80 C310,80 315,14 330,14 C345,14 350,80 370,80 L420,80 V104 H0Z"/><path class="l2" d="M0,88 L50,88 C70,88 75,74 90,74 C105,74 110,88 130,88 L170,88 C190,88 195,74 210,74 C225,74 230,88 250,88 L290,88 C310,88 315,74 330,74 C345,74 350,88 370,88 L420,88"/><path class="l1" d="M0,80 L50,80 C70,80 75,14 90,14 C105,14 110,80 130,80 L170,80 C190,80 195,14 210,14 C225,14 230,80 250,80 L290,80 C310,80 315,14 330,14 C345,14 350,80 370,80 L420,80"/></svg><div class="pvf c"><span>{crit}</span><p>{f1}</p><b>5.71%</b></div><div class="pvf c"><span>{crit}</span><p>{f2}</p><b>2.54%</b></div><div class="pvf h"><span>{high}</span><p>{f3}</p><b>9.51%</b></div></div>"##,
        sample = t("Esempio di report", "Sample report"),
        crit = t("CRITICO", "CRITICAL"),
        high = t("ALTO", "HIGH"),
        f1 = t(
            "Accesso a database sincrono sul main thread",
            "Synchronous database access on the main thread"
        ),
        f2 = t(
            "Caricamento di chunk sincrono sul main thread",
            "Synchronous chunk loading on the main thread"
        ),
        f3 = t("AI dei mob (goal/brain)", "Mob AI (goals/brain)"),
    )
}

fn usage_html((people, analyses): (u64, u64)) -> String {
    if people == 0 {
        return String::new();
    }
    let t = crate::i18n::t;
    let people_label = if people == 1 {
        t("User l'ha già usato", "user has used it")
    } else {
        t("User l'hanno già usato", "users have used it")
    };
    let analyses_label = if analyses == 1 {
        t("profilo analizzato", "profile analyzed")
    } else {
        t("profili analizzati", "profiles analyzed")
    };
    format!(
        r#"<div class="usage"><div class="ust"><span class="usi" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75"/></svg></span><div><b data-count="{people}">{pv}</b><span>{people_label}</span></div></div><div class="ust"><span class="usi" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12h4l2-6 4 12 2-6h6"/></svg></span><div><b data-count="{analyses}">{av}</b><span>{analyses_label}</span></div></div></div>"#,
        pv = group_thousands(people),
        av = group_thousands(analyses),
    )
}

fn group_thousands(n: u64) -> String {
    let digits = n.to_string();
    let sep = if crate::i18n::is_en() { ',' } else { '.' };
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(c);
    }
    out
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
        "readingDump": t("Leggo l'heap dump {f} (beta)…", "Reading heap dump {f} (beta)…"),
        "dumpPacked": t("Questo heap dump è compresso in un formato che il browser non può leggere a pezzi (xz, lzma, zip…). Rifallo con /spark heapdump senza --compress (oppure con --compress gzip), o decomprimilo prima di caricarlo.", "This heap dump is compressed in a format the browser cannot read in pieces (xz, lzma, zip…). Take it again with /spark heapdump without --compress (or with --compress gzip), or decompress it before uploading."),
        "tooBig": t("File troppo grande ({s} GB) per essere letto nel browser e non è un heap dump .hprof.", "File too large ({s} GB) to be read in the browser and it is not an .hprof heap dump."),
        "proxyLimit": t("Il file è stato rifiutato dal server web davanti a Spark Resource Analyzer (HTTP {s}, limite di dimensione di nginx o Cloudflare). Aumenta client_max_body_size in nginx oppure usa il sito, che legge il file nel browser.", "The file was rejected by the web server in front of Spark Resource Analyzer (HTTP {s}, nginx or Cloudflare size limit). Raise client_max_body_size in nginx or use the website, which reads the file in the browser."),
        "badReply": t("Risposta non valida dal server (HTTP {s}): probabilmente un proxy (nginx, Cloudflare) davanti a Spark Resource Analyzer ha interrotto la richiesta.", "Invalid reply from the server (HTTP {s}): probably a proxy (nginx, Cloudflare) in front of Spark Resource Analyzer cut the request."),
        "blockedBy": t("La richiesta è stata bloccata da {w} (HTTP {s}) prima di arrivare a Spark Resource Analyzer.", "The request was blocked by {w} (HTTP {s}) before reaching Spark Resource Analyzer."),
        "viaBrowser": t("Il server web ha bloccato la richiesta: scarico il profilo dal browser…", "The web server blocked the request: downloading the profile from the browser…"),
        "lost": t("Connessione a Spark Resource Analyzer persa: ", "Lost connection to Spark Resource Analyzer: "),
        "wasm": t("Impossibile caricare il modulo di analisi: ", "Could not load the analysis module: "),
        "notFound": t("Profilo non trovato: il link potrebbe essere scaduto (spark conserva i profili per un periodo limitato).", "Profile not found: the link may have expired (spark keeps profiles for a limited time)."),
        "httpFail": t("Download fallito (HTTP {s})", "Download failed (HTTP {s})"),
        "badLink": t("Link o codice spark non valido. Esempio: https://spark.lucko.me/AbCd1234", "Invalid spark link or code. Example: https://spark.lucko.me/AbCd1234"),
        "recent": t("Analisi recenti", "Recent analyses"),
        "cmpMissing": t("Carica i due file. L'ordine non conta: li riordino io in base alla data.", "Load both files. The order doesn't matter: I sort them by date."),
        "cmpOneLeft": t("Manca il secondo file. L'ordine non conta: li riordino io in base alla data.", "The second file is missing. The order doesn't matter: I sort them by date."),
        "cmpReady": t("Pronto: premi Confronta.", "Ready: press Compare."),
        "cmpSame": t("Hai caricato due volte lo stesso dump: serve un secondo heap summary fatto più tardi.", "You loaded the same dump twice: you need a second heap summary taken later."),
        "cmpLoading": t("Leggo {f}…", "Reading {f}…"),
        "cmpRunning": t("Confronto i due heap summary…", "Comparing the two heap summaries…"),
        "open": t("Apri →", "Open →"),
    });
    format!("var M={m};")
}

pub fn local_index(recent: &[Recent]) -> String {
    local_index_with(recent, "")
}

pub fn local_index_with(recent: &[Recent], wasm_query: &str) -> String {
    local_index_full(recent, wasm_query, None)
}

pub fn local_index_full(recent: &[Recent], wasm_query: &str, usage: Option<(u64, u64)>) -> String {
    local_index_public(recent, wasm_query, usage, None)
}

pub fn local_index_public(
    recent: &[Recent],
    wasm_query: &str,
    usage: Option<(u64, u64)>,
    public_base: Option<&str>,
) -> String {
    let t = crate::i18n::t;
    let script = format!(
        "var SA_WV={};{LOCAL_JS}",
        serde_json::Value::from(wasm_query)
    );
    let (side, panel) = recent_html(recent);
    let main = news_banner() + &index_main_with(&panel, usage);
    let switch = format!("/?lang={}", crate::report::html::lang_to());
    let page = shell(&Page {
        crumb: t("Nuova analisi", "New analysis"),
        lang_switch: &switch,
        title: home_title(),
        side_extra: &side,
        main: &main,
        script: &script,
        module_script: "",
        home: if crate::i18n::is_en() {
            "/"
        } else {
            "/?lang=it"
        },
        footer: t("in esecuzione sul tuo computer", "running on your computer"),
    });
    match public_base {
        Some(base) => page.replacen(data_uri_icon(), "", 1).replacen(
            "</head>",
            &(seo_head(base) + "</head>"),
            1,
        ),
        None => page,
    }
}

pub fn local_not_found() -> String {
    let t = crate::i18n::t;
    let main = format!(
        r#"<div class="top"><div><h1>{}</h1><p class="sub">{} <a href="/">{}</a>.</p></div></div>"#,
        t("Report non trovato", "Report not found"),
        t(
            "Il link è sbagliato oppure il report è stato eliminato: i report vengono cancellati dopo 30 giorni.",
            "The link is wrong or the report was deleted: reports are removed after 30 days."
        ),
        t("Torna alla pagina iniziale", "Back to the home page")
    );
    shell(&Page {
        crumb: t("Report non trovato", "Report not found"),
        lang_switch: "/",
        title: t(
            "Spark Resource Analyzer — non trovato",
            "Spark Resource Analyzer — not found",
        ),
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
        crate::i18n::Lang::It => "it.html",
        crate::i18n::Lang::En => "index.html",
    }
}

pub fn static_index() -> String {
    static_index_versioned("")
}

pub fn static_index_versioned(version: &str) -> String {
    static_index_page().replace("__SA_V__", &version_suffix(version))
}

pub fn home_title() -> &'static str {
    crate::i18n::t(
        "Spark Analyzer · Analisi dei profili spark per server Minecraft",
        "Spark Analyzer · Spark profile analysis for Minecraft servers",
    )
}

pub fn home_description() -> &'static str {
    crate::i18n::t(
        "Analizza i profili spark del tuo server Minecraft: trova il plugin, la classe e il metodo che causano il lag, i lag spike e i memory leak, con le correzioni consigliate. Gratis e open source.",
        "Analyze your Minecraft server's spark profiles: find the plugin, class and method causing lag, lag spikes and memory leaks, with recommended fixes. Free and open source.",
    )
}

pub fn seo_head(base: &str) -> String {
    let it = !crate::i18n::is_en();
    let canonical = if it {
        format!("{base}?lang=it")
    } else {
        base.to_string()
    };
    let title = home_title();
    let d = esc(home_description());
    format!(
        r##"<meta name="description" content="{d}"><link rel="canonical" href="{canonical}"><link rel="alternate" hreflang="en" href="{base}"><link rel="alternate" hreflang="it" href="{base}?lang=it"><link rel="alternate" hreflang="x-default" href="{base}"><link rel="icon" href="/favicon.svg" type="image/svg+xml"><link rel="apple-touch-icon" href="/apple-touch-icon.png"><meta property="og:type" content="website"><meta property="og:site_name" content="Spark Analyzer"><meta property="og:title" content="{t}"><meta property="og:description" content="{d}"><meta property="og:url" content="{canonical}"><meta property="og:image" content="{base}og-image.png"><meta property="og:image:width" content="1200"><meta property="og:image:height" content="630"><meta property="og:locale" content="{loc}"><meta name="twitter:card" content="summary_large_image"><meta name="twitter:title" content="{t}"><meta name="twitter:description" content="{d}"><meta name="twitter:image" content="{base}og-image.png"><script type="application/ld+json">{{"@context":"https://schema.org","@type":"WebApplication","name":"Spark Analyzer","alternateName":["Spark Resource Analyzer","SparkAnalyzer"],"url":"{base}","description":"{d}","applicationCategory":"DeveloperApplication","operatingSystem":"Web","inLanguage":["it","en"],"offers":{{"@type":"Offer","price":"0","priceCurrency":"EUR"}},"isAccessibleForFree":true}}</script>"##,
        t = esc(title),
        loc = if it { "it_IT" } else { "en_US" },
    )
}

pub const SITE_DESCRIPTION: &str = "Analyze spark profiles: find what makes your Minecraft server lag, which plugin and method, and how to fix it.";

const SITE_ICONS: &str = r#"<link rel="icon" href="./favicon.ico" sizes="16x16 32x32 48x48"><link rel="icon" href="./favicon.svg" type="image/svg+xml"><link rel="icon" href="./favicon-96.png" type="image/png" sizes="96x96"><link rel="icon" href="./favicon-192.png" type="image/png" sizes="192x192"><link rel="apple-touch-icon" href="./apple-touch-icon.png">"#;

fn data_uri_icon() -> &'static str {
    let start = FAVICON.find("<link rel=\"icon\"").unwrap_or(FAVICON.len());
    let end = FAVICON[start..].find('>').map_or(start, |e| start + e + 1);
    &FAVICON[start..end]
}

fn social_meta() -> String {
    let site = crate::report::html::PUBLIC_SITE;
    let title = "Spark Resource Analyzer";
    let description = SITE_DESCRIPTION;
    format!(
        r##"<meta name="description" content="{d}"><link rel="canonical" href="{canonical}"><link rel="alternate" hreflang="en" href="{main_site}"><link rel="alternate" hreflang="it" href="{main_site}?lang=it"><meta name="theme-color" content="#0b0b0c"><meta property="og:type" content="website"><meta property="og:site_name" content="{title}"><meta property="og:title" content="{title}"><meta property="og:description" content="{d}"><meta property="og:url" content="{site}"><meta property="og:image" content="{site}og-image.png"><meta property="og:image:width" content="1200"><meta property="og:image:height" content="630"><meta name="twitter:card" content="summary_large_image"><meta name="twitter:title" content="{title}"><meta name="twitter:description" content="{d}"><meta name="twitter:image" content="{site}og-image.png"><script type="application/ld+json">{{"@context":"https://schema.org","@type":"WebSite","name":"{title}","alternateName":"Spark Analyzer","url":"{site}","description":"{d}"}}</script>"##,
        d = esc(description),
        main_site = MAIN_SITE,
        canonical = if crate::i18n::is_en() {
            MAIN_SITE.to_string()
        } else {
            format!("{MAIN_SITE}?lang=it")
        },
    )
}

pub const MAIN_SITE: &str = "https://sparkanalyzer.me/";

pub const SITE_NEWS: &str =
    "Beta: open full heap dumps (.hprof), even several GB, right in your browser";
pub const SITE_NEWS_URL: &str =
    "https://github.com/OnlyDehBest/Spark-Analyzer/blob/main/CHANGELOG.md";

fn news_banner() -> String {
    let key: u32 = SITE_NEWS.bytes().fold(2_166_136_261u32, |h, b| {
        (h ^ b as u32).wrapping_mul(16_777_619)
    });
    format!(
        r#"<div class="news" id="news" data-k="{key:08x}" hidden><span class="nb">New</span><span class="nt">{text}</span><a href="{url}" target="_blank" rel="noopener">Details →</a><button type="button" id="newsx" aria-label="Close">×</button></div><script>(function(){{var n=document.getElementById('news'),k='sa-news';try{{if(sessionStorage.getItem(k)===n.dataset.k)return}}catch(e){{}}n.hidden=false;document.getElementById('newsx').onclick=function(){{n.remove();try{{sessionStorage.setItem(k,n.dataset.k)}}catch(e){{}}}}}})()</script>"#,
        text = esc(SITE_NEWS),
        url = esc(SITE_NEWS_URL),
    )
}

fn static_index_page() -> String {
    let t = crate::i18n::t;
    let main = news_banner() + &index_main("");
    let other = if crate::i18n::is_en() {
        "./it.html"
    } else {
        "./"
    };
    let home = if crate::i18n::is_en() {
        "./"
    } else {
        "./it.html"
    };
    let page = shell(&Page {
        crumb: t("Nuova analisi", "New analysis"),
        lang_switch: other,
        title: home_title(),
        side_extra: r#"<div id="recentside"></div>"#,
        main: &main,
        script: "",
        module_script: STATIC_JS,
        home,
        footer: t(
            "analisi eseguita nel tuo browser",
            "analysis runs in your browser",
        ),
    });
    page.replacen(data_uri_icon(), SITE_ICONS, 1).replacen(
        "</head>",
        &format!(
            r#"{}<link rel="modulepreload" href="./pkg/sparkscope_web.js__SA_V__"><link rel="preload" href="./pkg/sparkscope_web_bg.wasm__SA_V__" as="fetch" type="application/wasm" crossorigin></head>"#,
            social_meta()
        ),
        1,
    )
}

pub const EXTRA_CSS: &str = r#"
.pick .fmt{display:flex;align-items:center;gap:8px;color:var(--muted);font:500 12px "JetBrains Mono",ui-monospace,monospace;border-left:1px solid var(--line);padding-left:12px;margin-left:4px;white-space:nowrap}.pick .fmt i{font:700 9.5px Inter,system-ui,sans-serif;font-style:normal;letter-spacing:.06em;color:var(--med);border:1px solid rgba(234,179,8,.5);border-radius:4px;padding:2px 5px}
@media(max-width:700px){.pick{flex-wrap:wrap;justify-content:center;row-gap:6px}.pick .fmt{flex-basis:100%;justify-content:center;border-left:0;padding-left:0;margin-left:0;font-size:11px}}
.pring{position:relative;width:44px;height:44px;flex:none}.pring[hidden],.pmeta[hidden],.busy .spin[hidden]{display:none}.pring svg{width:44px;height:44px;transform:rotate(-90deg)}.pring circle{fill:none;stroke-width:3.2}.pring .bg{stroke:var(--line)}.pring .fg{stroke:var(--acc);stroke-linecap:round;stroke-dasharray:88;stroke-dashoffset:88;transition:stroke-dashoffset .15s linear}.pring.ind .fg{stroke-dashoffset:66;transition:none}.pring.ind svg{animation:sa-ring .9s linear infinite}@keyframes sa-ring{from{transform:rotate(-90deg)}to{transform:rotate(270deg)}}.pring b{position:absolute;inset:0;display:grid;place-items:center;font-size:11px;font-weight:700;line-height:1;color:var(--hd)}.pmeta{margin-left:auto;color:var(--muted);font-size:12px;font-variant-numeric:tabular-nums;white-space:nowrap}
.hcard{position:relative;overflow:hidden;border:1px solid var(--line);border-radius:16px;padding:32px 30px 28px;margin:4px 0 22px;background:radial-gradient(120% 140% at 0% 0%,rgba(163,230,53,.10),transparent 55%),radial-gradient(90% 120% at 100% 100%,rgba(56,189,248,.10),transparent 60%),var(--card);animation:sa-fade .4s ease both}
.hcard .wave{position:absolute;right:-10px;top:18px;width:46%;height:90px;opacity:.5;pointer-events:none}.hcard .wave polyline{fill:none;stroke:var(--ok);stroke-width:3;stroke-linejoin:round;stroke-linecap:round;vector-effect:non-scaling-stroke}
.hrow{position:relative;display:flex;align-items:center;gap:16px}.hlogo{width:58px;height:58px;flex:none}.hlogo svg{width:100%;height:100%}
.hcard h1{margin:0;font-size:42px;letter-spacing:-.03em;line-height:1.05;color:var(--hd)}
.htag{position:relative;margin:14px 0 0;color:var(--fg);font-size:16px;max-width:640px}
.chips{position:relative;display:flex;gap:8px;flex-wrap:wrap;margin-top:16px}.chips span{font-size:12px;border:1px solid var(--line);border-radius:999px;padding:5px 10px;color:var(--muted);background:var(--card2)}
@media(max-width:700px){.hcard{padding:22px 18px}.hcard h1{font-size:30px}.hlogo{width:44px;height:44px}.hcard .wave{width:100%;right:0;top:auto;bottom:6px;height:56px;opacity:.18}.htag{font-size:15px}}
.news{display:flex;align-items:center;gap:10px;margin:0 0 18px;padding:9px 10px 9px 14px;border:1px solid var(--line);border-radius:999px;background:var(--card);font-size:13px;color:var(--fg);width:fit-content;max-width:100%;box-sizing:border-box;min-width:0}.news[hidden]{display:none}.news .nb{font-weight:600;font-size:11px;line-height:1;text-transform:uppercase;letter-spacing:.05em;color:var(--ok);background:rgba(163,230,53,.12);padding:5px 8px;border-radius:999px;flex:none}.news .nt{flex:1 1 auto;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.news a{color:var(--muted);white-space:nowrap}.news a:hover{color:var(--hd);text-decoration:none}.news button{display:grid;place-items:center;padding:0;flex:none;width:24px;height:24px;border-radius:50%;border:0;background:transparent;color:var(--muted);cursor:pointer;font-size:16px;line-height:1}.news button:hover{background:var(--card3);color:var(--hd)}
@media(max-width:700px){.news{width:100%;border-radius:12px;flex-wrap:wrap}.news .nt{white-space:normal;flex-basis:calc(100% - 110px)}.news a{order:3;margin-left:auto}}
.lrow{display:flex;gap:8px;flex-wrap:wrap}.lrow input{flex:1;min-width:220px;padding:10px 12px;border:1px solid var(--line);border-radius:5px;background:var(--code);color:var(--fg);font:inherit;outline:none}.lrow input:focus{border-color:var(--acc)}
.drop{display:flex;flex-direction:column;align-items:center;justify-content:center;gap:4px;text-align:center;border:2px dashed var(--line);border-radius:8px;padding:34px 16px;cursor:pointer;background:var(--card2);transition:border-color .15s,background .15s}.drop:hover,.drop.over{border-color:var(--acc);background:var(--accl)}.drop b{color:var(--hd);font-size:15px}.drop span{color:var(--muted)}.drop small{color:var(--muted);margin-top:8px}.dicon{display:grid;place-items:center;width:48px;height:48px;border-radius:8px;background:var(--acc);color:#fff!important;margin-bottom:6px}.dicon svg{width:24px;height:24px}
.busy{display:flex;align-items:center;gap:12px;background:var(--card);border:1px solid var(--line);border-left:3px solid var(--acc);border-radius:8px;padding:14px 16px;margin-bottom:14px;color:var(--hd)}.busy[hidden],.errbox[hidden]{display:none}.spin{width:18px;height:18px;border:2px solid var(--line);border-top-color:var(--acc);border-radius:50%;animation:sp 0.8s linear infinite}@keyframes sp{to{transform:rotate(360deg)}}
.errbox{background:rgba(250,82,82,.1);border:1px solid var(--crit);border-radius:8px;padding:12px 16px;margin-bottom:14px;color:var(--crit);white-space:pre-wrap}
.rlist{display:flex;flex-direction:column;gap:8px}.ritem{display:flex;align-items:center;justify-content:space-between;gap:12px;background:var(--card2);border:1px solid var(--line);border-left:3px solid var(--ok);border-radius:6px;padding:10px 14px;color:var(--fg)}.ritem.bad{border-left-color:var(--crit)}.ritem:hover{text-decoration:none;border-color:var(--muted)}.ritem div{display:flex;flex-direction:column}.ritem b{color:var(--hd)}.ritem div span{color:var(--muted);font-size:12px}.go{color:var(--acc);font-size:13px;white-space:nowrap}
.recent{margin-top:10px;padding-top:10px;border-top:1px solid var(--line2);overflow:auto}.recent h5{margin:0 8px 6px}.recent a{display:flex;align-items:center;gap:8px;padding:6px 8px;border-radius:5px;color:var(--fg);font-size:12px}.recent a:hover{background:var(--card2);text-decoration:none}.recent a span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.recent i{width:8px;height:8px;border-radius:50%;flex:none;background:var(--ok)}.recent i.bad{background:var(--crit)}
.ghost.lang{font-size:11px;font-weight:700;letter-spacing:.04em;color:var(--muted)}
pre.heap{margin:0;background:var(--code);border:1px solid var(--line);border-radius:6px;padding:12px;overflow:auto;font:12px ui-monospace,Menlo,Consolas,monospace}
@media (max-width:900px){.recent,.cmds{display:none}}
.hero{border:2px dashed var(--line);border-radius:16px;background:var(--card2);padding:34px 28px;text-align:center;transition:border-color .15s,background .15s;margin:0 0 14px}.hero:hover,.hero.over{border-color:var(--acc)}.hero.over{background:var(--accl)}.hero .dicon{margin:0 auto 10px}.hero h2{justify-content:center;font-size:20px;margin:0 0 6px}.hero>p{color:var(--muted);margin:0 0 18px}.hero .lrow{max-width:720px;margin:0 auto}.hero .or{display:flex;align-items:center;gap:12px;max-width:720px;margin:16px auto;color:var(--muted);font-size:12px}.hero .or:before,.hero .or:after{content:'';flex:1;height:1px;background:var(--line)}.hero .pick{cursor:pointer}.hero .pick svg{width:18px;height:18px}.hero .hint{margin:18px auto 0;max-width:720px;color:var(--muted);font-size:12px;line-height:1.9}.hero .hint code{white-space:nowrap}@media(min-width:901px){#recentbox{display:none}}
.cmds{margin-top:10px;padding-top:10px;border-top:1px solid var(--line2)}.cmds h5{margin:0 8px 6px}.cmds div{padding:5px 8px;font-size:12px;color:var(--fg)}.cmds code{display:block;white-space:normal;word-break:normal;overflow-wrap:normal;font:11px "JetBrains Mono",monospace;color:var(--hd);background:var(--code);border:1px solid var(--line);border-radius:6px;padding:4px 6px;margin-top:3px;cursor:copy}.cmds code.ok{border-color:var(--ok);color:var(--ok)}
.modesel{display:flex;align-items:center;gap:14px;flex-wrap:wrap;margin:0 0 18px}.seg.big{border-radius:12px}.seg.big button{padding:10px 18px;font-size:13px;font-family:Inter,system-ui,sans-serif;cursor:pointer}.seg.big button.on{background:var(--card3);color:var(--hd)}
.cmp2{display:grid;grid-template-columns:1fr 1fr;gap:16px}@media(max-width:800px){.cmp2{grid-template-columns:1fr}}.slot{border:1px solid var(--line);border-radius:14px;padding:16px;background:var(--card2)}.slot h3{margin:0 0 4px;font-size:15px;color:var(--hd);display:flex;gap:8px;align-items:center}.slot .tag{font:600 11px "JetBrains Mono",monospace;border:1px solid var(--line);border-radius:999px;padding:2px 8px;color:var(--muted)}.slot p.h{margin:0 0 12px;color:var(--muted);font-size:13px}.slot .lrow{margin-bottom:10px}.slot .drop{min-height:150px;padding:20px 12px}.slot.busy-slot{opacity:.6;pointer-events:none}
.schip{display:flex;gap:12px;align-items:center;border:1px solid var(--ok);background:rgba(163,230,53,.06);border-radius:12px;padding:12px 14px}.schip[hidden],.sinput[hidden]{display:none}.schip .sok{color:var(--ok);flex:none;display:grid}.schip .stxt{flex:1;min-width:0}.schip .sname{display:block;color:var(--hd);font-size:14px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.schip .sdet{display:block;color:var(--muted);font-size:12px}.schip .sclear{flex:none}
.cmpbar{display:flex;align-items:center;gap:14px;flex-wrap:wrap;margin-top:16px}.btn[disabled]{opacity:.45;cursor:not-allowed;pointer-events:none}.tips{margin:14px 0 0;padding-left:18px;color:var(--muted);font-size:13px}.tips li{margin:4px 0}
"#;

pub const COMPARE_JS: &str = r#"
window.saSetupCompare=function(api){
var one=document.getElementById('mode-one'),cmp=document.getElementById('mode-cmp'),hint=document.getElementById('modehint'),btns=document.querySelectorAll('#modesel button');
var go=document.getElementById('cmpgo'),msg=document.getElementById('cmpmsg'),err=document.getElementById('err');
var slots=[null,null];
function setMode(m){var c=m==='cmp';one.hidden=c;cmp.hidden=!c;if(hint)hint.hidden=c;btns.forEach(function(b){b.classList.toggle('on',b.dataset.mode===m)});window.__saMode=m;try{history.replaceState(null,'',c?'#confronta':location.pathname+location.search)}catch(e){}}
btns.forEach(function(b){b.addEventListener('click',function(){setMode(b.dataset.mode)})});
function refresh(){var n=(slots[0]?1:0)+(slots[1]?1:0);var same=n===2&&slots[0].detail===slots[1].detail&&slots[0].size===slots[1].size;go.disabled=n<2||same;msg.textContent=same?M.cmpSame:n===2?M.cmpReady:n===1?M.cmpOneLeft:M.cmpMissing}
function show(i){var el=document.querySelector('.slot[data-slot="'+i+'"]'),s=slots[i],chip=el.querySelector('.schip'),inp=el.querySelector('.sinput');chip.hidden=!s;inp.hidden=!!s;if(s){chip.querySelector('.sname').textContent=s.name;chip.querySelector('.sdet').textContent=s.detail}refresh()}
function fail(e){err.textContent=(e&&e.message)||String(e);err.hidden=false}
function accept(i,p){var el=document.querySelector('.slot[data-slot="'+i+'"]'),d=document.documentElement;err.hidden=true;el.classList.add('busy-slot');d.classList.add('sa-busy');return p.then(function(s){slots[i]=s;show(i)}).catch(fail).then(function(){el.classList.remove('busy-slot');d.classList.remove('sa-busy')})}
document.querySelectorAll('.slot').forEach(function(el){var i=+el.dataset.slot,file=el.querySelector('.sfile'),drop=el.querySelector('.sdrop');
el.querySelector('.slinkform').addEventListener('submit',function(e){e.preventDefault();var v=el.querySelector('.slink').value.trim();if(!v){fail(M.pasteFirst);return}accept(i,api.fromLink(v))});
file.addEventListener('change',function(){if(file.files[0])accept(i,api.fromFile(file.files[0]));file.value=''});
['dragenter','dragover'].forEach(function(t){drop.addEventListener(t,function(e){e.preventDefault();e.stopPropagation();drop.classList.add('over')})});
['dragleave','drop'].forEach(function(t){drop.addEventListener(t,function(e){e.preventDefault();drop.classList.remove('over')})});
drop.addEventListener('drop',function(e){e.stopPropagation();var f=e.dataTransfer.files;if(!f.length)return;accept(i,api.fromFile(f[0]));if(f.length>1&&!slots[1-i])accept(1-i,api.fromFile(f[1]))});
el.querySelector('.sclear').addEventListener('click',function(){slots[i]=null;show(i)})});
cmp.addEventListener('drop',function(e){e.preventDefault();e.stopPropagation();var f=e.dataTransfer.files;for(var k=0,j=0;k<f.length&&j<2;j++){if(!slots[j]){accept(j,api.fromFile(f[k]));k++}}});
go.addEventListener('click',function(){if(go.disabled)return;err.hidden=true;api.run(slots[0],slots[1]).catch(fail)});
window.__saCmpLoad=function(i,p){setMode('cmp');return accept(i,p)};
if(location.hash==='#confronta')setMode('cmp');
refresh();
};
"#;

pub const THEME_JS: &str = r#"
document.querySelectorAll('.cmds code').forEach(function(c){c.addEventListener('click',function(){var t=c.textContent;function ok(){c.classList.add('ok');setTimeout(function(){c.classList.remove('ok')},1200)}if(navigator.clipboard&&window.isSecureContext)navigator.clipboard.writeText(t).then(ok,function(){});else{var a=document.createElement('textarea');a.value=t;document.body.appendChild(a);a.select();try{document.execCommand('copy');ok()}catch(e){}a.remove()}})});
(function(){var d=document.documentElement;try{var t=localStorage.getItem('sparkscope-theme');if(t)d.dataset.theme=t}catch(e){}var b=document.getElementById('theme');if(b)b.addEventListener('click',function(){var n=d.dataset.theme==='dark'?'light':'dark';d.dataset.theme=n;try{localStorage.setItem('sparkscope-theme',n)}catch(e){}})})();
"#;

pub const LOCAL_JS: &str = r#"
(function(){var els=document.querySelectorAll('.usage [data-count]');if(!els.length||!window.requestAnimationFrame||(window.matchMedia&&matchMedia('(prefers-reduced-motion: reduce)').matches))return;var sep=document.documentElement.lang==='en'?',':'.';function fmt(n){return String(n).replace(/\B(?=(\d{3})+(?!\d))/g,sep)}els.forEach(function(el){var to=+el.dataset.count,t0=null;if(!(to>0))return;el.textContent='0';function step(t){if(t0===null)t0=t;var k=Math.min(1,(t-t0)/1100),e=1-Math.pow(1-k,3);el.textContent=fmt(Math.round(to*e));if(k<1)requestAnimationFrame(step)}requestAnimationFrame(step)})})();
(function(){
var LQ=M.lang==='it'?'lang=it':'';
try{var pref=localStorage.getItem('sa-lang');if(pref&&pref!==M.lang&&!/[?&]lang=/.test(location.search)){location.replace('/?lang='+pref+location.hash);return}}catch(e){}
var lb=document.getElementById('langbtn');if(lb)lb.addEventListener('click',function(){try{localStorage.setItem('sa-lang',lb.dataset.to)}catch(e){}location.href=lb.dataset.href+(window.__saMode==='cmp'?'#confronta':'')});
var busy=document.getElementById('busy'),bt=document.getElementById('busytext'),err=document.getElementById('err');
function start(t){err.hidden=true;bt.textContent=t;busy.hidden=false;document.documentElement.classList.add('sa-busy')}
function fail(m){busy.hidden=true;err.textContent=m;err.hidden=false;document.documentElement.classList.remove('sa-busy')}
function who(r){var s=(r.headers.get('server')||'').toLowerCase();if(r.headers.get('cf-ray')||s.indexOf('cloudflare')>=0)return 'Cloudflare';if(s.indexOf('nginx')>=0)return 'nginx';if(s.indexOf('caddy')>=0)return 'Caddy';return ''}
function replyError(r){if(r.status===413)return M.proxyLimit.replace('{s}',r.status);var w=who(r);return w?M.blockedBy.replace('{w}',w).replace('{s}',r.status):M.badReply.replace('{s}',r.status)}
function body(r){return r.text().then(function(t){try{return JSON.parse(t)}catch(e){var x=new Error(replyError(r));x.proxy=true;throw x}})}
function sparkKey(v){v=(v||'').trim();var m=v.match(/^(?:https?:\/\/)?[^\/\s]+\/([A-Za-z0-9]+)/);if(m)return m[1];return /^[A-Za-z0-9]{4,64}$/.test(v)?v:null}
function browserFile(k){return fetch('https://spark-usercontent.lucko.me/'+encodeURIComponent(k)).then(function(r){if(r.status===404)throw new Error(M.notFound);if(!r.ok)throw new Error(M.httpFail.replace('{s}',r.status));return r.blob()}).then(function(b){return new File([b],k)})}
function done(r){return body(r).then(function(j){if(j.id){location.href='/r/'+j.id+(LQ?'?'+LQ:'')}else{fail(j.error||M.unknown)}})}
var PR={ring:document.getElementById('pring'),fg:document.getElementById('pfg'),pct:document.getElementById('ppct'),meta:document.getElementById('pmeta'),spin:busy.querySelector('.spin')};
function mb(n){var s=(n/1048576).toFixed(1);return M.lang==='it'?s.replace('.',','):s}
function prog(got,total,rate){if(!PR.ring)return;PR.spin.hidden=true;PR.ring.hidden=false;PR.meta.hidden=false;var p=Math.min(100,Math.floor(got/Math.max(1,total)*100));PR.fg.style.strokeDashoffset=String(88-88*p/100);PR.pct.textContent=p+'%';PR.meta.textContent=mb(got)+' / '+mb(total)+' MB'+(rate?' · '+mb(rate)+' MB/s':'')}
function progOff(){if(!PR.ring)return;PR.ring.hidden=true;PR.meta.hidden=true;PR.spin.hidden=false}
function json(u,b){return fetch(u,{method:'POST',body:b}).then(function(r){if(r.status===413)return {tooBig:true};return body(r)})}
function direct(f,to,x){return fetch((to==='heap'?'/api/heap/file':'/api/file')+'?name='+encodeURIComponent(f.name)+(x||'')+'&'+LQ,{method:'POST',body:f})}
function upload(f,to,kx){kx=kx||'';if(f.size<=786432)return direct(f,to,kx);return dumpHead(f).then(function(d){if(!d||!dumpWorker())return chunked(f,to,kx);return readDumpHere(f).then(function(b){return packSummary(b,f.name)}).then(function(s){var sx='&summary=1';return (s.size<=786432?direct(s,to,sx).then(function(r){return r.status===413?chunked(s,to,sx):r}):chunked(s,to,sx)).then(function(r){return r.ok?r:chunked(f,to)},function(){return chunked(f,to)})},function(){return chunked(f,to)})})}
function packSummary(b,name){var blob=new Blob([b]);if(typeof CompressionStream!=='function')return Promise.resolve(new File([blob],name));return new Response(blob.stream().pipeThrough(new CompressionStream('gzip'))).blob().then(function(z){return new File([z],name)})}
function dumpHead(f){return f.slice(0,12).arrayBuffer().then(function(b){var u=new Uint8Array(b);return String.fromCharCode.apply(null,u)==='JAVA PROFILE'||(u[0]===0x1f&&u[1]===0x8b&&/hprof/i.test(f.name))})}
var DW=null,DN=0,DQ={};
function dumpWorker(){if(DW===null){DW=false;if(SA_WV&&typeof Worker==='function'){try{DW=new Worker('/sa-worker.js'+SA_WV,{type:'module'});DW.onmessage=onDump;DW.onerror=dumpLost}catch(e){DW=false}}}return DW}
function onDump(e){var d=e.data,q=DQ[d.id];if(!q)return;if(d.progress!=null){q.p(d.progress);return}delete DQ[d.id];if(d.err!=null){if(d.fatal){try{DW.terminate()}catch(x){}DW=null}q.ko(new Error(d.err))}else q.ok(d.ok)}
function dumpLost(){var q=DQ;DQ={};try{DW.terminate()}catch(x){}DW=false;for(var k in q)q[k].ko(new Error('worker'))}
function readDumpHere(f){var t0=Date.now(),was=bt.textContent;bt.textContent=M.readingDump.replace('{f}',f.name);prog(0,f.size);return new Promise(function(ok,ko){var id=++DN;DQ[id]={ok:ok,ko:ko,p:function(n){prog(n,f.size,n/Math.max(.001,(Date.now()-t0)/1000))}};DW.postMessage({id:id,fn:'hprof',args:[f]})}).then(function(b){progOff();bt.textContent=M.analyzing;return b},function(e){progOff();bt.textContent=was;throw e})}
var MINCH=262144;function chGet(){try{var v=+localStorage.getItem('sa-chunk');if(v>=MINCH&&v<=8388608)return v}catch(e){}return 8388608}function chSet(v){try{localStorage.setItem('sa-chunk',String(v))}catch(e){}}
async function chunked(f,to,x){x=x||'';var h=new Uint8Array(await f.slice(0,12).arrayBuffer()),dump=!x&&String.fromCharCode.apply(null,h)==='JAVA PROFILE',gz=dump&&typeof CompressionStream==='function',CAP=chGet(),CH=Math.min(CAP,4194304),sent=0,t0=Date.now();
var j=await json('/api/upload/start?name='+encodeURIComponent(f.name)+(dump?'&dump=1':'')+x+'&'+LQ,'');if(j.error||!j.upload)throw new Error(j.error||M.unknown);var id=j.upload;prog(0,f.size);
function piece(a,b){var s=f.slice(a,b);return gz?new Response(s.stream().pipeThrough(new CompressionStream('gzip'))).blob():Promise.resolve(s)}
function wait(ms){return new Promise(function(r){setTimeout(r,ms)})}
async function put(a,b){for(var k=0;;k++){var t=Date.now(),r=null,x;try{var p=await piece(a,b);r=await fetch('/api/upload/chunk?id='+id+'&off='+a+'&end='+b+'&len='+p.size+'&'+LQ,{method:'POST',body:p})}catch(e){x=e}
if(r&&r.status===413){if(b-a<=MINCH)throw new Error(M.proxyLimit.replace('{s}',413));var n=Math.max(MINCH,Math.floor((b-a)/4));CAP=Math.min(CAP,n);CH=Math.min(CH,n);chSet(CAP);for(var y=a;y<b;y+=n)await put(y,Math.min(b,y+n));return}
if(r&&r.status!==408&&r.status!==429&&r.status<500){var j=await body(r);if(j.error)throw new Error(j.error);var dt=(Date.now()-t)/1000;if(dt>12)CH=Math.max(MINCH,Math.floor(CH/2));else if(dt<3&&CH*2<=CAP)CH*=2;return}
if(k>=5)throw r?new Error(M.badReply.replace('{s}',r.status)):x;CH=Math.max(MINCH,Math.floor(CH/2));await wait(1000*Math.pow(2,k))}}
function moved(n){sent+=n;prog(sent,f.size,sent/Math.max(.001,(Date.now()-t0)/1000))}
var off=Math.min(CH,f.size);await put(0,off);moved(off);var stop=false;
async function lane(){while(!stop&&off<f.size){var a=off,b=Math.min(f.size,a+CH);off=b;try{await put(a,b)}catch(e){stop=true;throw e}moved(b-a)}}
await Promise.all([lane(),lane(),lane()]);progOff();bt.textContent=M.analyzing;return fetch('/api/upload/finish?id='+id+'&to='+(to||'file')+'&size='+f.size+(x&&x.indexOf('&key=')>=0?x.slice(x.indexOf('&key=')):'')+'&'+LQ,{method:'POST'})}
document.getElementById('lform').addEventListener('submit',function(e){e.preventDefault();var v=document.getElementById('link').value.trim();if(!v){fail(M.pasteFirst);return}var k=sparkKey(v);if(!k){fail(M.badLink);return}start(M.downloading);
fetch('/api/link?'+LQ,{method:'POST',body:k}).then(done).catch(function(x){if(!(x&&x.proxy)){fail(M.lost+x);return}bt.textContent=M.viaBrowser;browserFile(k).then(function(f){bt.textContent=M.analyzing;return upload(f,'file','&key='+k)}).then(done).catch(function(y){progOff();fail(y&&y.message?y.message:M.lost+y)})})});
function send(f){if(!f)return;start(M.analyzingFile.replace('{f}',f.name).replace('{s}',(f.size/1048576).toFixed(1)));upload(f,'file').then(done).catch(function(x){progOff();fail(x&&x.message&&!/fetch|network|load failed/i.test(x.message)?x.message:M.lost+x)})}
var drop=document.getElementById('drop'),file=document.getElementById('file');
file.addEventListener('change',function(){send(file.files[0])});
['dragenter','dragover'].forEach(function(t){drop.addEventListener(t,function(e){e.preventDefault();drop.classList.add('over')})});
['dragleave','drop'].forEach(function(t){drop.addEventListener(t,function(e){e.preventDefault();drop.classList.remove('over')})});
drop.addEventListener('drop',function(e){send(e.dataTransfer.files[0])});
document.addEventListener('dragover',function(e){e.preventDefault()});document.addEventListener('drop',function(e){e.preventDefault();if(window.__saMode==='cmp')return;if(e.target.closest&&!e.target.closest('#drop'))send(e.dataTransfer.files[0])});
document.getElementById('link').addEventListener('paste',function(){var i=this;setTimeout(function(){if(/spark\.lucko\.me\/|^[A-Za-z0-9]{6,}$/.test(i.value.trim()))document.getElementById('lform').requestSubmit()},50)});
function staged(r){return body(r).then(function(j){if(j.error)throw new Error(j.error);return j})}
window.saSetupCompare({
fromFile:function(f){return upload(f,'heap').then(staged)},
fromLink:function(v){var k=sparkKey(v);if(!k)return Promise.reject(new Error(M.badLink));return fetch('/api/heap/link?'+LQ,{method:'POST',body:k}).then(staged).catch(function(x){if(!(x&&x.proxy))throw x;return browserFile(k).then(function(f){return upload(f,'heap','&key='+k)}).then(staged)})},
run:function(a,b){start(M.cmpRunning);return fetch('/api/heap/compare?a='+encodeURIComponent(a.stage)+'&b='+encodeURIComponent(b.stage)+'&'+LQ,{method:'POST'}).then(done)}
});
})();
"#;

const WORKER_JS: &str = r#"import init, { analyze, describe_heap, compare_heaps, HprofReader, last_panic } from './pkg/sparkscope_web.js__SA_V__';
const ready = init({ module_or_path: './pkg/sparkscope_web_bg.wasm__SA_V__' });
async function hprof(id, file) {
  const r = new HprofReader();
  const CH = 16 << 20;
  let next = file.slice(0, CH).arrayBuffer();
  for (let off = 0; off < file.size; off += CH) {
    const buf = await next;
    if (off + CH < file.size) next = file.slice(off + CH, off + 2 * CH).arrayBuffer();
    r.feed(new Uint8Array(buf));
    self.postMessage({ id, progress: Math.min(off + CH, file.size) });
  }
  return r.finish();
}
const FN = { analyze, describe_heap, compare_heaps };
self.onmessage = async (e) => {
  const { id, fn, args } = e.data;
  try {
    await ready;
    const ok = fn === 'hprof' ? await hprof(id, args[0]) : FN[fn](...args);
    self.postMessage({ id, ok });
  } catch (x) {
    let msg = String((x && x.message) || x);
    let fatal = x instanceof WebAssembly.RuntimeError || /unreachable|memory/i.test(msg);
    try { const p = last_panic(); if (p) msg += ' (' + p + ')'; } catch (_) {}
    self.postMessage({ id, err: msg, fatal });
  }
};
"#;

pub fn static_worker_versioned(version: &str) -> String {
    WORKER_JS.replace("__SA_V__", &version_suffix(version))
}

pub(crate) fn version_suffix(version: &str) -> String {
    let query: String = version
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(40)
        .collect();
    if query.is_empty() {
        String::new()
    } else {
        format!("?v={query}")
    }
}

const STATIC_JS: &str = r#"
import init, { analyze, describe_heap, compare_heaps, HprofReader } from './pkg/sparkscope_web.js__SA_V__';
var OTHER=document.getElementById('langbtn').dataset.href;
var REDIR=false;try{var pref=localStorage.getItem('sa-lang');if(pref&&pref!==M.lang){REDIR=true;location.replace(OTHER+location.search+location.hash)}}catch(e){}
if(REDIR)await new Promise(function(){});
document.getElementById('langbtn').addEventListener('click',function(){try{localStorage.setItem('sa-lang',this.dataset.to)}catch(e){}location.href=OTHER+location.search+(window.__saMode==='cmp'?'#confronta':'')});
var busy=document.getElementById('busy'),bt=document.getElementById('busytext'),err=document.getElementById('err');
function start(t){err.hidden=true;bt.textContent=t;busy.hidden=false;document.documentElement.classList.add('sa-busy');try{busy.scrollIntoView({block:'nearest',behavior:'smooth'})}catch(e){}}
function fail(m){busy.hidden=true;err.textContent=m;err.hidden=false;document.documentElement.classList.remove('sa-busy');try{err.scrollIntoView({block:'nearest',behavior:'smooth'})}catch(e){}}
var readyP=null;function ready(){return readyP||(readyP=init({module_or_path:'./pkg/sparkscope_web_bg.wasm__SA_V__'}).catch(function(e){readyP=null;fail(M.wasm+e);throw e}))}
function hprofLocal(file,prog){var r=new HprofReader(),CH=16<<20,off=0;function step(){if(off>=file.size)return r.finish();return file.slice(off,off+CH).arrayBuffer().then(function(b){r.feed(new Uint8Array(b));off+=CH;if(prog)prog(Math.min(off,file.size));return step()})}return step()}
var FN={analyze:analyze,describe_heap:describe_heap,compare_heaps:compare_heaps};
function local(fn,args,prog){return ready().then(function(){return fn==='hprof'?hprofLocal(args[0],prog):FN[fn].apply(null,args)})}
function restartWorker(){try{W.terminate();W=new Worker('./sa-worker.js__SA_V__',{type:'module'});W.onmessage=onW;W.onerror=onWE}catch(e){W=null}}
function onW(e){var q=WQ[e.data.id];if(!q)return;if('progress' in e.data){if(q.prog)q.prog(e.data.progress);return}delete WQ[e.data.id];if('ok' in e.data)q.ok(e.data.ok);else{if(e.data.fatal)restartWorker();q.ko(new Error(e.data.err))}}
function onWE(){W=null;Object.keys(WQ).forEach(function(k){var q=WQ[k];delete WQ[k];local(q.fn,q.args,q.prog).then(q.ok,q.ko)})}
var W=null,WN=0,WQ={};try{W=new Worker('./sa-worker.js__SA_V__',{type:'module'});W.onmessage=onW;W.onerror=onWE}catch(e){W=null}
function call(fn,args,prog){if(!W)return local(fn,args,prog);return new Promise(function(ok,ko){var id=++WN;WQ[id]={ok:ok,ko:ko,fn:fn,args:args,prog:prog};W.postMessage({id:id,fn:fn,args:args})})}
function isDump(f){return f.slice(0,12).arrayBuffer().then(function(b){var u=new Uint8Array(b),h=String.fromCharCode.apply(null,u);if(h==='JAVA PROFILE')return true;var gz=u[0]===0x1f&&u[1]===0x8b;if(gz&&/hprof/i.test(f.name))return true;if((u[0]===0xfd&&h.slice(1,5)==='7zXZ')||/\.(xz|lzma|zst|zip|7z|bz2)$/i.test(f.name))throw new Error(M.dumpPacked);if(/\.hprof$/i.test(f.name))return true;if(f.size>MAXMEM)throw new Error(M.tooBig.replace('{s}',(f.size/1073741824).toFixed(1)));return false})}
var MAXMEM=1536*1048576;
function readDump(f){progShow();progSet(0,f.size);return call('hprof',[f],function(n){progSet(n,f.size)}).then(function(b){progHide();return b},function(e){progHide();throw e})}
function extractKey(v){v=v.trim();var m=v.match(/^(?:https?:\/\/)?[^\/\s]+\/([A-Za-z0-9]+)/);if(m)return m[1];return /^[A-Za-z0-9]{4,64}$/.test(v)?v:null}
var RKEY='sparkscope-recent';
function loadRecent(){try{return JSON.parse(localStorage.getItem(RKEY)||'[]')}catch(e){return []}}
function saveRecent(item){try{var l=loadRecent().filter(function(x){return x.key!==item.key});l.unshift(item);localStorage.setItem(RKEY,JSON.stringify(l.slice(0,15)))}catch(e){}}
function esc(s){return String(s).replace(/[&<>"']/g,function(c){return {'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]})}
function showRecent(){var l=loadRecent();if(!l.length)return;var side='<div class="recent"><h5>'+M.recent+'</h5>',p='<section class="panel"><h2>'+M.recent+'</h2><div class="rlist">';l.forEach(function(x){var c=x.bad?'bad':'ok';var h='?p='+encodeURIComponent(x.key);side+='<a href="'+h+'"><i class="'+c+'"></i><span>'+esc(x.title)+'</span></a>';p+='<a class="ritem '+c+'" href="'+h+'"><div><b>'+esc(x.title)+'</b><span>'+esc(x.detail)+' · '+esc(x.key)+'</span></div><span class="go">'+M.open+'</span></a>'});document.getElementById('recentside').innerHTML=side+'</div>';document.getElementById('recentbox').innerHTML=p+'</div></section>'}
var HOME=location.pathname;
var last=null;
var CUR=M.lang;
var cache={};
function uploadBytes(bytes,ctype){var h={'Content-Type':ctype};if(bytes.length>2&&bytes[0]===0x1f&&bytes[1]===0x8b)h['Content-Encoding']='gzip';return fetch('https://spark-usercontent.lucko.me/post',{method:'POST',headers:h,body:bytes}).then(function(r){if(!r.ok)throw new Error('HTTP '+r.status);return r.json()}).then(function(j){if(!j||!/^[A-Za-z0-9]{4,64}$/.test(j.key||''))throw new Error(M.unknown);return j.key})}
function siteUrl(q){return location.origin+HOME+q}
function setQuery(q){try{history.replaceState(null,'',siteUrl(q))}catch(e){}}
function render(bytes,name,key,lang,win){lang=lang||CUR;CUR=lang;if(!last||last.bytes!==bytes||Object.keys(cache).length>8)cache={};last={bytes:bytes,name:name,key:key,win:win||null};var ck=lang+'|'+(win||'');return (cache[ck]?Promise.resolve(cache[ck]):call('analyze',[bytes,name,key||undefined,HOME,lang,win||undefined]).then(function(s){return cache[ck]=JSON.parse(s)})).then(function(res){if(key){try{var u=new URL(location.href);u.searchParams.set('p',key);if(win)u.searchParams.set('w',win);else u.searchParams.delete('w');history.replaceState(null,'',u.toString())}catch(e){}}if(key)saveRecent({key:key,title:res.title,detail:res.detail,bad:res.bad});var cur=last;window.__saShare=function(){var q=function(k){return '?p='+encodeURIComponent(k)+(cur.win?'&w='+encodeURIComponent(cur.win):'')};if(cur.key)return {url:siteUrl(q(cur.key)),upload:null};return {url:null,upload:function(){return uploadBytes(cur.bytes,res.ctype||'application/x-spark-sampler').then(function(k){cur.key=k;setQuery(q(k));saveRecent({key:k,title:res.title,detail:res.detail,bad:res.bad});return siteUrl(q(k))})}}};setTimeout(function(){document.open();document.write(res.html);document.close();window.scrollTo(0,0)},0)})}
window.__saRender=function(l){if(!last)return;render(last.bytes,last.name,last.key,l,last.win).catch(function(e){alert(e.message||String(e))})};
window.__saFilter=function(w){if(!last)return;render(last.bytes,last.name,last.key,CUR,w||null).catch(function(e){alert(e.message||String(e))})};
var PR={ring:document.getElementById('pring'),fg:document.getElementById('pfg'),pct:document.getElementById('ppct'),meta:document.getElementById('pmeta'),spin:busy.querySelector('.spin')};
function mb(n){var s=(n/1048576).toFixed(1);return M.lang==='it'?s.replace('.',','):s}
function progShow(){PR.spin.hidden=true;PR.ring.hidden=false;PR.meta.hidden=false;progSet(0,0)}
function progSet(got,total){var known=total>0&&got<=total;PR.ring.classList.toggle('ind',!known);if(known){var p=Math.min(100,Math.floor(got/total*100));PR.fg.style.strokeDashoffset=String(88-88*p/100);PR.pct.textContent=p+'%';PR.meta.textContent=mb(got)+' / '+mb(total)+' MB'}else{PR.fg.style.strokeDashoffset='';PR.pct.textContent='';PR.meta.textContent=mb(got)+' MB'}}
function progHide(){PR.ring.hidden=true;PR.meta.hidden=true;PR.spin.hidden=false}
function download(key){return new Promise(function(ok,ko){var x=new XMLHttpRequest();x.open('GET','https://spark-usercontent.lucko.me/'+encodeURIComponent(key));x.responseType='arraybuffer';progShow();x.onprogress=function(e){progSet(e.loaded,e.lengthComputable?e.total:0)};x.onload=function(){if(x.status===404)return ko(new Error(M.notFound));if(x.status<200||x.status>=300)return ko(new Error(M.httpFail.replace('{s}',x.status)));ok(new Uint8Array(x.response))};x.onerror=function(){ko(new Error(M.httpFail.replace('{s}','0')))};x.send()}).then(function(b){progHide();return b},function(e){progHide();throw e})}
function runKey(key){start(M.downloadingKey.replace('{k}',key));download(key).then(function(b){bt.textContent=M.analyzing;return render(b,key,key,null,new URLSearchParams(location.search).get('w'))}).catch(function(e){fail(e.message||String(e))})}
document.getElementById('lform').addEventListener('submit',function(e){e.preventDefault();var k=extractKey(document.getElementById('link').value);if(!k){fail(M.badLink);return}location.search='?p='+encodeURIComponent(k)});
function send(f){if(!f)return;start(M.analyzingFile.replace('{f}',f.name).replace('{s}',(f.size/1048576).toFixed(1)));isDump(f).then(function(d){if(!d)return f.arrayBuffer().then(function(b){return new Uint8Array(b)});bt.textContent=M.readingDump.replace('{f}',f.name);return readDump(f).then(function(b){bt.textContent=M.analyzing;return b})}).then(function(b){return render(b,f.name,null)}).catch(function(e){fail(e.message||String(e))})}
var drop=document.getElementById('drop'),file=document.getElementById('file');
file.addEventListener('change',function(){send(file.files[0])});
['dragenter','dragover'].forEach(function(t){drop.addEventListener(t,function(e){e.preventDefault();drop.classList.add('over')})});
['dragleave','drop'].forEach(function(t){drop.addEventListener(t,function(e){e.preventDefault();drop.classList.remove('over')})});
drop.addEventListener('drop',function(e){send(e.dataTransfer.files[0])});
document.addEventListener('dragover',function(e){e.preventDefault()});document.addEventListener('drop',function(e){e.preventDefault();if(window.__saMode==='cmp')return;if(e.target.closest&&!e.target.closest('#drop'))send(e.dataTransfer.files[0])});
document.getElementById('link').addEventListener('paste',function(){var i=this;setTimeout(function(){if(extractKey(i.value))document.getElementById('lform').requestSubmit()},50)});
function fetchKey(key){return fetch('https://spark-usercontent.lucko.me/'+encodeURIComponent(key)).then(function(r){if(r.status===404)throw new Error(M.notFound);if(!r.ok)throw new Error(M.httpFail.replace('{s}',r.status));return r.arrayBuffer()}).then(function(b){return new Uint8Array(b)})}
function heapSlot(bytes,name,key){return call('describe_heap',[bytes,name,CUR]).then(function(s){var j=JSON.parse(s);j.bytes=bytes;j.key=key||null;return j})}
function renderCmp(a,b,lang){lang=lang||CUR;CUR=lang;return call('compare_heaps',[a.bytes,a.name,b.bytes,b.name,HOME,lang]).then(function(s){var res=JSON.parse(s);if(a.key&&b.key){try{var u=new URL(location.href);u.search='?a='+encodeURIComponent(a.key)+'&b='+encodeURIComponent(b.key);u.hash='';history.replaceState(null,'',u.toString())}catch(e){}}last=null;window.__saRender=function(l){renderCmp(a,b,l).catch(function(e){alert(e.message||String(e))})};window.__saShare=function(){var q=function(){return '?a='+encodeURIComponent(a.key)+'&b='+encodeURIComponent(b.key)};if(a.key&&b.key)return {url:siteUrl(q()),upload:null};return {url:null,upload:function(){var up=function(s){return s.key?Promise.resolve(s.key):uploadBytes(s.bytes,'application/x-spark-heap').then(function(k){s.key=k;return k})};return up(a).then(function(){return up(b)}).then(function(){setQuery(q());return siteUrl(q())})}}};setTimeout(function(){document.open();document.write(res.html);document.close();window.scrollTo(0,0)},0)})}
window.saSetupCompare({
fromFile:function(f){return isDump(f).then(function(d){return d?call('hprof',[f]):f.arrayBuffer().then(function(b){return new Uint8Array(b)})}).then(function(b){return heapSlot(b,f.name,null)})},
fromLink:function(v){var k=extractKey(v);if(!k)return Promise.reject(new Error(M.badLink));return fetchKey(k).then(function(b){return heapSlot(b,k,k)})},
run:function(a,b){start(M.cmpRunning);return renderCmp(a,b).catch(function(e){fail(e.message||String(e))})}
});
showRecent();
var sp=new URLSearchParams(location.search),q=sp.get('p'),qa=sp.get('a'),qb=sp.get('b');
if(qa&&qb){var ka=extractKey(qa),kb=extractKey(qb);if(ka&&kb){start(M.cmpRunning);Promise.all([fetchKey(ka).then(function(x){return heapSlot(x,ka,ka)}),fetchKey(kb).then(function(x){return heapSlot(x,kb,kb)})]).then(function(s){return renderCmp(s[0],s[1])}).catch(function(e){fail(e.message||String(e))})}}
else if(q){var k=extractKey(q);if(k){document.getElementById('link').value='https://spark.lucko.me/'+k;runKey(k)}}
"#;

#[cfg(test)]
mod tests {
    #[test]
    fn local_home_declares_the_browser_module_only_when_present() {
        let with = super::local_index_with(&[], "?v=abc");
        assert!(with.contains(r#"var SA_WV="?v=abc";"#));
        let without = super::local_index(&[]);
        assert!(without.contains(r#"var SA_WV="";"#));
    }
}

#[cfg(test)]
mod usage_tests {
    #[test]
    fn usage_counter_shows_grouped_numbers_and_hides_when_empty() {
        let _guard = crate::i18n::scoped(crate::i18n::Lang::It);
        let html = super::usage_html((1284, 3912));
        assert!(html.contains(r#"data-count="1284">1.284</b>"#));
        assert!(html.contains("3.912"));
        assert!(html.contains("User l'hanno"));
        assert!(super::usage_html((1, 1)).contains("User l'ha già"));
        assert!(super::usage_html((0, 0)).is_empty());
        assert!(!super::index_main("").contains("class=\"usage\""));
    }
}

pub fn privacy_page_public(base: Option<&str>) -> String {
    let page = privacy_page(true);
    let Some(base) = base else {
        return page;
    };
    let (own, d) = if crate::i18n::is_en() {
        (
            "privacy.html",
            "Which cookies and browser storage Spark Analyzer uses, how reports are stored and how to delete your data.",
        )
    } else {
        (
            "privacy-it.html",
            "Quali cookie e dati nel browser usa Spark Analyzer, come vengono conservati i report e come cancellare i tuoi dati.",
        )
    };
    page.replacen(
        "</head>",
        &format!(
            r#"<meta name="description" content="{d}"><link rel="canonical" href="{base}{own}"><link rel="alternate" hreflang="en" href="{base}privacy.html"><link rel="alternate" hreflang="it" href="{base}privacy-it.html"></head>"#
        ),
        1,
    )
}

pub fn privacy_page(hosted: bool) -> String {
    let t = crate::i18n::t;
    let row = |name: &str, kind: &str, what: &str, life: &str| {
        format!(
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>",
            esc(name),
            esc(kind),
            esc(what),
            esc(life)
        )
    };
    let local = t("memoria del browser", "browser storage");
    let until = t("finché non la cancelli", "until you clear it");
    let mut rows = String::new();
    if hosted {
        rows.push_str(&row(
            "sa_vid",
            t("cookie tecnico", "technical cookie"),
            t(
                "collega i report che analizzi al tuo browser, così nell'elenco vedi solo i tuoi",
                "links the reports you analyze to your browser, so the list only shows yours",
            ),
            t("1 anno", "1 year"),
        ));
    }
    for (name, what) in [
        (
            "sparkscope-theme",
            t("tema chiaro o scuro", "light or dark theme"),
        ),
        ("sa-lang", t("lingua del sito", "site language")),
        (
            "sa-unit",
            t(
                "unità dei consumi nel report (% o ms/tick)",
                "usage unit in reports (% or ms/tick)",
            ),
        ),
        ("sa-noside", t("barra laterale chiusa", "sidebar collapsed")),
        (
            "sparkscope-recent",
            t(
                "elenco delle tue analisi recenti",
                "list of your recent analyses",
            ),
        ),
        (
            "sa-chunk",
            t(
                "dimensione dei pezzi di upload più adatta alla tua rete",
                "upload piece size that suits your network",
            ),
        ),
        (
            "sa-cookies",
            t(
                "ricorda che hai chiuso l'avviso sui cookie",
                "remembers that you closed the cookie notice",
            ),
        ),
    ] {
        rows.push_str(&row(name, local, what, until));
    }
    rows.push_str(&row(
        "sa-news",
        t("memoria della sessione", "session storage"),
        t(
            "ricorda che hai chiuso l'avviso delle novità",
            "remembers that you closed the news notice",
        ),
        t("finché chiudi la scheda", "until you close the tab"),
    ));
    let data = if hosted {
        format!(
            "<h2>{}</h2><p>{}</p><p>{}</p><p>{}</p>",
            t("I report sul server", "Reports on the server"),
            t(
                "Quando analizzi un profilo su questo sito, il report viene salvato sul server ed eliminato automaticamente dopo 30 giorni. Nell'elenco delle analisi recenti vedi solo i tuoi; chi riceve il link di un report può aprirlo.",
                "When you analyze a profile on this site, the report is stored on the server and deleted automatically after 30 days. The recent list only shows your own; anyone you send a report link to can open it.",
            ),
            t(
                "Il contatore nella home conta i browser in forma anonima: salviamo solo un'impronta dell'identificativo, mai l'identificativo stesso.",
                "The counter on the home page counts browsers anonymously: we only store a fingerprint of the ID, never the ID itself.",
            ),
            t(
                "Come ogni server web, il server può registrare indirizzo IP, data e pagina richiesta nei log tecnici, usati solo per sicurezza e diagnosi dei problemi.",
                "Like any web server, the server may record IP address, time and requested page in technical logs, used only for security and troubleshooting.",
            ),
        )
    } else {
        format!(
            "<h2>{}</h2><p>{}</p><p>{}</p>",
            t("Dove avviene l'analisi", "Where the analysis runs"),
            t(
                "Su questo sito l'analisi avviene interamente nel tuo browser: il profilo non viene inviato a noi. Se incolli un link di spark, il browser scarica il profilo direttamente dal servizio di spark (spark.lucko.me).",
                "On this site the analysis runs entirely in your browser: the profile is never sent to us. If you paste a spark link, the browser downloads the profile directly from spark's service (spark.lucko.me).",
            ),
            t(
                "Il sito è ospitato da GitHub Pages, che può registrare l'indirizzo IP dei visitatori secondo l'informativa privacy di GitHub.",
                "The site is hosted on GitHub Pages, which may log visitors' IP addresses under GitHub's privacy statement.",
            ),
        )
    };
    let repo = "https://github.com/OnlyDehBest/Spark-Analyzer";
    let who = format!(
        "{} <a href=\"{repo}/issues\">{}</a>.",
        t(
            "Spark Resource Analyzer è un progetto open source di onlynelchilling. Per domande sulla privacy apri una segnalazione sul",
            "Spark Resource Analyzer is an open-source project by onlynelchilling. For privacy questions, open an issue on the",
        ),
        t("repository GitHub", "GitHub repository"),
    );
    let main = format!(
        r#"<article class="legal"><h1>{title}</h1><p class="sub">{updated}</p>
<h2>{who_t}</h2><p>{who}</p>
<h2>{ck_t}</h2><p>{ck}</p>
<div class="tblwrap" tabindex="0" role="region" aria-label="{c_reg}"><table><thead><tr><th>{c_name}</th><th>{c_kind}</th><th>{c_what}</th><th>{c_life}</th></tr></thead><tbody>{rows}</tbody></table></div>
<p>{none}</p>
{data}
<h2>{font_t}</h2><p>{font}</p>
<h2>{del_t}</h2><p>{del}</p>
<p><a class="btn sec" href="{home}">{back}</a></p></article>"#,
        title = t("Cookie e privacy", "Cookies & privacy"),
        updated = t(
            "Ultimo aggiornamento: 12 novembre 2026",
            "Last updated: 12 November 2026"
        ),
        who_t = t("Chi gestisce il sito", "Who runs the site"),
        ck_t = t(
            "Cookie e memoria del browser",
            "Cookies and browser storage"
        ),
        ck = t(
            "Il sito usa solo strumenti tecnici, necessari a farlo funzionare o a ricordare le tue scelte:",
            "The site only uses technical storage it needs to work or to remember your choices:",
        ),
        c_reg = t(
            "Elenco di cookie e memoria del browser",
            "List of cookies and browser storage"
        ),
        c_name = t("Nome", "Name"),
        c_kind = t("Tipo", "Type"),
        c_what = t("A cosa serve", "Purpose"),
        c_life = t("Durata", "Duration"),
        none = t(
            "Non usiamo cookie di profilazione, pubblicità o statistiche di terze parti.",
            "We don't use profiling, advertising or third-party analytics cookies.",
        ),
        font_t = t("Caratteri", "Fonts"),
        font = t(
            "I caratteri del sito (Inter e JetBrains Mono) sono serviti dal sito stesso: nessuna richiesta a Google Fonts o ad altri servizi esterni.",
            "The site's fonts (Inter and JetBrains Mono) are served by the site itself: no requests to Google Fonts or other external services.",
        ),
        del_t = t("Come cancellare i dati", "How to delete your data"),
        del = t(
            "Puoi cancellare cookie e memoria del sito dalle impostazioni del browser. I report sul server si eliminano da soli dopo 30 giorni.",
            "You can delete the site's cookies and storage from your browser settings. Reports on the server delete themselves after 30 days.",
        ),
        back = t("Torna all'analisi", "Back to analysis"),
        home = if hosted {
            if crate::i18n::is_en() {
                "/"
            } else {
                "/?lang=it"
            }
        } else if crate::i18n::is_en() {
            "./"
        } else {
            "./it.html"
        },
    );
    let other = if crate::i18n::is_en() {
        "privacy-it.html"
    } else {
        "privacy.html"
    };
    shell(&Page {
        crumb: t("Cookie e privacy", "Cookies & privacy"),
        lang_switch: other,
        title: t(
            "Cookie e privacy · Spark Resource Analyzer",
            "Cookies & privacy · Spark Resource Analyzer",
        ),
        side_extra: "",
        main: &main,
        script: "",
        module_script: "",
        home: if hosted {
            if crate::i18n::is_en() {
                "/"
            } else {
                "/?lang=it"
            }
        } else if crate::i18n::is_en() {
            "./"
        } else {
            "./it.html"
        },
        footer: "",
    })
}
