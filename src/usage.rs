use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

pub struct UsageStats {
    file: Option<PathBuf>,
    totals: Mutex<Totals>,
}

#[derive(Default, Serialize, Deserialize)]
struct Totals {
    people: HashSet<String>,
    analyses: u64,
}

impl UsageStats {
    pub fn in_memory() -> UsageStats {
        UsageStats {
            file: None,
            totals: Mutex::new(Totals::default()),
        }
    }

    pub fn load<'a>(file: PathBuf, existing_owners: impl Iterator<Item = &'a str>) -> UsageStats {
        let saved = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice::<Totals>(&b).ok());
        let fresh = saved.is_none();
        let mut totals = saved.unwrap_or_default();
        if fresh {
            for owner in existing_owners {
                totals.people.insert(fingerprint(owner));
                totals.analyses += 1;
            }
        }
        let stats = UsageStats {
            file: Some(file),
            totals: Mutex::new(totals),
        };
        if fresh {
            stats.persist(&stats.lock());
        }
        stats
    }

    pub fn record(&self, owner: &str) {
        let mut totals = self.lock();
        totals.people.insert(fingerprint(owner));
        totals.analyses += 1;
        self.persist(&totals);
    }

    pub fn snapshot(&self) -> (u64, u64) {
        let totals = self.lock();
        (totals.people.len() as u64, totals.analyses)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Totals> {
        self.totals.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn persist(&self, totals: &Totals) {
        let Some(file) = &self.file else {
            return;
        };
        let Ok(json) = serde_json::to_vec(totals) else {
            return;
        };
        let tmp = file.with_extension("tmp");
        let written = std::fs::File::create(&tmp)
            .and_then(|mut f| f.write_all(&json).and_then(|()| f.sync_all()))
            .and_then(|()| std::fs::rename(&tmp, file));
        if let Err(e) = written {
            eprintln!("usage stats {}: {e}", file.display());
        }
    }
}

fn fingerprint(owner: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in b"spark-analyzer-usage:".iter().chain(owner.as_bytes()) {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}
