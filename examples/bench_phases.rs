use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("percorso del profilo");
    let bytes = std::fs::read(&path).expect("lettura");
    let t = Instant::now();
    let loaded = sparkscope::loader::load_bytes(&bytes, &path).expect("decode");
    let decode = t.elapsed();
    let sparkscope::loader::Payload::Sampler(data) = loaded.payload else {
        panic!("non è un sampler")
    };
    let t = Instant::now();
    let p = sparkscope::model::Profile::from_proto(*data);
    let model = t.elapsed();
    let t = Instant::now();
    let a = sparkscope::analysis::analyze(&p);
    let analysis = t.elapsed();
    let t = Instant::now();
    let r = sparkscope::report::build(&p, a, "x", None, "profilo");
    let rules = t.elapsed();
    let t = Instant::now();
    let html = sparkscope::webui::render_report(&r, None).html;
    let render = t.elapsed();
    eprintln!(
        "decode {:?} · modello {:?} · analisi {:?} · regole {:?} · html {:?} ({:.1} MB)",
        decode,
        model,
        analysis,
        rules,
        render,
        html.len() as f64 / 1e6
    );
}
