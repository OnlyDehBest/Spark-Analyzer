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
            && let Some(s) = self.class_sources.get(outer)
        {
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
    pub window_filter: Option<Vec<usize>>,
    pub windows_total: usize,
    pub window_map: Vec<usize>,
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
            window_filter: None,
            windows_total: 0,
            window_map: Vec::new(),
        }
    }

    pub fn window_count(&self) -> usize {
        self.threads.first().map(|t| t.windows).unwrap_or(0)
    }

    pub fn restrict_windows(&mut self, selected: &[usize]) -> bool {
        let total = self.window_count();
        let mut sel: Vec<usize> = selected.iter().copied().filter(|w| *w < total).collect();
        sel.sort_unstable();
        sel.dedup();
        if total < 2 || sel.is_empty() || sel.len() == total {
            return false;
        }
        let ids: Vec<i32> = sel
            .iter()
            .map(|i| self.window_ids.get(*i).copied().unwrap_or(*i as i32))
            .collect();
        for t in &mut self.threads {
            let w = t.windows;
            if w != total {
                continue;
            }
            let n = t.nodes.len();
            let mut times = vec![0.0f32; n * sel.len()];
            for node in 0..n {
                let mut sum = 0.0f64;
                for (k, wi) in sel.iter().enumerate() {
                    let v = t.node_times[node * w + wi];
                    times[node * sel.len() + k] = v;
                    sum += v as f64;
                }
                t.nodes[node].total = sum;
            }
            t.node_times = times;
            t.window_totals = sel
                .iter()
                .map(|wi| t.window_totals.get(*wi).copied().unwrap_or(0.0))
                .collect();
            t.total = t.window_totals.iter().sum();
            if t.total <= 0.0 {
                t.total = t.roots.iter().map(|r| t.nodes[*r as usize].total).sum();
            }
            t.windows = sel.len();
        }
        self.threads.sort_by(|a, b| {
            b.total
                .partial_cmp(&a.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let stats: Vec<&WindowStatistics> = ids
            .iter()
            .filter_map(|id| self.window_stats.get(id))
            .collect();
        let tick_sum: i64 = stats.iter().map(|s| s.ticks as i64).sum();
        let scale = sel.len() as f64 / total as f64;
        if stats.len() == sel.len() && tick_sum > 0 {
            self.meta.number_of_ticks = tick_sum as i32;
        } else {
            self.meta.number_of_ticks = (self.meta.number_of_ticks as f64 * scale).round() as i32;
        }
        if let Some(agg) = self.meta.data_aggregator.as_mut() {
            if agg.number_of_included_ticks > 0 {
                agg.number_of_included_ticks = ((agg.number_of_included_ticks as f64) * scale)
                    .round()
                    .max(1.0) as i32;
            }
        }
        let starts: Vec<i64> = stats
            .iter()
            .map(|s| s.start_time)
            .filter(|v| *v > 0)
            .collect();
        let ends: Vec<i64> = stats
            .iter()
            .map(|s| s.end_time)
            .filter(|v| *v > 0)
            .collect();
        if let (Some(a), Some(b)) = (starts.iter().min(), ends.iter().max()) {
            self.meta.start_time = *a;
            self.meta.end_time = *b;
        } else {
            let dur = (self.duration_ms() as f64 * scale) as i64;
            self.meta.end_time = self.meta.start_time + dur;
        }
        self.window_stats.retain(|k, _| ids.contains(k));
        self.window_ids = ids;
        self.window_map = sel.clone();
        self.windows_total = total;
        self.window_filter = Some(sel);
        true
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
        if threads.is_empty() {
            let w = window_ids.len().max(1);
            threads.push(Thread {
                name: "Server thread".into(),
                nodes: Vec::new(),
                roots: Vec::new(),
                windows: w,
                node_times: Vec::new(),
                total: 0.0,
                window_totals: vec![0.0; w],
            });
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
            window_filter: None,
            windows_total: 0,
            window_map: Vec::new(),
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

    pub fn effective_ticks(&self) -> i32 {
        if let Some(agg) = self.meta.data_aggregator.as_ref() {
            if agg.r#type == 1 && agg.number_of_included_ticks > 0 {
                return agg.number_of_included_ticks;
            }
        }
        self.meta.number_of_ticks
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
