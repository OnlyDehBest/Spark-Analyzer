use std::time::Instant;

fn main() {
    let p = std::env::args().nth(1).expect("file .hprof");
    let t = Instant::now();
    let mut f = std::fs::File::open(&p).unwrap();
    let size = f.metadata().unwrap().len();
    let h = sparkscope::hprof::parse_reader(&mut f, |_| {}).expect("parse");
    let el = t.elapsed();
    eprintln!(
        "{} MB in {el:?} ({:.0} MB/s), {} classi",
        size >> 20,
        size as f64 / 1048576.0 / el.as_secs_f64(),
        h.entries.len()
    );
    let t = Instant::now();
    let r = sparkscope::heap::analyze(&h);
    eprintln!(
        "analisi heap {:?}, top {}",
        t.elapsed(),
        r.top.first().map(|x| x.kind.as_str()).unwrap_or("")
    );
}
