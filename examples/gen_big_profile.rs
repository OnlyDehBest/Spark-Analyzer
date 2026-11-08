#[path = "../tests/support/fixture.rs"]
mod fixture;

use std::collections::BTreeMap;

use prost::Message;
use sparkscope::proto::{PluginOrModMetadata, SamplerData, StackTraceNode, ThreadNode};

const WINDOWS: usize = 30;
const PLUGINS: usize = 12;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

struct Builder<'a> {
    rng: Rng,
    nodes: Vec<StackTraceNode>,
    classes: &'a [String],
    budget: usize,
}

impl Builder<'_> {
    fn grow(&mut self, depth: usize) -> (i32, Vec<f64>) {
        let fanout = match depth {
            0..=3 => 3 + self.rng.below(6),
            4..=18 => self.rng.below(4),
            _ => u64::from(self.rng.below(5) == 0),
        };
        let mut refs = Vec::new();
        let mut times = vec![0.0; WINDOWS];
        for _ in 0..fanout {
            if self.nodes.len() >= self.budget {
                break;
            }
            let (r, t) = self.grow(depth + 1);
            refs.push(r);
            for (a, b) in times.iter_mut().zip(&t) {
                *a += b;
            }
        }
        let own = (self.rng.below(200) as f64) * if refs.is_empty() { 4.0 } else { 0.3 };
        for (i, t) in times.iter_mut().enumerate() {
            *t += own * (1.0 + (i % 7) as f64 * 0.05);
        }
        let class = self.classes[self.rng.below(self.classes.len() as u64) as usize].clone();
        self.nodes.push(StackTraceNode {
            class_name: class,
            method_name: format!("m{}", self.rng.below(40)),
            parent_line_number: 0,
            line_number: 0,
            method_desc: String::new(),
            times: times.clone(),
            children_refs: refs,
        });
        ((self.nodes.len() - 1) as i32, times)
    }
}

fn thread(name: &str, nodes: usize, seed: u64, classes: &[String]) -> ThreadNode {
    let mut b = Builder {
        rng: Rng(seed),
        nodes: Vec::with_capacity(nodes),
        classes,
        budget: nodes,
    };
    let mut roots = Vec::new();
    let mut total = vec![0.0; WINDOWS];
    while b.nodes.len() < nodes {
        let (r, t) = b.grow(0);
        roots.push(r);
        for (a, x) in total.iter_mut().zip(&t) {
            *a += x;
        }
    }
    ThreadNode {
        name: name.into(),
        children: b.nodes,
        times: total,
        children_refs: roots,
    }
}

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "big.sparkprofile".into());
    let main_nodes: usize = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(200_000);
    let mut classes: Vec<String> = (0..2500)
        .map(|i| format!("net.minecraft.world.level.pkg{}.Class{i}", i % 40))
        .collect();
    let mut sources = BTreeMap::new();
    let mut class_sources = BTreeMap::new();
    for p in 0..PLUGINS {
        let name = format!("Plugin{p}");
        sources.insert(
            name.clone(),
            PluginOrModMetadata {
                name: name.clone(),
                version: "1.0".into(),
                author: String::new(),
                description: String::new(),
                builtin: false,
            },
        );
        for c in 0..40 {
            let class = format!("com.example.plugin{p}.feature.Class{c}");
            class_sources.insert(class.clone(), name.clone());
            classes.push(class);
        }
    }
    let mut data: SamplerData = fixture::sample();
    let meta = data.metadata.get_or_insert_with(Default::default);
    meta.sources.extend(sources);
    meta.end_time = meta.start_time + (WINDOWS as i64) * 60_000;
    let mut threads = vec![thread("Server thread", main_nodes, 7, &classes)];
    for t in 0..40 {
        threads.push(thread(
            &format!("Worker-{t}"),
            main_nodes / 60,
            100 + t,
            &classes,
        ));
    }
    data.threads = threads;
    data.class_sources.extend(class_sources);
    data.time_windows = (0..WINDOWS as i32).collect();
    let first = data
        .time_window_statistics
        .values()
        .next()
        .cloned()
        .unwrap_or_default();
    data.time_window_statistics = (0..WINDOWS as i32).map(|w| (w, first.clone())).collect();
    let bytes = data.encode_to_vec();
    std::fs::write(&out, &bytes).expect("scrittura fallita");
    eprintln!("scritto {out}: {:.1} MB", bytes.len() as f64 / 1e6);
}
