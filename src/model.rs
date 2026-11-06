use std::collections::{BTreeMap, HashMap};

use crate::proto::{
    PluginOrModMetadata, SamplerData, SamplerMetadata, WindowStatistics,
    sampler_metadata::SamplerMode,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Mode {
    Execution,
    Allocation,
    Lock,
}

impl Mode {
    pub fn unit(self) -> &'static str {
        match self {
            Mode::Execution | Mode::Lock => "ms",
            Mode::Allocation => "byte",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub class: String,
    pub method: String,
    pub desc: String,
    pub line: i32,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    pub total: f64,
}

impl Node {
    pub fn simple_class(&self) -> &str {
        simple_class(&self.class)
    }

    pub fn label(&self) -> String {
        format!("{}.{}", self.class, self.method)
    }

    pub fn short_label(&self) -> String {
        format!("{}.{}", self.simple_class(), self.method)
    }
}

pub fn simple_class(class: &str) -> &str {
    class.rsplit('.').next().unwrap_or(class)
}

pub fn outer_class(class: &str) -> &str {
    match class.find('$') {
        Some(i) => &class[..i],
        None => class,
    }
}

#[derive(Clone, Debug)]
pub struct Thread {
    pub name: String,
    pub nodes: Vec<Node>,
    pub roots: Vec<u32>,
    pub windows: usize,
    pub node_times: Vec<f32>,
    pub total: f64,
    pub window_totals: Vec<f64>,
}

impl Thread {
    pub fn node(&self, id: u32) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn times_of(&self, id: u32) -> &[f32] {
        let start = id as usize * self.windows;
        &self.node_times[start..start + self.windows]
    }

    pub fn self_time(&self, id: u32) -> f64 {
        let n = self.node(id);
        let child_sum: f64 = n.children.iter().map(|c| self.node(*c).total).sum();
        (n.total - child_sum).max(0.0)
    }

    pub fn path_to(&self, id: u32) -> Vec<u32> {
        let mut out = vec![id];
        let mut cur = self.node(id).parent;
        while let Some(p) = cur {
            out.push(p);
            cur = self.node(p).parent;
        }
        out.reverse();
        out
    }

    pub fn is_server_thread(&self) -> bool {
        let n = self.name.to_ascii_lowercase();
        n == "server thread"
            || n.starts_with("server thread")
            || n.contains("region scheduler thread")
            || n.starts_with("tick thread")
    }
}

#[derive(Clone, Debug, Default)]
pub struct SourceResolver {
    pub class_sources: HashMap<String, String>,
    pub method_sources: HashMap<String, String>,
    pub line_sources: HashMap<String, String>,
    pub meta: BTreeMap<String, PluginOrModMetadata>,
}

impl SourceResolver {
    pub fn resolve(&self, node: &Node) -> Option<&str> {
        if !self.method_sources.is_empty() {
            let key = format!("{};{};{}", node.class, node.method, node.desc);
            if let Some(s) = self.method_sources.get(&key) {
                return Some(s);
            }
        }
        if !self.line_sources.is_empty() && node.line > 0 {
            let key = format!("{};{}", node.class, node.line);
            if let Some(s) = self.line_sources.get(&key) {
                return Some(s);
            }
        }
        if let Some(s) = self.class_sources.get(&node.class) {
            return Some(s);
        }
        let outer = outer_class(&node.class);
        if outer != node.class
            && let Some(s) = self.class_sources.get(outer) {
                return Some(s);
            }
        None
    }

    pub fn is_builtin(&self, source: &str) -> bool {
        self.meta.get(source).map(|m| m.builtin).unwrap_or(false)
    }

    pub fn display_name(&self, source: &str) -> String {
        match self.meta.get(source) {
            Some(m) if !m.version.is_empty() => format!(
                "{} {}",
                if m.name.is_empty() { source } else { &m.name },
                m.version
            ),
            Some(m) if !m.name.is_empty() => m.name.clone(),
            _ => source.to_string(),
        }
    }
}

pub struct Profile {
    pub meta: SamplerMetadata,
    pub mode: Mode,
    pub window_ids: Vec<i32>,
    pub window_stats: BTreeMap<i32, WindowStatistics>,
    pub threads: Vec<Thread>,
    pub sources: SourceResolver,
    pub thread_override: Option<usize>,
}

impl Profile {
    pub fn from_health(h: crate::proto::HealthData) -> Profile {
        let hm = h.metadata.unwrap_or_default();
        let meta = SamplerMetadata {
            creator: hm.creator,
            platform_metadata: hm.platform_metadata,
            platform_statistics: hm.platform_statistics,
            system_statistics: hm.system_statistics,
            server_configurations: hm.server_configurations,
            sources: hm.sources.clone(),
            extra_platform_metadata: hm.extra_platform_metadata,
            metrics: hm.metrics,
            start_time: hm.generated_time,
            end_time: hm.generated_time,
            ..Default::default()
        };
        let window_ids: Vec<i32> = h.time_window_statistics.keys().copied().collect();
        let w = window_ids.len().max(1);
        let thread = Thread {
            name: "Server thread".into(),
            nodes: Vec::new(),
            roots: Vec::new(),
            windows: w,
            node_times: Vec::new(),
            total: 0.0,
            window_totals: vec![0.0; w],
        };
        Profile {
            meta,
            mode: Mode::Execution,
            window_ids,
            window_stats: h.time_window_statistics,
            threads: vec![thread],
            sources: SourceResolver {
                meta: hm.sources,
                ..Default::default()
            },
            thread_override: None,
        }
    }

    pub fn select_thread(&mut self, name: &str) -> bool {
        let q = name.to_ascii_lowercase();
        let idx = self
            .threads
            .iter()
            .position(|t| t.name.to_ascii_lowercase() == q)
            .or_else(|| {
                self.threads
                    .iter()
                    .position(|t| t.name.to_ascii_lowercase().contains(&q))
            });
        self.thread_override = idx;
        idx.is_some()
    }

    pub fn from_proto(data: SamplerData) -> Profile {
        let meta = data.metadata.unwrap_or_default();
        let mode = match SamplerMode::try_from(meta.sampler_mode).unwrap_or(SamplerMode::Execution)
        {
            SamplerMode::Execution => Mode::Execution,
            SamplerMode::Allocation => Mode::Allocation,
            SamplerMode::Lock => Mode::Lock,
        };
        let mut window_ids = data.time_windows.clone();
        window_ids.sort_unstable();
        window_ids.dedup();

        let mut threads = Vec::with_capacity(data.threads.len());
        for t in data.threads {
            threads.push(build_thread(t, window_ids.len()));
        }
        threads.sort_by(|a, b| {
            b.total
                .partial_cmp(&a.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let sources = SourceResolver {
            class_sources: data.class_sources.into_iter().collect(),
            method_sources: data.method_sources.into_iter().collect(),
            line_sources: data.line_sources.into_iter().collect(),
            meta: meta.sources.clone(),
        };
        Profile {
            mode,
            window_ids,
            window_stats: data.time_window_statistics,
            threads,
            sources,
            meta,
            thread_override: None,
        }
    }

    pub fn server_thread(&self) -> Option<usize> {
        self.threads
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case("server thread"))
            .or_else(|| self.threads.iter().position(|t| t.is_server_thread()))
    }

    pub fn main_thread_index(&self) -> usize {
        self.thread_override
            .or_else(|| self.server_thread())
            .unwrap_or(0)
    }

    pub fn duration_ms(&self) -> i64 {
        let m = &self.meta;
        if m.end_time > m.start_time {
            m.end_time - m.start_time
        } else {
            0
        }
    }

    pub fn interval_ms(&self) -> f64 {
        match self.mode {
            Mode::Execution => self.meta.interval as f64 / 1000.0,
            _ => self.meta.interval as f64,
        }
    }

    pub fn ticked_threshold(&self) -> Option<i64> {
        let agg = self.meta.data_aggregator.as_ref()?;
        if agg.r#type == 1 {
            Some(agg.tick_length_threshold)
        } else {
            None
        }
    }
}

fn build_thread(t: crate::proto::ThreadNode, expected_windows: usize) -> Thread {
    let windows = if t.times.is_empty() {
        expected_windows.max(1)
    } else {
        t.times.len()
    };
    let count = t.children.len();
    let mut nodes: Vec<Node> = Vec::with_capacity(count);
    let mut node_times: Vec<f32> = vec![0.0; count * windows];
    for (i, c) in t.children.into_iter().enumerate() {
        let total: f64 = c.times.iter().sum();
        let base = i * windows;
        for (w, v) in c.times.iter().enumerate().take(windows) {
            node_times[base + w] = *v as f32;
        }
        let children = c
            .children_refs
            .iter()
            .filter_map(|r| {
                if *r >= 0 && (*r as usize) < count {
                    Some(*r as u32)
                } else {
                    None
                }
            })
            .collect();
        nodes.push(Node {
            class: c.class_name,
            method: c.method_name,
            desc: c.method_desc,
            line: c.line_number,
            parent: None,
            children,
            total,
        });
    }
    for i in 0..nodes.len() {
        let kids = nodes[i].children.clone();
        for k in kids {
            nodes[k as usize].parent = Some(i as u32);
        }
    }
    let roots: Vec<u32> = t
        .children_refs
        .iter()
        .filter_map(|r| {
            if *r >= 0 && (*r as usize) < count {
                Some(*r as u32)
            } else {
                None
            }
        })
        .collect();
    let roots = if roots.is_empty() {
        (0..nodes.len() as u32)
            .filter(|i| nodes[*i as usize].parent.is_none())
            .collect()
    } else {
        roots
    };
    let mut window_totals: Vec<f64> = if t.times.is_empty() {
        vec![0.0; windows]
    } else {
        t.times.clone()
    };
    window_totals.resize(windows, 0.0);
    let mut total: f64 = window_totals.iter().sum();
    if total <= 0.0 {
        total = roots.iter().map(|r| nodes[*r as usize].total).sum();
        if windows == 1 {
            window_totals[0] = total;
        }
    }
    Thread {
        name: t.name,
        nodes,
        roots,
        windows,
        node_times,
        total,
        window_totals,
    }
}
