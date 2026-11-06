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
