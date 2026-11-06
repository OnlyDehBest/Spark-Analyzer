# sparkscope

Analizzatore da riga di comando, scritto in Rust, per i profili di [spark](https://spark.lucko.me).
Legge un profilo (link del viewer o file), scompone il tempo del main thread fino al singolo metodo,
individua i plugin/mod responsabili, isola i lag spike e propone soluzioni concrete con i valori di
configurazione da cambiare.

## Cosa analizza

- **Fasi del tick**: entità, AI dei mob, pathfinding, villager/POI, collisioni, item/XP, block entity,
  hopper, spawner, redstone, fluidi, random tick, spawn naturale, caricamento/generazione/salvataggio
  chunk, luce, tracking entità, giocatori, pacchetti, comandi, task schedulati, eventi Bukkit e altro.
  Il tempo viene attribuito alla categoria più specifica dello stack.
- **Plugin e mod**: tempo «nello stack» (tutto il lavoro innescato dal plugin) e tempo «proprio»,
  punti di ingresso con il contesto (quale evento, task schedulato, comando, pacchetto), e avvisi
  per operazioni pericolose sul main thread (database, HTTP, file, attese bloccanti, caricamento chunk
  sincrono, eccezioni, logging).
- **Eventi Bukkit**: costo per evento e per plugin (es. `PlayerMoveEvent → AntiCheat`).
- **Entità e block entity**: costo per tipo (Villager, Zombie, HopperBlockEntity, …).
- **Lag spike**: ogni finestra di ~1 minuto viene confrontata con le finestre normali per mostrare cosa
  è cresciuto durante il picco (categorie, plugin, metodi), con rilevamento dei picchi periodici
  (autosave, backup, timer).
- **Thread secondari**: thread sempre occupati (task async dei plugin, generazione chunk, Netty).
- **JVM e macchina**: versione Java, flag (-Xmx/-Xms, GC, flag di Aikar), swap, CPU rubata da altri
  processi, frequenza e durata del GC.
- **Mondo**: chunk con più entità con le coordinate e il comando per teletrasportarsi, gamerule.
- **Configurazione**: legge `server.properties`, `bukkit.yml`, `spigot.yml`, `paper-world-defaults.yml`,
  `purpur.yml`, `pufferfish.yml` inclusi nel profilo e suggerisce i valori da cambiare.
- **Heap summary** (`.sparkheap`) e **health report** (`.sparkhealth`).
- **Confronto** tra due profili (prima/dopo una modifica).

L'attesa tra un tick e l'altro viene esclusa, mentre i pacchetti e i task eseguiti durante quell'attesa
restano conteggiati, così le percentuali descrivono il lavoro reale del main thread.

## Installazione

Serve Rust 1.85 o più recente.

```sh
git clone <questa repo>
cd sparkscope
cargo build --release
# binario in target/release/sparkscope (sparkscope.exe su Windows)
```

Oppure `cargo install --path .`

## Uso

```sh
# analisi da link del viewer (o solo il codice)
sparkscope analyze https://spark.lucko.me/AbCd1234

# da file salvato con /spark profiler stop --save-to-file
sparkscope analyze plugins/spark/profile-2026-11-06.sparkprofile

# report HTML interattivo (flamegraph con ricerca, timeline, tabelle ordinabili)
sparkscope analyze AbCd1234 --html report.html

# anche Markdown e JSON completo
sparkscope analyze AbCd1234 --md report.md --json report.json

# albero delle chiamate del main thread, solo i rami che contengono un plugin
sparkscope tree AbCd1234 --min-pct 0.5 --filter NomePlugin

# flamegraph SVG standalone
sparkscope flame AbCd1234 -o flame.svg

# confronto prima/dopo
sparkscope diff VecchioCodice NuovoCodice

# analizzare un thread diverso dal main thread
sparkscope analyze AbCd1234 --thread "Craft Scheduler"
```

Opzioni utili: `--top N` (righe per tabella), `--all` (mostra anche le note informative),
`--no-color`, `--quiet`, `--bytebin URL` (server bytebin personalizzato).

## Consigli per registrare un buon profilo

- Profilo normale durante il lag: `/spark profiler start --timeout 300` (almeno 5 minuti per avere
  più finestre da confrontare).
- Solo i lag spike: `/spark profiler start --only-ticks-over 100 --timeout 600`.
- Problemi di memoria/GC: `/spark profiler start --alloc` e `/spark heapsummary`.

## Test

```sh
cargo test
cargo run --example gen_fixture -- sample.sparkprofile   # profilo sintetico realistico
sparkscope analyze sample.sparkprofile --html sample.html
```

## Licenza

GPL-3.0. Le definizioni protobuf in `proto/` provengono da
[lucko/spark](https://github.com/lucko/spark) (GPL-3.0).
