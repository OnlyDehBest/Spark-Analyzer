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
            html = html.replacen("</main>", &format!("{section}</main>"), 1);
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
        r#"<!doctype html><html lang="{}" data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">{}<title>{}</title><style>{}{}</style></head><body><div class="shell"><aside class="side"><a class="brand" href="{}" title="Home"><span class="logo">{}</span><span>Spark-Analyzer</span></a><nav><a class="on" href="{}">{}<span>{}</span></a></nav>{}<div class="sidefoot"><div class="srv"><b>Spark-Analyzer</b><span>v{}</span></div><button id="langbtn" class="ghost lang" data-to="{}" data-href="{}" title="Lingua / Language">{}</button><button id="theme" class="ghost" title="{}">{}</button></div></aside><main class="content">{}{}</main></div><script>{}{}{}{}{}</script>{}</body></html>"#,
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
        COMPARE_JS,
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
        r#"<div class="slot" data-slot="{index}"><h3>{title} <span class="tag">{n}</span></h3><p class="h">{hint}</p><div class="schip" hidden><span class="sok">{check}</span><div class="stxt"><b class="sname"></b><span class="sdet"></span></div><button type="button" class="ghost sclear" title="{remove}">{x}</button></div><div class="sinput"><form class="lrow slinkform"><input class="slink" autocomplete="off" placeholder="{ph}"><button class="btn sec" type="submit">{link}{load}</button></form><label class="drop sdrop"><input class="sfile" type="file" accept=".sparkheap" hidden><span class="dicon">{upload}</span><b>{drop}</b><span>{click}</span></label></div></div>"#,
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

pub fn index_main(subtitle: &str, recent_panel: &str) -> String {
    let t = crate::i18n::t;
    format!(
        r#"<div class="top"><div><h1>{h1}</h1><p class="sub">{}</p></div></div>
<div class="modesel"><div class="seg big" id="modesel"><button type="button" class="on" data-mode="one">{mode_one}</button><button type="button" data-mode="cmp">{mode_cmp}</button></div><span class="dim small" id="modehint">{mode_hint}</span></div>
<div id="mode-one">
<section class="panel"><h2><span class="hi">{link}</span>{link_t}</h2>
<form id="lform" class="lrow"><input id="link" autocomplete="off" placeholder="{ph}"><button class="btn" type="submit">{bolt}{go}</button></form>
<p class="note">{note1} <code>/spark profiler stop</code>.</p></section>
<section class="panel"><h2><span class="hi">{upload}</span>{file_t}</h2>
<label id="drop" class="drop"><input id="file" type="file" accept=".sparkprofile,.sparkheap,.sparkhealth" hidden><span class="dicon">{upload}</span><b>{drop}</b><span>{click}</span><small>{saved} <code>/spark profiler stop --save-to-file</code> {in_dir} <code>plugins/spark/</code></small></label></section>
</div>
<div id="mode-cmp" hidden>{compare}</div>
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
        mode_one = t("Analizza un profilo", "Analyze a profile"),
        mode_cmp = t("Confronta due heap", "Compare two heaps"),
        mode_hint = t(
            "oppure scegli «Confronta due heap» per trovare i memory leak",
            "or pick “Compare two heaps” to find memory leaks",
        ),
        compare = compare_panel(),
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
    let page = shell(&Page {
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
    });
    page.replacen(
        "</head>",
        r#"<link rel="modulepreload" href="./pkg/sparkscope_web.js"><link rel="preload" href="./pkg/sparkscope_web_bg.wasm" as="fetch" type="application/wasm" crossorigin></head>"#,
        1,
    )
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
function accept(i,p){var el=document.querySelector('.slot[data-slot="'+i+'"]');err.hidden=true;el.classList.add('busy-slot');return p.then(function(s){slots[i]=s;show(i)}).catch(fail).then(function(){el.classList.remove('busy-slot')})}
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
document.addEventListener('dragover',function(e){e.preventDefault()});document.addEventListener('drop',function(e){e.preventDefault();if(window.__saMode==='cmp')return;if(e.target.closest&&!e.target.closest('#drop'))send(e.dataTransfer.files[0])});
document.getElementById('link').addEventListener('paste',function(){var i=this;setTimeout(function(){if(/spark\.lucko\.me\/|^[A-Za-z0-9]{6,}$/.test(i.value.trim()))document.getElementById('lform').requestSubmit()},50)});
function staged(r){return r.json().then(function(j){if(j.error)throw new Error(j.error);return j})}
window.saSetupCompare({
fromFile:function(f){return fetch('/api/heap/file?name='+encodeURIComponent(f.name)+'&'+LQ,{method:'POST',body:f}).then(staged)},
fromLink:function(v){return fetch('/api/heap/link?'+LQ,{method:'POST',body:v}).then(staged)},
run:function(a,b){start(M.cmpRunning);return fetch('/api/heap/compare?a='+encodeURIComponent(a.stage)+'&b='+encodeURIComponent(b.stage)+'&'+LQ,{method:'POST'}).then(done)}
});
})();
"#;

