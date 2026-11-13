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
pub struct PluginMem {
    pub name: String,
    pub version: String,
    pub size: i64,
    pub instances: i64,
    pub pct: f64,
    pub classes: usize,
    pub top: Vec<HeapRow>,
    pub level: u8,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HeapReport {
    pub total_size: i64,
    pub total_instances: i64,
    pub top: Vec<HeapRow>,
    pub by_package: Vec<HeapRow>,
    pub by_plugin: Vec<PluginMem>,
    pub players: i64,
    pub notes: Vec<String>,
}

const KNOWN: &[(&str, &str)] = &[
    ("com.earth2me.essentials", "Essentials"),
    ("net.ess3", "Essentials"),
    ("com.sk89q.worldedit", "WorldEdit"),
    ("com.fastasyncworldedit", "FastAsyncWorldEdit"),
    ("com.sk89q.worldguard", "WorldGuard"),
    ("com.comphenix.protocol", "ProtocolLib"),
    ("net.milkbowl.vault", "Vault"),
    ("com.gmail.nossr50", "mcMMO"),
    ("me.clip.placeholderapi", "PlaceholderAPI"),
    ("com.palmergames.bukkit.towny", "Towny"),
    ("io.lumine.mythic", "MythicMobs"),
    ("com.ticxo.modelengine", "ModelEngine"),
    ("dev.lone.itemsadder", "ItemsAdder"),
    ("org.geysermc", "Geyser"),
    ("eu.decentsoftware.holograms", "DecentHolograms"),
    ("me.filoghost.holographicdisplays", "HolographicDisplays"),
    ("com.nisovin.shopkeepers", "Shopkeepers"),
    ("org.maxgamer.quickshop", "QuickShop"),
    ("com.ghostchu.quickshop", "QuickShop"),
    ("com.gamingmesh.jobs", "Jobs"),
    ("me.angeschossen.lands", "Lands"),
    ("com.github.retrooper.packetevents", "packetevents"),
    ("io.github.retrooper.packetevents", "packetevents"),
    ("me.neznamy.tab", "TAB"),
    ("github.scarsz.discordsrv", "DiscordSRV"),
    ("com.denizenscript", "Denizen"),
    ("ch.njol.skript", "Skript"),
    ("me.ryanhamshire.griefprevention", "GriefPrevention"),
    ("com.onarandombox.multiversecore", "Multiverse-Core"),
    ("org.mvplugins.multiverse", "Multiverse-Core"),
    ("net.citizensnpcs", "Citizens"),
    ("com.viaversion", "ViaVersion"),
    ("net.coreprotect", "CoreProtect"),
    ("litebans", "LiteBans"),
];

const SERVER: &[&str] = &[
    "net.minecraft.",
    "org.bukkit.",
    "org.spigotmc.",
    "io.papermc.",
    "com.destroystokyo.",
    "com.mojang.",
    "ca.spottedleaf.",
    "co.aikar.timings",
    "org.purpurmc.",
    "gg.pufferfish.",
    "net.md_5.",
];
const LIBS: &[&str] = &[
    "io.netty.",
    "it.unimi.dsi.",
    "com.google.",
    "org.apache.",
    "org.slf4j.",
    "net.kyori.",
    "org.joml.",
    "com.zaxxer.",
    "org.yaml.",
    "org.jetbrains.",
    "kotlin.",
    "com.fasterxml.",
    "org.objectweb.",
    "org.checkerframework.",
    "com.github.benmanes.caffeine.",
    "org.jline.",
    "org.fusesource.",
    "com.lmax.",
    "org.lz4.",
    "com.velocitypowered.",
];
const TLD: &[&str] = &[
    "com", "net", "org", "me", "io", "dev", "de", "fr", "it", "eu", "xyz", "gg", "uk", "co", "pl",
    "ru", "nl", "es", "br", "cz", "github", "gitlab", "gmail", "fun", "pw", "tk", "us", "ca",
];
const GENERIC: &[&str] = &[
    "api",
    "plugin",
    "plugins",
    "bukkit",
    "spigot",
    "paper",
    "core",
    "common",
    "util",
    "utils",
    "lib",
    "libs",
    "shaded",
    "internal",
    "impl",
    "minecraft",
    "server",
    "mc",
    "main",
    "example",
    "data",
    "model",
    "listener",
    "listeners",
    "command",
    "commands",
    "manager",
    "libraries",
];

fn norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    norm_into(s, &mut out);
    out
}

