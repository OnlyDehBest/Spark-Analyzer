use crate::model::{Node, simple_class};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord, serde::Serialize)]
pub enum Cat {
    EntityTick,
    EntityMove,
    Collisions,
    MobAi,
    Pathfinding,
    Villagers,
    ItemsXp,
    BlockEntities,
    Hoppers,
    Spawners,
    Redstone,
    Fluids,
    ChunkTick,
    MobSpawning,
    ScheduledTicks,
    ChunkLoadSync,
    ChunkGen,
    ChunkSave,
    ChunkSystem,
    Lighting,
    EntityTracking,
    Players,
    Network,
    Commands,
    PluginTasks,
    PluginEvents,
    ServerTasks,
    Maps,
    Explosions,
    Scoreboard,
    Logging,
    Exceptions,
    SyncDatabase,
    SyncNetworkIo,
    SyncFileIo,
    Blocking,
    Serialization,
    TextFormatting,
    Gc,
    Jit,
    Native,
    WorldTickOther,
    ServerTickOther,
    Other,
}

impl Cat {
    pub fn label(self) -> &'static str {
        match self {
            Cat::EntityTick => "Tick entità",
            Cat::EntityMove => "Movimento entità",
            Cat::Collisions => "Collisioni / ricerche entità",
            Cat::MobAi => "AI dei mob (goal/brain)",
            Cat::Pathfinding => "Pathfinding",
            Cat::Villagers => "Villager / POI",
            Cat::ItemsXp => "Item e XP a terra",
            Cat::BlockEntities => "Block entity (tile)",
            Cat::Hoppers => "Hopper",
            Cat::Spawners => "Spawner",
            Cat::Redstone => "Redstone / aggiornamenti blocchi",
            Cat::Fluids => "Fluidi",
            Cat::ChunkTick => "Tick chunk (random tick, meteo)",
            Cat::MobSpawning => "Spawn naturale dei mob",
            Cat::ScheduledTicks => "Tick schedulati blocchi/fluidi",
            Cat::ChunkLoadSync => "Caricamento chunk sul main thread",
            Cat::ChunkGen => "Generazione chunk",
            Cat::ChunkSave => "Salvataggio chunk / autosave",
            Cat::ChunkSystem => "Gestione chunk (ticket, load/unload)",
            Cat::Lighting => "Motore di luce",
            Cat::EntityTracking => "Tracking entità (invio pacchetti)",
            Cat::Players => "Tick giocatori",
            Cat::Network => "Rete / pacchetti in ingresso",
            Cat::Commands => "Comandi",
            Cat::PluginTasks => "Task schedulati (sync)",
            Cat::PluginEvents => "Eventi Bukkit",
            Cat::ServerTasks => "Coda task del main thread",
            Cat::Maps => "Mappe",
            Cat::Explosions => "Esplosioni",
            Cat::Scoreboard => "Scoreboard / team",
            Cat::Logging => "Logging su console",
            Cat::Exceptions => "Creazione eccezioni",
            Cat::SyncDatabase => "Database sincrono",
            Cat::SyncNetworkIo => "Rete sincrona (HTTP/socket)",
            Cat::SyncFileIo => "I/O su file sincrono",
            Cat::Blocking => "Attese bloccanti (lock/sleep/future)",
            Cat::Serialization => "Serializzazione JSON/NBT/YAML",
            Cat::TextFormatting => "Formattazione testo / placeholder",
            Cat::Gc => "Garbage collector",
            Cat::Jit => "Compilatore JIT",
            Cat::Native => "Codice nativo",
            Cat::WorldTickOther => "Tick mondo (altro)",
            Cat::ServerTickOther => "Tick server (altro)",
            Cat::Other => "Altro",
        }
    }

    pub fn key(self) -> String {
        format!("{self:?}")
    }

    pub fn is_sync_hazard(self) -> bool {
        matches!(
            self,
            Cat::SyncDatabase
                | Cat::SyncNetworkIo
                | Cat::SyncFileIo
                | Cat::Blocking
                | Cat::ChunkLoadSync
                | Cat::ChunkGen
        )
    }
}

fn starts_any(s: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|p| s.starts_with(p))
}

