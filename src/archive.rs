use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

const META_EXT: &str = "json";
const FIRST_EXT: &str = "a.gz";
const SECOND_EXT: &str = "b.gz";
const DIR_NAME: &str = "spark-analyzer";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchivedReport {
    pub id: String,
    pub owner: String,
    pub title: String,
    pub detail: String,
    pub bad: bool,
    pub name: String,
    pub key: Option<String>,
    pub second_name: Option<String>,
    pub second_key: Option<String>,
    pub created: u64,
}

pub struct Archive {
    dir: PathBuf,
    retention: Duration,
}

impl Archive {
    pub fn open(dir: PathBuf, retention: Duration) -> std::io::Result<Archive> {
        std::fs::create_dir_all(&dir)?;
        let probe = dir.join(".write-test");
        std::fs::write(&probe, b"ok")?;
        std::fs::remove_file(&probe)?;
        Ok(Archive { dir, retention })
    }

    pub fn candidate_dirs() -> Vec<PathBuf> {
        let env = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let mut dirs = Vec::new();
        if let Some(dir) = env("SPARKSCOPE_DATA") {
            dirs.push(dir);
        }
        if let Ok(state) = std::env::var("STATE_DIRECTORY")
            && let Some(first) = state.split(':').next().filter(|v| !v.is_empty())
        {
            dirs.push(PathBuf::from(first));
        }
        if cfg!(windows) {
            if let Some(dir) = env("APPDATA") {
                dirs.push(dir.join(DIR_NAME));
            }
        } else if let Some(dir) = env("XDG_DATA_HOME") {
            dirs.push(dir.join(DIR_NAME));
        } else if let Some(home) = env("HOME") {
            dirs.push(home.join(".local").join("share").join(DIR_NAME));
        }
        if !cfg!(windows) {
            dirs.push(PathBuf::from("/var/lib").join(DIR_NAME));
        }
        if let Some(exe_dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
        {
            dirs.push(exe_dir.join("data"));
        }
        dirs
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn retention(&self) -> Duration {
        self.retention
    }

    pub fn save(
        &self,
        meta: &ArchivedReport,
        first: &[u8],
        second: Option<&[u8]>,
    ) -> std::io::Result<()> {
        let id = checked_id(&meta.id)?;
        self.write_atomic(&self.file(id, FIRST_EXT), &compress(first)?)?;
        if let Some(bytes) = second {
            self.write_atomic(&self.file(id, SECOND_EXT), &compress(bytes)?)?;
        }
        self.update(meta)
    }

    pub fn update(&self, meta: &ArchivedReport) -> std::io::Result<()> {
        let id = checked_id(&meta.id)?;
        let json = serde_json::to_vec(meta).map_err(std::io::Error::other)?;
        self.write_atomic(&self.file(id, META_EXT), &json)
    }

    pub fn load_all(&self, now: u64) -> Vec<ArchivedReport> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut reports = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some(META_EXT) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(meta) = serde_json::from_slice::<ArchivedReport>(&bytes) else {
                continue;
            };
            if checked_id(&meta.id).is_err() || self.is_expired(&meta, now) {
                continue;
            }
            if !self.file(&meta.id, FIRST_EXT).is_file() {
                continue;
            }
            reports.push(meta);
        }
        reports.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| a.id.cmp(&b.id)));
        reports
    }

    pub fn payload(&self, id: &str) -> std::io::Result<(Vec<u8>, Option<Vec<u8>>)> {
        let id = checked_id(id)?;
        let first = decompress(&std::fs::read(self.file(id, FIRST_EXT))?)?;
        let second_path = self.file(id, SECOND_EXT);
        let second = if second_path.is_file() {
            Some(decompress(&std::fs::read(second_path)?)?)
        } else {
            None
        };
        Ok((first, second))
    }

    pub fn prune(&self, now: u64) -> Vec<String> {
        let mut removed = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return removed;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(file) = path.file_name().and_then(|f| f.to_str()) else {
                continue;
            };
            let Some(id) = file.split('.').next() else {
                continue;
            };
            if checked_id(id).is_err() || !file.ends_with(META_EXT) {
                continue;
            }
            let expired = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<ArchivedReport>(&b).ok())
                .is_none_or(|meta| self.is_expired(&meta, now));
            if expired {
                for ext in [META_EXT, FIRST_EXT, SECOND_EXT] {
                    let _ = std::fs::remove_file(self.file(id, ext));
                }
                removed.push(id.to_string());
            }
        }
        removed
    }

    pub fn is_expired(&self, meta: &ArchivedReport, now: u64) -> bool {
        now.saturating_sub(meta.created) > self.retention.as_secs()
    }

    fn file(&self, id: &str, ext: &str) -> PathBuf {
        self.dir.join(format!("{id}.{ext}"))
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let tmp = path.with_extension("tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn checked_id(id: &str) -> std::io::Result<&str> {
    let valid = !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_hexdigit());
    if valid {
        Ok(id)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid report id",
        ))
    }
}

fn compress(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut e = flate2::write::GzEncoder::new(
        Vec::with_capacity(bytes.len() / 4),
        flate2::Compression::default(),
    );
    e.write_all(bytes)?;
    e.finish()
}

fn decompress(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(bytes.len() * 4);
    flate2::read::GzDecoder::new(bytes).read_to_end(&mut out)?;
    Ok(out)
}
