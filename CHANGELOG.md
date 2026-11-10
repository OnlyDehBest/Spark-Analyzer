# Novità

## Non ancora rilasciato

### Heap dump
- Si possono aprire gli heap dump completi (`.hprof` di `/spark heapdump`, anche compressi `.hprof.gz`)
  da molti GB: il file viene letto a pezzi senza caricarlo tutto in memoria (circa 1,5 GB/s sul
  computer), con la percentuale di lettura; poi si ottengono lo stesso report e lo stesso confronto
  degli heap summary.
- Nell'app sul server l'upload dell'heap dump viene letto mentre arriva, senza limite di 512 MB.
- Heap summary analizzati più in fretta e confronto tra heap con risultato sempre uguale.

### Velocità
- Analisi dei profili fino a ~1,7× più veloce: hash veloce, cache per metodo e sorgente,
  meno allocazioni e selezione dei primi N invece di ordinare tutto.
- Flamegraph costruito durante l'analisi: generazione del report ~10× più veloce e pagina
  più leggera (es. da 1,0 a 0,4 MB su un profilo grande).
- Sul sito il modulo WebAssembly viene ottimizzato con `wasm-opt` e la schermata «Analisi in corso»
  viene mostrata prima di iniziare il calcolo.
- Ordine dei risultati stabile: a parità di tempo, stesso ordine a ogni analisi.
- Sul sito l'analisi gira in un Web Worker: la pagina resta reattiva anche con profili grandi.

### Privacy e sicurezza
- App sul server (`serve` raggiungibile da altri): ogni visitatore vede solo le proprie analisi,
  id dei report casuali e non indovinabili.

### Sito
- Avviso «New» in home con link alle novità; icona del sito come file veri e logo per Google;
  descrizione sempre in inglese.
- Grafico senza la scritta «budget 50 ms».

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