const STATIC_JS: &str = r#"
import init, { analyze, describe_heap, compare_heaps } from './pkg/sparkscope_web.js';
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
var cache={};
function render(bytes,name,key,lang,win){lang=lang||CUR;CUR=lang;if(!last||last.bytes!==bytes||Object.keys(cache).length>8)cache={};last={bytes:bytes,name:name,key:key,win:win||null};return ready.then(function(){var ck=lang+'|'+(win||''),res=cache[ck]||(cache[ck]=JSON.parse(analyze(bytes,name,key||undefined,HOME,lang,win||undefined)));if(key){try{var u=new URL(location.href);u.searchParams.set('p',key);if(win)u.searchParams.set('w',win);else u.searchParams.delete('w');history.replaceState(null,'',u.toString())}catch(e){}}if(key)saveRecent({key:key,title:res.title,detail:res.detail,bad:res.bad});setTimeout(function(){document.open();document.write(res.html);document.close();window.scrollTo(0,0)},0)})}
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
document.addEventListener('dragover',function(e){e.preventDefault()});document.addEventListener('drop',function(e){e.preventDefault();if(window.__saMode==='cmp')return;if(e.target.closest&&!e.target.closest('#drop'))send(e.dataTransfer.files[0])});
document.getElementById('link').addEventListener('paste',function(){var i=this;setTimeout(function(){if(extractKey(i.value))document.getElementById('lform').requestSubmit()},50)});
function fetchKey(key){return fetch('https://spark-usercontent.lucko.me/'+encodeURIComponent(key)).then(function(r){if(r.status===404)throw new Error(M.notFound);if(!r.ok)throw new Error(M.httpFail.replace('{s}',r.status));return r.arrayBuffer()}).then(function(b){return new Uint8Array(b)})}
function heapSlot(bytes,name,key){return ready.then(function(){var j=JSON.parse(describe_heap(bytes,name,CUR));j.bytes=bytes;j.key=key||null;return j})}
function renderCmp(a,b,lang){lang=lang||CUR;CUR=lang;return ready.then(function(){var res=JSON.parse(compare_heaps(a.bytes,a.name,b.bytes,b.name,HOME,lang));if(a.key&&b.key){try{var u=new URL(location.href);u.search='?a='+encodeURIComponent(a.key)+'&b='+encodeURIComponent(b.key);u.hash='';history.replaceState(null,'',u.toString())}catch(e){}}last=null;window.__saRender=function(l){renderCmp(a,b,l).catch(function(e){alert(e.message||String(e))})};setTimeout(function(){document.open();document.write(res.html);document.close();window.scrollTo(0,0)},0)})}
window.saSetupCompare({
fromFile:function(f){return f.arrayBuffer().then(function(b){return heapSlot(new Uint8Array(b),f.name,null)})},
fromLink:function(v){var k=extractKey(v);if(!k)return Promise.reject(new Error(M.badLink));return fetchKey(k).then(function(b){return heapSlot(b,k,k)})},
run:function(a,b){start(M.cmpRunning);return renderCmp(a,b).catch(function(e){fail(e.message||String(e))})}
});
showRecent();
var sp=new URLSearchParams(location.search),q=sp.get('p'),qa=sp.get('a'),qb=sp.get('b');
if(qa&&qb){var ka=extractKey(qa),kb=extractKey(qb);if(ka&&kb){start(M.cmpRunning);Promise.all([fetchKey(ka).then(function(x){return heapSlot(x,ka,ka)}),fetchKey(kb).then(function(x){return heapSlot(x,kb,kb)})]).then(function(s){return renderCmp(s[0],s[1])}).catch(function(e){fail(e.message||String(e))})}}
else if(q){var k=extractKey(q);if(k){document.getElementById('link').value='https://spark.lucko.me/'+k;runKey(k)}}
"#;
