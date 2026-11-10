#[path = "../tests/support/fixture.rs"]
mod fixture;

use prost::Message;
use sparkscope::proto::{HeapData, HeapEntry};

fn entries(n: usize, grow: f64) -> Vec<HeapEntry> {
    let pkgs = [
        "net.minecraft.world.level.chunk",
        "net.minecraft.world.entity",
        "net.minecraft.server.level",
        "com.example.crates.data",
        "net.example.hololines",
        "me.lucko.luckperms.common.node",
        "com.zaxxer.hikari.pool",
        "io.netty.buffer",
        "org.bukkit.craftbukkit.entity",
        "com.example.shop.cache",
    ];
    (0..n)
        .map(|i| {
            let p = pkgs[i % pkgs.len()];
            let ty = if i % 7 == 0 {
                format!("{p}.Class{i}$$Lambda/0x00007f3a{i:08x}")
            } else {
                format!("{p}.Class{i}")
            };
            let base = 1_000_000.0 / (1.0 + i as f64).powf(0.9);
            let f = if p.starts_with("com.example.crates") {
                grow
            } else {
                1.0
            };
            let inst = (base * f).max(1.0) as i32;
            HeapEntry {
                order: i as i32,
                instances: inst,
                size: inst as i64 * (16 + (i % 9) as i64 * 8),
                r#type: ty,
            }
        })
        .collect()
}

fn main() {
    let mut a = std::env::args().skip(1);
    let n: usize = a.next().and_then(|v| v.parse().ok()).unwrap_or(150_000);
    let out = a.next().unwrap_or_else(|| "big".into());
    let (b0, b1) = fixture::sample_heap_pair();
    for (name, base, grow) in [("prima", b0, 1.0), ("dopo", b1, 1.6)] {
        let h = HeapData {
            metadata: base.metadata,
            entries: entries(n, grow),
        };
        let path = format!("{out}-{name}.sparkheap");
        std::fs::write(&path, h.encode_to_vec()).expect("scrittura fallita");
        eprintln!("scritto {path}");
    }
}
