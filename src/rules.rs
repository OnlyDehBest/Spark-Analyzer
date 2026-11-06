use serde::Serialize;

use crate::analysis::{Analysis, CatStat};
use crate::config::{Cfg, Found, ServerConfig};
use crate::model::{Mode, Profile};
use crate::patterns::Cat;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Severity {
    Critico,
    Alto,
    Medio,
    Basso,
    Info,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Critico => "CRITICO",
            Severity::Alto => "ALTO",
            Severity::Medio => "MEDIO",
            Severity::Basso => "BASSO",
            Severity::Info => "INFO",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub area: String,
    pub title: String,
    pub impact_pct: f64,
    pub evidence: Vec<String>,
    pub explanation: String,
    pub solutions: Vec<String>,
    pub culprits: Vec<String>,
}

pub struct Ctx<'a> {
    pub a: &'a Analysis,
    pub p: &'a Profile,
    pub cfg: ServerConfig,
    pub lagging: bool,
    pub tps: Option<f64>,
    pub mspt: Option<f64>,
}

fn f(v: f64) -> String {
    if v >= 100.0 {
        format!("{v:.0}")
    } else if v >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}

impl<'a> Ctx<'a> {
    pub fn new(a: &'a Analysis, p: &'a Profile) -> Ctx<'a> {
        let cfg = ServerConfig::new(&p.meta.server_configurations);
        let ps = p.meta.platform_statistics.as_ref();
        let tps = ps
            .and_then(|s| s.tps.as_ref())
            .map(|t| t.last1m)
            .filter(|v| *v > 0.0);
        let mspt = ps
            .and_then(|s| s.mspt.as_ref())
            .and_then(|m| m.last1m.as_ref())
            .map(|m| m.mean)
            .filter(|v| *v > 0.0);
        let window_lag = a
            .windows
            .iter()
            .any(|w| w.tps > 0.0 && w.tps < 19.0 || w.mspt_max > 50.0);
        let lagging = tps.map(|t| t < 19.5).unwrap_or(false)
            || mspt.map(|m| m > 45.0).unwrap_or(false)
            || window_lag
            || a.main.busy_per_tick.map(|b| b > 45.0).unwrap_or(false)
            || p.ticked_threshold().is_some();
        Ctx {
            a,
            p,
            cfg,
            lagging,
            tps,
            mspt,
        }
    }

    fn cat(&self, c: Cat) -> Option<&'a CatStat> {
        self.a.categories.iter().find(|x| x.cat == c)
    }

    fn sev(&self, pct: f64) -> Severity {
        let s = if pct >= 30.0 {
            Severity::Alto
        } else if pct >= 15.0 {
            Severity::Medio
        } else {
            Severity::Basso
        };
        if self.lagging {
            match s {
                Severity::Alto if pct >= 40.0 => Severity::Critico,
                Severity::Alto => Severity::Alto,
                Severity::Medio => Severity::Alto,
                Severity::Basso if pct >= 8.0 => Severity::Medio,
                x => x,
            }
        } else {
            s
        }
    }

    fn ms(&self, time: f64) -> String {
        match self.a.main.busy_per_tick {
            Some(_) if self.a.main.ticks > 0 => {
                format!("{} ms/tick", f(time / self.a.main.ticks as f64))
            }
            _ => format!("{} {}", f(time), self.a.unit),
        }
    }

    fn cat_evidence(&self, c: &CatStat) -> Vec<String> {
        let mut ev = vec![format!(
            "{}: {}% del tempo attivo del main thread ({})",
            c.label,
            f(c.pct),
            self.ms(c.time)
        )];
        let ms: Vec<String> = c
            .top_methods
            .iter()
            .take(4)
            .filter(|m| m.pct >= 0.3)
            .map(|m| format!("{} ({}%)", short(&m.name), f(m.pct)))
            .collect();
        if !ms.is_empty() {
            ev.push(format!("Metodi più pesanti: {}", ms.join(", ")));
        }
        if !c.example_stack.is_empty() {
            ev.push(format!(
                "Stack tipico: {}",
                c.example_stack
                    .iter()
                    .map(|s| s.trim().trim_start_matches("↳ ").to_string())
                    .collect::<Vec<_>>()
                    .join(" → ")
            ));
        }
        ev
    }

    fn culprits(&self, c: &CatStat) -> Vec<String> {
        c.top_owners
            .iter()
            .filter(|o| o.pct >= 0.5)
            .map(|o| {
                format!(
                    "{} ({}% del main thread in questa categoria)",
                    self.p.sources.display_name(&o.name),
                    f(o.pct)
                )
            })
            .collect()
    }

    fn adv(
        &self,
        found: Option<Found>,
        file: &str,
        path: &str,
        rec: &str,
        bad: impl Fn(&Cfg) -> bool,
    ) -> Option<String> {
        match found {
            Some(x) => {
                if bad(&x.value) {
                    Some(format!("{} → consigliato {}", x.describe(), rec))
                } else {
                    None
                }
            }
            None => {
                let file_known = self.cfg.has(file)
                    || (file.contains("paper-world-defaults") && self.cfg.has("paper/"))
                    || (file.contains("paper-global") && self.cfg.has("paper/"));
                if self.cfg.is_empty() || !file_known {
                    Some(format!("{file} › {path}: verifica e imposta {rec}"))
                } else {
                    None
                }
            }
        }
    }

    fn paper(
        &self,
        path: &str,
        legacy: Option<&str>,
        rec: &str,
        bad: impl Fn(&Cfg) -> bool,
    ) -> Option<String> {
        let found = self.cfg.paper_world(path, legacy);
        if found.is_none() && !self.cfg.is_paper() && !self.cfg.is_empty() {
            return None;
        }
        self.adv(found, "config/paper-world-defaults.yml", path, rec, bad)
    }

    fn spigot(&self, path: &str, rec: &str, bad: impl Fn(&Cfg) -> bool) -> Option<String> {
        if !self.cfg.has("spigot.yml") && !self.cfg.is_empty() {
            return None;
        }
        self.adv(
            self.cfg.spigot_world(path),
            "spigot.yml",
            &format!("world-settings.default.{path}"),
            rec,
            bad,
        )
    }

    fn bukkit(&self, path: &str, rec: &str, bad: impl Fn(&Cfg) -> bool) -> Option<String> {
        if !self.cfg.has("bukkit.yml") && !self.cfg.is_empty() {
            return None;
        }
        self.adv(
            self.cfg.get("bukkit.yml", path),
            "bukkit.yml",
            path,
            rec,
            bad,
        )
    }

    fn props(&self, key: &str, rec: &str, bad: impl Fn(&Cfg) -> bool) -> Option<String> {
        if !self.cfg.has("server.properties") && !self.cfg.is_empty() {
            return None;
        }
        self.adv(
            self.cfg.get("server.properties", key),
            "server.properties",
            key,
            rec,
            bad,
        )
    }

    fn method_pct(&self, pred: impl Fn(&str) -> bool) -> f64 {
        self.a
            .methods_inclusive
            .iter()
            .filter(|m| pred(&m.method))
            .map(|m| m.incl_pct)
            .fold(0.0, f64::max)
    }
}

fn gt(v: f64) -> impl Fn(&Cfg) -> bool {
    move |c: &Cfg| c.num().map(|n| n > v).unwrap_or(false)
}
fn lt(v: f64) -> impl Fn(&Cfg) -> bool {
    move |c: &Cfg| c.num().map(|n| n < v).unwrap_or(false)
}
fn is_bool(b: bool) -> impl Fn(&Cfg) -> bool {
    move |c: &Cfg| c.bool() == Some(b)
}
fn neq_text(v: &'static str) -> impl Fn(&Cfg) -> bool {
    move |c: &Cfg| !c.text().eq_ignore_ascii_case(v)
}

pub fn short(method: &str) -> String {
    let mut parts = method.rsplitn(2, '.');
    let m = parts.next().unwrap_or(method);
    let class = parts.next().unwrap_or("");
    let simple = class.rsplit('.').next().unwrap_or(class);
    if simple.is_empty() {
        m.to_string()
    } else {
        format!("{simple}.{m}")
    }
}

fn push_some(v: &mut Vec<String>, items: Vec<Option<String>>) {
    for i in items.into_iter().flatten() {
        v.push(i);
    }
}

