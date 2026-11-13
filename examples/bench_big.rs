use std::collections::BTreeMap;
use std::time::Instant;

use prost::Message;
use sparkscope::proto::*;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

const MC: &[(&str, &str)] = &[
    ("net.minecraft.server.level.ServerLevel", "tick"),
    ("net.minecraft.world.entity.LivingEntity", "tick"),
    ("net.minecraft.world.entity.Mob", "serverAiStep"),
    ("net.minecraft.world.entity.ai.goal.GoalSelector", "tick"),
    (
        "net.minecraft.world.level.pathfinder.PathFinder",
        "findPath",
    ),
    (
        "net.minecraft.world.level.block.entity.HopperBlockEntity",
        "pushItemsTick",
    ),
    ("net.minecraft.server.level.ServerChunkCache", "tickChunks"),
    (
        "net.minecraft.world.level.lighting.LevelLightEngine",
        "runLightUpdates",
    ),
    (
        "net.minecraft.server.network.ServerGamePacketListenerImpl",
        "handleMovePlayer",
    ),
    ("org.bukkit.plugin.SimplePluginManager", "callEvent"),
    (
        "org.bukkit.craftbukkit.scheduler.CraftScheduler",
        "mainThreadHeartbeat",
    ),
    ("org.bukkit.craftbukkit.entity.CraftEntity", "getLocation"),
];
const JDK: &[(&str, &str)] = &[
    ("java.util.HashMap", "get"),
    ("java.util.ArrayList", "grow"),
    ("java.lang.StringBuilder", "append"),
    ("java.util.stream.ReferencePipeline", "collect"),
    ("java.lang.String", "format"),
    ("java.util.concurrent.locks.LockSupport", "parkNanos"),
];

struct Gen {
    rng: Rng,
    windows: usize,
    nodes: Vec<StackTraceNode>,
    plugins: usize,
}

impl Gen {
    fn frame(&mut self, depth: usize) -> (String, String) {
        let r = self.rng.below(100);
        if depth > 6 && r < 30 {
            let (c, m) = JDK[self.rng.below(JDK.len() as u64) as usize];
            (c.into(), m.into())
        } else if r < 55 {
            let (c, m) = MC[self.rng.below(MC.len() as u64) as usize];
            (c.into(), m.into())
        } else {
            let p = self.rng.below(self.plugins as u64);
            let k = self.rng.below(40);
            (
                format!("com.plugin{p}.module{}.Class{k}", k % 5),
                format!("method{}", self.rng.below(12)),
            )
        }
    }

    fn build(&mut self, depth: usize, budget: usize) -> (i32, Vec<f64>) {
        let (class, method) = self.frame(depth);
        let mut times = vec![0.0f64; self.windows];
        let mut refs = Vec::new();
        let mut left = budget.saturating_sub(1);
        if left == 0 || depth > 120 {
            for t in times.iter_mut() {
                let hot = if self.rng.below(40) == 0 { 300.0 } else { 1.0 };
                *t = (self.rng.below(2000) as f64) / 10.0 * hot;
            }
        } else {
            let kids = if depth < 4 {
                4
            } else {
                1 + self.rng.below(3) as usize
            };
            for k in 0..kids {
                let share = if k + 1 == kids {
                    left
                } else {
                    left / (kids - k)
                };
                if share == 0 {
                    continue;
                }
                left -= share;
                let (idx, t) = self.build(depth + 1, share);
                refs.push(idx);
                for (a, b) in times.iter_mut().zip(t) {
                    *a += b;
                }
            }
            for t in times.iter_mut() {
                *t += (self.rng.below(50) as f64) / 10.0;
            }
        }
        self.nodes.push(StackTraceNode {
            class_name: class,
            method_name: method,
            parent_line_number: 0,
            line_number: 0,
            method_desc: "()V".into(),
            times: times.clone(),
            children_refs: refs,
        });
        ((self.nodes.len() - 1) as i32, times)
    }
}

