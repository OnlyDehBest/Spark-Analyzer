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

#[test]
fn allocation_profile_is_analyzed_across_threads() {
    let bytes = fixture::sample_alloc().encode_to_vec();
    let l = sparkscope::loader::load_bytes(&bytes, "a.sparkprofile").unwrap();
    let (r, _, _) = sparkscope::pipeline::build(l, None).unwrap();
    let al = r.alloc.as_ref().expect("report allocazioni presente");
    assert!((al.total - 1.4e9 * 10.0).abs() < 1e6, "{}", al.total);
    let top = &al.sources[0];
    assert!(top.name.starts_with("AntiCheatX"), "{}", top.name);
    assert!((top.pct - 9.0 / 14.0 * 100.0).abs() < 0.5, "{}", top.pct);
    assert!(al.threads.iter().any(|t| t.name.starts_with("Netty")));
    let site = al
        .sites
        .iter()
        .find(|s| s.method.ends_with("MoveListener.onMove"))
        .expect("il punto di allocazione e' il metodo del plugin, non la classe JDK");
    assert!(
        site.via.as_deref().unwrap_or("").contains("Location"),
        "{:?}",
        site.via
    );
    let hab = |h: sparkscope::alloc::Habit| {
        al.habits
            .iter()
            .find(|x| x.habit == h)
            .map(|x| x.pct)
            .unwrap_or(0.0)
    };
    assert!(hab(sparkscope::alloc::Habit::Locations) > 40.0);
    assert!(hab(sparkscope::alloc::Habit::Streams) > 20.0);
    assert!(hab(sparkscope::alloc::Habit::Strings) > 10.0);
    assert!(
        r.findings
            .iter()
            .any(|f| f.title.contains("AntiCheatX") && f.title.contains("alloca"))
    );
    assert!(
        !r.findings
            .iter()
            .any(|f| f.title == "Profilo di allocazione memoria")
    );
    let html = report::html::render(&r);
    assert!(html.contains(r#"id="allocazioni""#) && html.contains("Punti di allocazione"));
    let md = report::markdown::render(&r);
    assert!(md.contains("Allocazioni di memoria"));
}

#[test]
fn health_summary_score_and_lines() {
    let r = build();
    let s = &r.summary;
    assert!(
        s.score < 85,
        "un server con lag spike non e' ottimo: {}",
        s.score
    );
    assert_eq!(s.lines.len(), 3, "{:?}", s.lines);
    assert!(s.lines[0].starts_with(&format!("Salute {}/100", s.score)));
    assert!(s.cause.is_some() && s.action.is_some());
    assert!(!s.lines.iter().any(|l| l.contains('⟪')));
    let html = report::html::render(&r);
    assert!(html.contains(r#"id="copysum""#) && html.contains("class=\"score\""));
    assert!(report::markdown::render(&r).contains(&format!("**Salute {}/100", s.score)));
}

#[test]
fn recommended_profile_command() {
    let cmd = |r: &report::Report| {
        r.findings
            .iter()
            .find(|f| f.title == "Comando consigliato per il prossimo profilo")
            .map(|f| f.solutions.join("\n"))
    };
    let mut data = fixture::sample();
    if let Some(m) = data.metadata.as_mut() {
        m.data_aggregator = Some(sparkscope::proto::sampler_metadata::DataAggregator {
            r#type: 1,
            thread_grouper: 0,
            tick_length_threshold: 150,
            number_of_included_ticks: 12,
        });
    }
    let p = Profile::from_proto(data);
    let a = analysis::analyze(&p);
    let r = report::build(&p, a, "test", None, "x");
    let c = cmd(&r).expect("pochi tick sopra soglia: consiglio presente");
    assert!(c.contains("--only-ticks-over 100 --timeout 900"), "{c}");

    let mut data = fixture::sample();
    if let Some(m) = data.metadata.as_mut() {
        m.end_time = m.start_time + 45_000;
    }
    let p = Profile::from_proto(data);
    let a = analysis::analyze(&p);
    let r = report::build(&p, a, "test", None, "x");
    let c = cmd(&r).unwrap_or_default();
    assert!(c.contains("/spark profiler start"), "{c}");
    assert!(
        c.contains("/spark profiler start --alloc"),
        "GC frequente nel fixture: {c}"
    );
}

#[test]
fn summary_and_alloc_in_english() {
    let _g = sparkscope::i18n::scoped(sparkscope::i18n::Lang::En);
    let bytes = fixture::sample_alloc().encode_to_vec();
    let l = sparkscope::loader::load_bytes(&bytes, "a.sparkprofile").unwrap();
    let (r, _, _) = sparkscope::pipeline::build(l, None).unwrap();
    assert!(r.summary.lines[0].starts_with("Health "));
    assert!(r.summary.lines[1].starts_with("Main cause: AntiCheatX"));
    let html = report::html::render(&r);
    assert!(html.contains("Copy summary") && html.contains("Allocation sites"));
    assert!(!html.contains("Punti di allocazione"));
}

#[test]
fn advice_names_the_responsible_plugin_and_method() {
    let r = build();
    let all: Vec<String> = r
        .findings
        .iter()
        .flat_map(|f| f.solutions.iter().cloned())
        .collect();
    assert!(
        !all.iter()
            .any(|s| s.contains("plugin responsabil") || s.contains("tra i responsabili")),
        "{all:#?}"
    );
    let db = r
        .findings
        .iter()
        .find(|f| f.title.starts_with("Accesso a database sincrono"))
        .unwrap();
    assert!(
        db.solutions[0].starts_with("EconomyShop 2.4.1 (ShopRepository.saveBalances) deve"),
        "{}",
        db.solutions[0]
    );
    assert!(db.culprits[0].contains("ShopRepository.saveBalances"));
    let ev = r
        .findings
        .iter()
        .find(|f| f.title == "Eventi Bukkit più costosi")
        .unwrap();
    assert!(
        ev.solutions
            .iter()
            .any(|s| s.contains("AntiCheatX 5.0.0 (MoveListener.onMove) su PlayerMoveEvent")),
        "{:?}",
        ev.solutions
    );
    assert!(
        r.summary
            .action
            .as_deref()
            .unwrap_or("")
            .contains("EconomyShop")
    );

    let mut data = fixture::sample();
    data.class_sources
        .retain(|k, _| !k.starts_with("com.example.shop"));
    let p = Profile::from_proto(data);
    let a = analysis::analyze(&p);
    let r = report::build(&p, a, "test", None, "x");
    let db = r
        .findings
        .iter()
        .find(|f| f.title.starts_with("Accesso a database sincrono"))
        .unwrap();
    assert!(
        db.solutions[0].contains("pacchetto com.example.shop (SaveTask.run)")
            && db.culprits[0].contains("com.example.shop"),
        "senza mappatura spark si indica il pacchetto del .jar: {}",
        db.solutions[0]
    );
}

#[test]
fn heap_memory_by_plugin_flags_leaks() {
    let bytes = fixture::sample_heap().encode_to_vec();
    let l = sparkscope::loader::load_bytes(&bytes, "a.sparkheap").unwrap();
    let (r, _, h) = sparkscope::pipeline::build(l, None).unwrap();
    let h = h.expect("heap report");
    let crates = h
        .by_plugin
        .iter()
        .find(|p| p.name == "CrateRewards")
        .expect("CrateRewards riconosciuto dalla classe principale");
    assert_eq!(crates.level, 2);
    assert!(
        crates
            .reasons
            .iter()
            .any(|x| x.contains("giocatori usciti"))
    );
    let holo = h.by_plugin.iter().find(|p| p.name == "HoloLines").unwrap();
    assert_eq!(holo.level, 1);
    let lp = h.by_plugin.iter().find(|p| p.name == "LuckPerms").unwrap();
    assert_eq!(lp.level, 0);
    assert!(h.by_plugin.iter().any(|p| p.name.starts_with("QuickShop")));
    assert!(
        r.findings
            .iter()
            .any(|f| f.title.contains("memory leak in CrateRewards"))
    );
    assert!(
        r.summary
            .cause
            .as_deref()
            .unwrap_or("")
            .contains("CrateRewards")
    );
    let html = sparkscope::webui::render_report(&r, Some(&h)).html;
    assert!(html.contains(r#"<section id="heap""#) && html.contains("Sospetto leak"));
    assert!(
        !html.contains(r##"href="#tempo""##),
        "link del menu verso sezioni assenti"
    );
    let _g = sparkscope::i18n::scoped(sparkscope::i18n::Lang::En);
    let l = sparkscope::loader::load_bytes(&bytes, "a.sparkheap").unwrap();
    let (r, _, h) = sparkscope::pipeline::build(l, None).unwrap();
    let html = sparkscope::webui::render_report(&r, h.as_ref()).html;
    assert!(
        html.contains("Memory by plugin")
            && html.contains("Suspected leak")
            && html.contains("1,840,000")
    );
}

#[test]
fn malicious_profile_cannot_inject_html() {
    let bytes = fixture::sample_malicious().encode_to_vec();
    let l = sparkscope::loader::load_bytes(&bytes, "evil.sparkprofile").unwrap();
    let (r, _, _) = sparkscope::pipeline::build(l, None).unwrap();
    assert!(
        r.analysis
            .sources
            .iter()
            .any(|s| s.display.contains("<img")),
        "il fixture deve davvero contenere il nome malevolo"
    );
    let html = report::html::render(&r);
    for raw in ["<img src=x", "</script><img", "<script>window.__xss"] {
        assert!(!html.contains(raw), "HTML non sicuro: {raw}");
    }
    let mut rest = html.as_str();
    while let Some(i) = rest.find(r#"<script type="application/json""#) {
        let body = &rest[i..];
        let start = body.find('>').unwrap() + 1;
        let end = body.find("</script>").unwrap();
        let json = &body[start..end];
        assert!(
            !json.contains('<'),
            "JSON incorporato con '<': {}",
            &json[..json.len().min(80)]
        );
        serde_json::from_str::<serde_json::Value>(json).expect("JSON incorporato valido");
        rest = &body[end..];
    }
}
