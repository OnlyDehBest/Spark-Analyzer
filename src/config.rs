use std::collections::BTreeMap;

use serde_json::Value;

#[derive(Default, Clone)]
pub struct ServerConfig {
    files: BTreeMap<String, Value>,
}

#[derive(Clone, Debug)]
pub enum Cfg {
    Num(f64),
    Bool(bool),
    Text(String),
}

impl Cfg {
    pub fn num(&self) -> Option<f64> {
        match self {
            Cfg::Num(n) => Some(*n),
            Cfg::Text(t) => t.trim().parse().ok(),
            Cfg::Bool(_) => None,
        }
    }
    pub fn bool(&self) -> Option<bool> {
        match self {
            Cfg::Bool(b) => Some(*b),
            Cfg::Text(t) => match t.trim().to_ascii_lowercase().as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            },
            Cfg::Num(_) => None,
        }
    }
    pub fn text(&self) -> String {
        match self {
            Cfg::Num(n) => {
                if n.fract() == 0.0 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            Cfg::Bool(b) => b.to_string(),
            Cfg::Text(t) => t.clone(),
        }
    }
}

fn to_cfg(v: &Value) -> Option<Cfg> {
    match v {
        Value::Number(n) => n.as_f64().map(Cfg::Num),
        Value::Bool(b) => Some(Cfg::Bool(*b)),
        Value::String(s) => Some(Cfg::Text(s.clone())),
        Value::Null => None,
        other => Some(Cfg::Text(other.to_string())),
    }
}

pub struct Found {
    pub file: String,
    pub path: String,
    pub value: Cfg,
}

impl ServerConfig {
    pub fn new(raw: &BTreeMap<String, String>) -> ServerConfig {
        let mut files = BTreeMap::new();
        for (k, v) in raw {
            if let Ok(val) = serde_json::from_str::<Value>(v) {
                files.insert(k.clone(), val);
            }
        }
        ServerConfig { files }
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn file_names(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }

    pub fn has(&self, file: &str) -> bool {
        self.files.contains_key(file)
    }

    fn lookup<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
        if let Some(v) = root.get(path) {
            return Some(v);
        }
        let mut cur = root;
        let parts: Vec<&str> = path.split('.').collect();
        let mut i = 0;
        while i < parts.len() {
            let mut matched = false;
            for j in (i + 1..=parts.len()).rev() {
                let key = parts[i..j].join(".");
                if let Some(next) = cur.get(&key) {
                    cur = next;
                    i = j;
                    matched = true;
                    break;
                }
            }
            if !matched {
                return None;
            }
        }
        Some(cur)
    }

    pub fn get(&self, file: &str, path: &str) -> Option<Found> {
        let root = self.files.get(file)?;
        let v = Self::lookup(root, path)?;
        Some(Found {
            file: file.to_string(),
            path: path.to_string(),
            value: to_cfg(v)?,
        })
    }

    pub fn spigot_world(&self, path: &str) -> Option<Found> {
        self.get("spigot.yml", &format!("world-settings.default.{path}"))
    }

    pub fn paper_world(&self, path: &str, legacy: Option<&str>) -> Option<Found> {
        if let Some(root) = self.files.get("paper/")
            && let Some(wd) = root.get("world-defaults.yml")
            && let Some(v) = Self::lookup(wd, path)
            && let Some(c) = to_cfg(v)
        {
            return Some(Found {
                file: "config/paper-world-defaults.yml".into(),
                path: path.into(),
                value: c,
            });
        }
        if let Some(l) = legacy
            && let Some(f) = self.get("paper.yml", &format!("world-settings.default.{l}"))
        {
            return Some(f);
        }
        None
    }

    pub fn paper_global(&self, path: &str) -> Option<Found> {
        let root = self.files.get("paper/")?;
        let g = root.get("global.yml")?;
        let v = Self::lookup(g, path)?;
        Some(Found {
            file: "config/paper-global.yml".into(),
            path: path.into(),
            value: to_cfg(v)?,
        })
    }

    pub fn is_paper(&self) -> bool {
        self.files.contains_key("paper/") || self.files.contains_key("paper.yml")
    }

    pub fn is_purpur(&self) -> bool {
        self.files.contains_key("purpur.yml")
    }

    pub fn is_pufferfish(&self) -> bool {
        self.files.contains_key("pufferfish.yml")
    }
}

impl Found {
    pub fn describe(&self) -> String {
        format!("{} › {} = {}", self.file, self.path, self.value.text())
    }
}
