use std::collections::HashMap;
use std::fmt::Write;

use super::{Report, fmt_duration, fmt_num, fmt_time, fmt_utc};
use crate::rules::{Severity, short};

pub(crate) fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '&' => o.push_str("&amp;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            _ => o.push(c),
        }
    }
    o
}

fn sev_class(s: Severity) -> &'static str {
    match s {
        Severity::Critico => "crit",
        Severity::Alto => "high",
        Severity::Medio => "med",
        Severity::Basso => "low",
        Severity::Info => "info",
    }
}

struct Trie {
    names: Vec<String>,
    name_idx: HashMap<String, u32>,
    nodes: Vec<(u32, u64, Vec<usize>)>,
}

impl Trie {
    fn new() -> Trie {
        Trie {
            names: Vec::new(),
            name_idx: HashMap::new(),
            nodes: vec![(0, 0, Vec::new())],
        }
    }
    fn name(&mut self, s: &str) -> u32 {
        if let Some(i) = self.name_idx.get(s) {
            return *i;
        }
        let i = self.names.len() as u32;
        self.names.push(s.to_string());
        self.name_idx.insert(s.to_string(), i);
        i
    }
    fn add(&mut self, frames: &[&str], v: u64) {
        let mut cur = 0usize;
        self.nodes[0].1 += v;
        for f in frames {
            let n = self.name(f);
            let found = self.nodes[cur]
                .2
                .iter()
                .copied()
                .find(|c| self.nodes[*c].0 == n);
            let next = match found {
                Some(c) => c,
                None => {
                    self.nodes.push((n, 0, Vec::new()));
                    let id = self.nodes.len() - 1;
                    self.nodes[cur].2.push(id);
                    id
                }
            };
            self.nodes[next].1 += v;
            cur = next;
        }
    }
    fn to_json(&self, id: usize, out: &mut String, min: u64) {
        let (n, v, kids) = &self.nodes[id];
        let _ = write!(out, "[{},{}", n, v);
        let mut ks: Vec<usize> = kids
            .iter()
            .copied()
            .filter(|k| self.nodes[*k].1 >= min)
            .collect();
        ks.sort_by(|a, b| self.nodes[*b].1.cmp(&self.nodes[*a].1));
        for k in ks {
            out.push(',');
            self.to_json(k, out, min);
        }
        out.push(']');
    }
}

fn flame_json(folded: &[String]) -> String {
    let mut t = Trie::new();
    t.name("main thread");
    for line in folded {
        let Some((stack, v)) = line.rsplit_once(' ') else {
            continue;
        };
        let Ok(v) = v.parse::<u64>() else { continue };
        let frames: Vec<&str> = stack.split(';').collect();
        t.add(&frames, v);
    }
    let total = t.nodes[0].1;
    let min = total / 4000;
    let mut tree = String::new();
    t.to_json(0, &mut tree, min.max(1));
    let names = serde_json::to_string(&t.names).unwrap_or_else(|_| "[]".into());
    format!("{{\"names\":{names},\"tree\":{tree}}}")
}