pub fn diagnose(a: &Analysis, p: &Profile) -> Vec<Finding> {
    let ctx = Ctx::new(a, p);
    let mut out: Vec<Finding> = Vec::new();
    overview(&ctx, &mut out);
    if p.mode == Mode::Execution {
        category_rules(&ctx, &mut out);
        plugin_rules(&ctx, &mut out);
        spike_rules(&ctx, &mut out);
        thread_rules(&ctx, &mut out);
    } else {
        mode_note(&ctx, &mut out);
    }
    jvm_rules(
        &ctx.p.meta.system_statistics,
        ctx.p.meta.platform_statistics.as_ref(),
        ctx.lagging,
        &mut out,
    );
    world_rules(ctx.p.meta.platform_statistics.as_ref(), &mut out);
    config_rules(&ctx, &mut out);
    out.sort_by(|x, y| {
        let px = x.area == "Panoramica";
        let py = y.area == "Panoramica";
        py.cmp(&px)
            .then(x.severity.cmp(&y.severity))
            .then(y.impact_pct.total_cmp(&x.impact_pct))
    });
    out
}

fn overview(c: &Ctx, out: &mut Vec<Finding>) {
    let mut ev = Vec::new();
    if let Some(t) = c.tps {
        ev.push(format!("TPS ultimo minuto: {}", f(t)));
    }
    if let Some(m) = c.mspt {
        ev.push(format!(
            "MSPT medio ultimo minuto: {} ms (budget 50 ms)",
            f(m)
        ));
    }
    if let Some(ps) = c.p.meta.platform_statistics.as_ref()
        && let Some(m) = ps.mspt.as_ref().and_then(|m| m.last1m.as_ref())
        && m.max > 0.0
    {
        ev.push(format!(
            "MSPT 95° percentile {} ms, massimo {} ms",
            f(m.percentile95),
            f(m.max)
        ));
    }
    if let Some(b) = c.a.main.busy_per_tick {
        ev.push(format!(
            "Lavoro medio del main thread misurato dal profilo: {} ms/tick su {} tick",
            f(b),
            c.a.main.ticks
        ));
    }
    ev.push(format!(
        "Main thread occupato per il {}% del tempo campionato",
        f(c.a.main.busy_pct)
    ));
    if let Some(th) = c.p.ticked_threshold() {
        ev.push(format!("Profilo registrato con --only-ticks-over {th}: contiene SOLO i tick lenti, quindi le percentuali descrivono direttamente i lag spike"));
    }
    let (sev, title, expl) = if c.lagging {
        (
            Severity::Info,
            "Il server mostra lag",
            "Le sezioni seguenti indicano dove il main thread spende il tempo. Ogni tick deve stare sotto 50 ms: tutto ciò che supera il budget abbassa i TPS.",
        )
    } else {
        (
            Severity::Info,
            "Nessun lag evidente nel periodo profilato",
            "Il main thread rientra nel budget di 50 ms per tick. Se il lag è sporadico, registra un profilo con /spark profiler start --only-ticks-over 100 per catturare solo i picchi.",
        )
    };
    out.push(Finding {
        severity: sev,
        area: "Panoramica".into(),
        title: title.into(),
        impact_pct: 0.0,
        evidence: ev,
        explanation: expl.into(),
        solutions: Vec::new(),
        culprits: Vec::new(),
    });
}

struct CatRule {
    cat: Cat,
    min: f64,
    title: &'static str,
    explanation: &'static str,
    solutions: &'static [&'static str],
}

