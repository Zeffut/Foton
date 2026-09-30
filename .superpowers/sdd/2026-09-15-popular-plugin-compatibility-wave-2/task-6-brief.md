### Task 6: Add additional API already backed by live foundations

**Files:**
- Modify: `plugin-api/src/org/bukkit/entity/Player.java`
- Modify: `plugin-api/src/org/bukkit/OfflinePlayer.java`
- Modify/add: `plugin-api/src/org/bukkit/entity/AnimalTamer.java`
- Modify: `plugin-api/src/org/bukkit/GameRule.java`
- Modify: `plugin-api/src/org/bukkit/Tag.java`
- Modify: `plugin-api/src/org/bukkit/enchantments/Enchantment.java`
- Modify: `plugin-api/src/org/bukkit/event/entity/CreatureSpawnEvent.java`
- Modify: `plugin-api/src/foton/FotonEntity.java`
- Modify: `plugin-api/src/foton/Native.java`
- Modify: `plugin-api/src/foton/EventBridge.java`
- Modify: `foton-core/src/entity/spawn.rs`
- Modify: `foton-core/src/entity/base/mod.rs`
- Modify: `foton-core/src/entity/mob/mod.rs`
- Modify: `foton-core/src/event/entity.rs`
- Modify: `foton-core/src/world/natural_spawn.rs`
- Modify: `foton-core/src/world/base_spawner.rs`
- Modify: `foton-core/src/block_entity/entities/beehive.rs`
- Modify: `foton-plugin/src/forward.rs`
- Modify: `foton-plugin/src/natives.rs`
- Modify focused Java/Rust checks

**Interfaces:**
- Preserve: `EntityBase::spawn_reason() -> Option<EntitySpawnReason>` remains the Vanilla-facing accessor.
- Add: `PluginSpawnReason`, the typed 47-value Paper surface; a single provenance snapshot under the existing spawn-reason lock stores `Option<EntitySpawnReason>` and `Option<PluginSpawnReason>`.
- Add: typed plugin-reason read/write accessors used by event dispatch and `Entity.getEntitySpawnReason()`; no string is stored in core state.
- Produce: beehive release emits one cancellable `CreatureSpawnEvent(BEEHIVE)` before insertion and retains the occupant on cancellation.

