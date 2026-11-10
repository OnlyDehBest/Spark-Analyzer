//! Streaming reader for HotSpot heap dumps (`.hprof`, also gzip-compressed).
//!
//! The dump is read in chunks and never kept in memory: only class names and a
//! per-class histogram (instances and shallow size) are collected, which is what a
//! spark heap summary contains. The result is a `HeapData` that the heap analysis
//! and the heap comparison already understand.

use std::io::Write;

use anyhow::{Result, anyhow, bail};
use rustc_hash::FxHashMap as HashMap;

use crate::proto::{HeapData, HeapEntry, HeapMetadata};

const MAGIC: &[u8] = b"JAVA PROFILE";
const COMPACT_AT: usize = 1 << 20;

pub fn is_hprof(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

pub fn is_hprof_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".hprof") || n.ends_with(".hprof.gz") || n.ends_with(".hprof.gzip")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Header,
    Top,
    Heap,
}

#[derive(Default, Clone, Copy)]
struct ClassInfo {
    super_id: u64,
    own_refs: u32,
}

/// Incremental HPROF parser: call [`HprofParser::feed`] with consecutive chunks,
/// then [`HprofParser::finish`].
pub struct HprofParser {
    buf: Vec<u8>,
    pos: usize,
    state: State,
    skip: u64,
    heap_rem: u64,
    id_size: usize,
    timestamp: i64,
    strings: HashMap<u64, String>,
    class_names: HashMap<u64, u64>,
    classes: HashMap<u64, ClassInfo>,
    instances: HashMap<u64, (u64, u64)>,
    obj_arrays: HashMap<u64, (u64, u64)>,
    prim_arrays: [(u64, u64); 12],
    consumed: u64,
}

impl Default for HprofParser {
    fn default() -> Self {
        Self::new()
    }
}

enum Step {
    Done,
    NeedMore,
}

impl HprofParser {
    pub fn new() -> HprofParser {
        HprofParser {
            buf: Vec::with_capacity(4 << 20),
            pos: 0,
            state: State::Header,
            skip: 0,
            heap_rem: 0,
            id_size: 8,
            timestamp: 0,
            strings: HashMap::default(),
            class_names: HashMap::default(),
            classes: HashMap::default(),
            instances: HashMap::default(),
            obj_arrays: HashMap::default(),
            prim_arrays: [(0, 0); 12],
            consumed: 0,
        }
    }

    /// Bytes of the (uncompressed) dump processed so far.
    pub fn consumed(&self) -> u64 {
        self.consumed
    }

