use std::collections::HashMap;

use serde::Serialize;

use crate::analysis::Stat;
use crate::model::{Node, Profile, Thread};
use crate::patterns::{self, Cat};
use crate::report::{fmt_num, fmt_time};
use crate::rules::{Finding, Severity, short};

/// Abitudini di codice riconoscibili dallo stack che creano molti oggetti.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum Habit {
    Strings,
    Streams,
    Boxing,
    Regex,
    Reflection,
    Locations,
    ItemCopies,
    Collections,
    Lambdas,
    NettyBuffers,
}

impl Habit {
    pub fn label(self) -> &'static str {
        let t = crate::i18n::t;
        match self {
            Habit::Strings => t("Costruzione di stringhe", "String building"),
            Habit::Streams => t(
                "Stream API (java.util.stream)",
                "Stream API (java.util.stream)",
            ),
            Habit::Boxing => t(
                "Boxing di numeri (Integer/Double…)",
                "Number boxing (Integer/Double…)",
            ),
            Habit::Regex => t("Espressioni regolari", "Regular expressions"),
            Habit::Reflection => t("Reflection", "Reflection"),
            Habit::Locations => t("Location / Vector di Bukkit", "Bukkit Location / Vector"),
            Habit::ItemCopies => t("Copie di ItemStack", "ItemStack copies"),
            Habit::Collections => t("Liste e mappe che crescono", "Growing lists and maps"),
            Habit::Lambdas => t("Iteratori e lambda", "Iterators and lambdas"),
            Habit::NettyBuffers => t("Buffer di rete (Netty)", "Network buffers (Netty)"),
        }
    }

    pub fn advice(self) -> &'static str {
        let t = crate::i18n::t;
        match self {
            Habit::Strings => t(
                "Stringhe costruite a ogni tick o evento (String.format, concatenazioni, messaggi e placeholder rigenerati): vanno messe in cache o costruite solo quando servono, per esempio solo se il messaggio viene davvero inviato o il log è abilitato",
                "Strings built on every tick or event (String.format, concatenation, messages and placeholders rebuilt): cache them or build them only when needed, e.g. only if the message is actually sent or logging is enabled",
            ),
            Habit::Streams => t(
                "Gli Stream creano più oggetti per ogni chiamata (pipeline, lambda, collector): nei percorsi caldi (eventi di movimento, task a ogni tick) un ciclo for normale alloca molto meno",
                "Streams create several objects on every call (pipeline, lambdas, collectors): in hot paths (movement events, per-tick tasks) a plain for loop allocates far less",
            ),
            Habit::Boxing => t(
                "Numeri primitivi trasformati in oggetti (Map<Integer,…>, List<Double>): le collezioni primitive (fastutil, già incluso nel server) evitano un oggetto per ogni valore",
                "Primitive numbers turned into objects (Map<Integer,…>, List<Double>): primitive collections (fastutil, already bundled with the server) avoid one object per value",
            ),
            Habit::Regex => t(
                "Pattern regex compilati o usati a ogni chiamata (String.split, replaceAll, matches): va compilato una volta sola con Pattern.compile in un campo statico",
                "Regex patterns compiled or used on every call (String.split, replaceAll, matches): compile once with Pattern.compile into a static field",
            ),
            Habit::Reflection => t(
                "Chiamate via reflection ripetute: il MethodHandle o il Field va cercato una volta e riusato",
                "Repeated reflective calls: look up the MethodHandle or Field once and reuse it",
            ),
            Habit::Locations => t(
                "getLocation(), clone() e new Location/Vector creano un oggetto nuovo a ogni chiamata: nei listener di movimento conviene riusare una Location con getLocation(Location) o confrontare direttamente le coordinate a blocchi",
                "getLocation(), clone() and new Location/Vector create a new object on every call: in movement listeners reuse a Location with getLocation(Location) or compare block coordinates directly",
            ),
            Habit::ItemCopies => t(
                "Copie di ItemStack (getItemInMainHand, getContents, asBukkitCopy, clone): ogni copia duplica anche i componenti/NBT. Leggere l'inventario una volta per tick invece che per ogni controllo riduce molto le copie",
                "ItemStack copies (getItemInMainHand, getContents, asBukkitCopy, clone): every copy also duplicates components/NBT. Reading the inventory once per tick instead of once per check cuts copies a lot",
            ),
            Habit::Collections => t(
                "Liste e mappe ricreate di continuo o che crescono a scatti (ArrayList.grow, HashMap.resize): vanno riusate (clear()) o create con la capacità giusta",
                "Lists and maps recreated all the time or growing in steps (ArrayList.grow, HashMap.resize): reuse them (clear()) or create them with the right capacity",
            ),
            Habit::Lambdas => t(
                "Iteratori, lambda catturanti e Optional creati in cicli molto frequenti: un for indicizzato o un controllo esplicito evita l'oggetto",
                "Iterators, capturing lambdas and Optionals created in very frequent loops: an indexed for loop or an explicit check avoids the object",
            ),
            Habit::NettyBuffers => t(
                "Buffer dei pacchetti: è normale che la rete allochi. Se è molto alto controlla compressione (network-compression-threshold 256 in server.properties) e quante entità/chunk vengono inviati (view-distance, entity-tracking-range)",
                "Packet buffers: some network allocation is normal. If it is very high check compression (network-compression-threshold 256 in server.properties) and how many entities/chunks are sent (view-distance, entity-tracking-range)",
            ),
        }
    }
}