**Steps:**
1. Add failing checks for the original Task 6 scope: the Bukkit player hierarchy, four legacy game-rule handles, live trimmable-armor tag lookup, and bidirectional enchantment conflicts. Keep the expected Zelda compile reduction at 11 errors (about 330 to about 319); do not count the later `BEEHIVE` runtime linkage failure as a compile reduction.
2. Add a failing ABI check that compares `CreatureSpawnEvent.SpawnReason.values()` against Paper 26.2's exact 47 names and source order: `NATURAL`, `JOCKEY`, `CHUNK_GEN`, `SPAWNER`, `TRIAL_SPAWNER`, `EGG`, `SPAWNER_EGG`, `LIGHTNING`, `BUILD_SNOWMAN`, `BUILD_IRONGOLEM`, `BUILD_COPPERGOLEM`, `BUILD_WITHER`, `VILLAGE_DEFENSE`, `VILLAGE_INVASION`, `BREEDING`, `SLIME_SPLIT`, `REINFORCEMENTS`, `NETHER_PORTAL`, `DISPENSE_EGG`, `INFECTION`, `CURED`, `OCELOT_BABY`, `SILVERFISH_BLOCK`, `MOUNT`, `TRAP`, `ENDER_PEARL`, `SHOULDER_ENTITY`, `DROWNED`, `SHEARED`, `EXPLOSION`, `RAID`, `PATROL`, `BEEHIVE`, `PIGLIN_ZOMBIFIED`, `SPELL`, `FROZEN`, `METAMORPHOSIS`, `DUPLICATION`, `COMMAND`, `ENCHANTMENT`, `OMINOUS_ITEM_SPAWNER`, `BUCKET`, `POTION_EFFECT`, `REANIMATE`, `REHYDRATION`, `CUSTOM`, `DEFAULT`. Use the [official enum source](https://github.com/PaperMC/Paper/blob/ver/26.2/paper-api/src/main/java/org/bukkit/event/entity/CreatureSpawnEvent.java) as the oracle.
3. Add failing Rust provenance tests, then introduce `PluginSpawnReason` and change the existing spawn-reason lock payload to hold both Vanilla and plugin-visible values. Keep `EntitySpawnReason` and `spawn_reason()` intact. Map `Natural -> NATURAL`, `ChunkGeneration -> CHUNK_GEN`, `Spawner -> SPAWNER`, `Breeding -> BREEDING`, `Jockey -> JOCKEY`, `Reinforcement -> REINFORCEMENTS`, `Bucket -> BUCKET`, `SpawnItemUse -> SPAWNER_EGG`, `Command -> COMMAND`, `Dispenser -> DISPENSE_EGG`, `Patrol -> PATROL`, and `TrialSpawner -> TRIAL_SPAWNER`. Map `Structure`, `MobSummoned`, `Event`, `Conversion`, `Triggered`, `Load`, and `DimensionTravel` to `DEFAULT` unless their call site supplies a precise typed reason. Never derive `CUSTOM`; reserve it for an explicit plugin-originated spawn request.
4. Replace stringly core spawn-event reasons with `PluginSpawnReason` through `PreCreatureSpawnEvent`, `CreatureSpawnEvent`, and the forwarding bridge. Serialize only at the JNI boundary. Change Java's unknown-name handling from `CUSTOM` to `DEFAULT`, expose stored plugin provenance through `FotonEntity.getEntitySpawnReason()`, and prove an unknown or absent server reason never becomes `CUSTOM`.
5. Add failing beehive release tests before implementation. On a release attempt, set typed provenance to `BEEHIVE`, register the entity in the existing pending-spawn scope, and fire the cancellable event before `try_add_entity`. Match [Paper's release ordering](https://github.com/PaperMC/Paper/blob/ver/26.2/paper-server/patches/sources/net/minecraft/world/level/block/entity/BeehiveBlockEntity.java.patch): cancellation returns the occupant to the hive and produces no insertion, exit sound, game event, honey-level change, or released-bee side effect; success inserts once and both the event and later entity query report `BEEHIVE`.
6. Correct the `Player` / `OfflinePlayer` / `AnimalTamer` hierarchy without duplicating player state or weakening existing implementations.
7. Map `ANNOUNCE_ADVANCEMENTS`, `DO_INSOMNIA`, `DO_PATROL_SPAWNING`, and `DO_TRADER_SPAWNING` to the exact Minecraft 26.2 rule names already accepted by the live native registry, and add `ITEMS_TRIMMABLE_ARMOR` as a normal live item-tag handle.
8. Delegate `Enchantment.conflictsWith` to the existing registry exclusive-set logic through a request-time binding; do not copy or hardcode the set.
9. Run the focused Java ABI/behavior checks, focused core/plugin Rust tests, `bash dev/beehive-test.sh`, `cargo clippy -r -p foton-core -p foton-plugin --all-targets --all-features -- -D warnings`, and `dev/build-plugin-api.sh`. Confirm the provenance pair reuses the existing lock and no polling, per-tick scan, allocation, JNI call, or extra synchronization was added.
10. Re-run Zelda `ae06d49cb093a977ce2192bb31426a118d342c80` with the Foton-only classpath. Record separately that Task 5 already passed SQLite using `sqlite-jdbc-3.49.1.0.jar` SHA-256 `5c8609d2ca341deb8c6f71778974b5ba4995c7d32d7c7c89d9392a3e72c39291`, and verify Task 6 now passes the `SpawnReason.BEEHIVE` runtime gate.
11. Commit.
