use std::fmt::Write;

use super::html::{esc, x_label};
use crate::analysis::{Analysis, Delta, Spike, WindowStat};
use crate::rules::short;

const SHARE_PARTS: usize = 3;

pub enum ChronoEvent<'a> {
    Calm {
        first: &'a WindowStat,
        last: &'a WindowStat,
        windows: usize,
        mspt_min: f64,
        mspt_max: f64,
    },
    Spike(&'a Spike),
}

pub fn chrono_events(a: &Analysis) -> Vec<ChronoEvent<'_>> {
    let mut events = Vec::with_capacity(a.spikes.len() * 2 + 1);
    let mut calm: Option<(usize, usize)> = None;
    for (i, w) in a.windows.iter().enumerate() {
        let spike = if w.is_spike {
            a.spikes.iter().find(|s| s.window.index == w.index)
        } else {
            None
        };
        match spike {
            Some(s) => {
                if let Some((from, to)) = calm.take() {
                    events.push(calm_event(&a.windows[from..=to]));
                }
                events.push(ChronoEvent::Spike(s));
            }
            None => calm = Some(calm.map_or((i, i), |(from, _)| (from, i))),
        }
    }
    if let Some((from, to)) = calm {
        events.push(calm_event(&a.windows[from..=to]));
    }
    events
}

fn calm_event(run: &[WindowStat]) -> ChronoEvent<'_> {
    let mut mspt_min = f64::INFINITY;
    let mut mspt_max = 0.0f64;
    for w in run {
        mspt_min = mspt_min.min(w.mspt_median);
        mspt_max = mspt_max.max(w.mspt_median);
    }
    ChronoEvent::Calm {
        first: &run[0],
        last: &run[run.len() - 1],
        windows: run.len(),
        mspt_min,
        mspt_max,
    }
}

struct Baseline {
    busy_per_tick: f64,
}

impl Baseline {
    fn of(a: &Analysis) -> Baseline {
        let mut calm: Vec<f64> = Vec::with_capacity(a.windows.len());
        for w in &a.windows {
            if !w.is_spike && w.busy_per_tick > 0.0 {
                calm.push(w.busy_per_tick);
            }
        }
        calm.sort_by(f64::total_cmp);
        Baseline {
            busy_per_tick: calm.get(calm.len() / 2).copied().unwrap_or(0.0),
        }
    }
}

pub fn chrono_html(a: &Analysis) -> String {
    let Some(first) = a.windows.first() else {
        return String::new();
    };
    let base = Baseline::of(a);
    let mut events = chrono_events(a);
    let opening = match events.first() {
        Some(ChronoEvent::Calm { .. }) => Some(events.remove(0)),
        _ => None,
    };
    let timed = first.start_time > 0;
    let mut out = String::with_capacity(4096 + a.spikes.len() * 2600);
    let _ = write!(
        out,
        r#"<div class="chrono-meta"><span>{}</span><span>{}</span></div><ol class="chrono">"#,
        esc(&tf!(
            "{} finestre da circa 1 minuto",
            "{} windows of about 1 minute",
            a.windows.len()
        )),
        if timed {
            crate::i18n::t("orari in UTC", "times in UTC")
        } else {
            crate::i18n::t("finestre numerate in ordine", "windows numbered in order")
        }
    );
    let mut start = tf!(
        "{} giocatori, TPS {}",
        "{} players, TPS {}",
        first.players,
        num(first.tps)
    );
    if let Some(ChronoEvent::Calm {
        first: f,
        last: l,
        windows,
        mspt_min,
        mspt_max,
    }) = &opening
    {
        start.push_str(" · ");
        start.push_str(&calm_text(f, l, *windows, *mspt_min, *mspt_max));
    }
    write_edge(
        &mut out,
        "start",
        &span_label(first, first),
        crate::i18n::t("Inizio registrazione", "Recording start"),
        &start,
    );
    let total = a.spikes.len();
    let mut nth = 0;
    for e in &events {
        match e {
            ChronoEvent::Calm {
                first,
                last,
                windows,
                mspt_min,
                mspt_max,
            } => write_calm(&mut out, first, last, *windows, *mspt_min, *mspt_max),
            ChronoEvent::Spike(s) => {
                nth += 1;
                write_spike(&mut out, s, nth, total, &base);
            }
        }
    }
    write_summary(&mut out, a);
    out.push_str("</ol>");
    out
}

