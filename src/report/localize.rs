use regex::Regex;

const PAIRS: &[(&str, &str)] = &[
    ("<html lang=\"it\"", "<html lang=\"en\""),
    ("<i></i>In lag</div>", "<i></i>Lagging</div>"),
    ("<i></i>Stabile</div>", "<i></i>Stable</div>"),
    ("<span>Diagnosi</span>", "<span>Diagnosis</span>"),
    (
        "<span>Tempo main thread</span>",
        "<span>Main thread time</span>",
    ),
    ("<span>Plugin e mod</span>", "<span>Plugins and mods</span>"),
    ("<span>Lag spike</span>", "<span>Lag spikes</span>"),
    ("<span>Metodi</span>", "<span>Methods</span>"),
    ("<span>Entità</span>", "<span>Entities</span>"),
    ("<span>Mondo</span>", "<span>World</span>"),
    ("title=\"Tema chiaro/scuro\"", "title=\"Light/dark theme\""),
    ("<h1>Analisi profilo</h1>", "<h1>Profile analysis</h1>"),
    (" · inizio ", " · started "),
    ("Apri nel viewer spark</a>", "Open in the spark viewer</a>"),
    (
        "<div class=\"sl\">MSPT medio</div>",
        "<div class=\"sl\">Average MSPT</div>",
    ),
    (
        "<div class=\"sl\">MSPT massimo</div>",
        "<div class=\"sl\">Max MSPT</div>",
    ),
    (
        "<div class=\"sl\">Lavoro main thread</div>",
        "<div class=\"sl\">Main thread work</div>",
    ),
    (
        "<div class=\"sl\">Problemi gravi</div>",
        "<div class=\"sl\">Serious problems</div>",
    ),
    (
        "<div class=\"sl\">Lag spike</div>",
        "<div class=\"sl\">Lag spikes</div>",
    ),
    (
        "<div class=\"sl\">Giocatori</div>",
        "<div class=\"sl\">Players</div>",
    ),
    (
        "<div class=\"sl\">Durata profilo</div>",
        "<div class=\"sl\">Profile duration</div>",
    ),
    (" finestre</small>", " windows</small>"),
    (
        "Profilo registrato con <code>",
        "Profile recorded with <code>",
    ),
    (
        "</code>: contiene solo i tick più lenti di ",
        "</code>: it only contains ticks slower than ",
    ),
    ("</span>Diagnosi</h2>", "</span>Diagnosis</h2>"),
    (
        "mostra anche le note informative (",
        "also show informational notes (",
    ),
    ("<b>Responsabili:</b>", "<b>Responsible:</b>"),
    ("<h4>Cosa fare</h4>", "<h4>What to do</h4>"),
    (
        "</span>Dove va il tempo del main thread</h2>",
        "</span>Where the main thread time goes</h2>",
    ),
    ("<th>Categoria</th>", "<th>Category</th>"),
    ("<th>Plugin coinvolti</th>", "<th>Plugins involved</th>"),
    ("<th>Metodi principali</th>", "<th>Main methods</th>"),
    ("<th class=\"n\">tempo</th>", "<th class=\"n\">time</th>"),
    ("Tempo fuori dai plugin: ", "Time outside plugins: "),
    ("</span>Plugin e mod</h2>", "</span>Plugins and mods</h2>"),
    (
        "«Nello stack» = tutto il lavoro avvenuto mentre il codice del plugin era attivo (incluse le chiamate a Minecraft fatte dal plugin). «Proprio» = tempo attribuito al plugin come chiamante più interno.",
        "“On stack” = all the work done while the plugin's code was active (including Minecraft calls made by the plugin). “Own” = time attributed to the plugin as the innermost caller.",
    ),
    (
        "Nessun plugin/mod rilevato sul main thread (oppure il profilo non contiene le informazioni sulle sorgenti).",
        "No plugin/mod detected on the main thread (or the profile has no source information).",
    ),
    (
        "<th class=\"n\">% nello stack</th>",
        "<th class=\"n\">% on stack</th>",
    ),
    (
        "<th class=\"n\">% proprio</th>",
        "<th class=\"n\">% own</th>",
    ),
    ("<th>Punti di ingresso</th>", "<th>Entry points</th>"),
    ("<th>Avvisi</th>", "<th>Warnings</th>"),
    ("<h3>Eventi Bukkit</h3>", "<h3>Bukkit events</h3>"),
    ("<th>Evento</th>", "<th>Event</th>"),
    (
        "</span>Andamento e lag spike</h2>",
        "</span>Trend and lag spikes</h2>",
    ),
    (
        "Il profilo contiene una sola finestra temporale: registra almeno 3-5 minuti per vedere l'andamento.",
        "The profile contains a single time window: record at least 3-5 minutes to see the trend.",
    ),
    (
        "aria-label=\"Timeline MSPT per finestra\"",
        "aria-label=\"MSPT timeline per window\"",
    ),
    ("<title>Finestra #", "<title>Window #"),
    (" giocatori · ", " players · "),
    (" entità · ", " entities · "),
    ("MSPT massimo <span", "Max MSPT <span"),
    ("MSPT mediano <span", "Median MSPT <span"),
    (
        "lavoro main thread (ms/tick) <span",
        "main thread work (ms/tick) <span",
    ),
    ("carico main thread <span", "main thread load <span"),
    ("<h5>Categorie cresciute</h5>", "<h5>Grown categories</h5>"),
    ("<h5>Plugin cresciuti</h5>", "<h5>Grown plugins</h5>"),
    ("<h5>Metodi cresciuti</h5>", "<h5>Grown methods</h5>"),
    (
        "</span>Flamegraph del main thread</h2>",
        "</span>Main thread flamegraph</h2>",
    ),
    (
        "Solo tempo attivo. Clic su un blocco per zoomare, clic sulla radice per tornare indietro. Larghezza = tempo.",
        "Active time only. Click a block to zoom in, click the root to go back. Width = time.",
    ),
    (
        "Cerca metodo o plugin (es. HopperBlockEntity, MyPlugin)",
        "Search method or plugin (e.g. HopperBlockEntity, MyPlugin)",
    ),
    (
        "</span>Metodi più costosi</h2>",
        "</span>Most expensive methods</h2>",
    ),
    ("placeholder=\"Filtra…\"", "placeholder=\"Filter…\""),
    ("<th>Metodo</th>", "<th>Method</th>"),
    (
        "<th class=\"n\">% totale</th>",
        "<th class=\"n\">% total</th>",
    ),
    ("<th>Sorgente</th>", "<th>Source</th>"),
    (
        "<h3>Metodi per tempo totale (inclusi i chiamati)</h3>",
        "<h3>Methods by total time (including callees)</h3>",
    ),
    ("<h3>Percorsi più caldi</h3>", "<h3>Hottest paths</h3>"),
    (
        "</span>Entità e block entity</h2>",
        "</span>Entities and block entities</h2>",
    ),
    ("<h3>Entità (per tipo)</h3>", "<h3>Entities (by type)</h3>"),
    (
        "<h3>Block entity (per tipo)</h3>",
        "<h3>Block entities (by type)</h3>",
    ),
    ("<th>Tipo</th>", "<th>Type</th>"),
    ("class=dim>nessun dato</td>", "class=dim>no data</td>"),
    ("<th class=\"n\">Attivo</th>", "<th class=\"n\">Active</th>"),
    (
        "<th class=\"n\">% del campionato</th>",
        "<th class=\"n\">% of sampled</th>",
    ),
    ("<th>Lavoro principale</th>", "<th>Main work</th>"),
    ("<th>Nota</th>", "<th>Note</th>"),
    ("</span>Mondo</h2>", "</span>World</h2>"),
    (
        "Il profilo non contiene statistiche sul mondo.",
        "The profile contains no world statistics.",
    ),
    ("<th>Mondo</th>", "<th>World</th>"),
    (
        "<th class=\"n\">Entità</th>",
        "<th class=\"n\">Entities</th>",
    ),
    ("<th>Tipi principali</th>", "<th>Main types</th>"),
    ("<th>Teletrasporto</th>", "<th>Teleport</th>"),
    (
        "<h3>Plugin / mod installati</h3>",
        "<h3>Installed plugins / mods</h3>",
    ),
    (" · sorgente: ", " · source: "),
    ("c.title='Clic per copiare'", "c.title='Click to copy'"),
    ("'corrispondenze: '", "'matches: '"),
    ("+'% del tempo'", "+'% of time'"),
];

pub fn to_english(html: &str) -> String {
    let mut out = html.to_string();
    for (it, en) in PAIRS {
        out = out.replace(it, en);
    }
    let re = Regex::new(r"Thread «([^»]*)»: attivo (.*?) su (.*?) campionati \(([^)]*)%\)\. Le percentuali sono calcolate sul tempo attivo, esclusa l'attesa tra un tick e l'altro\. Ogni riga attribuisce il tempo alla categoria più specifica in cui si trova lo stack\.").expect("regex");
    re.replace_all(&out, "Thread “$1”: active $2 of $3 sampled ($4%). Percentages are computed on active time, excluding the wait between ticks. Each row attributes time to the most specific category in the stack.").into_owned()
}
