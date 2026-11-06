pub mod cli;
pub mod html;
pub mod markdown;

use serde::Serialize;

use crate::analysis::Analysis;
use crate::model::Profile;
use crate::proto::sampler_metadata::SamplerEngine;
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
}

pub fn build(
    p: &Profile,
    a: Analysis,
    origin: &str,
    viewer_url: Option<String>,
    kind: &str,
) -> Report {
    let findings = rules::diagnose(&a, p);
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
    let meta = MetaInfo {
        kind: kind.to_string(),
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
            crate::model::Mode::Allocation => "Allocazioni".into(),
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
        ticks: m.number_of_ticks,
        tps,
        mspt_mean: mspt.map(|x| x.mean),
        mspt_p95: mspt.map(|x| x.percentile95),
        mspt_max: mspt.map(|x| x.max),
        ticked_threshold: p.ticked_threshold(),
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
    Report {
        meta,
        findings,
        analysis: a,
        hot_chunks,
        plugins,
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
