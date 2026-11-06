use std::collections::HashMap;
use std::fmt::Write;

use serde::Serialize;

use crate::proto::HeapData;
use crate::report::fmt_time;

#[derive(Clone, Debug, Serialize)]
pub struct HeapRow {
    pub kind: String,
    pub instances: i64,
    pub size: i64,
    pub pct: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct HeapReport {
    pub total_size: i64,
    pub total_instances: i64,
    pub top: Vec<HeapRow>,
    pub by_package: Vec<HeapRow>,
    pub notes: Vec<String>,
}

fn package_group(t: &str) -> String {
    let t = t.trim_end_matches("[]");
    if !t.contains('.') {
        return "array/primitivi".into();
    }
    let parts: Vec<&str> = t.split('.').collect();
    if t.starts_with("java.") || t.starts_with("jdk.") || t.starts_with("sun.") {
        return "JDK".into();
    }
    if t.starts_with("net.minecraft.") {
        return format!("minecraft.{}", parts.get(2).copied().unwrap_or(""));
    }
    let n = if parts.len() >= 4 {
        3
    } else {
        parts.len().saturating_sub(1).max(1)
    };
    parts[..n].join(".")
}

pub fn analyze(h: &HeapData) -> HeapReport {
    let total_size: i64 = h.entries.iter().map(|e| e.size).sum();
    let total_instances: i64 = h.entries.iter().map(|e| e.instances as i64).sum();
    let pct = |s: i64| {
        if total_size > 0 {
            s as f64 / total_size as f64 * 100.0
        } else {
            0.0
        }
    };
    let mut top: Vec<HeapRow> = h
        .entries
        .iter()
        .map(|e| HeapRow {
            kind: e.r#type.clone(),
            instances: e.instances as i64,
            size: e.size,
            pct: pct(e.size),
        })
        .collect();
    top.sort_by(|a, b| b.size.cmp(&a.size));
    let mut groups: HashMap<String, (i64, i64)> = HashMap::new();
    for e in &h.entries {
        let g = groups.entry(package_group(&e.r#type)).or_insert((0, 0));
        g.0 += e.instances as i64;
        g.1 += e.size;
    }
    let mut by_package: Vec<HeapRow> = groups
        .into_iter()
        .map(|(k, (i, s))| HeapRow {
            kind: k,
            instances: i,
            size: s,
            pct: pct(s),
        })
        .collect();
    by_package.sort_by(|a, b| b.size.cmp(&a.size));
    by_package.truncate(25);

    let count = |needle: &str| -> i64 {
        h.entries
            .iter()
            .filter(|e| e.r#type.ends_with(needle))
            .map(|e| e.instances as i64)
            .sum()
    };
    let mut notes = Vec::new();
    let chunks = count(".LevelChunk") + count(".Chunk");
    if chunks > 0 {
        notes.push(tf!("Chunk in memoria: circa {chunks}. Valori molto alti indicano view-distance alta o plugin che tengono chunk caricati.", "Chunks in memory: about {chunks}. Very high values indicate a high view-distance or plugins keeping chunks loaded."));
    }
    let items = count(".ItemEntity");
    if items > 2000 {
        notes.push(tf!(
            "{items} item a terra (ItemEntity): farm o drop che si accumulano.",
            "{items} dropped items (ItemEntity): farms or drops piling up."
        ));
    }
    let players = count(".ServerPlayer");
    if players > 0 {
        notes.push(tf!("{players} oggetti ServerPlayer: se sono molto più dei giocatori online c'è un memory leak (un plugin tiene riferimenti a giocatori usciti).", "{players} ServerPlayer objects: if this is far more than the online players there is a memory leak (a plugin keeps references to players who left)."));
    }
    let worlds = count(".ServerLevel");
    if worlds > 0 {
        notes.push(tf!("{worlds} mondi (ServerLevel) in memoria: se più dei mondi caricati, un plugin tiene riferimenti a mondi scaricati.", "{worlds} worlds (ServerLevel) in memory: if more than the loaded worlds, a plugin keeps references to unloaded worlds."));
    }
    for g in by_package.iter().take(12) {
        if !g.kind.starts_with("JDK")
            && !g.kind.starts_with("minecraft")
            && !g.kind.starts_with("array")
            && !g.kind.starts_with("it.unimi")
            && g.pct >= 5.0
        {
            notes.push(tf!("Il package {} occupa il {:.1}% dell'heap: probabile cache/dati di un plugin, verifica se cresce nel tempo.", "Package {} takes {:.1}% of the heap: probably a plugin's cache/data, check whether it grows over time.", g.kind, g.pct));
        }
    }
    top.truncate(50);
    HeapReport {
        total_size,
        total_instances,
        top,
        by_package,
        notes,
    }
}

pub fn render_text(r: &HeapReport) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "\n━━ sparkscope · heap summary ━━");
    let _ = writeln!(
        o,
        "{}",
        tf!(
            "Totale: {} in {} oggetti\n",
            "Total: {} in {} objects\n",
            fmt_time(r.total_size as f64, "byte"),
            r.total_instances
        )
    );
    if !r.notes.is_empty() {
        let _ = writeln!(o, "{}", crate::i18n::t("■ Osservazioni", "■ Observations"));
        for n in &r.notes {
            let _ = writeln!(o, "  • {n}");
        }
        let _ = writeln!(o);
    }
    let _ = writeln!(o, "{}", crate::i18n::t("■ Per package", "■ By package"));
    for g in r.by_package.iter().take(15) {
        let _ = writeln!(
            o,
            "  {:>6.2}%  {:>10}  {:>10} ist.  {}",
            g.pct,
            fmt_time(g.size as f64, "byte"),
            g.instances,
            g.kind
        );
    }
    let _ = writeln!(
        o,
        "{}",
        crate::i18n::t("\n■ Tipi più grandi", "\n■ Largest types")
    );
    for t in r.top.iter().take(30) {
        let _ = writeln!(
            o,
            "  {:>6.2}%  {:>10}  {:>10} ist.  {}",
            t.pct,
            fmt_time(t.size as f64, "byte"),
            t.instances,
            t.kind
        );
    }
    o
}
