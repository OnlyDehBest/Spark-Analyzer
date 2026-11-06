use comfy_table::{Cell, CellAlignment, ContentArrangement, Table, presets};
use owo_colors::OwoColorize;

use super::{Report, fmt_duration, fmt_num, fmt_time, fmt_utc};
use crate::rules::{Severity, short};

pub struct CliOptions {
    pub color: bool,
    pub top: usize,
    pub all: bool,
}

struct P {
    color: bool,
}

impl P {
    fn h1(&self, s: &str) -> String {
        if self.color {
            format!("{}", s.bold().bright_cyan())
        } else {
            s.to_string()
        }
    }
    fn h2(&self, s: &str) -> String {
        if self.color {
            format!("{}", s.bold().cyan())
        } else {
            s.to_string()
        }
    }
    fn dim(&self, s: &str) -> String {
        if self.color {
            format!("{}", s.dimmed())
        } else {
            s.to_string()
        }
    }
    fn bold(&self, s: &str) -> String {
        if self.color {
            format!("{}", s.bold())
        } else {
            s.to_string()
        }
    }
    fn sev(&self, s: Severity) -> String {
        let l = format!(" {} ", s.label());
        if !self.color {
            return format!("[{}]", s.label());
        }
        match s {
            Severity::Critico => format!("{}", l.bold().white().on_red()),
            Severity::Alto => format!("{}", l.bold().black().on_bright_red()),
            Severity::Medio => format!("{}", l.bold().black().on_yellow()),
            Severity::Basso => format!("{}", l.black().on_bright_blue()),
            Severity::Info => format!("{}", l.black().on_white()),
        }
    }
    fn good_bad(&self, s: &str, bad: bool) -> String {
        if !self.color {
            return s.to_string();
        }
        if bad {
            format!("{}", s.bright_red().bold())
        } else {
            format!("{}", s.bright_green())
        }
    }
}

fn table(headers: &[&str]) -> Table {
    let mut t = Table::new();
    t.load_style(presets::UTF8_FULL_CONDENSED.with_rounded_corners())
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(headers.iter().map(Cell::new).collect::<Vec<_>>());
    if let Some(w) = terminal_width() {
        t.set_width(w);
    } else {
        t.set_width(140);
    }
    t
}

fn terminal_width() -> Option<u16> {
    std::env::var("COLUMNS").ok().and_then(|c| c.parse().ok())
}

fn num(v: f64) -> Cell {
    Cell::new(fmt_num(v)).set_alignment(CellAlignment::Right)
}

