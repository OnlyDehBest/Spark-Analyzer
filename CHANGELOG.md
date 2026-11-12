# Novità

## Non ancora rilasciato

### Ricerca su Google
- Titolo e descrizione della home pensati per la ricerca «spark analyzer», robots.txt, sitemap,
  anteprima per i link condivisi; i singoli report non vengono indicizzati.
- Il sito su GitHub Pages indica sparkanalyzer.me come sito principale.

### Privacy
- Avviso sui cookie e pagina «Cookie e privacy» con l'elenco di cookie e dati salvati nel browser.
- I caratteri sono serviti dal sito stesso: nessuna richiesta a Google Fonts.

### Grafica
- Nuovo stile da strumento tecnico: superfici piatte, angoli più squadrati e numeri in monospazio.
- La home mostra un esempio di report; il campo del link ha un prompt da terminale e l'analisi
  mostra i passaggi come righe di log.
- Nel report ogni sezione ha una barra del titolo a finestra.

### Home
- Nell'app sul server la home mostra quanti User hanno già usato il sito e quanti profili
  sono stati analizzati. Il conteggio è anonimo e resta dopo i riavvii.

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
- Caricamento degli heap dump sul server da 4 a 6 volte più veloce: il browser comprime il
  dump in gzip mentre lo invia, manda più pezzi alla volta e ricorda il limite del proxy
  (es. 1 GB da 79 a 19 s, 94 MB dietro un proxy da 1 MB da 43 a 7 s). Durante il
  caricamento viene mostrata anche la velocità.
- Sul server gli heap dump `.hprof` vengono letti direttamente nel browser con lo stesso
  modulo WebAssembly del sito: al server arriva solo il riepilogo (es. 94 MB → 18 KB),
  quindi anche un dump di vari GB è pronto in pochi secondi. Se il browser non lo
  supporta resta il caricamento compresso.
- Trasferimento verificato: il riepilogo viene compresso e controllato dal server prima
  dell'analisi, ogni pezzo porta la sua lunghezza (i pezzi arrivati a metà vengono
  reinviati) e a fine caricamento si controlla la dimensione totale. Se il riepilogo
  arriva danneggiato si ripiega da soli sul caricamento del dump.
- Caricamento più robusto con connessioni lente: i pezzi rifiutati dal proxy per timeout
  (HTTP 408) o persi per strada vengono reinviati, e la dimensione dei pezzi si adatta
  alla velocità della connessione.

### Privacy e sicurezza
- App sul server (`serve` raggiungibile da altri): ogni visitatore vede solo le proprie analisi,
  id dei report casuali e non indovinabili.

### Sito
- Sul server i report vengono salvati su disco (compressi) e restano disponibili anche dopo
  un riavvio o un aggiornamento: nessun limite di numero, quelli più vecchi di 30 giorni
  vengono eliminati da soli (controllo all'avvio e ogni ora). Cartella e giorni si
  cambiano con `--data-dir` e `--retention-days`; in memoria restano solo gli ultimi 30.
- Ottimizzato per iPhone Duo e altri pieghevoli: impaginazione verificata su schermo esterno
  e interno, in verticale e in orizzontale; in orizzontale con poca altezza il riquadro
  principale diventa compatto, zone di tocco da 44 px, rispetto delle aree sicure
  (Dynamic Island) e testo che non si ingrandisce da solo ruotando il telefono.
- Restyling grafico: sfondo con aloni leggeri, riquadro principale con bordo sfumato verde-blu
  e titolo più grande, area di caricamento in rilievo con pulsante «Analizza» blu, selettore
  delle modalità e barra laterale rifiniti, riquadri del report più morbidi. Tolta la sezione
  «Comandi spark» dalla barra laterale. Migliorati i contrasti del tema chiaro.
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