const CAT_RULES: &[CatRule] = &[
    CatRule {
        cat: Cat::EntityTick,
        min: 12.0,
        title: "Troppo tempo speso nel tick delle entità",
        explanation: "Ogni entità caricata viene aggiornata 20 volte al secondo. Farm di mob, allevamenti enormi, armor stand e item a terra moltiplicano questo costo.",
        solutions: &[
            "Riduci il numero di entità: limita gli animali per chunk con un plugin (es. un limiter di entità per chunk) e controlla i chunk più affollati elencati nella sezione Mondo",
            "Abbassa gli entity-activation-range in spigot.yml così le entità lontane dai giocatori vengono aggiornate meno spesso",
            "Abbassa simulation-distance in server.properties (le entità oltre questa distanza non vengono aggiornate)",
        ],
    },
    CatRule {
        cat: Cat::MobAi,
        min: 6.0,
        title: "L'intelligenza artificiale dei mob pesa molto",
        explanation: "Goal selector, brain e sensori dei mob (soprattutto villager, piglin, axolotl, frog, warden, allay) vengono valutati ad ogni tick per ogni mob attivo.",
        solutions: &[
            "Su Pufferfish/Purpur attiva DAB (dynamic activation of brain) per ridurre la frequenza dell'AI dei mob lontani",
            "Riduci entity-activation-range in spigot.yml e tick-inactive-villagers: false",
            "Limita i mob negli allevamenti e nelle farm: mob in spazi stretti continuano a cercare target e percorsi",
        ],
    },
    CatRule {
        cat: Cat::Pathfinding,
        min: 5.0,
        title: "Pathfinding costoso",
        explanation: "I mob ricalcolano spesso i percorsi: succede quando sono intrappolati, in grandi gruppi o inseguono target irraggiungibili.",
        solutions: &[
            "Evita mob bloccati in spazi chiusi senza uscita (spawner, trappole), che ricalcolano il percorso di continuo",
            "Su Purpur/Pufferfish riduci la frequenza di aggiornamento del pathfinding per i mob inattivi",
            "Limita gli zombie/piglin che inseguono i villager nelle trading hall",
        ],
    },
    CatRule {
        cat: Cat::Villagers,
        min: 5.0,
        title: "Villager e POI pesanti",
        explanation: "I villager cercano continuamente letti, postazioni di lavoro e campane (POI). Le trading hall con decine di villager sono una delle cause più comuni di lag.",
        solutions: &[
            "Su Purpur attiva mobs.villager.lobotomize.enabled: true (i villager bloccati smettono di calcolare l'AI)",
            "Imposta tick-inactive-villagers: false in spigot.yml",
            "Limita il numero di villager per chunk e spezza le trading hall troppo grandi",
        ],
    },
    CatRule {
        cat: Cat::Collisions,
        min: 6.0,
        title: "Collisioni e ricerche di entità costose",
        explanation: "Quando molte entità si trovano nello stesso blocco ogni entità controlla le collisioni con tutte le altre: il costo cresce in modo quadratico.",
        solutions: &[
            "Riduci il massimo di collisioni per tick (collisions.max-entity-collisions in paper-world-defaults.yml, es. 2)",
            "Abbassa la gamerule maxEntityCramming (es. /gamerule maxEntityCramming 8)",
            "Cerca farm di mob o allevamenti con entità ammassate: vedi i chunk con più entità nella sezione Mondo",
        ],
    },
    CatRule {
        cat: Cat::ItemsXp,
        min: 4.0,
        title: "Troppi item o sfere di esperienza a terra",
        explanation: "Ogni item e orb di XP è un'entità che si muove, si unisce ad altre e controlla le collisioni. Farm senza raccolta generano migliaia di item.",
        solutions: &[
            "Aumenta merge-radius in spigot.yml (item 3.5, exp 4.0) per unire gli item vicini",
            "Riduci il despawn degli item comuni con entities.spawning.alt-item-despawn-rate in paper-world-defaults.yml",
            "Verifica le farm che lasciano cadere item a terra e aggiungi hopper/raccolta o limitale",
        ],
    },
    CatRule {
        cat: Cat::BlockEntities,
        min: 8.0,
        title: "Block entity (tile entity) pesanti",
        explanation: "Fornaci, hopper, spawner, beacon, campfire, sculk e altri blocchi con logica propria vengono aggiornati ogni tick nei chunk caricati.",
        solutions: &[
            "Guarda la tabella «Tipi di block entity» per capire quale tipo pesa di più",
            "Limita il numero di blocchi di quel tipo per chunk o per giocatore",
        ],
    },
    CatRule {
        cat: Cat::Hoppers,
        min: 4.0,
        title: "Gli hopper consumano molto tempo",
        explanation: "Gli hopper controllano ogni tick l'inventario sopra e sotto e le entità item sopra di loro. Catene di hopper e sorter grandi sono molto costosi, e ogni spostamento chiama l'evento InventoryMoveItemEvent.",
        solutions: &[
            "Imposta hopper.disable-move-event: true in paper-world-defaults.yml se nessun plugin usa InventoryMoveItemEvent",
            "Aumenta ticks-per.hopper-transfer e hopper-amount in spigot.yml (es. 8 tick e 8 item: stessa velocità media, meno controlli)",
            "Sostituisci le catene lunghe di hopper con water stream o limita gli hopper per chunk",
        ],
    },
    CatRule {
        cat: Cat::Spawners,
        min: 3.0,
        title: "Spawner di mob pesanti",
        explanation: "Ogni spawner attivo controlla la presenza di giocatori e tenta lo spawn ad ogni tick.",
        solutions: &[
            "Imposta tick-rates.mob-spawner: 2 o più in paper-world-defaults.yml",
            "Attiva nerf-spawner-mobs: true in spigot.yml (i mob degli spawner non hanno AI)",
            "Se usi un plugin per spawner impilati, verifica le sue impostazioni di stack",
        ],
    },
    CatRule {
        cat: Cat::Redstone,
        min: 5.0,
        title: "Redstone e aggiornamenti di blocchi pesanti",
        explanation: "Clock di redstone, piston e observer generano a cascata migliaia di aggiornamenti dei blocchi vicini.",
        solutions: &[
            "Imposta misc.redstone-implementation: ALTERNATE_CURRENT in paper-world-defaults.yml (stesso comportamento, molto meno aggiornamenti)",
            "Individua i clock di redstone sempre attivi e usa un plugin che limita/disattiva i clock troppo veloci",
            "Evita farm con piston/observer in loop continuo",
        ],
    },
    CatRule {
        cat: Cat::Fluids,
        min: 4.0,
        title: "Fluidi che si propagano",
        explanation: "Acqua e lava che scorrono generano tick schedulati e aggiornamenti di blocchi; laghi di lava o cascate create da giocatori sono spesso la causa.",
        solutions: &[
            "Cerca e rimuovi grandi flussi di lava/acqua creati dai giocatori",
            "Usa un plugin di protezione che limita il flusso dei fluidi in aree non costruite",
        ],
    },
    CatRule {
        cat: Cat::ChunkTick,
        min: 10.0,
        title: "Tick dei chunk (random tick, meteo) elevato",
        explanation: "Ogni chunk entro la simulation-distance riceve random tick (crescita delle colture, foglie, ghiaccio) e aggiornamenti meteo. Più giocatori sparsi = più chunk attivi.",
        solutions: &[
            "Abbassa simulation-distance in server.properties (es. 6) mantenendo la view-distance per la visuale",
            "Verifica che la gamerule randomTickSpeed sia 3 (valore vanilla)",
            "Riduci farm di colture enormi o foreste di foglie piantate dai giocatori",
        ],
    },
    CatRule {
        cat: Cat::MobSpawning,
        min: 5.0,
        title: "Lo spawn naturale dei mob è costoso",
        explanation: "Il server cerca posizioni valide per lo spawn in tutti i chunk vicini ai giocatori, ogni tick, per ogni categoria di mob.",
        solutions: &[
            "Aumenta ticks-per.monster-spawns in bukkit.yml (es. 10) e riduci spawn-limits",
            "Imposta entities.spawning.per-player-mob-spawns: true in paper-world-defaults.yml",
            "Abbassa mob-spawn-range in spigot.yml (es. 6) e la simulation-distance",
        ],
    },
    CatRule {
        cat: Cat::ScheduledTicks,
        min: 5.0,
        title: "Tick schedulati di blocchi e fluidi elevati",
        explanation: "Blocchi come redstone, fluidi, cactus, sabbia/gravel che cadono e osservatori programmano tick futuri: tante macchine attive li accumulano.",
        solutions: &[
            "Cerca macchine e farm sempre attive nelle aree con più giocatori",
            "Valuta ALTERNATE_CURRENT per la redstone e limita i fluidi che scorrono",
        ],
    },
    CatRule {
        cat: Cat::ChunkLoadSync,
        min: 1.0,
        title: "Caricamento di chunk sincrono sul main thread",
        explanation: "Il main thread si ferma ad aspettare il caricamento da disco o la generazione di un chunk. È una delle cause principali di lag spike: spesso causato da plugin che chiamano getChunkAt/getBlockAt in chunk non caricati, teletrasporti o giocatori che volano velocemente.",
        solutions: &[
            "Se un plugin compare tra i responsabili, deve usare le API asincrone (World#getChunkAtAsync, teleportAsync su Paper)",
            "Pregenera il mondo con Chunky e imposta un world border, così i chunk non vanno generati al volo",
            "Imposta chunks.prevent-moving-into-unloaded-chunks: true in paper-world-defaults.yml",
            "Usa dischi SSD/NVMe: il caricamento da disco lento allunga ogni attesa",
        ],
    },
    CatRule {
        cat: Cat::ChunkGen,
        min: 3.0,
        title: "Generazione di chunk che pesa sul server",
        explanation: "La generazione di nuovi chunk (rumore, biomi, strutture, feature) è molto costosa. Se compare sul main thread blocca il tick; altrove consuma CPU che serve al server.",
        solutions: &[
            "Pregenera il mondo con Chunky (es. /chunky radius 10000) e imposta un world border",
            "Se la generazione avviene per mappe del tesoro o delfini, disattiva environment.treasure-maps.find-already-discovered o le mappe del tesoro (paper-world-defaults.yml)",
            "Limita i giocatori che esplorano a grande velocità (elytra) con un limite di velocità o pregenerazione",
        ],
    },
    CatRule {
        cat: Cat::ChunkSave,
        min: 4.0,
        title: "Salvataggio dei chunk / autosave pesante",
        explanation: "Il salvataggio scrive i chunk modificati su disco. Se troppi chunk vengono salvati nello stesso tick si crea un picco periodico.",
        solutions: &[
            "Imposta chunks.max-auto-save-chunks-per-tick: 8 in paper-world-defaults.yml per spalmare l'autosave",
            "Imposta sync-chunk-writes=false in server.properties",
            "Controlla backup e plugin che chiamano World#save o /save-all frequentemente",
            "Verifica la velocità del disco (SSD consigliato)",
        ],
    },
    CatRule {
        cat: Cat::ChunkSystem,
        min: 6.0,
        title: "Gestione dei chunk (ticket, load/unload) elevata",
        explanation: "Il sistema dei chunk calcola quali chunk tenere caricati. Teletrasporti frequenti, view-distance alta e plugin che forzano il caricamento di chunk lo appesantiscono.",
        solutions: &[
            "Abbassa view-distance e simulation-distance",
            "Controlla plugin che tengono chunk caricati (chunk loader, forceload) e la loro quantità",
            "Imposta chunks.delay-chunk-unloads-by: 10s in paper-world-defaults.yml per evitare caricamenti/scaricamenti a ripetizione",
        ],
    },
    CatRule {
        cat: Cat::Lighting,
        min: 4.0,
        title: "Calcolo della luce pesante",
        explanation: "La luce viene ricalcolata quando molti blocchi cambiano: WorldEdit, esplosioni, generazione o farm che piazzano/rompono blocchi in continuazione.",
        solutions: &[
            "Usa FastAsyncWorldEdit invece di WorldEdit per le grandi modifiche",
            "Controlla macchine che piazzano/rompono blocchi di continuo",
            "Aggiorna il server: le versioni recenti di Paper hanno un motore di luce molto più veloce",
        ],
    },
    CatRule {
        cat: Cat::EntityTracking,
        min: 6.0,
        title: "Tracking delle entità costoso",
        explanation: "Il server calcola per ogni entità quali giocatori la vedono e invia i pacchetti di movimento. Molte entità e molti giocatori vicini moltiplicano il costo.",
        solutions: &[
            "Abbassa entity-tracking-range in spigot.yml (es. players 48, animals 32, monsters 32, misc 16)",
            "Abbassa entity-broadcast-range-percentage in server.properties (es. 75)",
            "Riduci il numero di entità nelle aree affollate",
        ],
    },
    CatRule {
        cat: Cat::Players,
        min: 10.0,
        title: "Tick dei giocatori elevato",
        explanation: "Ogni giocatore fa caricare chunk, calcolare la visuale e aggiornare inventario e statistiche.",
        solutions: &[
            "Abbassa view-distance (o usa send-view-distance più alta della simulation-distance)",
            "Controlla i plugin che agiscono su ogni giocatore ad ogni tick (scoreboard, tablist, boss bar)",
        ],
    },
    CatRule {
        cat: Cat::Network,
        min: 8.0,
        title: "Gestione dei pacchetti dei giocatori pesante",
        explanation: "I pacchetti ricevuti dai giocatori (movimento, click, chat) vengono processati sul main thread. Anticheat e plugin che ascoltano PlayerMoveEvent aggiungono costo a ogni pacchetto.",
        solutions: &[
            "Controlla i plugin responsabili: listener pesanti su PlayerMoveEvent o PlayerInteractEvent vanno ottimizzati",
            "Imposta un network-compression-threshold ragionevole (256) in server.properties",
        ],
    },
    CatRule {
        cat: Cat::PluginTasks,
        min: 6.0,
        title: "Task schedulati sincroni costosi",
        explanation: "I task dei plugin eseguiti con runTask/runTaskTimer girano sul main thread e rallentano ogni tick in cui si attivano.",
        solutions: &[
            "Guarda la sezione Plugin per vedere quali task pesano di più",
            "Il lavoro che non tocca il mondo (database, calcoli, file, HTTP) va spostato in runTaskAsynchronously",
            "Aumenta l'intervallo dei task periodici o distribuisci il lavoro su più tick",
        ],
    },
    CatRule {
        cat: Cat::PluginEvents,
        min: 6.0,
        title: "Listener di eventi costosi",
        explanation: "Gli eventi Bukkit vengono chiamati sul main thread. Eventi frequenti (movimento, fisica dei blocchi, spostamento item) con listener pesanti rallentano ogni tick.",
        solutions: &[
            "Guarda la tabella «Eventi» per vedere quale evento e quale plugin pesano di più",
            "Nei listener esci subito quando l'evento non è rilevante (controlla prima i casi più economici)",
            "Evita accessi a database, file o calcoli pesanti dentro i listener",
        ],
    },
    CatRule {
        cat: Cat::Maps,
        min: 3.0,
        title: "Aggiornamento delle mappe costoso",
        explanation: "Le mappe tenute in mano o negli item frame vengono aggiornate continuamente.",
        solutions: &[
            "Limita i muri di mappe e usa un plugin per mappe statiche (image maps) con rendering in cache",
        ],
    },
    CatRule {
        cat: Cat::Explosions,
        min: 3.0,
        title: "Esplosioni costose",
        explanation: "TNT e creeper calcolano raggi e danni su molte entità e blocchi.",
        solutions: &[
            "Imposta environment.optimize-explosions: true in paper-world-defaults.yml",
            "Limita i cannoni TNT e le TNT duper",
        ],
    },
    CatRule {
        cat: Cat::Scoreboard,
        min: 3.0,
        title: "Scoreboard e team aggiornati troppo spesso",
        explanation: "Plugin di scoreboard/tablist che aggiornano team e punteggi per ogni giocatore ad ogni tick generano molti pacchetti e calcoli.",
        solutions: &[
            "Aumenta l'intervallo di aggiornamento del plugin di scoreboard/tablist",
            "Riduci i placeholder dinamici aggiornati ogni tick",
        ],
    },
    CatRule {
        cat: Cat::Logging,
        min: 2.0,
        title: "Scrittura su console/log sul main thread",
        explanation: "Un plugin o il server stanno scrivendo molti messaggi nel log; la formattazione e la scrittura costano e spesso indicano errori ripetuti.",
        solutions: &[
            "Apri logs/latest.log e cerca messaggi o errori che si ripetono",
            "Disattiva il debug dei plugin responsabili",
        ],
    },
    CatRule {
        cat: Cat::Exceptions,
        min: 2.0,
        title: "Creazione di eccezioni in grande quantità",
        explanation: "Creare un'eccezione costa (cattura dello stack trace). Molte eccezioni al secondo indicano un errore continuo o un plugin che usa le eccezioni per il controllo di flusso.",
        solutions: &[
            "Controlla logs/latest.log per errori ripetuti",
            "Aggiorna o segnala il plugin responsabile",
        ],
    },
    CatRule {
        cat: Cat::SyncDatabase,
        min: 1.0,
        title: "Accesso a database sincrono sul main thread",
        explanation: "Il main thread aspetta la risposta del database: ogni query aggiunge millisecondi al tick e, se il database è lento, causa lag spike.",
        solutions: &[
            "Il plugin responsabile deve eseguire le query in asincrono (runTaskAsynchronously / CompletableFuture) e mettere i dati in cache",
            "Controlla la latenza tra server e database (stessa macchina o rete locale)",
        ],
    },
    CatRule {
        cat: Cat::SyncNetworkIo,
        min: 1.0,
        title: "Chiamate di rete (HTTP/socket) sincrone sul main thread",
        explanation: "Il main thread aspetta una risposta via rete (API web, Redis, webhook, controllo licenze). Qualsiasi ritardo di rete diventa lag.",
        solutions: &[
            "Il plugin responsabile deve fare le richieste in asincrono",
            "Aggiorna il plugin o segnala il problema allo sviluppatore",
        ],
    },
    CatRule {
        cat: Cat::SyncFileIo,
        min: 2.0,
        title: "Lettura/scrittura di file sincrona sul main thread",
        explanation: "Il main thread aspetta il disco per salvare o leggere file (config, dati giocatori, YAML/JSON).",
        solutions: &[
            "Il plugin responsabile deve salvare i dati in asincrono e non ricaricare file ad ogni tick",
            "Verifica la velocità del disco",
        ],
    },
    CatRule {
        cat: Cat::Blocking,
        min: 2.0,
        title: "Il main thread resta bloccato in attesa",
        explanation: "Il main thread sta aspettando un lock, un Future (.get()/.join()) o uno sleep fuori dalla normale pausa tra un tick e l'altro. Mentre aspetta il server è fermo.",
        solutions: &[
            "Il plugin responsabile non deve chiamare .get()/.join() su CompletableFuture o Thread.sleep sul main thread",
            "Usa callback (thenAccept) e torna sul main thread con il scheduler",
        ],
    },
    CatRule {
        cat: Cat::Serialization,
        min: 3.0,
        title: "Serializzazione JSON/YAML pesante sul main thread",
        explanation: "Conversioni continue da/verso JSON o YAML (es. salvataggi di dati, config ricaricate) consumano CPU sul main thread.",
        solutions: &[
            "Mantieni i dati in memoria e serializza solo al salvataggio, in asincrono",
            "Evita di ricaricare file di configurazione durante il gioco",
        ],
    },
    CatRule {
        cat: Cat::TextFormatting,
        min: 3.0,
        title: "Formattazione di testo e placeholder costosa",
        explanation: "Parsing di MiniMessage/colori e risoluzione di placeholder ripetuti per ogni giocatore e ogni tick.",
        solutions: &[
            "Aumenta l'intervallo di aggiornamento di scoreboard, tablist e hologram",
            "Metti in cache i componenti già convertiti invece di riparsarli ogni volta",
        ],
    },
];

