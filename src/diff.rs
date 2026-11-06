use std::collections::BTreeMap;
use std::fmt::Write;

use serde::Serialize;

use crate::report::{Report, fmt_num};
use crate::rules::short;

#[derive(Clone, Debug, Serialize)]
pub struct DiffRow {
    pub name: String,
    pub before: f64,
    pub after: f64,
    pub delta: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct DiffReport {
    pub unit: String,
    pub before_label: String,
    pub after_label: String,
    pub summary: Vec<DiffRow>,
    pub categories: Vec<DiffRow>,
    pub sources: Vec<DiffRow>,
    pub methods: Vec<DiffRow>,
    pub entities: Vec<DiffRow>,
    pub new_findings: Vec<String>,
    pub resolved_findings: Vec<String>,
}

fn scale(r: &Report) -> (f64, &'static str) {
    match (r.analysis.main.ticks, r.analysis.unit.as_str()) {
        (t, "ms") if t > 0 => (r.analysis.main.busy / t as f64 / 100.0, "ms/tick"),
        _ => (1.0, "%"),
    }
}

fn rows(
    before: Vec<(String, f64)>,
    after: Vec<(String, f64)>,
    sb: f64,
    sa: f64,
    limit: usize,
) -> Vec<DiffRow> {
    let mut map: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for (k, v) in before {
        map.entry(k).or_insert((0.0, 0.0)).0 += v * sb;
    }
    for (k, v) in after {
        map.entry(k).or_insert((0.0, 0.0)).1 += v * sa;
    }
    let mut out: Vec<DiffRow> = map
        .into_iter()
        .map(|(name, (b, a))| DiffRow {
            name,
            before: b,
            after: a,
            delta: a - b,
        })
        .filter(|r| r.before.abs() > 0.0001 || r.after.abs() > 0.0001)
        .collect();
    out.sort_by(|x, y| y.delta.abs().total_cmp(&x.delta.abs()));
    out.truncate(limit);
    out
}

pub fn diff(b: &Report, a: &Report) -> DiffReport {
    let (sb, ub) = scale(b);
    let (sa, ua) = scale(a);
    let (sb, sa, unit) = if ub == ua {
        (sb, sa, ub)
    } else {
        (1.0, 1.0, "%")
    };
    let mut summary = Vec::new();
    let mut push = |name: &str, x: Option<f64>, y: Option<f64>| {
        if let (Some(x), Some(y)) = (x, y) {
            summary.push(DiffRow {
                name: name.into(),
                before: x,
                after: y,
                delta: y - x,
            });
        }
    };
    push(
        "TPS (1m)",
        b.meta.tps.map(|t| t[0]),
        a.meta.tps.map(|t| t[0]),
    );
    push("MSPT medio", b.meta.mspt_mean, a.meta.mspt_mean);
    push("MSPT max", b.meta.mspt_max, a.meta.mspt_max);
    push(
        "Main thread ms/tick",
        b.analysis.main.busy_per_tick,
        a.analysis.main.busy_per_tick,
    );
    push(
        "Giocatori",
        Some(b.meta.players as f64),
        Some(a.meta.players as f64),
    );
    push(
        "Lag spike (finestre)",
        Some(b.analysis.spikes.len() as f64),
        Some(a.analysis.spikes.len() as f64),
    );

    let cats = rows(
        b.analysis
            .categories
            .iter()
            .map(|c| (c.label.clone(), c.pct))
            .collect(),
        a.analysis
            .categories
            .iter()
            .map(|c| (c.label.clone(), c.pct))
            .collect(),
        sb,
        sa,
        25,
    );
    let srcs = rows(
        b.analysis
            .sources
            .iter()
            .map(|s| (s.source.clone(), s.incl_pct))
            .collect(),
        a.analysis
            .sources
            .iter()
            .map(|s| (s.source.clone(), s.incl_pct))
            .collect(),
        sb,
        sa,
        25,
    );
    let meths = rows(
        b.analysis
            .methods
            .iter()
            .map(|m| (m.method.clone(), m.self_pct))
            .collect(),
        a.analysis
            .methods
            .iter()
            .map(|m| (m.method.clone(), m.self_pct))
            .collect(),
        sb,
        sa,
        30,
    );
    let ents = rows(
        b.analysis
            .entity_types
            .iter()
            .map(|e| (e.name.clone(), e.pct))
            .collect(),
        a.analysis
            .entity_types
            .iter()
            .map(|e| (e.name.clone(), e.pct))
            .collect(),
        sb,
        sa,
        15,
    );
    let key = |f: &crate::rules::Finding| format!("{} [{}]", f.title, f.severity.label());
    let bt: Vec<String> = b
        .findings
        .iter()
        .filter(|f| f.severity <= crate::rules::Severity::Medio)
        .map(|f| f.title.clone())
        .collect();
    let at: Vec<String> = a
        .findings
        .iter()
        .filter(|f| f.severity <= crate::rules::Severity::Medio)
        .map(|f| f.title.clone())
        .collect();
    let new_findings = a
        .findings
        .iter()
        .filter(|f| f.severity <= crate::rules::Severity::Medio && !bt.contains(&f.title))
        .map(key)
        .collect();
    let resolved_findings = b
        .findings
        .iter()
        .filter(|f| f.severity <= crate::rules::Severity::Medio && !at.contains(&f.title))
        .map(key)
        .collect();
    DiffReport {
        unit: unit.into(),
        before_label: b
            .meta
            .viewer_url
            .clone()
            .unwrap_or_else(|| b.meta.origin.clone()),
        after_label: a
            .meta
            .viewer_url
            .clone()
            .unwrap_or_else(|| a.meta.origin.clone()),
        summary,
        categories: cats,
        sources: srcs,
        methods: meths,
        entities: ents,
        new_findings,
        resolved_findings,
    }
}

fn arrow(d: f64, higher_is_worse: bool) -> &'static str {
    if d.abs() < 1e-9 {
        "="
    } else if (d > 0.0) == higher_is_worse {
        "▲ peggio"
    } else {
        "▼ meglio"
    }
}

