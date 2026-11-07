use std::collections::HashMap;
use std::fmt::Write;

use anyhow::{Result, bail};
use serde::Serialize;

use crate::heap::{ClassOwners, Owner, count, is_server_class, short_class};
use crate::proto::HeapData;
use crate::report::{fmt_duration, fmt_time};

const STABLE_RATIO: f64 = 0.05;
const LEAK_MARGIN: f64 = 0.10;
const MIN_LEAK_BYTES: i64 = 512 * 1024;
const MIN_LISTENER_GROWTH: i64 = 1_000;
const RESTART_TOLERANCE_MS: i64 = 10 * 60_000;
const MIN_USEFUL_HOURS: f64 = 0.5;
const MAX_JAVA_ROWS: usize = 8;
const MAX_PLUGIN_CLASSES: usize = 5;
const MAX_MULTIPLE: f64 = 8.0;
const MULTIPLE_TOLERANCE: f64 = 0.10;

#[derive(Clone, Debug, Default, Serialize)]
pub struct SnapshotInfo {
    pub name: String,
    pub generated_ms: i64,
    pub uptime_ms: Option<i64>,
    pub players: i64,
    pub used_bytes: i64,
    pub max_bytes: i64,
    pub objects: i64,
    pub platform: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Verdict {
    Leak,
    ListenerLeak,
    GrowsWithPlayers,
    New,
    Shrinking,
    Stable,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        let t = crate::i18n::t;
        match self {
            Verdict::Leak => t("Leak confermato", "Leak confirmed"),
            Verdict::ListenerLeak => t("Leak confermato (listener)", "Leak confirmed (listeners)"),
            Verdict::GrowsWithPlayers => t("Cresce con i giocatori", "Grows with players"),
            Verdict::New => t("Nuovo", "New"),
            Verdict::Shrinking => t("In calo", "Shrinking"),
            Verdict::Stable => t("Stabile", "Stable"),
        }
    }

    pub fn is_leak(self) -> bool {
        matches!(self, Verdict::Leak | Verdict::ListenerLeak)
    }