fn category_rules(c: &Ctx, out: &mut Vec<Finding>) {
    for rule in CAT_RULES {
        let Some(cs) = c.cat(rule.cat) else { continue };
        if cs.pct < rule.min {
            continue;
        }
        let mut solutions: Vec<String> = Vec::new();
        let mut cfg: Vec<Option<String>> = Vec::new();
        match rule.cat {
            Cat::EntityTick | Cat::MobAi => {
                cfg.push(c.spigot("entity-activation-range.animals", "16", gt(16.0)));
                cfg.push(c.spigot("entity-activation-range.monsters", "24", gt(24.0)));
                cfg.push(c.spigot("entity-activation-range.misc", "8", gt(8.0)));
                cfg.push(c.spigot("entity-activation-range.water", "8", gt(8.0)));
                cfg.push(c.spigot("entity-activation-range.villagers", "16", gt(16.0)));
                cfg.push(c.spigot("tick-inactive-villagers", "false", is_bool(true)));
                cfg.push(c.props("simulation-distance", "6", gt(6.0)));
                cfg.push(c.paper(
                    "entities.armor-stands.tick",
                    Some("armor-stands-tick"),
                    "false",
                    is_bool(true),
                ));
                if c.cfg.is_pufferfish() || c.cfg.is_purpur() {
                    cfg.push(c.adv(
                        c.cfg.get("pufferfish.yml", "dab.enabled"),
                        "pufferfish.yml",
                        "dab.enabled",
                        "true",
                        is_bool(false),
                    ));
                }
            }
            Cat::Villagers => {
                if c.cfg.is_purpur() {
                    cfg.push(c.adv(
                        c.cfg.get(
                            "purpur.yml",
                            "world-settings.default.mobs.villager.lobotomize.enabled",
                        ),
                        "purpur.yml",
                        "world-settings.default.mobs.villager.lobotomize.enabled",
                        "true",
                        is_bool(false),
                    ));
                }
                cfg.push(c.spigot("tick-inactive-villagers", "false", is_bool(true)));
                cfg.push(c.spigot("entity-activation-range.villagers", "16", gt(16.0)));
            }
            Cat::Collisions => {
                cfg.push(c.paper(
                    "collisions.max-entity-collisions",
                    Some("max-entity-collisions"),
                    "2",
                    gt(2.0),
                ));
                cfg.push(c.spigot("max-entity-collisions", "2", gt(2.0)));
            }
            Cat::ItemsXp => {
                cfg.push(c.spigot("merge-radius.item", "3.5", lt(3.5)));
                cfg.push(c.spigot("merge-radius.exp", "4.0", lt(4.0)));
            }
            Cat::Hoppers => {
                cfg.push(c.paper(
                    "hopper.disable-move-event",
                    Some("hopper.disable-move-event"),
                    "true (se nessun plugin lo usa)",
                    is_bool(false),
                ));
                cfg.push(c.paper(
                    "hopper.ignore-occluding-blocks",
                    Some("hopper.ignore-occluding-blocks"),
                    "true",
                    is_bool(false),
                ));
                cfg.push(c.spigot("ticks-per.hopper-transfer", "8", lt(8.0)));
                cfg.push(c.spigot("hopper-amount", "8", lt(8.0)));
                cfg.push(c.spigot("ticks-per.hopper-check", "8", lt(8.0)));
            }
            Cat::Spawners => {
                cfg.push(c.paper(
                    "tick-rates.mob-spawner",
                    Some("mob-spawner-tick-rate"),
                    "2",
                    lt(2.0),
                ));
                cfg.push(c.spigot("nerf-spawner-mobs", "true", is_bool(false)));
            }
            Cat::Redstone | Cat::ScheduledTicks => {
                cfg.push(c.paper(
                    "misc.redstone-implementation",
                    Some("redstone-implementation"),
                    "ALTERNATE_CURRENT",
                    neq_text("ALTERNATE_CURRENT"),
                ));
            }
            Cat::ChunkTick => {
                cfg.push(c.props("simulation-distance", "6", gt(6.0)));
            }
            Cat::MobSpawning => {
                cfg.push(c.paper(
                    "entities.spawning.per-player-mob-spawns",
                    Some("per-player-mob-spawns"),
                    "true",
                    is_bool(false),
                ));
                cfg.push(c.bukkit("ticks-per.monster-spawns", "10", lt(10.0)));
                cfg.push(c.bukkit("spawn-limits.monsters", "50", gt(50.0)));
                cfg.push(c.bukkit("spawn-limits.animals", "8", gt(10.0)));
                cfg.push(c.bukkit("spawn-limits.water-animals", "3", gt(5.0)));
                cfg.push(c.bukkit("spawn-limits.ambient", "1", gt(3.0)));
                cfg.push(c.spigot("mob-spawn-range", "6", gt(6.0)));
            }
            Cat::ChunkLoadSync | Cat::ChunkGen => {
                cfg.push(c.paper(
                    "chunks.prevent-moving-into-unloaded-chunks",
                    Some("prevent-moving-into-unloaded-chunks"),
                    "true",
                    is_bool(false),
                ));
                let treasure = c.method_pct(|m| {
                    m.contains("findNearestMapStructure")
                        || m.contains("findNearestMapFeature")
                        || m.contains("locateStructure")
                        || m.contains("ExplorationMapFunction")
                });
                if treasure >= 0.5 {
                    solutions.push(format!("Le ricerche di strutture (mappe del tesoro/delfini/eye of ender) pesano {}%: imposta environment.treasure-maps.find-already-discovered.villager-trade e loot-tables a true, o disattiva environment.treasure-maps.enabled", f(treasure)));
                }
            }
            Cat::ChunkSave => {
                cfg.push(c.paper(
                    "chunks.max-auto-save-chunks-per-tick",
                    Some("max-auto-save-chunks-per-tick"),
                    "8",
                    gt(8.0),
                ));
                cfg.push(c.props("sync-chunk-writes", "false", is_bool(true)));
            }
            Cat::ChunkSystem => {
                cfg.push(c.props("view-distance", "8", gt(10.0)));
                cfg.push(c.paper(
                    "chunks.delay-chunk-unloads-by",
                    Some("delay-chunk-unloads-by"),
                    "10s",
                    |v: &Cfg| v.text().starts_with('0'),
                ));
            }
            Cat::EntityTracking => {
                cfg.push(c.spigot("entity-tracking-range.players", "48", gt(48.0)));
                cfg.push(c.spigot("entity-tracking-range.animals", "32", gt(32.0)));
                cfg.push(c.spigot("entity-tracking-range.monsters", "32", gt(32.0)));
                cfg.push(c.spigot("entity-tracking-range.misc", "16", gt(16.0)));
                cfg.push(c.props("entity-broadcast-range-percentage", "75", gt(100.0)));
            }
            Cat::Players => {
                cfg.push(c.props("view-distance", "8", gt(10.0)));
            }
            Cat::Network => {
                cfg.push(c.props("network-compression-threshold", "256", |v: &Cfg| {
                    v.num().map(|n| n < 64.0).unwrap_or(false)
                }));
            }
            Cat::Explosions => {
                cfg.push(c.paper(
                    "environment.optimize-explosions",
                    Some("optimize-explosions"),
                    "true",
                    is_bool(false),
                ));
            }
            _ => {}
        }
        let cfg_lines: Vec<String> = cfg.into_iter().flatten().collect();
        solutions.extend(rule.solutions.iter().map(|s| s.to_string()));
        if !cfg_lines.is_empty() {
            solutions.push(format!(
                "Configurazione da rivedere:\n    - {}",
                cfg_lines.join("\n    - ")
            ));
        }
        let mut evidence = c.cat_evidence(cs);
        if rule.cat == Cat::EntityTick || rule.cat == Cat::MobAi {
            let ents: Vec<String> =
                c.a.entity_types
                    .iter()
                    .take(6)
                    .filter(|e| e.pct >= 0.5)
                    .map(|e| format!("{} {}%", e.name, f(e.pct)))
                    .collect();
            if !ents.is_empty() {
                evidence.push(format!("Tipi di entità più costosi: {}", ents.join(", ")));
            }
        }
        if rule.cat == Cat::BlockEntities {
            let bes: Vec<String> =
                c.a.block_entity_types
                    .iter()
                    .take(6)
                    .filter(|e| e.pct >= 0.3)
                    .map(|e| format!("{} {}%", e.name, f(e.pct)))
                    .collect();
            if !bes.is_empty() {
                evidence.push(format!(
                    "Tipi di block entity più costosi: {}",
                    bes.join(", ")
                ));
            }
        }
        let hazard = rule.cat.is_sync_hazard();
        let severity = if hazard && cs.pct >= 1.0 && rule.cat != Cat::ChunkGen {
            if c.lagging {
                Severity::Critico
            } else {
                Severity::Alto
            }
        } else {
            c.sev(cs.pct)
        };
        out.push(Finding {
            severity,
            area: "Main thread".into(),
            title: rule.title.into(),
            impact_pct: cs.pct,
            evidence,
            explanation: rule.explanation.into(),
            solutions,
            culprits: c.culprits(cs),
        });
    }
}

