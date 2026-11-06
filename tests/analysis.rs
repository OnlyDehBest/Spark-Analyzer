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
fn inclusive_methods_exclude_idle_wait() {
    let r = build();
    let a = &r.analysis;
    for m in &a.methods_inclusive {
        assert!(
            m.incl_time <= a.main.busy + 1e-6 && m.incl_pct <= 100.0 + 1e-6,
            "{} {} > busy {}",
            m.method,
            m.incl_time,
            a.main.busy
        );
    }
    let wait = a
        .methods_inclusive
        .iter()
        .find(|m| m.method.contains("waitForTasks"))
        .map(|m| m.incl_pct)
        .unwrap_or(0.0);
    assert!(wait < 1.0, "l'attesa idle non e' un hotspot: {wait}");
}

#[test]
fn paper_tick_wait_is_idle_not_blocking() {
    let bytes = fixture::sample_paper_wait().encode_to_vec();
    let data = sparkscope::proto::SamplerData::decode(&*bytes).unwrap();
    let p = Profile::from_proto(data);
    let a = analysis::analyze(&p);
    let r = report::build(&p, a, "test", None, "profilo CPU (sampler)");
    let blocking: f64 = r
        .analysis
        .categories
        .iter()
        .filter(|c| c.cat == Cat::Blocking)
        .map(|c| c.pct)
        .sum();
    assert!(
        blocking < 1.0,
        "l'attesa tra un tick e l'altro in recordTaskExecutionTimeWhileWaiting e' idle: {blocking}"
    );
    let per_tick = r.analysis.main.busy / r.analysis.main.ticks as f64;
    assert!(per_tick > 30.0 && per_tick < 60.0, "{per_tick}");
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
fn unattributed_spike_explains_cause_instead_of_empty_lists() {
    let mut r = build();
    let note = sparkscope::rules::spike_unattributed_note();
    assert!(!report::html::render(&r).contains(note));
    for s in &mut r.analysis.spikes {
        s.cat_deltas.clear();
        s.owner_deltas.clear();
        s.method_deltas.clear();
    }
    let html = report::html::render(&r);
    assert!(html.contains(&note.replace('\'', "&#39;")));
    assert!(!html.contains("<h5>Categorie cresciute</h5>"));
    assert!(report::markdown::render(&r).contains(note));
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
fn config_advice_keeps_farms_working() {
    let r = build();
    let sols: Vec<&String> = r.findings.iter().flat_map(|f| &f.solutions).collect();
    assert!(
        sols.iter().all(|s| !s.contains("hopper-amount =")),
        "hopper-amount vanilla (1) non va alzato: cambia la portata degli hopper"
    );
    let cfg = r
        .findings
        .iter()
        .find(|f| f.area == "Configurazione")
        .unwrap();
    assert!(
        cfg.solutions
            .iter()
            .all(|s| !s.contains("nerf-spawner-mobs")),
        "nerf-spawner-mobs rompe le farm: solo se gli spawner pesano"
    );
    assert!(
        sols.iter()
            .filter(|s| s.contains("redstone-implementation"))
            .all(|s| !s.contains("stesso comportamento")),
        "ALTERNATE_CURRENT non e' identico al vanilla"
    );
}

#[test]
fn jvm_advice_matches_gc_and_java_version() {
    let jvm = |args: &str| {
        let mut data = fixture::sample();
        let java = data
            .metadata
            .as_mut()
            .and_then(|m| m.system_statistics.as_mut())
            .and_then(|s| s.java.as_mut())
            .unwrap();
        java.vm_args = args.into();
        let p = Profile::from_proto(data);
        let a = analysis::analyze(&p);
        let r = report::build(&p, a, "test", None, "profilo CPU (sampler)");
        r.findings
            .into_iter()
            .find(|f| f.area == "JVM / Macchina")
            .map(|f| f.solutions)
            .unwrap_or_default()
    };
    let z = jvm("-Xmx16G -XX:+UseZGC");
    assert!(z.iter().any(|s| s.contains("-XX:+ZGenerational")), "{z:?}");
    assert!(z.iter().any(|s| s.contains("-Xms")), "-Xms assente: {z:?}");
    let ok = jvm("-Xms16G -Xmx16G -XX:+UseZGC -XX:+ZGenerational");
    assert!(
        ok.iter()
            .all(|s| !s.contains("ZGenerational") && !s.contains("-Xms")),
        "{ok:?}"
    );
}

#[test]
fn machine_and_resources_are_reported() {
    let r = build();
    let ram = r.meta.resources.iter().find(|x| x.label == "RAM").unwrap();
    assert!((ram.pct.unwrap() - 87.3).abs() < 1.0);
    let html = report::html::render(&r);
    assert!(html.contains(r#"id="macchina""#) && html.contains("AMD Ryzen 9 5950X"));
    assert!(report::markdown::render(&r).contains("AMD Ryzen 9 5950X"));
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

#[test]
fn info_toggle_only_when_there_are_hidden_notes() {
    let mut r = build();
    let html = report::html::render(&r);
    let hidden = r
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Info && f.area != "Panoramica")
        .count();
    assert_eq!(html.contains("id=\"showinfo\""), hidden > 0);
    assert_eq!(html.matches("finding info hidable").count(), hidden);
    assert_eq!(html.contains("id=\"infonotes\""), hidden > 0);
    r.findings.push(sparkscope::rules::Finding {
        severity: Severity::Info,
        area: "Test".into(),
        title: "nota".into(),
        impact_pct: 0.0,
        evidence: vec![],
        explanation: String::new(),
        solutions: vec![],
        culprits: vec![],
    });
    let html = report::html::render(&r);
    assert!(html.contains(&format!("note informative ({})", hidden + 1)));
}

#[test]
fn empty_and_ticked_profiles() {
    let mut data = fixture::sample();
    data.threads.clear();
    if let Some(m) = data.metadata.as_mut() {
        m.data_aggregator = Some(sparkscope::proto::sampler_metadata::DataAggregator {
            r#type: 1,
            thread_grouper: 0,
            tick_length_threshold: 100,
            number_of_included_ticks: 0,
        });
    }
    let bytes = data.encode_to_vec();
    let loaded = sparkscope::loader::load_bytes(&bytes, "AbCd1234")
        .expect("un profilo senza campioni va riconosciuto");
    let (r, _, _) = sparkscope::pipeline::build(loaded, None).unwrap();
    assert!(
        r.findings
            .iter()
            .any(|f| f.title.contains("non contiene campioni"))
    );
    report::html::render(&r);

    let mut data = fixture::sample();
    if let Some(m) = data.metadata.as_mut() {
        m.data_aggregator = Some(sparkscope::proto::sampler_metadata::DataAggregator {
            r#type: 1,
            thread_grouper: 0,
            tick_length_threshold: 50,
            number_of_included_ticks: 111,
        });
    }
    let p = Profile::from_proto(data);
    let a = analysis::analyze(&p);
    assert_eq!(a.main.ticks, 111);

    let err = sparkscope::loader::load_bytes(b"<html>Not Found</html>", "x")
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("testo"), "{err}");
}

#[test]
fn unit_toggle_and_no_raw_tokens() {
    let r = build();
    let html = report::html::render(&r);
    assert!(html.contains("id=\"unitsel\"") && html.contains("data-u=\"ms/tick\""));
    assert!(html.matches("class=\"pv\"").count() > 50);
    assert!(!html.contains('⟪') && !html.contains('⟫'));
    let md = report::markdown::render(&r);
    assert!(!md.contains('⟪'));
}

#[test]
fn window_filter_recomputes_report() {
    let bytes = fixture::sample().encode_to_vec();
    let full = {
        let l = sparkscope::loader::load_bytes(&bytes, "a.sparkprofile").unwrap();
        sparkscope::pipeline::build(l, None).unwrap().0
    };
    let l = sparkscope::loader::load_bytes(&bytes, "a.sparkprofile").unwrap();
    let (r, _, _) = sparkscope::pipeline::build_with(l, None, Some(&[2, 5, 8])).unwrap();
    assert_eq!(r.meta.window_filter.as_deref(), Some(&[2usize, 5, 8][..]));
    assert_eq!(r.meta.windows_total, 10);
    assert_eq!(r.analysis.main.ticks, 2700);
    assert_eq!(r.analysis.windows.len(), 3);
    assert_eq!(r.analysis.windows[0].index, 2);
    let save = |rep: &report::Report| {
        rep.analysis
            .categories
            .iter()
            .find(|c| c.cat == Cat::ChunkSave)
            .map(|c| c.pct)
            .unwrap_or(0.0)
    };
    assert!(
        save(&r) > save(&full) * 2.0,
        "{} vs {}",
        save(&r),
        save(&full)
    );
    assert!(r.analysis.main.busy_per_tick.unwrap() > full.analysis.main.busy_per_tick.unwrap());
    let h_full = report::html::render(&full);
    assert!(h_full.contains(r#"data-w="2,5,8""#));
    let h = report::html::render(&r);
    assert!(h.contains("Solo i lag spike") && h.contains("disabled"));
    assert!(h.contains("Mostra tutto il profilo"));
}
