use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};

use sparkscope::loader::{self, DEFAULT_BYTEBIN, Payload};
use sparkscope::model::Profile;
use sparkscope::report::{self, Report, cli::CliOptions};
use sparkscope::{analysis, diff, heap};

#[derive(Parser)]
#[command(
    name = "sparkscope",
    version,
    about = "Analisi approfondita dei profili spark: trova la causa del lag del server e suggerisce le soluzioni"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args, Clone)]
struct Common {
    #[arg(long, default_value = DEFAULT_BYTEBIN, help = "URL del server bytebin da cui scaricare i profili")]
    bytebin: String,
    #[arg(
        long,
        help = "Analizza un thread diverso dal main thread (nome o parte del nome)"
    )]
    thread: Option<String>,
}

#[derive(Subcommand)]
enum Cmd {
    #[command(
        about = "Analizza un profilo (link spark.lucko.me, codice o file .sparkprofile/.sparkheap/.sparkhealth)"
    )]
    Analyze {
        input: String,
        #[command(flatten)]
        common: Common,
        #[arg(long, value_name = "FILE", help = "Scrive un report HTML interattivo")]
        html: Option<PathBuf>,
        #[arg(long, value_name = "FILE", help = "Scrive un report Markdown")]
        md: Option<PathBuf>,
        #[arg(
            long,
            value_name = "FILE",
            help = "Scrive tutti i dati dell'analisi in JSON"
        )]
        json: Option<PathBuf>,
        #[arg(long, default_value_t = 15, help = "Righe per tabella")]
        top: usize,
        #[arg(long, help = "Mostra anche le note informative")]
        all: bool,
        #[arg(long, help = "Disattiva i colori")]
        no_color: bool,
        #[arg(long, help = "Non stampare il report nel terminale")]
        quiet: bool,
    },
    #[command(about = "Confronta due profili (prima/dopo una modifica)")]
    Diff {
        before: String,
        after: String,
        #[command(flatten)]
        common: Common,
        #[arg(long, value_name = "FILE", help = "Scrive il confronto in JSON")]
        json: Option<PathBuf>,
    },
    #[command(about = "Stampa l'albero delle chiamate del main thread, potato sotto una soglia")]
    Tree {
        input: String,
        #[command(flatten)]
        common: Common,
        #[arg(
            long,
            default_value_t = 1.0,
            help = "Soglia minima in % del tempo attivo"
        )]
        min_pct: f64,
        #[arg(long, default_value_t = 60, help = "Profondità massima")]
        depth: usize,
        #[arg(
            long,
            help = "Mostra solo i rami che contengono questo testo (classe, metodo o plugin)"
        )]
        filter: Option<String>,
    },
    #[command(about = "Genera un flamegraph SVG interattivo del main thread")]
    Flame {
        input: String,
        #[command(flatten)]
        common: Common,
        #[arg(short, long, default_value = "flamegraph.svg")]
        output: PathBuf,
    },
}

fn load_report(
    input: &str,
    common: &Common,
) -> Result<(Report, Option<Profile>, Option<heap::HeapReport>)> {
    let loaded = loader::load(input, &common.bytebin)?;
    let kind = loaded.payload.kind();
    match loaded.payload {
        Payload::Sampler(s) => {
            let mut p = Profile::from_proto(*s);
            if let Some(t) = &common.thread
                && !p.select_thread(t)
            {
                let names: Vec<&str> = p.threads.iter().map(|t| t.name.as_str()).collect();
                bail!(
                    "thread '{t}' non trovato. Disponibili: {}",
                    names.join(", ")
                );
            }
            let a = analysis::analyze(&p);
            let r = report::build(&p, a, &loaded.origin, loaded.viewer_url, kind);
            Ok((r, Some(p), None))
        }
        Payload::Health(h) => {
            let p = Profile::from_health(*h);
            let a = analysis::analyze(&p);
            let r = report::build(&p, a, &loaded.origin, loaded.viewer_url, kind);
            Ok((r, None, None))
        }
        Payload::Heap(h) => {
            let hm = h.metadata.clone().unwrap_or_default();
            let hr = heap::analyze(&h);
            let health = sparkscope::proto::HealthData {
                metadata: Some(sparkscope::proto::HealthMetadata {
                    creator: hm.creator,
                    platform_metadata: hm.platform_metadata,
                    platform_statistics: hm.platform_statistics,
                    system_statistics: hm.system_statistics,
                    generated_time: hm.generated_time,
                    server_configurations: hm.server_configurations,
                    sources: hm.sources,
                    extra_platform_metadata: hm.extra_platform_metadata,
                    metrics: hm.metrics,
                }),
                ..Default::default()
            };
            let p = Profile::from_health(health);
            let a = analysis::analyze(&p);
            let r = report::build(&p, a, &loaded.origin, loaded.viewer_url, kind);
            Ok((r, None, Some(hr)))
        }
    }
}

