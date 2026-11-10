use std::time::Instant;

fn main() {
    let mut a = std::env::args().skip(1);
    let pa = a.next().expect("file prima");
    let pb = a.next();
    let ba = std::fs::read(&pa).expect("lettura");
    let t = Instant::now();
    let loaded = sparkscope::loader::load_bytes(&ba, &pa).expect("decode");
    let t_dec = t.elapsed();
    let t = Instant::now();
    let (r, _, h) = sparkscope::pipeline::build(loaded, None).expect("build");
    let t_build = t.elapsed();
    let t = Instant::now();
    let html = sparkscope::webui::render_report(&r, h.as_ref());
    let t_html = t.elapsed();
    eprintln!(
        "singolo: {:.1} MB · decode {t_dec:?} · analisi {t_build:?} · html {t_html:?} ({:.1} MB)",
        ba.len() as f64 / 1048576.0,
        html.html.len() as f64 / 1048576.0
    );
    if let Some(pb) = pb {
        let bb = std::fs::read(&pb).expect("lettura");
        let t = Instant::now();
        let c = sparkscope::heapdiff::compare_bytes(&ba, &pa, &bb, &pb).expect("compare");
        let t_cmp = t.elapsed();
        let t = Instant::now();
        let out = sparkscope::webui::render_comparison(&c);
        eprintln!(
            "confronto: {t_cmp:?} · html {:?} ({:.1} MB)",
            t.elapsed(),
            out.html.len() as f64 / 1048576.0
        );
        let t = Instant::now();
        let _ = sparkscope::heapdiff::describe_bytes(&ba, &pa).expect("describe");
        eprintln!("describe: {:?}", t.elapsed());
    }
}
