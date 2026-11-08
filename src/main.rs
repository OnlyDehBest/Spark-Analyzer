use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};

use sparkscope::loader::{self, DEFAULT_BYTEBIN};
use sparkscope::report::{self, cli::CliOptions};
use sparkscope::{diff, heap};

#[derive(Parser)]
#[command(
    name = "sparkscope",
    version,
    about = "Analisi approfondita dei profili spark: trova la causa del lag del server e suggerisce le soluzioni"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
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
        about = "Avvia il sito locale: incolli il link o trascini il file e si apre il report (predefinito se avvii senza argomenti)"
    )]
    Serve {
        #[arg(long, default_value_t = 8085, help = "Porta del sito")]
        port: u16,
        #[arg(
            long,
            default_value = "127.0.0.1",
            help = "Indirizzo su cui ascoltare (0.0.0.0 per renderlo raggiungibile dalla rete)"
        )]
        host: String,
        #[arg(long, help = "Non aprire il browser automaticamente")]
        no_open: bool,
        #[arg(long, default_value = DEFAULT_BYTEBIN, help = "URL del server bytebin da cui scaricare i profili")]
        bytebin: String,
    },
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
            help = "Scrive un .txt con le configurazioni da ottimizzare"
        )]
        config: Option<PathBuf>,
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
    #[command(
        about = "Confronta due profili (prima/dopo una modifica) o due heap summary per trovare i memory leak"
    )]
    Diff {
        before: String,
        after: String,
        #[command(flatten)]
        common: Common,
        #[arg(long, value_name = "FILE", help = "Scrive il confronto in JSON")]
        json: Option<PathBuf>,
        #[arg(long, value_name = "FILE", help = "Scrive il confronto heap in HTML")]
        html: Option<PathBuf>,
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

fn load_report(input: &str, common: &Common) -> Result<sparkscope::pipeline::Built> {
    let loaded = loader::load(input, &common.bytebin)?;
    sparkscope::pipeline::build(loaded, common.thread.as_deref())
}

fn write_file(path: &PathBuf, content: &str, what: &str) -> Result<()> {
    std::fs::write(path, content)
        .with_context(|| format!("impossibile scrivere {}", path.display()))?;
    eprintln!("✔ {what} scritto in {}", path.display());
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cmd = cli.cmd.unwrap_or(Cmd::Serve {
        port: 8085,
        host: "127.0.0.1".into(),
        no_open: false,
        bytebin: DEFAULT_BYTEBIN.into(),
    });
    match cmd {
        Cmd::Serve {
            port,
            host,
            no_open,
            bytebin,
        } => {
            sparkscope::server::serve(sparkscope::server::ServeOptions {
                host,
                port,
                open: !no_open,
                bytebin,
            })?;
        }
        Cmd::Analyze {
            input,
            common,
            html,
            md,
            config,
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
                write_file(
                    &p,
                    &sparkscope::webui::render_report(&r, heap_report.as_ref()).html,
                    "Report HTML",
                )?;
            }
            if let Some(p) = config {
                write_file(
                    &p,
                    &report::config_txt::render(&r),
                    "Configurazioni da ottimizzare",
                )?;
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
            html,
        } => {
            eprintln!("Carico {before} e {after} …");
            let first = loader::load(&before, &common.bytebin)?;
            let second = loader::load(&after, &common.bytebin)?;
            if let (loader::Payload::Heap(a), loader::Payload::Heap(b)) =
                (&first.payload, &second.payload)
            {
                let c = sparkscope::heapdiff::compare(a, b);
                print!("{}", sparkscope::heapdiff::render_text(&c));
                if let Some(p) = html {
                    write_file(
                        &p,
                        &sparkscope::report::heapdiff_html::render(&c),
                        "Report HTML",
                    )?;
                }
                if let Some(p) = json {
                    write_file(&p, &serde_json::to_string_pretty(&c)?, "JSON")?;
                }
                return Ok(());
            }
            if matches!(first.payload, loader::Payload::Heap(_))
                != matches!(second.payload, loader::Payload::Heap(_))
            {
                bail!(
                    "per confrontare la memoria servono due heap summary (/spark heapsummary); per confrontare la CPU servono due profili"
                );
            }
            let (rb, _, _) = sparkscope::pipeline::build(first, common.thread.as_deref())?;
            let (ra, _, _) = sparkscope::pipeline::build(second, common.thread.as_deref())?;
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
            if r.analysis.flame.is_empty() {
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
            let folded = r.analysis.flame.folded_lines();
            let lines: Vec<&str> = folded.iter().map(|s| s.as_str()).collect();
            inferno::flamegraph::from_lines(&mut opts, lines, std::io::BufWriter::new(file))?;
            eprintln!("✔ Flamegraph scritto in {}", output.display());
        }
    }
    Ok(())
}