fn habit_of(n: &Node) -> Option<Habit> {
    let c = n.class.as_str();
    let m = n.method.as_str();
    if c == "java.lang.StringBuilder"
        || c == "java.lang.AbstractStringBuilder"
        || c == "java.lang.StringConcatHelper"
        || c == "java.lang.StringLatin1"
        || c == "java.lang.StringUTF16"
        || c == "java.util.Formatter"
        || c.starts_with("java.lang.invoke.StringConcat")
        || c == "java.lang.String"
            && matches!(
                m,
                "format"
                    | "formatted"
                    | "concat"
                    | "replace"
                    | "toLowerCase"
                    | "toUpperCase"
                    | "substring"
                    | "join"
                    | "valueOf"
                    | "<init>"
                    | "repeat"
                    | "trim"
                    | "strip"
            )
    {
        return Some(Habit::Strings);
    }
    if c.starts_with("java.util.regex.")
        || c == "java.lang.String"
            && matches!(m, "split" | "replaceAll" | "replaceFirst" | "matches")
    {
        return Some(Habit::Regex);
    }
    if c.starts_with("java.util.stream.") {
        return Some(Habit::Streams);
    }
    if matches!(
        c,
        "java.lang.Integer"
            | "java.lang.Long"
            | "java.lang.Double"
            | "java.lang.Float"
            | "java.lang.Short"
            | "java.lang.Byte"
            | "java.lang.Character"
    ) && m == "valueOf"
    {
        return Some(Habit::Boxing);
    }
    if c.starts_with("java.lang.reflect.")
        || c.starts_with("jdk.internal.reflect.")
        || c == "java.lang.Class"
            && m.starts_with("get")
            && (m.contains("Method") || m.contains("Field"))
    {
        return Some(Habit::Reflection);
    }
    if c == "org.bukkit.Location"
        && matches!(
            m,
            "clone" | "<init>" | "add" | "subtract" | "toVector" | "getBlock" | "getDirection"
        )
        || c == "org.bukkit.util.Vector" && matches!(m, "clone" | "<init>" | "toLocation")
        || c.starts_with("org.bukkit.craftbukkit")
            && matches!(m, "getLocation" | "getEyeLocation" | "getVelocity")
    {
        return Some(Habit::Locations);
    }
    if c.ends_with("CraftItemStack")
        && matches!(m, "asBukkitCopy" | "asNMSCopy" | "asCraftCopy" | "clone")
        || c == "org.bukkit.inventory.ItemStack" && matches!(m, "clone" | "<init>")
        || c == "net.minecraft.world.item.ItemStack" && matches!(m, "copy" | "copyWithCount")
        || c.ends_with("CraftInventory")
            && matches!(m, "getContents" | "getStorageContents" | "getItem")
        || c.ends_with("CraftInventoryPlayer")
            && matches!(
                m,
                "getItemInMainHand" | "getItemInOffHand" | "getItemInHand" | "getArmorContents"
            )
    {
        return Some(Habit::ItemCopies);
    }
    if matches!(
        c,
        "java.util.ArrayList"
            | "java.util.HashMap"
            | "java.util.HashSet"
            | "java.util.LinkedHashMap"
            | "java.util.ArrayDeque"
    ) && matches!(m, "grow" | "resize" | "<init>" | "addAll" | "putAll")
        || c == "java.util.Arrays" && m == "copyOf"
        || c.starts_with("it.unimi.dsi.fastutil.")
            && matches!(m, "grow" | "rehash" | "ensureCapacity")
    {
        return Some(Habit::Collections);
    }
    if c.starts_with("java.lang.invoke.LambdaForm")
        || c.contains("$$Lambda")
        || c == "java.util.Optional"
        || (c.starts_with("java.util.")
            && (m == "iterator" || m == "spliterator" || m == "forEach"))
    {
        return Some(Habit::Lambdas);
    }
    if c.starts_with("io.netty.buffer.") {
        return Some(Habit::NettyBuffers);
    }
    None
}