pub fn categorize(node: &Node) -> Option<Cat> {
    let class = node.class.as_str();
    let method = node.method.as_str();
    if class.is_empty() {
        return native_category(method);
    }

    if starts_any(
        class,
        &[
            "java.sql.",
            "com.mysql.",
            "org.mariadb.",
            "org.sqlite.",
            "org.h2.",
            "org.postgresql.",
            "com.zaxxer.hikari.",
            "redis.clients.jedis.",
            "io.lettuce.core.",
            "com.mongodb.",
            "org.mongodb.",
            "org.redisson.",
            "org.jooq.",
            "org.hibernate.",
        ],
    ) {
        return Some(Cat::SyncDatabase);
    }
    if starts_any(
        class,
        &[
            "java.net.http.",
            "jdk.internal.net.http.",
            "okhttp3.",
            "okio.",
            "sun.net.www.",
            "java.net.HttpURLConnection",
            "java.net.URL",
            "java.net.Socket",
            "java.net.InetAddress",
            "java.net.Inet",
            "sun.nio.ch.SocketChannelImpl",
            "sun.nio.ch.NioSocketImpl",
            "java.net.SocketInputStream",
            "sun.security.ssl.",
            "org.apache.http.",
        ],
    ) {
        return Some(Cat::SyncNetworkIo);
    }
    if starts_any(
        class,
        &[
            "java.io.FileInputStream",
            "java.io.FileOutputStream",
            "java.io.RandomAccessFile",
            "java.io.FileDescriptor",
            "sun.nio.ch.FileChannelImpl",
            "sun.nio.fs.",
            "java.nio.file.Files",
            "java.io.File",
            "java.io.UnixFileSystem",
            "java.io.WinNTFileSystem",
        ],
    ) {
        return Some(Cat::SyncFileIo);
    }
    if starts_any(
        class,
        &[
            "org.apache.logging.log4j.",
            "org.slf4j.",
            "java.util.logging.",
            "net.minecrell.terminalconsole.",
            "org.jline.",
            "jline.",
        ],
    ) {
        return Some(Cat::Logging);
    }
    if class == "java.lang.Throwable" && (method == "fillInStackTrace" || method == "<init>")
        || class == "java.lang.StackTraceElement"
        || class == "java.lang.StackWalker"
    {
        return Some(Cat::Exceptions);
    }
    if is_blocking_frame(class, method) {
        return Some(Cat::Blocking);
    }
    if starts_any(
        class,
        &[
            "com.google.gson.",
            "com.fasterxml.jackson.",
            "org.yaml.snakeyaml.",
            "org.json.",
            "org.spongepowered.configurate.",
        ],
    ) {
        return Some(Cat::Serialization);
    }
    if starts_any(
        class,
        &[
            "me.clip.placeholderapi.",
            "net.kyori.adventure.text.minimessage.",
            "net.kyori.adventure.text.serializer.",
            "net.md_5.bungee.api.chat.",
            "net.md_5.bungee.chat.",
        ],
    ) {
        return Some(Cat::TextFormatting);
    }
    if class.starts_with("net.minecraft.world.level.redstone.")
        || class.starts_with("alternate.current.")
    {
        return Some(Cat::Redstone);
    }
    if class.starts_with("net.minecraft.world.level.levelgen.")
        || class.starts_with("net.minecraft.world.level.biome.") && method != "getBiome"
    {
        return Some(Cat::ChunkGen);
    }
    if class.starts_with("net.minecraft.world.level.lighting.")
        || class.starts_with("ca.spottedleaf.starlight.")
        || class.starts_with("ca.spottedleaf.moonrise.patches.starlight.")
    {
        return Some(Cat::Lighting);
    }
    if class.starts_with("net.minecraft.world.level.pathfinder.")
        || class.starts_with("net.minecraft.world.entity.ai.navigation.")
    {
        return Some(Cat::Pathfinding);
    }
    if class.starts_with("net.minecraft.world.entity.ai.village.poi.") {
        return Some(Cat::Villagers);
    }
    if class.starts_with("net.minecraft.world.entity.ai.goal.")
        || class.starts_with("net.minecraft.world.entity.ai.behavior.")
        || class.starts_with("net.minecraft.world.entity.ai.sensing.")
        || class.starts_with("net.minecraft.world.entity.ai.Brain")
    {
        return Some(Cat::MobAi);
    }
    if class.starts_with("net.minecraft.world.level.saveddata.maps.") {
        return Some(Cat::Maps);
    }
    if class.starts_with("net.minecraft.world.scores.") {
        return Some(Cat::Scoreboard);
    }
    if class.starts_with("net.minecraft.world.level.chunk.storage.") {
        return Some(Cat::ChunkSave);
    }

    let simple = simple_class(class);
    let outer = match simple.find('$') {
        Some(i) => &simple[..i],
        None => simple,
    };
    let inner = simple.rsplit('$').next().unwrap_or(simple);

    match outer {
        "SimplePluginManager"
        | "PaperPluginManagerImpl"
        | "PaperEventManager"
        | "RegisteredListener"
        | "TimedRegisteredListener"
        | "EventExecutor"
        | "MethodHandleEventExecutor"
        | "StaticMethodHandleEventExecutor"
        | "ASMEventExecutorGenerator" => {
            if method == "callEvent"
                || method == "execute"
                || outer == "RegisteredListener"
                || outer.ends_with("EventExecutor")
            {
                return Some(Cat::PluginEvents);
            }
        }
        "CraftScheduler"
        | "CraftTask"
        | "CraftAsyncTask"
        | "CraftFuture"
        | "FoliaGlobalRegionScheduler"
        | "FoliaRegionScheduler"
        | "FoliaEntityScheduler"
        | "EntityScheduler"
        | "GlobalRegionScheduler" => {
            return Some(Cat::PluginTasks);
        }
        "SimpleCommandMap" | "CraftServer"
            if method == "dispatchCommand" || method == "dispatch" =>
        {
            return Some(Cat::Commands);
        }
        "Commands"
        | "CommandDispatcher"
        | "BukkitCommandWrapper"
        | "VanillaCommandWrapper"
        | "PluginCommand"
        | "CommandManager"
            if method != "<init>" =>
        {
            return Some(Cat::Commands);
        }
        _ => {}
    }
    if class.starts_with("com.mojang.brigadier.") {
        return Some(Cat::Commands);
    }

    match outer {
        "HopperBlockEntity"
        | "TileEntityHopper"
        | "HopperMinecartEntity"
        | "MinecartHopper"
        | "HopperBlock"
        | "BlockHopper" => return Some(Cat::Hoppers),
        "SpawnerBlockEntity"
        | "BaseSpawner"
        | "TileEntityMobSpawner"
        | "MobSpawnerAbstract"
        | "MobSpawnerLogic"
        | "MobSpawnerBlockEntity"
        | "TrialSpawner"
        | "TrialSpawnerBlockEntity" => return Some(Cat::Spawners),
        "RedStoneWireBlock"
        | "RedstoneWireBlock"
        | "BlockRedstoneWire"
        | "DiodeBlock"
        | "AbstractRedstoneGateBlock"
        | "RepeaterBlock"
        | "ComparatorBlock"
        | "PistonBaseBlock"
        | "PistonBlock"
        | "BlockPiston"
        | "PistonMovingBlockEntity"
        | "PistonBlockEntity"
        | "ObserverBlock"
        | "BlockObserver"
        | "NeighborUpdater"
        | "CollectingNeighborUpdater"
        | "InstantNeighborUpdater"
        | "ChainRestrictedNeighborUpdater"
        | "RedstoneTorchBlock"
        | "TargetBlock"
        | "DetectorRailBlock"
        | "PoweredRailBlock"
        | "RedstoneWireTurbo"
        | "LevelRedstone" => return Some(Cat::Redstone),
        "FlowingFluid" | "FlowableFluid" | "FluidTypeFlowing" | "LavaFluid" | "WaterFluid"
        | "LiquidBlock" | "FluidBlock" | "FluidState" | "Fluid" => {
            if method != "getType" && method != "getAmount" {
                return Some(Cat::Fluids);
            }
        }
        "ItemEntity"
        | "EntityItem"
        | "ExperienceOrb"
        | "EntityExperienceOrb"
        | "ExperienceOrbEntity" => return Some(Cat::ItemsXp),
        "Villager"
        | "EntityVillager"
        | "VillagerEntity"
        | "AbstractVillager"
        | "PoiManager"
        | "PointOfInterestStorage"
        | "VillagePlace"
        | "WanderingTrader"
        | "AcquirePoi"
        | "YieldJobSite"
        | "PoiSection" => return Some(Cat::Villagers),
        "GoalSelector"
        | "PathfinderGoalSelector"
        | "WrappedGoal"
        | "PathfinderGoalWrapped"
        | "PrioritizedGoal"
        | "Brain"
        | "BehaviorController"
        | "Sensor"
        | "GateBehavior"
        | "Behavior"
        | "MultiTask"
        | "SensorType"
        | "NearestLivingEntitySensor"
        | "NearestItemSensor"
        | "TargetingConditions"
        | "PathfinderTargetCondition"
        | "TargetPredicate"
        | "LookControl"
        | "MoveControl"
        | "JumpControl" => return Some(Cat::MobAi),
        "PathNavigation"
        | "NavigationAbstract"
        | "EntityNavigation"
        | "GroundPathNavigation"
        | "Navigation"
        | "PathFinder"
        | "Pathfinder"
        | "PathNodeNavigator"
        | "NodeEvaluator"
        | "WalkNodeEvaluator"
        | "PathfinderNormal"
        | "LandPathNodeMaker"
        | "PathNodeMaker"
        | "Path"
        | "PathEntity"
        | "BinaryHeap"
        | "PathMinHeap" => return Some(Cat::Pathfinding),
        "NaturalSpawner"
        | "SpawnerCreature"
        | "SpawnHelper"
        | "PhantomSpawner"
        | "PatrolSpawner"
        | "WanderingTraderSpawner"
        | "VillageSiege"
        | "CatSpawner"
        | "MobSpawnerPhantom"
        | "MobSpawnerPatrol" => return Some(Cat::MobSpawning),
        "LevelLightEngine"
        | "ThreadedLevelLightEngine"
        | "LightEngineThreaded"
        | "ServerLightingProvider"
        | "LightingProvider"
        | "LightEngine"
        | "BlockLightEngine"
        | "SkyLightEngine"
        | "ChunkLightProvider"
        | "StarLightInterface"
        | "StarLightEngine"
        | "StarLightLightingProvider" => return Some(Cat::Lighting),
        "ChunkGenerator"
        | "NoiseBasedChunkGenerator"
        | "ChunkGeneratorAbstract"
        | "WorldGenRegion"
        | "RegionLimitedWorldAccess"
        | "ChunkRegion"
        | "ChunkStatusTasks"
        | "StructureStart"
        | "StructureManager"
        | "NoiseChunk"
        | "Aquifer"
        | "ProtoChunk"
        | "ChunkGenerationTask"
        | "ChunkUpgrader" => return Some(Cat::ChunkGen),
        "ChunkSerializer"
        | "SerializableChunkData"
        | "RegionFile"
        | "RegionFileStorage"
        | "RegionFileCache"
        | "IOWorker"
        | "StorageIoWorker"
        | "ChunkStorage"
        | "IChunkLoader"
        | "VersionedChunkStorage"
        | "RegionBasedStorage"
        | "SectionStorage"
        | "EntityStorage"
        | "ChunkDataSerializer" => {
            if method.contains("read") || method.contains("load") || method == "parse" {
                return Some(Cat::ChunkLoadSync);
            }
            return Some(Cat::ChunkSave);
        }
        "TrackedEntity"
        | "EntityTracker"
        | "EntityTrackerEntry"
        | "ServerEntity"
        | "EntityTrackerEntryState" => return Some(Cat::EntityTracking),
        "MapItemSavedData" | "WorldMap" | "MapState" | "MapItem" | "ItemWorldMap"
        | "FilledMapItem" | "CraftMapRenderer" | "CraftMapView" => return Some(Cat::Maps),
        "Explosion" | "ServerExplosion" | "ExplosionImpl" => return Some(Cat::Explosions),
        "ServerScoreboard"
        | "Scoreboard"
        | "ScoreboardServer"
        | "PlayerTeam"
        | "ScoreboardTeam"
        | "CraftScoreboard"
        | "CraftScoreboardManager"
        | "CraftTeam" => return Some(Cat::Scoreboard),
        "LevelTicks" | "WorldTickScheduler" | "TickListServer" | "LevelChunkTicks"
        | "ChunkTickScheduler" => return Some(Cat::ScheduledTicks),
        "DistanceManager"
        | "ChunkTicketManager"
        | "ChunkMapDistance"
        | "ChunkHolder"
        | "PlayerChunk"
        | "ChunkHolderManager"
        | "ChunkTaskScheduler"
        | "TicketManager"
        | "NewChunkHolder"
        | "RegionizedPlayerChunkLoader"
        | "PlayerChunkLoader"
        | "ChunkTicketSystem" => return Some(Cat::ChunkSystem),
        "ServerConnectionListener"
        | "ServerConnection"
        | "ServerNetworkIo"
        | "Connection"
        | "NetworkManager"
        | "ClientConnection"
        | "PacketUtils"
        | "PlayerConnectionUtils"
        | "NetworkThreadUtils"
        | "ServerGamePacketListenerImpl"
        | "PlayerConnection"
        | "ServerPlayNetworkHandler"
        | "ServerCommonPacketListenerImpl"
        | "ServerCommonNetworkHandler"
        | "ServerLoginPacketListenerImpl"
        | "ServerConfigurationPacketListenerImpl" => return Some(Cat::Network),
        _ => {}
    }

    if (inner.ends_with("BlockEntity")
        || outer.starts_with("TileEntity")
        || outer == "TickingBlockEntity"
        || outer == "BoundTickingBlockEntity"
        || outer == "RebindableTickingBlockEntityWrapper"
        || outer == "BlockEntityTickInvoker")
        && (method.contains("tick") || method.contains("Tick") || method.starts_with("lambda$")) {
            return Some(Cat::BlockEntities);
        }

    match (outer, method) {
        (
            "ServerChunkCache"
            | "ChunkProviderServer"
            | "ServerChunkManager"
            | "ServerChunkLoadingManager",
            m,
        ) if m.starts_with("getChunk")
            || m == "getChunkAtImmediately"
            || m == "getChunkAtIfLoadedMainThread"
            || m == "syncLoad"
            || m == "getChunkBlocking"
            || m == "getChunkFutureMainThread" =>
        {
            return Some(Cat::ChunkLoadSync);
        }
        (
            "ChunkMap"
            | "PlayerChunkMap"
            | "ThreadedAnvilChunkStorage"
            | "ServerChunkLoadingManager",
            m,
        ) if m.starts_with("save") || m.starts_with("flush") => return Some(Cat::ChunkSave),
        (
            "ChunkMap"
            | "PlayerChunkMap"
            | "ThreadedAnvilChunkStorage"
            | "ServerChunkLoadingManager",
            m,
        ) if m.starts_with("scheduleChunkLoad") || m == "readChunk" || m == "read" => {
            return Some(Cat::ChunkLoadSync);
        }
        (
            "ChunkMap"
            | "PlayerChunkMap"
            | "ThreadedAnvilChunkStorage"
            | "ServerChunkLoadingManager",
            m,
        ) if m == "tick"
            || m.starts_with("process")
            || m.contains("Unload")
            || m.contains("Tickets") =>
        {
            return Some(Cat::ChunkSystem);
        }
        (
            "ChunkMap" | "PlayerChunkMap" | "ThreadedAnvilChunkStorage",
            "move" | "updatePlayerStatus",
        ) => return Some(Cat::Players),
        ("ChunkMap" | "PlayerChunkMap" | "ThreadedAnvilChunkStorage", m)
            if m.contains("Track") || m == "newTrackerTick" =>
        {
            return Some(Cat::EntityTracking);
        }
        ("CraftWorld" | "CraftChunk", m) if m.starts_with("getChunkAt") || m == "loadChunk" => {
            return Some(Cat::ChunkLoadSync);
        }
        ("ServerChunkCache" | "ChunkProviderServer" | "ServerChunkManager", m)
            if m == "tickChunks" || m.starts_with("tickChunk") =>
        {
            return Some(Cat::ChunkTick);
        }
        ("ServerChunkCache" | "ChunkProviderServer" | "ServerChunkManager", m)
            if m.starts_with("save") =>
        {
            return Some(Cat::ChunkSave);
        }
        ("ServerChunkCache" | "ChunkProviderServer" | "ServerChunkManager", m)
            if m.contains("DistanceManager") || m.contains("Tickets") || m == "pollTask" =>
        {
            return Some(Cat::ChunkSystem);
        }
        ("ServerLevel" | "WorldServer" | "ServerWorld", m)
            if m == "tickChunk"
                || m == "tickPrecipitation"
                || m == "tickIceAndSnow"
                || m.contains("RandomTick")
                || m == "tickThunder" =>
        {
            return Some(Cat::ChunkTick);
        }
        ("ServerLevel" | "WorldServer" | "ServerWorld", m)
            if m == "tickBlock" || m == "tickFluid" =>
        {
            return Some(Cat::ScheduledTicks);
        }
        ("ServerLevel" | "WorldServer" | "ServerWorld", m) if m == "tickCustomSpawners" => {
            return Some(Cat::MobSpawning);
        }
        ("ServerLevel" | "WorldServer" | "ServerWorld" | "Level" | "World", m)
            if m == "tickNonPassenger"
                || m == "tickPassenger"
                || m == "guardEntityTick"
                || m == "tickEntity"
                || m == "entityJoinedWorld" =>
        {
            return Some(Cat::EntityTick);
        }
        (
            "Level" | "World" | "ServerLevel" | "ServerWorld",
            "tickBlockEntities" | "tickTileEntities",
        ) => return Some(Cat::BlockEntities),
        ("ServerLevel" | "WorldServer" | "ServerWorld", m) if m.starts_with("save") => {
            return Some(Cat::ChunkSave);
        }
        ("ServerLevel" | "WorldServer" | "ServerWorld", "tick") => {
            return Some(Cat::WorldTickOther);
        }
        ("EntityTickList" | "EntityList", "forEach") => return Some(Cat::EntityTick),
        (
            "Level"
            | "World"
            | "EntityGetter"
            | "CollisionGetter"
            | "LevelEntityGetter"
            | "LevelEntityGetterAdapter"
            | "EntitySectionStorage"
            | "EntitySection"
            | "ClassInstanceMultiMap"
            | "EntityLookup"
            | "ChunkEntitySlices"
            | "SectionedEntityCache"
            | "SimpleEntityLookup"
            | "CollisionView"
            | "EntityView"
            | "WorldUtil",
            m,
        ) if m.starts_with("getEntities")
            || m.contains("Collision")
            || m.starts_with("getEntitiesOfClass")
            || m == "noCollision"
            || m == "getOtherEntities"
            || m == "getEntitiesByClass"
            || m == "getEntitiesByType"
            || m.starts_with("getNearby")
            || m.starts_with("getNearest")
            || m == "isUnobstructed"
            || m == "collidesWithAnyBlock"
            || m == "getBlockCollisions"
            || m == "findSupportingBlock"
            || m == "forEach" =>
        {
            return Some(Cat::Collisions);
        }
        (
            "Shapes"
            | "VoxelShapes"
            | "VoxelShape"
            | "BlockCollisions"
            | "BlockCollisionSpliterator"
            | "CollisionUtil",
            _,
        ) => return Some(Cat::Collisions),
        (
            "Entity" | "LivingEntity" | "EntityLiving" | "Mob" | "EntityInsentient" | "MobEntity",
            m,
        ) if m == "collide"
            || m == "pushEntities"
            || m == "tickCramming"
            || m == "push"
            || m == "collideBoundingBox"
            || m == "adjustMovementForCollisions"
            || m == "checkInsideBlocks"
            || m == "checkBlockCollision"
            || m == "updateFluidHeightAndDoFluidPushing"
            || m == "updateMovementInFluid"
            || m == "isInWall"
            || m == "isFree" =>
        {
            return Some(Cat::Collisions);
        }
        (
            "Entity" | "LivingEntity" | "EntityLiving" | "Mob" | "EntityInsentient" | "MobEntity",
            m,
        ) if m == "move" || m == "travel" || m == "moveRelative" || m == "travelInAir" => {
            return Some(Cat::EntityMove);
        }
        ("Mob" | "EntityInsentient" | "MobEntity", m)
            if m == "serverAiStep" || m == "tickNewAi" || m == "doTick" || m == "inactiveTick" =>
        {
            return Some(Cat::MobAi);
        }
        (
            "Mob" | "EntityInsentient" | "MobEntity" | "LivingEntity" | "EntityLiving",
            "customServerAiStep" | "mobTick",
        ) => return Some(Cat::MobAi),
        ("ServerPlayer" | "EntityPlayer" | "ServerPlayerEntity", m)
            if m == "doTick" || m == "tick" || m == "playerTick" || m == "trackChunk" =>
        {
            return Some(Cat::Players);
        }
        (
            "PlayerList"
            | "PlayerManager"
            | "ServerPlayerGameMode"
            | "PlayerInteractManager"
            | "ServerPlayerInteractionManager",
            _,
        ) => return Some(Cat::Players),
        ("MinecraftServer" | "DedicatedServer" | "MinecraftDedicatedServer", m)
            if m.starts_with("save") || m == "autoSave" =>
        {
            return Some(Cat::ChunkSave);
        }
        (
            "MinecraftServer"
            | "DedicatedServer"
            | "BlockableEventLoop"
            | "ReentrantBlockableEventLoop"
            | "ThreadExecutor"
            | "IAsyncTaskHandler"
            | "IAsyncTaskHandlerReentrant"
            | "ReentrantThreadExecutor",
            m,
        ) if m == "pollTask"
            || m == "runAllTasks"
            || m == "doRunTask"
            || m == "runTask"
            || m == "executeTask"
            || m == "runTasks"
            || m == "executeQueuedTask"
            || m == "pollTaskInternal" =>
        {
            return Some(Cat::ServerTasks);
        }
        (
            "DedicatedServer" | "MinecraftDedicatedServer",
            "handleConsoleInputs" | "executeQueuedCommands",
        ) => return Some(Cat::Commands),
        ("MinecraftServer" | "DedicatedServer" | "MinecraftDedicatedServer", m)
            if m == "tickChildren"
                || m == "tickServer"
                || m == "tick"
                || m == "tickWorlds"
                || m == "tickConnection" =>
        {
            return Some(Cat::ServerTickOther);
        }
        _ => {}
    }

    if outer.ends_with("Explosion") {
        return Some(Cat::Explosions);
    }
    if class.starts_with("net.minecraft.world.entity.")
        && (method == "tick"
            || method == "aiStep"
            || method == "baseTick"
            || method == "rideTick"
            || method == "inactiveTick"
            || method == "tickMovement"
            || method == "postTick")
    {
        return Some(Cat::EntityTick);
    }
    None
}