pub fn render_text(d: &DiffReport) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "\n━━ sparkscope · confronto profili ━━");
    let _ = writeln!(
        o,
        "Prima: {}\nDopo:  {}\nValori in {} (normalizzati per tick quando possibile)\n",
        d.before_label, d.after_label, d.unit
    );
    let sec = |o: &mut String, title: &str, rows: &[DiffRow], worse_up: bool, shorten: bool| {
        if rows.is_empty() {
            return;
        }
        let _ = writeln!(o, "■ {title}");
        let w = rows
            .iter()
            .map(|r| {
                if shorten {
                    short(&r.name).chars().count()
                } else {
                    r.name.chars().count()
                }
            })
            .max()
            .unwrap_or(10)
            .min(60);
        for r in rows {
            let name = if shorten {
                short(&r.name)
            } else {
                r.name.clone()
            };
            let _ = writeln!(
                o,
                "  {:<w$}  {:>9} → {:>9}  {:>+9}  {}",
                name,
                fmt_num(r.before),
                fmt_num(r.after),
                fmt_num(r.delta),
                arrow(r.delta, worse_up),
                w = w
            );
        }
        let _ = writeln!(o);
    };
    let mut summ = String::new();
    if !d.summary.is_empty() {
        let _ = writeln!(summ, "■ Riepilogo");
        for r in &d.summary {
            let worse_up = !r.name.starts_with("TPS");
            let _ = writeln!(
                summ,
                "  {:<22}  {:>9} → {:>9}  {:>+9}  {}",
                r.name,
                fmt_num(r.before),
                fmt_num(r.after),
                fmt_num(r.delta),
                arrow(r.delta, worse_up)
            );
        }
        let _ = writeln!(summ);
    }
    o.push_str(&summ);
    sec(
        &mut o,
        "Categorie del main thread",
        &d.categories,
        true,
        false,
    );
    sec(&mut o, "Plugin/mod (nello stack)", &d.sources, true, false);
    sec(&mut o, "Metodi (tempo proprio)", &d.methods, true, true);
    sec(&mut o, "Entità", &d.entities, true, false);
    if !d.new_findings.is_empty() {
        let _ = writeln!(
            o,
            "■ Problemi nuovi\n  - {}\n",
            d.new_findings.join("\n  - ")
        );
    }
    if !d.resolved_findings.is_empty() {
        let _ = writeln!(
            o,
            "■ Problemi risolti\n  - {}\n",
            d.resolved_findings.join("\n  - ")
        );
    }
    o
}
