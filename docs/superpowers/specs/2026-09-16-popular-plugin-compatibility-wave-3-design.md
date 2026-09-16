# Popular Plugin Compatibility — Wave 3 Design

## Goal

Eliminate the 41 Zelda Civ compiler errors measured after Wave 2 and advance the same plugin through successive runtime blockers with real, state-backed Bukkit/Paper behavior. This wave does not add declaration-only compatibility, fake Vanilla data, generated-source edits, or idle/game-tick overhead.

## Measured baseline

- Foton base: `5fa4936d6bc58c114d8bcee0e6c5d6520a8cf55a`.
- Zelda base: `3d7dc062b4353bced69f8383f52c961a5d42e17f`.
- Gate: Zelda's `tools/check-foton-compat.sh` with an 86-entry Maven classpath containing zero `paper-api` jars.
- Result: 41 compiler errors, 351 fewer than the valid 392-error baseline (`-89.54%`).
- Runtime: SQLite acquisition succeeds, both databases pass `PRAGMA integrity_check`, and `CreatureSpawnEvent.SpawnReason.BEEHIVE` succeeds. The first remaining Foton-owned failure is `NoClassDefFoundError: org/bukkit/event/inventory/BrewEvent` during reflective listener registration.

The 41 residual diagnostics are the complete scope oracle for this wave:

| Family | Errors | Symbols |
|---|---:|---|
| Ray tracing | 7 | `FluidCollisionMode` ×3, `RayTraceResult` ×4 |
| Entities and displays | 11 | `ItemDisplay` ×8, `Allay`, `TextDisplay`, `ChestBoat` |
| Events and lifecycle | 19 | `EntitiesLoadEvent` ×1, `EntitiesUnloadEvent` ×2, `EntityDismountEvent` ×2, `EntityPlaceEvent` ×2, `BlockDropItemEvent` ×2, `PlayerArmorChangeEvent` ×2, `PrepareSmithingEvent` ×2, `SmithItemEvent` ×2, `BrewEvent` ×2, `PrepareItemEnchantEvent` ×1, its package import ×1 |
| Inventory, metadata, data components | 3 | `BannerPatternLayers`, `MusicInstrument`, `MusicInstrumentMeta` |
| Block data | 1 | `Ageable` |

Javac can hide later missing members behind a missing type. Therefore each task must rerun the exact gate and classify newly exposed diagnostics; it must not add speculative API merely because a neighboring Paper type exists.

## Architecture and ordering

### 1. Correct entity identity before adding entity APIs

`FotonEntity.handle` currently resolves a UUID to an entity type and then passes that type string into `FotonWorld.wrapEntity`, which performs a second UUID lookup. `FotonWorld.spawn(Class<T>)` also derives registry keys by lowercasing class names, which cannot produce keys such as `item_display`.

Make wrapper construction explicit and single-pass:

```java
static Entity wrapEntity(UUID uuid, String type)
```

All callers resolve the type once. Class-based spawning uses generated class-to-`EntityType` metadata from `dev/gen-entity-type.py`; it never guesses a registry key. This foundation is required by ray predicates, display wrappers, Allay identity, chest-boat holders, lifecycle event entity lists, and placed-entity events.

### 2. Unblock the listener class with two complete inventory events

Zelda's `VanillaSuppressor` declares both `BrewEvent` and `PrepareItemEnchantEvent`. Adding only `BrewEvent` would move reflective registration to the next missing parameter type, so both belong in one independently reviewable checkpoint.

`BrewEvent` fires only when a brew completes. Foton calculates three candidate bottle results from a five-slot snapshot, releases the brewing-container lock before JNI, exposes a snapshot-backed `BrewerInventory`, and dispatches the event. Cancellation preserves all five pre-brew slots, consumes no ingredient, creates no remainder, and emits no brew sound. Success commits listener-mutated bottle results, treats omitted trailing results as empty, applies snapshot inventory mutations, then consumes the ingredient and handles its remainder. Fuel is the value remaining at completion. Cancellation leaves `brew_time` at zero; an otherwise brewable stand starts a normal new cycle on the next tick.