fn native_category(method: &str) -> Option<Cat> {
    let m = method.to_ascii_lowercase();
    if m.contains("g1")
        || m.contains("gc")
        || m.contains("zgc")
        || m.contains("shenandoah")
        || m.contains("parallelscavenge")
        || m.contains("collectedheap")
        || m.contains("oopclosure")
        || m.contains("markbitmap")
    {
        return Some(Cat::Gc);
    }
    if m.contains("compile")
        || m.contains("c2compiler")
        || m.contains("c1_")
        || m.contains("phaseidealloop")
        || m.contains("phasechaitin")
        || m.contains("node::")
    {
        return Some(Cat::Jit);
    }
    Some(Cat::Native)
}

pub fn is_blocking_frame(class: &str, method: &str) -> bool {
    match class {
        "jdk.internal.misc.Unsafe" | "sun.misc.Unsafe" => method == "park",
        "java.util.concurrent.locks.LockSupport" => method.starts_with("park"),
        "java.lang.Object" => method == "wait" || method == "wait0",
        "java.lang.Thread" => {
            method == "sleep"
                || method == "sleep0"
                || method == "sleepNanos"
                || method == "sleepNanos0"
                || method == "yield"
                || method == "yield0"
                || method == "join"
        }
        "java.util.concurrent.CompletableFuture" => {
            method == "join" || method == "get" || method == "waitingGet" || method == "timedGet"
        }
        "java.util.concurrent.FutureTask" => method == "get" || method == "awaitDone",
        "java.util.concurrent.CountDownLatch"
        | "java.util.concurrent.Semaphore"
        | "java.util.concurrent.CyclicBarrier" => method == "await" || method == "acquire",
        "java.util.concurrent.locks.ReentrantLock"
        | "java.util.concurrent.locks.ReentrantReadWriteLock$ReadLock"
        | "java.util.concurrent.locks.ReentrantReadWriteLock$WriteLock" => {
            method == "lock" || method == "lockInterruptibly"
        }
        "java.util.concurrent.locks.AbstractQueuedSynchronizer" => {
            method.starts_with("acquire") || method == "await"
        }
        _ => false,
    }
}