fn norm_into(s: &str, out: &mut String) {
    out.clear();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        }
    }
}

fn starts_with_ignore_case(t: &str, prefix: &str) -> bool {
    t.len() >= prefix.len() && t.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

fn main_packages(
    heaps: &[&HeapData],
    plugins: &[(String, String, String)],
) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut c = String::new();
    for h in heaps {
        for e in &h.entries {
            let t = e.r#type.trim_end_matches("[]");
            let Some((pkg, cls)) = t.rsplit_once('.') else {
                continue;
            };
            if pkg.starts_with("java.")
                || SERVER.iter().any(|p| t.starts_with(p))
                || pkg.split('.').count() < 2
            {
                continue;
            }
            norm_into(cls, &mut c);
            if c.len() < 4 {
                continue;
            }
            for (i, p) in plugins.iter().enumerate() {
                let n = &p.2;
                if n.len() >= 4
                    && (c == *n
                        || c.strip_suffix("plugin") == Some(n)
                        || c.strip_suffix("main") == Some(n))
                {
                    let key = format!("{pkg}.");
                    if !out.iter().any(|(k, _)| *k == key) {
                        out.push((key, i));
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    out
}

pub fn known_plugins() -> &'static [(&'static str, &'static str)] {
    KNOWN
}

pub fn is_builtin_class(t: &str) -> bool {
    !t.contains('.')
        || t.starts_with("java.")
        || t.starts_with("jdk.")
        || t.starts_with("sun.")
        || SERVER.iter().any(|p| t.starts_with(p))
        || LIBS.iter().any(|p| t.starts_with(p))
}

pub fn is_server_class(t: &str) -> bool {
    SERVER.iter().any(|p| t.starts_with(p))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    Plugin(usize),
    Package(String),
    Builtin,
}

pub struct ClassOwners {
    pub plugins: Vec<(String, String, String)>,
    mains: Vec<(String, usize)>,
    known: Vec<Option<usize>>,
    by_package: std::cell::RefCell<HashMap<String, Option<usize>>>,
    groups: std::cell::RefCell<HashMap<String, String>>,
    exact: HashMap<String, usize>,
}

impl ClassOwners {
    pub fn from_heaps(heaps: &[&HeapData]) -> ClassOwners {
        let mut plugins: Vec<(String, String, String)> = Vec::new();
        for h in heaps.iter().rev() {
            if let Some(m) = &h.metadata {
                for s in m.sources.values() {
                    if !s.builtin && !plugins.iter().any(|p| p.0 == s.name) {
                        plugins.push((s.name.clone(), s.version.clone(), norm(&s.name)));
                    }
                }
            }
        }
        plugins.sort();
        let mains = main_packages(heaps, &plugins);
        let mut exact: HashMap<String, usize> = HashMap::new();
        for h in heaps {
            let Some(lines) = h
                .metadata
                .as_ref()
                .and_then(|m| m.server_configurations.get(crate::hprof::PACKAGES_KEY))
            else {
                continue;
            };
            for line in lines.lines() {
                if let Some((pkg, name)) = line.split_once('\t')
                    && let Some(i) = plugins.iter().position(|p| p.0 == name)
                {
                    exact.entry(pkg.to_string()).or_insert(i);
                }
            }
        }
        let known = KNOWN
            .iter()
            .map(|(_, name)| {
                let n = norm(name);
                plugins
                    .iter()
                    .position(|p| p.2 == n || p.2.starts_with(&n) || n.starts_with(&p.2))
            })
            .collect();
        ClassOwners {
            plugins,
            mains,
            known,
            by_package: std::cell::RefCell::new(HashMap::new()),
            groups: std::cell::RefCell::new(HashMap::new()),
            exact,
        }
    }

    pub fn owner(&self, t: &str) -> Owner {
        if !self.exact.is_empty() {
            let bare = t.trim_end_matches("[]");
            if let Some((pkg, _)) = bare.rsplit_once('.')
                && let Some(i) = self.exact.get(pkg)
            {
                return Owner::Plugin(*i);
            }
        }
        if is_builtin_class(t) {
            return Owner::Builtin;
        }
        match self.owner_index(t) {
            Some(i) => Owner::Plugin(i),
            None => Owner::Package(self.group(t)),
        }
    }

    fn group(&self, t: &str) -> String {
        let bare = t.trim_end_matches("[]");
        let Some((pkg, _)) = bare.rsplit_once('.') else {
            return package_group(t);
        };
        if bare.starts_with("net.minecraft.") {
            return package_group(t);
        }
        if let Some(g) = self.groups.borrow().get(pkg) {
            return g.clone();
        }
        let g = package_group(t);
        self.groups.borrow_mut().insert(pkg.to_string(), g.clone());
        g
    }

    fn owner_index(&self, t: &str) -> Option<usize> {
        let t = t.trim_end_matches("[]");
        if let Some((_, i)) = self.mains.iter().find(|(k, _)| t.starts_with(k.as_str())) {
            return Some(*i);
        }
        for (k, (prefix, _)) in KNOWN.iter().enumerate() {
            if starts_with_ignore_case(t, prefix)
                && let Some(i) = self.known[k]
            {
                return Some(i);
            }
        }
        let pkg = t.rsplit_once('.').map_or("", |(p, _)| p);
        if let Some(hit) = self.by_package.borrow().get(pkg) {
            return *hit;
        }
        let hit = segment_owner(pkg, &self.plugins);
        self.by_package.borrow_mut().insert(pkg.to_string(), hit);
        hit
    }
}

fn segment_owner(pkg: &str, plugins: &[(String, String, String)]) -> Option<usize> {
    if pkg.is_empty() {
        return None;
    }
    let mut s = String::new();
    for (k, seg) in pkg.split('.').enumerate() {
        norm_into(seg, &mut s);
        if s.len() < 3 || (k == 0 && TLD.contains(&s.as_str())) || GENERIC.contains(&s.as_str()) {
            continue;
        }
        for (i, p) in plugins.iter().enumerate() {
            let n = &p.2;
            if n.len() < 3 {
                continue;
            }
            if s == *n
                || (s.len() >= 5
                    && n.len() >= 5
                    && (n.starts_with(s.as_str()) || s.starts_with(n.as_str())))
            {
                return Some(i);
            }
        }
    }
    None
}

pub fn count(n: i64) -> String {
    let sep = if crate::i18n::is_en() { ',' } else { '.' };
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(ch);
    }
    if n < 0 {
        out.insert(0, '-');
    }
    out
}

pub fn short_class(t: &str) -> &str {
    let c = t.rsplit('.').next().unwrap_or(t);
    c.split('/').next().unwrap_or(c)
}

fn is_lambda(t: &str) -> bool {
    t.contains("$$Lambda")
        || t.rsplit('$')
            .next()
            .is_some_and(|x| !x.is_empty() && x.chars().all(|c| c.is_ascii_digit()))
}

fn looks_per_player(t: &str) -> bool {
    let c = short_class(t).to_ascii_lowercase();
    ["player", "user", "session", "profile", "account", "member"]
        .iter()
        .any(|k| c.contains(k))
}

fn plugin_memory(h: &HeapData, total_size: i64, players: i64) -> Vec<PluginMem> {
    let owners = ClassOwners::from_heaps(&[h]);
    let mut plugins = owners.plugins.clone();
    let mut acc: Vec<Vec<&crate::proto::HeapEntry>> = vec![Vec::new(); plugins.len()];
    let mut unknown: Vec<(String, Vec<&crate::proto::HeapEntry>)> = Vec::new();
    let mut unknown_idx: HashMap<String, usize> = HashMap::new();
    for e in &h.entries {
        match owners.owner(&e.r#type) {
            Owner::Builtin => {}
            Owner::Plugin(i) => acc[i].push(e),
            Owner::Package(g) => match unknown_idx.get(&g) {
                Some(k) => unknown[*k].1.push(e),
                None => {
                    unknown_idx.insert(g.clone(), unknown.len());
                    unknown.push((g, vec![e]));
                }
            },
        }
    }
    let n_known = plugins.len();
    for (g, list) in unknown {
        plugins.push((g, String::new(), String::new()));
        acc.push(list);
    }
    let pct = |s: i64| {
        if total_size > 0 {
            s as f64 / total_size as f64 * 100.0
        } else {
            0.0
        }
    };
    let mut out: Vec<PluginMem> = Vec::new();
    for (i, list) in acc.into_iter().enumerate() {
        if list.is_empty() {
            continue;
        }
        let size: i64 = list.iter().map(|e| e.size).sum();
        let instances: i64 = list.iter().map(|e| e.instances as i64).sum();
        let mut top: Vec<HeapRow> = top_entries(&list, 6, true)
            .into_iter()
            .map(|e| HeapRow {
                kind: e.r#type.clone(),
                instances: e.instances as i64,
                size: e.size,
                pct: pct(e.size),
            })
            .collect();
        let mut level = 0u8;
        let mut reasons = Vec::new();
        let p = pct(size);
        if p >= 10.0 || size >= 1_000_000_000 {
            level = 2;
            reasons.push(tf!(
                "da solo occupa il {:.1}% dell'heap ({})",
                "on its own it takes {:.1}% of the heap ({})",
                p,
                fmt_time(size as f64, "byte")
            ));
        } else if p >= 3.0 || size >= 200_000_000 {
            level = 1;
            reasons.push(tf!(
                "occupa il {:.1}% dell'heap ({})",
                "takes {:.1}% of the heap ({})",
                p,
                fmt_time(size as f64, "byte")
            ));
        }
        for e in &list {
            let n = e.instances as i64;
            let c = short_class(&e.r#type);
            if is_lambda(&e.r#type) {
                if n >= 10_000 {
                    level = level.max(1);
                    reasons.push(tf!(
                        "{} lambda/classi anonime ({c}): listener o task creati di continuo e mai rimossi",
                        "{} lambdas/anonymous classes ({c}): listeners or tasks created over and over and never removed",
                        count(n)
                    ));
                }
            } else if c.contains("Listener") || c.contains("Handler") {
                if n > (players * 20).max(1_000) {
                    level = level.max(1);
                    reasons.push(tf!(
                        "{} istanze di {c}: viene registrato un listener nuovo (per giocatore o evento) che non viene mai deregistrato. Oltre alla memoria, ogni evento li esegue tutti e consuma CPU",
                        "{} instances of {c}: a new listener is registered (per player or event) and never unregistered. Besides memory, every event runs all of them and costs CPU",
                        count(n)
                    ));
                }
            } else if players > 0 && looks_per_player(&e.r#type) && n > (players * 50).max(5_000) {
                level = 2;
                reasons.push(tf!(
                    "{} oggetti {c} con solo {players} giocatori online (circa {} per giocatore): tiene in memoria i dati dei giocatori usciti",
                    "{} {c} objects with only {players} players online (about {} per player): it keeps the data of players who left in memory",
                    count(n),
                    count(n / players.max(1))
                ));
            } else if n >= 1_000_000 {
                level = 2;
                reasons.push(tf!(
                    "{} istanze di {c}: una mappa o cache che cresce senza limiti",
                    "{} instances of {c}: a map or cache growing without limits",
                    count(n)
                ));
            } else if n >= 200_000 {
                level = level.max(1);
                reasons.push(tf!(
                    "{} istanze di {c}: verifica che la cache abbia un limite o una scadenza",
                    "{} instances of {c}: check that the cache has a size limit or expiry",
                    count(n)
                ));
            }
        }
        top.truncate(6);
        let unknown_pkg = i >= n_known;
        if unknown_pkg && level == 0 && p < 1.0 {
            continue;
        }
        out.push(PluginMem {
            name: if unknown_pkg {
                tf!(
                    "{} (plugin non riconosciuto)",
                    "{} (unrecognised plugin)",
                    plugins[i].0
                )
            } else {
                plugins[i].0.clone()
            },
            version: plugins[i].1.clone(),
            size,
            instances,
            pct: p,
            classes: list.len(),
            top,
            level,
            reasons,
        });
    }
    out.sort_by(|a, b| b.level.cmp(&a.level).then(b.size.cmp(&a.size)));
    out
}

fn top_entries<'a>(
    list: &[&'a crate::proto::HeapEntry],
    k: usize,
    by_instances: bool,
) -> Vec<&'a crate::proto::HeapEntry> {
    let mut idx: Vec<usize> = (0..list.len()).collect();
    let cmp = |a: &usize, b: &usize| {
        let (x, y) = (list[*a], list[*b]);
        let c = y.size.cmp(&x.size);
        let c = if by_instances {
            c.then(y.instances.cmp(&x.instances))
        } else {
            c
        };
        c.then(a.cmp(b))
    };
    if idx.len() > k && k > 0 {
        idx.select_nth_unstable_by(k - 1, cmp);
        idx.truncate(k);
    }
    idx.sort_unstable_by(cmp);
    idx.into_iter().map(|i| list[i]).collect()
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
    let all: Vec<&crate::proto::HeapEntry> = h.entries.iter().collect();
    let mut top: Vec<HeapRow> = top_entries(&all, 50, false)
        .into_iter()
        .map(|e| HeapRow {
            kind: e.r#type.clone(),
            instances: e.instances as i64,
            size: e.size,
            pct: pct(e.size),
        })
        .collect();
    let mut groups: HashMap<String, (i64, i64)> = HashMap::new();
    let mut group_of: HashMap<&str, String> = HashMap::new();
    for e in &h.entries {
        let t = e.r#type.as_str();
        let bare = t.trim_end_matches("[]");
        let key = match bare.rsplit_once('.') {
            Some((pkg, _)) if !bare.starts_with("net.minecraft.") => Some(pkg),
            _ => None,
        };
        let name = match key {
            Some(pkg) => group_of
                .entry(pkg)
                .or_insert_with(|| package_group(t))
                .clone(),
            None => package_group(t),
        };
        let g = groups.entry(name).or_insert((0, 0));
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

    let players = h
        .metadata
        .as_ref()
        .and_then(|m| m.platform_statistics.as_ref())
        .map(|s| s.player_count)
        .unwrap_or(0);
    let by_plugin = plugin_memory(h, total_size, players);
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
    let sp = count(".ServerPlayer");
    if players > 0 && sp > players + players / 5 + 3 {
        notes.push(tf!("{sp} oggetti ServerPlayer ma solo {players} giocatori online: un plugin tiene riferimenti a giocatori usciti (memory leak). Spesso è una mappa Player → dati che non viene svuotata al quit.", "{sp} ServerPlayer objects but only {players} players online: a plugin keeps references to players who left (memory leak). It is often a Player → data map that is not cleared on quit."));
    } else if players == 0 && sp > 0 {
        let players = sp;
        notes.push(tf!("{players} oggetti ServerPlayer: se sono molto più dei giocatori online c'è un memory leak (un plugin tiene riferimenti a giocatori usciti).", "{players} ServerPlayer objects: if this is far more than the online players there is a memory leak (a plugin keeps references to players who left)."));
    }
    let worlds = count(".ServerLevel");
    if worlds > 0 {
        notes.push(tf!("{worlds} mondi (ServerLevel) in memoria: se più dei mondi caricati, un plugin tiene riferimenti a mondi scaricati.", "{worlds} worlds (ServerLevel) in memory: if more than the loaded worlds, a plugin keeps references to unloaded worlds."));
    }
    for g in by_package.iter().take(12) {
        if by_plugin.iter().any(|p| p.level > 0) {
            break;
        }
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
        by_plugin,
        players,
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
            count(r.total_instances)
        )
    );
    if !r.notes.is_empty() {
        let _ = writeln!(o, "{}", crate::i18n::t("■ Osservazioni", "■ Observations"));
        for n in &r.notes {
            let _ = writeln!(o, "  • {n}");
        }
        let _ = writeln!(o);
    }
    if !r.by_plugin.is_empty() {
        let _ = writeln!(
            o,
            "{}",
            crate::i18n::t("■ Memoria per plugin", "■ Memory by plugin")
        );
        for p in &r.by_plugin {
            let tag = match p.level {
                2 => crate::i18n::t("SOSPETTO LEAK", "SUSPECTED LEAK"),
                1 => crate::i18n::t("da tenere d'occhio", "keep an eye on"),
                _ => "ok",
            };
            let _ = writeln!(
                o,
                "  {:>6.2}%  {:>10}  {:>10} ist.  {} {}  [{}]",
                p.pct,
                fmt_time(p.size as f64, "byte"),
                p.instances,
                p.name,
                p.version,
                tag
            );
            for why in &p.reasons {
                let _ = writeln!(o, "            → {why}");
            }
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

pub fn findings(r: &HeapReport) -> Vec<crate::rules::Finding> {
    use crate::rules::{Finding, Severity};
    let t = crate::i18n::t;
    let mut out = Vec::new();
    for p in r.by_plugin.iter().filter(|p| p.level > 0) {
        let name = if p.version.is_empty() {
            p.name.clone()
        } else {
            format!("{} {}", p.name, p.version)
        };
        let mut evidence = vec![tf!(
            "Memoria occupata dalle classi del plugin: {} ({:.1}% dell'heap) in {} oggetti",
            "Memory taken by the plugin's classes: {} ({:.1}% of the heap) in {} objects",
            fmt_time(p.size as f64, "byte"),
            p.pct,
            count(p.instances)
        )];
        evidence.extend(
            p.reasons
                .iter()
                .filter(|x| !x.contains(t("dell'heap (", "of the heap (")))
                .cloned(),
        );
        for c in p.top.iter().take(3) {
            evidence.push(tf!(
                "{} · {} · {} oggetti",
                "{} · {} · {} objects",
                c.kind.split('/').next().unwrap_or(&c.kind),
                fmt_time(c.size as f64, "byte"),
                count(c.instances)
            ));
        }
        let per_player = p
            .reasons
            .iter()
            .any(|x| x.contains(t("giocatori usciti", "players who left")));
        let mut solutions = vec![
            tf!(
                "Aggiorna {} all'ultima versione e leggi il changelog: cerca «memory leak», «leak», «memory»",
                "Update {} to the latest version and read the changelog: look for “memory leak”, “leak”, “memory”",
                p.name
            ),
            t(
                "Conferma il leak: fai un secondo /spark heapsummary dopo qualche ora (o dopo che molti giocatori sono entrati e usciti). Se i numeri continuano a salire è un leak, se restano stabili è solo una cache grande",
                "Confirm the leak: take a second /spark heapsummary a few hours later (or after many players joined and left). If the numbers keep growing it is a leak, if they stay stable it is just a large cache",
            )
            .to_string(),
        ];
        if per_player {
            solutions.push(tf!(
                "Se il plugin è tuo o open source: i dati per giocatore vanno rimossi in PlayerQuitEvent (map.remove(uuid)), oppure usa una cache con scadenza (Caffeine expireAfterAccess)",
                "If the plugin is yours or open source: per-player data must be removed in PlayerQuitEvent (map.remove(uuid)), or use a cache with expiry (Caffeine expireAfterAccess)"
            ));
        }
        solutions.push(tf!(
            "Segnala allo sviluppatore di {} allegando i due heap summary: è il dato che serve per correggerlo",
            "Report it to the {} developer attaching the two heap summaries: that's what they need to fix it",
            p.name
        ));
        out.push(Finding {
            severity: if p.level >= 2 { Severity::Alto } else { Severity::Medio },
            area: t("Memoria", "Memory").into(),
            title: if p.level >= 2 {
                tf!("Possibile memory leak in {name}", "Possible memory leak in {name}")
            } else {
                tf!("{name} occupa molta memoria", "{name} uses a lot of memory")
            },
            impact_pct: p.pct,
            evidence,
            explanation: t(
                "Un heap summary mostra quanta memoria occupano gli oggetti di ogni classe in quel momento. Le classi del plugin sono riconosciute dal package. La memoria indicata è quella degli oggetti del plugin: i dati che tiene dentro HashMap, liste e array compaiono come classi Java e si aggiungono a questa cifra.",
                "A heap summary shows how much memory the objects of each class take at that moment. The plugin's classes are recognised by their package. The figure is the memory of the plugin's own objects: data it keeps inside HashMaps, lists and arrays shows up as Java classes and comes on top of it.",
            )
            .into(),
            solutions,
            culprits: vec![name],
        });
    }
    out
}