fn is_library(class: &str) -> bool {
    const P: &[&str] = &[
        "java.",
        "javax.",
        "jdk.",
        "sun.",
        "com.sun.",
        "kotlin.",
        "kotlinx.",
        "scala.",
        "it.unimi.dsi.fastutil.",
        "com.google.common.",
        "com.google.gson.",
        "org.apache.commons.",
        "io.netty.",
        "com.mojang.serialization.",
        "com.mojang.datafixers.",
        "org.yaml.snakeyaml.",
        "com.fasterxml.jackson.",
        "org.bukkit.",
        "io.papermc.paper.",
        "com.destroystokyo.paper.",
        "net.kyori.",
    ];
    class.is_empty() || P.iter().any(|p| class.starts_with(p))
}

#[derive(Clone, Debug, Serialize)]
pub struct AllocSite {
    pub method: String,
    pub source: Option<String>,
    pub via: Option<String>,
    pub bytes: f64,
    pub pct: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct HabitStat {
    pub habit: Habit,
    pub label: String,
    pub bytes: f64,
    pub pct: f64,
    pub owners: Vec<Stat>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AllocReport {
    pub total: f64,
    pub duration_s: f64,
    pub rate_bps: Option<f64>,
    pub jvm_rate_bps: Option<f64>,
    pub main_thread: String,
    pub main_pct: f64,
    pub threads: Vec<Stat>,
    pub sources: Vec<Stat>,
    pub categories: Vec<Stat>,
    /// Plugin che allocano dentro ogni fase (chiave: etichetta della fase).
    pub category_owners: std::collections::BTreeMap<String, Vec<Stat>>,
    pub sites: Vec<AllocSite>,
    pub habits: Vec<HabitStat>,
}

pub fn fmt_rate(bps: f64) -> String {
    if bps >= 1e9 {
        format!("{:.2} GB/s", bps / 1e9)
    } else if bps >= 1e6 {
        format!("{:.0} MB/s", bps / 1e6)
    } else {
        format!("{:.0} KB/s", bps / 1e3)
    }
}

type SiteMap = HashMap<(String, Option<String>), (f64, HashMap<String, f64>)>;

struct Acc<'a> {
    p: &'a Profile,
    owner_bytes: HashMap<String, f64>,
    cat_bytes: HashMap<Cat, f64>,
    cat_owner: HashMap<(Cat, String), f64>,
    site_bytes: SiteMap,
    habit_bytes: HashMap<Habit, (f64, HashMap<String, f64>)>,
}

fn walk(acc: &mut Acc, t: &Thread) {
    struct St {
        id: u32,
        owner: Option<String>,
        site: Option<u32>,
        via: Option<u32>,
        habit: Option<Habit>,
        cat: Option<Cat>,
    }
    let mut stack: Vec<St> = t
        .roots
        .iter()
        .map(|r| St {
            id: *r,
            owner: None,
            site: None,
            via: None,
            habit: None,
            cat: None,
        })
        .collect();
    while let Some(st) = stack.pop() {
        let n = t.node(st.id);
        let src = acc
            .p
            .sources
            .resolve(n)
            .filter(|s| !acc.p.sources.is_builtin(s))
            .map(|s| s.to_string());
        let owner = src.or(st.owner);
        let lib = is_library(&n.class);
        let site = if lib { st.site } else { Some(st.id) };
        let via = if lib { st.via.or(Some(st.id)) } else { None };
        let habit = habit_of(n).or(st.habit);
        let cat = patterns::categorize(n).or(st.cat);
        let self_b = t.self_time(st.id);
        if self_b > 0.0 {
            if let Some(o) = &owner {
                *acc.owner_bytes.entry(o.clone()).or_insert(0.0) += self_b;
            }
            *acc.cat_bytes
                .entry(cat.unwrap_or(Cat::Other))
                .or_insert(0.0) += self_b;
            if let Some(o) = &owner {
                *acc.cat_owner
                    .entry((cat.unwrap_or(Cat::Other), o.clone()))
                    .or_insert(0.0) += self_b;
            }
            let (site_id, via) = match (site, via) {
                (Some(s), Some(v)) => (s, Some(t.node(v).short_label())),
                (Some(s), None) => (s, None),
                (None, _) => (st.id, None),
            };
            let key = (t.node(site_id).label(), owner.clone());
            let e = acc
                .site_bytes
                .entry(key)
                .or_insert_with(|| (0.0, HashMap::new()));
            e.0 += self_b;
            if let Some(v) = via {
                *e.1.entry(v).or_insert(0.0) += self_b;
            }
            if let Some(h) = habit {
                let e = acc
                    .habit_bytes
                    .entry(h)
                    .or_insert_with(|| (0.0, HashMap::new()));
                e.0 += self_b;
                let who = owner
                    .clone()
                    .unwrap_or_else(|| crate::i18n::t("server/JVM", "server/JVM").to_string());
                *e.1.entry(who).or_insert(0.0) += self_b;
            }
        }
        for c in &n.children {
            stack.push(St {
                id: *c,
                owner: owner.clone(),
                site,
                via,
                habit,
                cat,
            });
        }
    }
}

fn sorted(map: HashMap<String, f64>, total: f64, limit: usize) -> Vec<Stat> {
    let mut v: Vec<Stat> = map
        .into_iter()
        .filter(|(_, b)| *b > 0.0)
        .map(|(name, time)| Stat {
            name,
            time,
            pct: if total > 0.0 {
                time / total * 100.0
            } else {
                0.0
            },
        })
        .collect();
    v.sort_by(|a, b| b.time.total_cmp(&a.time).then(a.name.cmp(&b.name)));
    v.truncate(limit);
    v
}

pub fn analyze(p: &Profile) -> AllocReport {
    let mut acc = Acc {
        p,
        owner_bytes: HashMap::new(),
        cat_bytes: HashMap::new(),
        cat_owner: HashMap::new(),
        site_bytes: HashMap::new(),
        habit_bytes: HashMap::new(),
    };
    let mut thread_bytes: HashMap<String, f64> = HashMap::new();
    for t in &p.threads {
        if t.total <= 0.0 {
            continue;
        }
        *thread_bytes.entry(t.name.clone()).or_insert(0.0) += t.total;
        walk(&mut acc, t);
    }
    let total: f64 = thread_bytes.values().sum();
    let pct = |v: f64| if total > 0.0 { v / total * 100.0 } else { 0.0 };
    let main = &p.threads[p.main_thread_index()];
    let main_pct = pct(main.total.max(0.0));
    let duration_s = p.duration_ms() as f64 / 1000.0;
    let rate_bps = (duration_s > 0.0 && total > 0.0).then(|| total / duration_s);
    let jvm_rate_bps = p
        .meta
        .platform_statistics
        .as_ref()
        .and_then(|s| s.memory.as_ref())
        .and_then(|m| m.alloc_bps_last1m.as_ref())
        .map(|a| a.mean)
        .filter(|v| *v > 0.0);

    let sources = sorted(acc.owner_bytes, total, 30)
        .into_iter()
        .map(|mut s| {
            s.name = p.sources.display_name(&s.name);
            s
        })
        .collect();
    let mut by_cat: HashMap<Cat, HashMap<String, f64>> = HashMap::new();
    for ((c, o), b) in std::mem::take(&mut acc.cat_owner) {
        *by_cat.entry(c).or_default().entry(o).or_insert(0.0) += b;
    }
    let category_owners = by_cat
        .into_iter()
        .map(|(c, m)| {
            let v = sorted(m, total, 4)
                .into_iter()
                .map(|mut s| {
                    s.name = p.sources.display_name(&s.name);
                    s
                })
                .collect();
            (c.label().to_string(), v)
        })
        .collect();
    let mut categories: Vec<Stat> = acc
        .cat_bytes
        .into_iter()
        .map(|(c, b)| Stat {
            name: c.label().to_string(),
            time: b,
            pct: pct(b),
        })
        .collect();
    categories.sort_by(|a, b| b.time.total_cmp(&a.time));
    categories.truncate(20);

    let mut sites: Vec<AllocSite> = acc
        .site_bytes
        .into_iter()
        .map(|((method, source), (bytes, vias))| {
            let via = vias
                .into_iter()
                .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
                .filter(|(_, b)| *b >= bytes * 0.2)
                .map(|(v, _)| v);
            AllocSite {
                method,
                source: source.map(|s| p.sources.display_name(&s)),
                via,
                bytes,
                pct: pct(bytes),
            }
        })
        .collect();
    sites.sort_by(|a, b| b.bytes.total_cmp(&a.bytes).then(a.method.cmp(&b.method)));
    sites.truncate(60);

    let mut habits: Vec<HabitStat> = acc
        .habit_bytes
        .into_iter()
        .map(|(h, (bytes, owners))| HabitStat {
            habit: h,
            label: h.label().to_string(),
            bytes,
            pct: pct(bytes),
            owners: sorted(owners, total, 4)
                .into_iter()
                .map(|mut s| {
                    s.name = p.sources.display_name(&s.name);
                    s
                })
                .collect(),
        })
        .collect();
    habits.sort_by(|a, b| b.bytes.total_cmp(&a.bytes));

    AllocReport {
        total,
        duration_s,
        rate_bps,
        jvm_rate_bps,
        main_thread: main.name.clone(),
        main_pct,
        threads: sorted(thread_bytes, total, 15),
        sources,
        categories,
        category_owners,
        sites,
        habits,
    }
}

fn category_advice(name: &str) -> Option<&'static str> {
    let t = crate::i18n::t;
    let is = |c: Cat| name == c.label();
    if is(Cat::ChunkGen) {
        Some(t(
            "La generazione di chunk nuovi alloca moltissimo: pregenera il mondo (es. Chunky) e imposta un world border, così i giocatori non generano terreno mentre giocano",
            "Generating new chunks allocates a lot: pregenerate the world (e.g. Chunky) and set a world border, so players don't generate terrain while playing",
        ))
    } else if is(Cat::ChunkLoadSync) || is(Cat::ChunkSystem) || is(Cat::ChunkSave) {
        Some(t(
            "Caricamento e salvataggio chunk: riduci view-distance/simulation-distance o il numero di chunk tenuti caricati da plugin e spawn chunk",
            "Chunk loading and saving: lower view-distance/simulation-distance or the number of chunks kept loaded by plugins and spawn chunks",
        ))
    } else if is(Cat::EntityTracking) || is(Cat::Network) {
        Some(t(
            "Pacchetti verso i giocatori: riduci entity-tracking-range in spigot.yml e il numero di entità vicino ai giocatori",
            "Packets to players: lower entity-tracking-range in spigot.yml and the number of entities near players",
        ))
    } else if is(Cat::Serialization) {
        Some(t(
            "Serializzazione JSON/NBT/YAML: di solito un plugin che salva o legge dati troppo spesso; il salvataggio va fatto a intervalli o solo quando i dati cambiano",
            "JSON/NBT/YAML serialization: usually a plugin saving or reading data too often; saving should happen on an interval or only when the data changes",
        ))
    } else if is(Cat::TextFormatting) {
        Some(t(
            "Formattazione testo e placeholder: scoreboard, tab e hologram aggiornati troppo spesso. Alza l'intervallo di aggiornamento nelle loro config (es. 20-40 tick invece di 1-5)",
            "Text formatting and placeholders: scoreboards, tab lists and holograms updated too often. Raise their refresh interval in their configs (e.g. 20-40 ticks instead of 1-5)",
        ))
    } else if is(Cat::Logging) {
        Some(t(
            "Logging: qualcosa scrive in console di continuo. Controlla latest.log e disattiva il debug dei plugin indicati",
            "Logging: something writes to the console constantly. Check latest.log and turn off debug in the plugins listed",
        ))
    } else if is(Cat::Exceptions) {
        Some(t(
            "Eccezioni create di continuo (anche se gestite): ognuna copia l'intero stack. Cerca errori ripetuti in latest.log",
            "Exceptions created constantly (even if handled): each one copies the whole stack. Look for repeated errors in latest.log",
        ))
    } else {
        None
    }
}