pub fn is_generic_idle(node: &Node) -> bool {
    let class = node.class.as_str();
    let method = node.method.as_str();
    if is_blocking_frame(class, method) {
        return true;
    }
    match class {
        "sun.nio.ch.EPoll" | "sun.nio.ch.KQueue" => {
            method == "wait" || method == "poll" || method == "kevent0"
        }
        "sun.nio.ch.EPollArrayWrapper" => method == "epollWait",
        "sun.nio.ch.WindowsSelectorImpl$SubSelector" => method.starts_with("poll"),
        "sun.nio.ch.SelectorImpl"
        | "sun.nio.ch.EPollSelectorImpl"
        | "sun.nio.ch.KQueueSelectorImpl"
        | "sun.nio.ch.WindowsSelectorImpl" => {
            method == "doSelect" || method == "select" || method == "lockAndDoSelect"
        }
        "io.netty.channel.epoll.Native" => {
            method.starts_with("epollWait") || method.starts_with("epollBusyWait")
        }
        "io.netty.channel.kqueue.Native" | "io.netty.channel.kqueue.KQueueEventLoop" => {
            method.starts_with("keventWait") || method == "kqueueWait"
        }
        "io.netty.channel.nio.NioEventLoop" => method == "select",
        "sun.nio.ch.Net" => method == "poll" || method == "accept",
        "sun.nio.ch.SocketDispatcher"
        | "sun.nio.ch.UnixDispatcher"
        | "sun.nio.ch.FileDispatcherImpl" => method == "read0" && !node.class.contains("File"),
        "java.lang.ref.Reference" => method == "waitForReferencePendingList",
        "java.lang.ProcessImpl" | "java.lang.ProcessHandleImpl" => method.starts_with("waitFor"),
        "java.io.FileInputStream" | "java.io.FileInputStream$1" => {
            method == "readBytes" && node.parent.is_some()
        }
        "java.net.SocketInputStream"
        | "sun.nio.ch.NioSocketImpl"
        | "java.net.PlainSocketImpl"
        | "sun.nio.ch.ServerSocketChannelImpl" => {
            method == "socketRead0"
                || method == "accept0"
                || method == "park"
                || method == "accept"
                || method == "timedRead"
                || method == "socketAccept"
        }
        "jdk.internal.misc.VirtualThread" => method == "park",
        _ => false,
    }
}

