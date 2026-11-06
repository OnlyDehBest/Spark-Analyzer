use std::collections::HashMap;
use std::fmt::Write;

use super::{Report, fmt_duration, fmt_num, fmt_time, fmt_utc};
use crate::rules::{Severity, short};

fn esc(s: &str) -> String {
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
            r#"<circle cx="{:.1}" cy="{:.1}" r="3" class="dot{}"><title>Finestra #{}&#10;TPS {:.1} · MSPT med {:.1} / max {:.1}&#10;main thread {:.1} ms/tick&#10;{} giocatori · {} entità · {} chunk</title></circle>"#,
            x(i),
            y(vals_a[i]),
            if w.is_spike { " sp" } else { "" },
            i + 1,
            w.tps,
            w.mspt_median,
            w.mspt_max,
            w.busy_per_tick,
            w.players,
            w.entities,
            w.chunks
        );
    }
    let step = ((ws.len() as f64) / 12.0).ceil().max(1.0) as usize;
    for i in (0..ws.len()).step_by(step) {
        let _ = write!(
            s,
            r#"<text x="{:.1}" y="{:.1}" class="axis" text-anchor="middle">#{}</text>"#,
            x(i),
            ht - 8.0,
            i + 1
        );
    }
    s.push_str("</svg>");
    let legend = if has_mspt {
        r#"<div class="legend"><span class="k1"></span>MSPT massimo <span class="k2"></span>MSPT mediano <span class="k3"></span>lavoro main thread (ms/tick) <span class="kb"></span>budget 50 ms <span class="ks"></span>lag spike</div>"#
    } else {
        r#"<div class="legend"><span class="k1"></span>carico main thread <span class="ks"></span>lag spike</div>"#
    };
    Some(format!("{s}{legend}"))
}

fn pct_bar(p: f64) -> String {
    format!(
        r#"<span class="bar"><i style="width:{:.1}%"></i></span>"#,
        p.clamp(0.0, 100.0)
    )
}

