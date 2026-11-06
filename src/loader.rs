use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use prost::Message;

use crate::proto::{HealthData, HeapData, SamplerData};

pub const DEFAULT_BYTEBIN: &str = "https://spark-usercontent.lucko.me/";

pub enum Payload {
    Sampler(Box<SamplerData>),
    Heap(Box<HeapData>),
    Health(Box<HealthData>),
}

impl Payload {
    pub fn kind(&self) -> &'static str {
        match self {
            Payload::Sampler(_) => "profilo CPU (sampler)",
            Payload::Heap(_) => "heap summary",
            Payload::Health(_) => "health report",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Hint {
    Sampler,
    Heap,
    Health,
    Unknown,
}

pub struct Loaded {
    pub payload: Payload,
    pub origin: String,
    pub viewer_url: Option<String>,
}

pub fn load(input: &str, bytebin: &str) -> Result<Loaded> {
    let path = Path::new(input);
    if path.exists() {
        let bytes = std::fs::read(path).with_context(|| format!("impossibile leggere {input}"))?;
        let hint = match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
            "sparkprofile" => Hint::Sampler,
            "sparkheap" => Hint::Heap,
            "sparkhealth" => Hint::Health,
            _ => Hint::Unknown,
        };
        let payload = decode(&bytes, hint)?;
        return Ok(Loaded {
            payload,
            origin: input.to_string(),
            viewer_url: None,
        });
    }
    let key = extract_key(input).ok_or_else(|| {
        anyhow!("'{input}' non è un file esistente né un link/codice spark valido (es. https://spark.lucko.me/AbCd1234)")
    })?;
    let (bytes, hint) = download(bytebin, &key)?;
    let payload = decode(&bytes, hint)?;
    Ok(Loaded {
        payload,
        origin: format!("{}{}", normalize_base(bytebin), key),
        viewer_url: Some(format!("https://spark.lucko.me/{key}")),
    })
}

pub fn load_bytes(bytes: &[u8], name: &str) -> Result<Loaded> {
    let lower = name.to_ascii_lowercase();
    let hint = if lower.ends_with(".sparkprofile") {
        Hint::Sampler
    } else if lower.ends_with(".sparkheap") {
        Hint::Heap
    } else if lower.ends_with(".sparkhealth") {
        Hint::Health
    } else {
        Hint::Unknown
    };
    let payload = decode(bytes, hint)?;
    Ok(Loaded {
        payload,
        origin: name.to_string(),
        viewer_url: None,
    })
}

pub fn load_link(input: &str, bytebin: &str) -> Result<Loaded> {
    let key = extract_key(input).ok_or_else(|| {
        anyhow!(
            "'{input}' non è un link o codice spark valido (es. https://spark.lucko.me/AbCd1234)"
        )
    })?;
    let (bytes, hint) = download(bytebin, &key)?;
    let payload = decode(&bytes, hint)?;
    Ok(Loaded {
        payload,
        origin: format!("{}{}", normalize_base(bytebin), key),
        viewer_url: Some(format!("https://spark.lucko.me/{key}")),
    })
}

pub fn extract_key(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let candidate = if let Some((_, rest)) = without_scheme.split_once('/') {
        rest
    } else {
        without_scheme
    };
    let candidate = candidate
        .split(['?', '#', '/'])
        .find(|s| !s.is_empty())
        .unwrap_or("");
    let valid = candidate.len() >= 4
        && candidate.len() <= 64
        && candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Some(candidate.to_string())
    } else {
        None
    }
}

fn normalize_base(base: &str) -> String {
    if base.ends_with('/') {
        base.to_string()
    } else {
        format!("{base}/")
    }
}

fn download(bytebin: &str, key: &str) -> Result<(Vec<u8>, Hint)> {
    let url = format!("{}{}", normalize_base(bytebin), key);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .user_agent(concat!("sparkscope/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut resp = agent
        .get(&url)
        .call()
        .with_context(|| format!("download fallito da {url} (il link potrebbe essere scaduto)"))?;
    let ctype = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let hint = if ctype.contains("x-spark-sampler") {
        Hint::Sampler
    } else if ctype.contains("x-spark-heap") {
        Hint::Heap
    } else if ctype.contains("x-spark-health") {
        Hint::Health
    } else {
        Hint::Unknown
    };
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(512 * 1024 * 1024)
        .read_to_vec()
        .context("lettura della risposta fallita")?;
    Ok((bytes, hint))
}

fn maybe_gunzip(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() > 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut out = Vec::with_capacity(bytes.len() * 4);
        flate2::read::MultiGzDecoder::new(bytes)
            .read_to_end(&mut out)
            .context("decompressione gzip fallita")?;
        Ok(out)
    } else {
        Ok(bytes.to_vec())
    }
}

fn decode(raw: &[u8], hint: Hint) -> Result<Payload> {
    let bytes = maybe_gunzip(raw)?;
    match hint {
        Hint::Sampler => return Ok(Payload::Sampler(Box::new(SamplerData::decode(&*bytes)?))),
        Hint::Heap => return Ok(Payload::Heap(Box::new(HeapData::decode(&*bytes)?))),
        Hint::Health => return Ok(Payload::Health(Box::new(HealthData::decode(&*bytes)?))),
        Hint::Unknown => {}
    }
    if let Ok(s) = SamplerData::decode(&*bytes)
        && !s.threads.is_empty()
        && s.metadata.is_some()
    {
        return Ok(Payload::Sampler(Box::new(s)));
    }
    if let Ok(h) = HeapData::decode(&*bytes)
        && !h.entries.is_empty()
    {
        return Ok(Payload::Heap(Box::new(h)));
    }
    if let Ok(h) = HealthData::decode(&*bytes)
        && h.metadata.is_some()
    {
        return Ok(Payload::Health(Box::new(h)));
    }
    bail!("formato non riconosciuto: non sembra un file spark (sampler, heap o health)")
}

#[cfg(test)]
mod tests {
    use super::extract_key;

    #[test]
    fn keys() {
        assert_eq!(
            extract_key("https://spark.lucko.me/AbCd1234").as_deref(),
            Some("AbCd1234")
        );
        assert_eq!(
            extract_key("spark.lucko.me/AbCd1234?hl=12").as_deref(),
            Some("AbCd1234")
        );
        assert_eq!(extract_key("AbCd1234").as_deref(), Some("AbCd1234"));
        assert_eq!(extract_key("https://spark.lucko.me/").as_deref(), None);
    }
}
