fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "site".into());
    std::fs::create_dir_all(&dir).expect("cartella non creabile");
    std::fs::write(
        format!("{dir}/index.html"),
        sparkscope::webui::static_index(),
    )
    .expect("scrittura fallita");
    std::fs::write(format!("{dir}/.nojekyll"), "").expect("scrittura fallita");
    eprintln!("sito scritto in {dir}/");
}
