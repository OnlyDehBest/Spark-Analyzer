#[path = "support/fixture.rs"]
mod fixture;

use prost::Message;
use sparkscope::heapdiff::{self, Verdict};
use sparkscope::i18n::{self, Lang};

fn plugin<'a>(c: &'a heapdiff::HeapComparison, name: &str) -> &'a heapdiff::PluginDelta {
    c.plugins
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("plugin {name} assente"))
}

#[test]
fn confirms_leaks_and_separates_player_driven_growth() {
    let (before, after) = fixture::sample_heap_pair();
    let c = heapdiff::compare(&before, &after);
    assert!(!c.swapped);
    assert!(!c.restarted);
    assert!((c.hours - 3.0).abs() < 1e-9);
    assert_eq!((c.before.players, c.after.players), (38, 42));
    assert_eq!(plugin(&c, "CrateRewards").verdict, Verdict::Leak);
    assert_eq!(plugin(&c, "HoloLines").verdict, Verdict::ListenerLeak);
    assert_eq!(plugin(&c, "EconomyShop").verdict, Verdict::GrowsWithPlayers);
    assert_eq!(plugin(&c, "LuckPerms").verdict, Verdict::Stable);
    assert_eq!(plugin(&c, "QuickShop-Hikari").verdict, Verdict::Stable);
    assert_eq!(c.plugins[0].name, "CrateRewards");
    let crates = plugin(&c, "CrateRewards");
    assert!(
        crates.per_hour > 20_000_000.0 && crates.per_hour < 30_000_000.0,
        "{}",
        crates.per_hour
    );
    assert_eq!(
        crates.classes[0].after_instances - crates.classes[0].before_instances,
        472_000
    );
    assert!(c.heap_per_hour > 0.0);
    assert!(c.hours_to_full.is_some_and(|h| h > 1.0 && h < 100.0));
    assert_eq!(c.leaks().count(), 2);
}

#[test]
fn links_java_collections_to_the_leaking_plugin() {
    let (before, after) = fixture::sample_heap_pair();
    let c = heapdiff::compare(&before, &after);
    let java = |k: &str| c.java.iter().find(|j| j.class.kind == k).unwrap();
    assert_eq!(
        java("java.util.UUID").owner_hint.as_deref(),
        Some("CrateRewards")
    );
    assert_eq!(
        java("java.util.HashMap$Node").owner_hint.as_deref(),
        Some("CrateRewards")
    );
    assert_eq!(java("byte[]").owner_hint, None);
}

#[test]
fn order_does_not_matter() {
    let (before, after) = fixture::sample_heap_pair();
    let c = heapdiff::compare(&after, &before);
    assert!(c.swapped);
    assert_eq!(c.before.players, 38);
    assert_eq!(plugin(&c, "CrateRewards").verdict, Verdict::Leak);
}

#[test]
fn stable_server_has_no_leak() {
    let (before, after) = fixture::sample_heap_stable_pair();
    let c = heapdiff::compare(&before, &after);
    assert_eq!(c.leaks().count(), 0);
    assert!(c.plugins.iter().all(|p| p.verdict == Verdict::Stable));
    assert!(c.hours_to_full.is_none());
}

#[test]
fn restart_between_dumps_is_flagged() {
    let (before, _) = fixture::sample_heap_pair();
    let c = heapdiff::compare(&before, &fixture::sample_heap_after_restart());
    assert!(c.restarted);
    assert!(c.warnings.iter().any(|w| w.contains("riavviato")));
}

#[test]
fn loads_from_bytes_and_rejects_non_heap_files() {
    let (before, after) = fixture::sample_heap_pair();
    let a = before.encode_to_vec();
    let b = after.encode_to_vec();
    let c = heapdiff::compare_bytes(&a, "a.sparkheap", &b, "b.sparkheap").unwrap();
    assert_eq!(c.leaks().count(), 2);
    let info = heapdiff::describe_bytes(&a, "a.sparkheap").unwrap();
    assert_eq!(info.players, 38);
    let profile = fixture::sample().encode_to_vec();
    let err = heapdiff::compare_bytes(&profile, "p.sparkprofile", &b, "b.sparkheap").unwrap_err();
    assert!(format!("{err:#}").contains("heap summary"));
}

#[test]
fn renders_report_in_both_languages() {
    let (before, after) = fixture::sample_heap_pair();
    let c = heapdiff::compare(&before, &after);
    let html = sparkscope::report::heapdiff_html::render(&c);
    for needle in [
        "Confronto heap",
        "Leak confermato",
        "Cresce con i giocatori",
        "Stabile",
        "CrateRewards",
        "2.312.000",
        r#"<div class="actions">"#,
    ] {
        assert!(html.contains(needle), "manca {needle}");
    }
    let text = heapdiff::render_text(&c);
    assert!(text.contains("CrateRewards") && text.contains("Leak confermato"));
    let _g = i18n::scoped(Lang::En);
    let c = heapdiff::compare(&before, &after);
    let html = sparkscope::report::heapdiff_html::render(&c);
    for needle in [
        "Heap comparison",
        "Leak confirmed",
        "Grows with players",
        "2,312,000",
    ] {
        assert!(html.contains(needle), "missing {needle}");
    }
    for it in ["Confronto heap", "Leak confermato", "giocatori"] {
        assert!(!html.contains(it), "italiano rimasto: {it}");
    }
}
