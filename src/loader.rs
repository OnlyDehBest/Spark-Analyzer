#![cfg_attr(not(feature = "cli"), allow(unused_imports, dead_code))]

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
            Payload::Sampler(_) => crate::i18n::t("profilo CPU (sampler)", "CPU profile (sampler)"),
            Payload::Heap(_) => "heap summary",
            Payload::Health(_) => crate::i18n::t("health report", "health report"),
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

#[cfg(feature = "cli")]
pub fn load(input: &str, bytebin: &str) -> Result<Loaded> {
    let path = Path::new(input);
    if path.exists() {
        let bytes = std::fs::read(path)
            .with_context(|| tf!("impossibile leggere {input}", "cannot read {input}"))?;
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
        anyhow!("{}", tf!("'{input}' non è un file esistente né un link/codice spark valido (es. https://spark.lucko.me/AbCd1234)", "'{input}' is neither an existing file nor a valid spark link/code (e.g. https://spark.lucko.me/AbCd1234)"))
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

#[cfg(feature = "cli")]
pub fn fetch_link(input: &str, bytebin: &str) -> Result<(Vec<u8>, String)> {
    let key = extract_key(input).ok_or_else(|| {
        anyhow!("{}", tf!("'{input}' non è un link o codice spark valido (es. https://spark.lucko.me/AbCd1234)", "'{input}' is not a valid spark link or code (e.g. https://spark.lucko.me/AbCd1234)"))
    })?;
    let (bytes, _) = download(bytebin, &key)?;
    Ok((bytes, key))
}

#[cfg(feature = "cli")]
pub fn load_link(input: &str, bytebin: &str) -> Result<Loaded> {
    let key = extract_key(input).ok_or_else(|| {
        anyhow!("{}", tf!("'{input}' non è un link o codice spark valido (es. https://spark.lucko.me/AbCd1234)", "'{input}' is not a valid spark link or code (e.g. https://spark.lucko.me/AbCd1234)"))
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

#[cfg(feature = "cli")]
fn download(bytebin: &str, key: &str) -> Result<(Vec<u8>, Hint)> {
    let url = format!("{}{}", normalize_base(bytebin), key);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .user_agent(concat!("sparkscope/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut resp = agent.get(&url).call().with_context(|| {
        tf!(
            "download fallito da {url} (il link potrebbe essere scaduto)",
            "download from {url} failed (the link may have expired)"
        )
    })?;
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
        .context(crate::i18n::t(
            "lettura della risposta fallita",
            "failed to read the response",
        ))?;
    Ok((bytes, hint))
}

pub fn spark_content_type(payload: &Payload) -> &'static str {
    match payload {
        Payload::Sampler(_) => "application/x-spark-sampler",
        Payload::Heap(_) => "application/x-spark-heap",
        Payload::Health(_) => "application/x-spark-health",
    }
}

pub fn is_gzip(bytes: &[u8]) -> bool {
    bytes.len() > 2 && bytes[0] == 0x1f && bytes[1] == 0x8b
}

#[cfg(feature = "cli")]
pub fn upload(bytes: &[u8], content_type: &str, bytebin: &str) -> Result<String> {
    let url = format!("{}post", normalize_base(bytebin));
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .user_agent(concat!("sparkscope/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut request = agent.post(&url).header("Content-Type", content_type);
    if is_gzip(bytes) {
        request = request.header("Content-Encoding", "gzip");
    }
    let mut resp = request
        .send(bytes)
        .with_context(|| tf!("caricamento su {url} fallito", "upload to {url} failed"))?;
    let body = resp.body_mut().read_to_string().context(crate::i18n::t(
        "lettura della risposta fallita",
        "failed to read the response",
    ))?;
    let key = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("key").and_then(|k| k.as_str()).map(str::to_string))
        .filter(|k| extract_key(k).as_deref() == Some(k.as_str()))
        .ok_or_else(|| {
            anyhow!(
                "{}",
                tf!(
                    "risposta inattesa dal server dei link: {body}",
                    "unexpected response from the link server: {body}"
                )
            )
        })?;
    Ok(key)
}

fn maybe_gunzip(bytes: &[u8]) -> Result<Vec<u8>> {
    if is_gzip(bytes) {
        let mut out = Vec::with_capacity(bytes.len() * 4);
        flate2::read::MultiGzDecoder::new(bytes)
            .read_to_end(&mut out)
            .context(crate::i18n::t(
                "decompressione gzip fallita",
                "gzip decompression failed",
            ))?;
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
    let mut errors: Vec<String> = Vec::new();
    match SamplerData::decode(&*bytes) {
        Ok(s) if s.metadata.is_some() && (!s.threads.is_empty() || looks_like_sampler(&s)) => {
            return Ok(Payload::Sampler(Box::new(s)));
        }
        Ok(_) => errors.push(crate::i18n::t("sampler: nessun dato", "sampler: no data").into()),
        Err(e) => errors.push(format!("sampler: {e}")),
    }
    match HeapData::decode(&*bytes) {
        Ok(h) if !h.entries.is_empty() => return Ok(Payload::Heap(Box::new(h))),
        Ok(_) => errors.push(crate::i18n::t("heap: nessun dato", "heap: no data").into()),
        Err(e) => errors.push(format!("heap: {e}")),
    }
    match HealthData::decode(&*bytes) {
        Ok(h) if h.metadata.is_some() => return Ok(Payload::Health(Box::new(h))),
        Ok(_) => errors.push(crate::i18n::t("health: nessun dato", "health: no data").into()),
        Err(e) => errors.push(format!("health: {e}")),
    }
    bail!("{}", describe_unknown(raw, &bytes, &errors))
}

fn looks_like_sampler(s: &SamplerData) -> bool {
    let m = s
        .metadata
        .as_ref()
        .map(|m| m.start_time > 0 || m.interval > 0 || m.platform_metadata.is_some());
    m.unwrap_or(false)
}

fn describe_unknown(raw: &[u8], bytes: &[u8], errors: &[String]) -> String {
    let head: Vec<String> = bytes.iter().take(16).map(|b| format!("{b:02x}")).collect();
    let printable = bytes
        .iter()
        .take(200)
        .filter(|b| b.is_ascii_graphic() || **b == b' ' || **b == b'\n')
        .count();
    let mut msg = String::from(crate::i18n::t(
        "formato non riconosciuto: non sembra un profilo spark (sampler, heap o health).",
        "unrecognized format: this doesn't look like a spark profile (sampler, heap or health).",
    ));
    if bytes.is_empty() {
        msg.push_str(crate::i18n::t(
            " Il contenuto è vuoto: il link potrebbe essere scaduto.",
            " The content is empty: the link may have expired.",
        ));
    } else if printable * 10 >= bytes.len().min(200) * 9 {
        let text: String =
            String::from_utf8_lossy(&bytes[..bytes.len().min(160)]).replace('\n', " ");
        msg.push_str(&tf!(
            " Il contenuto è testo, non un profilo: «{}»",
            " The content is text, not a profile: “{}”",
            text.trim()
        ));
    } else {
        msg.push_str(&tf!(
            " Dettagli: {} byte{}, inizio {}; {}.",
            " Details: {} bytes{}, starting with {}; {}.",
            bytes.len(),
            if raw.len() != bytes.len() {
                tf!(" ({} compressi)", " ({} compressed)", raw.len())
            } else {
                String::new()
            },
            head.join(" "),
            errors.join("; ")
        ));
    }
    msg
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
