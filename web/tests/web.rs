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
