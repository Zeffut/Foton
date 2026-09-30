# Popular Plugin Compatibility — Wave 2 Design

## Goal

Increase practical Paper/Bukkit plugin compatibility without signature-only stubs, fake Vanilla data, new tick-path work, or misleading compatibility claims. This wave is driven by two concrete gates: a fresh plugin-API build and the Zelda Civ plugin compiled and started against Foton.

## Baseline

- Foton branch baseline: `75f78b09351b3aa622dffd311831c7d61b566d38`.
- Shared-member compatibility: 2478/2487; the nine residuals are architectural boundaries and have no Zelda overlap.
- Zelda Civ baseline: 392 compiler errors at `de3d12ac00a04f077b68c7537b36c689ac3679a2`, measured with a gate that rejects every residual `paper-api-*.jar` from the Maven classpath. The former 293-error measurement was contaminated by Paper and is invalid.
- Zelda at `ae06d49cb093a977ce2192bb31426a118d342c80` makes Simple Voice Chat genuinely optional and reaches `onEnable`.
- Task 5's host-runtime changes (`d7fdc5609`, followed by lifecycle fix `abb708f06`) let that Zelda build pass SQLite acquisition. The verified host artifact is `sqlite-jdbc-3.49.1.0.jar`, SHA-256 `5c8609d2ca341deb8c6f71778974b5ba4995c7d32d7c7c89d9392a3e72c39291`. The next Foton-owned runtime linkage gate is `CreatureSpawnEvent.SpawnReason.BEEHIVE`, which is absent from Foton's truncated enum.

## Scope

### 1. Reproducible plugin-API build

`dev/build-plugin-api.sh` must work from a fresh checkout where ignored generated Rust registry sources do not yet exist. It may generate the required registry sources through the existing `foton-registry` build before running the Java generators. The Java source argfile must remain valid when the repository path contains spaces.

`update-minecraft-src.sh` must derive Minecraft 26.2 from the workspace version, pin the GitCraft revision used by the repository, use a temporary directory outside the checkout, and bound generation to the exact target version. This wave updates and tests the command construction but does not run the destructive source regeneration.

### 2. Existing entity truth exposed through Bukkit

Expose only state and mutations already implemented by Foton core:

- scoreboard tags;
- width and height derived from the existing bounding box;
- destination-filling `getLocation(Location)`;
- gravity state and mutation;
- silent state mutation;
- rotation mutation;
- rain-state query.

JNI work is request-driven. It must not add polling, caches, locks, or work to the game tick. Existing core validation and Vanilla semantics remain authoritative.

### 3. Exact API adapters

Add adapters whose complete behavior is already expressible through existing live APIs:

- equipment slot dispatch;
- `Plugin.reloadConfig()` and `Plugin.saveConfig()` contract exposure;
- world entity filtering by class;
- registry iteration from the existing stream;
- repeated shapeless ingredients;
- the precise `ResolvableProfile.Builder.build()` return type;
- Adventure `Key` implementation on `NamespacedKey`.

Adapters must delegate rather than duplicate state.

### 4. Backed modern aliases

Add modern attribute and potion names only as references to the existing keyed instances:

- attributes: `ARMOR`, `ARMOR_TOUGHNESS`, `KNOCKBACK_RESISTANCE`, `MOVEMENT_SPEED`, `SCALE`;
- potion effects: `HASTE`, `JUMP_BOOST`, `RESISTANCE`, `STRENGTH`.

Identity tests must prove aliases are the same objects/enum constants. No separate unbacked registry identities are permitted.

### 5. Paper-compatible JDBC runtime

Paper places Xerial SQLite JDBC on the server classpath. Foton must match that practical contract with a pinned, checksum-verified runtime library and initialize host-visible `java.sql.Driver` providers before any plugin constructor or lifecycle callback.

Foton must not set the thread context classloader to a plugin loader for discovery: `DriverManager` provider initialization is process-global and plugin-owned registrations would retain plugin classloaders across disable/reload. A deterministic host-classpath fixture must prove provider discovery while preserving the original context classloader.

