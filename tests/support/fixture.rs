use std::collections::BTreeMap;

use sparkscope::proto::*;

pub const WINDOWS: usize = 10;
pub const SPIKES: [usize; 3] = [2, 5, 8];

pub struct N {
    class: String,
    method: String,
    desc: String,
    self_w: [f64; WINDOWS],
    kids: Vec<N>,
}

fn n(class: &str, method: &str, kids: Vec<N>) -> N {
    N {
        class: class.into(),
        method: method.into(),
        desc: String::new(),
        self_w: [0.0; WINDOWS],
        kids,
    }
}

fn leaf(class: &str, method: &str, per_window: f64) -> N {
    N {
        class: class.into(),
        method: method.into(),
        desc: String::new(),
        self_w: [per_window; WINDOWS],
        kids: Vec::new(),
    }
}

fn spiky(class: &str, method: &str, normal: f64, spike: f64) -> N {
    let mut w = [normal; WINDOWS];
    for s in SPIKES {
        w[s] = spike;
    }
    N {
        class: class.into(),
        method: method.into(),
        desc: String::new(),
        self_w: w,
        kids: Vec::new(),
    }
}

fn with_self(mut x: N, per_window: f64) -> N {
    x.self_w = [per_window; WINDOWS];
    x
}

fn desc(mut x: N, d: &str) -> N {
    x.desc = d.into();
    x
}

fn chain(frames: &[(&str, &str)], end: N) -> N {
    let mut cur = end;
    for (c, m) in frames.iter().rev() {
        cur = n(c, m, vec![cur]);
    }
    cur
}

fn inclusive(x: &N) -> [f64; WINDOWS] {
    let mut out = x.self_w;
    for k in &x.kids {
        let c = inclusive(k);
        for i in 0..WINDOWS {
            out[i] += c[i];
        }
    }
    out
}

fn flatten(x: &N, out: &mut Vec<StackTraceNode>) -> i32 {
    let refs: Vec<i32> = x.kids.iter().map(|k| flatten(k, out)).collect();
    let inc = inclusive(x);
    out.push(StackTraceNode {
        class_name: x.class.clone(),
        method_name: x.method.clone(),
        parent_line_number: 0,
        line_number: 0,
        method_desc: x.desc.clone(),
        times: inc.to_vec(),
        children_refs: refs,
    });
    (out.len() - 1) as i32
}

fn thread(name: &str, roots: Vec<N>) -> ThreadNode {
    let mut nodes = Vec::new();
    let mut refs = Vec::new();
    let mut times = [0.0; WINDOWS];
    for r in &roots {
        refs.push(flatten(r, &mut nodes));
        let inc = inclusive(r);
        for i in 0..WINDOWS {
            times[i] += inc[i];
        }
    }
    ThreadNode {
        name: name.into(),
        children: nodes,
        times: times.to_vec(),
        children_refs: refs,
    }
}

const MS: &str = "net.minecraft.server.MinecraftServer";
const SL: &str = "net.minecraft.server.level.ServerLevel";
const LV: &str = "net.minecraft.world.level.Level";
const LE: &str = "net.minecraft.world.entity.LivingEntity";
const MOB: &str = "net.minecraft.world.entity.Mob";
const SCC: &str = "net.minecraft.server.level.ServerChunkCache";
const BEL: &str = "net.minecraft.util.thread.BlockableEventLoop";