pub fn is_server_idle_marker(node: &Node) -> bool {
    let simple = node.simple_class();
    let outer = simple.split('$').next().unwrap_or(simple);
    let method = node.method.as_str();
    matches!(
        outer,
        "MinecraftServer" | "DedicatedServer" | "MinecraftDedicatedServer" | "IntegratedServer"
    ) && matches!(
        method,
        "waitUntilNextTick"
            | "sleepForTick"
            | "runTasksTillTickEnd"
            | "managedBlockUntilNextTick"
            | "waitUntilNextTickWithSleep"
    ) || matches!(
        outer,
        "SchedulerThreadPool" | "TickRegionScheduler" | "TickThread"
    )
}

pub fn is_wait_helper(node: &Node) -> bool {
    let simple = node.simple_class();
    let outer = simple.split('$').next().unwrap_or(simple);
    let method = node.method.as_str();
    matches!(
        outer,
        "MinecraftServer"
            | "DedicatedServer"
            | "BlockableEventLoop"
            | "ReentrantBlockableEventLoop"
            | "ThreadExecutor"
            | "ReentrantThreadExecutor"
            | "IAsyncTaskHandler"
            | "IAsyncTaskHandlerReentrant"
            | "SchedulerThreadPool"
            | "TickRegionScheduler"
            | "PrioritisedThreadPool"
            | "TickThread"
    ) && matches!(
        method,
        "managedBlock"
            | "waitForTasks"
            | "waitUntilNextTick"
            | "sleepForTick"
            | "runTasksTillTickEnd"
            | "runAllTasks"
            | "pollTask"
            | "waitForTasksAndTick"
            | "run"
            | "lambda$run$0"
            | "sleep"
            | "park"
            | "parkNanos"
            | "awaitTasks"
    ) && method != "pollTask"
        && method != "runAllTasks"
}

