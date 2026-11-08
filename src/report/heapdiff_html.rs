use std::fmt::Write;

use super::html::{
    ANIM_JS, CSS, FAVICON, JS, copy_link_button, copyright_footer, esc, icon, lang_to, logo_svg,
    time_title, topbar,
};
use super::{fmt_duration, fmt_time, fmt_utc};
use crate::heap::{count, short_class};
use crate::heapdiff::{HeapComparison, PluginDelta, signed_bytes};

const PAGE_CSS: &str = ".lv{display:inline-block;border-radius:999px;padding:2px 9px;font-size:11px;font-weight:600;margin-bottom:4px;border:1px solid var(--line)}.lv2{color:var(--crit);border-color:var(--crit)}.lv1{color:var(--high);border-color:var(--high)}.lv0{color:var(--ok)}.up{color:var(--crit);font-weight:600}.down{color:var(--ok)}table.hd{table-layout:fixed;width:100%}table.hd td{vertical-align:top;white-space:normal}table.hd td.n{white-space:nowrap}table.hd td code{white-space:nowrap}table.hd .cl{display:block;margin-bottom:3px}table.hd .why{margin-top:2px}#cmp th:nth-child(1){width:15%}#cmp th:nth-child(2),#cmp th:nth-child(3),#cmp th:nth-child(4),#cmp th:nth-child(5){width:8.5%}#cmp th:nth-child(6){width:24%}#cmp th:nth-child(7){width:27%}.warnbox{background:rgba(249,115,22,.08);border:1px solid var(--high);border-radius:12px;padding:12px 16px;margin:0 0 14px;color:var(--hd)}.sol li{margin:6px 0}@media(max-width:900px){table.hd{table-layout:auto}}";

fn stat(out: &mut String, _icon: &str, label: &str, value: &str, suffix: &str, cls: &str) {
    let _ = write!(
        out,
        r#"<div class="kpi {cls}"><div class="sl">{}</div><div class="sv">{}<small>{}</small></div></div>"#,
        esc(label),
        esc(value),
        esc(suffix)
    );
}