fn plugin_rules(c: &Ctx, out: &mut Vec<Finding>) {
    for s in &c.a.sources {
        if s.incl_pct < 5.0 && s.hazards.iter().all(|h| h.pct < 1.0) {
            continue;
        }
        let name = c.p.sources.display_name(&s.source);
        let mut ev = vec![format!(
            "{} è nello stack per il {}% del tempo attivo del main thread ({}), di cui {}% nel suo codice o in chiamate fatte direttamente",
            name,
            f(s.incl_pct),
            c.ms(s.incl_time),
            f(s.self_pct)
        )];
        let entries: Vec<String> = s
            .top_entries
            .iter()
            .take(5)
            .filter(|e| e.pct >= 0.3)
            .map(|e| format!("{} [{}] {}%", short(&e.entry), e.context, f(e.pct)))
            .collect();
        if !entries.is_empty() {
            ev.push(format!("Punti di ingresso: {}", entries.join("; ")));
        }
        let ms: Vec<String> = s
            .top_methods
            .iter()
            .take(4)
            .filter(|m| m.pct >= 0.3)
            .map(|m| format!("{} {}%", short(&m.name), f(m.pct)))
            .collect();
        if !ms.is_empty() {
            ev.push(format!("Dove spende il tempo: {}", ms.join(", ")));
        }
        let mut solutions = Vec::new();
        for h in &s.hazards {
            if h.pct < 0.5 {
                continue;
            }
            ev.push(format!("⚠ {}: {}% del main thread", h.name, f(h.pct)));
            solutions.push(format!("{}: il plugin esegue «{}» sul main thread; va spostato in un task asincrono o messo in cache", name, h.name.to_lowercase()));
        }
        let from_events = s
            .top_entries
            .iter()
            .any(|e| e.context.starts_with("Evento") && e.pct >= 1.0);
        let from_tasks = s
            .top_entries
            .iter()
            .any(|e| e.context == "Task schedulato" && e.pct >= 1.0);
        if from_events {
            solutions.push("Ottimizza i listener indicati: controlla prima le condizioni economiche e usa ignoreCancelled/priorità corrette".into());
        }
        if from_tasks {
            solutions.push("Riduci la frequenza dei suoi task ripetuti o spostali in asincrono se non toccano il mondo".into());
        }
        solutions.push(format!("Aggiorna {name} all'ultima versione e controlla la sua configurazione (intervalli di aggiornamento, funzioni non usate da disattivare)"));
        solutions.push("Se il costo non è giustificato dalla funzione che offre, valuta un'alternativa più leggera o segnala il profilo allo sviluppatore".into());
        let hazard_pct: f64 = s
            .hazards
            .iter()
            .filter(|h| {
                h.name == Cat::SyncDatabase.label()
                    || h.name == Cat::SyncNetworkIo.label()
                    || h.name == Cat::SyncFileIo.label()
                    || h.name == Cat::Blocking.label()
                    || h.name == Cat::ChunkLoadSync.label()
            })
            .map(|h| h.pct)
            .sum();
        let severity = if hazard_pct >= 2.0 {
            if c.lagging {
                Severity::Critico
            } else {
                Severity::Alto
            }
        } else {
            c.sev(s.incl_pct)
        };
        out.push(Finding {
            severity,
            area: "Plugin/Mod".into(),
            title: format!("{name} pesa sul main thread"),
            impact_pct: s.incl_pct,
            evidence: ev,
            explanation: "La percentuale «nello stack» include tutto il lavoro innescato dal plugin (anche chiamate a Minecraft/Bukkit fatte dal plugin). È il costo che sparirebbe se il plugin non facesse quel lavoro.".into(),
            solutions,
            culprits: vec![name],
        });
    }
    let heavy_events: Vec<String> =
        c.a.events
            .iter()
            .take(5)
            .filter(|e| e.pct >= 2.0)
            .map(|e| {
                format!(
                    "{} → {} ({}%)",
                    e.entry,
                    c.p.sources.display_name(&e.source),
                    f(e.pct)
                )
            })
            .collect();
    if !heavy_events.is_empty() {
        out.push(Finding {
            severity: Severity::Medio,
            area: "Plugin/Mod".into(),
            title: "Eventi Bukkit più costosi".into(),
            impact_pct: c.a.events.first().map(|e| e.pct).unwrap_or(0.0),
            evidence: heavy_events,
            explanation: "Questi eventi, gestiti dai plugin indicati, occupano una parte significativa del main thread.".into(),
            solutions: vec![
                "PlayerMoveEvent: esci subito se il giocatore non ha cambiato blocco (confronta getFrom/getTo a livello di blocco)".into(),
                "BlockPhysicsEvent / InventoryMoveItemEvent: sono chiamati migliaia di volte al tick, i listener devono essere minimi".into(),
            ],
            culprits: Vec::new(),
        });
    }
}