fn write_file(path: &PathBuf, content: &str, what: &str) -> Result<()> {
    std::fs::write(path, content)
        .with_context(|| format!("impossibile scrivere {}", path.display()))?;
    eprintln!("✔ {what} scritto in {}", path.display());
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Analyze {
            input,
            common,
            html,
            md,
            json,
            top,
            all,
            no_color,
            quiet,
        } => {
            eprintln!("Carico {input} …");
            let (r, _, heap_report) = load_report(&input, &common)?;
            if !quiet {
                let color = !no_color
                    && std::io::stdout().is_terminal()
                    && std::env::var_os("NO_COLOR").is_none();
                if let Some(h) = &heap_report {
                    print!("{}", heap::render_text(h));
                }
                report::cli::print(&r, &CliOptions { color, top, all });
            }
            if let Some(p) = html {
                write_file(&p, &report::html::render(&r), "Report HTML")?;
            }
            if let Some(p) = md {
                let mut text = report::markdown::render(&r);
                if let Some(h) = &heap_report {
                    text = format!("```\n{}\n```\n\n{}", heap::render_text(h), text);
                }
                write_file(&p, &text, "Report Markdown")?;
            }
            if let Some(p) = json {
                let value = serde_json::json!({ "report": r, "heap": heap_report });
                write_file(&p, &serde_json::to_string_pretty(&value)?, "JSON")?;
            }
        }
        Cmd::Diff {
            before,
            after,
            common,
            json,
        } => {
            eprintln!("Carico {before} e {after} …");
            let (rb, _, _) = load_report(&before, &common)?;
            let (ra, _, _) = load_report(&after, &common)?;
            let d = diff::diff(&rb, &ra);
            print!("{}", diff::render_text(&d));
            if let Some(p) = json {
                write_file(&p, &serde_json::to_string_pretty(&d)?, "JSON")?;
            }
        }
        Cmd::Tree {
            input,
            common,
            min_pct,
            depth,
            filter,
        } => {
            let (r, p, _) = load_report(&input, &common)?;
            let Some(p) = p else {
                bail!("il comando tree richiede un profilo CPU (sampler)")
            };
            print!(
                "{}",
                sparkscope::analysis::render_tree(
                    &p,
                    &r.analysis,
                    min_pct,
                    depth,
                    filter.as_deref()
                )
            );
        }
        Cmd::Flame {
            input,
            common,
            output,
        } => {
            let (r, _, _) = load_report(&input, &common)?;
            if r.analysis.folded.is_empty() {
                bail!("nessun dato da disegnare");
            }
            let mut opts = inferno::flamegraph::Options::default();
            opts.title = format!(
                "sparkscope — {} ({})",
                r.meta.platform, r.analysis.main.thread
            );
            opts.count_name = "µs".into();
            opts.min_width = 0.05;
            let file = std::fs::File::create(&output)
                .with_context(|| format!("impossibile creare {}", output.display()))?;
            let lines: Vec<&str> = r.analysis.folded.iter().map(|s| s.as_str()).collect();
            inferno::flamegraph::from_lines(&mut opts, lines, std::io::BufWriter::new(file))?;
            eprintln!("✔ Flamegraph scritto in {}", output.display());
        }
    }
    Ok(())
}
