use serde_json::{Value, json};

use crate::model::Mode;
use crate::report::{Report, fmt_duration, fmt_num, fmt_time};
use crate::rules::Severity;

pub const FLAG_COMPONENTS_V2: u64 = 1 << 15;
pub const DEFAULT_SITE: &str = "https://sparkanalyzer.me/";

const TEXT: u8 = 10;
const SEPARATOR: u8 = 14;
const CONTAINER: u8 = 17;
const ACTION_ROW: u8 = 1;
const BUTTON: u8 = 2;
const LINK: u8 = 5;

fn text(content: impl Into<String>) -> Value {
    json!({ "type": TEXT, "content": content.into() })
}

fn separator(big: bool) -> Value {
    json!({ "type": SEPARATOR, "divider": true, "spacing": if big { 2 } else { 1 } })
}

fn link_button(label: &str, url: &str) -> Value {
    json!({ "type": BUTTON, "style": LINK, "label": label, "url": url })
}

pub fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '_' | '*' | '~' | '|' | '>' | '`' | '\\' | '#') {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

fn is_method(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty()
        && s.contains('.')
        && !s.contains(' ')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '$' | '_' | '<' | '>'))
}

fn rich(s: &str, plugins: &[String]) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('(') {
        let Some(j) = rest[i..].find(')') else { break };
        let inner = &rest[i + 1..i + j];
        let parts: Vec<&str> = inner.split(", ").collect();
        out.push_str(&escape(&rest[..i]));
        if parts.iter().all(|p| is_method(p)) {
            let codes: Vec<String> = parts.iter().map(|p| format!("`{}`", p.trim())).collect();
            out.push_str(&format!("→ {}", codes.join(", ")));
        } else {
            out.push('(');
            out.push_str(&escape(inner));
            out.push(')');
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(&escape(rest));
    let mut names: Vec<&String> = plugins.iter().collect();
    names.sort_by_key(|n| std::cmp::Reverse(n.len()));
    for n in names {
        let e = escape(n);
        if let Some(i) = out.find(&e) {
            out = format!("{}**{}**{}", &out[..i], e, &out[i + e.len()..]);
            break;
        }
    }
    out.replace("  ", " ").trim().to_string()
}

fn accent(score: u32) -> u32 {
    match score {
        85.. => 0x23a55a,
        65.. => 0xf0b232,
        40.. => 0xf0a43a,
        _ => 0xf23f43,
    }
}

fn dot(level: u8) -> &'static str {
    match level {
        0 => "🟢",
        1 => "🟠",
        _ => "🔴",
    }
}

pub fn profile_key(r: &Report) -> Option<String> {
    let v = r.meta.viewer_url.as_deref()?;
    let k = v.trim_end_matches('/').rsplit('/').next()?;
    (!k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric())).then(|| k.to_string())
}