fn spike_rules(c: &Ctx, out: &mut Vec<Finding>) {
    if c.a.spikes.is_empty() {
        return;
    }
    let mut ev = Vec::new();
    for s in c.a.spikes.iter().take(5) {
        let causes: Vec<String> = s
            .cat_deltas
            .iter()
            .take(3)
            .filter(|d| d.delta > 0.0)
            .map(|d| format!("{} +{} {}", d.name, f(d.delta), s.unit))
            .collect();
        let owners: Vec<String> = s
            .owner_deltas
            .iter()
            .take(2)
            .filter(|d| d.delta > 0.0)
            .map(|d| format!("{} +{}", c.p.sources.display_name(&d.name), f(d.delta)))
            .collect();
        let mut line = format!("{}: {}", window_label(&s.window), s.reason);
        if !causes.is_empty() {
            line.push_str(&format!(" — crescita: {}", causes.join(", ")));
        }
        if !owners.is_empty() {
            line.push_str(&format!("; plugin: {}", owners.join(", ")));
        }
        ev.push(line);
    }
    if let Some(p) = &c.a.periodic_spikes {
        ev.push(p.clone());
    }
    let n = c.a.spikes.len();
    let total = c.a.windows.len();
    let mut solutions = vec!["Confronta le categorie in crescita con le sezioni dedicate qui sotto: sono la causa più probabile del picco".to_string()];
    if c.a.periodic_spikes.is_some() {
        solutions.push("Picchi periodici: controlla autosave (chunks.max-auto-save-chunks-per-tick), backup automatici e task dei plugin con timer fisso".into());
    }
    solutions.push("Per isolare meglio i picchi registra un profilo con /spark profiler start --only-ticks-over 100 --timeout 600".into());
    out.push(Finding {
        severity: if n * 3 >= total { Severity::Alto } else { Severity::Medio },
        area: "Picchi di lag".into(),
        title: format!("{n} finestre su {total} con lag spike"),
        impact_pct: 0.0,
        evidence: ev,
        explanation: "Ogni finestra è circa un minuto di profilo. Il confronto con le finestre normali mostra cosa è cresciuto durante il picco.".into(),
        solutions,
        culprits: Vec::new(),
    });
}

pub fn window_label(w: &crate::analysis::WindowStat) -> String {
    if w.start_time > 0 {
        let secs = w.start_time / 1000;
        let h = (secs / 3600) % 24;
        let m = (secs / 60) % 60;
        format!("finestra #{} ({:02}:{:02} UTC)", w.index + 1, h, m)
    } else {
        format!("finestra #{}", w.index + 1)
    }
}

fn thread_rules(c: &Ctx, out: &mut Vec<Finding>) {
    let main_busy = c.a.main.busy;
    for t in &c.a.threads {
        if t.name == c.a.main.thread || main_busy <= 0.0 {
            continue;
        }
        let ratio = t.busy / c.a.main.total.max(1.0);
        if ratio < 0.6 {
            continue;
        }
        let tops: Vec<String> = t
            .top_methods
            .iter()
            .map(|m| format!("{} {}%", m.name, f(m.pct)))
            .collect();
        let mut ev = vec![format!(
            "Thread «{}» occupato per il {}% della durata del profilo",
            t.name,
            f(ratio * 100.0)
        )];
        if !tops.is_empty() {
            ev.push(format!("Lavoro principale: {}", tops.join(", ")));
        }
        if let Some(o) = &t.top_owner {
            ev.push(format!("Codice di: {}", c.p.sources.display_name(o)));
        }
        let kind = t
            .plugin_hint
            .clone()
            .unwrap_or_else(|| "thread secondario".into());
        out.push(Finding {
            severity: Severity::Basso,
            area: "Thread secondari".into(),
            title: format!("Thread sempre occupato: {} ({kind})", t.name),
            impact_pct: ratio * 100.0,
            evidence: ev,
            explanation: "Un thread secondario sempre al lavoro non blocca direttamente il tick, ma consuma un core: se la CPU è limitata ruba tempo al main thread.".into(),
            solutions: vec!["Verifica che il lavoro sia necessario (task asincroni in loop, generazione chunk continua, rete)".into(), "Se la CPU è satura, riduci questo carico o aumenta i core disponibili".into()],
            culprits: t.top_owner.iter().map(|o| c.p.sources.display_name(o)).collect(),
        });
    }
}

fn mode_note(c: &Ctx, out: &mut Vec<Finding>) {
    let (title, expl) = match c.p.mode {
        Mode::Allocation => (
            "Profilo di allocazione memoria",
            "Questo profilo misura i byte allocati, non il tempo CPU. Le percentuali indicano chi crea più oggetti: allocazioni elevate causano GC frequenti.",
        ),
        _ => (
            "Profilo di contesa dei lock",
            "Questo profilo misura il tempo passato ad aspettare lock. Le percentuali indicano dove i thread si bloccano a vicenda.",
        ),
    };
    let mut ev = Vec::new();
    for s in c.a.sources.iter().take(5) {
        ev.push(format!(
            "{}: {}%",
            c.p.sources.display_name(&s.source),
            f(s.incl_pct)
        ));
    }
    for m in c.a.methods.iter().take(5) {
        ev.push(format!("{}: {}%", short(&m.method), f(m.self_pct)));
    }
    out.push(Finding {
        severity: Severity::Info,
        area: "Profilo".into(),
        title: title.into(),
        impact_pct: 0.0,
        evidence: ev,
        explanation: expl.into(),
        solutions: vec![
            "Per trovare la causa del lag usa un profilo CPU normale: /spark profiler start".into(),
        ],
        culprits: Vec::new(),
    });
}

pub struct JvmArgs {
    pub xmx: Option<u64>,
    pub xms: Option<u64>,
    pub gc: Option<String>,
    pub aikar: bool,
    pub raw: String,
}

fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num, mult) = match s.chars().last()? {
        'g' | 'G' => (&s[..s.len() - 1], 1u64 << 30),
        'm' | 'M' => (&s[..s.len() - 1], 1u64 << 20),
        'k' | 'K' => (&s[..s.len() - 1], 1u64 << 10),
        _ => (s, 1),
    };
    num.parse::<u64>().ok().map(|n| n * mult)
}

