//! Writes small but valid HotSpot heap dumps for tests and benchmarks.
#![allow(dead_code)]

pub struct Gen {
    out: Vec<u8>,
    seg: Vec<u8>,
    next_id: u64,
}

pub const PLAYER_DATA: &str = "com/example/crates/data/PlayerCrateData";

impl Gen {
    pub fn new() -> Gen {
        let mut out = b"JAVA PROFILE 1.0.2\0".to_vec();
        out.extend_from_slice(&8u32.to_be_bytes());
        out.extend_from_slice(&1_762_423_200_000u64.to_be_bytes());
        Gen {
            out,
            seg: Vec::new(),
            next_id: 0x1000,
        }
    }

    fn id(&mut self) -> u64 {
        self.next_id += 16;
        self.next_id
    }

    fn record(&mut self, tag: u8, body: &[u8]) {
        self.out.push(tag);
        self.out.extend_from_slice(&0u32.to_be_bytes());
        self.out
            .extend_from_slice(&(body.len() as u32).to_be_bytes());
        self.out.extend_from_slice(body);
    }

    /// Declares a class (name in JVM form) with `refs` reference fields and
    /// `prims` int fields; returns its class object id.
    pub fn class(&mut self, name: &str, super_id: u64, refs: u16, prims: u16) -> u64 {
        self.class_in(name, super_id, 0, refs, prims)
    }

    /// Like [`Gen::class`], with the class loader object that defined the class.
    pub fn class_in(
        &mut self,
        name: &str,
        super_id: u64,
        loader: u64,
        refs: u16,
        prims: u16,
    ) -> u64 {
        let sid = self.id();
        let mut b = sid.to_be_bytes().to_vec();
        b.extend_from_slice(name.as_bytes());
        self.record(0x01, &b);
        let cid = self.id();
        let mut b = 1u32.to_be_bytes().to_vec();
        b.extend_from_slice(&cid.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&sid.to_be_bytes());
        self.record(0x02, &b);
        let fname = self.id();
        let mut d = vec![0x20];
        d.extend_from_slice(&cid.to_be_bytes());
        d.extend_from_slice(&0u32.to_be_bytes());
        d.extend_from_slice(&super_id.to_be_bytes());
        d.extend_from_slice(&loader.to_be_bytes());
        for _ in 0..4 {
            d.extend_from_slice(&0u64.to_be_bytes());
        }
        d.extend_from_slice(&((refs as u32) * 8 + (prims as u32) * 4).to_be_bytes());
        d.extend_from_slice(&1u16.to_be_bytes());
        d.extend_from_slice(&7u16.to_be_bytes());
        d.push(10);
        d.extend_from_slice(&42u32.to_be_bytes());
        d.extend_from_slice(&1u16.to_be_bytes());
        d.extend_from_slice(&fname.to_be_bytes());
        d.push(2);
        d.extend_from_slice(&0u64.to_be_bytes());
        d.extend_from_slice(&(refs + prims).to_be_bytes());
        for i in 0..refs + prims {
            d.extend_from_slice(&fname.to_be_bytes());
            d.push(if i < refs { 2 } else { 10 });
        }
        self.seg.extend_from_slice(&d);
        cid
    }

    pub fn instances(&mut self, class_id: u64, count: usize, bytes: u32) {
        for _ in 0..count {
            let oid = self.id();
            self.instance_with_id(oid, class_id, bytes);
        }
    }

    pub fn new_id(&mut self) -> u64 {
        self.id()
    }

    pub fn instance_with_id(&mut self, oid: u64, class_id: u64, bytes: u32) {
        {
            self.seg.push(0x21);
            self.seg.extend_from_slice(&oid.to_be_bytes());
            self.seg.extend_from_slice(&0u32.to_be_bytes());
            self.seg.extend_from_slice(&class_id.to_be_bytes());
            self.seg.extend_from_slice(&bytes.to_be_bytes());
            self.seg.extend(std::iter::repeat_n(0u8, bytes as usize));
        }
    }

    pub fn byte_arrays(&mut self, count: usize, len: u32) {
        for _ in 0..count {
            let oid = self.id();
            self.seg.push(0x23);
            self.seg.extend_from_slice(&oid.to_be_bytes());
            self.seg.extend_from_slice(&0u32.to_be_bytes());
            self.seg.extend_from_slice(&len.to_be_bytes());
            self.seg.push(8);
            self.seg.extend(std::iter::repeat_n(7u8, len as usize));
        }
    }

    pub fn object_arrays(&mut self, array_class: u64, count: usize, len: u32) {
        for _ in 0..count {
            let oid = self.id();
            self.seg.push(0x22);
            self.seg.extend_from_slice(&oid.to_be_bytes());
            self.seg.extend_from_slice(&0u32.to_be_bytes());
            self.seg.extend_from_slice(&len.to_be_bytes());
            self.seg.extend_from_slice(&array_class.to_be_bytes());
            self.seg.extend(std::iter::repeat_n(0u8, len as usize * 8));
        }
    }

    pub fn root(&mut self) {
        let oid = self.id();
        self.seg.push(0xFF);
        self.seg.extend_from_slice(&oid.to_be_bytes());
    }

    /// Closes the current heap dump segment.
    pub fn segment(&mut self) {
        let seg = std::mem::take(&mut self.seg);
        self.record(0x1C, &seg);
    }

    pub fn finish(mut self) -> Vec<u8> {
        if !self.seg.is_empty() {
            self.segment();
        }
        self.record(0x2C, &[]);
        self.out
    }
}

/// A dump with a leaking plugin class, JDK objects, arrays and two segments.
pub fn sample(players: usize) -> Vec<u8> {
    let mut g = Gen::new();
    let object = g.class("java/lang/Object", 0, 0, 0);
    let string = g.class("java/lang/String", object, 1, 2);
    let data = g.class(PLAYER_DATA, object, 3, 2);
    let arr = g.class("[Ljava/lang/Object;", object, 0, 0);
    g.root();
    g.instances(string, 500, 16);
    g.segment();
    g.instances(data, players * 40, 32);
    g.byte_arrays(300, 100);
    g.object_arrays(arr, 20, 10);
    g.finish()
}
