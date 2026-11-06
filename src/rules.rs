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
            Severity::Critico => crate::i18n::t("CRITICO", "CRITICAL"),
            Severity::Alto => crate::i18n::t("ALTO", "HIGH"),
            Severity::Medio => crate::i18n::t("MEDIO", "MEDIUM"),
            Severity::Basso => crate::i18n::t("BASSO", "LOW"),
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
                tf!(
                    "{} ms/tick",
                    "{} ms/tick",
                    f(time / self.a.main.ticks as f64)
                )
            }
            _ => format!("{} {}", f(time), self.a.unit),
        }
    }

    fn cat_evidence(&self, c: &CatStat) -> Vec<String> {
        let mut ev = vec![tf!(
            "{}: {}% del tempo attivo del main thread ({})",
            "{}: {}% of main thread active time ({})",
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
            ev.push(tf!(
                "Metodi più pesanti: {}",
                "Heaviest methods: {}",
                ms.join(", ")
            ));
        }
        if !c.example_stack.is_empty() {
            ev.push(tf!(
                "Stack tipico: {}",
                "Typical stack: {}",
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
                tf!(
                    "{} ({}% del main thread in questa categoria)",
                    "{} ({}% of the main thread in this category)",
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
                    Some(tf!(
                        "{} → consigliato {}",
                        "{} → recommended {}",
                        x.describe(),
                        rec
                    ))
                } else {
                    None
                }
            }
            None => {
                let file_known = self.cfg.has(file)
                    || (file.contains("paper-world-defaults") && self.cfg.has("paper/"))
                    || (file.contains("paper-global") && self.cfg.has("paper/"));
                if self.cfg.is_empty() || !file_known {
                    Some(tf!(
                        "{file} › {path}: verifica e imposta {rec}",
                        "{file} › {path}: check and set {rec}"
                    ))
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
    if p.meta.interval > 0 && a.main.total <= 0.0 {
        let mut ev = vec![tf!(
            "Thread registrati: {}",
            "Recorded threads: {}",
            p.threads.iter().filter(|t| t.total > 0.0).count()
        )];
        if let Some(th) = p.ticked_threshold() {
            ev.push(tf!("Il profilo è stato avviato con --only-ticks-over {th}: nessun tick ha superato {th} ms durante la registrazione", "The profile was started with --only-ticks-over {th}: no tick exceeded {th} ms while recording"));
        }
        if let Some(td) = p.meta.thread_dumper.as_ref() {
            if !td.patterns.is_empty() || !td.ids.is_empty() {
                ev.push(tf!(
                    "Filtro thread attivo: {:?} {:?}",
                    "Active thread filter: {:?} {:?}",
                    td.patterns,
                    td.ids
                ));
            }
        }
        out.push(Finding {
            severity: Severity::Info,
            area: crate::i18n::t("Panoramica", "Overview").into(),
            title: crate::i18n::t("Il profilo non contiene campioni del main thread", "The profile contains no main thread samples").into(),
            impact_pct: 0.0,
            evidence: ev,
            explanation: crate::i18n::t("Senza campioni non è possibile dire dove va il tempo. Restano valide le sezioni su JVM, mondo e configurazione.", "Without samples it is not possible to tell where the time goes. The JVM, world and configuration sections are still valid.").into(),
            solutions: vec![
                crate::i18n::t("Se hai usato --only-ticks-over, abbassa la soglia (es. 60) o registra mentre il lag è in corso", "If you used --only-ticks-over, lower the threshold (e.g. 60) or record while the lag is happening").into(),
                crate::i18n::t("Per un profilo completo usa /spark profiler start --timeout 300 senza filtri", "For a full profile use /spark profiler start --timeout 300 without filters").into(),
            ],
            culprits: Vec::new(),
        });
    }
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
        let px = x.area == crate::i18n::t("Panoramica", "Overview");
        let py = y.area == crate::i18n::t("Panoramica", "Overview");
        py.cmp(&px)
            .then(x.severity.cmp(&y.severity))
            .then(y.impact_pct.total_cmp(&x.impact_pct))
    });
    out
}

fn overview(c: &Ctx, out: &mut Vec<Finding>) {
    let mut ev = Vec::new();
    if let Some(t) = c.tps {
        ev.push(tf!("TPS ultimo minuto: {}", "TPS last minute: {}", f(t)));
    }
    if let Some(m) = c.mspt {
        ev.push(tf!(
            "MSPT medio ultimo minuto: {} ms (budget 50 ms)",
            "Average MSPT last minute: {} ms (budget 50 ms)",
            f(m)
        ));
    }
    if let Some(ps) = c.p.meta.platform_statistics.as_ref()
        && let Some(m) = ps.mspt.as_ref().and_then(|m| m.last1m.as_ref())
        && m.max > 0.0
    {
        ev.push(tf!(
            "MSPT 95° percentile {} ms, massimo {} ms",
            "MSPT 95th percentile {} ms, max {} ms",
            f(m.percentile95),
            f(m.max)
        ));
    }
    if let Some(b) = c.a.main.busy_per_tick {
        ev.push(tf!(
            "Lavoro medio del main thread misurato dal profilo: {} ms/tick su {} tick",
            "Average main thread work measured by the profile: {} ms/tick over {} ticks",
            f(b),
            c.a.main.ticks
        ));
    }
    ev.push(tf!(
        "Main thread occupato per il {}% del tempo campionato",
        "Main thread busy for {}% of the sampled time",
        f(c.a.main.busy_pct)
    ));
    if let Some(th) = c.p.ticked_threshold() {
        ev.push(tf!("Profilo registrato con --only-ticks-over {th}: contiene SOLO i tick lenti, quindi le percentuali descrivono direttamente i lag spike", "Profile recorded with --only-ticks-over {th}: it contains ONLY slow ticks, so the percentages describe the lag spikes directly"));
    }
    let (sev, title, expl) = if c.lagging {
        (
            Severity::Info,
            crate::i18n::t("Il server mostra lag", "The server is lagging"),
            crate::i18n::t(
                "Le sezioni seguenti indicano dove il main thread spende il tempo. Ogni tick deve stare sotto 50 ms: tutto ciò che supera il budget abbassa i TPS.",
                "The sections below show where the main thread spends its time. Each tick must stay under 50 ms: anything over budget lowers the TPS.",
            ),
        )
    } else {
        (
            Severity::Info,
            crate::i18n::t(
                "Nessun lag evidente nel periodo profilato",
                "No evident lag in the profiled period",
            ),
            crate::i18n::t(
                "Il main thread rientra nel budget di 50 ms per tick. Se il lag è sporadico, registra un profilo con /spark profiler start --only-ticks-over 100 per catturare solo i picchi.",
                "The main thread stays within the 50 ms per tick budget. If the lag is sporadic, record a profile with /spark profiler start --only-ticks-over 100 to capture only the spikes.",
            ),
        )
    };
    out.push(Finding {
        severity: sev,
        area: crate::i18n::t("Panoramica", "Overview").into(),
        title: title.into(),
        impact_pct: 0.0,
        evidence: ev,
        explanation: expl.into(),
        solutions: Vec::new(),
        culprits: Vec::new(),
    });
}

type Tr = (&'static str, &'static str);

fn tr(x: Tr) -> &'static str {
    crate::i18n::t(x.0, x.1)
}

struct CatRule {
    cat: Cat,
    min: f64,
    title: Tr,
    explanation: Tr,
    solutions: &'static [Tr],
}

const CAT_RULES: &[CatRule] = &[
    CatRule {
        cat: Cat::EntityTick,
        min: 12.0,
        title: (
            "Troppo tempo speso nel tick delle entità",
            "Too much time spent ticking entities",
        ),
        explanation: (
            "Ogni entità caricata viene aggiornata 20 volte al secondo. Farm di mob, allevamenti enormi, armor stand e item a terra moltiplicano questo costo.",
            "Every loaded entity is updated 20 times per second. Mob farms, huge animal pens, armor stands and dropped items multiply this cost.",
        ),
        solutions: &[
            (
                "Riduci il numero di entità: limita gli animali per chunk con un plugin (es. un limiter di entità per chunk) e controlla i chunk più affollati elencati nella sezione Mondo",
                "Reduce the number of entities: limit animals per chunk with a plugin (e.g. a per-chunk entity limiter) and check the most crowded chunks listed in the World section",
            ),
            (
                "Abbassa gli entity-activation-range in spigot.yml così le entità lontane dai giocatori vengono aggiornate meno spesso",
                "Lower entity-activation-range in spigot.yml so entities far from players are updated less often",
            ),
            (
                "Abbassa simulation-distance in server.properties (le entità oltre questa distanza non vengono aggiornate)",
                "Lower simulation-distance in server.properties (entities beyond this distance are not ticked)",
            ),
        ],
    },
    CatRule {
        cat: Cat::MobAi,
        min: 6.0,
        title: (
            "L'intelligenza artificiale dei mob pesa molto",
            "Mob AI is heavy",
        ),
        explanation: (
            "Goal selector, brain e sensori dei mob (soprattutto villager, piglin, axolotl, frog, warden, allay) vengono valutati ad ogni tick per ogni mob attivo.",
            "Mob goal selectors, brains and sensors (especially villagers, piglins, axolotls, frogs, wardens, allays) are evaluated every tick for every active mob.",
        ),
        solutions: &[
            (
                "Su Pufferfish/Purpur attiva DAB (dynamic activation of brain) per ridurre la frequenza dell'AI dei mob lontani",
                "On Pufferfish/Purpur enable DAB (dynamic activation of brain) to reduce how often distant mobs run their AI",
            ),
            (
                "Riduci entity-activation-range in spigot.yml e tick-inactive-villagers: false",
                "Lower entity-activation-range in spigot.yml and set tick-inactive-villagers: false",
            ),
            (
                "Limita i mob negli allevamenti e nelle farm: mob in spazi stretti continuano a cercare target e percorsi",
                "Limit mobs in pens and farms: mobs in tight spaces keep looking for targets and paths",
            ),
        ],
    },
    CatRule {
        cat: Cat::Pathfinding,
        min: 5.0,
        title: ("Pathfinding costoso", "Expensive pathfinding"),
        explanation: (
            "I mob ricalcolano spesso i percorsi: succede quando sono intrappolati, in grandi gruppi o inseguono target irraggiungibili.",
            "Mobs recompute paths often: it happens when they are trapped, in large groups or chasing unreachable targets.",
        ),
        solutions: &[
            (
                "Evita mob bloccati in spazi chiusi senza uscita (spawner, trappole), che ricalcolano il percorso di continuo",
                "Avoid mobs stuck in enclosed spaces with no exit (spawners, traps), which keep recomputing their path",
            ),
            (
                "Su Purpur/Pufferfish riduci la frequenza di aggiornamento del pathfinding per i mob inattivi",
                "On Purpur/Pufferfish reduce the pathfinding update rate for inactive mobs",
            ),
            (
                "Limita gli zombie/piglin che inseguono i villager nelle trading hall",
                "Limit zombies/piglins chasing villagers in trading halls",
            ),
        ],
    },
    CatRule {
        cat: Cat::Villagers,
        min: 5.0,
        title: ("Villager e POI pesanti", "Heavy villagers and POI"),
        explanation: (
            "I villager cercano continuamente letti, postazioni di lavoro e campane (POI). Le trading hall con decine di villager sono una delle cause più comuni di lag.",
            "Villagers constantly search for beds, workstations and bells (POI). Trading halls with dozens of villagers are one of the most common causes of lag.",
        ),
        solutions: &[
            (
                "Su Purpur attiva mobs.villager.lobotomize.enabled: true (i villager bloccati smettono di calcolare l'AI)",
                "On Purpur enable mobs.villager.lobotomize.enabled: true (stuck villagers stop running their AI)",
            ),
            (
                "Imposta tick-inactive-villagers: false in spigot.yml",
                "Set tick-inactive-villagers: false in spigot.yml",
            ),
            (
                "Limita il numero di villager per chunk e spezza le trading hall troppo grandi",
                "Limit villagers per chunk and split up trading halls that are too large",
            ),
        ],
    },
    CatRule {
        cat: Cat::Collisions,
        min: 6.0,
        title: (
            "Collisioni e ricerche di entità costose",
            "Expensive collisions and entity lookups",
        ),
        explanation: (
            "Quando molte entità si trovano nello stesso blocco ogni entità controlla le collisioni con tutte le altre: il costo cresce in modo quadratico.",
            "When many entities share the same block, each one checks collisions against all the others: the cost grows quadratically.",
        ),
        solutions: &[
            (
                "Riduci il massimo di collisioni per tick (collisions.max-entity-collisions in paper-world-defaults.yml, es. 2)",
                "Reduce the max collisions per tick (collisions.max-entity-collisions in paper-world-defaults.yml, e.g. 2)",
            ),
            (
                "Abbassa la gamerule maxEntityCramming (es. /gamerule maxEntityCramming 8)",
                "Lower the maxEntityCramming gamerule (e.g. /gamerule maxEntityCramming 8)",
            ),
            (
                "Cerca farm di mob o allevamenti con entità ammassate: vedi i chunk con più entità nella sezione Mondo",
                "Look for mob farms or pens with piled-up entities: see the chunks with the most entities in the World section",
            ),
        ],
    },
    CatRule {
        cat: Cat::ItemsXp,
        min: 4.0,
        title: (
            "Troppi item o sfere di esperienza a terra",
            "Too many dropped items or XP orbs",
        ),
        explanation: (
            "Ogni item e orb di XP è un'entità che si muove, si unisce ad altre e controlla le collisioni. Farm senza raccolta generano migliaia di item.",
            "Every item and XP orb is an entity that moves, merges with others and checks collisions. Farms without collection generate thousands of items.",
        ),
        solutions: &[
            (
                "Aumenta merge-radius in spigot.yml (item 3.5, exp 4.0) per unire gli item vicini",
                "Increase merge-radius in spigot.yml (item 3.5, exp 4.0) to merge nearby items",
            ),
            (
                "Riduci il despawn degli item comuni con entities.spawning.alt-item-despawn-rate in paper-world-defaults.yml",
                "Despawn common items sooner with entities.spawning.alt-item-despawn-rate in paper-world-defaults.yml",
            ),
            (
                "Verifica le farm che lasciano cadere item a terra e aggiungi hopper/raccolta o limitale",
                "Check farms that drop items on the ground and add hoppers/collection or limit them",
            ),
        ],
    },
    CatRule {
        cat: Cat::BlockEntities,
        min: 8.0,
        title: (
            "Block entity (tile entity) pesanti",
            "Heavy block entities (tile entities)",
        ),
        explanation: (
            "Fornaci, hopper, spawner, beacon, campfire, sculk e altri blocchi con logica propria vengono aggiornati ogni tick nei chunk caricati.",
            "Furnaces, hoppers, spawners, beacons, campfires, sculk and other blocks with their own logic are updated every tick in loaded chunks.",
        ),
        solutions: &[
            (
                "Guarda la tabella «Tipi di block entity» per capire quale tipo pesa di più",
                "Look at the “Block entity types” table to see which type costs the most",
            ),
            (
                "Limita il numero di blocchi di quel tipo per chunk o per giocatore",
                "Limit the number of blocks of that type per chunk or per player",
            ),
        ],
    },
    CatRule {
        cat: Cat::Hoppers,
        min: 4.0,
        title: (
            "Gli hopper consumano molto tempo",
            "Hoppers use a lot of time",
        ),
        explanation: (
            "Gli hopper controllano ogni tick l'inventario sopra e sotto e le entità item sopra di loro. Catene di hopper e sorter grandi sono molto costosi, e ogni spostamento chiama l'evento InventoryMoveItemEvent.",
            "Every tick hoppers check the inventory above and below them and the item entities on top. Long hopper chains and large sorters are very expensive, and every transfer fires InventoryMoveItemEvent.",
        ),
        solutions: &[
            (
                "Imposta hopper.disable-move-event: true in paper-world-defaults.yml se nessun plugin usa InventoryMoveItemEvent",
                "Set hopper.disable-move-event: true in paper-world-defaults.yml if no plugin uses InventoryMoveItemEvent",
            ),
            (
                "Aumenta ticks-per.hopper-transfer e hopper-amount in spigot.yml (es. 8 tick e 8 item: stessa velocità media, meno controlli)",
                "Increase ticks-per.hopper-transfer and hopper-amount in spigot.yml (e.g. 8 ticks and 8 items: same average speed, fewer checks)",
            ),
            (
                "Sostituisci le catene lunghe di hopper con water stream o limita gli hopper per chunk",
                "Replace long hopper chains with water streams or limit hoppers per chunk",
            ),
        ],
    },
    CatRule {
        cat: Cat::Spawners,
        min: 3.0,
        title: ("Spawner di mob pesanti", "Heavy mob spawners"),
        explanation: (
            "Ogni spawner attivo controlla la presenza di giocatori e tenta lo spawn ad ogni tick.",
            "Every active spawner checks for nearby players and tries to spawn every tick.",
        ),
        solutions: &[
            (
                "Imposta tick-rates.mob-spawner: 2 o più in paper-world-defaults.yml",
                "Set tick-rates.mob-spawner: 2 or higher in paper-world-defaults.yml",
            ),
            (
                "Attiva nerf-spawner-mobs: true in spigot.yml (i mob degli spawner non hanno AI)",
                "Enable nerf-spawner-mobs: true in spigot.yml (spawner mobs have no AI)",
            ),
            (
                "Se usi un plugin per spawner impilati, verifica le sue impostazioni di stack",
                "If you use a stacked-spawner plugin, check its stacking settings",
            ),
        ],
    },
    CatRule {
        cat: Cat::Redstone,
        min: 5.0,
        title: (
            "Redstone e aggiornamenti di blocchi pesanti",
            "Heavy redstone and block updates",
        ),
        explanation: (
            "Clock di redstone, piston e observer generano a cascata migliaia di aggiornamenti dei blocchi vicini.",
            "Redstone clocks, pistons and observers cascade into thousands of updates to neighbouring blocks.",
        ),
        solutions: &[
            (
                "Imposta misc.redstone-implementation: ALTERNATE_CURRENT in paper-world-defaults.yml (stesso comportamento, molto meno aggiornamenti)",
                "Set misc.redstone-implementation: ALTERNATE_CURRENT in paper-world-defaults.yml (same behaviour, far fewer updates)",
            ),
            (
                "Individua i clock di redstone sempre attivi e usa un plugin che limita/disattiva i clock troppo veloci",
                "Find always-on redstone clocks and use a plugin that limits/disables clocks that are too fast",
            ),
            (
                "Evita farm con piston/observer in loop continuo",
                "Avoid farms with pistons/observers in a continuous loop",
            ),
        ],
    },
    CatRule {
        cat: Cat::Fluids,
        min: 4.0,
        title: ("Fluidi che si propagano", "Spreading fluids"),
        explanation: (
            "Acqua e lava che scorrono generano tick schedulati e aggiornamenti di blocchi; laghi di lava o cascate create da giocatori sono spesso la causa.",
            "Flowing water and lava create scheduled ticks and block updates; lava lakes or waterfalls created by players are often the cause.",
        ),
        solutions: &[
            (
                "Cerca e rimuovi grandi flussi di lava/acqua creati dai giocatori",
                "Find and remove large lava/water flows created by players",
            ),
            (
                "Usa un plugin di protezione che limita il flusso dei fluidi in aree non costruite",
                "Use a protection plugin that limits fluid flow in unbuilt areas",
            ),
        ],
    },
    CatRule {
        cat: Cat::ChunkTick,
        min: 10.0,
        title: (
            "Tick dei chunk (random tick, meteo) elevato",
            "High chunk ticking (random ticks, weather)",
        ),
        explanation: (
            "Ogni chunk entro la simulation-distance riceve random tick (crescita delle colture, foglie, ghiaccio) e aggiornamenti meteo. Più giocatori sparsi = più chunk attivi.",
            "Every chunk within simulation-distance receives random ticks (crop growth, leaves, ice) and weather updates. More scattered players = more active chunks.",
        ),
        solutions: &[
            (
                "Abbassa simulation-distance in server.properties (es. 6) mantenendo la view-distance per la visuale",
                "Lower simulation-distance in server.properties (e.g. 6) while keeping view-distance for visuals",
            ),
            (
                "Verifica che la gamerule randomTickSpeed sia 3 (valore vanilla)",
                "Check that the randomTickSpeed gamerule is 3 (vanilla value)",
            ),
            (
                "Riduci farm di colture enormi o foreste di foglie piantate dai giocatori",
                "Reduce huge crop farms or leaf forests planted by players",
            ),
        ],
    },
    CatRule {
        cat: Cat::MobSpawning,
        min: 5.0,
        title: (
            "Lo spawn naturale dei mob è costoso",
            "Natural mob spawning is expensive",
        ),
        explanation: (
            "Il server cerca posizioni valide per lo spawn in tutti i chunk vicini ai giocatori, ogni tick, per ogni categoria di mob.",
            "The server looks for valid spawn positions in all chunks near players, every tick, for every mob category.",
        ),
        solutions: &[
            (
                "Aumenta ticks-per.monster-spawns in bukkit.yml (es. 10) e riduci spawn-limits",
                "Increase ticks-per.monster-spawns in bukkit.yml (e.g. 10) and reduce spawn-limits",
            ),
            (
                "Imposta entities.spawning.per-player-mob-spawns: true in paper-world-defaults.yml",
                "Set entities.spawning.per-player-mob-spawns: true in paper-world-defaults.yml",
            ),
            (
                "Abbassa mob-spawn-range in spigot.yml (es. 6) e la simulation-distance",
                "Lower mob-spawn-range in spigot.yml (e.g. 6) and the simulation-distance",
            ),
        ],
    },
    CatRule {
        cat: Cat::ScheduledTicks,
        min: 5.0,
        title: (
            "Tick schedulati di blocchi e fluidi elevati",
            "High scheduled block and fluid ticks",
        ),
        explanation: (
            "Blocchi come redstone, fluidi, cactus, sabbia/gravel che cadono e osservatori programmano tick futuri: tante macchine attive li accumulano.",
            "Blocks such as redstone, fluids, cactus, falling sand/gravel and observers schedule future ticks: many active machines pile them up.",
        ),
        solutions: &[
            (
                "Cerca macchine e farm sempre attive nelle aree con più giocatori",
                "Look for always-on machines and farms in the areas with the most players",
            ),
            (
                "Valuta ALTERNATE_CURRENT per la redstone e limita i fluidi che scorrono",
                "Consider ALTERNATE_CURRENT for redstone and limit flowing fluids",
            ),
        ],
    },
    CatRule {
        cat: Cat::ChunkLoadSync,
        min: 1.0,
        title: (
            "Caricamento di chunk sincrono sul main thread",
            "Synchronous chunk loading on the main thread",
        ),
        explanation: (
            "Il main thread si ferma ad aspettare il caricamento da disco o la generazione di un chunk. È una delle cause principali di lag spike: spesso causato da plugin che chiamano getChunkAt/getBlockAt in chunk non caricati, teletrasporti o giocatori che volano velocemente.",
            "The main thread stops and waits for a chunk to load from disk or be generated. It is one of the main causes of lag spikes: often caused by plugins calling getChunkAt/getBlockAt on unloaded chunks, teleports or players flying fast.",
        ),
        solutions: &[
            (
                "Se un plugin compare tra i responsabili, deve usare le API asincrone (World#getChunkAtAsync, teleportAsync su Paper)",
                "If a plugin is listed as responsible, it must use the async APIs (World#getChunkAtAsync, teleportAsync on Paper)",
            ),
            (
                "Pregenera il mondo con Chunky e imposta un world border, così i chunk non vanno generati al volo",
                "Pregenerate the world with Chunky and set a world border, so chunks don't have to be generated on the fly",
            ),
            (
                "Imposta chunks.prevent-moving-into-unloaded-chunks: true in paper-world-defaults.yml",
                "Set chunks.prevent-moving-into-unloaded-chunks: true in paper-world-defaults.yml",
            ),
            (
                "Usa dischi SSD/NVMe: il caricamento da disco lento allunga ogni attesa",
                "Use SSD/NVMe drives: slow disk loading makes every wait longer",
            ),
        ],
    },
    CatRule {
        cat: Cat::ChunkGen,
        min: 3.0,
        title: (
            "Generazione di chunk che pesa sul server",
            "Chunk generation is heavy on the server",
        ),
        explanation: (
            "La generazione di nuovi chunk (rumore, biomi, strutture, feature) è molto costosa. Se compare sul main thread blocca il tick; altrove consuma CPU che serve al server.",
            "Generating new chunks (noise, biomes, structures, features) is very expensive. On the main thread it blocks the tick; elsewhere it uses CPU the server needs.",
        ),
        solutions: &[
            (
                "Pregenera il mondo con Chunky (es. /chunky radius 10000) e imposta un world border",
                "Pregenerate the world with Chunky (e.g. /chunky radius 10000) and set a world border",
            ),
            (
                "Se la generazione avviene per mappe del tesoro o delfini, disattiva environment.treasure-maps.find-already-discovered o le mappe del tesoro (paper-world-defaults.yml)",
                "If generation is caused by treasure maps or dolphins, disable environment.treasure-maps.find-already-discovered or treasure maps (paper-world-defaults.yml)",
            ),
            (
                "Limita i giocatori che esplorano a grande velocità (elytra) con un limite di velocità o pregenerazione",
                "Limit players exploring at high speed (elytra) with a speed limit or pregeneration",
            ),
        ],
    },
    CatRule {
        cat: Cat::ChunkSave,
        min: 4.0,
        title: (
            "Salvataggio dei chunk / autosave pesante",
            "Heavy chunk saving / autosave",
        ),
        explanation: (
            "Il salvataggio scrive i chunk modificati su disco. Se troppi chunk vengono salvati nello stesso tick si crea un picco periodico.",
            "Saving writes modified chunks to disk. If too many chunks are saved in the same tick a periodic spike appears.",
        ),
        solutions: &[
            (
                "Imposta chunks.max-auto-save-chunks-per-tick: 8 in paper-world-defaults.yml per spalmare l'autosave",
                "Set chunks.max-auto-save-chunks-per-tick: 8 in paper-world-defaults.yml to spread out autosave",
            ),
            (
                "Imposta sync-chunk-writes=false in server.properties",
                "Set sync-chunk-writes=false in server.properties",
            ),
            (
                "Controlla backup e plugin che chiamano World#save o /save-all frequentemente",
                "Check backups and plugins that call World#save or /save-all frequently",
            ),
            (
                "Verifica la velocità del disco (SSD consigliato)",
                "Check disk speed (SSD recommended)",
            ),
        ],
    },
    CatRule {
        cat: Cat::ChunkSystem,
        min: 6.0,
        title: (
            "Gestione dei chunk (ticket, load/unload) elevata",
            "High chunk management (tickets, load/unload)",
        ),
        explanation: (
            "Il sistema dei chunk calcola quali chunk tenere caricati. Teletrasporti frequenti, view-distance alta e plugin che forzano il caricamento di chunk lo appesantiscono.",
            "The chunk system computes which chunks to keep loaded. Frequent teleports, high view-distance and plugins that force chunks loaded make it heavier.",
        ),
        solutions: &[
            (
                "Abbassa view-distance e simulation-distance",
                "Lower view-distance and simulation-distance",
            ),
            (
                "Controlla plugin che tengono chunk caricati (chunk loader, forceload) e la loro quantità",
                "Check plugins that keep chunks loaded (chunk loaders, forceload) and how many",
            ),
            (
                "Imposta chunks.delay-chunk-unloads-by: 10s in paper-world-defaults.yml per evitare caricamenti/scaricamenti a ripetizione",
                "Set chunks.delay-chunk-unloads-by: 10s in paper-world-defaults.yml to avoid repeated loading/unloading",
            ),
        ],
    },
    CatRule {
        cat: Cat::Lighting,
        min: 4.0,
        title: ("Calcolo della luce pesante", "Heavy light calculation"),
        explanation: (
            "La luce viene ricalcolata quando molti blocchi cambiano: WorldEdit, esplosioni, generazione o farm che piazzano/rompono blocchi in continuazione.",
            "Light is recalculated when many blocks change: WorldEdit, explosions, generation or farms that constantly place/break blocks.",
        ),
        solutions: &[
            (
                "Usa FastAsyncWorldEdit invece di WorldEdit per le grandi modifiche",
                "Use FastAsyncWorldEdit instead of WorldEdit for large edits",
            ),
            (
                "Controlla macchine che piazzano/rompono blocchi di continuo",
                "Check machines that constantly place/break blocks",
            ),
            (
                "Aggiorna il server: le versioni recenti di Paper hanno un motore di luce molto più veloce",
                "Update the server: recent Paper versions have a much faster light engine",
            ),
        ],
    },
    CatRule {
        cat: Cat::EntityTracking,
        min: 6.0,
        title: ("Tracking delle entità costoso", "Expensive entity tracking"),
        explanation: (
            "Il server calcola per ogni entità quali giocatori la vedono e invia i pacchetti di movimento. Molte entità e molti giocatori vicini moltiplicano il costo.",
            "For every entity the server computes which players can see it and sends movement packets. Many entities and many nearby players multiply the cost.",
        ),
        solutions: &[
            (
                "Abbassa entity-tracking-range in spigot.yml (es. players 48, animals 32, monsters 32, misc 16)",
                "Lower entity-tracking-range in spigot.yml (e.g. players 48, animals 32, monsters 32, misc 16)",
            ),
            (
                "Abbassa entity-broadcast-range-percentage in server.properties (es. 75)",
                "Lower entity-broadcast-range-percentage in server.properties (e.g. 75)",
            ),
            (
                "Riduci il numero di entità nelle aree affollate",
                "Reduce the number of entities in crowded areas",
            ),
        ],
    },
    CatRule {
        cat: Cat::Players,
        min: 10.0,
        title: ("Tick dei giocatori elevato", "High player ticking"),
        explanation: (
            "Ogni giocatore fa caricare chunk, calcolare la visuale e aggiornare inventario e statistiche.",
            "Every player loads chunks, computes visibility and updates inventory and statistics.",
        ),
        solutions: &[
            (
                "Abbassa view-distance (o usa send-view-distance più alta della simulation-distance)",
                "Lower view-distance (or use a send-view-distance higher than simulation-distance)",
            ),
            (
                "Controlla i plugin che agiscono su ogni giocatore ad ogni tick (scoreboard, tablist, boss bar)",
                "Check plugins that act on every player every tick (scoreboards, tab lists, boss bars)",
            ),
        ],
    },
    CatRule {
        cat: Cat::Network,
        min: 8.0,
        title: (
            "Gestione dei pacchetti dei giocatori pesante",
            "Heavy player packet handling",
        ),
        explanation: (
            "I pacchetti ricevuti dai giocatori (movimento, click, chat) vengono processati sul main thread. Anticheat e plugin che ascoltano PlayerMoveEvent aggiungono costo a ogni pacchetto.",
            "Packets received from players (movement, clicks, chat) are processed on the main thread. Anticheats and plugins listening to PlayerMoveEvent add cost to every packet.",
        ),
        solutions: &[
            (
                "Controlla i plugin responsabili: listener pesanti su PlayerMoveEvent o PlayerInteractEvent vanno ottimizzati",
                "Check the responsible plugins: heavy listeners on PlayerMoveEvent or PlayerInteractEvent need optimizing",
            ),
            (
                "Imposta un network-compression-threshold ragionevole (256) in server.properties",
                "Set a reasonable network-compression-threshold (256) in server.properties",
            ),
        ],
    },
    CatRule {
        cat: Cat::PluginTasks,
        min: 6.0,
        title: (
            "Task schedulati sincroni costosi",
            "Expensive synchronous scheduled tasks",
        ),
        explanation: (
            "I task dei plugin eseguiti con runTask/runTaskTimer girano sul main thread e rallentano ogni tick in cui si attivano.",
            "Plugin tasks run with runTask/runTaskTimer execute on the main thread and slow down every tick they run in.",
        ),
        solutions: &[
            (
                "Guarda la sezione Plugin per vedere quali task pesano di più",
                "Look at the Plugins section to see which tasks cost the most",
            ),
            (
                "Il lavoro che non tocca il mondo (database, calcoli, file, HTTP) va spostato in runTaskAsynchronously",
                "Work that doesn't touch the world (database, calculations, files, HTTP) should move to runTaskAsynchronously",
            ),
            (
                "Aumenta l'intervallo dei task periodici o distribuisci il lavoro su più tick",
                "Increase the interval of periodic tasks or spread the work over several ticks",
            ),
        ],
    },
    CatRule {
        cat: Cat::PluginEvents,
        min: 6.0,
        title: ("Listener di eventi costosi", "Expensive event listeners"),
        explanation: (
            "Gli eventi Bukkit vengono chiamati sul main thread. Eventi frequenti (movimento, fisica dei blocchi, spostamento item) con listener pesanti rallentano ogni tick.",
            "Bukkit events are called on the main thread. Frequent events (movement, block physics, item transfers) with heavy listeners slow down every tick.",
        ),
        solutions: &[
            (
                "Guarda la tabella «Eventi» per vedere quale evento e quale plugin pesano di più",
                "Look at the “Events” table to see which event and which plugin cost the most",
            ),
            (
                "Nei listener esci subito quando l'evento non è rilevante (controlla prima i casi più economici)",
                "In listeners return early when the event is irrelevant (check the cheapest conditions first)",
            ),
            (
                "Evita accessi a database, file o calcoli pesanti dentro i listener",
                "Avoid database access, files or heavy calculations inside listeners",
            ),
        ],
    },
    CatRule {
        cat: Cat::Maps,
        min: 3.0,
        title: ("Aggiornamento delle mappe costoso", "Expensive map updates"),
        explanation: (
            "Le mappe tenute in mano o negli item frame vengono aggiornate continuamente.",
            "Maps held in hand or in item frames are updated continuously.",
        ),
        solutions: &[(
            "Limita i muri di mappe e usa un plugin per mappe statiche (image maps) con rendering in cache",
            "Limit map walls and use a static map plugin (image maps) with cached rendering",
        )],
    },
    CatRule {
        cat: Cat::Explosions,
        min: 3.0,
        title: ("Esplosioni costose", "Expensive explosions"),
        explanation: (
            "TNT e creeper calcolano raggi e danni su molte entità e blocchi.",
            "TNT and creepers compute rays and damage over many entities and blocks.",
        ),
        solutions: &[
            (
                "Imposta environment.optimize-explosions: true in paper-world-defaults.yml",
                "Set environment.optimize-explosions: true in paper-world-defaults.yml",
            ),
            (
                "Limita i cannoni TNT e le TNT duper",
                "Limit TNT cannons and TNT dupers",
            ),
        ],
    },
    CatRule {
        cat: Cat::Scoreboard,
        min: 3.0,
        title: (
            "Scoreboard e team aggiornati troppo spesso",
            "Scoreboards and teams updated too often",
        ),
        explanation: (
            "Plugin di scoreboard/tablist che aggiornano team e punteggi per ogni giocatore ad ogni tick generano molti pacchetti e calcoli.",
            "Scoreboard/tab list plugins that update teams and scores for every player every tick generate lots of packets and work.",
        ),
        solutions: &[
            (
                "Aumenta l'intervallo di aggiornamento del plugin di scoreboard/tablist",
                "Increase the update interval of the scoreboard/tab list plugin",
            ),
            (
                "Riduci i placeholder dinamici aggiornati ogni tick",
                "Reduce dynamic placeholders updated every tick",
            ),
        ],
    },
    CatRule {
        cat: Cat::Logging,
        min: 2.0,
        title: (
            "Scrittura su console/log sul main thread",
            "Console/log writing on the main thread",
        ),
        explanation: (
            "Un plugin o il server stanno scrivendo molti messaggi nel log; la formattazione e la scrittura costano e spesso indicano errori ripetuti.",
            "A plugin or the server is writing many log messages; formatting and writing are costly and often indicate repeated errors.",
        ),
        solutions: &[
            (
                "Apri logs/latest.log e cerca messaggi o errori che si ripetono",
                "Open logs/latest.log and look for repeating messages or errors",
            ),
            (
                "Disattiva il debug dei plugin responsabili",
                "Disable debug mode in the responsible plugins",
            ),
        ],
    },
    CatRule {
        cat: Cat::Exceptions,
        min: 2.0,
        title: (
            "Creazione di eccezioni in grande quantità",
            "Creating exceptions in large numbers",
        ),
        explanation: (
            "Creare un'eccezione costa (cattura dello stack trace). Molte eccezioni al secondo indicano un errore continuo o un plugin che usa le eccezioni per il controllo di flusso.",
            "Creating an exception is costly (stack trace capture). Many exceptions per second indicate a continuous error or a plugin using exceptions for control flow.",
        ),
        solutions: &[
            (
                "Controlla logs/latest.log per errori ripetuti",
                "Check logs/latest.log for repeated errors",
            ),
            (
                "Aggiorna o segnala il plugin responsabile",
                "Update or report the responsible plugin",
            ),
        ],
    },
    CatRule {
        cat: Cat::SyncDatabase,
        min: 1.0,
        title: (
            "Accesso a database sincrono sul main thread",
            "Synchronous database access on the main thread",
        ),
        explanation: (
            "Il main thread aspetta la risposta del database: ogni query aggiunge millisecondi al tick e, se il database è lento, causa lag spike.",
            "The main thread waits for the database to answer: every query adds milliseconds to the tick and, if the database is slow, causes lag spikes.",
        ),
        solutions: &[
            (
                "Il plugin responsabile deve eseguire le query in asincrono (runTaskAsynchronously / CompletableFuture) e mettere i dati in cache",
                "The responsible plugin must run queries asynchronously (runTaskAsynchronously / CompletableFuture) and cache the data",
            ),
            (
                "Controlla la latenza tra server e database (stessa macchina o rete locale)",
                "Check the latency between server and database (same machine or local network)",
            ),
        ],
    },
    CatRule {
        cat: Cat::SyncNetworkIo,
        min: 1.0,
        title: (
            "Chiamate di rete (HTTP/socket) sincrone sul main thread",
            "Synchronous network calls (HTTP/socket) on the main thread",
        ),
        explanation: (
            "Il main thread aspetta una risposta via rete (API web, Redis, webhook, controllo licenze). Qualsiasi ritardo di rete diventa lag.",
            "The main thread waits for a network response (web APIs, Redis, webhooks, license checks). Any network delay becomes lag.",
        ),
        solutions: &[
            (
                "Il plugin responsabile deve fare le richieste in asincrono",
                "The responsible plugin must make the requests asynchronously",
            ),
            (
                "Aggiorna il plugin o segnala il problema allo sviluppatore",
                "Update the plugin or report the problem to the developer",
            ),
        ],
    },
    CatRule {
        cat: Cat::SyncFileIo,
        min: 2.0,
        title: (
            "Lettura/scrittura di file sincrona sul main thread",
            "Synchronous file read/write on the main thread",
        ),
        explanation: (
            "Il main thread aspetta il disco per salvare o leggere file (config, dati giocatori, YAML/JSON).",
            "The main thread waits for the disk to save or read files (configs, player data, YAML/JSON).",
        ),
        solutions: &[
            (
                "Il plugin responsabile deve salvare i dati in asincrono e non ricaricare file ad ogni tick",
                "The responsible plugin must save data asynchronously and not reload files every tick",
            ),
            ("Verifica la velocità del disco", "Check disk speed"),
        ],
    },
    CatRule {
        cat: Cat::Blocking,
        min: 2.0,
        title: (
            "Il main thread resta bloccato in attesa",
            "The main thread is blocked waiting",
        ),
        explanation: (
            "Il main thread sta aspettando un lock, un Future (.get()/.join()) o uno sleep fuori dalla normale pausa tra un tick e l'altro. Mentre aspetta il server è fermo.",
            "The main thread is waiting on a lock, a Future (.get()/.join()) or a sleep outside the normal pause between ticks. While it waits the server is frozen.",
        ),
        solutions: &[
            (
                "Il plugin responsabile non deve chiamare .get()/.join() su CompletableFuture o Thread.sleep sul main thread",
                "The responsible plugin must not call .get()/.join() on CompletableFuture or Thread.sleep on the main thread",
            ),
            (
                "Usa callback (thenAccept) e torna sul main thread con il scheduler",
                "Use callbacks (thenAccept) and switch back to the main thread with the scheduler",
            ),
        ],
    },
    CatRule {
        cat: Cat::Serialization,
        min: 3.0,
        title: (
            "Serializzazione JSON/YAML pesante sul main thread",
            "Heavy JSON/YAML serialization on the main thread",
        ),
        explanation: (
            "Conversioni continue da/verso JSON o YAML (es. salvataggi di dati, config ricaricate) consumano CPU sul main thread.",
            "Continuous conversions to/from JSON or YAML (e.g. saving data, reloading configs) use CPU on the main thread.",
        ),
        solutions: &[
            (
                "Mantieni i dati in memoria e serializza solo al salvataggio, in asincrono",
                "Keep data in memory and serialize only when saving, asynchronously",
            ),
            (
                "Evita di ricaricare file di configurazione durante il gioco",
                "Avoid reloading configuration files during gameplay",
            ),
        ],
    },
    CatRule {
        cat: Cat::TextFormatting,
        min: 3.0,
        title: (
            "Formattazione di testo e placeholder costosa",
            "Expensive text formatting and placeholders",
        ),
        explanation: (
            "Parsing di MiniMessage/colori e risoluzione di placeholder ripetuti per ogni giocatore e ogni tick.",
            "MiniMessage/colour parsing and placeholder resolution repeated for every player every tick.",
        ),
        solutions: &[
            (
                "Aumenta l'intervallo di aggiornamento di scoreboard, tablist e hologram",
                "Increase the update interval of scoreboards, tab lists and holograms",
            ),
            (
                "Metti in cache i componenti già convertiti invece di riparsarli ogni volta",
                "Cache already converted components instead of re-parsing them every time",
            ),
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
                    crate::i18n::t(
                        "true (se nessun plugin lo usa)",
                        "true (if no plugin uses it)",
                    ),
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
                    solutions.push(tf!("Le ricerche di strutture (mappe del tesoro/delfini/eye of ender) pesano {}%: imposta environment.treasure-maps.find-already-discovered.villager-trade e loot-tables a true, o disattiva environment.treasure-maps.enabled", "Structure searches (treasure maps/dolphins/eye of ender) cost {}%: set environment.treasure-maps.find-already-discovered.villager-trade and loot-tables to true, or disable environment.treasure-maps.enabled", f(treasure)));
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
        solutions.extend(rule.solutions.iter().map(|s| tr(*s).to_string()));
        if !cfg_lines.is_empty() {
            solutions.push(tf!(
                "Configurazione da rivedere:\n    - {}",
                "Configuration to review:\n    - {}",
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
                evidence.push(tf!(
                    "Tipi di entità più costosi: {}",
                    "Most expensive entity types: {}",
                    ents.join(", ")
                ));
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
                evidence.push(tf!(
                    "Tipi di block entity più costosi: {}",
                    "Most expensive block entity types: {}",
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
            title: tr(rule.title).into(),
            impact_pct: cs.pct,
            evidence,
            explanation: tr(rule.explanation).into(),
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
        let mut ev = vec![tf!(
            "{} è nello stack per il {}% del tempo attivo del main thread ({}), di cui {}% nel suo codice o in chiamate fatte direttamente",
            "{} is on the stack for {}% of main thread active time ({}), of which {}% in its own code or direct calls",
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
            ev.push(tf!(
                "Punti di ingresso: {}",
                "Entry points: {}",
                entries.join("; ")
            ));
        }
        let ms: Vec<String> = s
            .top_methods
            .iter()
            .take(4)
            .filter(|m| m.pct >= 0.3)
            .map(|m| format!("{} {}%", short(&m.name), f(m.pct)))
            .collect();
        if !ms.is_empty() {
            ev.push(tf!(
                "Dove spende il tempo: {}",
                "Where it spends time: {}",
                ms.join(", ")
            ));
        }
        let mut solutions = Vec::new();
        for h in &s.hazards {
            if h.pct < 0.5 {
                continue;
            }
            ev.push(tf!(
                "⚠ {}: {}% del main thread",
                "⚠ {}: {}% of the main thread",
                h.name,
                f(h.pct)
            ));
            solutions.push(tf!("{}: il plugin esegue «{}» sul main thread; va spostato in un task asincrono o messo in cache", "{}: the plugin performs “{}” on the main thread; it should be moved to an async task or cached", name, h.name.to_lowercase()));
        }
        let from_events = s
            .top_entries
            .iter()
            .any(|e| e.context.starts_with(crate::i18n::t("Evento", "Event")) && e.pct >= 1.0);
        let from_tasks = s.top_entries.iter().any(|e| {
            e.context == crate::i18n::t("Task schedulato", "Scheduled task") && e.pct >= 1.0
        });
        if from_events {
            solutions.push(crate::i18n::t("Ottimizza i listener indicati: controlla prima le condizioni economiche e usa ignoreCancelled/priorità corrette", "Optimize the listed listeners: check cheap conditions first and use ignoreCancelled/correct priorities").into());
        }
        if from_tasks {
            solutions.push(crate::i18n::t("Riduci la frequenza dei suoi task ripetuti o spostali in asincrono se non toccano il mondo", "Reduce the frequency of its repeating tasks or move them async if they don't touch the world").into());
        }
        solutions.push(tf!("Aggiorna {name} all'ultima versione e controlla la sua configurazione (intervalli di aggiornamento, funzioni non usate da disattivare)", "Update {name} to the latest version and review its configuration (update intervals, unused features to disable)"));
        solutions.push(crate::i18n::t("Se il costo non è giustificato dalla funzione che offre, valuta un'alternativa più leggera o segnala il profilo allo sviluppatore", "If the cost isn't justified by what it does, consider a lighter alternative or send the profile to the developer").into());
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
            area: crate::i18n::t("Plugin/Mod", "Plugin/Mod").into(),
            title: tf!("{name} pesa sul main thread", "{name} is heavy on the main thread"),
            impact_pct: s.incl_pct,
            evidence: ev,
            explanation: crate::i18n::t("La percentuale «nello stack» include tutto il lavoro innescato dal plugin (anche chiamate a Minecraft/Bukkit fatte dal plugin). È il costo che sparirebbe se il plugin non facesse quel lavoro.", "The “on stack” percentage includes all the work triggered by the plugin (including Minecraft/Bukkit calls it makes). It is the cost that would disappear if the plugin didn't do that work.").into(),
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
            area: crate::i18n::t("Plugin/Mod", "Plugin/Mod").into(),
            title: crate::i18n::t("Eventi Bukkit più costosi", "Most expensive Bukkit events").into(),
            impact_pct: c.a.events.first().map(|e| e.pct).unwrap_or(0.0),
            evidence: heavy_events,
            explanation: crate::i18n::t("Questi eventi, gestiti dai plugin indicati, occupano una parte significativa del main thread.", "These events, handled by the listed plugins, take a significant share of the main thread.").into(),
            solutions: vec![
                crate::i18n::t("PlayerMoveEvent: esci subito se il giocatore non ha cambiato blocco (confronta getFrom/getTo a livello di blocco)", "PlayerMoveEvent: return early if the player didn't change block (compare getFrom/getTo at block level)").into(),
                crate::i18n::t("BlockPhysicsEvent / InventoryMoveItemEvent: sono chiamati migliaia di volte al tick, i listener devono essere minimi", "BlockPhysicsEvent / InventoryMoveItemEvent: they are called thousands of times per tick, listeners must be minimal").into(),
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
            line.push_str(&tf!(" — crescita: {}", " — growth: {}", causes.join(", ")));
        }
        if !owners.is_empty() {
            line.push_str(&tf!("; plugin: {}", "; plugins: {}", owners.join(", ")));
        }
        ev.push(line);
    }
    if let Some(p) = &c.a.periodic_spikes {
        ev.push(p.clone());
    }
    let n = c.a.spikes.len();
    let total = c.a.windows.len();
    let mut solutions = vec![crate::i18n::t("Confronta le categorie in crescita con le sezioni dedicate qui sotto: sono la causa più probabile del picco", "Compare the growing categories with the dedicated sections below: they are the most likely cause of the spike").to_string()];
    if c.a.periodic_spikes.is_some() {
        solutions.push(crate::i18n::t("Picchi periodici: controlla autosave (chunks.max-auto-save-chunks-per-tick), backup automatici e task dei plugin con timer fisso", "Periodic spikes: check autosave (chunks.max-auto-save-chunks-per-tick), automatic backups and plugin tasks with fixed timers").into());
    }
    solutions.push(crate::i18n::t("Per isolare meglio i picchi registra un profilo con /spark profiler start --only-ticks-over 100 --timeout 600", "To better isolate the spikes record a profile with /spark profiler start --only-ticks-over 100 --timeout 600").into());
    out.push(Finding {
        severity: if n * 3 >= total { Severity::Alto } else { Severity::Medio },
        area: crate::i18n::t("Picchi di lag", "Lag spikes").into(),
        title: tf!("{n} finestre su {total} con lag spike", "{n} of {total} windows with lag spikes"),
        impact_pct: 0.0,
        evidence: ev,
        explanation: crate::i18n::t("Ogni finestra è circa un minuto di profilo. Il confronto con le finestre normali mostra cosa è cresciuto durante il picco.", "Each window is about one minute of profile. Comparing it with normal windows shows what grew during the spike.").into(),
        solutions,
        culprits: Vec::new(),
    });
}

pub fn window_label(w: &crate::analysis::WindowStat) -> String {
    if w.start_time > 0 {
        let secs = w.start_time / 1000;
        let h = (secs / 3600) % 24;
        let m = (secs / 60) % 60;
        tf!(
            "finestra #{} ({:02}:{:02} UTC)",
            "window #{} ({:02}:{:02} UTC)",
            w.index + 1,
            h,
            m
        )
    } else {
        tf!("finestra #{}", "window #{}", w.index + 1)
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
        let mut ev = vec![tf!(
            "Thread «{}» occupato per il {}% della durata del profilo",
            "Thread “{}” busy for {}% of the profile duration",
            t.name,
            f(ratio * 100.0)
        )];
        if !tops.is_empty() {
            ev.push(tf!(
                "Lavoro principale: {}",
                "Main work: {}",
                tops.join(", ")
            ));
        }
        if let Some(o) = &t.top_owner {
            ev.push(tf!(
                "Codice di: {}",
                "Code from: {}",
                c.p.sources.display_name(o)
            ));
        }
        let kind = t
            .plugin_hint
            .clone()
            .unwrap_or_else(|| crate::i18n::t("thread secondario", "secondary thread").into());
        out.push(Finding {
            severity: Severity::Basso,
            area: crate::i18n::t("Thread secondari", "Secondary threads").into(),
            title: tf!("Thread sempre occupato: {} ({kind})", "Always busy thread: {} ({kind})", t.name),
            impact_pct: ratio * 100.0,
            evidence: ev,
            explanation: crate::i18n::t("Un thread secondario sempre al lavoro non blocca direttamente il tick, ma consuma un core: se la CPU è limitata ruba tempo al main thread.", "A secondary thread that is always working doesn't block the tick directly, but it uses a core: if CPU is limited it steals time from the main thread.").into(),
            solutions: vec![crate::i18n::t("Verifica che il lavoro sia necessario (task asincroni in loop, generazione chunk continua, rete)", "Check that the work is necessary (looping async tasks, continuous chunk generation, network)").into(), crate::i18n::t("Se la CPU è satura, riduci questo carico o aumenta i core disponibili", "If the CPU is saturated, reduce this load or add more cores").into()],
            culprits: t.top_owner.iter().map(|o| c.p.sources.display_name(o)).collect(),
        });
    }
}

fn mode_note(c: &Ctx, out: &mut Vec<Finding>) {
    let (title, expl) = match c.p.mode {
        Mode::Allocation => (
            crate::i18n::t(
                "Profilo di allocazione memoria",
                "Memory allocation profile",
            ),
            crate::i18n::t(
                "Questo profilo misura i byte allocati, non il tempo CPU. Le percentuali indicano chi crea più oggetti: allocazioni elevate causano GC frequenti.",
                "This profile measures allocated bytes, not CPU time. The percentages show who creates the most objects: high allocation causes frequent GC.",
            ),
        ),
        _ => (
            crate::i18n::t("Profilo di contesa dei lock", "Lock contention profile"),
            crate::i18n::t(
                "Questo profilo misura il tempo passato ad aspettare lock. Le percentuali indicano dove i thread si bloccano a vicenda.",
                "This profile measures time spent waiting on locks. The percentages show where threads block each other.",
            ),
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
        area: crate::i18n::t("Profilo", "Profile").into(),
        title: title.into(),
        impact_pct: 0.0,
        evidence: ev,
        explanation: expl.into(),
        solutions: vec![
            crate::i18n::t(
                "Per trovare la causa del lag usa un profilo CPU normale: /spark profiler start",
                "To find the cause of lag use a normal CPU profile: /spark profiler start",
            )
            .into(),
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
        ev.push(tf!(
            "Java {} ({})",
            "Java {} ({})",
            java.version,
            if java.vendor_version.is_empty() {
                &java.vendor
            } else {
                &java.vendor_version
            }
        ));
        if m < 21 {
            sol.push(crate::i18n::t("Aggiorna a Java 21 o più recente: GC e JIT sono nettamente migliori (richiesto da Minecraft 1.20.5+)", "Upgrade to Java 21 or newer: GC and JIT are much better (required by Minecraft 1.20.5+)").into());
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
        ev.push(tf!("Heap massimo (-Xmx): {}", "Max heap (-Xmx): {}", gb(x)));
        if let Some((_, total)) = phys
            && total > 0
            && x as f64 > total as f64 * 0.9
        {
            sol.push(tf!("-Xmx ({}) è quasi tutta la RAM della macchina ({}): lascia almeno 1-2 GB al sistema per evitare swap", "-Xmx ({}) is almost all of the machine's RAM ({}): leave at least 1-2 GB to the system to avoid swapping", gb(x), gb(total)));
            bump(Severity::Alto, &mut worst);
        }
        if x < (3u64 << 30) {
            sol.push(
                crate::i18n::t("Heap sotto i 3 GB: con più giocatori il GC sarà molto frequente; valuta 6-10 GB", "Heap under 3 GB: with more players GC will be very frequent; consider 6-10 GB")
                    .into(),
            );
            bump(Severity::Medio, &mut worst);
        }
        if let Some(s) = args.xms
            && s != x
        {
            sol.push(
                crate::i18n::t("Imposta -Xms uguale a -Xmx: evita che la JVM ridimensioni l'heap durante il gioco", "Set -Xms equal to -Xmx: it prevents the JVM from resizing the heap during gameplay")
                    .into(),
            );
            bump(Severity::Basso, &mut worst);
        }
    }
    match args.gc.as_deref() {
        Some("Parallel") | Some("Serial") | Some("CMS") => {
            sol.push(tf!("GC {} non adatto a un server di gioco: usa G1 con i flag di Aikar o ZGC generazionale", "GC {} is not suited to a game server: use G1 with Aikar's flags or generational ZGC", args.gc.clone().unwrap_or_default()));
            bump(Severity::Alto, &mut worst);
        }
        Some("G1") | None if !args.aikar => {
            sol.push(crate::i18n::t("Usa i flag di Aikar per G1 (vedi docs.papermc.io → Aikar's flags) oppure, con heap ≥ 12 GB e Java 21+, -XX:+UseZGC -XX:+ZGenerational", "Use Aikar's flags for G1 (see docs.papermc.io → Aikar's flags) or, with heap ≥ 12 GB and Java 21+, -XX:+UseZGC -XX:+ZGenerational").into());
            bump(Severity::Basso, &mut worst);
        }
        _ => {}
    }
    if let Some(g) = &args.gc {
        ev.push(tf!(
            "Garbage collector: {g}{}",
            "Garbage collector: {g}{}",
            if args.aikar {
                crate::i18n::t(" (flag di Aikar)", " (Aikar's flags)")
            } else {
                ""
            }
        ));
    }
    if let Some((used, total)) = swap
        && used > (256u64 << 20)
    {
        ev.push(tf!(
            "Swap in uso: {} su {}",
            "Swap in use: {} of {}",
            gb(used),
            gb(total)
        ));
        sol.push(crate::i18n::t("La macchina sta usando lo swap: riduci la RAM assegnata o aggiungi memoria, lo swap causa pause enormi del GC", "The machine is swapping: reduce the assigned RAM or add memory, swapping causes huge GC pauses").into());
        bump(Severity::Alto, &mut worst);
    }
    if let Some(cpu) = sys.cpu.as_ref() {
        let proc1 = cpu.process_usage.as_ref().map(|u| u.last1m).unwrap_or(0.0);
        let sys1 = cpu.system_usage.as_ref().map(|u| u.last1m).unwrap_or(0.0);
        ev.push(tf!(
            "CPU: {} thread{}, processo {}%, sistema {}%",
            "CPU: {} threads{}, process {}%, system {}%",
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
            sol.push(crate::i18n::t("Altri processi sulla macchina usano molta CPU (CPU di sistema molto più alta di quella del server): spostali o verifica un host condiviso sovraccarico", "Other processes on the machine use a lot of CPU (system CPU much higher than the server's): move them or check for an overloaded shared host").into());
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
            sol.push(crate::i18n::t("La CPU è quasi satura: serve ridurre il carico o un processore più veloce (Minecraft dipende dalla velocità single-core)", "The CPU is almost saturated: reduce the load or get a faster processor (Minecraft depends on single-core speed)").into());
            bump(Severity::Alto, &mut worst);
        }
        if cpu.threads > 0 && cpu.threads <= 2 {
            sol.push(crate::i18n::t("Solo 1-2 core disponibili: GC, rete e chunk competono con il main thread; consigliati almeno 4 core", "Only 1-2 cores available: GC, network and chunks compete with the main thread; at least 4 cores recommended").into());
            bump(Severity::Medio, &mut worst);
        }
    }
    let gc_map = ps.map(|p| p.gc.clone()).unwrap_or_default();
    let mut gc_ev = Vec::new();
    for (name, g) in &gc_map {
        if g.total <= 0 {
            continue;
        }
        gc_ev.push(tf!(
            "{name}: {} raccolte, media {} ms, ogni {} s",
            "{name}: {} collections, avg {} ms, every {} s",
            g.total,
            f(g.avg_time),
            f(g.avg_frequency / 1000.0)
        ));
        let lname = name.to_ascii_lowercase();
        let is_old = lname.contains("old")
            || lname.contains("full")
            || lname.contains("mark") && !lname.contains("young");
        if is_old && g.total >= 2 && g.avg_frequency > 0.0 && g.avg_frequency < 600_000.0 {
            sol.push(tf!("Raccolte «{name}» frequenti: l'heap è quasi pieno (troppi dati in memoria, memory leak o heap troppo piccolo). Fai /spark heapsummary per vedere cosa occupa memoria", "Frequent “{name}” collections: the heap is almost full (too much data in memory, a memory leak or a heap that is too small). Run /spark heapsummary to see what uses memory"));
            bump(Severity::Alto, &mut worst);
        }
        if !is_old && g.avg_time > 60.0 {
            sol.push(tf!(
                "Pause «{name}» lunghe ({} ms in media): applica i flag di Aikar o passa a ZGC",
                "Long “{name}” pauses ({} ms on average): apply Aikar's flags or switch to ZGC",
                f(g.avg_time)
            ));
            bump(Severity::Medio, &mut worst);
        }
        if !is_old && g.avg_frequency > 0.0 && g.avg_frequency < 1500.0 {
            sol.push(tf!("«{name}» ogni {} s: molte allocazioni. Un profilo /spark profiler start --alloc mostra chi alloca di più", "“{name}” every {} s: lots of allocation. A /spark profiler start --alloc profile shows who allocates the most", f(g.avg_frequency / 1000.0)));
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
        ev.push(tf!(
            "Heap in uso: {} su {} ({}%)",
            "Heap in use: {} of {} ({}%)",
            gb(mem.used as u64),
            gb(mem.max as u64),
            f(pct)
        ));
    }
    if ev.is_empty() {
        return;
    }
    let title = if sol.is_empty() {
        crate::i18n::t(
            "JVM e macchina: nessun problema evidente",
            "JVM and machine: no evident problems",
        )
        .to_string()
    } else {
        crate::i18n::t(
            "JVM, memoria e CPU da rivedere",
            "JVM, memory and CPU to review",
        )
        .to_string()
    };
    out.push(Finding {
        severity: if sol.is_empty() {
            Severity::Info
        } else {
            worst
        },
        area: crate::i18n::t("JVM / Macchina", "JVM / Machine").into(),
        title,
        impact_pct: 0.0,
        evidence: ev,
        explanation: crate::i18n::t(
            "Impostazioni della JVM e stato della macchina al momento del profilo.",
            "JVM settings and machine state at the time of the profile.",
        )
        .into(),
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
    let mut ev = vec![tf!(
        "Entità totali: {}",
        "Total entities: {}",
        world.total_entities
    )];
    let mut counts: Vec<(&String, &i32)> = world.entity_counts.iter().collect();
    counts.sort_by(|a, b| b.1.cmp(a.1));
    let top: Vec<String> = counts
        .iter()
        .take(8)
        .map(|(k, v)| format!("{} {}", k.trim_start_matches("minecraft:"), v))
        .collect();
    if !top.is_empty() {
        ev.push(tf!("Per tipo: {}", "By type: {}", top.join(", ")));
    }
    let hot = hot_chunks(ps, 8);
    let crowded: Vec<&ChunkHot> = hot.iter().filter(|h| h.total >= 60).collect();
    for h in crowded.iter().take(6) {
        let types: Vec<String> = h.top.iter().map(|(k, v)| format!("{k} {v}")).collect();
        ev.push(tf!("Chunk affollato in {} a x={} z={} (blocchi): {} entità — {} → /execute in {} run tp {} 100 {}", "Crowded chunk in {} at x={} z={} (blocks): {} entities — {} → /execute in {} run tp {} 100 {}", h.world, h.x * 16 + 8, h.z * 16 + 8, h.total, types.join(", "), dim_name(&h.world), h.x * 16 + 8, h.z * 16 + 8));
    }
    let mut sol = Vec::new();
    let mut sev = Severity::Info;
    if !crowded.is_empty() {
        sol.push(crate::i18n::t("Visita i chunk elencati e riduci le entità (farm, allevamenti, item a terra, armor stand)", "Visit the listed chunks and reduce the entities (farms, animal pens, dropped items, armor stands)").into());
        sol.push(crate::i18n::t("Imposta chunks.entity-per-chunk-save-limit in paper-world-defaults.yml (es. experience_orb: 16, arrow: 16, snowball: 8) per evitare accumuli", "Set chunks.entity-per-chunk-save-limit in paper-world-defaults.yml (e.g. experience_orb: 16, arrow: 16, snowball: 8) to prevent build-up").into());
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
                    ev.push(tf!(
                        "Gamerule randomTickSpeed sopra il default: {:?}",
                        "Gamerule randomTickSpeed above default: {:?}",
                        gr.world_values
                    ));
                    sol.push(crate::i18n::t("Riporta randomTickSpeed a 3: valori più alti moltiplicano il costo dei random tick", "Set randomTickSpeed back to 3: higher values multiply the cost of random ticks").into());
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
                ev.push(tf!(
                    "Gamerule maxEntityCramming: {:?}",
                    "Gamerule maxEntityCramming: {:?}",
                    gr.world_values
                ));
                sol.push(crate::i18n::t("maxEntityCramming alto o disattivato permette ammassi enormi di mob: riportalo a 24 o meno", "A high or disabled maxEntityCramming allows huge mob piles: set it back to 24 or less").into());
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
        ev.push(tf!(
            "Datapack non di base: {}",
            "Non-builtin datapacks: {}",
            dp.join(", ")
        ));
    }
    out.push(Finding {
        severity: sev,
        area: crate::i18n::t("Mondo", "World").into(),
        title: if crowded.is_empty() { crate::i18n::t("Statistiche del mondo", "World statistics").into() } else { tf!("{} chunk con troppe entità", "{} chunks with too many entities", crowded.len()) },
        impact_pct: 0.0,
        evidence: ev,
        explanation: crate::i18n::t("Conteggi presi da spark al momento del profilo. Un singolo chunk con centinaia di entità può da solo causare lag.", "Counts taken by spark at the time of the profile. A single chunk with hundreds of entities can cause lag on its own.").into(),
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
        crate::i18n::t(
            "false (scritture chunk sincrone rallentano il salvataggio)",
            "false (synchronous chunk writes slow down saving)",
        ),
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
    let mut ev = vec![tf!(
        "File letti dal profilo: {}",
        "Files read from the profile: {}",
        c.cfg.file_names().join(", ")
    )];
    if lines.is_empty() {
        ev.push(
            crate::i18n::t(
                "Nessuna impostazione problematica tra quelle controllate",
                "No problematic settings among those checked",
            )
            .into(),
        );
    }
    let mut sol = Vec::new();
    push_some(&mut sol, lines.into_iter().map(Some).collect());
    out.push(Finding {
        severity: if sol.is_empty() { Severity::Info } else { Severity::Basso },
        area: crate::i18n::t("Configurazione", "Configuration").into(),
        title: if sol.is_empty() { crate::i18n::t("Configurazione server", "Server configuration").into() } else { tf!("{} impostazioni da ottimizzare", "{} settings to optimize", sol.len()) },
        impact_pct: 0.0,
        evidence: ev,
        explanation: crate::i18n::t("Valori letti dalla configurazione inclusa nel profilo. I consigli sono un punto di partenza sicuro; i valori legati alle categorie pesanti sono indicati anche nelle sezioni del main thread.", "Values read from the configuration included in the profile. The recommendations are a safe starting point; values related to heavy categories are also listed in the main thread sections.").into(),
        solutions: sol,
        culprits: Vec::new(),
    });
}
