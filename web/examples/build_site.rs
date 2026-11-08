use sparkscope::i18n::{self, Lang};

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "site".into());
    std::fs::create_dir_all(&dir).expect("cartella non creabile");
    let version = std::env::var("GITHUB_SHA")
        .map(|sha| sha.chars().take(12).collect::<String>())
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").replace('.', ""));
    for lang in [Lang::It, Lang::En] {
        let _guard = i18n::scoped(lang);
        let name = sparkscope::webui::static_page_name(lang);
        std::fs::write(
            format!("{dir}/{name}"),
            sparkscope::webui::static_index_versioned(&version),
        )
        .expect("scrittura fallita");
    }
    // old English page: keep existing links working, query and hash included
    std::fs::write(
        format!("{dir}/en.html"),
        r#"<!doctype html><meta charset="utf-8"><title>Spark Resource Analyzer</title><meta http-equiv="refresh" content="0; url=./"><link rel="canonical" href="./"><script>location.replace('./'+location.search+location.hash)</script><a href="./">Spark Resource Analyzer</a>"#,
    )
    .expect("scrittura fallita");
    std::fs::write(format!("{dir}/.nojekyll"), "").expect("scrittura fallita");
    let assets = concat!(env!("CARGO_MANIFEST_DIR"), "/static");
    if let Ok(entries) = std::fs::read_dir(assets) {
        for entry in entries.flatten() {
            let target = format!("{dir}/{}", entry.file_name().to_string_lossy());
            std::fs::copy(entry.path(), target).expect("copia dei file statici fallita");
        }
    }
    eprintln!("sito scritto in {dir}/");
}
