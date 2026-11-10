pub mod chrono;
#[cfg(feature = "cli")]
pub mod cli;
pub mod config_txt;
pub mod heapdiff_html;
pub mod html;
pub mod localize;
pub mod markdown;

use serde::Serialize;

use crate::analysis::Analysis;
use crate::model::Profile;
use crate::proto::sampler_metadata::SamplerEngine;
pub use crate::rules::ConfigChange;
use crate::rules::{self, Finding};

#[derive(Clone, Debug, Serialize, Default)]
pub struct MetaInfo {
    pub kind: String,
    pub origin: String,
    pub viewer_url: Option<String>,
    pub platform: String,
    pub minecraft_version: String,
    pub spark_version: String,
    pub engine: String,
    pub mode: String,
    pub start_time: i64,
    pub duration_s: f64,
    pub interval_ms: f64,
    pub creator: String,
    pub comment: String,
    pub players: i64,
    pub plugin_count: usize,
    pub threads: usize,
    pub ticks: i32,
    pub tps: Option<[f64; 3]>,
    pub mspt_mean: Option<f64>,
    pub mspt_p95: Option<f64>,
    pub mspt_max: Option<f64>,
    pub ticked_threshold: Option<i64>,
    pub uptime_s: Option<f64>,
    pub window_filter: Option<Vec<usize>>,
    pub windows_total: usize,
    pub machine: Vec<(String, String)>,
    pub resources: Vec<Resource>,
}

/// Riga di risorsa: `pct` (0..100) disegna la barra.
#[derive(Clone, Debug, Serialize, Default)]
pub struct Resource {
    pub label: String,
    pub value: String,
    pub pct: Option<f64>,
}

fn machine_info(
    sys: Option<&crate::proto::SystemStatistics>,
    ps: Option<&crate::proto::PlatformStatistics>,
) -> (Vec<(String, String)>, Vec<Resource>) {
    let t = crate::i18n::t;
    let gb = |b: i64| crate::rules::gb(b.max(0) as u64);
    let mut mach = Vec::new();
    let mut res = Vec::new();
    let mut m = |l: &str, v: String| {
        if !v.trim().is_empty() {
            mach.push((l.to_string(), v.trim().to_string()));
        }
    };
    let mut r = |l: String, v: String, pct: Option<f64>| {
        res.push(Resource {
            label: l,
            value: v,
            pct,
        });
    };
    let used_of = |used: i64, total: i64| (total > 0).then(|| used as f64 / total as f64 * 100.0);
    if let Some(s) = sys {
        if let Some(c) = &s.cpu {
            m("CPU", c.model_name.clone());
            if c.threads > 0 {
                m(t("Thread CPU", "CPU threads"), c.threads.to_string());
            }
            for (name, u) in [
                (t("CPU processo", "CPU process"), &c.process_usage),
                (t("CPU sistema", "CPU system"), &c.system_usage),
            ] {
                if let Some(u) = u.as_ref().filter(|u| u.last1m > 0.0 || u.last15m > 0.0) {
                    r(
                        name.into(),
                        format!(
                            "{:.0}% (1m) / {:.0}% (15m)",
                            u.last1m * 100.0,
                            u.last15m * 100.0
                        ),
                        Some(u.last1m * 100.0),
                    );
                }
            }
        }
        if let Some(o) = &s.os {
            m(
                t("Sistema operativo", "Operating system"),
                format!("{} {} ({})", o.name, o.version, o.arch),
            );
        }
        if let Some(j) = &s.java {
            m(
                "Java",
                format!("{} {} {}", j.vendor, j.version, j.vendor_version),
            );
        }
        if let Some(j) = &s.jvm {
            m("JVM", format!("{} {} {}", j.name, j.vendor, j.version));
        }
        if s.uptime > 0 {
            m(
                t("Uptime sistema", "System uptime"),
                fmt_duration(s.uptime as f64 / 1000.0),
            );
        }
        if let Some(mem) = &s.memory {
            for (name, p) in [("RAM", &mem.physical), ("Swap", &mem.swap)] {
                if let Some(p) = p.as_ref().filter(|p| p.total > 0) {
                    r(
                        name.into(),
                        format!("{} / {}", gb(p.used), gb(p.total)),
                        used_of(p.used, p.total),
                    );
                }
            }
        }
        if let Some(d) = s.disk.as_ref().filter(|d| d.total > 0) {
            r(
                t("Disco", "Disk").into(),
                format!("{} / {}", gb(d.used), gb(d.total)),
                used_of(d.used, d.total),
            );
        }
    }
    if let Some(mem) = ps.and_then(|s| s.memory.as_ref()) {
        if let Some(h) = mem.heap.as_ref().filter(|h| h.used > 0) {
            let cap = if h.max > 0 { h.max } else { h.committed };
            r(
                "Heap".into(),
                format!("{} / {}", gb(h.used), gb(cap)),
                used_of(h.used, cap),
            );
        }
        if let Some(h) = mem.non_heap.as_ref().filter(|h| h.used > 0) {
            r("Non-heap".into(), gb(h.used), None);
        }
        if let Some(a) = mem.alloc_bps_last1m.as_ref().filter(|a| a.mean > 0.0) {
            r(
                t("Allocazioni (1m)", "Allocation rate (1m)").into(),
                format!("{:.1} MB/s", a.mean / 1e6),
                None,
            );
        }
    }
    let gcs: Vec<_> = match ps.map(|s| &s.gc).filter(|g| !g.is_empty()) {
        Some(g) => g
            .iter()
            .map(|(k, v)| (k, v.total, v.avg_time, v.avg_frequency))
            .collect(),
        None => sys.map_or(vec![], |s| {
            s.gc.iter()
                .map(|(k, v)| (k, v.total, v.avg_time, v.avg_frequency))
                .collect()
        }),
    };
    let mut gcs = gcs;
    gcs.sort_by(|a, b| a.0.cmp(b.0));
    for (k, n, time, freq) in gcs.into_iter().filter(|g| g.1 > 0) {
        r(
            format!("GC {k}"),
            format!(
                "{n} {}, {time:.1} ms {}, {} {}",
                t("raccolte", "collections"),
                t("medi", "avg"),
                t("ogni", "every"),
                fmt_duration(freq / 1000.0)
            ),
            None,
        );
    }
    if let Some(s) = sys {
        let mut nets: Vec<_> = s.net.iter().collect();
        nets.sort_by(|a, b| a.0.cmp(b.0));
        for (k, n) in nets {
            let f =
                |v: &Option<crate::proto::RollingAverageValues>| v.as_ref().map_or(0.0, |v| v.mean);
            let (rx, tx) = (f(&n.rx_bytes_per_second), f(&n.tx_bytes_per_second));
            if rx > 0.0 || tx > 0.0 {
                r(
                    format!("Net {k}"),
                    format!("rx {:.2} MB/s, tx {:.2} MB/s", rx / 1e6, tx / 1e6),
                    None,
                );
            }
        }
    }
    (mach, res)
}