fn write_edge(out: &mut String, kind: &str, time: &str, title: &str, detail: &str) {
    let _ = write!(
        out,
        r#"<li class="ev {kind}"><span class="t">{}</span><span class="rail"><i></i></span><div class="b"><p class="calm"><b>{}</b> · {}</p></div></li>"#,
        esc(time),
        esc(title),
        esc(detail)
    );
}

fn write_calm(
    out: &mut String,
    first: &WindowStat,
    last: &WindowStat,
    windows: usize,
    mspt_min: f64,
    mspt_max: f64,
) {
    let _ = write!(
        out,
        r#"<li class="ev calm"><span class="t">{}</span><span class="rail"><i></i></span><div class="b"><p class="calm">{}</p></div></li>"#,
        esc(&span_label(first, last)),
        esc(&calm_text(first, last, windows, mspt_min, mspt_max))
    );
}

fn calm_text(
    first: &WindowStat,
    last: &WindowStat,
    windows: usize,
    mspt_min: f64,
    mspt_max: f64,
) -> String {
    let minutes = stretch_minutes(first, last, windows);
    let range = if (mspt_max - mspt_min).abs() < 0.05 {
        format!("{} ms", num(mspt_min))
    } else {
        format!("{}–{} ms", num(mspt_min), num(mspt_max))
    };
    tf!(
        "{minutes} min regolari, MSPT mediano {range}",
        "{minutes} min steady, median MSPT {range}"
    )
}

fn stretch_minutes(first: &WindowStat, last: &WindowStat, windows: usize) -> i64 {
    if first.start_time > 0 && last.end_time > first.start_time {
        ((last.end_time - first.start_time) as f64 / 60_000.0)
            .round()
            .max(1.0) as i64
    } else {
        i64::try_from(windows).unwrap_or(i64::MAX)
    }
}

fn span_label(first: &WindowStat, last: &WindowStat) -> String {
    if first.start_time > 0 && last.end_time > 0 {
        format!("{}–{}", clock(first.start_time), clock(last.end_time))
    } else if first.index == last.index {
        x_label(first)
    } else {
        format!("#{}–{}", first.index + 1, last.index + 1)
    }
}

