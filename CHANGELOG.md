# Novità

## v1.0.0

Prima versione stabile di **Spark Resource Analyzer**.

### Analisi dei profili spark
- Link di spark (`spark.lucko.me/…`) o file `.sparkprofile`, `.sparkheap`, `.sparkhealth`.
- Diagnosi con gravità, impatto e cosa fare: plugin e metodo responsabili, da dove parte il lavoro
  (evento, comando, task) e cosa fa di pesante (database sincrono, chunk, entità, hopper…).
- Riassunto con punteggio di salute, causa principale e primo intervento.
- Lag spike per finestra con le categorie, i plugin e i metodi cresciuti; filtro per periodo
  (trascina sul grafico o «Solo i lag spike»).
- Flamegraph, albero delle chiamate espandibile con ricerca, metodi, entità, thread, mondo, macchina.
- Profili di allocazione e comando `/spark` consigliato quando il profilo non basta.
- Configurazioni da ottimizzare in un `.txt` scaricabile (`server.properties`, `bukkit.yml`,
  `spigot.yml`, Paper, Purpur), con le modifiche che cambiano il gameplay segnate.

### Memoria
- Heap summary: memoria per plugin e segnali di memory leak (dati di giocatori usciti,
  listener mai deregistrati, cache senza limite).
- Confronto tra due heap summary: verdetto per plugin, ritmo all'ora, stima di quando l'heap si
  riempie, classi Java attribuite al plugin che le riempie.

### Sito e app
- Sito web (l'analisi avviene nel browser) e app per Windows, Linux e macOS.
- Italiano e inglese, tema chiaro e scuro, consumi in % o in ms/tick.
- «Copia link» anche per i file caricati, anteprima nei link condivisi.
- Sicurezza: dati del profilo sanificati, app locale protetta da richieste di altri siti.

### Terminale
- `analyze` (HTML, Markdown, JSON, `--config`), `diff` per profili e heap, `tree`, `flame`.