#[derive(Clone, Debug, Serialize)]
pub struct HotChunk {
    pub world: String,
    pub block_x: i32,
    pub block_z: i32,
    pub total: i32,
    pub top: Vec<(String, i32)>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub meta: MetaInfo,
    pub findings: Vec<Finding>,
    pub analysis: Analysis,
    pub hot_chunks: Vec<HotChunk>,
    pub plugins: Vec<(String, String)>,
    pub alloc: Option<crate::alloc::AllocReport>,
    pub summary: Summary,
    pub config_changes: Vec<ConfigChange>,
}

/// Riassunto in cima al report: punteggio, causa principale e primo intervento.
#[derive(Clone, Debug, Serialize, Default)]
pub struct Summary {
    pub score: u32,
    pub grade: String,
    pub level: String,
    pub cause: Option<String>,
    pub action: Option<String>,
    pub lines: Vec<String>,
}

fn clip(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    let cut = match cut.rfind(' ') {
        Some(i) if i > max / 2 => &cut[..i],
        _ => &cut,
    };
    format!("{}…", cut.trim_end_matches([',', ';', ':', '.', ' ']))
}

pub fn summarize(m: &MetaInfo, a: &Analysis, findings: &[Finding]) -> Summary {
    let t = crate::i18n::t;
    let overview = t("Panoramica", "Overview");
    let profile = t("Profilo", "Profile");
    let mut score = 100.0f64;
    if let Some(tps) = m.tps {
        score -= ((20.0 - tps[0]).max(0.0) * 5.0).min(40.0);
    }
    if let Some(load) = a.main.busy_per_tick.or(m.mspt_mean) {
        score -= ((load - 30.0) * 1.2).clamp(0.0, 35.0);
    }
    let exec = a.mode == crate::model::Mode::Execution && a.main.total > 0.0;
    if exec && !a.windows.is_empty() {
        score -= (a.spikes.len() as f64 / a.windows.len() as f64 * 20.0).min(15.0);
    }
    if let Some(mx) = m.mspt_max {
        score -= if mx > 500.0 {
            8.0
        } else if mx > 150.0 {
            4.0
        } else {
            0.0
        };
    }
    let relevant = |f: &&Finding| {
        f.severity < rules::Severity::Info && f.area != overview && f.area != profile
    };
    let penalty: f64 = findings
        .iter()
        .filter(relevant)
        .map(|f| match f.severity {
            rules::Severity::Critico => 8.0,
            rules::Severity::Alto => 4.0,
            rules::Severity::Medio => 1.5,
            _ => 0.5,
        })
        .sum();
    score -= penalty.min(20.0);
    let score = score.clamp(0.0, 100.0).round() as u32;
    let (grade, level) = match score {
        85.. => (t("Ottima", "Good"), "ok"),
        65.. => (t("Discreta", "Fair"), "wrn"),
        40.. => (t("Scarsa", "Poor"), "bad"),
        _ => (t("Critica", "Critical"), "bad"),
    };
    let top = findings.iter().find(relevant);
    let cause = top.map(|f| {
        let title = rules::plain_pv(&f.title);
        if f.impact_pct >= 1.0 && !title.contains('%') {
            format!("{title} ({}%)", fmt_num(f.impact_pct))
        } else {
            title
        }
    });
    let action = top
        .and_then(|f| f.solutions.first())
        .map(|s| {
            let s = rules::plain_pv(s);
            match s.split_once("\n    - ") {
                Some((head, rest)) => {
                    let first = rest.split("\n    - ").next().unwrap_or("");
                    format!("{}: {first}", head.trim_end_matches(':'))
                }
                None => s,
            }
        })
        .map(|s| clip(&s, 200));
    let mut head = tf!(
        "Salute {score}/100 ({grade})",
        "Health {score}/100 ({grade})"
    );
    if let Some(tps) = m.tps {
        head.push_str(&format!(" · TPS {}", fmt_num(tps[0])));
    }
    if let Some(ms) = m.mspt_mean {
        head.push_str(&format!(" · MSPT {} ms", fmt_num(ms)));
    }
    if exec && !a.windows.is_empty() {
        head.push_str(&tf!(
            " · {} lag spike su {} finestre",
            " · {} lag spikes in {} windows",
            a.spikes.len(),
            a.windows.len()
        ));
    }
    let mut lines = vec![head];
    match &cause {
        Some(c) => lines.push(tf!("Causa principale: {c}", "Main cause: {c}")),
        None => lines.push(
            t(
                "Nessun problema rilevante trovato nel profilo",
                "No significant problem found in the profile",
            )
            .into(),
        ),
    }
    if let Some(ac) = &action {
        lines.push(tf!("Primo intervento: {ac}", "First step: {ac}"));
    }
    Summary {
        score,
        grade: grade.into(),
        level: level.into(),
        cause,
        action,
        lines,
    }
}