`PrepareItemEnchantEvent` fires after Vanilla has computed the three offers and before they are installed in the menu. The Java offers are mutable and snapshot-backed. Nonempty non-enchantable items fire a pre-cancelled event. Final cancellation clears all offers; otherwise the listener-mutated offers become the live menu state. No enchantment RNG or cost is recreated in Java.

### 3. Delegate ray tracing to existing spatial foundations

`FluidCollisionMode` maps exactly to Foton's existing `ClipFluid`:

| Bukkit | Foton |
|---|---|
| `NEVER` | `ClipFluid::None` |
| `SOURCE_ONLY` | `ClipFluid::SourceOnly` |
| `ALWAYS` | `ClipFluid::Any` |

For block rays, `ignorePassableBlocks=true` uses `ClipBlockShape::Collider`; `false` uses `ClipBlockShape::Outline`. One request-time native delegates to `World::clip` and returns the exact hit position, block coordinates, face, and miss state.

Entity rays use the existing loaded-entity AABB query, clip each live bounding box against the normalized segment, apply the Java predicate, and select the strict nearest hit. They do not reuse projectile collision, because Bukkit's entity-only trace is not shortened by an intervening block.

`RayTraceResult` implements the complete small Paper 26.2 value contract required for truthful ABI: five public constructors, four getters, defensive copies of hit vectors, `equals`, `hashCode`, and `toString`.

General block-ray behavior across unloaded chunks remains explicitly bounded: Foton currently reads unloaded chunks as air rather than loading them. Zelda's measured 8–12 block player-local rays are valid against loaded chunks; this wave does not claim Paper's possible chunk-loading side effect.

### 4. Expose live entity/display behavior and holder identity

Add exact wrapper identity for `ItemDisplay`, `TextDisplay`, `Allay`, and every chest-boat registry type. Shared Display getters/setters delegate the existing synchronized entity-data fields. Item display stack/transform and text display content/style delegate existing core fields; Adventure text crosses as JSON so color and bold styling are preserved. Display mutations remain request-driven and existing dirty metadata coalescing sends the packet.

Display state that the API exposes must survive the same save/load lifecycle as the entity before the implementation claims persistent Bukkit state. If a shared Display field has no persistence path, add its normal entity serialization in the owning core module; do not retain a Java-only cache. Server-side interpolation that Paper exposes only as a client presentation behavior is not added unless a Zelda-observable contract requires it.

Chest-boat `InventoryHolder` identity travels with menu/open-event state as the entity UUID. `Inventory#getHolder()` returns a wrapper for that exact UUID, not the viewing player. The existing 27-slot container and persisted loot table/seed remain authoritative.

The broader `LootableEntityInventory` refill/player-history surface is not implemented in this wave. Foton has no refill-enabled flag, fill timestamps, next-refill timestamp, or per-player loot history. Consequently `ChestBoat` can truthfully implement the interfaces and methods backed by entity/inventory state, but it must not declare unsupported refill/history methods with invented return values. A plugin compiled against one of those omitted methods remains ABI-incompatible until that state model exists.

### 5. Use existing generated data for components, instruments, and age

`BannerPatternLayers` is backed by `foton-registry`'s existing `BannerPatternLayers`, keyed banner-pattern registry, and item-component codecs. Java builders and value objects preserve order and value equality. `DataComponentTypes.BANNER_PATTERNS` and `BASE_COLOR`, `Registry.BANNER_PATTERN`, and slot encode/decode all route the same native component state; nothing is stored only in Java.

`MusicInstrument` exposes existing Vanilla instrument registry entries, including `DREAM_GOAT_HORN`, and `MusicInstrumentMeta` round-trips the existing `INSTRUMENT` item component through native item serialization so the client keeps the horn's use behavior. `MusicInstrument.create(...)` is explicitly deferred: Foton lacks plugin-extensible dynamic registry construction. The method is omitted rather than implemented as a no-op, so plugins that require dynamic instrument registration remain ABI-incompatible instead of appearing to succeed.

`Ageable` block data is generated from existing extracted block/property metadata. `SimpleAgeableData` reads and writes the `age` property and validates each material's actual maximum; it must distinguish, for example, beetroot age 3 from wheat/carrot/potato age 7. No global maximum or handwritten material table is allowed.

### 6. Emit entity lifecycle events at the exact state boundary