fn clock(ms: i64) -> String {
    let secs = ms / 1000;
    format!("{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60)
}

fn write_spike(out: &mut String, s: &Spike, nth: usize, total: usize, base: &Baseline) {
    let w = &s.window;
    let load = if base.busy_per_tick > 0.0 && w.busy_per_tick > 0.0 {
        format!("{}×", num(w.busy_per_tick / base.busy_per_tick))
    } else {
        "—".into()
    };
    let _ = write!(
        out,
        r#"<li class="ev sp" data-w="{}"><span class="t">{}</span><span class="rail"><i></i></span><div class="b"><div class="box"><div class="bh"><span class="pill">{}</span><span class="nth">{}</span></div><dl class="kv"><div><dt>{}</dt><dd class="bad">{} ms</dd></div><div><dt>{}</dt><dd>{} ms</dd></div><div><dt>TPS</dt><dd>{}</dd></div><div><dt>{}</dt><dd>{}</dd></div></dl>"#,
        w.index,
        esc(&span_label(w, w)),
        crate::i18n::t("LAG SPIKE", "LAG SPIKE"),
        esc(&tf!("{nth} di {total}", "{nth} of {total}")),
        crate::i18n::t("Tick più lento", "Slowest tick"),
        num(w.mspt_max),
        crate::i18n::t("MSPT mediano", "Median MSPT"),
        num(w.mspt_median),
        num(w.tps),
        crate::i18n::t("Carico vs normale", "Load vs normal"),
        esc(&load)
    );
    write_cause(out, s);
    if !crate::rules::spike_unattributed(s) {
        let _ = write!(
            out,
            r#"<details><summary>{}</summary><div class="dts">{}{}{}</div></details>"#,
            crate::i18n::t("Dettaglio completo", "Full breakdown"),
            delta_list(
                crate::i18n::t("Categorie", "Categories"),
                &s.cat_deltas,
                &s.unit,
                false
            ),
            delta_list(
                crate::i18n::t("Plugin", "Plugins"),
                &s.owner_deltas,
                &s.unit,
                false
            ),
            delta_list(
                crate::i18n::t("Metodi", "Methods"),
                &s.method_deltas,
                &s.unit,
                true
            )
        );
    }
    out.push_str("</div></div></li>");
}

fn write_cause(out: &mut String, s: &Spike) {
    let Some(top) = s.cat_deltas.first() else {
        let _ = write!(
            out,
            r#"<p class="why">{}</p>"#,
            esc(crate::i18n::t(
                "Nessuna categoria cresce nella media del minuto: lo stallo è concentrato in pochi tick (pausa del GC, salvataggio o task a orario fisso).",
                "No category grows over the minute average: the stall is concentrated in a few ticks (GC pause, saving or a fixed-time task).",
            ))
        );
        return;
    };
    let growth_total: f64 = s.cat_deltas.iter().map(|d| d.delta.max(0.0)).sum();
    let share = if growth_total > 0.0 {
        top.delta / growth_total * 100.0
    } else {
        0.0
    };
    let _ = write!(
        out,
        r#"<p class="why"><span class="lbl">{}</span> <b>{}</b> <span class="q">{} {} · {}</span></p>"#,
        crate::i18n::t("Causa principale", "Main cause"),
        esc(&top.name),
        esc(&signed(top.delta)),
        esc(&s.unit),
        esc(&tf!(
            "{}% della crescita",
            "{}% of the growth",
            share.round()
        ))
    );
    if let Some(plugin) = s
        .owner_deltas
        .iter()
        .find(|d| !is_platform(&d.name) && d.delta > 0.0)
    {
        let _ = write!(
            out,
            r#"<p class="why"><span class="lbl">{}</span> <code>{}</code> <span class="q">{} → {} {} ({})</span></p>"#,
            crate::i18n::t("Plugin coinvolto", "Plugin involved"),
            esc(&plugin.name),
            num(plugin.baseline),
            num(plugin.value),
            esc(&s.unit),
            esc(&growth(plugin))
        );
    }
    write_share_bar(out, &s.cat_deltas, growth_total);
}

fn write_share_bar(out: &mut String, cats: &[Delta], growth_total: f64) {
    if growth_total <= 0.0 {
        return;
    }
    out.push_str(r#"<div class="share" aria-hidden="true">"#);
    let mut legend = String::new();
    let mut used = 0.0;
    let mut growing: Vec<&Delta> = cats.iter().filter(|d| d.delta > 0.0).collect();
    growing.sort_by(|x, y| y.delta.total_cmp(&x.delta));
    for (i, d) in growing.into_iter().take(SHARE_PARTS).enumerate() {
        let pct = d.delta / growth_total * 100.0;
        used += pct;
        let _ = write!(out, r#"<i class="s{i}" style="width:{pct:.2}%"></i>"#);
        let _ = write!(
            legend,
            r#"<span><i class="s{i}"></i>{} {}%</span>"#,
            esc(&d.name),
            pct.round()
        );
    }
    if used < 99.5 {
        let rest = 100.0 - used;
        let _ = write!(out, r#"<i class="sx" style="width:{rest:.2}%"></i>"#);
        let _ = write!(
            legend,
            r#"<span><i class="sx"></i>{} {}%</span>"#,
            crate::i18n::t("altro", "other"),
            rest.round()
        );
    }
    let _ = write!(out, r#"</div><div class="share-l">{legend}</div>"#);
}

fn delta_list(title: &str, ds: &[Delta], unit: &str, methods: bool) -> String {
    if ds.is_empty() {
        return String::new();
    }
    let mut x = format!(
        r#"<table class="dt"><thead><tr><th>{}</th><th class="n">{}</th><th class="n">{}</th><th class="n">Δ {}</th></tr></thead><tbody>"#,
        esc(title),
        crate::i18n::t("Normale", "Normal"),
        crate::i18n::t("Spike", "Spike"),
        esc(unit)
    );
    for d in ds.iter().take(5) {
        let name = if methods {
            format!(
                r#"<code title="{}">{}</code>"#,
                esc(&d.name),
                esc(&short(&d.name))
            )
        } else {
            esc(&d.name)
        };
        let _ = write!(
            x,
            r#"<tr><td>{name}</td><td class="n dim">{}</td><td class="n">{}</td><td class="n up">{}</td></tr>"#,
            num(d.baseline),
            num(d.value),
            esc(&signed(d.delta))
        );
    }
    x.push_str("</tbody></table>");
    x
}

fn write_summary(out: &mut String, a: &Analysis) {
    let total = a.spikes.len();
    let mut items: Vec<String> = Vec::with_capacity(4);
    items.push(if total == 1 {
        crate::i18n::t("1 lag spike", "1 lag spike").to_string()
    } else {
        tf!("{total} lag spike", "{total} lag spikes")
    });
    items.push(tf!(
        "{} di {} minuti coinvolti",
        "{} of {} minutes affected",
        total,
        a.windows.len()
    ));
    if let Some(gap) = mean_gap_minutes(a) {
        items.push(tf!(
            "uno ogni {} min in media",
            "one every {} min on average",
            if (gap - gap.round()).abs() < 0.05 {
                format!("{gap:.0}")
            } else {
                num(gap)
            }
        ));
    }
    if let Some((name, n)) = recurring_plugin(a) {
        items.push(tf!(
            "{name} coinvolto in {n} su {total}",
            "{name} involved in {n} of {total}"
        ));
    }
    let _ = write!(
        out,
        r#"<li class="ev end"><span class="t"></span><span class="rail"><i></i></span><div class="b"><p class="calm"><b>{}</b> · {}</p></div></li>"#,
        crate::i18n::t("Riepilogo", "Summary"),
        esc(&items.join(" · "))
    );
}

fn mean_gap_minutes(a: &Analysis) -> Option<f64> {
    if a.spikes.len() < 2 {
        return None;
    }
    let first = &a.spikes.first()?.window;
    let last = &a.spikes.last()?.window;
    let span = if first.start_time > 0 && last.start_time > first.start_time {
        (last.start_time - first.start_time) as f64 / 60_000.0
    } else {
        (last.index - first.index) as f64
    };
    Some(span / (a.spikes.len() - 1) as f64)
}

fn recurring_plugin(a: &Analysis) -> Option<(&str, usize)> {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for s in &a.spikes {
        let Some(top) = s
            .owner_deltas
            .iter()
            .find(|d| !is_platform(&d.name) && d.delta > 0.0)
        else {
            continue;
        };
        match counts.iter_mut().find(|(n, _)| *n == top.name) {
            Some(c) => c.1 += 1,
            None => counts.push((top.name.as_str(), 1)),
        }
    }
    counts
        .into_iter()
        .max_by(|x, y| x.1.cmp(&y.1).then_with(|| y.0.cmp(x.0)))
        .filter(|(_, n)| *n >= 2)
}

fn growth(d: &Delta) -> String {
    if d.baseline > 0.0 {
        format!("+{:.0}%", (d.value / d.baseline - 1.0) * 100.0)
    } else {
        crate::i18n::t("nuovo", "new").to_string()
    }
}

fn signed(v: f64) -> String {
    if v >= 0.0 {
        format!("+{}", num(v))
    } else {
        format!("−{}", num(-v))
    }
}

fn num(v: f64) -> String {
    let txt = if v >= 100.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    };
    if crate::i18n::is_en() {
        txt
    } else {
        txt.replace('.', ",")
    }
}

fn is_platform(name: &str) -> bool {
    matches!(
        name,
        "Minecraft"
            | "JDK"
            | "Netty"
            | "Nativo (JVM/OS)"
            | "Native (JVM/OS)"
            | "Librerie"
            | "Libraries"
            | "Sconosciuto"
            | "Unknown"
    ) || name.starts_with("Server (")
}
