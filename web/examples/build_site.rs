use sparkscope::i18n::{self, Lang};

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "site".into());
    std::fs::create_dir_all(&dir).expect("cartella non creabile");
    for lang in [Lang::It, Lang::En] {
        let _guard = i18n::scoped(lang);
        let name = sparkscope::webui::static_page_name(lang);
        std::fs::write(format!("{dir}/{name}"), sparkscope::webui::static_index())
            .expect("scrittura fallita");
    }
    std::fs::write(format!("{dir}/.nojekyll"), "").expect("scrittura fallita");
    eprintln!("sito scritto in {dir}/");
}