pub fn is_server_loop(node: &Node) -> bool {
    let simple = node.simple_class();
    matches!(simple, "MinecraftServer" | "DedicatedServer")
        && matches!(
            node.method.as_str(),
            "runServer" | "run" | "lambda$spin$0" | "lambda$spin$2" | "spin"
        )
}

pub fn entity_type(node: &Node) -> Option<String> {
    let class = node.class.as_str();
    if !(class.starts_with("net.minecraft.world.entity.")
        || class.starts_with("net.minecraft.entity.")
        || class.starts_with("net.minecraft.server.level.ServerPlayer"))
    {
        return None;
    }
    if class.starts_with("net.minecraft.world.entity.ai.")
        || class.starts_with("net.minecraft.entity.ai.")
    {
        return None;
    }
    let method = node.method.as_str();
    if !matches!(
        method,
        "tick"
            | "aiStep"
            | "baseTick"
            | "rideTick"
            | "inactiveTick"
            | "tickMovement"
            | "serverAiStep"
            | "customServerAiStep"
            | "mobTick"
            | "doTick"
    ) {
        return None;
    }
    let simple = simple_class(class);
    let simple = simple.split('$').next().unwrap_or(simple);
    if matches!(
        simple,
        "Entity"
            | "LivingEntity"
            | "Mob"
            | "PathfinderMob"
            | "AgeableMob"
            | "Animal"
            | "Monster"
            | "AbstractGolem"
            | "WaterAnimal"
            | "TamableAnimal"
            | "AbstractFish"
            | "MobEntity"
            | "PathAwareEntity"
            | "PassiveEntity"
            | "AnimalEntity"
            | "HostileEntity"
            | "AbstractHorse"
            | "AbstractIllager"
            | "Raider"
            | "PatrollingMonster"
            | "AbstractSkeleton"
            | "AbstractMinecart"
            | "AbstractMinecartContainer"
            | "Projectile"
            | "AbstractArrow"
            | "ThrowableProjectile"
            | "ThrowableItemProjectile"
            | "AgeableWaterCreature"
            | "WaterCreatureEntity"
            | "FlyingMob"
            | "AbstractSchoolingFish"
            | "ShoulderRidingEntity"
            | "SpellcasterIllager"
            | "AbstractPiglin"
            | "VehicleEntity"
            | "AbstractBoat"
            | "Player"
            | "PlayerEntity"
    ) {
        return None;
    }
    Some(simple.to_string())
}