If a plugin disables itself during `onEnable`, the host must respect that state: do not emit `PluginEnableEvent`, transition it back to enabled, or log a successful enable.

### 6. Additional already-backed API and spawn provenance

Add only the following low-risk contracts confirmed against the clean Zelda gate and existing runtime foundations:

- correct `Player` / `OfflinePlayer` / `AnimalTamer` hierarchy;
- legacy Bukkit game-rule handles backed by the current 26.2 rule names;
- `Tag.ITEMS_TRIMMABLE_ARMOR` backed by Foton's live item-tag bridge;
- `Enchantment.conflictsWith` backed by the registry's real bidirectional exclusive-set logic.

Those original Task 6 items account for the expected 11-error compile-gate reduction, from about 330 to about 319. The newly observed `BEEHIVE` failure is a later runtime linkage/behavior gate, not another compile-error reduction, and must be reported separately.

Foton's `CreatureSpawnEvent.SpawnReason` must expose Paper 26.2's exact 47-value surface, in source order:

`NATURAL`, `JOCKEY`, `CHUNK_GEN`, `SPAWNER`, `TRIAL_SPAWNER`, `EGG`, `SPAWNER_EGG`, `LIGHTNING`, `BUILD_SNOWMAN`, `BUILD_IRONGOLEM`, `BUILD_COPPERGOLEM`, `BUILD_WITHER`, `VILLAGE_DEFENSE`, `VILLAGE_INVASION`, `BREEDING`, `SLIME_SPLIT`, `REINFORCEMENTS`, `NETHER_PORTAL`, `DISPENSE_EGG`, `INFECTION`, `CURED`, `OCELOT_BABY`, `SILVERFISH_BLOCK`, `MOUNT`, `TRAP`, `ENDER_PEARL`, `SHOULDER_ENTITY`, `DROWNED`, `SHEARED`, `EXPLOSION`, `RAID`, `PATROL`, `BEEHIVE`, `PIGLIN_ZOMBIFIED`, `SPELL`, `FROZEN`, `METAMORPHOSIS`, `DUPLICATION`, `COMMAND`, `ENCHANTMENT`, `OMINOUS_ITEM_SPAWNER`, `BUCKET`, `POTION_EFFECT`, `REANIMATE`, `REHYDRATION`, `CUSTOM`, `DEFAULT`.

The declarations are necessary for binary linkage but are not sufficient. Add a core-owned `PluginSpawnReason` with the same typed values and keep it alongside, not in place of, Vanilla `EntitySpawnReason`. Replace the current `SyncMutex<Option<EntitySpawnReason>>` payload with one provenance snapshot containing both optional reasons, so reads and writes reuse the existing lock rather than adding synchronization. Existing Vanilla consumers continue to read `EntitySpawnReason`; Bukkit/Paper events and `Entity.getEntitySpawnReason()` read only the plugin-visible value.

The generic Vanilla-to-plugin mapping is deliberately narrow and exhaustive:

| Vanilla `EntitySpawnReason` | Plugin-visible reason |
|---|---|
| `Natural` | `NATURAL` |
| `ChunkGeneration` | `CHUNK_GEN` |
| `Spawner` | `SPAWNER` |
| `Breeding` | `BREEDING` |
| `Jockey` | `JOCKEY` |
| `Reinforcement` | `REINFORCEMENTS` |
| `Bucket` | `BUCKET` |
| `SpawnItemUse` | `SPAWNER_EGG` |
| `Command` | `COMMAND` |
| `Dispenser` | `DISPENSE_EGG` |
| `Patrol` | `PATROL` |
| `TrialSpawner` | `TRIAL_SPAWNER` |
| `Structure`, `MobSummoned`, `Event`, `Conversion`, `Triggered`, `Load`, `DimensionTravel` | `DEFAULT` unless the originating call site supplies a more precise typed plugin reason |

`CUSTOM` is never a server-originated fallback. It may be stored only when a plugin-facing spawn API explicitly requests `CUSTOM`. Unknown or absent server provenance resolves to `DEFAULT`, and the Java bridge must not turn an unknown string into `CUSTOM`.