pub fn build(
    p: &Profile,
    a: Analysis,
    origin: &str,
    viewer_url: Option<String>,
    kind: &str,
) -> Report {
    let (mut findings, config_changes) = rules::diagnose_with_config(&a, p);
    let alloc = (p.mode == crate::model::Mode::Allocation).then(|| crate::alloc::analyze(p));
    if let Some(ar) = &alloc {
        findings.extend(crate::alloc::findings(ar, p));
        rules::sort_findings(&mut findings);
    }
    let m = &p.meta;
    let pm = m.platform_metadata.clone().unwrap_or_default();
    let ps = m.platform_statistics.as_ref();
    let tps = ps
        .and_then(|s| s.tps.as_ref())
        .filter(|t| t.last1m > 0.0)
        .map(|t| [t.last1m, t.last5m, t.last15m]);
    let mspt = ps
        .and_then(|s| s.mspt.as_ref())
        .and_then(|x| x.last1m.as_ref())
        .filter(|x| x.mean > 0.0);
    let mut plugins: Vec<(String, String)> = p
        .sources
        .meta
        .values()
        .filter(|s| !s.builtin)
        .map(|s| (s.name.clone(), s.version.clone()))
        .collect();
    plugins.sort();
    let (machine, resources) = machine_info(m.system_statistics.as_ref(), ps);
    let meta = MetaInfo {
        machine,
        resources,
        kind: match p.mode {
            crate::model::Mode::Allocation if kind.contains("sampler") => crate::i18n::t(
                "profilo allocazioni (sampler)",
                "allocation profile (sampler)",
            )
            .to_string(),
            crate::model::Mode::Lock if kind.contains("sampler") => {
                crate::i18n::t("profilo lock (sampler)", "lock profile (sampler)").to_string()
            }
            _ => kind.to_string(),
        },
        origin: origin.to_string(),
        viewer_url,
        platform: format!(
            "{} {}",
            if pm.brand.is_empty() {
                &pm.name
            } else {
                &pm.brand
            },
            pm.version
        )
        .trim()
        .to_string(),
        minecraft_version: pm.minecraft_version.clone(),
        spark_version: pm.spark_version.clone(),
        engine: match SamplerEngine::try_from(m.sampler_engine).unwrap_or(SamplerEngine::Java) {
            SamplerEngine::Java => "Java (WarmRoast)".into(),
            SamplerEngine::Async => format!("async-profiler {}", m.sampler_engine_version),
        },
        mode: match p.mode {
            crate::model::Mode::Execution => "CPU".into(),
            crate::model::Mode::Allocation => crate::i18n::t("Allocazioni", "Allocations").into(),
            crate::model::Mode::Lock => "Lock".into(),
        },
        start_time: m.start_time,
        duration_s: p.duration_ms() as f64 / 1000.0,
        interval_ms: p.interval_ms(),
        creator: m
            .creator
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_default(),
        comment: m.comment.clone(),
        players: ps.map(|s| s.player_count).unwrap_or(0),
        plugin_count: plugins.len(),
        threads: p.threads.len(),
        ticks: p.effective_ticks(),
        tps,
        mspt_mean: mspt.map(|x| x.mean),
        mspt_p95: mspt.map(|x| x.percentile95),
        mspt_max: mspt.map(|x| x.max),
        ticked_threshold: p.ticked_threshold(),
        window_filter: p.window_filter.clone(),
        windows_total: if p.windows_total > 0 {
            p.windows_total
        } else {
            p.window_count()
        },
        uptime_s: ps.map(|s| s.uptime as f64 / 1000.0).filter(|v| *v > 0.0),
    };
    let hot_chunks = rules::hot_chunks(ps, 15)
        .into_iter()
        .map(|h| HotChunk {
            world: h.world,
            block_x: h.x * 16 + 8,
            block_z: h.z * 16 + 8,
            total: h.total,
            top: h.top,
        })
        .collect();
    let summary = summarize(&meta, &a, &findings);
    Report {
        meta,
        findings,
        analysis: a,
        hot_chunks,
        plugins,
        alloc,
        summary,
        config_changes,
    }
}