pub fn parse_jvm_args(raw: &str) -> JvmArgs {
    let mut a = JvmArgs {
        xmx: None,
        xms: None,
        gc: None,
        aikar: false,
        raw: raw.to_string(),
    };
    for tok in raw.split_whitespace() {
        if let Some(v) = tok.strip_prefix("-Xmx") {
            a.xmx = parse_size(v);
        } else if let Some(v) = tok.strip_prefix("-Xms") {
            a.xms = parse_size(v);
        } else if let Some(v) = tok.strip_prefix("-XX:MaxRAMPercentage=") {
            let _ = v;
        } else if tok.contains("+UseG1GC") {
            a.gc = Some("G1".into());
        } else if tok.contains("+UseZGC") {
            a.gc = Some("ZGC".into());
        } else if tok.contains("+UseShenandoahGC") {
            a.gc = Some("Shenandoah".into());
        } else if tok.contains("+UseParallelGC") {
            a.gc = Some("Parallel".into());
        } else if tok.contains("+UseSerialGC") {
            a.gc = Some("Serial".into());
        } else if tok.contains("+UseConcMarkSweepGC") {
            a.gc = Some("CMS".into());
        }
    }
    a.aikar = raw.contains("G1NewSizePercent") && raw.contains("MaxGCPauseMillis")
        || raw.contains("aikars.new.flags");
    a
}

fn gb(b: u64) -> String {
    format!("{:.1} GB", b as f64 / (1u64 << 30) as f64)
}

fn jvm_rules(
    sys: &Option<crate::proto::SystemStatistics>,
    ps: Option<&crate::proto::PlatformStatistics>,
    lagging: bool,
    out: &mut Vec<Finding>,
) {
    let Some(sys) = sys.as_ref() else { return };
    let java = sys.java.clone().unwrap_or_default();
    let args = parse_jvm_args(&java.vm_args);
    let mut ev = Vec::new();
    let mut sol = Vec::new();
    let mut worst = Severity::Info;
    let bump = |s: Severity, w: &mut Severity| {
        if s < *w {
            *w = s;
        }
    };

    let major: Option<u32> = java
        .version
        .split(['.', '+', '-'])
        .next()
        .and_then(|v| v.parse().ok())
        .map(|v: u32| if v == 1 { 8 } else { v });
    if let Some(m) = major {
        ev.push(format!(
            "Java {} ({})",
            java.version,
            if java.vendor_version.is_empty() {
                &java.vendor
            } else {
                &java.vendor_version
            }
        ));
        if m < 21 {
            sol.push("Aggiorna a Java 21 o più recente: GC e JIT sono nettamente migliori (richiesto da Minecraft 1.20.5+)".into());
            bump(Severity::Medio, &mut worst);
        }
    }
    let phys = sys
        .memory
        .as_ref()
        .and_then(|m| m.physical.as_ref())
        .map(|p| (p.used as u64, p.total as u64));
    let swap = sys
        .memory
        .as_ref()
        .and_then(|m| m.swap.as_ref())
        .map(|p| (p.used as u64, p.total as u64));
    if let Some(x) = args.xmx {
        ev.push(format!("Heap massimo (-Xmx): {}", gb(x)));
        if let Some((_, total)) = phys
            && total > 0
            && x as f64 > total as f64 * 0.9
        {
            sol.push(format!("-Xmx ({}) è quasi tutta la RAM della macchina ({}): lascia almeno 1-2 GB al sistema per evitare swap", gb(x), gb(total)));
            bump(Severity::Alto, &mut worst);
        }
        if x < (3u64 << 30) {
            sol.push(
                "Heap sotto i 3 GB: con più giocatori il GC sarà molto frequente; valuta 6-10 GB"
                    .into(),
            );
            bump(Severity::Medio, &mut worst);
        }
        if let Some(s) = args.xms
            && s != x
        {
            sol.push(
                "Imposta -Xms uguale a -Xmx: evita che la JVM ridimensioni l'heap durante il gioco"
                    .into(),
            );
            bump(Severity::Basso, &mut worst);
        }
    }
    match args.gc.as_deref() {
        Some("Parallel") | Some("Serial") | Some("CMS") => {
            sol.push(format!("GC {} non adatto a un server di gioco: usa G1 con i flag di Aikar o ZGC generazionale", args.gc.clone().unwrap_or_default()));
            bump(Severity::Alto, &mut worst);
        }
        Some("G1") | None if !args.aikar => {
            sol.push("Usa i flag di Aikar per G1 (vedi docs.papermc.io → Aikar's flags) oppure, con heap ≥ 12 GB e Java 21+, -XX:+UseZGC -XX:+ZGenerational".into());
            bump(Severity::Basso, &mut worst);
        }
        _ => {}
    }
    if let Some(g) = &args.gc {
        ev.push(format!(
            "Garbage collector: {g}{}",
            if args.aikar { " (flag di Aikar)" } else { "" }
        ));
    }
    if let Some((used, total)) = swap
        && used > (256u64 << 20)
    {
        ev.push(format!("Swap in uso: {} su {}", gb(used), gb(total)));
        sol.push("La macchina sta usando lo swap: riduci la RAM assegnata o aggiungi memoria, lo swap causa pause enormi del GC".into());
        bump(Severity::Alto, &mut worst);
    }
    if let Some(cpu) = sys.cpu.as_ref() {
        let proc1 = cpu.process_usage.as_ref().map(|u| u.last1m).unwrap_or(0.0);
        let sys1 = cpu.system_usage.as_ref().map(|u| u.last1m).unwrap_or(0.0);
        ev.push(format!(
            "CPU: {} thread{}, processo {}%, sistema {}%",
            cpu.threads,
            if cpu.model_name.is_empty() {
                String::new()
            } else {
                format!(" ({})", cpu.model_name)
            },
            f(proc1 * 100.0),
            f(sys1 * 100.0)
        ));
        if sys1 - proc1 > 0.3 {
            sol.push("Altri processi sulla macchina usano molta CPU (CPU di sistema molto più alta di quella del server): spostali o verifica un host condiviso sovraccarico".into());
            bump(
                if lagging {
                    Severity::Alto
                } else {
                    Severity::Medio
                },
                &mut worst,
            );
        }
        if proc1 > 0.85 {
            sol.push("La CPU è quasi satura: serve ridurre il carico o un processore più veloce (Minecraft dipende dalla velocità single-core)".into());
            bump(Severity::Alto, &mut worst);
        }
        if cpu.threads > 0 && cpu.threads <= 2 {
            sol.push("Solo 1-2 core disponibili: GC, rete e chunk competono con il main thread; consigliati almeno 4 core".into());
            bump(Severity::Medio, &mut worst);
        }
    }
    let gc_map = ps.map(|p| p.gc.clone()).unwrap_or_default();
    let mut gc_ev = Vec::new();
    for (name, g) in &gc_map {
        if g.total <= 0 {
            continue;
        }
        gc_ev.push(format!(
            "{name}: {} raccolte, media {} ms, ogni {} s",
            g.total,
            f(g.avg_time),
            f(g.avg_frequency / 1000.0)
        ));
        let lname = name.to_ascii_lowercase();
        let is_old = lname.contains("old")
            || lname.contains("full")
            || lname.contains("mark") && !lname.contains("young");
        if is_old && g.total >= 2 && g.avg_frequency > 0.0 && g.avg_frequency < 600_000.0 {
            sol.push(format!("Raccolte «{name}» frequenti: l'heap è quasi pieno (troppi dati in memoria, memory leak o heap troppo piccolo). Fai /spark heapsummary per vedere cosa occupa memoria"));
            bump(Severity::Alto, &mut worst);
        }
        if !is_old && g.avg_time > 60.0 {
            sol.push(format!(
                "Pause «{name}» lunghe ({} ms in media): applica i flag di Aikar o passa a ZGC",
                f(g.avg_time)
            ));
            bump(Severity::Medio, &mut worst);
        }
        if !is_old && g.avg_frequency > 0.0 && g.avg_frequency < 1500.0 {
            sol.push(format!("«{name}» ogni {} s: molte allocazioni. Un profilo /spark profiler start --alloc mostra chi alloca di più", f(g.avg_frequency / 1000.0)));
            bump(Severity::Basso, &mut worst);
        }
    }
    ev.extend(gc_ev);
    if let Some(mem) = ps
        .and_then(|p| p.memory.as_ref())
        .and_then(|m| m.heap.as_ref())
        && mem.max > 0
    {
        let pct = mem.used as f64 / mem.max as f64 * 100.0;
        ev.push(format!(
            "Heap in uso: {} su {} ({}%)",
            gb(mem.used as u64),
            gb(mem.max as u64),
            f(pct)
        ));
    }
    if ev.is_empty() {
        return;
    }
    let title = if sol.is_empty() {
        "JVM e macchina: nessun problema evidente".to_string()
    } else {
        "JVM, memoria e CPU da rivedere".to_string()
    };
    out.push(Finding {
        severity: if sol.is_empty() {
            Severity::Info
        } else {
            worst
        },
        area: "JVM / Macchina".into(),
        title,
        impact_pct: 0.0,
        evidence: ev,
        explanation: "Impostazioni della JVM e stato della macchina al momento del profilo.".into(),
        solutions: sol,
        culprits: Vec::new(),
    });
}