fn thread(name: &str, seed: u64, size: usize, windows: usize, plugins: usize) -> ThreadNode {
    let mut g = Gen {
        rng: Rng(seed),
        windows,
        nodes: Vec::with_capacity(size),
        plugins,
    };
    let (root, times) = g.build(0, size);
    ThreadNode {
        name: name.into(),
        children: g.nodes,
        times,
        children_refs: vec![root],
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let size: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(300_000);
    let windows: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let alloc = args.iter().any(|a| a == "--alloc");
    let plugins = 60;

    let mut threads = vec![thread("Server thread", 7, size, windows, plugins)];
    for i in 0..15 {
        threads.push(thread(
            &format!("Worker-{i}"),
            100 + i,
            size / 20,
            windows,
            plugins,
        ));
    }
    let mut class_sources = BTreeMap::new();
    let mut sources = BTreeMap::new();
    for p in 0..plugins {
        for k in 0..40 {
            class_sources.insert(
                format!("com.plugin{p}.module{}.Class{k}", k % 5),
                format!("Plugin{p}"),
            );
        }
        sources.insert(
            format!("Plugin{p}"),
            PluginOrModMetadata {
                name: format!("Plugin{p}"),
                version: "1.0".into(),
                ..Default::default()
            },
        );
    }
    let ids: Vec<i32> = (0..windows as i32).map(|i| 1000 + i).collect();
    let mut stats = BTreeMap::new();
    for (i, id) in ids.iter().enumerate() {
        stats.insert(
            *id,
            WindowStatistics {
                ticks: 1200,
                tps: if i % 7 == 3 { 15.0 } else { 20.0 },
                mspt_median: 30.0,
                mspt_max: if i % 7 == 3 { 300.0 } else { 45.0 },
                duration: 60_000,
                ..Default::default()
            },
        );
    }
    let data = SamplerData {
        metadata: Some(SamplerMetadata {
            start_time: 1_700_000_000_000,
            end_time: 1_700_000_000_000 + windows as i64 * 60_000,
            interval: 4000,
            number_of_ticks: windows as i32 * 1200,
            sources,
            sampler_mode: if alloc { 1 } else { 0 },
            sampler_engine: 1,
            ..Default::default()
        }),
        threads,
        class_sources,
        time_windows: ids,
        time_window_statistics: stats,
        ..Default::default()
    };
    let bytes = data.encode_to_vec();
    let nodes: usize = data.threads.iter().map(|t| t.children.len()).sum();
    eprintln!(
        "profilo: {} nodi totali, {} finestre, {:.1} MB",
        nodes,
        windows,
        bytes.len() as f64 / 1e6
    );

    let total = Instant::now();
    let t = Instant::now();
    let loaded = sparkscope::loader::load_bytes(&bytes, "bench.sparkprofile").unwrap();
    eprintln!(
        "decodifica      {:>8.1} ms",
        t.elapsed().as_secs_f64() * 1e3
    );
    let t = Instant::now();
    let sparkscope::loader::Payload::Sampler(s) = loaded.payload else {
        panic!()
    };
    let p = sparkscope::model::Profile::from_proto(*s);
    eprintln!(
        "modello         {:>8.1} ms",
        t.elapsed().as_secs_f64() * 1e3
    );
    let t = Instant::now();
    let a = sparkscope::analysis::analyze(&p);
    eprintln!(
        "analisi         {:>8.1} ms",
        t.elapsed().as_secs_f64() * 1e3
    );
    let t = Instant::now();
    let r = sparkscope::report::build(&p, a, "bench", None, "x");
    eprintln!(
        "diagnosi+report {:>8.1} ms",
        t.elapsed().as_secs_f64() * 1e3
    );
    let t = Instant::now();
    let html = sparkscope::report::html::render(&r);
    eprintln!(
        "html            {:>8.1} ms ({:.1} MB)",
        t.elapsed().as_secs_f64() * 1e3,
        html.len() as f64 / 1e6
    );
    let t = Instant::now();
    let md = sparkscope::report::markdown::render(&r);
    eprintln!(
        "markdown        {:>8.1} ms ({} KB)",
        t.elapsed().as_secs_f64() * 1e3,
        md.len() / 1000
    );
    let t = Instant::now();
    let tree = sparkscope::analysis::render_tree(&p, &r.analysis, 0.5, 40, None);
    eprintln!(
        "albero          {:>8.1} ms ({} KB)",
        t.elapsed().as_secs_f64() * 1e3,
        tree.len() / 1000
    );
    eprintln!(
        "TOTALE          {:>8.1} ms",
        total.elapsed().as_secs_f64() * 1e3
    );
}
