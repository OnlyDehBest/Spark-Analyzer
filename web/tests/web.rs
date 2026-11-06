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
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let html = v["html"].as_str().unwrap();
    assert!(html.contains("id=\"dlh\"") && html.contains("id=\"mdsrc\""));
    assert!(html.contains("href=\"/Spark-Analyzer/\""));
    assert!(html.contains("https://spark.lucko.me/AbCd1234"));
    assert_eq!(v["bad"], true);
    assert!(sparkscope_web::analyze_to_json(b"garbage", "x.txt", None, "/").is_err());
}

#[test]
fn static_index_loads_wasm_module() {
    let page = sparkscope::webui::static_index();
    assert!(page.contains("./pkg/sparkscope_web.js") && page.contains("type=\"module\""));
}