fn delta_span(v: i64, leak: bool) -> String {
    let cls = if v > 0 && leak {
        "up"
    } else if v < 0 {
        "down"
    } else {
        "dim"
    };
    format!(r#"<span class="{cls}">{}</span>"#, esc(&signed_bytes(v)))
}

fn plugin_row(out: &mut String, p: &PluginDelta) {
    let t = crate::i18n::t;
    let leak = p.verdict.is_leak();
    let mut classes = String::with_capacity(256);
    for c in p.classes.iter().filter(|c| c.instance_delta() != 0).take(3) {
        let d = c.instance_delta();
        let _ = write!(
            classes,
            r#"<span class="cl"><code title="{}">{}</code> <span class="dim">{} → {}</span> <span class="{}">{}{}</span></span>"#,
            esc(&c.kind),
            esc(short_class(&c.kind)),
            count(c.before_instances),
            count(c.after_instances),
            if leak && d > 0 { "up" } else { "dim" },
            if d > 0 { "+" } else { "" },
            count(d)
        );
    }
    if classes.is_empty() {
        classes.push_str(&format!(
            r#"<span class="dim">{}</span>"#,
            t("nessuna variazione", "no change")
        ));
    }
    let rate =
        if p.per_hour.abs() >= 1024.0 && !matches!(p.verdict, crate::heapdiff::Verdict::Stable) {
            format!("{}/h", signed_bytes(p.per_hour.round() as i64))
        } else {
            "—".to_string()
        };
    let _ = write!(
        out,
        r#"<tr><td><b>{}</b> <span class="dim">{}</span></td><td class="n" data-v="{}">{}</td><td class="n" data-v="{}">{}</td><td class="n" data-v="{}">{}</td><td class="n" data-v="{}">{}</td><td class="small">{}</td><td class="small"><span class="lv {}">{}</span>{}</td></tr>"#,
        esc(&p.name),
        esc(&p.version),
        p.before,
        esc(&fmt_time(p.before as f64, "byte")),
        p.after,
        esc(&fmt_time(p.after as f64, "byte")),
        p.delta(),
        delta_span(p.delta(), leak),
        p.per_hour.round() as i64,
        esc(&rate),
        classes,
        p.verdict.css(),
        esc(p.verdict.label()),
        if p.reason.is_empty() {
            String::new()
        } else {
            format!(r#"<div class="why">{}</div>"#, esc(&p.reason))
        }
    );
}

pub fn render(c: &HeapComparison) -> String {
    let t = crate::i18n::t;
    let bytes = |v: f64| fmt_time(v, "byte");
    let leaks = c.leaks().count();
    let bad = leaks > 0 || c.restarted;
    let mut b = String::with_capacity(32 * 1024);
    let title = t("Confronto heap", "Heap comparison");

    let _ = write!(
        b,
        r##"<div class="shell"><aside class="side"><a class="brand" id="brandlink" href="./" title="Home"><span class="logo">{}</span><span>Spark Resource Analyzer</span></a><div class="status {}"><i></i>{}</div><nav><a href="#riassunto" class="on">{}<span>{}</span></a><a href="#cmp">{}<span>{}</span></a><a href="#java">{}<span>{}</span></a><a href="#azioni">{}<span>{}</span></a></nav><div class="sidefoot"><div class="srv"><b>{}</b><span>{}</span></div><button id="langbtn" class="ghost lang" data-to="{}" title="Lingua / Language">{}</button><button id="theme" class="ghost" title="{}">{}</button></div></aside><main class="content">{}"##,
        logo_svg(leaks > 0),
        if bad { "lag" } else { "okk" },
        if leaks > 0 {
            t("Leak trovato", "Leak found")
        } else {
            t("Stabile", "Stable")
        },
        icon("clipboard"),
        t("Riassunto", "Summary"),
        icon("cube"),
        t("Per plugin", "By plugin"),
        icon("list"),
        t("Classi Java", "Java classes"),
        icon("alert"),
        t("Cosa fare", "What to do"),
        esc(if c.after.platform.is_empty() {
            "Server"
        } else {
            &c.after.platform
        }),
        t("confronto heap", "heap comparison"),
        lang_to(),
        lang_to().to_ascii_uppercase(),
        t("Tema chiaro/scuro", "Light/dark theme"),
        icon("moon"),
        topbar(title)
    );

    let uptime = match (c.before.uptime_ms, c.after.uptime_ms) {
        (Some(u1), Some(u2)) if !c.restarted => tf!(
            " · stesso avvio del server (uptime {} → {})",
            " · same server start (uptime {} → {})",
            fmt_duration(u1 as f64 / 1000.0),
            fmt_duration(u2 as f64 / 1000.0)
        ),
        _ => String::new(),
    };
    let _ = write!(
        b,
        r#"<div class="top"><div><h1>{}</h1><p class="sub">{}</p></div><div class="actions">{}</div></div>"#,
        esc(title),
        esc(&format!(
            "heap summary · {} · {} {} → {} {}{uptime}",
            if c.after.platform.is_empty() {
                "Server"
            } else {
                &c.after.platform
            },
            t("prima", "before"),
            fmt_utc(c.before.generated_ms),
            t("dopo", "after"),
            fmt_utc(c.after.generated_ms)
        )),
        copy_link_button()
    );

    for w in &c.warnings {
        let _ = write!(b, r#"<div class="warnbox">{}</div>"#, esc(w));
    }
    let _ = write!(
        b,
        r#"<section class="sumcard {}" id="riassunto"><div class="score" style="--p:{}"><b>{}</b></div><div class="sumtxt"><p class="l1">{}</p>"#,
        if bad { "bad" } else { "ok" },
        if leaks > 0 { 22 } else { 92 },
        if leaks > 0 { "!" } else { "✓" },
        esc(&c.headline)
    );
    for line in &c.explanation {
        let _ = write!(b, r#"<p class="ln">{}</p>"#, esc(line));
    }
    b.push_str("</div></section>");

    let mut stats = String::with_capacity(2048);
    stat(
        &mut stats,
        "clock",
        t("Intervallo", "Interval"),
        &fmt_duration(c.hours * 3600.0),
        "",
        "",
    );
    let heap_label = tf!(
        "Heap usato (prima {})",
        "Heap used (before {})",
        bytes(c.before.used_bytes as f64)
    );
    stat(
        &mut stats,
        "cube",
        &heap_label,
        &bytes(c.after.used_bytes as f64),
        &format!(" {}", signed_bytes(c.heap_delta())),
        if leaks > 0 { "bad" } else { "" },
    );
    stat(
        &mut stats,
        "cube",
        t("Crescita", "Growth"),
        &format!("{}/h", signed_bytes(c.heap_per_hour.round() as i64)),
        "",
        if leaks > 0 { "wrn" } else { "ok" },
    );
    stat(
        &mut stats,
        "users",
        t("Giocatori", "Players"),
        &format!("{} → {}", c.before.players, c.after.players),
        "",
        "",
    );
    if let Some(h) = c.hours_to_full {
        stat(
            &mut stats,
            "hourglass",
            t("Heap pieno tra", "Heap full in"),
            &format!("~{}", h.round().max(1.0)),
            t(" ore", " hours"),
            "bad",
        );
    }
    stat(
        &mut stats,
        "alert",
        t("Leak confermati", "Confirmed leaks"),
        &leaks.to_string(),
        "",
        if leaks > 0 { "bad" } else { "ok" },
    );
    let _ = write!(b, r#"<section class="kpis">{stats}</section>"#);

    let _ = write!(
        b,
        r#"<section id="cmp" class="panel"><h2><span class="hi">{}</span>{}</h2><p class="note">{}</p>"#,
        icon("cube"),
        t("Cosa è cresciuto, per plugin", "What grew, by plugin"),
        t(
            "Stesse classi confrontate nei due dump. «Leak confermato» = cresce in modo continuo più di quanto spieghino i giocatori. «Cresce con i giocatori» = la crescita segue i giocatori entrati. «Stabile» = non è cambiato: è una cache, non un leak.",
            "Same classes compared across both dumps. “Leak confirmed” = it keeps growing more than the players explain. “Grows with players” = the growth follows the players who joined. “Stable” = it didn't change: it's a cache, not a leak.",
        )
    );
    if c.plugins.is_empty() {
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
            r#"<table class="sortable hp hd"><thead><tr><th>Plugin</th><th class="n">{}</th><th class="n">{}</th><th class="n">{}</th><th class="n">{}</th><th>{}</th><th>{}</th></tr></thead><tbody>"#,
            t("Prima", "Before"),
            t("Dopo", "After"),
            t("Differenza", "Change"),
            t("Ritmo", "Rate"),
            t("Classi cambiate", "Changed classes"),
            t("Verdetto", "Verdict")
        );
        for p in &c.plugins {
            plugin_row(&mut b, p);
        }
        b.push_str("</tbody></table>");
    }
    b.push_str("</section>");

    let _ = write!(
        b,
        r#"<section id="java" class="panel"><h2><span class="hi">{}</span>{}</h2><p class="note">{}</p>"#,
        icon("list"),
        t("Classi Java cresciute", "Java classes that grew"),
        t(
            "Le mappe e le liste dei plugin compaiono come classi Java. Se crescono allo stesso ritmo delle classi di un plugin che perde memoria, quasi certamente sono sue.",
            "Plugins' maps and lists show up as Java classes. If they grow at the same rate as the classes of a leaking plugin, they almost certainly belong to it.",
        )
    );
    if c.java.is_empty() {
        let _ = write!(
            b,
            "<p>{}</p>",
            t("Nessuna classe Java è cresciuta.", "No Java class grew.")
        );
    } else {
        let _ = write!(
            b,
            r#"<table class="sortable"><thead><tr><th>{}</th><th class="n">{}</th><th class="n">{}</th><th class="n">{}</th><th>{}</th></tr></thead><tbody>"#,
            t("Classe", "Class"),
            t("Prima", "Before"),
            t("Dopo", "After"),
            t("Differenza", "Change"),
            t("Probabile proprietario", "Likely owner")
        );
        for j in &c.java {
            let raw = !j.class.kind.contains('.');
            let (v1, v2) = if raw {
                (
                    fmt_time(j.class.before_size as f64, "byte"),
                    fmt_time(j.class.after_size as f64, "byte"),
                )
            } else {
                (
                    count(j.class.before_instances),
                    count(j.class.after_instances),
                )
            };
            let diff = if raw {
                signed_bytes(j.class.size_delta())
            } else {
                format!(
                    "+{} ({})",
                    count(j.class.instance_delta()),
                    signed_bytes(j.class.size_delta())
                )
            };
            let _ = write!(
                b,
                r#"<tr><td><code>{}</code></td><td class="n">{}</td><td class="n">{}</td><td class="n"><span class="{}">{}</span></td><td class="small{}">{}</td></tr>"#,
                esc(&j.class.kind),
                v1,
                v2,
                if j.owner_hint.is_some() { "up" } else { "dim" },
                esc(&diff),
                if j.owner_hint.is_some() { "" } else { " dim" },
                esc(&j.reason)
            );
        }
        b.push_str("</tbody></table>");
    }
    b.push_str("</section>");

    let _ = write!(
        b,
        r#"<section id="azioni" class="panel"><h2><span class="hi">{}</span>{}</h2><ol class="sol">"#,
        icon("alert"),
        t("Cosa fare", "What to do")
    );
    for a in &c.actions {
        let _ = write!(b, "<li>{}</li>", esc(a));
    }
    b.push_str("</ol></section>");
    b.push_str(&copyright_footer());
    b.push_str("</main></div>");

    let (page_title, local_title) = time_title(title, c.after.generated_ms);
    format!(
        "<!doctype html><html lang=\"{}\" data-theme=\"dark\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">{}<title>{}</title><style>{}{}</style></head><body>{}<script>{}{}</script>{}</body></html>",
        crate::i18n::lang().code(),
        FAVICON,
        esc(&page_title),
        CSS,
        PAGE_CSS,
        b,
        ANIM_JS,
        JS,
        local_title
    )
}