pub struct ChunkHot {
    pub world: String,
    pub x: i32,
    pub z: i32,
    pub total: i32,
    pub top: Vec<(String, i32)>,
}

pub fn hot_chunks(ps: Option<&crate::proto::PlatformStatistics>, limit: usize) -> Vec<ChunkHot> {
    let mut out = Vec::new();
    let Some(world) = ps.and_then(|p| p.world.as_ref()) else {
        return out;
    };
    for w in &world.worlds {
        for r in &w.regions {
            for ch in &r.chunks {
                let mut top: Vec<(String, i32)> = ch
                    .entity_counts
                    .iter()
                    .map(|(k, v)| (k.trim_start_matches("minecraft:").to_string(), *v))
                    .collect();
                top.sort_by(|a, b| b.1.cmp(&a.1));
                top.truncate(3);
                out.push(ChunkHot {
                    world: w.name.clone(),
                    x: ch.x,
                    z: ch.z,
                    total: ch.total_entities,
                    top,
                });
            }
        }
    }
    out.sort_by(|a, b| b.total.cmp(&a.total));
    out.truncate(limit);
    out
}

fn world_rules(ps: Option<&crate::proto::PlatformStatistics>, out: &mut Vec<Finding>) {
    let Some(world) = ps.and_then(|p| p.world.as_ref()) else {
        return;
    };
    let mut ev = vec![format!("Entità totali: {}", world.total_entities)];
    let mut counts: Vec<(&String, &i32)> = world.entity_counts.iter().collect();
    counts.sort_by(|a, b| b.1.cmp(a.1));
    let top: Vec<String> = counts
        .iter()
        .take(8)
        .map(|(k, v)| format!("{} {}", k.trim_start_matches("minecraft:"), v))
        .collect();
    if !top.is_empty() {
        ev.push(format!("Per tipo: {}", top.join(", ")));
    }
    let hot = hot_chunks(ps, 8);
    let crowded: Vec<&ChunkHot> = hot.iter().filter(|h| h.total >= 60).collect();
    for h in crowded.iter().take(6) {
        let types: Vec<String> = h.top.iter().map(|(k, v)| format!("{k} {v}")).collect();
        ev.push(format!("Chunk affollato in {} a x={} z={} (blocchi): {} entità — {} → /execute in {} run tp {} 100 {}", h.world, h.x * 16 + 8, h.z * 16 + 8, h.total, types.join(", "), dim_name(&h.world), h.x * 16 + 8, h.z * 16 + 8));
    }
    let mut sol = Vec::new();
    let mut sev = Severity::Info;
    if !crowded.is_empty() {
        sol.push("Visita i chunk elencati e riduci le entità (farm, allevamenti, item a terra, armor stand)".into());
        sol.push("Imposta chunks.entity-per-chunk-save-limit in paper-world-defaults.yml (es. experience_orb: 16, arrow: 16, snowball: 8) per evitare accumuli".into());
        sev = if crowded.iter().any(|h| h.total >= 200) {
            Severity::Alto
        } else {
            Severity::Medio
        };
    }
    for gr in &world.game_rules {
        let vals: Vec<&String> = gr.world_values.values().collect();
        match gr.name.as_str() {
            "randomTickSpeed" => {
                if vals
                    .iter()
                    .any(|v| v.parse::<i32>().map(|n| n > 3).unwrap_or(false))
                {
                    ev.push(format!(
                        "Gamerule randomTickSpeed sopra il default: {:?}",
                        gr.world_values
                    ));
                    sol.push("Riporta randomTickSpeed a 3: valori più alti moltiplicano il costo dei random tick".into());
                    if sev > Severity::Medio {
                        sev = Severity::Medio;
                    }
                }
            }
            "maxEntityCramming"
                if vals
                    .iter()
                    .any(|v| v.parse::<i32>().map(|n| n > 24 || n == 0).unwrap_or(false)) =>
            {
                ev.push(format!("Gamerule maxEntityCramming: {:?}", gr.world_values));
                sol.push("maxEntityCramming alto o disattivato permette ammassi enormi di mob: riportalo a 24 o meno".into());
            }
            _ => {}
        }
    }
    let dp: Vec<&str> = world
        .data_packs
        .iter()
        .filter(|d| !d.builtin)
        .map(|d| d.name.as_str())
        .collect();
    if !dp.is_empty() {
        ev.push(format!("Datapack non di base: {}", dp.join(", ")));
    }
    out.push(Finding {
        severity: sev,
        area: "Mondo".into(),
        title: if crowded.is_empty() { "Statistiche del mondo".into() } else { format!("{} chunk con troppe entità", crowded.len()) },
        impact_pct: 0.0,
        evidence: ev,
        explanation: "Conteggi presi da spark al momento del profilo. Un singolo chunk con centinaia di entità può da solo causare lag.".into(),
        solutions: sol,
        culprits: Vec::new(),
    });
}

fn dim_name(world: &str) -> String {
    let w = world.to_ascii_lowercase();
    if w.ends_with("_nether") {
        "minecraft:the_nether".into()
    } else if w.ends_with("_the_end") {
        "minecraft:the_end".into()
    } else {
        format!(
            "minecraft:{}",
            if w == "world" { "overworld" } else { world }
        )
    }
}

fn config_rules(c: &Ctx, out: &mut Vec<Finding>) {
    if c.cfg.is_empty() {
        return;
    }
    let mut items: Vec<Option<String>> = Vec::new();
    items.push(c.props(
        "sync-chunk-writes",
        "false (scritture chunk sincrone rallentano il salvataggio)",
        is_bool(true),
    ));
    items.push(c.props("view-distance", "8-10", gt(12.0)));
    items.push(c.props("simulation-distance", "6-8", gt(10.0)));
    items.push(c.props("network-compression-threshold", "256", |v: &Cfg| {
        v.num().map(|n| (0.0..64.0).contains(&n)).unwrap_or(false)
    }));
    if c.cfg.is_paper() {
        items.push(c.paper(
            "chunks.max-auto-save-chunks-per-tick",
            Some("max-auto-save-chunks-per-tick"),
            "8",
            gt(24.0),
        ));
        items.push(c.paper(
            "chunks.prevent-moving-into-unloaded-chunks",
            Some("prevent-moving-into-unloaded-chunks"),
            "true",
            is_bool(false),
        ));
        items.push(c.paper(
            "misc.redstone-implementation",
            Some("redstone-implementation"),
            "ALTERNATE_CURRENT",
            |v: &Cfg| v.text().eq_ignore_ascii_case("VANILLA"),
        ));
        items.push(c.paper(
            "environment.optimize-explosions",
            Some("optimize-explosions"),
            "true",
            is_bool(false),
        ));
        items.push(c.paper(
            "collisions.max-entity-collisions",
            Some("max-entity-collisions"),
            "2-4",
            gt(8.0),
        ));
    }
    if c.cfg.has("spigot.yml") {
        items.push(c.spigot("merge-radius.item", "3.5", |v: &Cfg| {
            v.num().map(|n| n < 1.0).unwrap_or(false)
        }));
        items.push(c.spigot("merge-radius.exp", "4.0", |v: &Cfg| {
            v.num().map(|n| n < 1.0).unwrap_or(false)
        }));
        items.push(c.spigot("entity-activation-range.monsters", "24", gt(32.0)));
        items.push(c.spigot("nerf-spawner-mobs", "true", is_bool(false)));
    }
    if c.cfg.has("bukkit.yml") {
        items.push(c.bukkit("ticks-per.monster-spawns", "4-10", |v: &Cfg| {
            v.num().map(|n| n <= 1.0).unwrap_or(false)
        }));
    }
    let lines: Vec<String> = items.into_iter().flatten().collect();
    let mut ev = vec![format!(
        "File letti dal profilo: {}",
        c.cfg.file_names().join(", ")
    )];
    if lines.is_empty() {
        ev.push("Nessuna impostazione problematica tra quelle controllate".into());
    }
    let mut sol = Vec::new();
    push_some(&mut sol, lines.into_iter().map(Some).collect());
    out.push(Finding {
        severity: if sol.is_empty() { Severity::Info } else { Severity::Basso },
        area: "Configurazione".into(),
        title: if sol.is_empty() { "Configurazione server".into() } else { format!("{} impostazioni da ottimizzare", sol.len()) },
        impact_pct: 0.0,
        evidence: ev,
        explanation: "Valori letti dalla configurazione inclusa nel profilo. I consigli sono un punto di partenza sicuro; i valori legati alle categorie pesanti sono indicati anche nelle sezioni del main thread.".into(),
        solutions: sol,
        culprits: Vec::new(),
    });
}
