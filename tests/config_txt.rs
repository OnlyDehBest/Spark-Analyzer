#[path = "support/fixture.rs"]
mod fixture;

use prost::Message;
use sparkscope::i18n::{self, Lang};
use sparkscope::report::{self, ConfigChange};
use sparkscope::rules::Severity;

fn build(data: sparkscope::proto::SamplerData) -> report::Report {
    let bytes = data.encode_to_vec();
    let l = sparkscope::loader::load_bytes(&bytes, "p.sparkprofile").unwrap();
    sparkscope::pipeline::build(l, None).unwrap().0
}

fn change<'a>(r: &'a report::Report, file: &str, path: &str) -> &'a ConfigChange {
    r.config_changes
        .iter()
        .find(|c| c.file == file && c.path == path)
        .unwrap_or_else(|| panic!("manca {file} › {path}"))
}

#[test]
fn collects_structured_changes_linked_to_findings() {
    let r = build(fixture::sample());
    assert!(r.config_changes.len() >= 15, "{}", r.config_changes.len());
    let sync = change(&r, "server.properties", "sync-chunk-writes");
    assert_eq!(sync.current.as_deref(), Some("true"));
    assert_eq!(sync.recommended, "false");
    assert!(sync.note.contains("crash"));
    assert!(sync.gameplay);
    let explosions = change(
        &r,
        "config/paper-world-defaults.yml",
        "environment.optimize-explosions",
    );
    assert_eq!(explosions.recommended, "true");
    assert!(!explosions.gameplay);
    let chunks = change(
        &r,
        "config/paper-world-defaults.yml",
        "chunks.prevent-moving-into-unloaded-chunks",
    );
    assert_eq!(chunks.severity, Severity::Critico);
    assert!(chunks.reason.contains("chunk"));
    let villagers = change(
        &r,
        "spigot.yml",
        "world-settings.default.entity-activation-range.villagers",
    );
    assert_eq!(villagers.recommended, "16");
    let mut seen = std::collections::HashSet::new();
    for c in &r.config_changes {
        assert!(
            seen.insert((&c.file, &c.path)),
            "duplicato {} › {}",
            c.file,
            c.path
        );
    }
    let order: Vec<usize> = r
        .config_changes
        .iter()
        .map(|c| report::config_txt::file_rank(&c.file))
        .collect();
    assert!(
        order.windows(2).all(|w| w[0] <= w[1]),
        "ordine dei file: {order:?}"
    );
}

#[test]
fn renders_txt_in_both_languages() {
    let r = build(fixture::sample());
    let txt = report::config_txt::render(&r);
    for needle in [
        "CONFIGURAZIONI DA OTTIMIZZARE",
        "server.properties",
        "spigot.yml",
        "sync-chunk-writes",
        "attuale: true",
        "nuovo:   false [!]",
        "onlynelchilling",
    ] {
        assert!(txt.contains(needle), "manca {needle}");
    }
    let explosions = txt.split("environment.optimize-explosions").nth(1).unwrap();
    assert!(!explosions.lines().take(3).any(|l| l.contains("[!]")));
    let _g = i18n::scoped(Lang::En);
    let r = build(fixture::sample());
    let txt = report::config_txt::render(&r);
    assert!(
        txt.contains("SETTINGS TO OPTIMIZE")
            && txt.contains("current: true")
            && txt.contains("new:     false [!]")
    );
    for it in ["attuale", "nuovo:", "consigliato", "giocatori"] {
        assert!(!txt.contains(it), "italiano nel txt inglese: {it}");
    }
}

#[test]
fn report_has_download_link_only_when_there_is_something_to_change() {
    let r = build(fixture::sample());
    let html = report::html::render(&r);
    assert!(html.contains(r#"class="cfgdl""#));
    assert!(html.contains("download=\"configurazioni-ottimizzate.txt\""));
    assert!(html.contains("href=\"data:text/plain;charset=utf-8,"));
    assert!(html.contains(&format!(
        "Scarica le {} configurazioni da cambiare (.txt)",
        r.config_changes.len()
    )));
    let unknown = build(fixture::sample_without_config());
    assert!(!unknown.config_changes.is_empty());
    assert!(unknown.config_changes.iter().all(|c| c.current.is_none()));
    assert!(report::config_txt::render(&unknown).contains("non presente nel profilo"));
    let mut clean = build(fixture::sample());
    clean.config_changes.clear();
    assert!(!report::html::render(&clean).contains(r#"class="cfgdl""#));
}