- `EntitiesLoadEvent`: fire once after successfully restored entities are inserted and UUID-resolvable; aggregate persisted/restored trees without duplicates; expose an immutable entity list.
- `EntitiesUnloadEvent`: fire after the exact retained/unloaded set is selected but before inactive callbacks and final save. Event entities remain UUID-resolvable during dispatch.
- `EntityDismountEvent`: fire before passenger/vehicle unlink. Ordinary dismount is cancellable; forced teleport/removal sets `isCancellable=false`. Packet updates occur only after successful unlink.
- `EntityPlaceEvent`: fire after construction and collision validation but before insertion, game event, and item consumption. The pending-spawn scope makes the entity resolvable during dispatch. Cancellation suppresses every post-event effect. Hook boat, chest boat, minecart, armor stand, and end crystal placement paths.

These events execute only at existing load, unload, relationship-change, or item-placement boundaries.

### 7. Complete mutation-sensitive block, armor, and smithing events

- `BlockDropItemEvent`: preserve the original `BlockState`; compute Fortune/Silk loot first; expose a mutable list of pending item entities before insertion. Cancellation suppresses all listed drops. Respect Java `BlockBreakEvent#setDropItems(false)` and fire the drop event with an empty list when Paper does.
- `PlayerArmorChangeEvent`: piggyback the existing equipment-diff pass and emit one noncancellable event for each changed `HEAD`, `CHEST`, `LEGS`, or `FEET` slot with old/new snapshots. Do not add another equipment scan.
- `PrepareSmithingEvent`: compute the Vanilla result first, dispatch on every input recomputation through a snapshot-backed `SmithingInventory`, and install the possibly null/replaced result before menu synchronization.
- `SmithItemEvent`: dispatch the `InventoryClickEvent` subclass for a nonempty smithing result slot before input/result consumption. Cancellation prevents the click and all consumption.

### 8. Evidence closes the wave

The exact Zelda compile gate must reach zero errors without Paper jars. The isolated runtime fixture then advances one blocker at a time; a clean Foton process shutdown is not counted as Zelda enablement when plugin activation failed. After runtime requalification, run `bash dev/ci.sh` on the exact candidate and obtain a whole-branch specification and quality review.

## Scope boundaries

- No NMS/CraftBukkit classes, custom Paper loader, Maven resolver, or fake Simple Voice Chat API.
- No declaration-only event, entity, metadata, component, or registry type.
- No runtime parsing of Vanilla registry JSON; generators consume existing extracted assets and commit generators/source, never generated `src/generated/` output.
- No direct edits to extracted JSON. Missing data is an extractor blocker, not a license to transcribe values.
- No Java-only shadow state for native entities, inventories, item components, or block properties.
- No broad claim of Paper compatibility from Zelda alone. The result is exactly: zero measured Zelda compile residuals plus the runtime paths actually exercised.
- Deferred `ChestBoat` refill/player-history methods and `MusicInstrument.create(...)` remain honest ABI gaps. Their containing class/interface may exist for backed members, but omitted methods still cause linkage failure for plugins that call them.

## Performance invariant

An idle server and a server with no JVM plugin component pay no new work. The wave adds no polling, recurring scan, extra global lock, sleep, async wait, or unconditional JNI call to a game tick. Work is charged only to the operation that requested it: one ray request, one brew completion, one menu recomputation/click, one entity load/unload/dismount/place, one block break, one already-detected armor diff, one display mutation, or one item codec operation. Existing dirty metadata and equipment-diff passes are reused.

## Verification contract

1. Every task begins from its committed predecessor and records an observed RED before production edits.
2. Generated Java/API checks verify exact signatures and value behavior; Rust tests verify timing, cancellation, mutation, persistence, and live native state.
3. Real JVM/JNI tests that depend on the built plugin API run in isolated processes after `bash dev/build-plugin-api.sh --check`; they are not ordinary parallel workspace tests.
4. `python3 dev/check-natives.py --quiet`, focused crate checks, formatting, and `git diff --check` pass at each checkpoint.
5. Zelda is recompiled after each residual family. The final compile count is zero, and every runtime blocker is recorded with the exact Foton/Zelda SHA and first failing class/method.
6. The final exact commit passes `bash dev/ci.sh`, leaves no generated/build artifact tracked or untracked, and receives whole-branch approval.
