use std::sync::OnceLock;

const GLUE_PATH: &str = "/pkg/sparkscope_web.js";
const MODULE_PATH: &str = "/pkg/sparkscope_web_bg.wasm";
const WORKER_PATH: &str = "/sa-worker.js";
const SCRIPT_TYPE: &str = "text/javascript; charset=utf-8";
const WASM_TYPE: &str = "application/wasm";

static EMBEDDED_GLUE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sa_web.js"));
static EMBEDDED_MODULE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sa_web.wasm"));

pub struct WasmBundle {
    glue: &'static [u8],
    module: &'static [u8],
    query: String,
    worker: String,
    glue_gzip: OnceLock<Vec<u8>>,
    module_gzip: OnceLock<Vec<u8>>,
}

pub struct Asset<'a> {
    pub body: &'a [u8],
    pub content_type: &'static str,
}

impl WasmBundle {
    pub fn new(glue: &'static [u8], module: &'static [u8]) -> WasmBundle {
        let available = !glue.is_empty() && !module.is_empty();
        let version = if available {
            format!("{}{:016x}", env!("CARGO_PKG_VERSION"), fnv1a(module))
        } else {
            String::new()
        };
        WasmBundle {
            glue,
            module,
            query: crate::webui::version_suffix(&version),
            worker: crate::webui::static_worker_versioned(&version),
            glue_gzip: OnceLock::new(),
            module_gzip: OnceLock::new(),
        }
    }

    pub fn embedded() -> &'static WasmBundle {
        static BUNDLE: OnceLock<WasmBundle> = OnceLock::new();
        BUNDLE.get_or_init(|| WasmBundle::new(EMBEDDED_GLUE, EMBEDDED_MODULE))
    }

    pub fn is_available(&self) -> bool {
        !self.query.is_empty()
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn asset(&self, path: &str) -> Option<Asset<'_>> {
        if !self.is_available() {
            return None;
        }
        let (body, content_type): (&[u8], _) = match path {
            GLUE_PATH => (self.glue, SCRIPT_TYPE),
            MODULE_PATH => (self.module, WASM_TYPE),
            WORKER_PATH => (self.worker.as_bytes(), SCRIPT_TYPE),
            _ => return None,
        };
        Some(Asset { body, content_type })
    }

    pub fn asset_gzip(&self, path: &str) -> Option<Asset<'_>> {
        let cached = match path {
            GLUE_PATH => &self.glue_gzip,
            MODULE_PATH => &self.module_gzip,
            _ => return None,
        };
        let plain = self.asset(path)?;
        let body = cached.get_or_init(|| compress(plain.body));
        Some(Asset {
            body,
            content_type: plain.content_type,
        })
    }
}

fn compress(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(
        Vec::with_capacity(bytes.len() / 3),
        flate2::Compression::best(),
    );
    std::io::Write::write_all(&mut encoder, bytes).expect("writing to a Vec cannot fail");
    encoder.finish().expect("writing to a Vec cannot fail")
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::WasmBundle;

    fn bundle() -> WasmBundle {
        WasmBundle::new(b"export default 1;", b"\0asm\x01\0\0\0")
    }

    #[test]
    fn serves_glue_module_and_worker_with_their_types() {
        let b = bundle();
        assert!(b.is_available());
        let glue = b.asset("/pkg/sparkscope_web.js").unwrap();
        assert_eq!(glue.content_type, "text/javascript; charset=utf-8");
        assert_eq!(glue.body, b"export default 1;");
        let module = b.asset("/pkg/sparkscope_web_bg.wasm").unwrap();
        assert_eq!(module.content_type, "application/wasm");
        assert_eq!(module.body, b"\0asm\x01\0\0\0");
        let worker = b.asset("/sa-worker.js").unwrap();
        let text = String::from_utf8(worker.body.to_vec()).unwrap();
        assert!(text.contains(&format!("./pkg/sparkscope_web.js{}", b.query())));
        assert!(b.asset("/pkg/other.js").is_none());
    }

    #[test]
    fn version_changes_with_the_module() {
        let a = WasmBundle::new(b"x", b"one");
        let b = WasmBundle::new(b"x", b"two");
        assert!(a.query().starts_with("?v="));
        assert_ne!(a.query(), b.query());
    }

    #[test]
    fn empty_bundle_serves_nothing() {
        let b = WasmBundle::new(b"", b"");
        assert!(!b.is_available());
        assert!(b.query().is_empty());
        for p in [
            "/sa-worker.js",
            "/pkg/sparkscope_web.js",
            "/pkg/sparkscope_web_bg.wasm",
        ] {
            assert!(b.asset(p).is_none(), "{p}");
        }
    }

    #[test]
    fn compressed_body_is_computed_once_and_decodes_to_the_original() {
        let b = bundle();
        let first = b.asset_gzip("/pkg/sparkscope_web_bg.wasm").unwrap();
        let second = b.asset_gzip("/pkg/sparkscope_web_bg.wasm").unwrap();
        assert!(std::ptr::eq(first.body.as_ptr(), second.body.as_ptr()));
        let mut plain = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(first.body), &mut plain)
            .unwrap();
        assert_eq!(plain, b"\0asm\x01\0\0\0");
    }
}