fn bar(pct: f64, width: usize) -> String {
    let filled = ((pct / 100.0) * width as f64)
        .round()
        .clamp(0.0, width as f64) as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

pub fn print(r: &Report, o: &CliOptions) {
    let p = P { color: o.color };
    let m = &r.meta;
    let a = &r.analysis;
    println!();
    println!(
        "{}",
        p.h1(&format!("━━ sparkscope · analisi {} ━━", m.kind))
    );
    println!("{} {}", p.dim("Sorgente:"), m.origin);
    if let Some(v) = &m.viewer_url {
        println!("{} {}", p.dim("Viewer:  "), v);
    }
    let mc = if m.minecraft_version.is_empty() {
        String::new()
    } else {
        format!(" · MC {}", m.minecraft_version)
    };
    println!(
        "{} {}{} · spark {} · motore {} · modalità {}",
        p.dim("Server:  "),
        m.platform,
        mc,
        m.spark_version,
        m.engine,
        m.mode
    );
    println!(
        "{} inizio {} · durata {} · intervallo {} ms · {} tick · {} giocatori · {} plugin/mod",
        p.dim("Profilo: "),
        fmt_utc(m.start_time),
        fmt_duration(m.duration_s),
        fmt_num(m.interval_ms),
        m.ticks,
        m.players,
        m.plugin_count
    );
    if !m.comment.is_empty() {
        println!("{} {}", p.dim("Commento:"), m.comment);
    }
    let mut health = Vec::new();
    if let Some(t) = m.tps {
        health.push(format!(
            "TPS {} / {} / {} (1m/5m/15m)",
            p.good_bad(&fmt_num(t[0]), t[0] < 19.5),
            fmt_num(t[1]),
            fmt_num(t[2])
        ));
    }
    if let Some(ms) = m.mspt_mean {
        health.push(format!(
            "MSPT medio {} ms",
            p.good_bad(&fmt_num(ms), ms > 45.0)
        ));
    }
    if let Some(mx) = m.mspt_max {
        health.push(format!("max {} ms", p.good_bad(&fmt_num(mx), mx > 50.0)));
    }
    if let Some(b) = a.main.busy_per_tick {
        health.push(format!(
            "lavoro main thread {} ms/tick",
            p.good_bad(&fmt_num(b), b > 45.0)
        ));
    }
    if !health.is_empty() {
        println!("{} {}", p.dim("Salute:  "), health.join(" · "));
    }

    println!();
    println!("{}", p.h1("■ DIAGNOSI"));
    let mut shown = 0;
    for fnd in &r.findings {
        if !o.all
            && fnd.severity == Severity::Info
            && fnd.area != crate::i18n::t("Panoramica", "Overview")
        {
            continue;
        }
        if !o.all && shown >= o.top.max(6) * 2 {
            break;
        }
        shown += 1;
        println!();
        let impact = if fnd.impact_pct > 0.0 {
            format!(
                "  {}",
                p.dim(&format!("impatto {}%", fmt_num(fnd.impact_pct)))
            )
        } else {
            String::new()
        };
        println!(
            "{} {} {}{}",
            p.sev(fnd.severity),
            p.bold(&fnd.title),
            p.dim(&format!("[{}]", fnd.area)),
            impact
        );
        for e in &fnd.evidence {
            println!("   {} {}", p.dim("•"), e);
        }
        if !fnd.culprits.is_empty() && fnd.area != crate::i18n::t("Plugin/Mod", "Plugin/Mod") {
            println!("   {} {}", p.bold("Responsabili:"), fnd.culprits.join(", "));
        }
        if !fnd.explanation.is_empty() && fnd.area != crate::i18n::t("Panoramica", "Overview") {
            println!("   {} {}", p.dim("Perché:"), p.dim(&fnd.explanation));
        }
        if !fnd.solutions.is_empty() {
            println!("   {}", p.bold("Cosa fare:"));
            for (i, s) in fnd.solutions.iter().enumerate() {
                println!(
                    "     {}. {}",
                    i + 1,
                    s.replace("\n    - ", "\n          - ")
                );
            }
        }
    }

    if a.main.total <= 0.0 {
        println!();
        return;
    }

    println!();
    println!(
        "{}",
        p.h1(&format!(
            "■ DOVE VA IL TEMPO DEL MAIN THREAD («{}»)",
            a.main.thread
        ))
    );
    println!(
        "{}",
        p.dim(&format!(
            "attivo {} su {} campionati ({}%) · le percentuali sono sul tempo attivo, esclusa l'attesa tra un tick e l'altro",
            fmt_time(a.main.busy, &a.unit),
            fmt_time(a.main.total, &a.unit),
            fmt_num(a.main.busy_pct)
        ))
    );
    let per_tick = a.main.ticks > 0 && a.unit == "ms";
    let mut t = table(&[
        "Categoria",
        "%",
        "",
        if per_tick { "ms/tick" } else { "tempo" },
        "Plugin principale",
    ]);
    for c in a.categories.iter().take(o.top.max(12)) {
        if c.pct < 0.2 {
            continue;
        }
        let owner = c
            .top_owners
            .first()
            .map(|o| format!("{} ({}%)", o.name, fmt_num(o.pct)))
            .unwrap_or_default();
        let v = if per_tick {
            fmt_num(c.time / a.main.ticks as f64)
        } else {
            fmt_time(c.time, &a.unit)
        };
        t.add_row(vec![
            Cell::new(&c.label),
            num(c.pct),
            Cell::new(bar(c.pct, 16)),
            Cell::new(v).set_alignment(CellAlignment::Right),
            Cell::new(owner),
        ]);
    }
    println!("{t}");

    if !a.sources.is_empty() {
        println!();
        println!("{}", p.h2("■ PLUGIN / MOD SUL MAIN THREAD"));
        let mut t = table(&[
            "Plugin",
            "% nello stack",
            "% proprio",
            "Ingresso principale",
            "Avvisi",
        ]);
        for s in a.sources.iter().take(o.top.max(10)) {
            if s.incl_pct < 0.1 {
                continue;
            }
            let entry = s
                .top_entries
                .first()
                .map(|e| format!("{} [{}]", short(&e.entry), e.context))
                .unwrap_or_default();
            let hz: Vec<String> = s
                .hazards
                .iter()
                .filter(|h| h.pct >= 0.2)
                .map(|h| format!("{} {}%", h.name, fmt_num(h.pct)))
                .collect();
            t.add_row(vec![
                Cell::new(&s.display),
                num(s.incl_pct),
                num(s.self_pct),
                Cell::new(entry),
                Cell::new(hz.join("; ")),
            ]);
        }
        println!("{t}");
    }
    if !a.buckets.is_empty() {
        let b: Vec<String> = a
            .buckets
            .iter()
            .filter(|b| b.pct >= 0.5)
            .map(|b| format!("{} {}%", b.name, fmt_num(b.pct)))
            .collect();
        println!("{} {}", p.dim("Codice non di plugin:"), b.join(" · "));
    }

    println!();
    println!("{}", p.h2("■ METODI PIÙ COSTOSI (tempo proprio)"));
    let mut t = table(&["Metodo", "% proprio", "% totale", "Sorgente"]);
    for mm in a.methods.iter().take(o.top) {
        t.add_row(vec![
            Cell::new(short(&mm.method)),
            num(mm.self_pct),
            num(mm.incl_pct),
            Cell::new(mm.source.clone().unwrap_or_default()),
        ]);
    }
    println!("{t}");

    if !a.entity_types.is_empty() || !a.block_entity_types.is_empty() {
        println!();
        println!("{}", p.h2("■ ENTITÀ E BLOCK ENTITY PIÙ COSTOSE"));
        let mut t = table(&["Entità", "%", "Block entity", "%"]);
        let n = a
            .entity_types
            .len()
            .max(a.block_entity_types.len())
            .min(o.top.max(8));
        for i in 0..n {
            let e = a.entity_types.get(i);
            let b = a.block_entity_types.get(i);
            t.add_row(vec![
                Cell::new(e.map(|x| x.name.clone()).unwrap_or_default()),
                e.map(|x| num(x.pct)).unwrap_or_else(|| Cell::new("")),
                Cell::new(b.map(|x| x.name.clone()).unwrap_or_default()),
                b.map(|x| num(x.pct)).unwrap_or_else(|| Cell::new("")),
            ]);
        }
        println!("{t}");
    }

    if !a.events.is_empty() {
        println!();
        println!("{}", p.h2("■ EVENTI BUKKIT"));
        let mut t = table(&["Evento", "Plugin", "%"]);
        for e in a.events.iter().take(o.top.max(8)) {
            t.add_row(vec![Cell::new(&e.entry), Cell::new(&e.source), num(e.pct)]);
        }
        println!("{t}");
    }

    if !a.spikes.is_empty() {
        println!();
        println!(
            "{}",
            p.h2("■ LAG SPIKE (confronto con le finestre normali)")
        );
        for s in a.spikes.iter().take(o.top.min(8)) {
            println!(
                " {} {} — {}",
                p.bold("▲"),
                p.bold(&crate::rules::window_label(&s.window)),
                s.reason
            );
            if s.window.mspt_max > 0.0 {
                println!(
                    "   {}",
                    p.dim(&format!(
                        "TPS {} · MSPT mediana {} / max {} · {} giocatori · {} entità · {} tile · {} chunk",
                        fmt_num(s.window.tps),
                        fmt_num(s.window.mspt_median),
                        fmt_num(s.window.mspt_max),
                        s.window.players,
                        s.window.entities,
                        s.window.tile_entities,
                        s.window.chunks
                    ))
                );
            }
            let cats: Vec<String> = s
                .cat_deltas
                .iter()
                .take(4)
                .map(|d| format!("{} +{}", d.name, fmt_num(d.delta)))
                .collect();
            if !cats.is_empty() {
                println!("   categorie ({}): {}", s.unit, cats.join(", "));
            }
            let ow: Vec<String> = s
                .owner_deltas
                .iter()
                .take(3)
                .map(|d| format!("{} +{}", d.name, fmt_num(d.delta)))
                .collect();
            if !ow.is_empty() {
                println!("   plugin: {}", ow.join(", "));
            }
            let me: Vec<String> = s
                .method_deltas
                .iter()
                .take(3)
                .map(|d| format!("{} +{}", short(&d.name), fmt_num(d.delta)))
                .collect();
            if !me.is_empty() {
                println!("   metodi: {}", me.join(", "));
            }
        }
        if let Some(pp) = &a.periodic_spikes {
            println!(" {}", p.bold(pp));
        }
    }

    println!();
    println!("{}", p.h2("■ PERCORSI PIÙ CALDI"));
    for (i, h) in a.hot_paths.iter().take(o.top.min(6)).enumerate() {
        let tag = [h.cat.clone(), h.owner.clone()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        println!(" {}. {}% {}", i + 1, p.bold(&fmt_num(h.pct)), p.dim(&tag));
        let tail: Vec<&String> = h
            .frames
            .iter()
            .rev()
            .take(7)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        println!(
            "    {}",
            tail.iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" → ")
        );
    }

    let others: Vec<_> = a
        .threads
        .iter()
        .filter(|t| t.name != a.main.thread && t.busy > 0.0)
        .take(o.top.min(10))
        .collect();
    if !others.is_empty() {
        println!();
        println!("{}", p.h2("■ ALTRI THREAD (tempo attivo)"));
        let mut t = table(&["Thread", "Attivo", "Lavoro principale", "Nota"]);
        for th in others {
            let tm = th
                .top_methods
                .first()
                .map(|m| format!("{} {}%", m.name, fmt_num(m.pct)))
                .unwrap_or_default();
            let note = [th.plugin_hint.clone(), th.top_owner.clone()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
            t.add_row(vec![
                Cell::new(&th.name),
                Cell::new(fmt_time(th.busy, &a.unit)).set_alignment(CellAlignment::Right),
                Cell::new(tm),
                Cell::new(note),
            ]);
        }
        println!("{t}");
    }

    if !r.hot_chunks.is_empty() {
        println!();
        println!("{}", p.h2("■ CHUNK CON PIÙ ENTITÀ"));
        let mut t = table(&["Mondo", "x", "z", "Entità", "Tipi principali"]);
        for h in r.hot_chunks.iter().take(o.top.min(10)) {
            let ty: Vec<String> = h.top.iter().map(|(k, v)| format!("{k} {v}")).collect();
            t.add_row(vec![
                Cell::new(&h.world),
                Cell::new(h.block_x).set_alignment(CellAlignment::Right),
                Cell::new(h.block_z).set_alignment(CellAlignment::Right),
                Cell::new(h.total).set_alignment(CellAlignment::Right),
                Cell::new(ty.join(", ")),
            ]);
        }
        println!("{t}");
    }
    println!();
}