pub fn block_entity_type(node: &Node) -> Option<String> {
    let class = node.class.as_str();
    if !(class.starts_with("net.minecraft.world.level.block.entity.")
        || class.starts_with("net.minecraft.block.entity."))
    {
        return None;
    }
    let simple = simple_class(class);
    let simple = simple.split('$').next().unwrap_or(simple);
    if matches!(
        simple,
        "BlockEntity"
            | "BaseContainerBlockEntity"
            | "RandomizableContainerBlockEntity"
            | "TickingBlockEntity"
            | "BlockEntityTicker"
            | "BlockEntityType"
            | "LockableContainerBlockEntity"
            | "LootableContainerBlockEntity"
    ) {
        return None;
    }
    Some(simple.to_string())
}

pub fn package_bucket(class: &str) -> &'static str {
    if class.is_empty() {
        return "Nativo (JVM/OS)";
    }
    if class.starts_with("net.minecraft.") || class.starts_with("com.mojang.") {
        "Minecraft"
    } else if starts_any(
        class,
        &[
            "org.bukkit.",
            "org.spigotmc.",
            "io.papermc.",
            "com.destroystokyo.paper.",
            "ca.spottedleaf.",
            "net.md_5.bungee.",
            "gg.pufferfish.",
            "org.purpurmc.",
            "co.aikar.timings.",
            "net.minecraftforge.",
            "net.neoforged.",
            "net.fabricmc.",
            "org.spongepowered.",
            "io.canvasmc.",
            "dev.folia.",
        ],
    ) {
        "Server (Paper/Spigot/loader)"
    } else if starts_any(class, &["java.", "javax.", "jdk.", "sun.", "com.sun."]) {
        "JDK"
    } else if class.starts_with("io.netty.") {
        "Netty"
    } else if starts_any(
        class,
        &[
            "it.unimi.dsi.fastutil.",
            "com.google.",
            "org.apache.",
            "kotlin.",
            "kotlinx.",
            "net.kyori.",
            "org.objectweb.asm.",
            "org.slf4j.",
            "org.yaml.",
            "com.zaxxer.",
            "org.jetbrains.",
            "me.lucko.spark.",
        ],
    ) {
        "Librerie"
    } else {
        "Sconosciuto"
    }
}