fn sev_share(pct: f64) -> Severity {
    if pct >= 40.0 {
        Severity::Alto
    } else if pct >= 20.0 {
        Severity::Medio
    } else {
        Severity::Basso
    }
}

pub fn findings(r: &AllocReport, p: &Profile) -> Vec<Finding> {
    let t = crate::i18n::t;
    let area = t("Allocazioni", "Allocations").to_string();
    let mut out = Vec::new();
    let rate = r.jvm_rate_bps.or(r.rate_bps);
    let gc = p
        .meta
        .platform_statistics
        .as_ref()
        .map(|s| s.gc.clone())
        .unwrap_or_default();
    let young = gc
        .iter()
        .filter(|(k, g)| {
            let k = k.to_ascii_lowercase();
            g.total > 0
                && !(k.contains("old")
                    || k.contains("full")
                    || k.contains("mark") && !k.contains("young"))
        })
        .map(|(k, g)| (k.clone(), g.avg_frequency, g.avg_time))
        .min_by(|a, b| a.1.total_cmp(&b.1));

    let mut ev = vec![tf!(
        "Allocati {} in {} di profilo",
        "{} allocated over {} of profile",
        fmt_time(r.total, "byte"),
        crate::report::fmt_duration(r.duration_s)
    )];
    if let Some(v) = r.rate_bps {
        ev.push(tf!(
            "Velocità stimata dal profilo: {}",
            "Rate estimated from the profile: {}",
            fmt_rate(v)
        ));
    }
    if let Some(v) = r.jvm_rate_bps {
        ev.push(tf!(
            "Velocità misurata dalla JVM (ultimo minuto): {}",
            "Rate measured by the JVM (last minute): {}",
            fmt_rate(v)
        ));
    }
    ev.push(tf!(
        "Main thread «{}»: {}% delle allocazioni",
        "Main thread “{}”: {}% of allocations",
        r.main_thread,
        fmt_num(r.main_pct)
    ));
    for th in r.threads.iter().take(4) {
        ev.push(format!("{}: {}%", th.name, fmt_num(th.pct)));
    }
    if let Some((name, freq, avg)) = &young {
        ev.push(tf!(
            "{}: una raccolta ogni {} s, {} ms in media",
            "{}: one collection every {} s, {} ms on average",
            name,
            fmt_num(freq / 1000.0),
            fmt_num(*avg)
        ));
    }
    let mut sev = match rate {
        Some(v) if v >= 1.5e9 => Severity::Alto,
        Some(v) if v >= 7e8 => Severity::Medio,
        _ => Severity::Info,
    };
    if let Some((_, freq, _)) = &young
        && *freq > 0.0
        && *freq < 1000.0
        && sev > Severity::Medio
    {
        sev = Severity::Medio;
    }
    let mut sol = Vec::new();
    if let Some(s) = r.sources.first().filter(|s| s.pct >= 10.0) {
        sol.push(tf!(
            "Parti dal plugin che alloca di più ({}, {}%): le sue voci qui sotto dicono da dove arrivano gli oggetti",
            "Start from the plugin that allocates the most ({}, {}%): its entries below show where the objects come from",
            s.name,
            fmt_num(s.pct)
        ));
    }
    sol.push(t(
        "Meno allocazioni = GC più rari e più brevi. Se il GC resta frequente anche dopo, dai più heap (Xms = Xmx) e usa i flag di Aikar (vedi la sezione JVM)",
        "Less allocation = rarer and shorter GCs. If GC stays frequent afterwards, give more heap (Xms = Xmx) and use Aikar's flags (see the JVM section)",
    ).into());
    sol.push(
        t(
            "Per vedere cosa resta in memoria (non solo cosa viene creato) usa /spark heapsummary",
            "To see what stays in memory (not just what gets created) use /spark heapsummary",
        )
        .into(),
    );
    out.push(Finding {
        severity: sev,
        area: area.clone(),
        title: match rate {
            Some(v) => tf!("Allocazioni: {}", "Allocations: {}", fmt_rate(v)),
            None => t("Profilo di allocazione memoria", "Memory allocation profile").into(),
        },
        impact_pct: 0.0,
        evidence: ev,
        explanation: t(
            "Questo profilo misura i byte allocati, non il tempo CPU: le percentuali dicono chi crea più oggetti. Ogni oggetto creato va poi raccolto dal garbage collector, quindi chi alloca tanto rende i GC più frequenti e, con l'heap pieno, causa pause e lag spike. I valori sono campionati: contano le proporzioni più dei byte esatti.",
            "This profile measures allocated bytes, not CPU time: the percentages show who creates the most objects. Every object created must later be collected by the garbage collector, so heavy allocators make GC more frequent and, with a full heap, cause pauses and lag spikes. Values are sampled: the proportions matter more than the exact bytes.",
        ).into(),
        solutions: sol,
        culprits: r.sources.iter().take(3).map(|s| s.name.clone()).collect(),
    });

    for s in r.sources.iter().filter(|s| s.pct >= 10.0).take(5) {
        let mut ev = vec![tf!(
            "{}% di tutte le allocazioni{}",
            "{}% of all allocations{}",
            fmt_num(s.pct),
            rate.map(|v| format!(" (~{})", fmt_rate(v * s.pct / 100.0)))
                .unwrap_or_default()
        )];
        for site in r
            .sites
            .iter()
            .filter(|x| x.source.as_deref() == Some(&s.name))
            .take(4)
        {
            ev.push(match &site.via {
                Some(v) => tf!(
                    "{} → {}: {}%",
                    "{} → {}: {}%",
                    short(&site.method),
                    v,
                    fmt_num(site.pct)
                ),
                None => format!("{}: {}%", short(&site.method), fmt_num(site.pct)),
            });
        }
        let mut sol = Vec::new();
        if let Some(site) = r
            .sites
            .iter()
            .find(|x| x.source.as_deref() == Some(&s.name))
        {
            let via = site
                .via
                .as_deref()
                .map(|v| tf!(" tramite {v}", " via {v}"))
                .unwrap_or_default();
            sol.push(tf!(
                "{} ({}) crea il {}% di tutti gli oggetti{}: va corretto lì, riusando gli oggetti invece di crearne di nuovi a ogni chiamata",
                "{} ({}) creates {}% of all objects{}: fix it there, reusing objects instead of creating new ones on every call",
                s.name,
                short(&site.method),
                fmt_num(site.pct),
                via
            ));
        }
        for h in r
            .habits
            .iter()
            .filter(|h| h.owners.iter().any(|o| o.name == s.name && o.pct >= 2.0))
            .take(3)
        {
            sol.push(format!("{}: {}", h.label, h.habit.advice()));
        }
        sol.push(t(
            "Aggiorna il plugin e controlla nella sua config intervalli di aggiornamento, cache e funzioni che non usi (ogni funzione attiva crea oggetti a ogni tick)",
            "Update the plugin and check its config for refresh intervals, caches and features you don't use (every active feature creates objects every tick)",
        ).into());
        sol.push(t(
            "Se resta alto, segnala allo sviluppatore i metodi elencati: indicano esattamente dove vengono creati gli oggetti",
            "If it stays high, report the listed methods to the developer: they show exactly where objects are created",
        ).into());
        out.push(Finding {
            severity: sev_share(s.pct),
            area: area.clone(),
            title: tf!(
                "{} alloca il {}% della memoria",
                "{} allocates {}% of memory",
                s.name,
                fmt_num(s.pct)
            ),
            impact_pct: s.pct,
            evidence: ev,
            explanation: t(
                "Un plugin che crea molti oggetti non rallenta per forza il tick, ma alimenta il garbage collector: più GC significa più CPU usata e pause più frequenti.",
                "A plugin that creates many objects does not necessarily slow the tick, but it feeds the garbage collector: more GC means more CPU used and more frequent pauses.",
            ).into(),
            solutions: sol,
            culprits: vec![s.name.clone()],
        });
    }

    let server = crate::i18n::t("server/JVM", "server/JVM");
    let plugin_share = |h: &HabitStat| {
        h.owners
            .iter()
            .filter(|o| o.name != server)
            .map(|o| o.pct)
            .sum::<f64>()
    };
    for h in r
        .habits
        .iter()
        .filter(|h| h.pct >= 8.0 && plugin_share(h) >= 5.0)
        .take(4)
    {
        let mut ev = vec![tf!(
            "{}% di tutte le allocazioni",
            "{}% of all allocations",
            fmt_num(h.pct)
        )];
        for o in &h.owners {
            ev.push(format!("{}: {}%", o.name, fmt_num(o.pct)));
        }
        out.push(Finding {
            severity: if h.pct >= 25.0 { Severity::Medio } else { Severity::Basso },
            area: area.clone(),
            title: tf!("Allocazioni da: {}", "Allocations from: {}", h.label),
            impact_pct: h.pct,
            evidence: ev,
            explanation: t(
                "Lo stack mostra uno schema di codice che crea oggetti temporanei. Il responsabile è quasi sempre il plugin elencato più in alto.",
                "The stack shows a code pattern that creates temporary objects. The culprit is almost always the plugin listed first.",
            ).into(),
            solutions: vec![h.habit.advice().into()],
            culprits: h.owners.iter().map(|o| o.name.clone()).collect(),
        });
    }

    for c in r.categories.iter().filter(|c| c.pct >= 12.0).take(3) {
        let Some(adv) = category_advice(&c.name) else {
            continue;
        };
        let owners: Vec<&Stat> = r
            .category_owners
            .get(&c.name)
            .map(|v| v.iter().filter(|o| o.pct >= c.pct * 0.05).collect())
            .unwrap_or_default();
        let mut ev = vec![tf!(
            "{} allocati in questa fase",
            "{} allocated in this phase",
            fmt_time(c.time, "byte")
        )];
        if owners.is_empty() {
            ev.push(
                t(
                    "Nessun plugin compare negli stack di questa fase: le allocazioni partono dal server stesso",
                    "No plugin appears on the stacks of this phase: the allocations come from the server itself",
                )
                .into(),
            );
        }
        for o in &owners {
            ev.push(tf!(
                "{}: {}% delle allocazioni totali in questa fase",
                "{}: {}% of total allocations in this phase",
                o.name,
                fmt_num(o.pct)
            ));
        }
        out.push(Finding {
            severity: Severity::Basso,
            area: area.clone(),
            title: tf!(
                "{}: {}% delle allocazioni",
                "{}: {}% of allocations",
                c.name,
                fmt_num(c.pct)
            ),
            impact_pct: c.pct,
            evidence: ev,
            explanation: String::new(),
            solutions: vec![adv.into()],
            culprits: owners.iter().map(|o| o.name.clone()).collect(),
        });
    }
    out
}