    pub fn feed(&mut self, mut chunk: &[u8]) -> Result<()> {
        if self.pos >= self.buf.len() {
            self.buf.clear();
            self.pos = 0;
            if self.skip > 0 {
                let k = self.skip.min(chunk.len() as u64) as usize;
                self.skip -= k as u64;
                self.consumed += k as u64;
                chunk = &chunk[k..];
            }
        }
        if chunk.is_empty() {
            return Ok(());
        }
        if self.pos > COMPACT_AT {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        self.buf.extend_from_slice(chunk);
        loop {
            if self.skip > 0 {
                let avail = self.buf.len() - self.pos;
                let k = self.skip.min(avail as u64) as usize;
                self.pos += k;
                self.consumed += k as u64;
                self.skip -= k as u64;
                if self.skip > 0 {
                    break;
                }
            }
            match self.step()? {
                Step::Done => {}
                Step::NeedMore => break,
            }
        }
        Ok(())
    }

    fn avail(&self) -> &[u8] {
        &self.buf[self.pos..]
    }

    fn advance(&mut self, n: usize) {
        self.pos += n;
        self.consumed += n as u64;
    }

    fn id_at(&self, b: &[u8], at: usize) -> u64 {
        if self.id_size == 4 {
            u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as u64
        } else {
            u64::from_be_bytes([
                b[at],
                b[at + 1],
                b[at + 2],
                b[at + 3],
                b[at + 4],
                b[at + 5],
                b[at + 6],
                b[at + 7],
            ])
        }
    }

    fn type_size(&self, t: u8) -> Result<usize> {
        Ok(match t {
            2 => self.id_size,
            4 | 8 => 1,
            5 | 9 => 2,
            6 | 10 => 4,
            7 | 11 => 8,
            _ => bail!("heap dump: tipo di campo sconosciuto {t}"),
        })
    }

    fn step(&mut self) -> Result<Step> {
        match self.state {
            State::Header => self.header(),
            State::Top => self.top(),
            State::Heap => self.heap(),
        }
    }

    fn header(&mut self) -> Result<Step> {
        let b = self.avail();
        if b.len() < MAGIC.len() {
            return Ok(Step::NeedMore);
        }
        if !b.starts_with(MAGIC) {
            bail!(
                "{}",
                crate::i18n::t("non è un heap dump HPROF", "this is not an HPROF heap dump")
            );
        }
        let Some(nul) = b.iter().take(64).position(|c| *c == 0) else {
            if b.len() >= 64 {
                bail!("heap dump: intestazione non valida");
            }
            return Ok(Step::NeedMore);
        };
        if b.len() < nul + 1 + 12 {
            return Ok(Step::NeedMore);
        }
        let id = u32::from_be_bytes([b[nul + 1], b[nul + 2], b[nul + 3], b[nul + 4]]) as usize;
        if id != 4 && id != 8 {
            bail!("heap dump: dimensione degli id non supportata ({id})");
        }
        let hi = u32::from_be_bytes([b[nul + 5], b[nul + 6], b[nul + 7], b[nul + 8]]) as u64;
        let lo = u32::from_be_bytes([b[nul + 9], b[nul + 10], b[nul + 11], b[nul + 12]]) as u64;
        self.id_size = id;
        self.timestamp = ((hi << 32) | lo) as i64;
        self.advance(nul + 13);
        self.state = State::Top;
        Ok(Step::Done)
    }

    fn top(&mut self) -> Result<Step> {
        let b = self.avail();
        if b.len() < 9 {
            return Ok(Step::NeedMore);
        }
        let tag = b[0];
        let len = u32::from_be_bytes([b[5], b[6], b[7], b[8]]) as usize;
        match tag {
            0x01 | 0x02 => {
                if b.len() < 9 + len {
                    return Ok(Step::NeedMore);
                }
                let body = &b[9..9 + len];
                let ids = self.id_size;
                if tag == 0x01 {
                    if len >= ids {
                        let id = self.id_at(body, 0);
                        let s = String::from_utf8_lossy(&body[ids..]).into_owned();
                        self.strings.insert(id, s);
                    }
                } else if len >= 8 + 2 * ids {
                    let class_id = self.id_at(body, 4);
                    let name_id = self.id_at(body, 8 + ids);
                    self.class_names.insert(class_id, name_id);
                }
                self.advance(9 + len);
            }
            0x0C | 0x1C => {
                self.advance(9);
                self.heap_rem = len as u64;
                self.state = State::Heap;
            }
            _ => {
                self.advance(9);
                self.skip = len as u64;
            }
        }
        Ok(Step::Done)
    }

    fn heap(&mut self) -> Result<Step> {
        if self.heap_rem == 0 {
            self.state = State::Top;
            return Ok(Step::Done);
        }
        let ids = self.id_size;
        let buf = std::mem::take(&mut self.buf);
        let r = self.heap_record(&buf[self.pos..], ids);
        self.buf = buf;
        let (head, data) = match r? {
            Some(x) => x,
            None => return Ok(Step::NeedMore),
        };
        let total = head as u64 + data;
        if total > self.heap_rem {
            bail!("heap dump danneggiato o incompleto");
        }
        self.heap_rem -= total;
        self.advance(head);
        self.skip = data;
        Ok(Step::Done)
    }

    fn heap_record(&mut self, b: &[u8], ids: usize) -> Result<Option<(usize, u64)>> {
        if b.is_empty() {
            return Ok(None);
        }
        let sub = b[0];
        let (head, data) = match sub {
            0xFF | 0x05 | 0x07 | 0x89 | 0x8A | 0x8B | 0x8C | 0x8D | 0x90 => (1 + ids, 0u64),
            0x01 => (1 + 2 * ids, 0),
            0x02 | 0x03 | 0x08 | 0x8E => (1 + ids + 8, 0),
            0x04 | 0x06 => (1 + ids + 4, 0),
            0xFE => (1 + 4 + ids, 0),
            0x21 => {
                let h = 1 + 2 * ids + 8;
                if b.len() < h {
                    return Ok(None);
                }
                let class_id = self.id_at(b, 1 + ids + 4);
                let n = be32(b, h - 4);
                let e = self.instances.entry(class_id).or_insert((0, 0));
                e.0 += 1;
                e.1 += n;
                (h, n)
            }
            0x22 => {
                let h = 1 + 2 * ids + 8;
                if b.len() < h {
                    return Ok(None);
                }
                let n = be32(b, 1 + ids + 4);
                let class_id = self.id_at(b, 1 + ids + 8);
                let e = self.obj_arrays.entry(class_id).or_insert((0, 0));
                e.0 += 1;
                e.1 += align8(16 + n * 4);
                (h, n * ids as u64)
            }
            0x23 | 0xC3 => {
                let h = 1 + ids + 9;
                if b.len() < h {
                    return Ok(None);
                }
                let n = be32(b, 1 + ids + 4);
                let t = b[1 + ids + 8];
                let es = self.type_size(t)? as u64;
                if let Some(slot) = self.prim_arrays.get_mut(t as usize) {
                    slot.0 += 1;
                    slot.1 += align8(16 + n * es);
                }
                (h, if sub == 0x23 { n * es } else { 0 })
            }
            0x20 => match self.class_dump(b)? {
                Some((h, info, class_id)) => {
                    self.classes.insert(class_id, info);
                    (h, 0)
                }
                None => return Ok(None),
            },
            other => bail!(
                "heap dump: record sconosciuto 0x{other:02x} dopo {} byte",
                self.consumed
            ),
        };
        if b.len() < head {
            return Ok(None);
        }
        Ok(Some((head, data)))
    }

    fn class_dump(&self, b: &[u8]) -> Result<Option<(usize, ClassInfo, u64)>> {
        let ids = self.id_size;
        let mut at = 1 + 7 * ids + 8;
        if b.len() < at + 2 {
            return Ok(None);
        }
        let class_id = self.id_at(b, 1);
        let super_id = self.id_at(b, 1 + ids + 4);
        let u2 = |b: &[u8], at: usize| u16::from_be_bytes([b[at], b[at + 1]]) as usize;
        let consts = u2(b, at);
        at += 2;
        for _ in 0..consts {
            if b.len() < at + 3 {
                return Ok(None);
            }
            at += 3 + self.type_size(b[at + 2])?;
        }
        if b.len() < at + 2 {
            return Ok(None);
        }
        let statics = u2(b, at);
        at += 2;
        for _ in 0..statics {
            if b.len() < at + ids + 1 {
                return Ok(None);
            }
            at += ids + 1 + self.type_size(b[at + ids])?;
        }
        if b.len() < at + 2 {
            return Ok(None);
        }
        let fields = u2(b, at);
        at += 2;
        let mut own_refs = 0u32;
        for _ in 0..fields {
            if b.len() < at + ids + 1 {
                return Ok(None);
            }
            if b[at + ids] == 2 {
                own_refs += 1;
            }
            at += ids + 1;
        }
        if b.len() < at {
            return Ok(None);
        }
        Ok(Some((at, ClassInfo { super_id, own_refs }, class_id)))
    }

    fn name(&self, class_id: u64) -> Option<String> {
        let sid = self.class_names.get(&class_id)?;
        self.strings.get(sid).map(|s| java_name(s))
    }

    fn total_refs(&self, mut class_id: u64) -> u64 {
        let mut refs = 0u64;
        for _ in 0..64 {
            let Some(c) = self.classes.get(&class_id) else {
                break;
            };
            refs += c.own_refs as u64;
            if c.super_id == 0 {
                break;
            }
            class_id = c.super_id;
        }
        refs
    }

    pub fn finish(self) -> Result<HeapData> {
        if self.state == State::Header {
            bail!(
                "{}",
                crate::i18n::t(
                    "heap dump vuoto o incompleto",
                    "empty or incomplete heap dump"
                )
            );
        }
        if self.skip > 0 || self.heap_rem > 0 || self.pos < self.buf.len() {
            bail!(
                "{}",
                crate::i18n::t(
                    "heap dump incompleto: il file sembra tagliato",
                    "incomplete heap dump: the file looks truncated"
                )
            );
        }
        let ids = self.id_size as u64;
        let (header, ref_size) = if ids == 8 { (12u64, 4u64) } else { (8, 4) };
        let mut rows: Vec<(String, u64, u64)> = Vec::new();
        let mut merged: HashMap<String, usize> = HashMap::default();
        let mut push =
            |name: String, count: u64, size: u64, rows: &mut Vec<(String, u64, u64)>| match merged
                .get(&name)
            {
                Some(i) => {
                    rows[*i].1 += count;
                    rows[*i].2 += size;
                }
                None => {
                    merged.insert(name.clone(), rows.len());
                    rows.push((name, count, size));
                }
            };
        for (cid, (count, bytes)) in &self.instances {
            let name = self
                .name(*cid)
                .unwrap_or_else(|| format!("unknown@{cid:x}"));
            let per = bytes / (*count).max(1);
            let refs = self.total_refs(*cid);
            let compact = per.saturating_sub(refs * (ids - ref_size));
            let size = align8(header + compact) * count;
            push(name, *count, size, &mut rows);
        }
        for (cid, (count, size)) in &self.obj_arrays {
            let name = self
                .name(*cid)
                .unwrap_or_else(|| "java.lang.Object[]".into());
            push(name, *count, *size, &mut rows);
        }
        for (t, (count, size)) in self.prim_arrays.iter().enumerate() {
            if *count > 0 {
                push(
                    format!("{}[]", prim_name(t as u8)),
                    *count,
                    *size,
                    &mut rows,
                );
            }
        }
        rows.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
        let entries = rows
            .into_iter()
            .enumerate()
            .map(|(i, (name, count, size))| HeapEntry {
                order: i as i32 + 1,
                instances: count.min(i32::MAX as u64) as i32,
                size: size as i64,
                r#type: name,
            })
            .collect();
        Ok(HeapData {
            metadata: Some(HeapMetadata {
                generated_time: self.timestamp,
                ..Default::default()
            }),
            entries,
        })
    }
}

impl Write for HprofParser {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.feed(buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn be32(b: &[u8], at: usize) -> u64 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as u64
}

fn align8(n: u64) -> u64 {
    (n + 7) & !7
}

fn prim_name(t: u8) -> &'static str {
    match t {
        4 => "boolean",
        5 => "char",
        6 => "float",
        7 => "double",
        8 => "byte",
        9 => "short",
        10 => "int",
        11 => "long",
        _ => "unknown",
    }
}

/// `java/lang/String` → `java.lang.String`, `[Ljava/lang/Object;` → `java.lang.Object[]`,
/// `[[I` → `int[][]`.
pub fn java_name(raw: &str) -> String {
    let dims = raw.chars().take_while(|c| *c == '[').count();
    let rest = &raw[dims..];
    let base = if dims == 0 {
        rest.replace('/', ".")
    } else if let Some(c) = rest.strip_prefix('L') {
        c.trim_end_matches(';').replace('/', ".")
    } else {
        match rest {
            "Z" => "boolean",
            "C" => "char",
            "F" => "float",
            "D" => "double",
            "B" => "byte",
            "S" => "short",
            "I" => "int",
            "J" => "long",
            other => other,
        }
        .to_string()
    };
    let mut out = base;
    for _ in 0..dims {
        out.push_str("[]");
    }
    out
}

/// Plain or gzip-compressed dump, detected from the first bytes.
pub struct HprofStream {
    inner: Option<Inner>,
    pending: Vec<u8>,
}

enum Inner {
    Plain(Box<HprofParser>),
    Gz(Box<flate2::write::MultiGzDecoder<HprofParser>>),
}

impl Default for HprofStream {
    fn default() -> Self {
        Self::new()
    }
}

impl HprofStream {
    pub fn new() -> HprofStream {
        HprofStream {
            inner: None,
            pending: Vec::new(),
        }
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Result<()> {
        if self.inner.is_none() {
            self.pending.extend_from_slice(chunk);
            if self.pending.len() < 2 {
                return Ok(());
            }
            let gz = self.pending[0] == 0x1f && self.pending[1] == 0x8b;
            self.inner = Some(if gz {
                Inner::Gz(Box::new(flate2::write::MultiGzDecoder::new(
                    HprofParser::new(),
                )))
            } else {
                Inner::Plain(Box::default())
            });
            let first = std::mem::take(&mut self.pending);
            return self.feed(&first);
        }
        match self.inner.as_mut() {
            Some(Inner::Plain(p)) => p.feed(chunk),
            Some(Inner::Gz(g)) => g.write_all(chunk).map_err(|e| anyhow!("{e}")),
            None => Ok(()),
        }
    }

    pub fn finish(mut self) -> Result<HeapData> {
        if self.inner.is_none() {
            let first = std::mem::take(&mut self.pending);
            let mut p = HprofParser::new();
            p.feed(&first)?;
            return p.finish();
        }
        match self.inner.take() {
            Some(Inner::Plain(p)) => p.finish(),
            Some(Inner::Gz(g)) => g
                .finish()
                .map_err(|e| anyhow!("{}: {e}", crate::i18n::t("gzip non valido", "invalid gzip")))?
                .finish(),
            None => bail!("heap dump vuoto"),
        }
    }
}

/// Converts a whole dump held in memory (small files, tests, the CLI).
pub fn parse_bytes(bytes: &[u8]) -> Result<HeapData> {
    let mut s = HprofStream::new();
    for c in bytes.chunks(8 << 20) {
        s.feed(c)?;
    }
    s.finish()
}

/// Converts a dump read from any reader (a file on disk, an HTTP upload) without
/// keeping it in memory.
pub fn parse_reader(r: &mut dyn std::io::Read, mut progress: impl FnMut(u64)) -> Result<HeapData> {
    let mut s = HprofStream::new();
    let mut buf = vec![0u8; 8 << 20];
    let mut total = 0u64;
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        s.feed(&buf[..n])?;
        progress(total);
    }
    s.finish()
}