Beehive release is the first precise call-site override. Before attempting insertion, set plugin-visible provenance to `BEEHIVE`, expose the pending bee to the existing event bridge, and fire a real cancellable `CreatureSpawnEvent` with `BEEHIVE`. As in Paper, cancellation means the bee is not inserted, remains an occupant for a later release attempt, and causes no exit sound, game event, honey-level increase, or released-bee side effect. A successful release inserts once with `BEEHIVE`; both the event and subsequent `Entity.getEntitySpawnReason()` must observe that same typed value.

Authoritative Paper 26.2 references:

- [the exact `CreatureSpawnEvent.SpawnReason` enum](https://github.com/PaperMC/Paper/blob/ver/26.2/paper-api/src/main/java/org/bukkit/event/entity/CreatureSpawnEvent.java);
- [`Entity.getEntitySpawnReason()` API contract](https://github.com/PaperMC/Paper/blob/ver/26.2/paper-api/src/main/java/org/bukkit/entity/Entity.java);
- [Paper's entity-side spawn-reason storage](https://github.com/PaperMC/Paper/blob/ver/26.2/paper-server/patches/sources/net/minecraft/world/entity/Entity.java.patch) and [server insertion/event ordering](https://github.com/PaperMC/Paper/blob/ver/26.2/paper-server/patches/sources/net/minecraft/server/level/ServerLevel.java.patch);
- [Paper's beehive release, `BEEHIVE` event, and cancellation ordering](https://github.com/PaperMC/Paper/blob/ver/26.2/paper-server/patches/sources/net/minecraft/world/level/block/entity/BeehiveBlockEntity.java.patch).

## Explicitly deferred

- MavenLibraryResolver and custom Paper loader execution;
- PaperAdventure/NMS conversion types;
- `Effect.getId()` until authoritative mapping and functional world effects exist;
- particle declarations without packet routing;
- displays, merchants, pathfinding, ray tracing, teleports with flags, scoreboards, item data components, trims, rich Adventure presentation, event emission other than the verified beehive release path, and gameplay mechanics lacking verified foundations;
- any fake Simple Voice Chat classes.

## Verification

1. A copied checkout whose path contains spaces builds the plugin API without pre-existing ignored generated registry Rust.
2. Focused Java checks and Rust native tests cover the new bridge and adapter behavior.
3. Every planned Zelda diagnostic symbol disappears. The clean baseline should fall from 392 to about 330 after entity/adapters/aliases and about 319 after Task 6's original additional backed API, an 11-error reduction subject to javac cascade effects. The `BEEHIVE` runtime gate is tracked separately and does not inflate that compile metric.
4. A host-visible, service-discovered JDBC fixture connects during plugin enable without changing the thread context classloader or retaining a plugin-owned driver. A self-disabling plugin stays disabled and emits no enable event.
5. An ABI check compares `SpawnReason.values()` name-for-name and order-for-order with all 47 Paper 26.2 values. Focused Rust tests cover every generic mapping, prove `ChunkGeneration -> CHUNK_GEN` and `TrialSpawner -> TRIAL_SPAWNER`, and prove no server-originated path falls back to `CUSTOM`.
6. A beehive release test observes `BEEHIVE` during the event and through `Entity.getEntitySpawnReason()`. Its cancellation branch keeps the occupant and suppresses insertion, exit sound/game event, honey mutation, and released-bee effects; its success branch inserts exactly once.
7. Runtime Zelda validation uses the version where Simple Voice Chat is truly optional, progresses beyond the verified SQLite connection, and no longer stops at `SpawnReason.BEEHIVE`; the next gate, if any, is recorded separately.
8. `bash dev/ci.sh` passes on the exact final commit.

## Performance invariant

All new compatibility work runs only when a plugin invokes the API, during build/plugin loading, or at an entity's existing spawn/release boundary. The provenance pair reuses the existing lock. No new per-tick scans, allocations, JNI calls, synchronization, or steady tick work are introduced; beehive event work occurs only when an occupant already attempts release.