fn server_thread(paper_wait: bool) -> ThreadNode {
    let idle_w = |busy: [f64; WINDOWS]| {
        let mut w = [0.0; WINDOWS];
        for i in 0..WINDOWS {
            w[i] = (60_000.0 - busy[i]).max(500.0);
        }
        w
    };

    let villager = chain(
        &[
            ("net.minecraft.world.entity.npc.Villager", "tick"),
            (LE, "aiStep"),
            (MOB, "serverAiStep"),
            (
                "net.minecraft.world.entity.npc.Villager",
                "customServerAiStep",
            ),
        ],
        with_self(
            n(
                "net.minecraft.world.entity.ai.Brain",
                "tick",
                vec![chain(
                    &[(
                        "net.minecraft.world.entity.ai.behavior.AcquirePoi",
                        "lambda$create$5",
                    )],
                    leaf(
                        "net.minecraft.world.entity.ai.village.poi.PoiManager",
                        "findAllClosestFirstWithType",
                        4000.0,
                    ),
                )],
            ),
            2000.0,
        ),
    );
    let zombie = chain(
        &[
            ("net.minecraft.world.entity.monster.Zombie", "tick"),
            (LE, "aiStep"),
            (MOB, "serverAiStep"),
        ],
        n(
            MOB,
            "serverAiStepInner",
            vec![
                leaf(
                    "net.minecraft.world.entity.ai.goal.GoalSelector",
                    "tick",
                    2500.0,
                ),
                chain(
                    &[(
                        "net.minecraft.world.entity.ai.navigation.PathNavigation",
                        "tick",
                    )],
                    leaf(
                        "net.minecraft.world.level.pathfinder.PathFinder",
                        "findPath",
                        2500.0,
                    ),
                ),
            ],
        ),
    );
    let cow = chain(
        &[
            ("net.minecraft.world.entity.animal.Cow", "tick"),
            (LE, "aiStep"),
            (LE, "pushEntities"),
        ],
        leaf(LV, "getEntities", 3000.0),
    );
    let item = leaf("net.minecraft.world.entity.item.ItemEntity", "tick", 1500.0);
    let chicken = leaf("net.minecraft.world.entity.animal.Chicken", "tick", 2000.0);
    let entities = chain(
        &[
            ("net.minecraft.world.level.entity.EntityTickList", "forEach"),
            (SL, "lambda$tick$2"),
            (LV, "guardEntityTick"),
        ],
        n(
            SL,
            "tickNonPassenger",
            vec![villager, zombie, cow, item, chicken],
        ),
    );

    let block_entities = n(
        LV,
        "tickBlockEntities",
        vec![chain(
            &[(
                "net.minecraft.world.level.chunk.LevelChunk$BoundTickingBlockEntity",
                "tick",
            )],
            n(
                "net.minecraft.world.level.chunk.LevelChunk$BoundTickingBlockEntity",
                "tickInner",
                vec![
                    chain(
                        &[
                            (
                                "net.minecraft.world.level.block.entity.HopperBlockEntity",
                                "pushItemsTick",
                            ),
                            (
                                "net.minecraft.world.level.block.entity.HopperBlockEntity",
                                "tryMoveItems",
                            ),
                        ],
                        leaf(
                            "net.minecraft.world.level.block.entity.HopperBlockEntity",
                            "suckInItems",
                            4500.0,
                        ),
                    ),
                    leaf(
                        "net.minecraft.world.level.block.entity.AbstractFurnaceBlockEntity",
                        "serverTick",
                        800.0,
                    ),
                ],
            ),
        )],
    );

    let chunk_cache = n(
        SCC,
        "tick",
        vec![
            n(
                SCC,
                "tickChunks",
                vec![
                    chain(
                        &[(SL, "tickChunk")],
                        leaf(
                            "net.minecraft.world.level.block.state.BlockBehaviour$BlockStateBase",
                            "randomTick",
                            3000.0,
                        ),
                    ),
                    chain(
                        &[("net.minecraft.world.level.NaturalSpawner", "spawnForChunk")],
                        leaf(
                            "net.minecraft.world.level.NaturalSpawner",
                            "spawnCategoryForPosition",
                            2500.0,
                        ),
                    ),
                ],
            ),
            chain(
                &[
                    ("net.minecraft.server.level.ChunkMap", "tick"),
                    (
                        "net.minecraft.server.level.ChunkMap$TrackedEntity",
                        "updatePlayers",
                    ),
                ],
                leaf(
                    "net.minecraft.server.level.ServerEntity",
                    "sendChanges",
                    2500.0,
                ),
            ),
            leaf(
                "net.minecraft.server.level.DistanceManager",
                "runAllUpdates",
                900.0,
            ),
            leaf(
                "net.minecraft.world.level.lighting.LevelLightEngine",
                "runLightUpdates",
                600.0,
            ),
        ],
    );
    let redstone = chain(
        &[
            ("net.minecraft.world.ticks.LevelTicks", "tick"),
            (SL, "tickBlock"),
        ],
        leaf(
            "net.minecraft.world.level.block.RedStoneWireBlock",
            "updatePowerStrength",
            1500.0,
        ),
    );
    let level_tick = with_self(
        n(
            SL,
            "tick",
            vec![entities, block_entities, chunk_cache, redstone],
        ),
        600.0,
    );

    let shop_task = chain(
        &[
            (
                "org.bukkit.craftbukkit.scheduler.CraftScheduler",
                "mainThreadHeartbeat",
            ),
            ("org.bukkit.craftbukkit.scheduler.CraftTask", "run"),
        ],
        chain(
            &[
                ("com.example.shop.SaveTask", "run"),
                ("com.example.shop.db.ShopRepository", "saveBalances"),
                ("com.mysql.cj.jdbc.ClientPreparedStatement", "executeUpdate"),
            ],
            spiky(
                "com.mysql.cj.protocol.ReadAheadInputStream",
                "fill",
                1800.0,
                4800.0,
            ),
        ),
    );
    let autosave = chain(
        &[
            (MS, "autoSave"),
            (MS, "saveEverything"),
            (SL, "save"),
            ("net.minecraft.server.level.ChunkMap", "saveAllChunks"),
        ],
        spiky(
            "net.minecraft.world.level.chunk.storage.RegionFile",
            "write",
            0.0,
            12000.0,
        ),
    );
    let players = chain(
        &[
            (
                "net.minecraft.server.network.ServerConnectionListener",
                "tick",
            ),
            ("net.minecraft.network.Connection", "tick"),
            (
                "net.minecraft.server.network.ServerGamePacketListenerImpl",
                "tick",
            ),
        ],
        leaf("net.minecraft.server.level.ServerPlayer", "doTick", 1500.0),
    );
    let tick_children = with_self(
        n(MS, "tickChildren", vec![level_tick, shop_task, players]),
        400.0,
    );
    let tick_server = n(MS, "tickServer", vec![tick_children, autosave]);

    let anticheat = chain(
        &[
            (
                "net.minecraft.network.protocol.PacketUtils",
                "lambda$ensureRunningOnSameThread$0",
            ),
            (
                "net.minecraft.server.network.ServerGamePacketListenerImpl",
                "handleMovePlayer",
            ),
            ("org.bukkit.plugin.SimplePluginManager", "callEvent"),
            (
                "io.papermc.paper.plugin.manager.PaperEventManager",
                "callEvent",
            ),
            ("org.bukkit.plugin.RegisteredListener", "callEvent"),
            (
                "com.destroystokyo.paper.event.executor.asm.generated.GeneratedEventExecutor12",
                "execute",
            ),
        ],
        with_self(
            desc(
                n(
                    "com.example.anticheat.MoveListener",
                    "onMove",
                    vec![leaf(
                        "com.example.anticheat.check.SpeedCheck",
                        "check",
                        2000.0,
                    )],
                ),
                "(Lorg/bukkit/event/player/PlayerMoveEvent;)V",
            ),
            1500.0,
        ),
    );
    let rtp = chain(
        &[
            (
                "net.minecraft.network.protocol.PacketUtils",
                "lambda$ensureRunningOnSameThread$0",
            ),
            (
                "net.minecraft.server.network.ServerGamePacketListenerImpl",
                "handleChatCommand",
            ),
            ("org.bukkit.craftbukkit.CraftServer", "dispatchCommand"),
            ("com.example.rtp.RtpCommand", "onCommand"),
            ("org.bukkit.craftbukkit.CraftWorld", "getChunkAt"),
            (SCC, "getChunk"),
            (
                "net.minecraft.server.level.ServerChunkCache$MainThreadExecutor",
                "managedBlock",
            ),
            (BEL, "managedBlock"),
            (BEL, "waitForTasks"),
            ("java.util.concurrent.locks.LockSupport", "parkNanos"),
        ],
        leaf("jdk.internal.misc.Unsafe", "park", 1200.0),
    );
    let tasks = chain(
        &[(MS, "pollTask"), (BEL, "doRunTask")],
        n("net.minecraft.server.TickTask", "run", vec![anticheat, rtp]),
    );

    let mut busy = [0.0; WINDOWS];
    let a = inclusive(&tick_server);
    let b = inclusive(&tasks);
    for i in 0..WINDOWS {
        busy[i] = a[i] + b[i];
    }
    let idle = N {
        class: "jdk.internal.misc.Unsafe".into(),
        method: "park".into(),
        desc: String::new(),
        self_w: idle_w(busy),
        kids: Vec::new(),
    };
    let wait = if paper_wait {
        n(
            MS,
            "recordTaskExecutionTimeWhileWaiting",
            vec![
                tasks,
                chain(
                    &[("java.util.concurrent.locks.LockSupport", "parkNanos")],
                    idle,
                ),
            ],
        )
    } else {
        n(
            MS,
            "waitUntilNextTick",
            vec![n(
                MS,
                "managedBlock",
                vec![
                    tasks,
                    chain(
                        &[
                            (BEL, "waitForTasks"),
                            ("java.util.concurrent.locks.LockSupport", "parkNanos"),
                        ],
                        idle,
                    ),
                ],
            )],
        )
    };
    let root = chain(
        &[("java.lang.Thread", "run"), (MS, "lambda$spin$2")],
        n(MS, "runServer", vec![tick_server, wait]),
    );
    thread("Server thread", vec![root])
}

