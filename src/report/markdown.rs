use std::fmt::Write;

use super::{Report, fmt_duration, fmt_num, fmt_time, fmt_utc};
use crate::rules::short;

fn esc(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

pub fn render(r: &Report) -> String {
    let mut o = String::new();
    let m = &r.meta;
    let a = &r.analysis;
    let _ = writeln!(o, "# Analisi spark — {}\n", m.platform);
    if let Some(v) = &m.viewer_url {
        let _ = writeln!(o, "Profilo: <{v}>  ");
    }
    let _ = writeln!(
        o,
        "Tipo: {} · modalità {} · motore {} · inizio {} · durata {} · {} tick · {} giocatori · {} plugin/mod\n",
        m.kind,
        m.mode,
        m.engine,
        fmt_utc(m.start_time),
        fmt_duration(m.duration_s),
        m.ticks,
        m.players,
        m.plugin_count
    );
    if let Some(t) = m.tps {
        let _ = write!(
            o,
            "**TPS** {} / {} / {} ",
            fmt_num(t[0]),
            fmt_num(t[1]),
            fmt_num(t[2])
        );
    }
    if let Some(ms) = m.mspt_mean {
        let _ = write!(o, "· **MSPT** medio {} ms ", fmt_num(ms));
    }
    if let Some(b) = a.main.busy_per_tick {
        let _ = write!(o, "· **main thread** {} ms/tick", fmt_num(b));
    }
    let _ = writeln!(o, "\n");

    for (i, l) in r.summary.lines.iter().enumerate() {
        if i == 0 {
            let _ = writeln!(o, "> **{}**  ", esc(l));
        } else {
            let _ = writeln!(o, "> {}  ", esc(l));
        }
    }
    let _ = writeln!(o);

    let _ = writeln!(o, "## Diagnosi\n");
    for f in &r.findings {
        let impact = if f.impact_pct > 0.0 {
            format!(" — impatto {}%", fmt_num(f.impact_pct))
        } else {
            String::new()
        };
        let _ = writeln!(o, "### [{}] {}{}\n", f.severity.label(), f.title, impact);
        let _ = writeln!(o, "_Area: {}_\n", f.area);
        for e in &f.evidence {
            let _ = writeln!(o, "- {}", crate::rules::plain_pv(e));
        }
        if !f.culprits.is_empty() {
            let _ = writeln!(
                o,
                "\n**Responsabili:** {}",
                crate::rules::plain_pv(&f.culprits.join(", "))
            );
        }
        if !f.explanation.is_empty() {
            let _ = writeln!(o, "\n{}", f.explanation);
        }
        if !f.solutions.is_empty() {
            let _ = writeln!(o, "\n**Cosa fare:**\n");
            for (i, s) in f.solutions.iter().enumerate() {
                let _ = writeln!(o, "{}. {}", i + 1, s.replace("\n    - ", "\n   - "));
            }
        }
        let _ = writeln!(o);
    }

    if let Some(al) = &r.alloc {
        alloc_md(&mut o, al);
    }

    if a.main.total > 0.0 {
        let per_tick = a.main.ticks > 0 && a.unit == "ms";
        let _ = writeln!(o, "## Main thread «{}»\n", a.main.thread);
        let _ = writeln!(
            o,
            "Attivo {} su {} ({}%).\n",
            fmt_time(a.main.busy, &a.unit),
            fmt_time(a.main.total, &a.unit),
            fmt_num(a.main.busy_pct)
        );
        let _ = writeln!(
            o,
            "| Categoria | % | {} | Plugin principale |\n|---|---:|---:|---|",
            if per_tick { "ms/tick" } else { "tempo" }
        );
        for c in a.categories.iter().filter(|c| c.pct >= 0.2) {
            let owner = c
                .top_owners
                .first()
                .map(|x| format!("{} ({}%)", x.name, fmt_num(x.pct)))
                .unwrap_or_default();
            let v = if per_tick {
                fmt_num(c.time / a.main.ticks as f64)
            } else {
                fmt_time(c.time, &a.unit)
            };
            let _ = writeln!(
                o,
                "| {} | {} | {} | {} |",
                c.label,
                fmt_num(c.pct),
                v,
                esc(&owner)
            );
        }
        if !a.sources.is_empty() {
            let _ = writeln!(
                o,
                "\n## Plugin / mod\n\n| Plugin | % nello stack | % proprio | Ingresso principale | Avvisi |\n|---|---:|---:|---|---|"
            );
            for s in a.sources.iter().filter(|s| s.incl_pct >= 0.1).take(30) {
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
                let _ = writeln!(
                    o,
                    "| {} | {} | {} | {} | {} |",
                    esc(&s.display),
                    fmt_num(s.incl_pct),
                    fmt_num(s.self_pct),
                    esc(&entry),
                    esc(&hz.join("; "))
                );
            }
        }
        let _ = writeln!(
            o,
            "\n## Metodi più costosi\n\n| Metodo | % proprio | % totale | Sorgente |\n|---|---:|---:|---|"
        );
        for mm in a.methods.iter().take(30) {
            let _ = writeln!(
                o,
                "| `{}` | {} | {} | {} |",
                mm.method,
                fmt_num(mm.self_pct),
                fmt_num(mm.incl_pct),
                mm.source.clone().unwrap_or_default()
            );
        }
        if !a.entity_types.is_empty() {
            let _ = writeln!(o, "\n## Entità più costose\n\n| Entità | % |\n|---|---:|");
            for e in a.entity_types.iter().take(15) {
                let _ = writeln!(o, "| {} | {} |", e.name, fmt_num(e.pct));
            }
        }
        if !a.block_entity_types.is_empty() {
            let _ = writeln!(
                o,
                "\n## Block entity più costose\n\n| Block entity | % |\n|---|---:|"
            );
            for e in a.block_entity_types.iter().take(15) {
                let _ = writeln!(o, "| {} | {} |", e.name, fmt_num(e.pct));
            }
        }
        if !a.events.is_empty() {
            let _ = writeln!(o, "\n## Eventi\n\n| Evento | Plugin | % |\n|---|---|---:|");
            for e in a.events.iter().take(20) {
                let _ = writeln!(
                    o,
                    "| {} | {} | {} |",
                    e.entry,
                    esc(&e.source),
                    fmt_num(e.pct)
                );
            }
        }
        if !a.spikes.is_empty() {
            let _ = writeln!(o, "\n## Lag spike\n");
            for s in a.spikes.iter().take(10) {
                let _ = writeln!(
                    o,
                    "- **{}** — {}",
                    crate::rules::window_label(&s.window),
                    s.reason
                );
                let cats: Vec<String> = s
                    .cat_deltas
                    .iter()
                    .take(4)
                    .map(|d| format!("{} +{}", d.name, fmt_num(d.delta)))
                    .collect();
                if !cats.is_empty() {
                    let _ = writeln!(o, "  - categorie ({}): {}", s.unit, cats.join(", "));
                }
                let ow: Vec<String> = s
                    .owner_deltas
                    .iter()
                    .take(3)
                    .map(|d| format!("{} +{}", d.name, fmt_num(d.delta)))
                    .collect();
                if !ow.is_empty() {
                    let _ = writeln!(o, "  - plugin: {}", ow.join(", "));
                }
            }
            if a.spikes
                .iter()
                .take(10)
                .any(crate::rules::spike_unattributed)
            {
                let _ = writeln!(
                    o,
                    "
{}",
                    crate::rules::spike_unattributed_note()
                );
            }
        }
        let _ = writeln!(o, "\n## Percorsi più caldi\n");
        for h in a.hot_paths.iter().take(10) {
            let _ = writeln!(o, "- {}% — `{}`", fmt_num(h.pct), h.frames.join(" → "));
        }
    }
    if !r.meta.machine.is_empty() || !r.meta.resources.is_empty() {
        let _ = writeln!(
            o,
            "
## Macchina e risorse

| | |
|---|---|"
        );
        for (k, v) in &r.meta.machine {
            let _ = writeln!(o, "| {} | {} |", esc(k), esc(v));
        }
        for x in &r.meta.resources {
            let _ = writeln!(o, "| {} | {} |", esc(&x.label), esc(&x.value));
        }
    }
    if !r.hot_chunks.is_empty() {
        let _ = writeln!(
            o,
            "\n## Chunk con più entità\n\n| Mondo | x | z | Entità | Tipi |\n|---|---:|---:|---:|---|"
        );
        for h in &r.hot_chunks {
            let ty: Vec<String> = h.top.iter().map(|(k, v)| format!("{k} {v}")).collect();
            let _ = writeln!(
                o,
                "| {} | {} | {} | {} | {} |",
                h.world,
                h.block_x,
                h.block_z,
                h.total,
                ty.join(", ")
            );
        }
    }
    let _ = writeln!(
        o,
        "\n_Generato da sparkscope {}_",
        env!("CARGO_PKG_VERSION")
    );
    o
}

fn alloc_md(o: &mut String, al: &crate::alloc::AllocReport) {
    let t = crate::i18n::t;
    let _ = writeln!(
        o,
        "## {}\n",
        t("Allocazioni di memoria", "Memory allocations")
    );
    let _ = writeln!(
        o,
        "{}\n",
        tf!(
            "{} allocati in {} su tutti i thread · main thread {}%",
            "{} allocated over {} across all threads · main thread {}%",
            fmt_time(al.total, "byte"),
            fmt_duration(al.duration_s),
            fmt_num(al.main_pct)
        )
    );
    if !al.sources.is_empty() {
        let _ = writeln!(o, "| Plugin | % |\n|---|---:|");
        for s in al.sources.iter().take(15) {
            let _ = writeln!(o, "| {} | {} |", esc(&s.name), fmt_num(s.pct));
        }
        let _ = writeln!(o);
    }
    let _ = writeln!(
        o,
        "### {}\n\n| {} | {} | {} | % |\n|---|---|---|---:|",
        t("Punti di allocazione", "Allocation sites"),
        t("Metodo", "Method"),
        t("Sorgente", "Source"),
        t("Tramite", "Via")
    );
    for s in al.sites.iter().take(20) {
        let _ = writeln!(
            o,
            "| `{}` | {} | {} | {} |",
            esc(&short(&s.method)),
            esc(s.source.as_deref().unwrap_or("-")),
            esc(s.via.as_deref().unwrap_or("")),
            fmt_num(s.pct)
        );
    }
    let _ = writeln!(o);
    let habits: Vec<_> = al.habits.iter().filter(|h| h.pct >= 0.5).take(8).collect();
    if !habits.is_empty() {
        let _ = writeln!(
            o,
            "### {}\n\n| {} | % | {} |\n|---|---:|---|",
            t("Schemi che creano oggetti", "Patterns that create objects"),
            t("Schema", "Pattern"),
            t("Chi", "Who")
        );
        for h in habits {
            let who: Vec<String> = h
                .owners
                .iter()
                .map(|x| format!("{} {}%", x.name, fmt_num(x.pct)))
                .collect();
            let _ = writeln!(
                o,
                "| {} | {} | {} |",
                esc(&h.label),
                fmt_num(h.pct),
                esc(&who.join(", "))
            );
        }
        let _ = writeln!(o);
    }
}