    pub fn css(self) -> &'static str {
        match self {
            Verdict::Leak | Verdict::ListenerLeak => "lv2",
            Verdict::GrowsWithPlayers | Verdict::New => "lv1",
            Verdict::Shrinking | Verdict::Stable => "lv0",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ClassDelta {
    pub kind: String,
    pub before_instances: i64,
    pub after_instances: i64,
    pub before_size: i64,
    pub after_size: i64,
}

impl ClassDelta {
    pub fn instance_delta(&self) -> i64 {
        self.after_instances - self.before_instances
    }

    pub fn size_delta(&self) -> i64 {
        self.after_size - self.before_size
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PluginDelta {
    pub name: String,
    pub version: String,
    pub before: i64,
    pub after: i64,
    pub per_hour: f64,
    pub verdict: Verdict,
    pub reason: String,
    pub classes: Vec<ClassDelta>,
}

impl PluginDelta {
    pub fn delta(&self) -> i64 {
        self.after - self.before
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct JavaDelta {
    pub class: ClassDelta,
    pub owner_hint: Option<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct HeapComparison {
    pub before: SnapshotInfo,
    pub after: SnapshotInfo,
    pub swapped: bool,
    pub hours: f64,
    pub restarted: bool,
    pub heap_per_hour: f64,
    pub hours_to_full: Option<f64>,
    pub plugins: Vec<PluginDelta>,
    pub java: Vec<JavaDelta>,
    pub headline: String,
    pub explanation: Vec<String>,
    pub warnings: Vec<String>,
    pub actions: Vec<String>,
}

impl HeapComparison {
    pub fn leaks(&self) -> impl Iterator<Item = &PluginDelta> {
        self.plugins.iter().filter(|p| p.verdict.is_leak())
    }

    pub fn heap_delta(&self) -> i64 {
        self.after.used_bytes - self.before.used_bytes
    }
}

pub fn snapshot_info(h: &HeapData, name: &str) -> SnapshotInfo {
    let meta = h.metadata.as_ref();
    let stats = meta.and_then(|m| m.platform_statistics.as_ref());
    let heap = stats
        .and_then(|s| s.memory.as_ref())
        .and_then(|m| m.heap.as_ref());
    let platform = meta
        .and_then(|m| m.platform_metadata.as_ref())
        .map(|p| {
            let brand = if p.brand.is_empty() {
                &p.name
            } else {
                &p.brand
            };
            format!("{brand} {}", p.version).trim().to_string()
        })
        .unwrap_or_default();
    let mut used = 0i64;
    let mut objects = 0i64;
    for e in &h.entries {
        used += e.size;
        objects += e.instances as i64;
    }
    SnapshotInfo {
        name: name.to_string(),
        generated_ms: meta.map_or(0, |m| m.generated_time),
        uptime_ms: stats.map(|s| s.uptime).filter(|u| *u > 0),
        players: stats.map_or(0, |s| s.player_count),
        used_bytes: used,
        max_bytes: heap.map_or(0, |h| h.max.max(0)),
        objects,
        platform,
    }
}

pub fn describe_label(i: &SnapshotInfo) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(5);
    parts.push("heap summary".to_string());
    if i.generated_ms > 0 {
        parts.push(tf!(
            "creato {}",
            "created {}",
            crate::report::fmt_utc(i.generated_ms)
        ));
    }
    if let Some(u) = i.uptime_ms {
        parts.push(tf!(
            "server acceso da {}",
            "server up for {}",
            fmt_duration(u as f64 / 1000.0)
        ));
    }
    parts.push(tf!("{} giocatori", "{} players", i.players));
    parts.push(tf!(
        "heap {}",
        "heap {}",
        fmt_time(i.used_bytes as f64, "byte")
    ));
    parts.join(" · ")
}

fn load_heap(bytes: &[u8], name: &str) -> Result<HeapData> {
    let loaded = crate::loader::load_bytes(bytes, name)?;
    match loaded.payload {
        crate::loader::Payload::Heap(h) => Ok(*h),
        _ => bail!(
            "{}",
            tf!(
                "{name} non è un heap summary: crealo con /spark heapsummary",
                "{name} is not a heap summary: create it with /spark heapsummary"
            )
        ),
    }
}

pub fn describe_bytes(bytes: &[u8], name: &str) -> Result<SnapshotInfo> {
    Ok(snapshot_info(&load_heap(bytes, name)?, name))
}

pub fn compare_bytes(a: &[u8], name_a: &str, b: &[u8], name_b: &str) -> Result<HeapComparison> {
    let first = load_heap(a, name_a)?;
    let second = load_heap(b, name_b)?;
    Ok(compare_named(&first, name_a, &second, name_b))
}

pub fn compare(a: &HeapData, b: &HeapData) -> HeapComparison {
    compare_named(a, "", b, "")
}

fn class_map(h: &HeapData) -> HashMap<&str, (i64, i64)> {
    let mut m: HashMap<&str, (i64, i64)> = HashMap::with_capacity(h.entries.len());
    for e in &h.entries {
        let slot = m.entry(e.r#type.as_str()).or_insert((0, 0));
        slot.0 += e.instances as i64;
        slot.1 += e.size;
    }
    m
}

fn all_class_deltas(before: &HeapData, after: &HeapData) -> Vec<ClassDelta> {
    let old = class_map(before);
    let new = class_map(after);
    let mut out: Vec<ClassDelta> = Vec::with_capacity(new.len().max(old.len()));
    for (kind, (i2, s2)) in &new {
        let (i1, s1) = old.get(kind).copied().unwrap_or((0, 0));
        out.push(ClassDelta {
            kind: (*kind).to_string(),
            before_instances: i1,
            after_instances: *i2,
            before_size: s1,
            after_size: *s2,
        });
    }
    for (kind, (i1, s1)) in &old {
        if !new.contains_key(kind) {
            out.push(ClassDelta {
                kind: (*kind).to_string(),
                before_instances: *i1,
                after_instances: 0,
                before_size: *s1,
                after_size: 0,
            });
        }
    }
    out
}

fn ratio(delta: i64, base: i64) -> f64 {
    if base > 0 {
        delta as f64 / base as f64
    } else if delta > 0 {
        f64::INFINITY
    } else {
        0.0
    }
}

fn is_listener_like(kind: &str) -> bool {
    let c = short_class(kind);
    kind.contains("$$Lambda") || c.contains("Listener") || c.contains("Handler")
}

fn looks_per_player(kind: &str) -> bool {
    let c = short_class(kind).to_ascii_lowercase();
    ["player", "user", "session", "profile", "account", "member"]
        .iter()
        .any(|k| c.contains(k))
}

struct Context {
    hours: f64,
    player_growth: f64,
    players: i64,
}

fn judge(classes: &[ClassDelta], before: i64, after: i64, ctx: &Context) -> (Verdict, String) {
    let delta = after - before;
    let growth = ratio(delta, before);
    let per_hour = |n: i64| (n as f64 / ctx.hours).round() as i64;
    for c in classes {
        let d = c.instance_delta();
        if is_listener_like(&c.kind)
            && d >= MIN_LISTENER_GROWTH
            && ratio(d, c.before_instances) >= LEAK_MARGIN
        {
            return (
                Verdict::ListenerLeak,
                tf!(
                    "+{} {} all'ora: ne registra di nuovi (a ogni ingresso o evento) senza mai deregistrarli. Poca memoria, ma ogni evento li esegue tutti: il costo CPU cresce nel tempo",
                    "+{} {} per hour: it registers new ones (on every join or event) without ever unregistering them. Little memory, but every event runs all of them: the CPU cost grows over time",
                    count(per_hour(d)),
                    short_class(&c.kind)
                ),
            );
        }
    }
    if before == 0 && after > 0 {
        return (
            Verdict::New,
            crate::i18n::t(
                "compare solo nel secondo dump",
                "only present in the second dump",
            )
            .to_string(),
        );
    }
    let leak_threshold = LEAK_MARGIN.max(ctx.player_growth + LEAK_MARGIN);
    if growth >= leak_threshold && delta >= MIN_LEAK_BYTES {
        let top = classes.first();
        let reason = match top {
            Some(c) if looks_per_player(&c.kind) && ctx.players > 0 => tf!(
                "+{} oggetti {} all'ora con i giocatori quasi invariati: non libera i dati di chi esce. Circa {} oggetti per giocatore online",
                "+{} {} objects per hour with almost the same players: it doesn't free the data of players who leave. About {} objects per online player",
                count(per_hour(c.instance_delta())),
                short_class(&c.kind),
                count(c.after_instances / ctx.players.max(1))
            ),
            Some(c) => tf!(
                "+{} istanze di {} all'ora, più di quanto spieghino i giocatori: una mappa o cache che non viene mai svuotata",
                "+{} instances of {} per hour, more than the players explain: a map or cache that is never cleared",
                count(per_hour(c.instance_delta())),
                short_class(&c.kind)
            ),
            None => String::new(),
        };
        return (Verdict::Leak, reason);
    }
    if growth >= STABLE_RATIO {
        return (
            Verdict::GrowsWithPlayers,
            tf!(
                "+{:.0}% mentre i giocatori sono cambiati del {:+.0}%: crescita normale, ricontrolla nel prossimo confronto",
                "+{:.0}% while players changed by {:+.0}%: normal growth, check again in the next comparison",
                growth * 100.0,
                ctx.player_growth * 100.0
            ),
        );
    }
    if growth <= -STABLE_RATIO {
        return (
            Verdict::Shrinking,
            crate::i18n::t(
                "la memoria è scesa: nessun leak",
                "memory went down: no leak",
            )
            .to_string(),
        );
    }
    (Verdict::Stable, String::new())
}

fn compare_named(a: &HeapData, name_a: &str, b: &HeapData, name_b: &str) -> HeapComparison {
    let info_a = snapshot_info(a, name_a);
    let info_b = snapshot_info(b, name_b);
    let swapped = info_b.generated_ms < info_a.generated_ms;
    let (before, after, before_info, after_info) = if swapped {
        (b, a, info_b, info_a)
    } else {
        (a, b, info_a, info_b)
    };
    let elapsed_ms = (after_info.generated_ms - before_info.generated_ms).max(0);
    let hours = elapsed_ms as f64 / 3_600_000.0;
    let rate_hours = hours.max(1.0 / 60.0);
    let restarted = match (before_info.uptime_ms, after_info.uptime_ms) {
        (Some(u1), Some(u2)) => u2 < u1 || ((u2 - u1) - elapsed_ms).abs() > RESTART_TOLERANCE_MS,
        _ => false,
    };
    let ctx = Context {
        hours: rate_hours,
        player_growth: ratio(
            after_info.players - before_info.players,
            before_info.players,
        )
        .min(10.0),
        players: after_info.players,
    };

    let owners = ClassOwners::from_heaps(&[before, after]);
    let deltas = all_class_deltas(before, after);
    let mut by_plugin: Vec<Vec<ClassDelta>> = vec![Vec::new(); owners.plugins.len()];
    let mut by_package: Vec<(String, Vec<ClassDelta>)> = Vec::new();
    let mut builtin: Vec<ClassDelta> = Vec::new();
    for d in deltas {
        match owners.owner(&d.kind) {
            Owner::Plugin(i) => by_plugin[i].push(d),
            Owner::Package(g) => match by_package.iter_mut().find(|(k, _)| *k == g) {
                Some((_, v)) => v.push(d),
                None => by_package.push((g, vec![d])),
            },
            Owner::Builtin => builtin.push(d),
        }
    }

    let mut plugins: Vec<PluginDelta> = Vec::new();
    let groups = by_plugin
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            (
                owners.plugins[i].0.clone(),
                owners.plugins[i].1.clone(),
                v,
                false,
            )
        })
        .chain(
            by_package
                .into_iter()
                .map(|(g, v)| (g, String::new(), v, true)),
        );
    for (name, version, mut classes, unknown) in groups {
        if classes.is_empty() {
            continue;
        }
        classes.sort_by(|x, y| {
            y.size_delta()
                .cmp(&x.size_delta())
                .then(y.instance_delta().cmp(&x.instance_delta()))
        });
        let mut before_size = 0i64;
        let mut after_size = 0i64;
        for c in &classes {
            before_size += c.before_size;
            after_size += c.after_size;
        }
        let (verdict, reason) = judge(&classes, before_size, after_size, &ctx);
        if unknown && !verdict.is_leak() && after_size < 1_000_000 {
            continue;
        }
        classes.truncate(MAX_PLUGIN_CLASSES);
        plugins.push(PluginDelta {
            name: if unknown {
                tf!(
                    "{} (plugin non riconosciuto)",
                    "{} (unrecognised plugin)",
                    name
                )
            } else {
                name
            },
            version,
            before: before_size,
            after: after_size,
            per_hour: (after_size - before_size) as f64 / rate_hours,
            verdict,
            reason,
            classes,
        });
    }
    plugins.sort_by(|x, y| {
        x.verdict
            .cmp(&y.verdict)
            .then(y.delta().cmp(&x.delta()))
            .then(y.after.cmp(&x.after))
    });

    let java = java_deltas(builtin, &plugins, &before_info, &after_info);
    let heap_per_hour = (after_info.used_bytes - before_info.used_bytes) as f64 / rate_hours;
    let hours_to_full = if heap_per_hour > 0.0
        && after_info.max_bytes > after_info.used_bytes
        && plugins.iter().any(|p| p.verdict.is_leak())
    {
        Some((after_info.max_bytes - after_info.used_bytes) as f64 / heap_per_hour)
    } else {
        None
    };

    let mut c = HeapComparison {
        before: before_info,
        after: after_info,
        swapped,
        hours,
        restarted,
        heap_per_hour,
        hours_to_full,
        plugins,
        java,
        headline: String::new(),
        explanation: Vec::new(),
        warnings: Vec::new(),
        actions: Vec::new(),
    };
    summarize(&mut c);
    c
}

fn nearest_multiple(value: i64, unit: i64) -> Option<f64> {
    if unit <= 0 || value <= 0 {
        return None;
    }
    let r = value as f64 / unit as f64;
    let k = r.round();
    if !(1.0..=MAX_MULTIPLE).contains(&k) {
        return None;
    }
    let err = (r - k).abs() / k;
    (err <= MULTIPLE_TOLERANCE).then_some(err)
}

fn java_deltas(
    mut builtin: Vec<ClassDelta>,
    plugins: &[PluginDelta],
    before: &SnapshotInfo,
    after: &SnapshotInfo,
) -> Vec<JavaDelta> {
    builtin.retain(|c| c.size_delta() > 0 && c.instance_delta() > 0);
    builtin.sort_by(|x, y| y.size_delta().cmp(&x.size_delta()));
    builtin.truncate(MAX_JAVA_ROWS);
    let mut out = Vec::with_capacity(builtin.len());
    for c in builtin {
        let mut hint: Option<(f64, &PluginDelta, &ClassDelta)> = None;
        if c.kind.starts_with("java.") {
            for p in plugins.iter().filter(|p| p.verdict.is_leak()) {
                for pc in &p.classes {
                    if let Some(err) = nearest_multiple(c.instance_delta(), pc.instance_delta())
                        && hint.is_none_or(|(e, _, _)| err < e)
                    {
                        hint = Some((err, p, pc));
                    }
                }
            }
        }
        let reason = match hint {
            Some((_, p, pc)) if c.instance_delta() == pc.instance_delta() => tf!(
                "{}: +{} come {} (uno per ogni voce)",
                "{}: +{} like {} (one per entry)",
                p.name,
                count(c.instance_delta()),
                short_class(&pc.kind)
            ),
            Some((_, p, pc)) => tf!(
                "{}: cresce allo stesso ritmo di {}",
                "{}: grows at the same rate as {}",
                p.name,
                short_class(&pc.kind)
            ),
            None if is_server_class(&c.kind) => {
                let dp = after.players - before.players;
                if dp > 0 {
                    tf!(
                        "server: dati dei {dp} giocatori in più",
                        "server: data for the {dp} extra players"
                    )
                } else {
                    crate::i18n::t("server (Minecraft)", "server (Minecraft)").to_string()
                }
            }
            None => crate::i18n::t(
                "non attribuibile (chunk, rete, stringhe e cache del server)",
                "not attributable (chunks, network, strings and server caches)",
            )
            .to_string(),
        };
        out.push(JavaDelta {
            owner_hint: hint.map(|(_, p, _)| p.name.clone()),
            class: c,
            reason,
        });
    }
    out
}

fn summarize(c: &mut HeapComparison) {
    let t = crate::i18n::t;
    let bytes = |v: f64| fmt_time(v, "byte");
    let leaks: Vec<&PluginDelta> = c.plugins.iter().filter(|p| p.verdict.is_leak()).collect();
    if c.restarted {
        c.warnings.push(t(
            "Il server è stato riavviato tra i due dump (l'uptime non torna): il confronto non è affidabile. Rifai i due /spark heapsummary senza riavviare in mezzo.",
            "The server was restarted between the two dumps (the uptime doesn't add up): the comparison is not reliable. Take both /spark heapsummary again without restarting in between.",
        ).to_string());
    }
    if c.hours < MIN_USEFUL_HOURS {
        c.warnings.push(tf!(
            "I due dump sono a soli {} di distanza: per distinguere un leak dal normale servono almeno 1-2 ore con giocatori che entrano ed escono.",
            "The two dumps are only {} apart: telling a leak from normal use needs at least 1-2 hours with players joining and leaving.",
            fmt_duration(c.hours * 3600.0)
        ));
    }
    let players_text = match c.after.players - c.before.players {
        0 => t("con gli stessi giocatori", "with the same players").to_string(),
        d if d > 0 => tf!("con {d} giocatori in più", "with {d} more players"),
        d => tf!("con {} giocatori in meno", "with {} fewer players", -d),
    };
    let delta = c.heap_delta() as f64;
    c.explanation.push(if delta >= 0.0 {
        tf!(
            "In {} l'heap è salito di {} {players_text}.",
            "In {} the heap went up by {} {players_text}.",
            fmt_duration(c.hours * 3600.0),
            bytes(delta)
        )
    } else {
        tf!(
            "In {} l'heap è sceso di {} {players_text}.",
            "In {} the heap went down by {} {players_text}.",
            fmt_duration(c.hours * 3600.0),
            bytes(-delta)
        )
    });
    if let Some(h) = c.hours_to_full {
        c.explanation.push(tf!(
            "Con questo ritmo l'heap ({}) sarà pieno tra circa {} ore: da lì il garbage collector bloccherà il server e poi arriverà il crash.",
            "At this rate the heap ({}) will be full in about {} hours: from then on the garbage collector will freeze the server, then it will crash.",
            bytes(c.after.max_bytes as f64),
            h.round().max(1.0)
        ));
    }
    c.headline = match leaks.first() {
        Some(_) => {
            let total_rate: f64 = leaks.iter().map(|p| p.per_hour.max(0.0)).sum();
            let names: Vec<String> = leaks.iter().map(|p| display_name(p)).collect();
            tf!(
                "Memory leak confermato in {} · +{}/ora",
                "Memory leak confirmed in {} · +{}/hour",
                join_names(&names),
                bytes(total_rate)
            )
        }
        None if c
            .plugins
            .iter()
            .any(|p| p.verdict == Verdict::GrowsWithPlayers) =>
        {
            t(
                "Nessun leak: la memoria cresce solo insieme ai giocatori",
                "No leak: memory only grows together with the players",
            )
            .to_string()
        }
        None => t(
            "Nessun leak: la memoria dei plugin è stabile",
            "No leak: plugin memory is stable",
        )
        .to_string(),
    };
    if !leaks.is_empty() {
        let names: Vec<String> = leaks.iter().map(|p| display_name(p)).collect();
        let names = join_names(&names);
        c.actions.push(tf!(
            "Aggiorna {names} e leggi il changelog (cerca «memory leak», «leak», «memory»); se non è citato, segnala allo sviluppatore allegando i due heap summary: questo confronto è la prova",
            "Update {names} and read the changelog (look for “memory leak”, “leak”, “memory”); if it isn't mentioned, report it to the developer attaching both heap summaries: this comparison is the evidence"
        ));
    }
    for p in &leaks {
        let per_player = p.classes.first().is_some_and(|k| looks_per_player(&k.kind));
        if p.verdict == Verdict::ListenerLeak {
            c.actions.push(tf!(
                "Se {} è tuo o open source: registra i listener una sola volta in onEnable, deregistrali con HandlerList.unregisterAll e cancella i task in onDisable; non crearne uno nuovo a ogni ingresso",
                "If {} is yours or open source: register listeners once in onEnable, unregister them with HandlerList.unregisterAll and cancel tasks in onDisable; don't create a new one on every join",
                p.name
            ));
        } else if per_player {
            c.actions.push(tf!(
                "Se {} è tuo o open source: tieni i dati per giocatore in una ConcurrentHashMap con chiave UUID e rimuovili in PlayerQuitEvent, oppure usa una cache Caffeine con maximumSize ed expireAfterAccess: una cache senza limite è un memory leak",
                "If {} is yours or open source: keep per-player data in a ConcurrentHashMap keyed by UUID and remove it in PlayerQuitEvent, or use a Caffeine cache with maximumSize and expireAfterAccess: a cache without a limit is a memory leak",
                p.name
            ));
        } else {
            c.actions.push(tf!(
                "Se {} è tuo o open source: dai un limite alla cache (Caffeine con maximumSize) e invalidala al reload sostituendo l'istanza",
                "If {} is yours or open source: give the cache a limit (Caffeine with maximumSize) and invalidate it on reload by replacing the instance",
                p.name
            ));
        }
    }
    if let Some(h) = c.hours_to_full {
        let worst = leaks.first().map(|p| p.name.clone()).unwrap_or_default();
        c.actions.push(tf!(
            "Nel frattempo riavvia il server prima delle ~{} ore stimate, oppure disattiva {worst} se non è essenziale",
            "Meanwhile restart the server before the estimated ~{} hours, or disable {worst} if it isn't essential",
            h.round().max(1.0)
        ));
    }
    if !leaks.is_empty() {
        let names: Vec<String> = leaks.iter().map(|p| p.name.clone()).collect();
        let list = join_names(&names);
        c.actions.push(if names.len() > 1 {
            tf!(
                "Dopo l'aggiornamento rifai due dump a qualche ora di distanza: {list} devono risultare «Stabile»",
                "After updating take two dumps a few hours apart again: {list} should come out as “Stable”"
            )
        } else {
            tf!(
                "Dopo l'aggiornamento rifai due dump a qualche ora di distanza: {list} deve risultare «Stabile»",
                "After updating take two dumps a few hours apart again: {list} should come out as “Stable”"
            )
        });
    } else {
        c.actions.push(t(
            "Nessun intervento necessario sui plugin. Se il server va comunque in lag col tempo, controlla GC e chunk caricati con un profilo CPU (/spark profiler start --timeout 300).",
            "No action needed on plugins. If the server still lags over time, check GC and loaded chunks with a CPU profile (/spark profiler start --timeout 300).",
        ).to_string());
    }
}

fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} {} {last}", rest.join(", "), crate::i18n::t("e", "and")),
    }
}

pub fn signed_bytes(v: i64) -> String {
    match v {
        0 => "0".to_string(),
        v if v > 0 => format!("+{}", fmt_time(v as f64, "byte")),
        v => format!("-{}", fmt_time(-v as f64, "byte")),
    }
}

pub fn display_name(p: &PluginDelta) -> String {
    if p.version.is_empty() {
        p.name.clone()
    } else {
        format!("{} {}", p.name, p.version)
    }
}

pub fn render_text(c: &HeapComparison) -> String {
    let t = crate::i18n::t;
    let bytes = |v: i64| fmt_time(v as f64, "byte");
    let mut o = String::with_capacity(4096);
    let _ = writeln!(
        o,
        "\n━━ Spark-Analyzer · {} ━━",
        t("confronto heap", "heap comparison")
    );
    let _ = writeln!(o, "{}", c.headline);
    for line in c.warnings.iter().chain(&c.explanation) {
        let _ = writeln!(o, "  {line}");
    }
    let _ = writeln!(
        o,
        "  {}: {} → {} · {}: {} → {}\n",
        t("Heap", "Heap"),
        bytes(c.before.used_bytes),
        bytes(c.after.used_bytes),
        t("giocatori", "players"),
        c.before.players,
        c.after.players
    );
    let _ = writeln!(o, "{}", t("■ Per plugin", "■ By plugin"));
    for p in &c.plugins {
        let _ = writeln!(
            o,
            "  {:<28} {:>10} → {:<10} {:>11}  [{}]",
            display_name(p),
            bytes(p.before),
            bytes(p.after),
            signed_bytes(p.delta()),
            p.verdict.label()
        );
        if !p.reason.is_empty() {
            let _ = writeln!(o, "      → {}", p.reason);
        }
    }
    if !c.java.is_empty() {
        let _ = writeln!(
            o,
            "\n{}",
            t("■ Classi Java cresciute", "■ Java classes that grew")
        );
        for j in &c.java {
            let _ = writeln!(
                o,
                "  {:<44} +{:<12} {}",
                j.class.kind,
                count(j.class.instance_delta()),
                j.reason
            );
        }
    }
    let _ = writeln!(o, "\n{}", t("■ Cosa fare", "■ What to do"));
    for (i, a) in c.actions.iter().enumerate() {
        let _ = writeln!(o, "  {}. {a}", i + 1);
    }
    o
}