fn other_threads() -> Vec<ThreadNode> {
    vec![
        thread(
            "Craft Scheduler Thread - 7 - EconomyShop",
            vec![chain(
                &[
                    ("java.lang.Thread", "run"),
                    ("org.bukkit.craftbukkit.scheduler.CraftAsyncTask", "run"),
                    ("com.example.shop.sync.PriceSync", "run"),
                ],
                n(
                    "java.net.http.HttpClient",
                    "send",
                    vec![leaf("sun.nio.ch.SocketDispatcher", "read0", 52_000.0)],
                ),
            )],
        ),
        thread(
            "Netty Epoll Server IO #1",
            vec![chain(
                &[
                    ("java.lang.Thread", "run"),
                    ("io.netty.channel.epoll.EpollEventLoop", "run"),
                ],
                n(
                    "io.netty.channel.epoll.EpollEventLoop",
                    "process",
                    vec![
                        leaf("io.netty.channel.epoll.Native", "epollWait", 55_000.0),
                        leaf(
                            "io.netty.handler.codec.ByteToMessageDecoder",
                            "channelRead",
                            5_000.0,
                        ),
                    ],
                ),
            )],
        ),
        thread(
            "Worker-Main-2",
            vec![chain(
                &[
                    ("java.lang.Thread", "run"),
                    ("java.util.concurrent.ForkJoinWorkerThread", "run"),
                ],
                n(
                    "java.util.concurrent.ForkJoinPool",
                    "runWorker",
                    vec![
                        leaf(
                            "net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator",
                            "fillFromNoise",
                            20_000.0,
                        ),
                        leaf("jdk.internal.misc.Unsafe", "park", 40_000.0),
                    ],
                ),
            )],
        ),
    ]
}

