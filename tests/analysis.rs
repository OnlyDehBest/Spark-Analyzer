#[path = "support/fixture.rs"]
mod fixture;

use prost::Message;
use sparkscope::analysis;
use sparkscope::model::Profile;
use sparkscope::patterns::Cat;
use sparkscope::report;
use sparkscope::rules::Severity;

fn build() -> report::Report {
    let bytes = fixture::sample().encode_to_vec();
    let data = sparkscope::proto::SamplerData::decode(&*bytes).unwrap();
    let p = Profile::from_proto(data);
    let a = analysis::analyze(&p);
    report::build(&p, a, "test", None, "profilo CPU (sampler)")
}

#[test]
fn idle_is_excluded_and_packets_are_kept() {
    let r = build();
    let a = &r.analysis;
    assert_eq!(a.main.thread, "Server thread");
    assert!(a.main.idle > 0.0);
    let net = a
        .categories
        .iter()
        .find(|c| c.cat == Cat::PluginEvents)
        .expect("eventi plugin presenti");
    assert!(
        net.pct > 3.0,
        "i pacchetti gestiti dentro waitUntilNextTick non devono essere idle"
    );
    let busy_expected_per_tick = a.main.busy / a.main.ticks as f64;
    assert!(
        busy_expected_per_tick > 30.0 && busy_expected_per_tick < 60.0,
        "{busy_expected_per_tick}"
    );
}

#[test]
fn sync_chunk_wait_is_attributed_to_plugin() {
    let r = build();
    let c = r
        .analysis
        .categories
        .iter()
        .find(|c| c.cat == Cat::ChunkLoadSync)
        .expect("chunk load sync");
    assert_eq!(
        c.top_owners.first().map(|o| o.name.as_str()),
        Some("WildTeleport")
    );
    assert!(
        r.analysis.categories.iter().all(|c| c.cat != Cat::Blocking),
        "l'attesa dentro getChunk resta nel caricamento chunk"
    );
}

#[test]
fn plugins_and_events() {
    let r = build();
    let a = &r.analysis;
    let ac = a.sources.iter().find(|s| s.source == "AntiCheatX").unwrap();
    assert!(ac.incl_pct > 5.0);
    assert!(
        a.events
            .iter()
            .any(|e| e.entry == "PlayerMoveEvent" && e.source == "AntiCheatX")
    );
    let shop = a
        .sources
        .iter()
        .find(|s| s.source == "EconomyShop")
        .unwrap();
    assert!(
        shop.hazards
            .iter()
            .any(|h| h.name == Cat::SyncDatabase.label())
    );
    assert!(
        shop.top_entries
            .iter()
            .any(|e| e.context == "Task schedulato")
    );
}

#[test]
fn entities_and_block_entities() {
    let r = build();
    let a = &r.analysis;
    assert_eq!(
        a.entity_types.first().map(|e| e.name.as_str()),
        Some("Villager")
    );
    assert_eq!(
        a.block_entity_types.first().map(|e| e.name.as_str()),
        Some("HopperBlockEntity")
    );
}

#[test]
fn spikes_detected_with_autosave_cause() {
    let r = build();
    let a = &r.analysis;
    let idx: Vec<usize> = a.spikes.iter().map(|s| s.window.index).collect();
    for s in fixture::SPIKES {
        assert!(idx.contains(&s), "manca spike {s}: {idx:?}");
    }
    assert_eq!(a.spikes.len(), fixture::SPIKES.len());
    assert_eq!(a.spikes[0].cat_deltas[0].name, Cat::ChunkSave.label());
    assert!(a.periodic_spikes.is_some());
}

#[test]
fn findings_cover_main_causes() {
    let r = build();
    let titles: Vec<&str> = r.findings.iter().map(|f| f.title.as_str()).collect();
    let has = |s: &str| titles.iter().any(|t| t.contains(s));
    assert!(has("Caricamento di chunk sincrono"), "{titles:?}");
    assert!(has("database sincrono"), "{titles:?}");
    assert!(has("EconomyShop"), "{titles:?}");
    assert!(has("hopper"), "{titles:?}");
    assert!(has("lag spike"), "{titles:?}");
    let crit = r
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Critico)
        .count();
    assert!(crit >= 1);
    let cfg = r
        .findings
        .iter()
        .find(|f| f.area == "Configurazione")
        .unwrap();
    assert!(
        cfg.solutions
            .iter()
            .any(|s| s.contains("sync-chunk-writes"))
    );
    let jvm = r
        .findings
        .iter()
        .find(|f| f.area == "JVM / Macchina")
        .unwrap();
    assert!(jvm.solutions.iter().any(|s| s.contains("-Xms")));
    let world = r.findings.iter().find(|f| f.area == "Mondo").unwrap();
    assert!(
        world
            .evidence
            .iter()
            .any(|e| e.contains("x=200") && e.contains("z=-632"))
    );
}

#[test]
fn renders_all_formats() {
    let r = build();
    let html = report::html::render(&r);
    assert!(html.contains("flamedata") && html.contains("WildTeleport"));
    assert!(!html.contains("</script></script>"));
    let md = report::markdown::render(&r);
    assert!(md.contains("## Diagnosi"));
    let json = serde_json::to_string(&r).unwrap();
    assert!(json.len() > 1000);
}

#[test]
fn diff_detects_improvement() {
    let a = build();
    let b = build();
    let d = sparkscope::diff::diff(&a, &b);
    assert!(d.categories.iter().all(|r| r.delta.abs() < 1e-6));
}
