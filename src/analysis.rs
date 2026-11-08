use rustc_hash::FxHashMap as HashMap;
use std::cmp::Ordering;

use serde::Serialize;

use crate::model::{Mode, Profile, Thread};
use crate::patterns::{self, Cat};

#[derive(Clone, Debug, Serialize)]
pub struct Stat {
    pub name: String,
    pub time: f64,
    pub pct: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CatStat {
    pub cat: Cat,
    pub label: String,
    pub time: f64,
    pub pct: f64,
    pub top_owners: Vec<Stat>,
    pub top_methods: Vec<Stat>,
    pub example_stack: Vec<String>,
    /// Plugin e suo metodo più interno da cui parte il lavoro di questa categoria.
    pub culprit_sites: Vec<CulpritSite>,
    /// Primo metodo (dal basso dello stack) in un pacchetto che spark non associa a nessun plugin.
    pub unknown_sites: Vec<Stat>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CulpritSite {
    pub source: String,
    pub method: String,
    pub time: f64,
    pub pct: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MethodStat {
    pub method: String,
    pub source: Option<String>,
    pub self_time: f64,
    pub self_pct: f64,
    pub incl_time: f64,
    pub incl_pct: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct EntryStat {
    pub source: String,
    pub entry: String,
    pub context: String,
    pub time: f64,
    pub pct: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SourceStat {
    pub source: String,
    pub display: String,
    pub self_time: f64,
    pub self_pct: f64,
    pub incl_time: f64,
    pub incl_pct: f64,
    pub main_thread: bool,
    pub top_entries: Vec<EntryStat>,
    pub top_methods: Vec<Stat>,
    pub hazards: Vec<Stat>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HotPath {
    pub time: f64,
    pub pct: f64,
    pub cat: Option<String>,
    pub owner: Option<String>,
    pub frames: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ThreadSummary {
    pub name: String,
    pub total: f64,
    pub busy: f64,
    pub busy_pct_of_all: f64,
    pub top_methods: Vec<Stat>,
    pub top_owner: Option<String>,
    pub plugin_hint: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WindowStat {
    pub index: usize,
    pub id: i32,
    pub start_time: i64,
    pub end_time: i64,
    pub ticks: i32,
    pub tps: f64,
    pub mspt_median: f64,
    pub mspt_max: f64,
    pub cpu_process: f64,
    pub cpu_system: f64,
    pub players: i32,
    pub entities: i32,
    pub tile_entities: i32,
    pub chunks: i32,
    pub busy: f64,
    pub busy_per_tick: f64,
    pub is_spike: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Delta {
    pub name: String,
    pub baseline: f64,
    pub value: f64,
    pub delta: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Spike {
    pub window: WindowStat,
    pub reason: String,
    pub unit: String,
    pub cat_deltas: Vec<Delta>,
    pub owner_deltas: Vec<Delta>,
    pub method_deltas: Vec<Delta>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MainStats {
    pub thread: String,
    pub total: f64,
    pub busy: f64,
    pub idle: f64,
    pub busy_pct: f64,
    pub ticks: i32,
    pub busy_per_tick: Option<f64>,
    pub blocking: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Analysis {
    pub mode: Mode,
    pub unit: String,
    pub main: MainStats,
    pub categories: Vec<CatStat>,
    pub sources: Vec<SourceStat>,
    pub buckets: Vec<Stat>,
    pub methods: Vec<MethodStat>,
    pub methods_inclusive: Vec<MethodStat>,
    pub entity_types: Vec<Stat>,
    pub block_entity_types: Vec<Stat>,
    pub events: Vec<EntryStat>,
    pub entries: Vec<EntryStat>,
    pub hot_paths: Vec<HotPath>,
    pub threads: Vec<ThreadSummary>,
    pub windows: Vec<WindowStat>,
    pub spikes: Vec<Spike>,
    pub periodic_spikes: Option<String>,
    #[serde(skip)]
    pub flame: crate::flame::FlameTree,
}

struct Acc {
    total: f64,
    win: Vec<f64>,
}

impl Acc {
    fn new(w: usize) -> Acc {
        Acc {
            total: 0.0,
            win: vec![0.0; w],
        }
    }
}

struct Interner {
    map: HashMap<String, u32>,
    names: Vec<String>,
}

impl Interner {
    fn new() -> Interner {
        Interner {
            map: HashMap::default(),
            names: Vec::new(),
        }
    }
    fn id(&mut self, s: &str) -> u32 {
        if let Some(i) = self.map.get(s) {
            return *i;
        }
        let i = self.names.len() as u32;
        self.names.push(s.to_string());
        self.map.insert(s.to_string(), i);
        i
    }
}

type EntryKey = ((u32, u32, u32), f64);

#[derive(Clone, Copy)]
struct FrameInfo {
    cat: Option<Cat>,
    entity: Option<u32>,
    block_entity: Option<u32>,
    bucket: &'static str,
}

#[derive(Clone, Copy)]
struct State {
    node: u32,
    cat: Option<Cat>,
    owner: Option<u32>,
    ctx: Option<Cat>,
    event_caller: Option<u32>,
    owner_frame: Option<u32>,
    unknown_frame: Option<u32>,
    entered: bool,
}

pub fn idle_mask(t: &Thread) -> Vec<bool> {
    let server = t.is_server_thread();
    let mut mask = vec![false; t.nodes.len()];
    let mut stack: Vec<(u32, bool, bool, bool)> =
        t.roots.iter().map(|r| (*r, false, true, false)).collect();
    while let Some((id, parent_idle, loop_parent, tick_wait)) = stack.pop() {
        let n = t.node(id);
        let jdk = n.class.starts_with("java.")
            || n.class.starts_with("jdk.")
            || n.class.starts_with("sun.")
            || n.class.is_empty();
        let (idle, lp, tw) = if parent_idle {
            (true, loop_parent, tick_wait)
        } else if server {
            let marker = patterns::is_server_idle_marker(n);
            let helper = patterns::is_wait_helper(n);
            let tw = if marker {
                true
            } else if jdk || helper {
                tick_wait
            } else {
                false
            };
            let lp = if jdk {
                loop_parent
            } else {
                patterns::is_server_loop(n)
            };
            let idle = patterns::is_blocking_frame(&n.class, &n.method) && (tw || lp);
            (idle, lp, tw)
        } else {
            (patterns::is_generic_idle(n), false, false)
        };
        mask[id as usize] = idle;
        for c in &n.children {
            stack.push((*c, idle, lp, tw));
        }
    }
    mask
}

pub fn analyze(p: &Profile) -> Analysis {
    let main_idx = p.main_thread_index();
    let thread = &p.threads[main_idx];
    let w = thread.windows;
    let idle_mask = idle_mask(thread);
    let mut idle_sub = vec![0.0f64; thread.nodes.len()];
    for (i, n) in thread.nodes.iter().enumerate() {
        if idle_mask[i] && n.parent.is_none_or(|q| !idle_mask[q as usize]) {
            let mut cur = n.parent;
            while let Some(q) = cur {
                idle_sub[q as usize] += n.total;
                cur = thread.node(q).parent;
            }
        }
    }

    let mut strings = Interner::new();
    let mut methods = Interner::new();

    let mut cat_acc: HashMap<Cat, Acc> = HashMap::default();
    let mut owner_self: HashMap<u32, Acc> = HashMap::default();
    let mut owner_incl: HashMap<u32, f64> = HashMap::default();
    let mut bucket_self: HashMap<&'static str, f64> = HashMap::default();
    let mut method_self: HashMap<u32, Acc> = HashMap::default();
    let mut method_incl: HashMap<u32, f64> = HashMap::default();
    let mut method_source: HashMap<u32, Option<u32>> = HashMap::default();
    let mut cat_owner: HashMap<(Cat, u32), f64> = HashMap::default();
    let mut cat_method: HashMap<(Cat, u32), f64> = HashMap::default();
    let mut cat_site: HashMap<(Cat, u32, u32), f64> = HashMap::default();
    let mut cat_unknown: HashMap<(Cat, u32), f64> = HashMap::default();
    let unknown_bucket = crate::i18n::t("Sconosciuto", "Unknown");
    let mut owner_method: HashMap<(u32, u32), f64> = HashMap::default();
    let mut owner_hazard: HashMap<(u32, Cat), f64> = HashMap::default();
    let mut entity_self: HashMap<u32, f64> = HashMap::default();
    let mut be_self: HashMap<u32, f64> = HashMap::default();
    let mut entries: HashMap<(u32, u32, u32), f64> = HashMap::default();
    let mut events: HashMap<(u32, u32), f64> = HashMap::default();
    let mut cat_entry_best: HashMap<Cat, (u32, f64)> = HashMap::default();
    let mut hot: Vec<(f64, u32, Option<Cat>, Option<u32>)> = Vec::new();
    let mut busy_win = vec![0.0f64; w];
    let mut idle_total = 0.0;
    let mut blocking_total = 0.0;

    let mut owner_depth: HashMap<u32, u32> = HashMap::default();
    let mut method_depth: HashMap<u32, u32> = HashMap::default();
    let mut entity_stack: Vec<Option<u32>> = Vec::new();
    let mut be_stack: Vec<Option<u32>> = Vec::new();
    let mut flame = crate::flame::FlameTree::new("main thread");
    let mut flame_stack: Vec<u32> = Vec::new();
    let mut flame_names: HashMap<(&str, &str, Option<u32>), u32> = HashMap::default();
    let fold_min = thread.total * 0.00002;

    let mut method_ids: HashMap<(&str, &str), u32> =
        HashMap::with_capacity_and_hasher(thread.nodes.len() / 4, Default::default());
    let node_method_id: Vec<u32> = thread
        .nodes
        .iter()
        .map(|n| {
            *method_ids
                .entry((n.class.as_str(), n.method.as_str()))
                .or_insert_with(|| methods.id(&n.label()))
        })
        .collect();
    let mut source_cache: HashMap<(&str, &str, &str, i32), Option<u32>> =
        HashMap::with_capacity_and_hasher(thread.nodes.len() / 4, Default::default());
    let node_source: Vec<Option<u32>> = thread
        .nodes
        .iter()
        .map(|n| {
            *source_cache
                .entry((n.class.as_str(), n.method.as_str(), n.desc.as_str(), n.line))
                .or_insert_with(|| {
                    p.sources
                        .resolve(n)
                        .filter(|s| !p.sources.is_builtin(s))
                        .map(|s| strings.id(s))
                })
        })
        .collect();
    let mut frame_cache: HashMap<(&str, &str, bool), FrameInfo> =
        HashMap::with_capacity_and_hasher(thread.nodes.len() / 4, Default::default());
    let node_frame: Vec<FrameInfo> = thread
        .nodes
        .iter()
        .map(|n| {
            *frame_cache
                .entry((n.class.as_str(), n.method.as_str(), n.parent.is_some()))
                .or_insert_with(|| FrameInfo {
                    cat: patterns::categorize(n),
                    entity: patterns::entity_type(n).map(|s| strings.id(&s)),
                    block_entity: patterns::block_entity_type(n).map(|s| strings.id(&s)),
                    bucket: patterns::package_bucket(&n.class),
                })
        })
        .collect();
    let mut self_win = vec![0.0f64; w];
    for (i, m) in node_method_id.iter().enumerate() {
        method_source.entry(*m).or_insert(node_source[i]);
    }

    let mut stack: Vec<State> = Vec::new();
    for r in thread.roots.iter().rev() {
        stack.push(State {
            node: *r,
            cat: None,
            owner: None,
            ctx: None,
            event_caller: None,
            owner_frame: None,
            unknown_frame: None,
            entered: false,
        });
    }

    while let Some(top) = stack.last_mut() {
        if top.entered {
            let st = *top;
            stack.pop();
            let mid = node_method_id[st.node as usize];
            if let Some(d) = method_depth.get_mut(&mid) {
                *d -= 1;
            }
            if let Some(src) = node_source[st.node as usize]
                && let Some(d) = owner_depth.get_mut(&src)
            {
                *d -= 1;
            }
            entity_stack.pop();
            be_stack.pop();
            flame_stack.pop();
            continue;
        }
        top.entered = true;
        let st = *top;
        let id = st.node;
        let node = thread.node(id);
        let src = node_source[id as usize];
        let mid = node_method_id[id as usize];

        let idle = idle_mask[id as usize];
        let active = node.total - idle_sub[id as usize];

        let info = node_frame[id as usize];
        let mut own_cat = info.cat;
        if let (Some(oc), Some(pc)) = (own_cat, st.cat) {
            let sticky_parent = matches!(
                pc,
                Cat::ChunkLoadSync
                    | Cat::ChunkGen
                    | Cat::ChunkSave
                    | Cat::SyncDatabase
                    | Cat::SyncNetworkIo
                    | Cat::SyncFileIo
                    | Cat::Logging
            );
            let weak_child = matches!(
                oc,
                Cat::Blocking
                    | Cat::SyncFileIo
                    | Cat::SyncNetworkIo
                    | Cat::Serialization
                    | Cat::Exceptions
            );
            if sticky_parent && weak_child {
                own_cat = None;
            }
        }
        let cat = own_cat.or(st.cat);
        let owner = src.or(st.owner);
        let owner_frame = if src.is_some() {
            Some(mid)
        } else {
            st.owner_frame
        };
        let unknown_frame = st
            .unknown_frame
            .or_else(|| (src.is_none() && info.bucket == unknown_bucket).then_some(mid));
        let ctx = match own_cat {
            Some(c @ (Cat::PluginEvents | Cat::PluginTasks | Cat::Commands | Cat::Network))
                if src.is_none() =>
            {
                Some(c)
            }
            _ => st.ctx,
        };
        let mut event_caller = st.event_caller;
        if own_cat == Some(Cat::PluginEvents)
            && st.cat != Some(Cat::PluginEvents)
            && let Some(par) = node.parent
        {
            event_caller = Some(strings.id(&thread.node(par).short_label()));
        }

        let ent = info
            .entity
            .or_else(|| entity_stack.last().copied().flatten());
        let be = info
            .block_entity
            .or_else(|| be_stack.last().copied().flatten());
        entity_stack.push(ent);
        be_stack.push(be);
        let flame_name = match flame_names.get(&(node.class.as_str(), node.method.as_str(), src)) {
            Some(n) => *n,
            None => {
                let label = match src {
                    Some(s) => format!("{} [{}]", node.short_label(), strings.names[s as usize]),
                    None => node.short_label(),
                };
                let n = flame.intern(&label);
                flame_names.insert((node.class.as_str(), node.method.as_str(), src), n);
                n
            }
        };
        let flame_node = flame.child(flame_stack.last().copied().unwrap_or(0), flame_name);
        flame_stack.push(flame_node);

        let first_method = *method_depth.get(&mid).unwrap_or(&0) == 0;
        *method_depth.entry(mid).or_insert(0) += 1;
        if let Some(s) = src {
            let first_owner = *owner_depth.get(&s).unwrap_or(&0) == 0;
            *owner_depth.entry(s).or_insert(0) += 1;
            if !idle && first_owner {
                *owner_incl.entry(s).or_insert(0.0) += active;
            }
            if !idle && st.owner != Some(s) {
                let ctx_label = match ctx {
                    Some(Cat::PluginEvents) => {
                        let ev = event_name(&node.desc)
                            .unwrap_or_else(|| crate::i18n::t("evento", "event").to_string());
                        let caller = event_caller
                            .map(|c| strings.names[c as usize].clone())
                            .unwrap_or_default();
                        let ev_id = strings.id(&ev);
                        *events.entry((s, ev_id)).or_insert(0.0) += node.total;
                        if caller.is_empty() {
                            tf!("Evento {ev}", "Event {ev}")
                        } else {
                            tf!("Evento {ev} (da {caller})", "Event {ev} (from {caller})")
                        }
                    }
                    Some(Cat::PluginTasks) => {
                        crate::i18n::t("Task schedulato", "Scheduled task").to_string()
                    }
                    Some(Cat::Commands) => crate::i18n::t("Comando", "Command").to_string(),
                    Some(Cat::Network) => {
                        crate::i18n::t("Gestione pacchetti", "Packet handling").to_string()
                    }
                    _ => match st.owner {
                        Some(o) => tf!("Chiamato da {}", "Called by {}", strings.names[o as usize]),
                        None => crate::i18n::t("Altro", "Other").to_string(),
                    },
                };
                let ctx_id = strings.id(&ctx_label);
                *entries.entry((s, mid, ctx_id)).or_insert(0.0) += node.total;
            }
        }
        if !idle && first_method {
            *method_incl.entry(mid).or_insert(0.0) += active;
        }
        if !idle
            && let Some(c) = own_cat
            && st.cat != Some(c)
        {
            let e = cat_entry_best.entry(c).or_insert((id, 0.0));
            if node.total > e.1 {
                *e = (id, node.total);
            }
        }

        let self_t = thread.self_time(id);
        if self_t > 0.0 {
            if idle {
                idle_total += self_t;
            } else {
                let times = thread.times_of(id);
                for (slot, t) in self_win.iter_mut().zip(times) {
                    *slot = *t as f64;
                }
                for c in &node.children {
                    for (slot, t) in self_win.iter_mut().zip(thread.times_of(*c)) {
                        *slot -= *t as f64;
                    }
                }
                for slot in self_win.iter_mut() {
                    *slot = slot.max(0.0);
                }
                for wi in 0..w {
                    busy_win[wi] += self_win[wi];
                }
                let c = cat.unwrap_or(Cat::Other);
                let a = cat_acc.entry(c).or_insert_with(|| Acc::new(w));
                a.total += self_t;
                for wi in 0..w {
                    a.win[wi] += self_win[wi];
                }
                if c == Cat::Blocking {
                    blocking_total += self_t;
                }
                let ma = method_self.entry(mid).or_insert_with(|| Acc::new(w));
                ma.total += self_t;
                for wi in 0..w {
                    ma.win[wi] += self_win[wi];
                }
                *cat_method.entry((c, mid)).or_insert(0.0) += self_t;
                match owner {
                    Some(o) => {
                        let oa = owner_self.entry(o).or_insert_with(|| Acc::new(w));
                        oa.total += self_t;
                        for wi in 0..w {
                            oa.win[wi] += self_win[wi];
                        }
                        *cat_owner.entry((c, o)).or_insert(0.0) += self_t;
                        if let Some(fr) = owner_frame {
                            *cat_site.entry((c, o, fr)).or_insert(0.0) += self_t;
                        }
                        *owner_method.entry((o, mid)).or_insert(0.0) += self_t;
                        if c.is_sync_hazard()
                            || matches!(c, Cat::Logging | Cat::Exceptions | Cat::Serialization)
                        {
                            *owner_hazard.entry((o, c)).or_insert(0.0) += self_t;
                        }
                    }
                    None => {
                        if let Some(u) = unknown_frame {
                            *cat_unknown.entry((c, u)).or_insert(0.0) += self_t;
                        }
                        *bucket_self.entry(info.bucket).or_insert(0.0) += self_t;
                    }
                }
                if let Some(e) = ent {
                    *entity_self.entry(e).or_insert(0.0) += self_t;
                }
                if let Some(b) = be {
                    *be_self.entry(b).or_insert(0.0) += self_t;
                }
                hot.push((self_t, id, cat, owner));
                if self_t >= fold_min {
                    flame.add(flame_node, (self_t * 1000.0).round() as u64);
                }
            }
        }

        for c in node.children.iter().rev() {
            stack.push(State {
                node: *c,
                cat,
                owner,
                ctx,
                event_caller,
                owner_frame,
                unknown_frame,
                entered: false,
            });
        }
    }

    let total = thread.total;
    let busy = (total - idle_total).max(0.0);
    let pct = |v: f64| if busy > 0.0 { v / busy * 100.0 } else { 0.0 };
    let ticks = p.effective_ticks();

    let name_of = |i: u32| strings.names[i as usize].clone();
    let mname = |i: u32| methods.names[i as usize].clone();
    let snames: &[String] = &strings.names;
    let mnames: &[String] = &methods.names;
    let sname = |i: &u32| snames[*i as usize].as_str();
    let mstr = |i: &u32| mnames[*i as usize].as_str();
    let stats = |v: Vec<(u32, f64)>, k: usize, names: &[String]| -> Vec<Stat> {
        top_k(v, k, |a, b| by_time(a, b, names))
            .into_iter()
            .map(|(i, t)| Stat {
                name: names[i as usize].clone(),
                time: t,
                pct: pct(t),
            })
            .collect()
    };

    let mut cat_owner_g: HashMap<Cat, Vec<(u32, f64)>> = HashMap::default();
    for ((c, o), t) in &cat_owner {
        cat_owner_g.entry(*c).or_default().push((*o, *t));
    }
    let mut cat_method_g: HashMap<Cat, Vec<(u32, f64)>> = HashMap::default();
    for ((c, m), t) in &cat_method {
        cat_method_g.entry(*c).or_default().push((*m, *t));
    }
    let mut cat_unknown_g: HashMap<Cat, Vec<(u32, f64)>> = HashMap::default();
    for ((c, m), t) in &cat_unknown {
        cat_unknown_g.entry(*c).or_default().push((*m, *t));
    }
    let mut cat_site_g: HashMap<Cat, Vec<(u32, u32, f64)>> = HashMap::default();
    for ((c, o, m), t) in &cat_site {
        cat_site_g.entry(*c).or_default().push((*o, *m, *t));
    }

    let mut categories: Vec<CatStat> = cat_acc
        .iter()
        .map(|(c, a)| {
            let owners = stats(cat_owner_g.remove(c).unwrap_or_default(), 5, snames);
            let ms = stats(cat_method_g.remove(c).unwrap_or_default(), 6, mnames);
            let example_stack = cat_entry_best
                .get(c)
                .map(|(nid, _)| example_stack(thread, *nid))
                .unwrap_or_default();
            let culprit_sites: Vec<CulpritSite> =
                top_k(cat_site_g.remove(c).unwrap_or_default(), 6, |a, b| {
                    b.2.total_cmp(&a.2)
                        .then_with(|| mstr(&a.1).cmp(mstr(&b.1)))
                        .then_with(|| sname(&a.0).cmp(sname(&b.0)))
                })
                .into_iter()
                .map(|(o, m, t)| CulpritSite {
                    source: name_of(o),
                    method: mname(m),
                    time: t,
                    pct: pct(t),
                })
                .collect();
            let unknown_sites = stats(cat_unknown_g.remove(c).unwrap_or_default(), 4, mnames);
            CatStat {
                unknown_sites,
                culprit_sites,
                cat: *c,
                label: c.label().to_string(),
                time: a.total,
                pct: pct(a.total),
                top_owners: owners,
                top_methods: ms,
                example_stack,
            }
        })
        .collect();
    categories.sort_by(|a, b| {
        b.time
            .total_cmp(&a.time)
            .then_with(|| a.label.cmp(&b.label))
    });

    let entry_cmp = |a: &((u32, u32, u32), f64), b: &((u32, u32, u32), f64)| {
        b.1.total_cmp(&a.1)
            .then_with(|| sname(&a.0.0).cmp(sname(&b.0.0)))
            .then_with(|| mstr(&a.0.1).cmp(mstr(&b.0.1)))
            .then_with(|| sname(&a.0.2).cmp(sname(&b.0.2)))
    };
    let entry_stat = |((s, m, c), t): ((u32, u32, u32), f64)| EntryStat {
        source: name_of(s),
        entry: mname(m),
        context: name_of(c),
        time: t,
        pct: pct(t),
    };
    let mut owner_entries: HashMap<u32, Vec<EntryKey>> = HashMap::default();
    for (k, t) in &entries {
        owner_entries.entry(k.0).or_default().push((*k, *t));
    }
    let mut owner_methods: HashMap<u32, Vec<(u32, f64)>> = HashMap::default();
    for ((o, m), t) in &owner_method {
        owner_methods.entry(*o).or_default().push((*m, *t));
    }
    let mut owner_hazards: HashMap<u32, Vec<(Cat, f64)>> = HashMap::default();
    for ((o, c), t) in &owner_hazard {
        owner_hazards.entry(*o).or_default().push((*c, *t));
    }

    let mut sources: Vec<SourceStat> = owner_self
        .keys()
        .chain(owner_incl.keys())
        .copied()
        .collect::<std::collections::BTreeSet<u32>>()
        .into_iter()
        .map(|s| {
            let st = owner_self.get(&s).map(|a| a.total).unwrap_or(0.0);
            let it = owner_incl.get(&s).copied().unwrap_or(0.0);
            let ents: Vec<EntryStat> =
                top_k(owner_entries.remove(&s).unwrap_or_default(), 8, entry_cmp)
                    .into_iter()
                    .map(entry_stat)
                    .collect();
            let ms = stats(owner_methods.remove(&s).unwrap_or_default(), 8, mnames);
            let hz: Vec<Stat> = top_k(
                owner_hazards.remove(&s).unwrap_or_default(),
                usize::MAX,
                |a, b| {
                    b.1.total_cmp(&a.1)
                        .then_with(|| a.0.label().cmp(b.0.label()))
                },
            )
            .into_iter()
            .map(|(c, t)| Stat {
                name: c.label().to_string(),
                time: t,
                pct: pct(t),
            })
            .collect();
            let n = name_of(s);
            SourceStat {
                display: p.sources.display_name(&n),
                source: n,
                self_time: st,
                self_pct: pct(st),
                incl_time: it,
                incl_pct: pct(it),
                main_thread: true,
                top_entries: ents,
                top_methods: ms,
                hazards: hz,
            }
        })
        .collect();
    sources.sort_by(|a, b| {
        b.incl_time
            .total_cmp(&a.incl_time)
            .then_with(|| a.source.cmp(&b.source))
    });

    let mut buckets: Vec<Stat> = bucket_self
        .iter()
        .map(|(k, v)| Stat {
            name: k.to_string(),
            time: *v,
            pct: pct(*v),
        })
        .collect();
    sort_stats(&mut buckets);

    let method_stat = |m: u32, self_time: f64, incl_time: f64| MethodStat {
        method: mname(m),
        source: method_source.get(&m).copied().flatten().map(name_of),
        self_time,
        self_pct: pct(self_time),
        incl_time,
        incl_pct: pct(incl_time),
    };
    let method_list: Vec<MethodStat> = top_k(
        method_self.iter().map(|(m, a)| (*m, a.total)).collect(),
        80,
        |a, b| by_time(a, b, mnames),
    )
    .into_iter()
    .map(|(m, st)| method_stat(m, st, method_incl.get(&m).copied().unwrap_or(0.0)))
    .collect();
    let incl_list: Vec<MethodStat> = top_k(
        method_incl
            .iter()
            .filter(|(_, it)| pct(**it) < 99.5)
            .map(|(m, it)| (*m, *it))
            .collect(),
        60,
        |a, b| by_time(a, b, mnames),
    )
    .into_iter()
    .map(|(m, it)| method_stat(m, method_self.get(&m).map(|a| a.total).unwrap_or(0.0), it))
    .collect();

    let entity_types = stats(
        entity_self.iter().map(|(k, v)| (*k, *v)).collect(),
        usize::MAX,
        snames,
    );
    let block_entity_types = stats(
        be_self.iter().map(|(k, v)| (*k, *v)).collect(),
        usize::MAX,
        snames,
    );

    let event_list: Vec<EntryStat> = top_k(
        events.iter().map(|(k, t)| (*k, *t)).collect(),
        usize::MAX,
        |a: &((u32, u32), f64), b: &((u32, u32), f64)| {
            b.1.total_cmp(&a.1)
                .then_with(|| sname(&a.0.0).cmp(sname(&b.0.0)))
                .then_with(|| sname(&a.0.1).cmp(sname(&b.0.1)))
        },
    )
    .into_iter()
    .map(|((s, e), t)| EntryStat {
        source: name_of(s),
        entry: name_of(e),
        context: crate::i18n::t("Evento", "Event").into(),
        time: t,
        pct: pct(t),
    })
    .collect();
    let entry_list: Vec<EntryStat> = top_k(
        entries.iter().map(|(k, t)| (*k, *t)).collect(),
        40,
        entry_cmp,
    )
    .into_iter()
    .map(entry_stat)
    .collect();

    let hot = top_k(hot, 20, |a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let hot_paths: Vec<HotPath> = hot
        .iter()
        .map(|(t, id, c, o)| HotPath {
            time: *t,
            pct: pct(*t),
            cat: c.map(|c| c.label().to_string()),
            owner: o.map(name_of),
            frames: compress_path(thread, *id),
        })
        .collect();

    let main = MainStats {
        thread: thread.name.clone(),
        total,
        busy,
        idle: idle_total,
        busy_pct: if total > 0.0 {
            busy / total * 100.0
        } else {
            0.0
        },
        ticks,
        busy_per_tick: if ticks > 0 && p.mode == Mode::Execution {
            Some(busy / ticks as f64)
        } else {
            None
        },
        blocking: blocking_total,
    };

    let threads = summarize_threads(p, main_idx, &main);
    let (windows, spikes, periodic) = analyze_windows(
        p,
        thread,
        &busy_win,
        &cat_acc,
        &owner_self,
        &method_self,
        &strings,
        &methods,
    );

    Analysis {
        mode: p.mode,
        unit: p.mode.unit().to_string(),
        main,
        categories,
        sources,
        buckets,
        methods: method_list.into_iter().take(80).collect(),
        methods_inclusive: incl_list,
        entity_types,
        block_entity_types,
        events: event_list,
        entries: entry_list,
        hot_paths,
        threads,
        windows,
        spikes,
        periodic_spikes: periodic,
        flame,
    }
}

fn sort_stats(v: &mut [Stat]) {
    v.sort_by(|a, b| b.time.total_cmp(&a.time).then_with(|| a.name.cmp(&b.name)));
}

fn by_time(a: &(u32, f64), b: &(u32, f64), names: &[String]) -> Ordering {
    b.1.total_cmp(&a.1)
        .then_with(|| names[a.0 as usize].cmp(&names[b.0 as usize]))
}

fn top_k<T>(mut v: Vec<T>, k: usize, cmp: impl Fn(&T, &T) -> Ordering) -> Vec<T> {
    if k == 0 {
        return Vec::new();
    }
    if v.len() > k {
        v.select_nth_unstable_by(k - 1, &cmp);
        v.truncate(k);
    }
    v.sort_unstable_by(cmp);
    v
}

pub fn event_name(desc: &str) -> Option<String> {
    let inner = desc.strip_prefix('(')?;
    let end = inner.find(')')?;
    let params = &inner[..end];
    let first = params.split(';').next()?;
    let first = first.trim_start_matches('L');
    if first.is_empty() {
        return None;
    }
    let simple = first.rsplit('/').next()?.to_string();
    if simple.ends_with("Event") || simple.contains("Event") {
        Some(simple)
    } else {
        None
    }
}

fn is_noise_frame(label: &str) -> bool {
    label.starts_with("java.lang.Thread.run")
        || label.starts_with("java.util.concurrent.")
        || label.starts_with("jdk.internal.reflect.")
        || label.starts_with("java.lang.invoke.")
        || label.contains("$$Lambda")
}

fn compress_path(thread: &Thread, id: u32) -> Vec<String> {
    let path = thread.path_to(id);
    let labels: Vec<String> = path.iter().map(|n| thread.node(*n).short_label()).collect();
    let full: Vec<String> = path.iter().map(|n| thread.node(*n).label()).collect();
    let mut out: Vec<String> = Vec::new();
    let keep_tail = 14usize;
    let start = labels.len().saturating_sub(keep_tail);
    if start > 0 {
        out.push(tf!("… ({} frame sopra)", "… ({} frames above)", start));
    }
    for i in start..labels.len() {
        if i + 3 < labels.len() && is_noise_frame(&full[i]) {
            continue;
        }
        out.push(labels[i].clone());
    }
    out
}

fn example_stack(thread: &Thread, id: u32) -> Vec<String> {
    let mut frames: Vec<String> = Vec::new();
    let path = thread.path_to(id);
    let ctx_start = path.len().saturating_sub(4);
    for n in &path[ctx_start..] {
        frames.push(thread.node(*n).short_label());
    }
    let mut cur = id;
    let mut depth = 0;
    loop {
        let node = thread.node(cur);
        let best = node
            .children
            .iter()
            .copied()
            .max_by(|a, b| thread.node(*a).total.total_cmp(&thread.node(*b).total));
        match best {
            Some(b) if thread.node(b).total >= node.total * 0.35 && depth < 10 => {
                frames.push(format!("  ↳ {}", thread.node(b).short_label()));
                cur = b;
                depth += 1;
            }
            _ => break,
        }
    }
    frames
}

fn summarize_threads(p: &Profile, main_idx: usize, main: &MainStats) -> Vec<ThreadSummary> {
    let grand: f64 = p.threads.iter().map(|t| t.total).sum();
    let mut out = Vec::new();
    for (i, t) in p.threads.iter().enumerate() {
        if i == main_idx {
            out.push(ThreadSummary {
                name: t.name.clone(),
                total: t.total,
                busy: main.busy,
                busy_pct_of_all: if grand > 0.0 {
                    t.total / grand * 100.0
                } else {
                    0.0
                },
                top_methods: Vec::new(),
                top_owner: None,
                plugin_hint: None,
            });
            continue;
        }
        let (busy, mut tops, owner) = thread_busy(p, t);
        tops.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let top_methods = tops
            .iter()
            .take(3)
            .map(|(n, v)| Stat {
                name: n.clone(),
                time: *v,
                pct: if busy > 0.0 { v / busy * 100.0 } else { 0.0 },
            })
            .collect();
        out.push(ThreadSummary {
            name: t.name.clone(),
            total: t.total,
            busy,
            busy_pct_of_all: if grand > 0.0 {
                t.total / grand * 100.0
            } else {
                0.0
            },
            top_methods,
            top_owner: owner,
            plugin_hint: thread_plugin_hint(&t.name),
        });
    }
    out.sort_by(|a, b| b.busy.total_cmp(&a.busy).then_with(|| a.name.cmp(&b.name)));
    out
}

fn thread_plugin_hint(name: &str) -> Option<String> {
    let n = name.to_ascii_lowercase();
    if n.contains("craft scheduler") {
        return Some(
            crate::i18n::t(
                "task asincroni di plugin (Bukkit scheduler)",
                "plugin async tasks (Bukkit scheduler)",
            )
            .into(),
        );
    }
    if n.contains("netty") || n.contains("epoll") || n.contains("io thread") {
        return Some(crate::i18n::t("rete (Netty)", "network (Netty)").into());
    }
    if n.contains("worker-main")
        || n.contains("worldgen")
        || n.contains("chunk")
        || n.contains("c2me")
        || n.contains("moonrise")
    {
        return Some(
            crate::i18n::t("chunk / generazione mondo", "chunks / world generation").into(),
        );
    }
    if n.contains("gc") || n.contains("g1") {
        return Some("garbage collector".into());
    }
    if n.contains("hikari")
        || n.contains("mysql")
        || n.contains("redis")
        || n.contains("lettuce")
        || n.contains("jedis")
    {
        return Some(crate::i18n::t("database / cache", "database / cache").into());
    }
    None
}

fn thread_busy(p: &Profile, t: &Thread) -> (f64, Vec<(String, f64)>, Option<String>) {
    let mut busy = 0.0;
    let mut tops: HashMap<(&str, &str), f64> = HashMap::default();
    let mut owners: HashMap<&str, f64> = HashMap::default();
    let mut resolved: HashMap<(&str, &str, &str, i32), Option<&str>> = HashMap::default();
    let mask = idle_mask(t);
    let mut stack: Vec<(u32, Option<&str>)> = t.roots.iter().map(|r| (*r, None)).collect();
    while let Some((id, owner_parent)) = stack.pop() {
        let n = t.node(id);
        let idle = mask[id as usize];
        let own = *resolved
            .entry((n.class.as_str(), n.method.as_str(), n.desc.as_str(), n.line))
            .or_insert_with(|| p.sources.resolve(n).filter(|s| !p.sources.is_builtin(s)));
        let owner = own.or(owner_parent);
        if !idle {
            let s = t.self_time(id);
            if s > 0.0 {
                busy += s;
                *tops
                    .entry((n.simple_class(), n.method.as_str()))
                    .or_insert(0.0) += s;
                if let Some(o) = owner {
                    *owners.entry(o).or_insert(0.0) += s;
                }
            }
        }
        for c in &n.children {
            stack.push((*c, owner));
        }
    }
    let owner = owners
        .into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .filter(|(_, v)| *v > busy * 0.2)
        .map(|(k, _)| k.to_string());
    let mut merged: HashMap<String, f64> = HashMap::default();
    for ((c, m), v) in tops {
        *merged.entry(format!("{c}.{m}")).or_insert(0.0) += v;
    }
    (busy, merged.into_iter().collect(), owner)
}

#[allow(clippy::too_many_arguments)]
fn analyze_windows(
    p: &Profile,
    thread: &Thread,
    busy_win: &[f64],
    cat_acc: &HashMap<Cat, Acc>,
    owner_self: &HashMap<u32, Acc>,
    method_self: &HashMap<u32, Acc>,
    strings: &Interner,
    methods: &Interner,
) -> (Vec<WindowStat>, Vec<Spike>, Option<String>) {
    let w = thread.windows;
    let mut windows: Vec<WindowStat> = Vec::with_capacity(w);
    for i in 0..w {
        let id = p.window_ids.get(i).copied().unwrap_or(i as i32);
        let s = p.window_stats.get(&id).cloned().unwrap_or_default();
        let ticks = s.ticks;
        let busy = busy_win.get(i).copied().unwrap_or(0.0);
        windows.push(WindowStat {
            index: p.window_map.get(i).copied().unwrap_or(i),
            id,
            start_time: s.start_time,
            end_time: s.end_time,
            ticks,
            tps: s.tps,
            mspt_median: s.mspt_median,
            mspt_max: s.mspt_max,
            cpu_process: s.cpu_process,
            cpu_system: s.cpu_system,
            players: s.players,
            entities: s.entities,
            tile_entities: s.tile_entities,
            chunks: s.chunks,
            busy,
            busy_per_tick: if ticks > 0 { busy / ticks as f64 } else { 0.0 },
            is_spike: false,
        });
    }
    if w < 2 || p.mode != Mode::Execution {
        return (windows, Vec::new(), None);
    }
    let has_ticks = windows.iter().all(|x| x.ticks > 0);
    let tick_counts: Vec<i32> = windows.iter().map(|x| x.ticks).collect();
    let norm = |wi: usize, v: f64| -> f64 {
        if has_ticks {
            v / tick_counts[wi] as f64
        } else {
            v
        }
    };
    let per: Vec<f64> = (0..w).map(|i| norm(i, windows[i].busy)).collect();
    let med = median(&per);
    let has_stats = windows.iter().any(|x| x.tps > 0.0 || x.mspt_max > 0.0);
    let mut reasons: Vec<Option<String>> = vec![None; w];
    for i in 0..w {
        let x = &windows[i];
        let mut r = Vec::new();
        if has_stats && x.mspt_max >= 50.0 {
            r.push(tf!(
                "tick più lento {:.0} ms",
                "slowest tick {:.0} ms",
                x.mspt_max
            ));
        }
        if has_stats && x.tps > 0.0 && x.tps < 19.0 {
            r.push(tf!("TPS {:.1}", "TPS {:.1}", x.tps));
        }
        if med > 0.0 && per[i] > med * 1.6 && (!has_ticks || per[i] > 20.0) {
            r.push(tf!(
                "carico main thread {:.1}× la mediana",
                "main thread load {:.1}× the median",
                per[i] / med
            ));
        }
        if !r.is_empty() {
            reasons[i] = Some(r.join(", "));
        }
    }
    let spike_count = reasons.iter().filter(|r| r.is_some()).count();
    if spike_count == w {
        let worst = med * 1.25;
        for i in 0..w {
            if per[i] <= worst && windows[i].mspt_max < 100.0 {
                reasons[i] = None;
            }
        }
    }
    let base_idx: Vec<usize> = (0..w).filter(|i| reasons[*i].is_none()).collect();
    let base_idx = if base_idx.is_empty() {
        (0..w).collect()
    } else {
        base_idx
    };

    let deltas = |accs: Vec<(&str, &Acc)>, wi: usize| -> Vec<Delta> {
        let mut base_vals: Vec<f64> = Vec::with_capacity(base_idx.len());
        let out: Vec<(&str, f64, f64)> = accs
            .into_iter()
            .filter_map(|(name, a)| {
                let value = norm(wi, a.win[wi]);
                if value <= 0.0 {
                    return None;
                }
                base_vals.clear();
                base_vals.extend(base_idx.iter().map(|b| norm(*b, a.win[*b])));
                let baseline = median(&base_vals);
                (value - baseline > 0.0).then_some((name, baseline, value))
            })
            .collect();
        top_k(out, 6, |a, b| {
            (b.2 - b.1)
                .total_cmp(&(a.2 - a.1))
                .then_with(|| a.0.cmp(b.0))
        })
        .into_iter()
        .map(|(name, baseline, value)| Delta {
            name: name.to_string(),
            baseline,
            value,
            delta: value - baseline,
        })
        .collect()
    };

    let mut spikes = Vec::new();
    for i in 0..w {
        if let Some(r) = &reasons[i] {
            windows[i].is_spike = true;
            let cat_deltas = deltas(cat_acc.iter().map(|(c, a)| (c.label(), a)).collect(), i);
            let owner_deltas = deltas(
                owner_self
                    .iter()
                    .map(|(o, a)| (strings.names[*o as usize].as_str(), a))
                    .collect(),
                i,
            );
            let method_deltas = deltas(
                method_self
                    .iter()
                    .filter(|(_, a)| a.total > 0.0)
                    .map(|(m, a)| (methods.names[*m as usize].as_str(), a))
                    .collect(),
                i,
            );
            spikes.push(Spike {
                window: windows[i].clone(),
                reason: r.clone(),
                unit: if has_ticks {
                    "ms/tick".into()
                } else {
                    "ms/min".into()
                },
                cat_deltas,
                owner_deltas,
                method_deltas,
            });
        }
    }
    spikes.sort_by(|a, b| {
        b.window
            .busy_per_tick
            .total_cmp(&a.window.busy_per_tick)
            .then(b.window.busy.total_cmp(&a.window.busy))
    });
    let periodic = detect_periodic(&windows);
    (windows, spikes, periodic)
}

fn detect_periodic(windows: &[WindowStat]) -> Option<String> {
    let ids: Vec<i32> = windows
        .iter()
        .filter(|w| w.is_spike)
        .map(|w| w.id)
        .collect();
    if ids.len() < 3 {
        return None;
    }
    let gaps: Vec<i32> = ids.windows(2).map(|p| p[1] - p[0]).collect();
    let mut counts: HashMap<i32, usize> = HashMap::default();
    for g in &gaps {
        *counts.entry(*g).or_insert(0) += 1;
    }
    let (gap, n) = counts.into_iter().max_by_key(|(_, n)| *n)?;
    if gap >= 2 && n * 3 >= gaps.len() * 2 {
        Some(tf!(
            "I picchi si ripetono circa ogni {gap} minuti: tipico di autosave, backup o task schedulati a intervalli fissi.",
            "Spikes repeat about every {gap} minutes: typical of autosave, backups or tasks scheduled at fixed intervals."
        ))
    } else {
        None
    }
}

pub fn median(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let m = s.len() / 2;
    if s.len().is_multiple_of(2) {
        (s[m - 1] + s[m]) / 2.0
    } else {
        s[m]
    }
}

pub fn render_tree(
    p: &Profile,
    a: &Analysis,
    min_pct: f64,
    max_depth: usize,
    filter: Option<&str>,
) -> String {
    use std::fmt::Write;
    let t = &p.threads[p.main_thread_index()];
    let busy = a.main.busy.max(1e-9);
    let mask = idle_mask(t);
    let mut busy_of: Vec<f64> = vec![0.0; t.nodes.len()];
    {
        let mut order: Vec<u32> = Vec::with_capacity(t.nodes.len());
        let mut st: Vec<u32> = t.roots.clone();
        while let Some(id) = st.pop() {
            order.push(id);
            st.extend(t.node(id).children.iter().copied());
        }
        for id in order.iter().rev() {
            let i = *id as usize;
            if mask[i] {
                continue;
            }
            let kids: f64 = t
                .node(*id)
                .children
                .iter()
                .map(|c| busy_of[*c as usize])
                .sum();
            busy_of[i] = kids + t.self_time(*id);
        }
    }
    let q = filter.map(|f| f.to_ascii_lowercase());
    let mut matches: Vec<bool> = vec![q.is_none(); t.nodes.len()];
    let label = |id: u32| -> String {
        let n = t.node(id);
        match p.sources.resolve(n) {
            Some(s) => format!("{} [{}]", n.label(), s),
            None => n.label(),
        }
    };
    if let Some(q) = &q {
        let mut order: Vec<u32> = Vec::with_capacity(t.nodes.len());
        let mut st: Vec<u32> = t.roots.clone();
        while let Some(id) = st.pop() {
            order.push(id);
            st.extend(t.node(id).children.iter().copied());
        }
        for id in order.iter().rev() {
            let own = label(*id).to_ascii_lowercase().contains(q.as_str());
            let kid = t.node(*id).children.iter().any(|c| matches[*c as usize]);
            matches[*id as usize] = own || kid;
        }
    }
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}",
        tf!(
            "Albero di «{}» — % sul tempo attivo, soglia {}%",
            "Tree of “{}” — % of active time, threshold {}%",
            t.name,
            min_pct
        )
    );
    let ticks = a.main.ticks;
    let mut st: Vec<(u32, usize)> = t.roots.iter().rev().map(|r| (*r, 0)).collect();
    while let Some((id, d)) = st.pop() {
        let n = t.node(id);
        if mask[id as usize] {
            continue;
        }
        let pct = busy_of[id as usize] / busy * 100.0;
        if pct < min_pct || !matches[id as usize] || d > max_depth {
            continue;
        }
        let per = if ticks > 0 && p.mode == Mode::Execution {
            format!("{:>7.2} ms/t", busy_of[id as usize] / ticks as f64)
        } else {
            String::new()
        };
        let _ = writeln!(out, "{:>6.2}% {} {}{}", pct, per, "  ".repeat(d), label(id));
        let mut kids: Vec<u32> = n.children.clone();
        kids.sort_by(|x, y| busy_of[*x as usize].total_cmp(&busy_of[*y as usize]));
        for k in kids {
            st.push((k, d + 1));
        }
    }
    out
}
