#[path = "../../tests/support/fixture.rs"]
mod fixture;

use prost::Message;

#[test]
fn analyze_produces_standalone_report() {
    let bytes = fixture::sample().encode_to_vec();
    let json = sparkscope_web::analyze_to_json(
        &bytes,
        "AbCd1234",
        Some("AbCd1234".into()),
        "/Spark-Analyzer/",
        "it",
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let html = v["html"].as_str().unwrap();
    assert!(html.contains("Nuova analisi") && !html.contains("id=\"dlh\""));
    assert!(html.contains("href=\"/Spark-Analyzer/\""));
    assert!(html.contains("https://spark.lucko.me/AbCd1234"));
    assert_eq!(v["bad"], true);
    assert!(sparkscope_web::analyze_to_json(b"garbage", "x.txt", None, "/", "it").is_err());
}

#[test]
fn static_index_loads_wasm_module() {
    let page = sparkscope::webui::static_index();
    assert!(page.contains("./pkg/sparkscope_web.js") && page.contains("type=\"module\""));
}

#[test]
fn english_report_has_no_italian_chrome() {
    let bytes = fixture::sample().encode_to_vec();
    let json =
        sparkscope_web::analyze_to_json(&bytes, "AbCd1234", Some("AbCd1234".into()), "/", "en")
            .unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let html = v["html"].as_str().unwrap();
    assert!(html.contains("<html lang=\"en\""));
    for it in [
        "Diagnosi</h2>",
        "Cosa fare",
        "Responsabili",
        "Nuova analisi",
        "del tempo attivo",
        "consigliato",
        "Panoramica",
        "Mondo</h2>",
        "Thread «",
        "finestre</small>",
        "CRITICO",
    ] {
        assert!(!html.contains(it), "testo italiano rimasto: {it}");
    }
    assert!(
        html.contains("What to do")
            && html.contains("New analysis")
            && html.contains("data-to=\"it\"")
    );
}

#[test]
fn english_controls_and_headers() {
    let bytes = fixture::sample().encode_to_vec();
    let json = sparkscope_web::analyze_to_json(&bytes, "AbCd1234", None, "/", "en").unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let html = v["html"].as_str().unwrap();
    for it in [
        "Mostra i consumi",
        ">Nello stack<",
        ">Proprio<",
        ">Totale<",
        "Metodi per tempo",
    ] {
        assert!(!html.contains(it), "testo italiano rimasto: {it}");
    }
    assert!(
        html.contains("Show usage as") && html.contains(">On stack<") && html.contains(">Own<")
    );
}

#[test]
fn heap_comparison_through_the_web_api() {
    let (before, after) = fixture::sample_heap_pair();
    let a = before.encode_to_vec();
    let b = after.encode_to_vec();
    let info: serde_json::Value =
        serde_json::from_str(&sparkscope_web::describe_heap_json(&a, "a.sparkheap", "it").unwrap())
            .unwrap();
    assert!(info["detail"].as_str().unwrap().contains("38 giocatori"));
    let out: serde_json::Value = serde_json::from_str(
        &sparkscope_web::compare_heaps_json(
            &b,
            "dopo.sparkheap",
            &a,
            "prima.sparkheap",
            "./",
            "en",
        )
        .unwrap(),
    )
    .unwrap();
    let html = out["html"].as_str().unwrap();
    assert!(
        html.contains("Heap comparison")
            && html.contains("Leak confirmed")
            && html.contains("New analysis")
    );
    assert_eq!(out["bad"], true);
    let profile = fixture::sample().encode_to_vec();
    assert!(sparkscope_web::describe_heap_json(&profile, "x.sparkprofile", "it").is_err());
}

#[test]
fn static_index_has_compare_mode() {
    let _g = sparkscope::i18n::scoped(sparkscope::i18n::Lang::It);
    let html = sparkscope::webui::static_index();
    for needle in [
        "Confronta due heap",
        "id=\"cmpgo\"",
        "saSetupCompare",
        "compare_heaps",
        "describe_heap",
    ] {
        assert!(html.contains(needle), "manca {needle}");
    }
}

#[test]
fn home_title_is_spark_resource_analyzer() {
    for lang in [sparkscope::i18n::Lang::It, sparkscope::i18n::Lang::En] {
        let _g = sparkscope::i18n::scoped(lang);
        assert!(
            sparkscope::webui::static_index().contains("<title>Spark Resource Analyzer</title>")
        );
        assert!(
            sparkscope::webui::local_index(&[]).contains("<title>Spark Resource Analyzer</title>")
        );
    }
}

#[test]
fn static_assets_are_versioned() {
    let html = sparkscope::webui::static_index_versioned("abc123");
    assert!(html.contains("./pkg/sparkscope_web.js?v=abc123"));
    assert!(html.contains("./pkg/sparkscope_web_bg.wasm?v=abc123"));
    assert!(!html.contains("__SA_V__"));
    let plain = sparkscope::webui::static_index();
    assert!(plain.contains("'./pkg/sparkscope_web.js'") && !plain.contains("__SA_V__"));
}

#[test]
fn copyright_on_every_page() {
    use prost::Message;
    let footer = "© 2026 <b>onlynelchilling</b> · Spark Resource Analyzer";
    assert!(sparkscope::webui::static_index().contains(footer));
    assert!(sparkscope::webui::local_index(&[]).contains(footer));
    let bytes = fixture::sample().encode_to_vec();
    let json = sparkscope_web::analyze_to_json(&bytes, "a.sparkprofile", None, "./", "it").unwrap();
    assert!(json.contains("onlynelchilling"));
    let (a, b) = fixture::sample_heap_pair();
    let cmp = sparkscope_web::compare_heaps_json(
        &a.encode_to_vec(),
        "a.sparkheap",
        &b.encode_to_vec(),
        "b.sparkheap",
        "./",
        "en",
    )
    .unwrap();
    assert!(cmp.contains("onlynelchilling"));
}

#[test]
fn english_is_the_default_language() {
    use sparkscope::i18n::Lang;
    assert_eq!(Lang::parse(""), Lang::En);
    assert_eq!(Lang::parse("it"), Lang::It);
    assert_eq!(sparkscope::webui::static_page_name(Lang::En), "index.html");
    assert_eq!(sparkscope::webui::static_page_name(Lang::It), "it.html");
}

#[test]
fn site_has_social_preview_tags() {
    let html = sparkscope::webui::static_index();
    for needle in [
        "og:title",
        "og:image\" content=\"https://",
        "og-image.png",
        "twitter:card",
        "summary_large_image",
        "<meta name=\"description\"",
    ] {
        assert!(html.contains(needle), "manca {needle}");
    }
    assert!(
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/static/og-image.png")).exists()
    );
}

#[test]
fn site_description_is_english_and_icons_are_real_files() {
    use sparkscope::i18n::Lang;
    for lang in [Lang::It, Lang::En] {
        let _g = sparkscope::i18n::scoped(lang);
        let html = sparkscope::webui::static_index();
        let d = sparkscope::webui::SITE_DESCRIPTION;
        assert!(d.starts_with("Analyze spark profiles"));
        assert!(html.contains(&format!("<meta name=\"description\" content=\"{d}\">")));
        assert!(html.contains(&format!("og:description\" content=\"{d}\"")));
        assert!(html.contains("\"@type\":\"WebSite\",\"name\":\"Spark Resource Analyzer\""));
        assert!(html.contains("href=\"./favicon.ico\""));
        assert!(html.contains("href=\"./apple-touch-icon.png\""));
        assert!(
            !html.contains("rel=\"icon\" href=\"data:"),
            "icona inline nel sito"
        );
    }
    for f in [
        "favicon.ico",
        "favicon.svg",
        "favicon-48.png",
        "favicon-96.png",
        "favicon-192.png",
        "apple-touch-icon.png",
    ] {
        let p = format!("{}/static/{f}", env!("CARGO_MANIFEST_DIR"));
        assert!(std::path::Path::new(&p).exists(), "manca {f}");
    }
}

#[test]
fn home_shows_english_news_banner_linking_to_github() {
    use sparkscope::i18n::Lang;
    for lang in [Lang::It, Lang::En] {
        let _g = sparkscope::i18n::scoped(lang);
        let html = sparkscope::webui::static_index();
        assert!(html.contains("id=\"news\""));
        assert!(html.contains(sparkscope::webui::SITE_NEWS));
        assert!(html.contains("href=\"https://github.com/OnlyDehBest/Spark-Analyzer/"));
        assert!(html.contains(">Details →</a>"));
    }
}