pub fn message(r: &Report, site: &str) -> Value {
    let t = crate::i18n::t;
    let m = &r.meta;
    let a = &r.analysis;
    let s = &r.summary;
    let site = if site.ends_with('/') {
        site.to_string()
    } else {
        format!("{site}/")
    };
    let plugins: Vec<String> = r
        .plugins
        .iter()
        .map(|(n, v)| {
            if v.is_empty() {
                n.clone()
            } else {
                format!("{n} {v}")
            }
        })
        .chain(r.plugins.iter().map(|(n, _)| n.clone()))
        .collect();

    let mut sub = Vec::new();
    if !m.platform.is_empty() {
        sub.push(escape(&m.platform));
    }
    let what = match a.mode {
        Mode::Allocation => t("profilo allocazioni", "allocation profile"),
        Mode::Lock => t("profilo lock", "lock profile"),
        Mode::Execution => t("profilo CPU", "CPU profile"),
    };
    if m.duration_s > 0.0 {
        sub.push(format!(
            "{what} {} {}",
            t("di", "of"),
            fmt_duration(m.duration_s)
        ));
    } else {
        sub.push(escape(&m.kind));
    }
    if m.players > 0 {
        sub.push(tf!("{} giocatori", "{} players", m.players));
    }

    let mut parts: Vec<Value> = vec![text(format!(
        "### {}\n-# {}",
        tf!(
            "Salute del server: {}/100 · {}",
            "Server health: {}/100 · {}",
            s.score,
            s.grade
        ),
        sub.join(" · ")
    ))];

    let mut metrics: Vec<String> = Vec::new();
    match (&r.alloc, a.mode) {
        (Some(al), Mode::Allocation) => {
            metrics.push(format!(
                "**{}** {}",
                t("Allocato", "Allocated"),
                fmt_time(al.total, "byte")
            ));
            if let Some(rate) = al.jvm_rate_bps.or(al.rate_bps) {
                metrics.push(format!(
                    "{} **{}** {}",
                    dot(if rate >= 1.5e9 {
                        2
                    } else if rate >= 7e8 {
                        1
                    } else {
                        0
                    }),
                    t("Velocità", "Rate"),
                    crate::alloc::fmt_rate(rate)
                ));
            }
            metrics.push(format!(
                "**{}** {}%",
                t("Main thread", "Main thread"),
                fmt_num(al.main_pct)
            ));
        }
        _ => {
            if let Some(tps) = m.tps {
                let l = if tps[0] >= 19.5 {
                    0
                } else if tps[0] >= 17.0 {
                    1
                } else {
                    2
                };
                metrics.push(format!("{} **TPS** {}", dot(l), fmt_num(tps[0])));
            }
            if let Some(ms) = m.mspt_mean.or(a.main.busy_per_tick) {
                let l = if ms < 40.0 {
                    0
                } else if ms <= 50.0 {
                    1
                } else {
                    2
                };
                metrics.push(format!("{} **MSPT** {} ms", dot(l), fmt_num(ms)));
            }
            if a.mode == Mode::Execution && !a.windows.is_empty() {
                let ratio = a.spikes.len() as f64 / a.windows.len() as f64;
                let l = if a.spikes.is_empty() {
                    0
                } else if ratio <= 0.2 {
                    1
                } else {
                    2
                };
                metrics.push(format!(
                    "{} **{}** {} {} {} min",
                    dot(l),
                    t("Lag spike", "Lag spikes"),
                    a.spikes.len(),
                    t("su", "of"),
                    a.windows.len()
                ));
            }
        }
    }
    if !metrics.is_empty() {
        parts.push(separator(false));
        parts.push(text(metrics.join("　·　")));
    }

    parts.push(separator(true));
    match &s.cause {
        Some(c) => {
            parts.push(text(format!(
                "**{}**\n{}",
                t("Causa principale", "Main cause"),
                rich(c, &plugins)
            )));
            if let Some(ac) = &s.action {
                parts.push(text(format!(
                    "**{}**\n{}",
                    t("Soluzione", "Fix"),
                    rich(ac, &plugins)
                )));
            }
        }
        None => parts.push(text(t(
            "**Nessun problema rilevante**\nIl profilo non mostra cause di lag da correggere.",
            "**No significant problem**\nThe profile shows no lag cause to fix.",
        ))),
    }

    let overview = t("Panoramica", "Overview");
    let profile = t("Profilo", "Profile");
    let problems = r
        .findings
        .iter()
        .filter(|f| f.severity < Severity::Info && f.area != overview && f.area != profile)
        .count();
    let host = site
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    parts.push(text(format!(
        "-# {} · {}",
        tf!("{} problemi trovati", "{} problems found", problems),
        host
    )));

    let report_url = match profile_key(r) {
        Some(k) => format!("{site}?p={k}"),
        None => site.clone(),
    };
    let mut buttons = vec![link_button(
        t("Apri il report", "Open the report"),
        &report_url,
    )];
    if let Some(v) = &m.viewer_url {
        buttons.push(link_button(t("Viewer spark", "spark viewer"), v));
    }
    parts.push(json!({ "type": ACTION_ROW, "components": buttons }));

    json!({
        "flags": FLAG_COMPONENTS_V2,
        "allowed_mentions": { "parse": [] },
        "components": [{
            "type": CONTAINER,
            "accent_color": accent(s.score),
            "components": parts
        }]
    })
}

pub fn is_webhook_url(url: &str) -> bool {
    [
        "https://discord.com/api/webhooks/",
        "https://discordapp.com/api/webhooks/",
        "https://ptb.discord.com/api/webhooks/",
        "https://canary.discord.com/api/webhooks/",
    ]
    .iter()
    .any(|p| url.starts_with(p) && url.len() > p.len() + 10)
}

pub fn webhook_send_url(url: &str) -> String {
    let base = url.split('?').next().unwrap_or(url);
    let mut extra: Vec<&str> = url
        .split_once('?')
        .map(|(_, q)| {
            q.split('&')
                .filter(|p| !p.starts_with("wait=") && !p.starts_with("with_components="))
                .collect()
        })
        .unwrap_or_default();
    extra.retain(|p| !p.is_empty());
    let mut q = vec!["wait=true", "with_components=true"];
    q.extend(extra);
    format!("{base}?{}", q.join("&"))
}