pub fn fmt_num(v: f64) -> String {
    if v.abs() >= 1000.0 {
        format!("{:.0}", v)
    } else if v.abs() >= 100.0 {
        format!("{v:.0}")
    } else if v.abs() >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}

pub fn fmt_time(ms: f64, unit: &str) -> String {
    if unit == "byte" {
        let b = ms;
        if b >= 1e9 {
            format!("{:.2} GB", b / 1e9)
        } else if b >= 1e6 {
            format!("{:.1} MB", b / 1e6)
        } else if b >= 1e3 {
            format!("{:.1} KB", b / 1e3)
        } else {
            format!("{b:.0} B")
        }
    } else if ms >= 60_000.0 {
        format!("{:.1} min", ms / 60_000.0)
    } else if ms >= 1000.0 {
        format!("{:.1} s", ms / 1000.0)
    } else {
        format!("{ms:.0} ms")
    }
}

pub fn fmt_duration(s: f64) -> String {
    let s = s.max(0.0) as i64;
    if s >= 3600 {
        format!("{}h {:02}m", s / 3600, (s % 3600) / 60)
    } else if s >= 60 {
        format!("{}m {:02}s", s / 60, s % 60)
    } else {
        format!("{s}s")
    }
}

pub fn fmt_utc(ms: i64) -> String {
    if ms <= 0 {
        return "-".into();
    }
    let secs = ms / 1000;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, mo, d) = civil_from_days(days);
    format!(
        "{y:04}-{mo:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        (rem % 3600) / 60
    )
}

/// "2:05 PM, 08/11/2026" (UTC; the report page rewrites it in the viewer's local time).
pub fn fmt_ampm(ms: i64) -> String {
    let secs = ms / 1000;
    let (y, mo, d) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    let h = rem / 3600;
    format!(
        "{}:{:02} {}, {d:02}/{mo:02}/{y}",
        (h + 11) % 12 + 1,
        rem % 3600 / 60,
        if h < 12 { "AM" } else { "PM" }
    )
}

#[test]
fn ampm_format() {
    assert_eq!(fmt_ampm(0), "12:00 AM, 01/01/1970");
    assert_eq!(fmt_ampm(1_778_249_100_000), "2:05 PM, 08/05/2026");
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