fn timeline_svg(r: &Report) -> Option<String> {
    let ws = &r.analysis.windows;
    if ws.len() < 2 {
        return None;
    }
    let has_mspt = ws.iter().any(|w| w.mspt_max > 0.0);
    let vals_a: Vec<f64> = ws
        .iter()
        .map(|w| {
            if has_mspt {
                w.mspt_max
            } else {
                w.busy_per_tick.max(w.busy)
            }
        })
        .collect();
    let vals_b: Vec<f64> = ws
        .iter()
        .map(|w| if has_mspt { w.mspt_median } else { 0.0 })
        .collect();
    let vals_c: Vec<f64> = ws.iter().map(|w| w.busy_per_tick).collect();
    let max = vals_a
        .iter()
        .chain(vals_c.iter())
        .cloned()
        .fold(50.0f64, f64::max)
        * 1.1;
    let wd = 960.0;
    let ht = 220.0;
    let pad_l = 44.0;
    let pad_b = 26.0;
    let pw = wd - pad_l - 10.0;
    let ph = ht - pad_b - 10.0;
    let n = ws.len() as f64;
    let x = |i: usize| pad_l + pw * (i as f64 + 0.5) / n;
    let y = |v: f64| 10.0 + ph - (v / max) * ph;
    let mut s = String::new();
    let _ = write!(
        s,
        r#"<svg viewBox="0 0 {wd} {ht}" class="chart" role="img" aria-label="Timeline MSPT per finestra">"#
    );
    for i in 0..ws.len() {
        if ws[i].is_spike {
            let bw = pw / n;
            let _ = write!(
                s,
                r#"<rect x="{:.1}" y="10" width="{:.1}" height="{:.1}" class="spikebg"><title>Finestra #{} — lag spike</title></rect>"#,
                pad_l + bw * i as f64,
                bw,
                ph,
                i + 1
            );
        }
    }
    for g in [0.0, 25.0, 50.0, 100.0, 200.0, 400.0, 800.0] {
        if g > max {
            break;
        }
        let gy = y(g);
        let cls = if g == 50.0 { "grid budget" } else { "grid" };
        let _ = write!(
            s,
            r#"<line x1="{pad_l}" x2="{:.1}" y1="{gy:.1}" y2="{gy:.1}" class="{cls}"/><text x="{:.1}" y="{:.1}" class="axis" text-anchor="end">{g:.0}</text>"#,
            wd - 10.0,
            pad_l - 6.0,
            gy + 4.0
        );
    }
    let line = |vals: &[f64], cls: &str, s: &mut String| {
        let pts: Vec<String> = vals
            .iter()
            .enumerate()
            .map(|(i, v)| format!("{:.1},{:.1}", x(i), y(*v)))
            .collect();
        let _ = write!(s, r#"<polyline points="{}" class="{cls}"/>"#, pts.join(" "));
    };
    line(&vals_a, "l1", &mut s);
    if has_mspt {
        line(&vals_b, "l2", &mut s);
    }
    if ws.iter().any(|w| w.ticks > 0) {
        line(&vals_c, "l3", &mut s);
    }
    for (i, w) in ws.iter().enumerate() {
        let _ = write!(
            s,
            r#"<circle cx="{:.1}" cy="{:.1}" r="3" class="dot{}" data-i="{i}"/>"#,
            x(i),
            y(vals_a[i]),
            if w.is_spike { " sp" } else { "" }
        );
    }
    let _ = write!(
        s,
        r#"<line class="guide" x1="0" x2="0" y1="10" y2="{:.1}" style="display:none"/><rect class="brush" x="0" y="10" width="0" height="{:.1}" style="display:none"/>"#,
        10.0 + ph,
        ph
    );
    let bw = pw / n;
    for i in 0..ws.len() {
        let _ = write!(
            s,
            r#"<rect class="hov" x="{:.1}" y="10" width="{:.1}" height="{:.1}" data-i="{i}" data-x="{:.1}"/>"#,
            pad_l + bw * i as f64,
            bw,
            ph,
            x(i)
        );
    }
    let step = ((ws.len() as f64) / 12.0).ceil().max(1.0) as usize;
    for i in (0..ws.len()).step_by(step) {
        let _ = write!(
            s,
            r#"<text x="{:.1}" y="{:.1}" class="axis" text-anchor="middle">#{}</text>"#,
            x(i),
            ht - 8.0,
            ws[i].index + 1
        );
    }
    s.push_str("</svg>");
    let legend = if has_mspt {
        r#"<div class="legend"><span class="k1"></span>MSPT massimo <span class="k2"></span>MSPT mediano <span class="k3"></span>lavoro main thread (ms/tick) <span class="kb"></span>budget 50 ms <span class="ks"></span>lag spike</div>"#
    } else {
        r#"<div class="legend"><span class="k1"></span>carico main thread <span class="ks"></span>lag spike</div>"#
    };
    let t = crate::i18n::t;
    let rows: Vec<serde_json::Value> = ws
        .iter()
        .map(|w| {
            let reason = r
                .analysis
                .spikes
                .iter()
                .find(|sp| sp.window.index == w.index)
                .map(|sp| sp.reason.clone());
            let causes: Vec<String> = r
                .analysis
                .spikes
                .iter()
                .find(|sp| sp.window.index == w.index)
                .map(|sp| {
                    sp.cat_deltas
                        .iter()
                        .take(3)
                        .map(|d| format!("{} +{} {}", d.name, fmt_num(d.delta), sp.unit))
                        .collect()
                })
                .unwrap_or_default();
            serde_json::json!({
                "t": crate::rules::window_label(w),
                "o": w.index,
                "tps": w.tps, "med": w.mspt_median, "max": w.mspt_max, "busy": w.busy_per_tick,
                "pl": w.players, "en": w.entities, "te": w.tile_entities, "ch": w.chunks,
                "cpu": w.cpu_process * 100.0, "spike": reason, "causes": causes,
            })
        })
        .collect();
    let labels = serde_json::json!({
        "tps": "TPS",
        "med": t("MSPT mediano", "Median MSPT"),
        "max": t("MSPT massimo", "Max MSPT"),
        "busy": t("Lavoro main thread", "Main thread work"),
        "pl": t("Giocatori", "Players"),
        "en": t("Entità", "Entities"),
        "te": t("Block entity", "Block entities"),
        "ch": t("Chunk", "Chunks"),
        "cpu": t("CPU processo", "Process CPU"),
        "spike": t("Lag spike", "Lag spike"),
        "causes": t("Cresciuto", "Grew"),
    });
    let data = serde_json::json!({ "rows": rows, "labels": labels, "mspt": has_mspt })
        .to_string()
        .replace("</", "<\\/");
    Some(format!(
        r#"<div class="chartwrap">{s}<div class="ctip" hidden></div><script type="application/json" class="chartdata">{data}</script></div>{legend}"#
    ))
}

pub(crate) fn pvh(p: f64) -> String {
    format!(r#"<span class="pv" data-p="{p}">{}%</span>"#, fmt_num(p))
}

pub(crate) fn pv_html(escaped: &str) -> String {
    crate::rules::render_pv(escaped, pvh)
}

fn pct_bar(p: f64) -> String {
    format!(
        r#"<span class="bar"><i style="width:{:.1}%"></i></span>"#,
        p.clamp(0.0, 100.0)
    )
}

pub(crate) const LOGO_SVG: &str = r##"<svg viewBox="0 0 32 32" width="32" height="32" aria-hidden="true"><rect x=".5" y=".5" width="31" height="31" rx="9" style="fill:var(--card3);stroke:var(--line)"/><circle cx="14" cy="14" r="7.2" fill="none" style="stroke:var(--hd)" stroke-width="2.2"/><path d="M19.4 19.4L24.6 24.6" style="stroke:var(--hd)" stroke-width="2.8" stroke-linecap="round"/><path d="M9 14.4h2.1l1.4-3.2l2.2 5.9l1.6-3.8H19" fill="none" style="stroke:var(--ok)" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
pub(crate) const FAVICON: &str = r##"<link rel="preconnect" href="https://fonts.googleapis.com"><link rel="preconnect" href="https://fonts.gstatic.com" crossorigin><link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap"><link rel="icon" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect x='.5' y='.5' width='31' height='31' rx='9' fill='%231a1a1a' stroke='%23333'/%3E%3Ccircle cx='14' cy='14' r='7.2' fill='none' stroke='%23fafafa' stroke-width='2.2'/%3E%3Cpath d='M19.4 19.4L24.6 24.6' stroke='%23fafafa' stroke-width='2.8' stroke-linecap='round'/%3E%3Cpath d='M9 14.4h2.1l1.4-3.2l2.2 5.9l1.6-3.8H19' fill='none' stroke='%23a3e635' stroke-width='1.9' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E">"##;

pub(crate) fn topbar(page: &str, viewer: Option<&str>) -> String {
    let right = viewer
        .map(|v| {
            format!(
                r#"<a href="{}" target="_blank" rel="noopener">spark viewer ↗</a>"#,
                esc(v)
            )
        })
        .unwrap_or_default();
    format!(
        r#"<div class="topbar"><button class="ghost tg" id="sidetoggle" type="button" title="{}">{}</button><div class="crumb"><a href="./" id="crumbhome">Spark-Analyzer</a><span class="sep">›</span><b>{}</b></div><div class="tr">{}</div></div>"#,
        crate::i18n::t("Mostra/nascondi barra laterale", "Show/hide sidebar"),
        icon("sidebar"),
        esc(page),
        right
    )
}

pub(crate) fn lang_to() -> &'static str {
    if crate::i18n::is_en() { "it" } else { "en" }
}

pub(crate) fn icon(name: &str) -> String {
    let d = match name {
        "activity" => "M3 12h4l3 8l4-16l3 8h4",
        "clock" => "M3 12a9 9 0 1 0 18 0a9 9 0 0 0-18 0M12 7v5l3 3",
        "bolt" => "M13 3v7h6l-8 11v-7H5l8-11",
        "cpu" => {
            "M5 5h14v14H5zM9 9h6v6H9zM3 10h2M3 14h2M19 10h2M19 14h2M10 3v2M14 3v2M10 19v2M14 19v2"
        }
        "alert" => {
            "M12 9v4M12 17h.01M10.24 3.96L1.82 18.02A2 2 0 0 0 3.52 21h16.85a2 2 0 0 0 1.7-2.98L13.64 3.96a2 2 0 0 0-3.4 0z"
        }
        "line" => "M4 19h16M4 15l4-6l4 2l4-5l4 4",
        "users" => {
            "M5 7a4 4 0 1 0 8 0a4 4 0 1 0-8 0M3 21v-2a4 4 0 0 1 4-4h4a4 4 0 0 1 4 4v2M16 3.13a4 4 0 0 1 0 7.75M21 21v-2a4 4 0 0 0-3-3.85"
        }
        "hourglass" => {
            "M6.5 7h11M6.5 17h11M6 20v-2a6 6 0 1 1 12 0v2a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1zM6 4v2a6 6 0 1 0 12 0V4a1 1 0 0 0-1-1H7a1 1 0 0 0-1 1z"
        }
        "clipboard" => {
            "M9 5H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2M9 5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2a2 2 0 0 1-2 2h-2a2 2 0 0 1-2-2zM9 14l2 2l4-4"
        }
        "pie" => {
            "M10 3.2a9 9 0 1 0 10.8 10.8a1 1 0 0 0-1-1h-6.8a2 2 0 0 1-2-2V4a.9.9 0 0 0-1-.8M15 3.5A9 9 0 0 1 20.5 9H16a1 1 0 0 1-1-1V3.5"
        }
        "plug" => {
            "M9.79 6L18 14.21l-2.05 2.06a5.81 5.81 0 1 1-8.22-8.22L9.79 6zM4 20l3.5-3.5M15 4l-3.5 3.5M20 9l-3.5 3.5"
        }
        "flame" => {
            "M12 12c2-2.96 0-7-1-8c0 3.04-1.77 4.74-3 6c-1.23 1.26-2 3.24-2 5a6 6 0 1 0 12 0c0-1.53-1.06-3.94-2-5c-1.79 3-2.79 3-4 2z"
        }
        "list" => "M9 6h11M9 12h11M9 18h11M5 6v.01M5 12v.01M5 18v.01",
        "cube" => "M12 3l8 4.5v9L12 21l-8-4.5v-9L12 3M12 12l8-4.5M12 12v9M12 12L4 7.5",
        "stack" => "M12 4L4 8l8 4l8-4l-8-4M4 12l8 4l8-4M4 16l8 4l8-4",
        "world" => {
            "M3 12a9 9 0 1 0 18 0a9 9 0 0 0-18 0M3.6 9h16.8M3.6 15h16.8M11.5 3a17 17 0 0 0 0 18M12.5 3a17 17 0 0 1 0 18"
        }
        "external" => {
            "M12 6H6a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-6M11 13l9-9M15 4h5v5"
        }
        "tree" => {
            "M5 4v12a2 2 0 0 0 2 2h4M5 9h6M15 7a2 2 0 1 0 4 0a2 2 0 1 0-4 0M15 12a2 2 0 1 0 4 0a2 2 0 1 0-4 0M15 18a2 2 0 1 0 4 0a2 2 0 1 0-4 0M11 9h4M11 18h4"
        }
        "sidebar" => "M4 5a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1zM9 4v16",
        "moon" => "M12 3h.39a7.5 7.5 0 0 0 7.92 12.45A9 9 0 1 1 12 3z",
        "upload" => "M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2M7 9l5-5l5 5M12 4v12",
        "link" => {
            "M9 15l6-6M11 6l.46-.54a5 5 0 0 1 7.07 7.07l-.53.47M13 18l-.4.53a5.07 5.07 0 0 1-7.12 0a4.97 4.97 0 0 1 0-7.07l.52-.46"
        }
        "plus" => "M12 5v14M5 12h14",
        "download" => "M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2M7 11l5 5l5-5M12 4v12",
        "history" => "M12 8v4l2 2M3.05 11a9 9 0 1 1 .5 4m-.5 5v-5h5",
        _ => "M12 12h.01",
    };
    format!(
        r#"<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="{d}"/></svg>"#
    )
}

pub fn render(r: &Report) -> String {
    let m = &r.meta;
    let a = &r.analysis;
    let mut b = String::new();
    let title = format!(
        "Spark-Analyzer — {}",
        if m.platform.is_empty() {
            "profilo spark"
        } else {
            &m.platform
        }
    );

    let crit = r
        .findings
        .iter()
        .filter(|f| f.severity <= Severity::Alto)
        .count();
    let mut cards = String::new();
    let mut card = |ic: &str, label: &str, value: String, suffix: &str, cls: &str| {
        let _ = write!(
            cards,
            r#"<div class="stat {cls}"><div class="si">{}</div><div><div class="sl">{}</div><div class="sv">{}<small>{}</small></div></div></div>"#,
            icon(ic),
            esc(label),
            value,
            esc(suffix)
        );
    };
    if let Some(t) = m.tps {
        card(
            "activity",
            "TPS (1m)",
            fmt_num(t[0]),
            " / 20",
            if t[0] < 19.5 { "bad" } else { "ok" },
        );
    }
    if let Some(ms) = m.mspt_mean {
        card(
            "clock",
            "MSPT medio",
            fmt_num(ms),
            " ms",
            if ms > 45.0 { "bad" } else { "ok" },
        );
    }
    if let Some(mx) = m.mspt_max {
        card(
            "bolt",
            "MSPT massimo",
            fmt_num(mx),
            " ms",
            if mx > 50.0 { "bad" } else { "ok" },
        );
    }
    if let Some(bt) = a.main.busy_per_tick {
        card(
            "cpu",
            "Lavoro main thread",
            fmt_num(bt),
            " ms/tick",
            if bt > 45.0 { "bad" } else { "ok" },
        );
    }
    card(
        "alert",
        "Problemi gravi",
        crit.to_string(),
        "",
        if crit > 0 { "bad" } else { "ok" },
    );
    card(
        "line",
        "Lag spike",
        a.spikes.len().to_string(),
        &format!(" / {} finestre", a.windows.len()),
        if a.spikes.is_empty() { "ok" } else { "wrn" },
    );
    card("users", "Giocatori", m.players.to_string(), "", "");
    card(
        "hourglass",
        "Durata profilo",
        fmt_duration(m.duration_s),
        "",
        "",
    );

    let lag = r.findings.iter().any(|f| {
        f.area == crate::i18n::t("Panoramica", "Overview")
            && f.title == crate::i18n::t("Il server mostra lag", "The server is lagging")
    });
    let nav_items: [(&str, &str, &str); 10] = [
        ("diagnosi", "clipboard", "Diagnosi"),
        ("tempo", "pie", "Tempo main thread"),
        ("plugin", "plug", "Plugin e mod"),
        ("spike", "line", "Lag spike"),
        ("flame", "flame", "Flamegraph"),
        ("albero", "tree", "Albero chiamate"),
        ("metodi", "list", "Metodi"),
        ("entita", "cube", "Entità"),
        ("thread", "stack", "Thread"),
        ("mondo", "world", "Mondo"),
    ];
    let mut nav = String::new();
    for (id, ic, label) in nav_items {
        let _ = write!(
            nav,
            r##"<a href="#{id}">{}<span>{label}</span></a>"##,
            icon(ic)
        );
    }
    let mc = if m.minecraft_version.is_empty() {
        String::new()
    } else {
        format!("MC {}", m.minecraft_version)
    };
    let _ = write!(
        b,
        r#"<div class="shell"><aside class="side"><a class="brand" id="brandlink" href="./" title="Home"><span class="logo">{}</span><span>Spark-Analyzer</span></a><div class="status {}"><i></i>{}</div><nav>{nav}</nav><div class="sidefoot"><div class="srv"><b>{}</b><span>{}</span></div><button id="langbtn" class="ghost lang" data-to="{}" title="Lingua / Language">{}</button><button id="theme" class="ghost" title="Tema chiaro/scuro">{}</button></div></aside><main class="content">{}"#,
        LOGO_SVG,
        if lag { "lag" } else { "okk" },
        if lag { "In lag" } else { "Stabile" },
        esc(if m.platform.is_empty() {
            "Server"
        } else {
            &m.platform
        }),
        esc(&mc),
        lang_to(),
        lang_to().to_ascii_uppercase(),
        icon("moon"),
        topbar(
            crate::i18n::t("Analisi profilo", "Profile analysis"),
            m.viewer_url.as_deref()
        )
    );
    let _ = write!(
        b,
        r#"<div class="top"><div><h1>Analisi profilo</h1><p class="sub">{}</p></div><div class="actions">{}</div></div>"#,
        esc(&format!(
            "{} · {} · {} · spark {} · inizio {} · {} plugin/mod{}",
            m.kind,
            m.mode,
            m.engine,
            m.spark_version,
            fmt_utc(m.start_time),
            m.plugin_count,
            if m.comment.is_empty() { String::new() } else { format!(" · «{}»", m.comment) }
        )),
        format!(
            r#"<button type="button" class="btn sec" id="copylink" hidden data-ok="{}" data-label="{}">{}<span>{}</span></button>{}"#,
            crate::i18n::t("Link copiato!", "Link copied!"),
            crate::i18n::t("Copia link", "Copy link"),
            icon("link"),
            crate::i18n::t("Copia link", "Copy link"),
            m.viewer_url
                .as_ref()
                .map(|v| format!(r#"<a class="btn" href="{0}" target="_blank" rel="noopener">{1}Apri nel viewer spark</a>"#, esc(v), icon("external")))
                .unwrap_or_default()
        )
    );
    if a.unit == "ms" && a.main.busy > 0.0 {
        let (factor, unit) = match a.main.busy_per_tick {
            Some(bt) => (bt / 100.0, "ms/tick"),
            None => (a.main.busy / 100.0, "ms"),
        };
        let _ = write!(
            b,
            r#"<div class="unitbar"><span class="dim">{}</span><div class="seg" id="unitsel" data-f="{factor}" data-u="{unit}"><button type="button" data-m="pct" class="on">%</button><button type="button" data-m="ms">{unit}</button></div></div>"#,
            crate::i18n::t("Mostra i consumi in", "Show usage as")
        );
    }
    if let Some(sel) = &m.window_filter {
        let list: Vec<String> = sel.iter().map(|i| format!("#{}", i + 1)).collect();
        let _ = write!(
            b,
            r#"<div class="fbanner">{}<span>{}</span><button type="button" class="chip needs-filter" data-w="" hidden>{}</button></div>"#,
            icon("line"),
            esc(&tf!(
                "Analisi limitata a {} finestre su {} ({}): i valori si riferiscono solo a questo periodo.",
                "Analysis limited to {} of {} windows ({}): values refer to this period only.",
                sel.len(),
                m.windows_total,
                list.join(", ")
            )),
            crate::i18n::t("Mostra tutto il profilo", "Show the whole profile")
        );
    }
    let _ = write!(b, r#"<section class="stats">{cards}</section>"#);
    if let Some(th) = m.ticked_threshold {
        let _ = write!(
            b,
            r#"<p class="note">Profilo registrato con <code>--only-ticks-over {th}</code>: contiene solo i tick più lenti di {th} ms.</p>"#
        );
    }

    let hidable = r
        .findings
        .iter()
        .filter(|f| {
            f.severity == Severity::Info && f.area != crate::i18n::t("Panoramica", "Overview")
        })
        .count();
    b.push_str(r#"<section id="diagnosi"><h2>Diagnosi</h2>"#);
    if hidable > 0 {
        let _ = write!(
            b,
            r#"<div class="filters"><label><input type="checkbox" id="showinfo"> mostra anche le note informative ({hidable})</label></div>"#
        );
    }
    for f in &r.findings {
        let hide =
            if f.severity == Severity::Info && f.area != crate::i18n::t("Panoramica", "Overview") {
                " hidable"
            } else {
                ""
            };
        let open =
            if f.severity <= Severity::Alto || f.area == crate::i18n::t("Panoramica", "Overview") {
                " open"
            } else {
                ""
            };
        let impact = if f.impact_pct > 0.0 {
            format!(r#"<span class="imp">{}</span>"#, pvh(f.impact_pct))
        } else {
            String::new()
        };
        let _ = write!(
            b,
            r#"<details class="finding {}{hide}"{open}><summary><span class="sev">{}</span><span class="ft">{}</span><span class="area">{}</span>{impact}</summary><div class="fb">"#,
            sev_class(f.severity),
            f.severity.label(),
            esc(&f.title),
            esc(&f.area)
        );
        if !f.evidence.is_empty() {
            b.push_str("<ul class=\"ev\">");
            for e in &f.evidence {
                let _ = write!(b, "<li>{}</li>", pv_html(&esc(e)));
            }
            b.push_str("</ul>");
        }
        if !f.culprits.is_empty() {
            let _ = write!(
                b,
                r#"<p class="cul"><b>Responsabili:</b> {}</p>"#,
                pv_html(&esc(&f.culprits.join(", ")))
            );
        }
        if !f.explanation.is_empty() {
            let _ = write!(b, r#"<p class="why">{}</p>"#, esc(&f.explanation));
        }
        if !f.solutions.is_empty() {
            b.push_str("<h4>Cosa fare</h4><ol class=\"sol\">");
            for s in &f.solutions {
                if let Some((head, rest)) = s.split_once("\n    - ") {
                    let items: Vec<String> = std::iter::once(rest)
                        .flat_map(|r| r.split("\n    - "))
                        .map(|x| format!("<li><code>{}</code></li>", esc(x)))
                        .collect();
                    let _ = write!(b, "<li>{}<ul>{}</ul></li>", esc(head), items.join(""));
                } else {
                    let _ = write!(b, "<li>{}</li>", esc(s));
                }
            }
            b.push_str("</ol>");
        }
        b.push_str("</div></details>");
    }
    b.push_str("</section>");

    if a.main.total > 0.0 {
        let per_tick = a.main.ticks > 0 && a.unit == "ms";
        let _ = write!(
            b,
            r#"<section id="tempo"><h2>Dove va il tempo del main thread</h2><p class="note">Thread «{}»: attivo {} su {} campionati ({}%). Le percentuali sono calcolate sul tempo attivo, esclusa l'attesa tra un tick e l'altro. Ogni riga attribuisce il tempo alla categoria più specifica in cui si trova lo stack.</p>"#,
            esc(&a.main.thread),
            fmt_time(a.main.busy, &a.unit),
            fmt_time(a.main.total, &a.unit),
            fmt_num(a.main.busy_pct)
        );
        let _ = write!(
            b,
            r#"<table class="sortable"><thead><tr><th>Categoria</th><th class="n"><span class="uh">%</span></th><th></th><th>Plugin coinvolti</th><th>Metodi principali</th></tr></thead><tbody>"#
        );
        let _ = per_tick;
        for c in a.categories.iter().filter(|c| c.pct >= 0.1) {
            let owners: Vec<String> = c
                .top_owners
                .iter()
                .take(3)
                .filter(|o| o.pct >= 0.1)
                .map(|o| format!("{} {}", esc(&o.name), pvh(o.pct)))
                .collect();
            let ms: Vec<String> = c
                .top_methods
                .iter()
                .take(3)
                .map(|x| format!("<code>{}</code> {}", esc(&short(&x.name)), pvh(x.pct)))
                .collect();
            let _ = write!(
                b,
                r#"<tr><td>{}</td><td class="n">{}</td><td>{}</td><td>{}</td><td class="small">{}</td></tr>"#,
                esc(&c.label),
                pvh(c.pct),
                pct_bar(c.pct),
                owners.join("<br>"),
                ms.join("<br>")
            );
        }
        b.push_str("</tbody></table>");
        if !a.buckets.is_empty() {
            let bk: Vec<String> = a
                .buckets
                .iter()
                .filter(|x| x.pct >= 0.3)
                .map(|x| format!("{} <b>{}</b>", esc(&x.name), pvh(x.pct)))
                .collect();
            let _ = write!(
                b,
                r#"<p class="note">Tempo fuori dai plugin: {}</p>"#,
                bk.join(" · ")
            );
        }
        b.push_str("</section>");

        let _ = write!(
            b,
            r#"<section id="plugin"><h2>Plugin e mod</h2><p class="note">«Nello stack» = tutto il lavoro avvenuto mentre il codice del plugin era attivo (incluse le chiamate a Minecraft fatte dal plugin). «Proprio» = tempo attribuito al plugin come chiamante più interno.</p>"#
        );
        if a.sources.is_empty() {
            b.push_str("<p>Nessun plugin/mod rilevato sul main thread (oppure il profilo non contiene le informazioni sulle sorgenti).</p>");
        } else {
            let _ = write!(
                b,
                r#"<table class="sortable"><thead><tr><th>Plugin</th><th class="n">{}</th><th></th><th class="n">{}</th><th>Punti di ingresso</th><th>Avvisi</th></tr></thead><tbody>"#,
                crate::i18n::t("Nello stack", "On stack"),
                crate::i18n::t("Proprio", "Own")
            );
            for s in a.sources.iter().filter(|s| s.incl_pct >= 0.05) {
                let entries: Vec<String> = s
                    .top_entries
                    .iter()
                    .take(4)
                    .map(|e| {
                        format!(
                            "<code>{}</code> <span class=\"ctx\">{}</span> {}",
                            esc(&short(&e.entry)),
                            esc(&e.context),
                            pvh(e.pct)
                        )
                    })
                    .collect();
                let hz: Vec<String> = s
                    .hazards
                    .iter()
                    .filter(|h| h.pct >= 0.1)
                    .map(|h| {
                        format!(
                            "<span class=\"warn\">{} {}</span>",
                            esc(&h.name),
                            pvh(h.pct)
                        )
                    })
                    .collect();
                let _ = write!(
                    b,
                    r##"<tr><td><b>{}</b><br><a class="totree small" href="#albero" data-q="[{}]">{}</a></td><td class="n">{}</td><td>{}</td><td class="n">{}</td><td class="small">{}</td><td class="small">{}</td></tr>"##,
                    esc(&s.display),
                    esc(&s.source),
                    crate::i18n::t("vedi nell'albero →", "show in tree →"),
                    pvh(s.incl_pct),
                    pct_bar(s.incl_pct),
                    pvh(s.self_pct),
                    entries.join("<br>"),
                    hz.join("<br>")
                );
            }
            b.push_str("</tbody></table>");
        }
        if !a.events.is_empty() {
            b.push_str(r#"<h3>Eventi Bukkit</h3><table class="sortable"><thead><tr><th>Evento</th><th>Plugin</th><th class="n"><span class="uh">%</span></th></tr></thead><tbody>"#);
            for e in a.events.iter().take(40) {
                let _ = write!(
                    b,
                    r#"<tr><td>{}</td><td>{}</td><td class="n">{}</td></tr>"#,
                    esc(&e.entry),
                    esc(&e.source),
                    pvh(e.pct)
                );
            }
            b.push_str("</tbody></table>");
        }
        b.push_str("</section>");

        let _ = write!(b, r#"<section id="spike"><h2>Andamento e lag spike</h2>"#);
        if m.windows_total > 1 {
            let spikes: Vec<String> = if m.window_filter.is_some() {
                Vec::new()
            } else {
                a.spikes
                    .iter()
                    .map(|sp| sp.window.index.to_string())
                    .collect()
            };
            let _ = write!(
                b,
                r#"<div class="filterbar needs-filter" hidden><span class="dim">{}</span><button type="button" class="chip" data-w="">{}</button>{}<span class="dim small">{}</span></div>"#,
                crate::i18n::t("Periodo:", "Period:"),
                crate::i18n::t("Tutto il profilo", "Whole profile"),
                if spikes.is_empty() {
                    format!(
                        r#"<button type="button" class="chip" disabled title="{0}">{1}</button><span class="dim small">{0}</span>"#,
                        if m.window_filter.is_some() {
                            crate::i18n::t(
                                "stai già guardando un periodo ristretto",
                                "you are already viewing a narrowed period",
                            )
                        } else {
                            crate::i18n::t(
                                "nessun lag spike rilevato in questo profilo",
                                "no lag spikes detected in this profile",
                            )
                        },
                        crate::i18n::t("Solo i lag spike", "Lag spikes only")
                    )
                } else {
                    format!(
                        r#"<button type="button" class="chip" data-w="{}">{}</button>"#,
                        spikes.join(","),
                        crate::i18n::t("Solo i lag spike", "Lag spikes only")
                    )
                },
                crate::i18n::t(
                    "oppure trascina sul grafico per scegliere i minuti",
                    "or drag on the chart to pick the minutes"
                )
            );
        }
        match timeline_svg(r) {
            Some(svg) => b.push_str(&svg),
            None => b.push_str(r#"<p class="note">Il profilo contiene una sola finestra temporale (spark divide i dati in blocchi da 1 minuto): registra almeno 3-5 minuti per vedere l'andamento e usare il filtro per periodo.</p>"#),
        }
        if let Some(p) = &a.periodic_spikes {
            let _ = write!(b, r#"<p class="callout">{}</p>"#, esc(p));
        }
        for s in a.spikes.iter().take(8) {
            let _ = write!(
                b,
                r#"<div class="spike"><h4>{} <small>{}</small></h4><div class="grid3">"#,
                esc(&crate::rules::window_label(&s.window)),
                esc(&s.reason)
            );
            let list = |title: &str,
                        ds: &[crate::analysis::Delta],
                        unit: &str,
                        f: &dyn Fn(&str) -> String|
             -> String {
                let mut x = format!("<div><h5>{}</h5><table><tbody>", esc(title));
                for d in ds.iter().take(5) {
                    let _ = write!(
                        x,
                        r#"<tr><td>{}</td><td class="n">+{}</td><td class="n dim">{} → {}</td></tr>"#,
                        f(&d.name),
                        fmt_num(d.delta),
                        fmt_num(d.baseline),
                        fmt_num(d.value)
                    );
                }
                let _ = write!(
                    x,
                    "</tbody></table><p class=\"dim small\">{}</p></div>",
                    esc(unit)
                );
                x
            };
            b.push_str(&list("Categorie cresciute", &s.cat_deltas, &s.unit, &|n| {
                esc(n)
            }));
            b.push_str(&list("Plugin cresciuti", &s.owner_deltas, &s.unit, &|n| {
                esc(n)
            }));
            b.push_str(&list("Metodi cresciuti", &s.method_deltas, &s.unit, &|n| {
                format!("<code>{}</code>", esc(&short(n)))
            }));
            b.push_str("</div></div>");
        }
        b.push_str("</section>");

        let flame = flame_json(&a.folded);
        let _ = write!(
            b,
            r#"<section id="flame"><h2>Flamegraph del main thread</h2><p class="note">Solo tempo attivo. Clic su un blocco per zoomare, clic sulla radice per tornare indietro. Larghezza = tempo.</p><div class="fbar"><input id="fsearch" placeholder="Cerca metodo o plugin (es. HopperBlockEntity, MyPlugin)"><span id="fmatch"></span><button id="freset">Reset</button></div><div id="fwrap"><canvas id="flamec"></canvas><div id="ftip"></div></div><script type="application/json" id="flamedata">{}</script></section>"#,
            flame.replace("</", "<\\/")
        );
        let _ = write!(
            b,
            r#"<section id="albero"><h2>{}</h2><p class="note">{}</p><div class="fbar"><input id="tsearch" placeholder="{}"><span id="tmatch" class="dim small"></span><button type="button" id="theavy">{}</button><button type="button" id="tcollapse">{}</button></div><div id="tree" class="tree" data-more="{}" data-self="{}"></div></section>"#,
            crate::i18n::t("Albero delle chiamate", "Call tree"),
            crate::i18n::t(
                "Come nel viewer di spark: clic su una riga per espandere i metodi chiamati. Il valore è il tempo totale passato in quel ramo (solo tempo attivo del main thread).",
                "Like the spark viewer: click a row to expand the methods it calls. The value is the total time spent in that branch (main thread active time only)."
            ),
            crate::i18n::t(
                "Cerca metodo, classe o [Plugin]…",
                "Search method, class or [Plugin]…"
            ),
            crate::i18n::t(
                "Espandi il percorso più pesante",
                "Expand the heaviest path"
            ),
            crate::i18n::t("Chiudi tutto", "Collapse all"),
            crate::i18n::t("rami minori nascosti", "smaller branches hidden"),
            crate::i18n::t("proprio", "self")
        );

        let _ = write!(
            b,
            r#"<section id="metodi"><h2>Metodi più costosi</h2><input class="tfilter" data-t="mt" placeholder="Filtra…"><table class="sortable" id="mt"><thead><tr><th>Metodo</th><th class="n">{}</th><th class="n">{}</th><th>Sorgente</th></tr></thead><tbody>"#,
            crate::i18n::t("Proprio", "Own"),
            crate::i18n::t("Totale", "Total")
        );
        for mm in &a.methods {
            let _ = write!(
                b,
                r#"<tr><td><code title="{}">{}</code></td><td class="n">{}</td><td class="n">{}</td><td>{}</td></tr>"#,
                esc(&mm.method),
                esc(&short(&mm.method)),
                pvh(mm.self_pct),
                pvh(mm.incl_pct),
                esc(&mm.source.clone().unwrap_or_default())
            );
        }
        b.push_str("</tbody></table>");
        let _ = write!(
            b,
            r#"<h3>Metodi per tempo totale (inclusi i chiamati)</h3><table class="sortable"><thead><tr><th>Metodo</th><th class="n">{}</th><th class="n">{}</th><th>Sorgente</th></tr></thead><tbody>"#,
            crate::i18n::t("Totale", "Total"),
            crate::i18n::t("Proprio", "Own")
        );
        for mm in a.methods_inclusive.iter().take(40) {
            let _ = write!(
                b,
                r#"<tr><td><code title="{}">{}</code></td><td class="n">{}</td><td class="n">{}</td><td>{}</td></tr>"#,
                esc(&mm.method),
                esc(&short(&mm.method)),
                pvh(mm.incl_pct),
                pvh(mm.self_pct),
                esc(&mm.source.clone().unwrap_or_default())
            );
        }
        b.push_str("</tbody></table>");
        b.push_str("<h3>Percorsi più caldi</h3><ol class=\"paths\">");
        for h in a.hot_paths.iter().take(15) {
            let tag = [h.cat.clone(), h.owner.clone()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
            let _ = write!(
                b,
                r#"<li><b>{}</b> <span class="dim">{}</span><div class="stack">{}</div></li>"#,
                pvh(h.pct),
                esc(&tag),
                h.frames
                    .iter()
                    .map(|f| format!("<code>{}</code>", esc(f)))
                    .collect::<Vec<_>>()
                    .join(" → ")
            );
        }
        b.push_str("</ol></section>");

        let _ = write!(
            b,
            r#"<section id="entita"><h2>Entità e block entity</h2><div class="grid2">"#
        );
        for (title, list) in [
            ("Entità (per tipo)", &a.entity_types),
            ("Block entity (per tipo)", &a.block_entity_types),
        ] {
            let _ = write!(
                b,
                r#"<div><h3>{}</h3><table class="sortable"><thead><tr><th>Tipo</th><th class="n"><span class="uh">%</span></th><th></th></tr></thead><tbody>"#,
                title
            );
            for e in list.iter().take(25) {
                let _ = write!(
                    b,
                    r#"<tr><td>{}</td><td class="n">{}</td><td>{}</td></tr>"#,
                    esc(&e.name),
                    pvh(e.pct),
                    pct_bar(e.pct * 2.0)
                );
            }
            if list.is_empty() {
                b.push_str("<tr><td colspan=3 class=dim>nessun dato</td></tr>");
            }
            b.push_str("</tbody></table></div>");
        }
        b.push_str("</div></section>");

        let _ = write!(
            b,
            r#"<section id="thread"><h2>Thread</h2><table class="sortable"><thead><tr><th>Thread</th><th class="n">Attivo</th><th class="n">% del campionato</th><th>Lavoro principale</th><th>Nota</th></tr></thead><tbody>"#
        );
        for t in a.threads.iter().take(40) {
            let tm: Vec<String> = t
                .top_methods
                .iter()
                .map(|m| format!("<code>{}</code> {}%", esc(&m.name), fmt_num(m.pct)))
                .collect();
            let note = [t.plugin_hint.clone(), t.top_owner.clone()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
            let _ = write!(
                b,
                r#"<tr><td>{}</td><td class="n" data-v="{}">{}</td><td class="n">{}</td><td class="small">{}</td><td>{}</td></tr>"#,
                esc(&t.name),
                t.busy,
                fmt_time(t.busy, &a.unit),
                fmt_num(t.busy_pct_of_all),
                tm.join("<br>"),
                esc(&note)
            );
        }
        b.push_str("</tbody></table></section>");
    }

    let _ = write!(b, r#"<section id="mondo"><h2>Mondo</h2>"#);
    if r.hot_chunks.is_empty() {
        b.push_str("<p class=\"note\">Il profilo non contiene statistiche sul mondo.</p>");
    } else {
        b.push_str(r#"<table class="sortable"><thead><tr><th>Mondo</th><th class="n">x</th><th class="n">z</th><th class="n">Entità</th><th>Tipi principali</th><th>Teletrasporto</th></tr></thead><tbody>"#);
        for h in &r.hot_chunks {
            let ty: Vec<String> = h
                .top
                .iter()
                .map(|(k, v)| format!("{} {}", esc(k), v))
                .collect();
            let _ = write!(
                b,
                r#"<tr><td>{}</td><td class="n">{}</td><td class="n">{}</td><td class="n">{}</td><td>{}</td><td><code class="copy">/tp @s {} 100 {}</code></td></tr>"#,
                esc(&h.world),
                h.block_x,
                h.block_z,
                h.total,
                ty.join(", "),
                h.block_x,
                h.block_z
            );
        }
        b.push_str("</tbody></table>");
    }
    if !r.plugins.is_empty() {
        b.push_str("<h3>Plugin / mod installati</h3><p class=\"plist\">");
        let list: Vec<String> = r
            .plugins
            .iter()
            .map(|(n, v)| format!("<span>{} <i>{}</i></span>", esc(n), esc(v)))
            .collect();
        b.push_str(&list.join(""));
        b.push_str("</p>");
    }
    b.push_str("</section>");
    b.push_str("</main></div>");

    for (id, ic) in [
        ("diagnosi", "clipboard"),
        ("tempo", "pie"),
        ("plugin", "plug"),
        ("spike", "line"),
        ("flame", "flame"),
        ("albero", "tree"),
        ("metodi", "list"),
        ("entita", "cube"),
        ("thread", "stack"),
        ("mondo", "world"),
    ] {
        b = b.replace(
            &format!(r#"<section id="{id}"><h2>"#),
            &format!(
                r#"<section id="{id}" class="panel"><h2><span class="hi">{}</span>"#,
                icon(ic)
            ),
        );
    }
    let page = format!(
        "<!doctype html><html lang=\"it\" data-theme=\"dark\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">{}<title>{}</title><style>{}</style></head><body>{}<script>{}{}</script></body></html>",
        FAVICON,
        esc(&title),
        CSS,
        b,
        ANIM_JS,
        JS
    );
    if crate::i18n::is_en() {
        super::localize::to_english(&page)
    } else {
        page
    }
}

pub(crate) const CSS: &str = r#"
:root,[data-theme=dark]{--bg:#0e0e0e;--side:#0e0e0e;--card:#141414;--card2:#1a1a1a;--card3:#232323;--line:#262626;--line2:#1e1e1e;--fg:#d4d4d4;--hd:#fafafa;--muted:#8a8a8a;--acc:#38bdf8;--acc2:#0ea5e9;--accl:rgba(56,189,248,.12);--crit:#ef4444;--high:#f97316;--med:#eab308;--low:#38bdf8;--info:#737373;--ok:#a3e635;--bar:#38bdf8;--spike:rgba(239,68,68,.12);--code:#1a1a1a;--shadow:none;--fl-text:#111;--glass:rgba(14,14,14,.82)}
[data-theme=light]{--bg:#fafafa;--side:#fafafa;--card:#ffffff;--card2:#f5f5f5;--card3:#ebebeb;--line:#e5e5e5;--line2:#efefef;--fg:#404040;--hd:#0a0a0a;--muted:#737373;--acc:#0284c7;--acc2:#0369a1;--accl:rgba(2,132,199,.1);--crit:#dc2626;--high:#ea580c;--med:#ca8a04;--low:#0284c7;--info:#737373;--ok:#65a30d;--bar:#0284c7;--spike:rgba(220,38,38,.08);--code:#f5f5f5;--shadow:none;--fl-text:#111;--glass:rgba(250,250,250,.85)}
*{box-sizing:border-box}html{scroll-behavior:smooth;scroll-padding-top:16px}body{margin:0;background:var(--bg);color:var(--fg);font:14px/1.55 Helvetica,Arial,system-ui,sans-serif;-webkit-font-smoothing:antialiased}
a{color:var(--acc);text-decoration:none}a:hover{text-decoration:underline}
.shell{display:flex;gap:12px;padding:8px;min-height:100vh}
.side{position:sticky;top:8px;align-self:flex-start;width:224px;flex:0 0 224px;height:calc(100vh - 16px);background:var(--side);border:1px solid var(--line);border-radius:8px;display:flex;flex-direction:column;padding:10px}
.brand{text-decoration:none!important;display:flex;align-items:center;gap:10px;font-weight:700;color:var(--hd);font-size:15px;padding:6px 6px 12px;border-bottom:1px solid var(--line2)}
.logo{display:grid;place-items:center;width:32px;height:32px;flex:none}.logo svg{display:block}
.status{display:flex;align-items:center;gap:8px;margin:12px 6px 8px;font-size:12px;color:var(--muted)}.status i{width:10px;height:10px;border-radius:50%;background:var(--ok);box-shadow:0 0 0 3px rgba(64,192,87,.18)}.status.lag i{background:var(--crit);box-shadow:0 0 0 3px rgba(250,82,82,.2)}
.side nav{display:flex;flex-direction:column;gap:2px;margin-top:4px;padding-top:8px;border-top:1px solid var(--line2);overflow:auto}
.side nav a{display:flex;align-items:center;gap:10px;padding:7px 8px;border-radius:5px;color:var(--fg);font-size:13px}.side nav a:hover{background:var(--card2);text-decoration:none}.side nav a.on{background:var(--accl);color:var(--acc)}.side nav svg{width:16px;height:16px;flex:none;opacity:.85}
.sidefoot{margin-top:auto;display:flex;gap:8px;align-items:center;border:1px solid var(--line);border-radius:6px;padding:8px;background:var(--card)}
.srv{flex:1;min-width:0;display:flex;flex-direction:column;font-size:12px}.srv b{color:var(--hd);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.srv span{color:var(--muted)}
.ghost{background:transparent;border:1px solid var(--line);color:var(--fg);border-radius:5px;width:32px;height:32px;display:grid;place-items:center;cursor:pointer;padding:0}.ghost:hover{background:var(--card2)}.ghost.lang{font-size:11px;font-weight:700;letter-spacing:.04em;color:var(--muted)}
.content{flex:1;min-width:0;padding:6px 8px 0}
.top{display:flex;justify-content:space-between;align-items:flex-start;gap:12px;flex-wrap:wrap;margin:4px 0 16px}
h1{margin:0;font-size:28px;font-weight:600;color:var(--hd);letter-spacing:-.01em}.sub{margin:4px 0 0;color:var(--muted);font-size:13px}
.btn{display:inline-flex;align-items:center;gap:8px;background:var(--acc);color:#fff;border-radius:5px;padding:8px 14px;font-size:13px;font-weight:600;border:0}.btn:hover{background:var(--acc2);text-decoration:none}.btn.sec{background:var(--card2);color:var(--fg);border:1px solid var(--line)}.btn.sec:hover{border-color:var(--muted);background:var(--card2)}.actions{display:flex;gap:8px;flex-wrap:wrap}
.stats{display:grid;grid-template-columns:repeat(auto-fill,minmax(210px,1fr));gap:10px;margin-bottom:16px}
.stat{display:flex;align-items:center;gap:12px;background:var(--card);border:1px solid var(--line);border-radius:8px;padding:12px;box-shadow:var(--shadow)}
.si{display:grid;place-items:center;width:40px;height:40px;border-radius:6px;background:var(--acc);color:#fff;flex:none}.si svg{width:22px;height:22px}
.stat.bad .si{background:var(--crit)}.stat.wrn .si{background:var(--med);color:#1f1f1f}.stat.ok .si{background:var(--ok)}
.sl{font-size:12px;color:var(--muted)}.sv{font-size:18px;font-weight:600;color:var(--hd);font-variant-numeric:tabular-nums}.sv small{font-size:12px;color:var(--muted);font-weight:400;margin-left:2px}
section{margin:0 0 14px}
.panel{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:14px 16px;box-shadow:var(--shadow)}
h2{display:flex;align-items:center;gap:8px;font-size:15px;font-weight:600;color:var(--hd);margin:0 0 12px}.hi{display:inline-flex;color:var(--muted)}.hi svg{width:16px;height:16px}
h3{font-size:14px;color:var(--hd);margin:18px 0 8px;font-weight:600}h4{margin:12px 0 6px;font-size:13px;color:var(--hd)}h5{margin:0 0 6px;font-size:11px;color:var(--muted);text-transform:uppercase;letter-spacing:.04em}
.note{color:var(--muted);font-size:13px}.callout{background:var(--card2);border:1px solid var(--line);border-left:3px solid var(--med);padding:9px 12px;border-radius:6px}
.finding{background:var(--card2);border:1px solid var(--line);border-left:3px solid var(--info);border-radius:6px;margin:8px 0}
.finding summary{cursor:pointer;padding:11px 14px;display:flex;gap:10px;align-items:center;flex-wrap:wrap;list-style:none}.finding summary::-webkit-details-marker{display:none}.finding summary:hover{background:rgba(127,127,127,.05)}
.finding.crit{border-left-color:var(--crit)}.finding.high{border-left-color:var(--high)}.finding.med{border-left-color:var(--med)}.finding.low{border-left-color:var(--low)}
.sev{font-size:10.5px;font-weight:700;letter-spacing:.04em;padding:2px 8px;border-radius:10px;color:var(--info);background:rgba(134,142,150,.16)}
.crit .sev{color:var(--crit);background:rgba(250,82,82,.15)}.high .sev{color:var(--high);background:rgba(253,126,20,.15)}.med .sev{color:var(--med);background:rgba(250,176,5,.15)}.low .sev{color:var(--low);background:rgba(77,171,247,.15)}
.ft{font-weight:600;color:var(--hd);flex:1;min-width:200px}.area{color:var(--muted);font-size:11px;text-transform:uppercase;letter-spacing:.04em;background:var(--code);border:1px solid var(--line);padding:1px 8px;border-radius:10px}.imp{font-weight:700;color:var(--hd);font-variant-numeric:tabular-nums}
.fb{padding:0 16px 14px;border-top:1px solid var(--line2)}.ev{margin:10px 0 0;padding-left:18px}.ev li{margin:3px 0}.why{color:var(--muted)}.cul{margin:8px 0}.sol{padding-left:20px}.sol li{margin:5px 0}.sol ul{margin:6px 0;padding-left:16px}
body.hideinfo .finding.hidable{display:none}
table{width:100%;border-collapse:collapse;font-variant-numeric:tabular-nums;font-size:13px}th,td{padding:8px 10px;border-bottom:1px solid var(--line2);text-align:left;vertical-align:top}th{font-size:11px;color:var(--muted);font-weight:600;text-transform:uppercase;letter-spacing:.04em;cursor:pointer;user-select:none;border-bottom:1px solid var(--line)}tbody tr:hover{background:rgba(127,127,127,.06)}tr:last-child td{border-bottom:0}.n{text-align:right;white-space:nowrap}.small{font-size:12px}.dim{color:var(--muted)}
.panel>table,.grid2 table,.panel>div>table{background:var(--card2);border:1px solid var(--line);border-radius:6px;overflow:hidden;border-collapse:separate;border-spacing:0}
code{font:12px ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;background:var(--code);border:1px solid var(--line2);padding:1px 5px;border-radius:4px;word-break:break-all;color:var(--fg)}
.bar{display:inline-block;width:110px;height:6px;background:var(--code);border-radius:3px;overflow:hidden;vertical-align:middle}.bar i{display:block;height:100%;background:var(--bar);border-radius:3px}
.ctx{color:var(--muted);font-size:11px}.unitbar{display:flex;align-items:center;gap:10px;margin:-4px 0 12px;font-size:13px;flex-wrap:wrap}.seg{display:inline-flex;border:1px solid var(--line);border-radius:6px;overflow:hidden;background:var(--card)}.seg button{border:0;border-radius:0;background:transparent;padding:5px 12px;font-size:12px;font-weight:600;color:var(--muted)}.seg button+button{border-left:1px solid var(--line)}.seg button.on{background:var(--acc);color:#fff}.warn{color:var(--high);font-weight:600}
.chart{width:100%;height:auto;background:var(--card2);border:1px solid var(--line);border-radius:6px}.chart .grid{stroke:var(--line2)}.chart .budget{stroke:var(--crit);stroke-dasharray:4 4;opacity:.8}.chart .axis{fill:var(--muted);font-size:11px}.chart polyline{fill:none;stroke-width:2}.chart .l1{stroke:var(--high)}.chart .l2{stroke:var(--acc)}.chart .l3{stroke:var(--ok);stroke-dasharray:3 3}.chart circle{fill:var(--high)}.chart circle.sp{fill:var(--crit)}.chart .spikebg{fill:var(--spike)}.tree{background:var(--card2);border:1px solid var(--line);border-radius:14px;padding:6px 0;max-height:70vh;overflow:auto;font-size:13px}.tn{display:flex;align-items:center;gap:8px;padding:4px 12px 4px 0;cursor:pointer;white-space:nowrap;border-radius:6px}.tn:hover{background:var(--card3)}.tn .cr{width:14px;flex:none;color:var(--muted);text-align:center;transition:transform .15s ease;font-size:10px}.tn.open>.cr{transform:rotate(90deg)}.tn.leaf .cr{opacity:0}.tn .tv{min-width:86px;text-align:right;flex:none;font-variant-numeric:tabular-nums;color:var(--hd)}.tn .tb{width:60px;height:5px;flex:none;background:var(--card3);border-radius:999px;overflow:hidden}.tn .tb i{display:block;height:100%;background:var(--acc);border-radius:999px}.tn .tl{overflow:hidden;text-overflow:ellipsis;font-family:"JetBrains Mono",ui-monospace,Menlo,Consolas,monospace;font-size:12px}.tn .src{color:var(--ok);margin-left:6px}.tn .ts{color:var(--muted);font-size:11px;margin-left:8px}.tn.hit{background:var(--accl)}.tn.hit .tl{color:var(--hd)}.tmore{padding:3px 0 3px 0;color:var(--muted);font-size:11px}.totree{color:var(--acc)}.filterbar{display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin:0 0 12px}.filterbar[hidden],.needs-filter[hidden]{display:none!important}.chip{border-radius:999px;padding:5px 12px;font-size:12px;background:var(--card);border:1px solid var(--line);color:var(--fg)}.chip:hover{border-color:var(--muted);color:var(--hd)}.chip:disabled{opacity:.45;cursor:not-allowed}.chip:disabled:hover{border-color:var(--line);color:var(--fg)}.chip.on{background:var(--card3);color:var(--hd)}.fbanner{display:flex;align-items:center;gap:10px;flex-wrap:wrap;background:var(--accl);border:1px solid var(--acc);color:var(--hd);border-radius:12px;padding:10px 14px;margin:0 0 14px;font-size:13px}.fbanner svg{color:var(--acc);flex:none}.fbanner .chip{margin-left:auto}.chart .brush{fill:var(--accl);stroke:var(--acc);stroke-dasharray:4 3}.chart.brushing .hov{cursor:ew-resize}.chartwrap{position:relative}.chart .hov{fill:transparent;cursor:crosshair}.chart .hov:hover{fill:rgba(127,127,127,.08)}.chart .guide{stroke:var(--muted);stroke-dasharray:3 3}.chart circle.on{r:5;stroke:var(--card);stroke-width:2}.ctip{position:absolute;z-index:4;pointer-events:none;min-width:200px;max-width:300px;background:var(--card);border:1px solid var(--line);border-radius:8px;box-shadow:0 6px 24px rgba(0,0,0,.35);padding:10px 12px;font-size:12px;color:var(--fg)}.ctip b{color:var(--hd)}.ctip .r{display:flex;justify-content:space-between;gap:16px;margin-top:3px}.ctip .r span:last-child{font-variant-numeric:tabular-nums;color:var(--hd)}.ctip .sp{margin-top:7px;padding-top:6px;border-top:1px solid var(--line2);color:var(--crit)}.ctip .cs{color:var(--muted);margin-top:2px}
.legend{color:var(--muted);font-size:12px;margin:8px 0 12px;display:flex;gap:6px 14px;flex-wrap:wrap;align-items:center}.legend span{display:inline-block;width:14px;height:3px;vertical-align:middle;margin-right:4px;border-radius:2px}.k1{background:var(--high)}.k2{background:var(--acc)}.k3{background:var(--ok)}.kb{background:var(--crit)}.ks{background:var(--spike);height:10px!important}
.spike{background:var(--card2);border:1px solid var(--line);border-left:3px solid var(--crit);border-radius:6px;padding:10px 14px;margin:10px 0}.spike h4 small{color:var(--muted);font-weight:400}.grid3{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:12px}.grid3 td{padding:4px 6px}
.grid2{display:grid;grid-template-columns:repeat(auto-fit,minmax(320px,1fr));gap:16px}
.fbar{display:flex;gap:8px;margin-bottom:8px;align-items:center;flex-wrap:wrap}
input#fsearch,input#tsearch,.tfilter{flex:1;min-width:200px;padding:7px 10px;border:1px solid var(--line);border-radius:5px;background:var(--code);color:var(--fg);font:inherit;font-size:13px;outline:none}input#fsearch:focus,input#tsearch:focus,.tfilter:focus{border-color:var(--acc)}.tfilter{width:100%;margin-bottom:8px}
button{padding:7px 12px;border:1px solid var(--line);border-radius:5px;background:var(--card2);color:var(--fg);cursor:pointer;font:inherit;font-size:13px}button:hover{border-color:var(--muted)}#fmatch{color:var(--muted);font-size:12px}
#fwrap{position:relative;background:var(--card2);border:1px solid var(--line);border-radius:6px;overflow:hidden}#flamec{display:block;width:100%}#ftip{position:absolute;pointer-events:none;background:#101010;color:#f1f1f1;border:1px solid #3a3a3a;padding:6px 8px;border-radius:5px;font-size:12px;display:none;max-width:520px;z-index:3}
.paths li{margin:8px 0}.stack{margin-top:4px;line-height:1.9}.plist span{display:inline-block;margin:2px 6px 2px 0;padding:2px 9px;border:1px solid var(--line);border-radius:12px;background:var(--card2);font-size:12px}.plist i{color:var(--muted);font-style:normal}
footer{color:var(--muted);font-size:12px;text-align:right;padding:16px 4px 20px}footer span{color:var(--acc)}.filters{margin-bottom:6px;color:var(--muted);font-size:13px}.copy{cursor:copy}
input[type=checkbox]{accent-color:var(--acc)}
@media (max-width:900px){.shell{flex-direction:column;padding:0;gap:0}.side{position:sticky;top:0;z-index:10;width:auto;align-self:stretch;flex:none;height:auto;flex-direction:row;align-items:center;border-radius:0;border-width:0 0 1px;padding:8px 12px;gap:10px}.brand{border:0;padding:0}.brand span:last-child{display:none}.status{margin:0;white-space:nowrap}.side nav{flex-direction:row;border:0;margin:0;padding:0;overflow-x:auto;flex:1}.side nav a span{display:none}.side nav a{padding:8px}.sidefoot{border:0;padding:0;background:none;margin:0}.srv{display:none}.content{padding:14px 16px 0}h1{font-size:22px}.stats{grid-template-columns:repeat(2,minmax(0,1fr))}.stat{padding:10px;gap:10px}.si{width:34px;height:34px}.sv{font-size:16px}th,td{padding:6px}.bar{width:60px}.panel{padding:12px}}

@keyframes sa-rise{from{opacity:0;transform:translateY(14px)}to{opacity:1;transform:none}}
@keyframes sa-fade{from{opacity:0}to{opacity:1}}
@keyframes sa-grow{from{transform:scaleX(0)}to{transform:scaleX(1)}}
@keyframes sa-draw{from{stroke-dashoffset:var(--len,3000)}to{stroke-dashoffset:0}}
@keyframes sa-pop{0%{transform:scale(.6);opacity:0}70%{transform:scale(1.15)}100%{transform:scale(1);opacity:1}}
@keyframes sa-pulse{0%,100%{box-shadow:0 0 0 3px rgba(250,82,82,.2)}50%{box-shadow:0 0 0 7px rgba(250,82,82,.05)}}
body{transition:background-color .25s ease,color .25s ease}
.panel,.stat,.side,.finding,.spike,.ritem,.sidefoot,table,.chart,#fwrap{transition:background-color .25s ease,border-color .25s ease,transform .2s ease,box-shadow .2s ease}
.top{animation:sa-fade .4s ease both}
.stat{animation:sa-rise .45s cubic-bezier(.2,.7,.2,1) both}
.stats .stat:nth-child(1){animation-delay:.02s}.stats .stat:nth-child(2){animation-delay:.06s}.stats .stat:nth-child(3){animation-delay:.1s}.stats .stat:nth-child(4){animation-delay:.14s}.stats .stat:nth-child(5){animation-delay:.18s}.stats .stat:nth-child(6){animation-delay:.22s}.stats .stat:nth-child(7){animation-delay:.26s}.stats .stat:nth-child(8){animation-delay:.3s}
.stat:hover{transform:translateY(-2px);box-shadow:0 6px 18px rgba(0,0,0,.25);border-color:var(--muted)}
.stat .si{transition:transform .2s ease}.stat:hover .si{transform:scale(1.08) rotate(-4deg)}
.js .reveal{opacity:0;transform:translateY(16px)}
.js .reveal.in{opacity:1;transform:none;transition:opacity .5s ease,transform .5s cubic-bezier(.2,.7,.2,1)}
.finding{transition:border-color .2s ease,background-color .2s ease,transform .15s ease}.finding summary:hover{transform:translateX(2px)}
.finding[open] .fb{animation:sa-fade .3s ease both}
.finding.crit .sev{animation:sa-pop .4s ease both}
@keyframes sa-pulse-ok{0%,100%{box-shadow:0 0 0 3px rgba(64,192,87,.25);opacity:1}50%{box-shadow:0 0 0 7px rgba(64,192,87,.05);opacity:.45}}@keyframes sa-pulse-lag{0%,100%{box-shadow:0 0 0 3px rgba(250,82,82,.3);opacity:1}50%{box-shadow:0 0 0 8px rgba(250,82,82,.05);opacity:.4}}.status i{animation:sa-pulse-ok 1.6s ease-in-out infinite}.status.lag i{animation:sa-pulse-lag 1s ease-in-out infinite}
.logo{transition:transform .25s ease}.brand:hover .logo{transform:rotate(-8deg) scale(1.06)}
.bar i{transform-origin:left center;animation:sa-grow .9s cubic-bezier(.2,.7,.2,1) both}
.js .reveal:not(.in) .bar i{animation:none;transform:scaleX(0)}
.chart polyline{stroke-dasharray:var(--len,3000);animation:sa-draw 1.4s ease-out both}
.js .reveal:not(.in) .chart polyline{animation:none;stroke-dashoffset:var(--len,3000)}
.chart circle{animation:sa-pop .35s ease both;transform-box:fill-box;transform-origin:center}
.chart circle:nth-of-type(odd){animation-delay:.3s}.chart circle:nth-of-type(even){animation-delay:.45s}
.btn,button{transition:background-color .15s ease,border-color .15s ease,transform .1s ease}.btn:active,button:active{transform:scale(.97)}
.side nav a{transition:background-color .15s ease,color .15s ease,padding-left .15s ease}.side nav a:hover{padding-left:12px}
.ritem{transition:transform .15s ease,border-color .15s ease}.ritem:hover{transform:translateX(3px)}
.drop .dicon{transition:transform .25s ease}.drop:hover .dicon,.drop.over .dicon{transform:translateY(-4px)}
.ctip:not([hidden]){animation:sa-fade .15s ease both}
.seg button{transition:background-color .2s ease,color .2s ease}
tbody tr{transition:background-color .12s ease}
@media print{.js .reveal{opacity:1!important;transform:none!important}}
@media (prefers-reduced-motion:reduce){.stat{animation-name:sa-fade}.bar i,.chart polyline,.chart circle,.finding.crit .sev{animation:none!important}.chart polyline{stroke-dashoffset:0!important}.bar i,.js .reveal,.stat:hover,.stat:hover .si,.finding summary:hover,.brand:hover .logo,.ritem:hover,.drop:hover .dicon,.drop.over .dicon,.btn:active,button:active{transform:none!important}.js .reveal.in{transition:opacity .5s ease}}

body{font-family:Inter,"Inter Variable",ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;font-size:14px;letter-spacing:-.005em}
.mono,th,code,.area,.sv small,.sl,.legend,.chart .axis,.ctx,.note code,.ritem div span,h5,.seg button,.sev{font-family:"JetBrains Mono","IBM Plex Mono",ui-monospace,SFMono-Regular,Menlo,Consolas,monospace}
.shell{padding:0;gap:0}
.side{top:0;height:100vh;width:248px;flex-basis:248px;border-radius:0;border-width:0 1px 0 0;padding:14px 12px;background:var(--side);transition:margin-left .25s ease,background-color .25s ease}
body.noside .side{margin-left:-249px}
.brand{border-bottom:0;padding:4px 6px 14px}
.status{margin:4px 6px 12px}
.side nav{border-top:1px solid var(--line);padding-top:10px}
.side nav a{border-radius:8px;color:var(--muted);padding:8px 10px}.side nav a:hover{color:var(--hd);background:var(--card2)}.side nav a.on{background:var(--card3);color:var(--hd)}
.sidefoot{border-radius:12px;background:var(--card);border-color:var(--line)}
.ghost{border-radius:8px;border-color:var(--line);background:var(--card)}
.content{padding:0 32px}
.topbar{position:sticky;top:0;z-index:6;display:flex;align-items:center;gap:14px;height:58px;margin:0 -32px 26px;padding:0 32px;border-bottom:1px solid var(--line);background:var(--glass);backdrop-filter:saturate(1.2) blur(10px);-webkit-backdrop-filter:saturate(1.2) blur(10px)}
.topbar .tg{width:34px;height:34px;border:0;background:transparent;color:var(--fg)}.topbar .tg:hover{background:var(--card2)}
.crumb{display:flex;align-items:center;gap:10px;font-size:15px;color:var(--muted);min-width:0}.crumb a{color:var(--muted)}.crumb a:hover{color:var(--hd);text-decoration:none}.crumb b{color:var(--hd);font-weight:500;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.crumb .sep{opacity:.5}
.topbar .tr{margin-left:auto;display:flex;align-items:center;gap:16px;font-size:14px}.topbar .tr a{color:var(--muted)}.topbar .tr a:hover{color:var(--hd);text-decoration:none}
h1{font-size:36px;font-weight:700;letter-spacing:-.025em}.sub{font-size:15px;margin-top:8px}
.top{margin-bottom:22px}
.btn{background:var(--hd);color:var(--bg);border-radius:10px;padding:9px 16px}.btn:hover{background:var(--fg)}
.btn.sec{background:var(--card);color:var(--hd);border:1px solid var(--line)}.btn[hidden]{display:none!important}.btn.sec.ok{border-color:var(--ok);color:var(--ok)}.btn.sec:hover{background:var(--card2);border-color:var(--muted)}
.stats{gap:12px}
.stat{border-radius:14px;padding:14px;background:var(--card);box-shadow:none}
.stat:hover{box-shadow:none;border-color:var(--card3)}
.si{background:var(--card2);color:var(--acc);border:1px solid var(--line);border-radius:10px}
.stat.ok .si{background:rgba(163,230,53,.1);color:var(--ok);border-color:rgba(163,230,53,.25)}
.stat.bad .si{background:rgba(239,68,68,.1);color:var(--crit);border-color:rgba(239,68,68,.25)}
.stat.wrn .si{background:rgba(249,115,22,.1);color:var(--high);border-color:rgba(249,115,22,.25)}
.sl{font-size:11.5px}.sv{font-size:20px;letter-spacing:-.02em}
.panel{border-radius:18px;padding:18px 20px;background:var(--card);border-color:var(--line)}
h2{font-size:16px;font-weight:600;letter-spacing:-.01em}.hi{color:var(--muted)}
.finding{border-radius:12px;background:var(--card2);border-color:var(--line);border-left-width:3px}
.sev{border-radius:6px;font-size:10px;padding:3px 7px}
.area{border-radius:6px;text-transform:none;letter-spacing:0;font-size:11px}
th{text-transform:none;letter-spacing:0;font-size:12px;font-weight:500;color:var(--muted);padding:12px 14px}td{padding:11px 14px;border-color:var(--line)}
.panel>table,.grid2 table,.panel>div>table{border-radius:14px;background:var(--card2);border-color:var(--line)}
tbody tr:hover{background:var(--card3)}
code{border-radius:6px;background:var(--card3);border-color:transparent}
.bar{height:6px;background:var(--card3);border-radius:999px}.bar i{border-radius:999px}
.seg{border-radius:12px;padding:3px;gap:3px;background:var(--card);border-color:var(--line)}.seg button{border-radius:9px;padding:6px 14px;color:var(--muted)}.seg button+button{border-left:0}.seg button.on{background:var(--card3);color:var(--hd)}
.unitbar{margin:-6px 0 16px}
.status i{background:var(--ok)}
.chart{border-radius:14px;background:var(--card2);border-color:var(--line)}
.chart .l1{stroke:var(--high)}.chart .l2{stroke:var(--acc)}.chart .l3{stroke:var(--ok)}.chart circle{fill:var(--high)}
.k1{background:var(--high)}.k2{background:var(--acc)}.k3{background:var(--ok)}
.spike{border-radius:14px;background:var(--card2)}
#fwrap{border-radius:14px;background:var(--card2)}
input#fsearch,input#tsearch,.tfilter{border-radius:12px;padding:11px 14px;background:var(--card);font-size:14px}
.callout{border-radius:12px}
.ctip{border-radius:12px;background:var(--card2)}
.plist span{border-radius:8px;background:var(--card2)}
footer{text-align:left}
@media (max-width:900px){.side{height:auto;width:auto;flex-basis:auto;border-width:0 0 1px;margin-left:0!important;padding:8px 12px}.side nav{border-top:0;padding-top:0}.brand{padding:0}.sidefoot{border:0;background:none}.topbar{display:none}.content{padding:16px}}
"#;

pub(crate) const ANIM_JS: &str = r#"
(function(){var d=document.documentElement;d.classList.add('js');
try{if(localStorage.getItem('sa-noside')==='1')document.body.classList.add('noside')}catch(e){}var tgb=document.getElementById('sidetoggle');if(tgb)tgb.addEventListener('click',function(){var on=document.body.classList.toggle('noside');try{localStorage.setItem('sa-noside',on?'1':'0')}catch(e){}});
var red=window.matchMedia&&window.matchMedia('(prefers-reduced-motion: reduce)').matches;
document.querySelectorAll('.chart polyline').forEach(function(p){try{var l=Math.ceil(p.getTotalLength())+2;p.style.setProperty('--len',l)}catch(e){}});
var els=document.querySelectorAll('main section.panel, main .spike');
if(!('IntersectionObserver' in window)){return}
Array.from(els).filter(function(el){return el.getBoundingClientRect().top>=window.innerHeight*0.95}).forEach(function(el){el.classList.add('reveal')});
var io=new IntersectionObserver(function(es){es.forEach(function(e){if(e.isIntersecting){e.target.classList.add('in');io.unobserve(e.target)}})},{rootMargin:'0px 0px -8% 0px'});
document.querySelectorAll('.reveal').forEach(function(el){io.observe(el)});
if(!red)document.querySelectorAll('.stat .sv').forEach(function(sv){var n=sv.firstChild;if(!n||n.nodeType!==3)return;var txt=n.nodeValue.trim();if(!/^[0-9]+(\.[0-9]+)?$/.test(txt))return;var target=parseFloat(txt),dec=(txt.split('.')[1]||'').length,t0=null;function step(ts){if(!t0)t0=ts;var k=Math.min(1,(ts-t0)/700),v=target*(1-Math.pow(1-k,3));n.nodeValue=v.toFixed(dec);if(k<1)requestAnimationFrame(step)}n.nodeValue=(0).toFixed(dec);requestAnimationFrame(step)});
})();
"#;

const JS: &str = r#"
(function(){
var docEl=document.documentElement;try{var t=localStorage.getItem('sparkscope-theme');if(t)docEl.dataset.theme=t}catch(e){}
var tb=document.getElementById('theme');if(tb)tb.addEventListener('click',function(){var n=docEl.dataset.theme==='dark'?'light':'dark';docEl.dataset.theme=n;try{localStorage.setItem('sparkscope-theme',n)}catch(e){}if(window.__redrawFlame)window.__redrawFlame()});
var bl=document.getElementById('brandlink'),ch=document.getElementById('crumbhome'),nb=document.querySelector('.actions a.btn.sec');if(nb){if(bl)bl.href=nb.getAttribute('href');if(ch)ch.href=nb.getAttribute('href')}
var cl=document.getElementById('copylink');if(cl){var sh=null;try{var u=new URL(location.href);if(u.searchParams.get('p')){var k=u.searchParams.get('p');u=new URL(location.href);var wq=new URL(location.href).searchParams.get('w');u.search='?p='+encodeURIComponent(k)+(wq?'&w='+encodeURIComponent(wq):'');u.hash='';sh=u.toString()}else if(/\/r\/[^\/]+/.test(u.pathname)&&u.protocol.indexOf('http')===0){u.hash='';sh=u.toString()}}catch(e){}if(sh){cl.hidden=false;cl.addEventListener('click',function(){function done(){var sp=cl.querySelector('span');sp.textContent=cl.dataset.ok;cl.classList.add('ok');setTimeout(function(){sp.textContent=cl.dataset.label;cl.classList.remove('ok')},1800)}function fallback(){var ta=document.createElement('textarea');ta.value=sh;ta.style.position='fixed';ta.style.opacity='0';document.body.appendChild(ta);ta.select();try{document.execCommand('copy')}catch(e){}ta.remove();done()}if(navigator.clipboard&&window.isSecureContext){navigator.clipboard.writeText(sh).then(done,fallback)}else{fallback()}})}}
(function(){var can=!!window.__saFilter||(/^https?:/.test(location.protocol)&&/\/r\/[^\/]+/.test(location.pathname));if(!can)return;function apply(list){if(window.__saFilter){window.__saFilter(list);return}var u=new URL(location.href);if(list)u.searchParams.set('w',list);else u.searchParams.delete('w');location.href=u.toString()}document.querySelectorAll('.needs-filter').forEach(function(e){e.hidden=false});document.querySelectorAll('[data-w]').forEach(function(bt){bt.addEventListener('click',function(){apply(bt.dataset.w||null)})});document.querySelectorAll('.chartwrap').forEach(function(cw){var d;try{d=JSON.parse(cw.querySelector('.chartdata').textContent)}catch(e){return}var svg=cw.querySelector('svg'),br=svg.querySelector('.brush'),hs=svg.querySelectorAll('.hov'),a=null,b=null;function geom(i){var r=hs[i];return [parseFloat(r.getAttribute('x')),parseFloat(r.getAttribute('width'))]}function draw(){if(a===null)return;var lo=Math.min(a,b),hi=Math.max(a,b),g1=geom(lo),g2=geom(hi);br.setAttribute('x',g1[0]);br.setAttribute('width',g2[0]+g2[1]-g1[0]);br.style.display=''}hs.forEach(function(r,i){r.addEventListener('mousedown',function(e){e.preventDefault();a=b=i;svg.classList.add('brushing');draw()});r.addEventListener('mouseenter',function(){if(a!==null){b=i;draw()}})});window.addEventListener('mouseup',function(){if(a===null)return;var lo=Math.min(a,b),hi=Math.max(a,b);a=null;svg.classList.remove('brushing');br.style.display='none';if(hi-lo<1)return;var list=[];for(var k=lo;k<=hi;k++)list.push(d.rows[k].o);if(list.length===d.rows.length&&!document.querySelector('.fbanner'))return;apply(list.join(','))})})})();
(function(){var box=document.getElementById('tree'),el=document.getElementById('flamedata');if(!box||!el)return;var data;try{data=JSON.parse(el.textContent)}catch(e){return}var N=data.names,LN=N.map(function(x){return x.toLowerCase()}),R=data.tree,TOT=R[1]||1,MORE=box.dataset.more,SELF=box.dataset.self;
function fm(v){return v>=100?v.toFixed(0):v>=10?v.toFixed(1):v.toFixed(2)}
function val(v){var p=v/TOT*100,u=window.__saUnit;return u&&u.m==='ms'?fm(p*u.F)+' '+u.U:fm(p)+'%'}
function selfOf(n){var s=n[1];for(var i=2;i<n.length;i++)s-=n[i][1];return Math.max(0,s)}
function esc(s){return String(s).replace(/[&<>"]/g,function(c){return{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]})}
function label(n){var t=N[n[0]],m=t.match(/^(.*?)( \[[^\]]+\])$/);return m?esc(m[1])+'<span class="src">'+esc(m[2].trim())+'</span>':esc(t)}
function row(n,d){var r=document.createElement('div');var leaf=n.length<=2;r.className='tn'+(leaf?' leaf':'');r.style.paddingLeft=(10+d*16)+'px';r._n=n;r._d=d;var sf=selfOf(n);r.innerHTML='<span class="cr">▶</span><span class="tv" data-v="'+n[1]+'">'+val(n[1])+'</span><span class="tb"><i style="width:'+Math.max(1,n[1]/TOT*100).toFixed(1)+'%"></i></span><span class="tl">'+label(n)+'</span>'+(sf>0&&!leaf?'<span class="ts" data-s="'+sf+'">'+SELF+' '+val(sf)+'</span>':'');return r}
function kids(r){var c=r.nextSibling,out=[];while(c&&c._d>r._d){out.push(c);c=c.nextSibling}return out}
function expand(r){if(r.classList.contains('open')||r.classList.contains('leaf'))return;r.classList.add('open');var n=r._n,frag=document.createDocumentFragment(),shown=0,hidden=0;for(var i=2;i<n.length;i++){if(n[i][1]/TOT<0.0005&&shown>=3){hidden++;continue}frag.appendChild(row(n[i],r._d+1));shown++}if(hidden){var m=document.createElement('div');m.className='tmore';m._d=r._d+1;m.style.paddingLeft=(30+(r._d+1)*16)+'px';m.textContent='+'+hidden+' '+MORE;frag.appendChild(m)}r.parentNode.insertBefore(frag,r.nextSibling)}
function collapse(r){kids(r).forEach(function(k){k.remove()});r.classList.remove('open')}
box.addEventListener('click',function(e){var r=e.target.closest('.tn');if(!r)return;if(r.classList.contains('open'))collapse(r);else expand(r)});
function reset(){box.innerHTML='';for(var i=2;i<R.length;i++)box.appendChild(row(R[i],0))}
reset();
function heaviest(){reset();var r=box.firstChild;var guard=0;while(r&&guard++<60){var n=r._n;if(n.length<=2)break;expand(r);var best=null,c=r.nextSibling;while(c&&c._d>r._d){if(c._d===r._d+1&&c._n&&(!best||c._n[1]>best._n[1]))best=c;c=c.nextSibling}if(!best||best._n[1]<n[1]*0.25)break;r=best}if(r)r.scrollIntoView({block:'nearest'})}
document.getElementById('theavy').addEventListener('click',heaviest);document.getElementById('tcollapse').addEventListener('click',function(){reset();document.getElementById('tmatch').textContent='';document.getElementById('tsearch').value=''});
function search(q){reset();var tm=document.getElementById('tmatch');if(!q){tm.textContent='';return}q=q.toLowerCase();var paths=[],tot=0;(function walk(n,path,inHit){var hit=LN[n[0]].indexOf(q)>=0;path.push(n);if(hit&&!inHit){tot+=n[1];if(paths.length<40)paths.push(path.slice())}for(var i=2;i<n.length;i++)walk(n[i],path,inHit||hit);path.pop()})(R,[],false);tm.textContent=paths.length?(fm(tot/TOT*100)+'%'):'0';paths.forEach(function(p){var r=null;for(var j=1;j<p.length;j++){var target=p[j],c=r?r.nextSibling:box.firstChild,found=null;while(c){if(c._n===target&&c._d===j-1){found=c;break}if(r&&c._d<=r._d)break;c=c.nextSibling}if(!found)return;if(j<p.length-1)expand(found);else found.classList.add('hit');r=found}});var first=box.querySelector('.tn.hit');if(first)first.scrollIntoView({block:'nearest'})}
var to;document.getElementById('tsearch').addEventListener('input',function(e){clearTimeout(to);to=setTimeout(function(){search(e.target.value.trim())},200)});
document.addEventListener('sa-unit',function(){box.querySelectorAll('.tv').forEach(function(x){x.textContent=val(+x.dataset.v)});box.querySelectorAll('.ts').forEach(function(x){x.textContent=SELF+' '+val(+x.dataset.s)})});
document.querySelectorAll('.totree').forEach(function(a){a.addEventListener('click',function(){var q=a.dataset.q;var i=document.getElementById('tsearch');i.value=q;search(q)})});
})();
var lb=document.getElementById('langbtn');if(lb)lb.addEventListener('click',function(){var n=lb.dataset.to;try{localStorage.setItem('sa-lang',n)}catch(e){}if(window.__saRender){window.__saRender(n);return}var u=new URL(location.href);u.searchParams.set('lang',n);location.href=u.toString()});
var us=document.getElementById('unitsel');if(us){var F=parseFloat(us.dataset.f)||0,U=us.dataset.u;function fm(v){return v>=100?v.toFixed(0):v>=10?v.toFixed(1):v.toFixed(2)}function setUnit(m){window.__saUnit={m:m,F:F,U:U};document.dispatchEvent(new Event('sa-unit'));us.querySelectorAll('button').forEach(function(x){x.classList.toggle('on',x.dataset.m===m)});document.querySelectorAll('.pv').forEach(function(e){var p=parseFloat(e.dataset.p);e.textContent=m==='ms'?fm(p*F)+' '+U:fm(p)+'%'});document.querySelectorAll('.uh').forEach(function(e){e.textContent=m==='ms'?U:'%'})}us.addEventListener('click',function(e){var bt=e.target.closest('button');if(!bt)return;try{localStorage.setItem('sa-unit',bt.dataset.m)}catch(x){}setUnit(bt.dataset.m)});var su='pct';try{su=localStorage.getItem('sa-unit')||'pct'}catch(x){}if(su!=='pct')setUnit(su)}
document.querySelectorAll('.chartwrap').forEach(function(cw){var d;try{d=JSON.parse(cw.querySelector('.chartdata').textContent)}catch(e){return}var svg=cw.querySelector('svg'),tip=cw.querySelector('.ctip'),guide=cw.querySelector('.guide'),L=d.labels;function f1(v){return v>=100?v.toFixed(0):v.toFixed(1)}function row(k,v){return '<div class="r"><span>'+L[k]+'</span><span>'+v+'</span></div>'}function show(el){var i=+el.dataset.i,w=d.rows[i];if(!w)return;var h='<b>'+w.t+'</b>';if(d.mspt){h+=row('tps',f1(w.tps))+row('med',f1(w.med)+' ms')+row('max',f1(w.max)+' ms')}if(w.busy>0)h+=row('busy',f1(w.busy)+' ms/tick');if(w.cpu>0)h+=row('cpu',f1(w.cpu)+'%');if(w.pl||w.en){h+=row('pl',w.pl)+row('en',w.en)+row('te',w.te)+row('ch',w.ch)}if(w.spike){h+='<div class="sp">⚠ '+L.spike+': '+w.spike+'</div>';if(w.causes&&w.causes.length)h+='<div class="cs">'+L.causes+': '+w.causes.join(', ')+'</div>'}tip.innerHTML=h;tip.hidden=false;var vb=svg.viewBox.baseVal,sc=svg.clientWidth/vb.width,gx=parseFloat(el.dataset.x);guide.setAttribute('x1',gx);guide.setAttribute('x2',gx);guide.style.display='';svg.querySelectorAll('circle').forEach(function(c){c.classList.toggle('on',+c.dataset.i===i)});var px=gx*sc,tw=tip.offsetWidth,cwW=cw.clientWidth;var left=px+14;if(left+tw>cwW)left=Math.max(0,px-tw-14);tip.style.left=left+'px';tip.style.top='12px'}function hide(){tip.hidden=true;guide.style.display='none';svg.querySelectorAll('circle.on').forEach(function(c){c.classList.remove('on')})}svg.querySelectorAll('.hov').forEach(function(r){r.addEventListener('mouseenter',function(){show(r)});r.addEventListener('click',function(){show(r)})});svg.addEventListener('mouseleave',hide);document.addEventListener('click',function(e){if(!cw.contains(e.target))hide()})});
var links=Array.from(document.querySelectorAll('.side nav a'));if('IntersectionObserver' in window){var io=new IntersectionObserver(function(es){es.forEach(function(e){if(e.isIntersecting){links.forEach(function(l){l.classList.toggle('on',l.getAttribute('href')==='#'+e.target.id)})}})},{rootMargin:'-20% 0px -70% 0px'});document.querySelectorAll('section[id]').forEach(function(s){io.observe(s)})}
document.body.classList.add('hideinfo');
var si=document.getElementById('showinfo');if(si)si.addEventListener('change',function(){document.body.classList.toggle('hideinfo',!si.checked)});
document.querySelectorAll('table.sortable').forEach(function(t){var ths=t.querySelectorAll('th');ths.forEach(function(th,i){th.addEventListener('click',function(){var tb=t.tBodies[0];var rows=Array.from(tb.rows);var asc=th.dataset.asc!=='1';ths.forEach(function(x){delete x.dataset.asc});th.dataset.asc=asc?'1':'0';rows.sort(function(a,b){var A=a.cells[i],B=b.cells[i];if(!A||!B)return 0;var av=A.dataset.v||A.textContent.trim(),bv=B.dataset.v||B.textContent.trim();var an=parseFloat(av.replace(',','.')),bn=parseFloat(bv.replace(',','.'));var r=(!isNaN(an)&&!isNaN(bn))?an-bn:av.localeCompare(bv);return asc?r:-r});rows.forEach(function(r){tb.appendChild(r)})})})});
document.querySelectorAll('.tfilter').forEach(function(inp){inp.addEventListener('input',function(){var q=inp.value.toLowerCase();var t=document.getElementById(inp.dataset.t);Array.from(t.tBodies[0].rows).forEach(function(r){r.style.display=r.textContent.toLowerCase().indexOf(q)>=0?'':'none'})})});
document.querySelectorAll('code.copy').forEach(function(c){c.title='Clic per copiare';c.addEventListener('click',function(){if(navigator.clipboard)navigator.clipboard.writeText(c.textContent)})});
var el=document.getElementById('flamedata');if(!el)return;var data;try{data=JSON.parse(el.textContent)}catch(e){return}
var names=data.names,low=names.map(function(x){return x.toLowerCase()}),root=data.tree,cv=document.getElementById('flamec'),tip=document.getElementById('ftip'),wrap=document.getElementById('fwrap');
var ctx=cv.getContext('2d'),RH=18,focus=root,query='',rects=[];
function depth(n){var d=0;for(var i=2;i<n.length;i++){d=Math.max(d,depth(n[i]))}return d+1}
function pathTo(t,n,acc){if(t===n){acc.push(t);return true}for(var i=2;i<t.length;i++){if(pathTo(t[i],n,acc)){acc.push(t);return true}}return false}
function color(name,hit){if(query){return hit?'#ef4444':'rgba(148,163,184,.35)'}var h=0;for(var i=0;i<name.length;i++)h=(h*31+name.charCodeAt(i))|0;var plugin=name.indexOf('[')>=0;var hue=plugin?(h%30+200):(h%40+10);return 'hsl('+((hue+360)%360)+','+(plugin?'70%':'78%')+','+(plugin?'66%':'62%')+')'}
function draw(){var dpr=window.devicePixelRatio||1;var W=wrap.clientWidth;var anc=[];pathTo(root,focus,anc);anc.reverse();var D=depth(focus)+anc.length-1;var H=Math.min(D,140)*RH;cv.style.height=H+'px';cv.width=W*dpr;cv.height=H*dpr;ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,W,H);ctx.font='11px ui-monospace,Menlo,Consolas,monospace';ctx.textBaseline='middle';rects=[];
var y=0;for(var a=0;a<anc.length-1;a++){box(anc[a],0,y,W,true);y+=RH}
(function walk(n,x,yy,w){box(n,x,yy,w,false);var cx=x;for(var i=2;i<n.length;i++){var c=n[i];var cw=w*c[1]/n[1];if(cw>=0.5){walk(c,cx,yy+RH,cw)}cx+=cw}})(focus,0,y,W);var mt=document.getElementById('fmatch');if(query){var s=0;(function cnt(n,inHit){var hit=low[n[0]].indexOf(query)>=0;if(hit&&!inHit){s+=n[1]}for(var i=2;i<n.length;i++)cnt(n[i],inHit||hit)})(root,false);mt.textContent='corrispondenze: '+(s/root[1]*100).toFixed(2)+'% del tempo'}else{mt.textContent=''}}
function box(n,x,y,w,ancestor){var nm=names[n[0]];var hit=query&&low[n[0]].indexOf(query)>=0;ctx.fillStyle=ancestor?'rgba(134,142,150,.45)':color(nm,hit);ctx.fillRect(x,y,Math.max(w-0.5,0.5),RH-1);if(w>40){ctx.fillStyle='#111';var t=nm;var max=Math.floor((w-6)/6.6);if(t.length>max)t=t.slice(0,Math.max(max-1,1))+'…';ctx.fillText(t,x+3,y+RH/2)}rects.push([x,y,w,n])}
function hitAt(ev){var r=cv.getBoundingClientRect();var x=ev.clientX-r.left,y=ev.clientY-r.top;for(var i=rects.length-1;i>=0;i--){var b=rects[i];if(x>=b[0]&&x<=b[0]+b[2]&&y>=b[1]&&y<b[1]+RH)return b[3]}return null}
cv.addEventListener('mousemove',function(ev){var n=hitAt(ev);if(!n){tip.style.display='none';return}var r=wrap.getBoundingClientRect();tip.style.display='block';tip.textContent=names[n[0]]+' — '+(n[1]/root[1]*100).toFixed(2)+'% ('+(n[1]/1000).toFixed(0)+' ms)';var lx=ev.clientX-r.left+12;if(lx+300>r.width)lx=Math.max(0,r.width-320);tip.style.left=lx+'px';tip.style.top=(ev.clientY-r.top+14)+'px'});
cv.addEventListener('mouseleave',function(){tip.style.display='none'});
cv.addEventListener('click',function(ev){var n=hitAt(ev);if(n){focus=(n===focus)?root:n;draw()}});
document.getElementById('freset').addEventListener('click',function(){focus=root;query='';document.getElementById('fsearch').value='';draw()});
var tmo;document.getElementById('fsearch').addEventListener('input',function(e){clearTimeout(tmo);tmo=setTimeout(function(){query=e.target.value.trim().toLowerCase();draw()},150)});
window.addEventListener('resize',function(){clearTimeout(tmo);tmo=setTimeout(draw,100)});window.__redrawFlame=draw;draw();
})();
"#;