fn config() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert("server.properties".into(), r#"{"view-distance":"12","simulation-distance":"10","sync-chunk-writes":"true","network-compression-threshold":"256","entity-broadcast-range-percentage":"100"}"#.into());
    m.insert("spigot.yml".into(), r#"{"world-settings":{"default":{"entity-activation-range":{"animals":32,"monsters":32,"raiders":64,"misc":16,"water":16,"villagers":32,"flying-monsters":32},"merge-radius":{"item":0.5,"exp":-1.0},"ticks-per":{"hopper-transfer":8,"hopper-check":1},"hopper-amount":1,"nerf-spawner-mobs":false,"tick-inactive-villagers":true,"mob-spawn-range":8,"entity-tracking-range":{"players":128,"animals":96,"monsters":96,"misc":96}}}}"#.into());
    m.insert("bukkit.yml".into(), r#"{"spawn-limits":{"monsters":70,"animals":10,"water-animals":5,"ambient":15},"ticks-per":{"monster-spawns":1}}"#.into());
    m.insert("paper/".into(), r#"{"global.yml":{},"world-defaults.yml":{"chunks":{"max-auto-save-chunks-per-tick":24,"prevent-moving-into-unloaded-chunks":false,"delay-chunk-unloads-by":"10s"},"collisions":{"max-entity-collisions":8},"hopper":{"disable-move-event":false,"ignore-occluding-blocks":false},"misc":{"redstone-implementation":"VANILLA"},"environment":{"optimize-explosions":false},"tick-rates":{"mob-spawner":1},"entities":{"spawning":{"per-player-mob-spawns":true},"armor-stands":{"tick":true}}}}"#.into());
    m
}

fn src(name: &str, version: &str) -> PluginOrModMetadata {
    PluginOrModMetadata {
        name: name.into(),
        version: version.into(),
        author: "example".into(),
        description: String::new(),
        builtin: false,
    }
}

pub fn sample() -> SamplerData {
    sample_with(false)
}

pub fn sample_paper_wait() -> SamplerData {
    sample_with(true)
}

fn sample_with(paper_wait: bool) -> SamplerData {
    let start = 1_762_420_000_000i64;
    let mut windows = BTreeMap::new();
    let ids: Vec<i32> = (0..WINDOWS as i32).map(|i| 29_373_000 + i).collect();
    for (i, id) in ids.iter().enumerate() {
        let spike = SPIKES.contains(&i);
        windows.insert(
            *id,
            WindowStatistics {
                ticks: if spike { 900 } else { 1200 },
                cpu_process: if spike { 0.82 } else { 0.62 },
                cpu_system: 0.95,
                tps: if spike { 15.0 } else { 20.0 },
                mspt_median: if spike { 58.0 } else { 34.0 },
                mspt_max: if spike { 420.0 } else { 48.0 },
                players: 42,
                entities: 4200,
                tile_entities: 1800,
                chunks: 5400,
                start_time: start + i as i64 * 60_000,
                end_time: start + (i as i64 + 1) * 60_000,
                duration: 60_000,
            },
        );
    }
    let mut chunk_a = BTreeMap::new();
    chunk_a.insert("minecraft:villager".to_string(), 180);
    chunk_a.insert("minecraft:item".to_string(), 120);
    chunk_a.insert("minecraft:zombie".to_string(), 40);
    let mut chunk_b = BTreeMap::new();
    chunk_b.insert("minecraft:cow".to_string(), 120);
    let mut ec = BTreeMap::new();
    for (k, v) in [
        ("minecraft:villager", 620),
        ("minecraft:item", 900),
        ("minecraft:cow", 700),
        ("minecraft:zombie", 400),
        ("minecraft:chicken", 500),
    ] {
        ec.insert(k.to_string(), v);
    }
    let mut gc = BTreeMap::new();
    gc.insert(
        "G1 Young Generation".to_string(),
        platform_statistics::Gc {
            total: 400,
            avg_time: 35.0,
            avg_frequency: 1500.0,
        },
    );
    gc.insert(
        "G1 Old Generation".to_string(),
        platform_statistics::Gc {
            total: 3,
            avg_time: 900.0,
            avg_frequency: 200_000.0,
        },
    );
    let mut sources = BTreeMap::new();
    sources.insert("EconomyShop".to_string(), src("EconomyShop", "2.4.1"));
    sources.insert("AntiCheatX".to_string(), src("AntiCheatX", "5.0.0"));
    sources.insert("WildTeleport".to_string(), src("WildTeleport", "1.3"));
    sources.insert("spark".to_string(), src("spark", "1.10.124"));
    let mut class_sources = BTreeMap::new();
    for (c, s) in [
        ("com.example.shop.SaveTask", "EconomyShop"),
        ("com.example.shop.db.ShopRepository", "EconomyShop"),
        ("com.example.shop.sync.PriceSync", "EconomyShop"),
        ("com.example.anticheat.MoveListener", "AntiCheatX"),
        ("com.example.anticheat.check.SpeedCheck", "AntiCheatX"),
        ("com.example.rtp.RtpCommand", "WildTeleport"),
    ] {
        class_sources.insert(c.to_string(), s.to_string());
    }
    let mut threads = vec![server_thread(paper_wait)];
    threads.extend(other_threads());
    SamplerData {
        metadata: Some(SamplerMetadata {
            creator: Some(CommandSenderMetadata {
                r#type: 1,
                name: "Admin".into(),
                unique_id: String::new(),
            }),
            start_time: start,
            end_time: start + 600_000,
            interval: 4000,
            thread_dumper: None,
            data_aggregator: Some(sampler_metadata::DataAggregator {
                r#type: 0,
                thread_grouper: 0,
                tick_length_threshold: 0,
                number_of_included_ticks: 0,
            }),
            comment: "profilo di prova".into(),
            platform_metadata: Some(PlatformMetadata {
                r#type: 0,
                name: "Paper".into(),
                version: "1.21.4-232".into(),
                minecraft_version: "1.21.4".into(),
                spark_data_version: 2,
                brand: "Paper".into(),
                spark_version: "1.10.124".into(),
            }),
            platform_statistics: Some(PlatformStatistics {
                memory: Some(platform_statistics::Memory {
                    heap: Some(MemoryUsage {
                        used: 7_200_000_000,
                        committed: 8_000_000_000,
                        init: 2_000_000_000,
                        max: 8_589_934_592,
                    }),
                    ..Default::default()
                }),
                gc,
                uptime: 86_400_000,
                tps: Some(platform_statistics::Tps {
                    last1m: 17.8,
                    last5m: 18.6,
                    last15m: 19.1,
                    game_target_tps: 20,
                }),
                mspt: Some(platform_statistics::Mspt {
                    last1m: Some(RollingAverageValues {
                        mean: 44.0,
                        max: 420.0,
                        min: 20.0,
                        median: 36.0,
                        percentile95: 120.0,
                    }),
                    last5m: None,
                    game_max_ideal_mspt: 50,
                }),
                ping: None,
                player_count: 42,
                world: Some(WorldStatistics {
                    total_entities: 4200,
                    entity_counts: ec,
                    worlds: vec![world_statistics::World {
                        name: "world".into(),
                        total_entities: 4200,
                        regions: vec![world_statistics::Region {
                            total_entities: 460,
                            chunks: vec![
                                world_statistics::Chunk {
                                    x: 12,
                                    z: -40,
                                    total_entities: 340,
                                    entity_counts: chunk_a,
                                },
                                world_statistics::Chunk {
                                    x: -3,
                                    z: 7,
                                    total_entities: 120,
                                    entity_counts: chunk_b,
                                },
                            ],
                        }],
                    }],
                    game_rules: vec![world_statistics::GameRule {
                        name: "randomTickSpeed".into(),
                        default_value: "3".into(),
                        world_values: [("world".to_string(), "3".to_string())]
                            .into_iter()
                            .collect(),
                    }],
                    data_packs: Vec::new(),
                }),
                online_mode: 2,
            }),
            system_statistics: Some(SystemStatistics {
                cpu: Some(system_statistics::Cpu {
                    threads: 4,
                    process_usage: Some(system_statistics::cpu::Usage {
                        last1m: 0.62,
                        last15m: 0.55,
                    }),
                    system_usage: Some(system_statistics::cpu::Usage {
                        last1m: 0.95,
                        last15m: 0.9,
                    }),
                    model_name: "AMD Ryzen 9 5950X".into(),
                }),
                memory: Some(system_statistics::Memory {
                    physical: Some(system_statistics::memory::MemoryPool {
                        used: 30_000_000_000,
                        total: 34_359_738_368,
                    }),
                    swap: Some(system_statistics::memory::MemoryPool {
                        used: 1_073_741_824,
                        total: 4_294_967_296,
                    }),
                }),
                java: Some(system_statistics::Java {
                    vendor: "Eclipse Adoptium".into(),
                    version: "21.0.5".into(),
                    vendor_version: "Temurin-21.0.5+11".into(),
                    vm_args: "-Xms2G -Xmx8G -XX:+UseG1GC".into(),
                }),
                ..Default::default()
            }),
            server_configurations: config(),
            number_of_ticks: 7 * 1200 + 3 * 900,
            sources,
            extra_platform_metadata: BTreeMap::new(),
            sampler_mode: 0,
            sampler_engine: 1,
            sampler_engine_version: "v4.1".into(),
            metrics: None,
        }),
        threads,
        class_sources,
        method_sources: BTreeMap::new(),
        line_sources: BTreeMap::new(),
        time_windows: ids,
        time_window_statistics: windows,
        channel_info: None,
    }
}

#[allow(dead_code)]
pub fn sample_alloc() -> SamplerData {
    let mut d = sample();
    let main = thread(
        "Server thread",
        vec![chain(
            &[(MS, "runServer"), (MS, "tickServer"), (MS, "tickChildren")],
            n(
                SL,
                "tick",
                vec![
                    chain(&[(LE, "tick")], leaf("java.util.ArrayList", "grow", 2.0e8)),
                    chain(
                        &[
                            ("org.bukkit.plugin.SimplePluginManager", "callEvent"),
                            ("com.example.anticheat.MoveListener", "onMove"),
                        ],
                        n(
                            "org.bukkit.craftbukkit.entity.CraftEntity",
                            "getLocation",
                            vec![leaf("org.bukkit.Location", "<init>", 6.0e8)],
                        ),
                    ),
                    chain(
                        &[
                            ("org.bukkit.plugin.SimplePluginManager", "callEvent"),
                            ("com.example.anticheat.check.SpeedCheck", "check"),
                            ("java.util.stream.ReferencePipeline", "collect"),
                        ],
                        leaf("java.util.stream.ReduceOps$3", "makeRef", 3.0e8),
                    ),
                    chain(
                        &[
                            (
                                "org.bukkit.craftbukkit.scheduler.CraftScheduler",
                                "mainThreadHeartbeat",
                            ),
                            ("com.example.shop.SaveTask", "run"),
                            ("java.lang.String", "format"),
                        ],
                        leaf("java.util.Formatter", "format", 2.0e8),
                    ),
                ],
            ),
        )],
    );
    let netty = thread(
        "Netty Epoll Server IO #1",
        vec![chain(
            &[("io.netty.channel.epoll.EpollEventLoop", "run")],
            leaf("io.netty.buffer.PoolArena", "allocate", 1.0e8),
        )],
    );
    d.threads = vec![main, netty];
    if let Some(m) = d.metadata.as_mut() {
        m.sampler_mode = 1;
        m.interval = 524_287;
    }
    d
}
