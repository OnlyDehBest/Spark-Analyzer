#[path = "../tests/support/hprof_gen.rs"]
mod hprof_gen;

use std::io::Write;

fn main() {
    let mut a = std::env::args().skip(1);
    let out = a.next().unwrap_or_else(|| "big.hprof".into());
    let mb: usize = a.next().and_then(|v| v.parse().ok()).unwrap_or(1024);
    let mut f = std::io::BufWriter::new(std::fs::File::create(&out).unwrap());
    let mut g = hprof_gen::Gen::new();
    let object = g.class("java/lang/Object", 0, 0, 0);
    let mut classes = Vec::new();
    for i in 0..3000 {
        let pkg = [
            "com/example/crates/data",
            "net/minecraft/world/entity",
            "java/util",
            "me/lucko/luckperms/common",
        ][i % 4];
        classes.push(g.class(
            &format!("{pkg}/Class{i}"),
            object,
            (i % 5) as u16,
            (i % 4) as u16,
        ));
    }
    let arr = g.class("[Ljava/lang/Object;", object, 0, 0);
    let first = g.finish();
    f.write_all(&first[..first.len() - 9]).unwrap();
    let mut written = first.len();
    let mut k = 0usize;
    while written < mb << 20 {
        let mut g = hprof_gen::Gen::new();
        for _ in 0..2000 {
            let c = classes[(k * 7919) % classes.len()];
            g.instances(c, 20, 24 + (k % 3) as u32 * 8);
            k += 1;
        }
        g.byte_arrays(200, 64);
        g.object_arrays(arr, 50, 8);
        let seg = g.finish();
        let body = &seg[31..seg.len() - 9];
        f.write_all(body).unwrap();
        written += body.len();
    }
    f.write_all(&[0x2C, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap();
    eprintln!("scritto {out}: {} MB", written >> 20);
}