pub fn render(r: &Report) -> String {
    let m = &r.meta;
    let a = &r.analysis;
    let mut b = String::new();
    let title = format!(
        "sparkscope — {}",
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
    let mut card = |label: &str, value: String, cls: &str| {
        let _ = write!(
            cards,
            r#"<div class="card {cls}"><div class="cl">{}</div><div class="cv">{}</div></div>"#,
            esc(label),
            value
        );
    };
    if let Some(t) = m.tps {
        card(
            "TPS (1m)",
            fmt_num(t[0]),
            if t[0] < 19.5 { "bad" } else { "ok" },
        );
    }
    if let Some(ms) = m.mspt_mean {
        card(
            "MSPT medio",
            format!("{} ms", fmt_num(ms)),
            if ms > 45.0 { "bad" } else { "ok" },
        );
    }
    if let Some(mx) = m.mspt_max {
        card(
            "MSPT max",
            format!("{} ms", fmt_num(mx)),
            if mx > 50.0 { "bad" } else { "ok" },
        );
    }
    if let Some(bt) = a.main.busy_per_tick {
        card(
            "Main thread",
            format!("{} ms/tick", fmt_num(bt)),
            if bt > 45.0 { "bad" } else { "ok" },
        );
    }
    card(
        "Problemi gravi",
        crit.to_string(),
        if crit > 0 { "bad" } else { "ok" },
    );
    card("Durata", fmt_duration(m.duration_s), "");
    card("Giocatori", m.players.to_string(), "");
    card(
        "Lag spike",
        format!("{} / {}", a.spikes.len(), a.windows.len()),
        if a.spikes.is_empty() { "ok" } else { "warn" },
    );

    let _ = write!(
        b,
        r#"<header><div class="wrap"><h1>{}</h1><p class="sub">{}{}</p></div></header>"#,
        esc(&title),
        esc(&format!(
            "{} · {} · {} · spark {} · inizio {} · {} plugin/mod",
            m.kind,
            m.mode,
            m.engine,
            m.spark_version,
            fmt_utc(m.start_time),
            m.plugin_count
        )),
        m.viewer_url
            .as_ref()
            .map(|v| format!(
                r#" · <a href="{0}" target="_blank" rel="noopener">apri nel viewer spark</a>"#,
                esc(v)
            ))
            .unwrap_or_default()
    );
    let _ = write!(
        b,
        r##"<nav><div class="wrap"><a href="#diagnosi">Diagnosi</a><a href="#tempo">Tempo</a><a href="#plugin">Plugin</a><a href="#spike">Spike</a><a href="#flame">Flamegraph</a><a href="#metodi">Metodi</a><a href="#entita">Entità</a><a href="#thread">Thread</a><a href="#mondo">Mondo</a></div></nav><main class="wrap">"##
    );
    let _ = write!(b, r#"<section class="cards">{cards}</section>"#);
    if let Some(th) = m.ticked_threshold {
        let _ = write!(
            b,
            r#"<p class="note">Profilo registrato con <code>--only-ticks-over {th}</code>: contiene solo i tick più lenti di {th} ms.</p>"#
        );
    }

    let _ = write!(
        b,
        r#"<section id="diagnosi"><h2>Diagnosi</h2><div class="filters"><label><input type="checkbox" id="showinfo"> mostra anche le note informative</label></div>"#
    );
    for f in &r.findings {
        let open = if f.severity <= Severity::Alto || f.area == "Panoramica" {
            " open"
        } else {
            ""
        };
        let impact = if f.impact_pct > 0.0 {
            format!(r#"<span class="imp">{}%</span>"#, fmt_num(f.impact_pct))
        } else {
            String::new()
        };
        let _ = write!(
            b,
            r#"<details class="finding {}"{open}><summary><span class="sev">{}</span><span class="ft">{}</span><span class="area">{}</span>{impact}</summary><div class="fb">"#,
            sev_class(f.severity),
            f.severity.label(),
            esc(&f.title),
            esc(&f.area)
        );
        if !f.evidence.is_empty() {
            b.push_str("<ul class=\"ev\">");
            for e in &f.evidence {
                let _ = write!(b, "<li>{}</li>", esc(e));
            }
            b.push_str("</ul>");
        }
        if !f.culprits.is_empty() {
            let _ = write!(
                b,
                r#"<p class="cul"><b>Responsabili:</b> {}</p>"#,
                esc(&f.culprits.join(", "))
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
            r#"<table class="sortable"><thead><tr><th>Categoria</th><th class="n">%</th><th></th><th class="n">{}</th><th>Plugin coinvolti</th><th>Metodi principali</th></tr></thead><tbody>"#,
            if per_tick { "ms/tick" } else { "tempo" }
        );
        for c in a.categories.iter().filter(|c| c.pct >= 0.1) {
            let owners: Vec<String> = c
                .top_owners
                .iter()
                .take(3)
                .filter(|o| o.pct >= 0.1)
                .map(|o| format!("{} {}%", esc(&o.name), fmt_num(o.pct)))
                .collect();
            let ms: Vec<String> = c
                .top_methods
                .iter()
                .take(3)
                .map(|x| format!("<code>{}</code> {}%", esc(&short(&x.name)), fmt_num(x.pct)))
                .collect();
            let v = if per_tick {
                fmt_num(c.time / a.main.ticks as f64)
            } else {
                fmt_time(c.time, &a.unit)
            };
            let _ = write!(
                b,
                r#"<tr><td>{}</td><td class="n">{}</td><td>{}</td><td class="n">{}</td><td>{}</td><td class="small">{}</td></tr>"#,
                esc(&c.label),
                fmt_num(c.pct),
                pct_bar(c.pct),
                v,
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
                .map(|x| format!("{} <b>{}%</b>", esc(&x.name), fmt_num(x.pct)))
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
            b.push_str(r#"<table class="sortable"><thead><tr><th>Plugin</th><th class="n">% nello stack</th><th></th><th class="n">% proprio</th><th>Punti di ingresso</th><th>Avvisi</th></tr></thead><tbody>"#);
            for s in a.sources.iter().filter(|s| s.incl_pct >= 0.05) {
                let entries: Vec<String> = s
                    .top_entries
                    .iter()
                    .take(4)
                    .map(|e| {
                        format!(
                            "<code>{}</code> <span class=\"ctx\">{}</span> {}%",
                            esc(&short(&e.entry)),
                            esc(&e.context),
                            fmt_num(e.pct)
                        )
                    })
                    .collect();
                let hz: Vec<String> = s
                    .hazards
                    .iter()
                    .filter(|h| h.pct >= 0.1)
                    .map(|h| {
                        format!(
                            "<span class=\"warn\">{} {}%</span>",
                            esc(&h.name),
                            fmt_num(h.pct)
                        )
                    })
                    .collect();
                let _ = write!(
                    b,
                    r#"<tr><td><b>{}</b></td><td class="n">{}</td><td>{}</td><td class="n">{}</td><td class="small">{}</td><td class="small">{}</td></tr>"#,
                    esc(&s.display),
                    fmt_num(s.incl_pct),
                    pct_bar(s.incl_pct),
                    fmt_num(s.self_pct),
                    entries.join("<br>"),
                    hz.join("<br>")
                );
            }
            b.push_str("</tbody></table>");
        }
        if !a.events.is_empty() {
            b.push_str(r#"<h3>Eventi Bukkit</h3><table class="sortable"><thead><tr><th>Evento</th><th>Plugin</th><th class="n">%</th></tr></thead><tbody>"#);
            for e in a.events.iter().take(40) {
                let _ = write!(
                    b,
                    r#"<tr><td>{}</td><td>{}</td><td class="n">{}</td></tr>"#,
                    esc(&e.entry),
                    esc(&e.source),
                    fmt_num(e.pct)
                );
            }
            b.push_str("</tbody></table>");
        }
        b.push_str("</section>");

        let _ = write!(b, r#"<section id="spike"><h2>Andamento e lag spike</h2>"#);
        match timeline_svg(r) {
            Some(svg) => b.push_str(&svg),
            None => b.push_str(r#"<p class="note">Il profilo contiene una sola finestra temporale: registra almeno 3-5 minuti per vedere l'andamento.</p>"#),
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
            r#"<section id="metodi"><h2>Metodi più costosi</h2><input class="tfilter" data-t="mt" placeholder="Filtra…"><table class="sortable" id="mt"><thead><tr><th>Metodo</th><th class="n">% proprio</th><th class="n">% totale</th><th>Sorgente</th></tr></thead><tbody>"#
        );
        for mm in &a.methods {
            let _ = write!(
                b,
                r#"<tr><td><code title="{}">{}</code></td><td class="n">{}</td><td class="n">{}</td><td>{}</td></tr>"#,
                esc(&mm.method),
                esc(&short(&mm.method)),
                fmt_num(mm.self_pct),
                fmt_num(mm.incl_pct),
                esc(&mm.source.clone().unwrap_or_default())
            );
        }
        b.push_str("</tbody></table>");
        b.push_str(r#"<h3>Metodi per tempo totale (inclusi i chiamati)</h3><table class="sortable"><thead><tr><th>Metodo</th><th class="n">% totale</th><th class="n">% proprio</th><th>Sorgente</th></tr></thead><tbody>"#);
        for mm in a.methods_inclusive.iter().take(40) {
            let _ = write!(
                b,
                r#"<tr><td><code title="{}">{}</code></td><td class="n">{}</td><td class="n">{}</td><td>{}</td></tr>"#,
                esc(&mm.method),
                esc(&short(&mm.method)),
                fmt_num(mm.incl_pct),
                fmt_num(mm.self_pct),
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
                r#"<li><b>{}%</b> <span class="dim">{}</span><div class="stack">{}</div></li>"#,
                fmt_num(h.pct),
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
                r#"<div><h3>{}</h3><table class="sortable"><thead><tr><th>Tipo</th><th class="n">%</th><th></th></tr></thead><tbody>"#,
                title
            );
            for e in list.iter().take(25) {
                let _ = write!(
                    b,
                    r#"<tr><td>{}</td><td class="n">{}</td><td>{}</td></tr>"#,
                    esc(&e.name),
                    fmt_num(e.pct),
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
    let _ = write!(
        b,
        r#"<footer>Generato da sparkscope {} · sorgente: {}</footer></main>"#,
        env!("CARGO_PKG_VERSION"),
        esc(&m.origin)
    );

    format!(
        "<!doctype html><html lang=\"it\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>{}</style></head><body>{}<script>{}</script></body></html>",
        esc(&title),
        CSS,
        b,
        JS
    )
}

const CSS: &str = r#"
:root{--bg:#f6f7f9;--fg:#1b1f24;--muted:#5d6672;--card:#fff;--line:#e3e6ea;--acc:#2563eb;--crit:#b91c1c;--high:#ea580c;--med:#ca8a04;--low:#2563eb;--info:#6b7280;--ok:#15803d;--bar:#3b82f6;--spike:rgba(239,68,68,.12);--code:#eef1f5}
@media (prefers-color-scheme:dark){:root{--bg:#0f1115;--fg:#e6e8eb;--muted:#9aa3ae;--card:#171a20;--line:#2a2f38;--acc:#60a5fa;--crit:#f87171;--high:#fb923c;--med:#facc15;--low:#60a5fa;--info:#9ca3af;--ok:#4ade80;--bar:#60a5fa;--spike:rgba(248,113,113,.15);--code:#222731}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:14px/1.5 system-ui,-apple-system,Segoe UI,Roboto,sans-serif}
.wrap{max-width:1200px;margin:0 auto;padding:0 16px}
header{padding:28px 0 14px;border-bottom:1px solid var(--line);background:var(--card)}h1{margin:0;font-size:22px}.sub{margin:6px 0 0;color:var(--muted)}a{color:var(--acc)}
nav{position:sticky;top:0;z-index:5;background:var(--card);border-bottom:1px solid var(--line)}nav .wrap{display:flex;gap:4px;overflow-x:auto}nav a{padding:10px 10px;text-decoration:none;color:var(--muted);white-space:nowrap}nav a:hover{color:var(--fg)}
section{margin:28px 0}h2{font-size:18px;margin:0 0 12px}h3{font-size:15px;margin:20px 0 8px}h4{margin:12px 0 6px;font-size:14px}h5{margin:0 0 6px;font-size:13px;color:var(--muted)}
.cards{display:grid;grid-template-columns:repeat(auto-fill,minmax(140px,1fr));gap:10px}.card{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:12px}.cl{color:var(--muted);font-size:12px}.cv{font-size:20px;font-weight:650;margin-top:2px}.card.bad .cv{color:var(--crit)}.card.ok .cv{color:var(--ok)}.card.warn .cv{color:var(--med)}
.note{color:var(--muted)}.callout{background:var(--card);border-left:4px solid var(--med);padding:10px 12px;border-radius:6px}
.finding{background:var(--card);border:1px solid var(--line);border-left:5px solid var(--info);border-radius:8px;margin:10px 0}.finding summary{cursor:pointer;padding:12px 14px;display:flex;gap:10px;align-items:center;flex-wrap:wrap;list-style:none}.finding summary::-webkit-details-marker{display:none}
.finding.crit{border-left-color:var(--crit)}.finding.high{border-left-color:var(--high)}.finding.med{border-left-color:var(--med)}.finding.low{border-left-color:var(--low)}
.sev{font-size:11px;font-weight:700;padding:2px 7px;border-radius:4px;color:#fff;background:var(--info)}.crit .sev{background:var(--crit)}.high .sev{background:var(--high)}.med .sev{background:var(--med);color:#111}.low .sev{background:var(--low)}
.ft{font-weight:600;flex:1;min-width:200px}.area{color:var(--muted);font-size:12px}.imp{font-weight:700;font-variant-numeric:tabular-nums}
.fb{padding:0 16px 14px}.ev{margin:0;padding-left:18px}.ev li{margin:3px 0}.why{color:var(--muted)}.cul{margin:8px 0}.sol li{margin:4px 0}.sol ul{margin:4px 0}
body.hideinfo .finding.info:not(:first-of-type){display:none}
table{width:100%;border-collapse:collapse;background:var(--card);border:1px solid var(--line);border-radius:8px;overflow:hidden;font-variant-numeric:tabular-nums}th,td{padding:7px 10px;border-bottom:1px solid var(--line);text-align:left;vertical-align:top}th{font-size:12px;color:var(--muted);font-weight:600;cursor:pointer;user-select:none;background:var(--card)}tr:last-child td{border-bottom:0}.n{text-align:right;white-space:nowrap}.small{font-size:12px}.dim{color:var(--muted)}
code{font:12px ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;background:var(--code);padding:1px 4px;border-radius:4px;word-break:break-all}
.bar{display:inline-block;width:110px;height:8px;background:var(--code);border-radius:4px;overflow:hidden;vertical-align:middle}.bar i{display:block;height:100%;background:var(--bar)}
.ctx{color:var(--muted);font-size:11px}.warn{color:var(--high);font-weight:600}
.chart{width:100%;height:auto;background:var(--card);border:1px solid var(--line);border-radius:8px}.chart .grid{stroke:var(--line)}.chart .budget{stroke:var(--crit);stroke-dasharray:4 4}.chart .axis{fill:var(--muted);font-size:11px}.chart polyline{fill:none;stroke-width:2}.chart .l1{stroke:var(--high)}.chart .l2{stroke:var(--acc)}.chart .l3{stroke:var(--ok);stroke-dasharray:3 3}.chart circle{fill:var(--high)}.chart circle.sp{fill:var(--crit);r:4}.chart .spikebg{fill:var(--spike)}
.legend{color:var(--muted);font-size:12px;margin:6px 0 14px;display:flex;gap:6px 14px;flex-wrap:wrap;align-items:center}.legend span{display:inline-block;width:14px;height:3px;vertical-align:middle;margin-right:4px}.k1{background:var(--high)}.k2{background:var(--acc)}.k3{background:var(--ok)}.kb{background:var(--crit)}.ks{background:var(--spike);height:10px!important}
.spike{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:10px 14px;margin:10px 0}.spike h4 small{color:var(--muted);font-weight:400}.grid3{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:12px}.grid3 table{border:0}.grid3 td{padding:3px 6px}
.grid2{display:grid;grid-template-columns:repeat(auto-fit,minmax(320px,1fr));gap:16px}
.fbar{display:flex;gap:8px;margin-bottom:8px;align-items:center;flex-wrap:wrap}#fsearch,.tfilter{flex:1;min-width:200px;padding:7px 10px;border:1px solid var(--line);border-radius:6px;background:var(--card);color:var(--fg)}.tfilter{width:100%;margin-bottom:8px}button{padding:7px 12px;border:1px solid var(--line);border-radius:6px;background:var(--card);color:var(--fg);cursor:pointer}#fmatch{color:var(--muted);font-size:12px}
#fwrap{position:relative;background:var(--card);border:1px solid var(--line);border-radius:8px;overflow:hidden}#flamec{display:block;width:100%}#ftip{position:absolute;pointer-events:none;background:#111827;color:#f9fafb;padding:6px 8px;border-radius:6px;font-size:12px;display:none;max-width:520px;z-index:3}
.paths li{margin:8px 0}.stack{margin-top:4px;line-height:1.9}.plist span{display:inline-block;margin:2px 6px 2px 0;padding:2px 8px;border:1px solid var(--line);border-radius:12px;background:var(--card);font-size:12px}.plist i{color:var(--muted);font-style:normal}
footer{color:var(--muted);font-size:12px;margin:40px 0 30px}.filters{margin-bottom:6px;color:var(--muted);font-size:13px}.copy{cursor:copy}
@media (max-width:640px){th,td{padding:6px}.bar{width:60px}.cv{font-size:17px}}
"#;

const JS: &str = r#"
(function(){
document.body.classList.add('hideinfo');
var si=document.getElementById('showinfo');if(si)si.addEventListener('change',function(){document.body.classList.toggle('hideinfo',!si.checked)});
document.querySelectorAll('table.sortable').forEach(function(t){var ths=t.querySelectorAll('th');ths.forEach(function(th,i){th.addEventListener('click',function(){var tb=t.tBodies[0];var rows=Array.from(tb.rows);var asc=th.dataset.asc!=='1';ths.forEach(function(x){delete x.dataset.asc});th.dataset.asc=asc?'1':'0';rows.sort(function(a,b){var A=a.cells[i],B=b.cells[i];if(!A||!B)return 0;var av=A.dataset.v||A.textContent.trim(),bv=B.dataset.v||B.textContent.trim();var an=parseFloat(av.replace(',','.')),bn=parseFloat(bv.replace(',','.'));var r=(!isNaN(an)&&!isNaN(bn))?an-bn:av.localeCompare(bv);return asc?r:-r});rows.forEach(function(r){tb.appendChild(r)})})})});
document.querySelectorAll('.tfilter').forEach(function(inp){inp.addEventListener('input',function(){var q=inp.value.toLowerCase();var t=document.getElementById(inp.dataset.t);Array.from(t.tBodies[0].rows).forEach(function(r){r.style.display=r.textContent.toLowerCase().indexOf(q)>=0?'':'none'})})});
document.querySelectorAll('code.copy').forEach(function(c){c.title='Clic per copiare';c.addEventListener('click',function(){if(navigator.clipboard)navigator.clipboard.writeText(c.textContent)})});
var el=document.getElementById('flamedata');if(!el)return;var data;try{data=JSON.parse(el.textContent)}catch(e){return}
var names=data.names,root=data.tree,cv=document.getElementById('flamec'),tip=document.getElementById('ftip'),wrap=document.getElementById('fwrap');
var ctx=cv.getContext('2d'),RH=18,focus=root,query='',rects=[];
function depth(n){var d=0;for(var i=2;i<n.length;i++){d=Math.max(d,depth(n[i]))}return d+1}
function pathTo(t,n,acc){if(t===n){acc.push(t);return true}for(var i=2;i<t.length;i++){if(pathTo(t[i],n,acc)){acc.push(t);return true}}return false}
function color(name,hit){if(query){return hit?'#ef4444':'rgba(148,163,184,.35)'}var h=0;for(var i=0;i<name.length;i++)h=(h*31+name.charCodeAt(i))|0;var plugin=name.indexOf('[')>=0;var hue=plugin?(h%40+260):(h%45+15);return 'hsl('+((hue+360)%360)+','+(plugin?'55%':'72%')+','+(plugin?'62%':'58%')+')'}
function draw(){var dpr=window.devicePixelRatio||1;var W=wrap.clientWidth;var anc=[];pathTo(root,focus,anc);anc.reverse();var D=depth(focus)+anc.length-1;var H=Math.min(D,140)*RH;cv.style.height=H+'px';cv.width=W*dpr;cv.height=H*dpr;ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,W,H);ctx.font='11px ui-monospace,Menlo,Consolas,monospace';ctx.textBaseline='middle';rects=[];
var y=0;for(var a=0;a<anc.length-1;a++){box(anc[a],0,y,W,true);y+=RH}
(function walk(n,x,yy,w){box(n,x,yy,w,false);var cx=x;for(var i=2;i<n.length;i++){var c=n[i];var cw=w*c[1]/n[1];if(cw>=0.5){walk(c,cx,yy+RH,cw)}cx+=cw}})(focus,0,y,W);var mt=document.getElementById('fmatch');if(query){var s=0;(function cnt(n,inHit){var nm=names[n[0]];var hit=nm.toLowerCase().indexOf(query)>=0;if(hit&&!inHit){s+=n[1]}for(var i=2;i<n.length;i++)cnt(n[i],inHit||hit)})(root,false);mt.textContent='corrispondenze: '+(s/root[1]*100).toFixed(2)+'% del tempo'}else{mt.textContent=''}}
function box(n,x,y,w,ancestor){var nm=names[n[0]];var hit=query&&nm.toLowerCase().indexOf(query)>=0;ctx.fillStyle=ancestor?'rgba(148,163,184,.45)':color(nm,hit);ctx.fillRect(x,y,Math.max(w-0.5,0.5),RH-1);if(w>40){ctx.fillStyle='#111';var t=nm;var max=Math.floor((w-6)/6.6);if(t.length>max)t=t.slice(0,Math.max(max-1,1))+'…';ctx.fillText(t,x+3,y+RH/2)}rects.push([x,y,w,n])}
function hitAt(ev){var r=cv.getBoundingClientRect();var x=ev.clientX-r.left,y=ev.clientY-r.top;for(var i=rects.length-1;i>=0;i--){var b=rects[i];if(x>=b[0]&&x<=b[0]+b[2]&&y>=b[1]&&y<b[1]+RH)return b[3]}return null}
cv.addEventListener('mousemove',function(ev){var n=hitAt(ev);if(!n){tip.style.display='none';return}var r=wrap.getBoundingClientRect();tip.style.display='block';tip.textContent=names[n[0]]+' — '+(n[1]/root[1]*100).toFixed(2)+'% ('+(n[1]/1000).toFixed(0)+' ms)';var lx=ev.clientX-r.left+12;if(lx+300>r.width)lx=Math.max(0,r.width-320);tip.style.left=lx+'px';tip.style.top=(ev.clientY-r.top+14)+'px'});
cv.addEventListener('mouseleave',function(){tip.style.display='none'});
cv.addEventListener('click',function(ev){var n=hitAt(ev);if(n){focus=(n===focus)?root:n;draw()}});
document.getElementById('freset').addEventListener('click',function(){focus=root;query='';document.getElementById('fsearch').value='';draw()});
var tmo;document.getElementById('fsearch').addEventListener('input',function(e){clearTimeout(tmo);tmo=setTimeout(function(){query=e.target.value.trim().toLowerCase();draw()},150)});
window.addEventListener('resize',function(){clearTimeout(tmo);tmo=setTimeout(draw,100)});draw();
})();
"#;
