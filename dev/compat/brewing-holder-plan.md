# Zelda compatibility: real brewing holder before Java BrewEvent

Prepared 2026-10-03. Design only; no production edits or builds performed. Preserve the stabilized wave and a60cd6692. The objective is truthful Paper 1.21.11 brewing-holder behavior on Foton's Minecraft 26.2 mechanics, followed by the existing native BrewEvent bridge and unchanged Zelda rerun.

## Evidence and newly identified prerequisites

- `foton-core/src/block_entity/entities/brewing_stand.rs` owns five items and authoritative brew time/fuel under one container lock. `BrewingStandDataSlots` is republished at the END of a tick. A completion callback therefore MUST capture the authoritative container, not read the menu atomics: the latter can still report the previous tick's time/fuel. Core already releases the inventory lock before invoking BrewEvent.
- `plugin-api/src/foton/FotonTileState.java` currently supplies no native snapshot. `FotonBlockState` owns only a fresh Java PDC, updates block data without tile payload, and rejects any full block-state-string difference. `BlockState.isPlaced()` defaults to true. These inherited behaviors cannot implement the new holder truthfully.
- Existing hopper slot JNI methods can access brewing containers. Reuse their item wire codec and generic container resolver, but do not reuse `FotonHopper`'s immediate native name setter for a captured state.
- `foton-registry/src/item_predicate.rs` owns typed `LockCode`. Full registered predicate matching currently lives privately inside command parsing. Its registered entry point accepts only `ItemStackTemplate`; the lower matcher already has a shared item/template view. Generalize the existing evaluator rather than write a custom-name-only lock check.
- `org.bukkit.Nameable` is absent. Add its real Adventure and legacy methods, not just a brewing-specific name getter.

Official references verified for the holder hierarchy and inventory semantics:

- [Container](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-api/src/main/java/org/bukkit/block/Container.java)
- [LockableTileState](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-api/src/main/java/io/papermc/paper/block/LockableTileState.java)
- [TileStateInventoryHolder](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-api/src/main/java/io/papermc/paper/block/TileStateInventoryHolder.java)
- [TileState](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-api/src/main/java/org/bukkit/block/TileState.java)
- [BrewerInventory](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-api/src/main/java/org/bukkit/inventory/BrewerInventory.java)
- [BrewingStand](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-api/src/main/java/org/bukkit/block/BrewingStand.java)

`BrewingStand` also requires `getRecipeBrewTime` and positive-only `setRecipeBrewTime`; this is a newly identified Paper extension, not an optional constant getter. `TileState.isSnapshot` must reflect actual snapshot/live construction. Inventory holder covariance must survive compilation against the official API.

## Dependency sequence and ownership

### 1. Reusable registered item predicates and native container lock

Core owner: new focused `foton-core/src/item_predicate/` module, command matcher integration, brewing block entity and brewing block behavior. Avoid moving command parsing wholesale: move the registered matcher and its transitive semantic helpers, retaining parser-specific responsibilities in `command/execution/item_predicate.rs`. Use one matching implementation for templates and live stacks; expose a minimal typed entry point such as `matches(&RegisteredItemPredicate, &ItemStack)` backed by the generic internal view. Keep recursive collection/partial-predicate evaluation intact.

Add a focused block-entity lock helper if the implementation needs shared serialization/access semantics; keep the Rust inventory mutex abstraction named `inventory::lock` distinct from gameplay key locks. Use `LockCode` and its existing codec for lowercase `lock` persistence and `LOCK` implicit components. Follow the current component lifecycle, including collection and removal from block-entity tags, and the real container/name semantics; do not continue comments claiming no lock exists.

Local authority: `minecraft-src/minecraft/src/net/minecraft/world/LockCode.java` and `world/level/block/entity/BaseContainerBlockEntity.java`. Main-hand predicate or spectator bypass permits opening. Rejection sends the translated overlay using the container display name and emits the locked-chest sound at the block center in BLOCKS, with local-source parameters. Use generated translation/sound references. Coordinate interaction code with `InventoryAccess` so it does not lock a player inventory already held by the caller.

Meaningful tests: exact components, nested partial/collection predicates and count/tag constraints on actual stacks; command matching unchanged; matching main hand succeeds, offhand alone fails, spectator succeeds; lock save/load and implicit-component round trip; real rejected use leaves menu closed and emits the correct notification. Preserve no-lock behavior for empty main hand.

### 2. Native brewing snapshot and commit foundation

Core owner: `foton-core/src/block_entity/entities/brewing_stand.rs`, focused sibling snapshot tests; common block-entity persistence in `foton-core/src/block_entity/mod.rs` only where needed for tile PDC.

Plugin native owner: a focused new `foton-plugin/src/block_state.rs` (recommended) plus registration/wrappers in `natives.rs` and module declaration in `lib.rs`. Keep new snapshot serialization and validation out of the already large natives file.

Define a typed snapshot containing five fully encoded items, authoritative brewing time/fuel, custom component name, full typed lock predicate, and tile persistent data. Keep recipe duration as a distinct transient field with the Paper snapshot/update semantics established below; it is not serialized with the other fields. Capture authoritative state under short-lived locks. Implement commit with target type/placement validation, force and physics semantics, application of the captured persistent fields, dirty marking and menu synchronization. Validate wire data before any world mutation. Do not implement commit as unrelated setters that can partially succeed.

Tile PDC must survive save/reload and remain isolated in a captured Java state until update. Store plugin-owned NBT under `PublicBukkitValues` with the normal block-entity persistence path, preserving types and other existing components. Reuse existing PDC/item NBT conversion where appropriate. Do not store this only in Java or invent an item component to hide it.

Tests: snapshot remains unchanged after native tick/live inventory changes; snapshot inventory/name/lock/timing/fuel/PDC setters do not mutate world before update; successful update persists and reloads; rejected wrong-type update mutates nothing; same-type bottle-flag changes do not incorrectly reject update; explicit force/physics behavior; fresh event-time snapshot observes brew time zero and current native fuel despite stale menu atomics.

### 3. Complete brewing holder and Java ABI

Java holder owner: `plugin-api/src/org/bukkit/block/{Container,Lockable,BrewingStand,TileState,BlockState}.java`, `org/bukkit/Nameable.java`, `io/papermc/paper/block/{LockableTileState,TileStateInventoryHolder}.java`, `org/bukkit/inventory/BrewerInventory.java`, and focused implementations `foton/FotonBrewingStand.java`, `FotonBrewerInventory.java`, snapshot inventory implementation. Integrate factory dispatch in `FotonBlock.java` and native declarations in `Native.java` only after native methods exist.

The native boundary should have capture and apply operations, plus existing live slot operations. Java state owns a deep independent captured payload; snapshot inventory and its holder refer to that payload. Placed getInventory uses the live container; detached getInventory uses its snapshot. Return real holder positions/type and brewing inventory type. Ensure `getHolder` returns a fresh authoritative holder snapshot where required, not a forever-cached state captured before brewing.

Implement all lock/name methods backed by native typed predicates/component codecs. `setLockItem` must preserve the exact predicate Paper constructs, including explicit item modifications; do not compare flattened display strings. `getLock` extracts the supported legacy name projection without discarding a complex predicate. Preserve the null versus empty-string distinction identified below; do not normalize it from the JavaDoc alone.

Use [CraftContainer](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-server/src/main/java/org/bukkit/craftbukkit/block/CraftContainer.java) for native predicate construction/name behavior. Its setters operate on captured data; `setLockItem` delegates to `CraftItemStack.asCriterionConditionItem`, inspected below. Keep null/empty-string legacy cases in the focused acceptance fixture.

[CraftBrewingStand](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-server/src/main/java/org/bukkit/craftbukkit/block/CraftBrewingStand.java) confirms snapshot time/fuel/name mutations, live placed inventory, detached inventory fallback and copy constructors. [CraftBlockEntityState](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-server/src/main/java/org/bukkit/craftbukkit/block/CraftBlockEntityState.java) confirms independent native snapshots, persisted PDC, tile application after block-state update, and the explicit snapshot-disabled mode.

Audit inherited BlockState/Inventory methods reachable from this holder against the official API before calling the holder complete. Implement required copy, placement, snapshot/live and update overload semantics rather than relying on false defaults. Do not extend this into a rewrite of every unrelated tile implementation; shared adjustments must preserve existing users and get focused regression checks.

### 4. BrewEvent Java shape and native forwarding

Bridge owner: `plugin-api/src/org/bukkit/event/inventory/BrewEvent.java`, `plugin-api/src/foton/EventBridge.java`, `foton-plugin/src/forward.rs`, and focused plugin bridge tests. Register the existing core event; do not replace or weaken the unlocked tick path.

Create the real block/holder/live BrewerInventory and mutable results list. Forward cancellation and all result items through the existing full item encoding, including PDC/custom components. Preserve existing core short-list semantics. JNI exceptions and malformed responses must follow established event-bridge failure handling, not silently fabricate successful results.

Test Java cancellation, mutable results replacement/clear, custom-data retention, holder state at completion, and JNI re-entry reads/writes without deadlock. Existing core event tests remain the regression base; do not duplicate constant assertions or claim Java completion from Rust-only tests.

### 5. Official fixture and unchanged Zelda acceptance

Compile `dev/fixtures/brew-event-plugin/BrewEventProbe.java` against the official Paper 1.21.11 API; run `dev/brew-event-plugin-test.sh` against the actual Foton build. Extend a focused holder fixture for snapshot/update/name/lock/PDC/detached behavior and save/reopen. Run the same meaningful edge cases on Paper where behavior is ambiguous. Require the existing enabled/cancellation/input-retention/modified-results markers and timeout protection for callback lock re-entry. Retain fixture output as evidence in the normal project workflow, not Desktop scratch files.

Then boot the exact unchanged Zelda artifact, record its hash and next real compatibility failure (or full success). Passing BrewEvent class loading alone does not prove the requested holder behavior.

## Source checks completed: implementation contracts

### Recipe duration is transient progress metadata, not a new recipe timer

The [Paper brewing BE patch](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-server/patches/sources/net/minecraft/world/level/block/entity/BrewingStandBlockEntity.java.patch), lines 6, 14, 25 and 106–119, initializes `recipeBrewTime` to 400 and exposes it as internal data index 2. Starting a cycle replaces it from `BrewingStartEvent.getRecipeBrewTime`; the separate remaining timer comes from `getBrewingTime`. Countdown still decrements the remaining timer. The patch adds no recipe-duration NBT load/save. Therefore do not add a persisted recipe-duration key or make the field directly change actual brewing speed.

The [menu patch](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-server/patches/sources/net/minecraft/world/inventory/BrewingStandMenu.java.patch), lines 19–47, retains TWO client data slots while using three internal values: displayed timer is `400 * remaining / recipeDuration`, fuel unchanged. In Foton, retain two protocol slots and use the extracted `potion_brewing::BREWING_TIME_TICKS` as the vanilla scale/default. Implement the transient denominator alongside the authoritative brewing fields and publish it for menu scaling; never send a fabricated third vanilla client slot. The live [CraftBrewingStandView](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-server/src/main/java/org/bukkit/craftbukkit/inventory/view/CraftBrewingStandView.java), lines 35–42, reads/writes internal index 2 and rejects nonpositive writes.

Important source-derived edge case: CraftBlockEntityState creates/copies snapshots and applies updates through NBT, while CraftBrewingStand has no duration-copy override. Thus an ordinary captured holder starts with the default recipe duration; its setter changes its own transient field, but ordinary `update` does not transfer that field into the live BE. Existing live duration remains unchanged during such an update; a newly created or reloaded BE resets it. `getState(false)` acts directly on the live field. These are deductions from the inspected implementations, not execution evidence; add differential tests for them before claiming Paper equivalence. Do not silently improve persistence or snapshot transfer.

The [BrewingStartEvent API](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-api/src/main/java/org/bukkit/event/block/BrewingStartEvent.java) confirms the independent positive recipe denominator and nonnegative remaining timer. This slice need not expose an unrelated new event: when no start hook exists, resetting the transient denominator to the extracted default preserves the unmodified cycle. Future start-event support feeds these same two fields. The Paper extension is bounded progress/API compatibility and leaves default vanilla mechanics unchanged.

### Lock item conversion and empty legacy strings

[CraftItemStack.asCriterionConditionItem](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-server/src/main/java/org/bukkit/craftbukkit/inventory/CraftItemStack.java), lines 143–147, constrains the native item holder, ignores stack count (`ANY`), and builds exact component expectations from the component PATCH applied to an EMPTY component map. It has no partial predicates. Therefore require explicit added/overridden components, not all default effective components; removed components disappear from that empty-map iteration and do not become absence requirements. Preserve complex typed values, including custom data. Matching item type is required even when the component patch is empty. Null is handled by CraftContainer as NO_LOCK; a nonnull empty stack follows `asNMSCopy`'s empty-item path.

CraftContainer's legacy `setLock(null)` sets the NO_LOCK sentinel, but `setLock("")` constructs a NEW exact CUSTOM_NAME expectation from `CraftChatMessage.fromStringOrNull("")`. [CraftChatMessage](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-server/src/main/java/org/bukkit/craftbukkit/util/CraftChatMessage.java), lines 189–194, returns null for empty strings. CraftContainer's `isLocked` compares sentinel identity, so the empty-string setter remains reported locked. This contradicts the Lockable JavaDoc's claim that empty removes the lock. It is not permission to normalize empty to null.

The local 26.2 `core/component/DataComponentExactPredicate.java` builder accepts a typed null expectation and `test` compares with `Objects.equals`, yielding a missing-custom-name check. Foton's registered typed exact values cannot safely pretend that null is an ordinary component. Treat the legacy absent-name expectation and its wire/save behavior as an explicit compatibility representation requirement; do not insert JSON null into vanilla extracted data. The exact Paper 1.21.11 runtime/save behavior of that malformed-looking edge requires the official fixture, because the inspected Craft source establishes construction but local 26.2 cannot prove the older codec's error behavior. Other lock predicates can proceed without guessing this edge. Test null, empty, a legacy-formatted name, explicit item components, removed components and count independence.

### Tile PDC and inherited state behavior

The [Paper BlockEntity patch](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-server/patches/sources/net/minecraft/world/level/block/entity/BlockEntity.java.patch), lines 17, 39–55, 62–67 and 112–117, initializes PDC once, clears then reads `PublicBukkitValues` on load, and writes it only when nonempty in BOTH normal and custom-only save paths. Client-sent NBT removes that key. Implement this lifecycle in Foton `BlockEntityBase` and the common `load_with_components`/`save_custom_only` path; `save_without_metadata` already delegates to custom-only. Ensure replacing with empty PDC removes old contents and repeated saves do not append duplicate keys. Keep persistence data off client update packets.

[CraftBlockState](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-server/src/main/java/org/bukkit/craftbukkit/block/CraftBlockState.java), lines 49–60 and 188–232: detached `update` returns true without world writes; overload defaults are `force=false`, `applyPhysics=true`. Nonforced update rejects a different current MATERIAL, not changed properties. Forced update replaces the block. `copy()` retains coordinates but clears world; `copy(location)` takes the supplied world/coordinates without writing the destination. `isPlaced` is world presence; detached getBlock/getWorld/getChunk throw; getLocation retains coordinates and a null world. `place(flags)` is an INTERNAL Craft helper, not a method to add to the public API.

The [official BlockState API](https://raw.githubusercontent.com/PaperMC/Paper/ver/1.21.11/paper-api/src/main/java/org/bukkit/block/BlockState.java) confirms detached update success, detached access exceptions, copy overload descriptors and default update overloads. Source takes precedence over the misleading copy(location) prose calling it unplaced: actual state placement follows world presence. `CraftBlockEntityState.isSnapshot` reports whether snapshotting was disabled; ordinary capture and copies are snapshots, `getState(false)` uses the live BE. Tile update applies serialized state to a matching current BE then marks it changed. The recipe-duration exception above follows from this same serialization boundary.

### Foton's existing mutation and serialization mechanisms

Verified local integration points:

- `foton-plugin/src/natives.rs:7560–7587`: `TICK_THREAD`, `on_tick`, `begin_tick`, and a deferred queue restricted to `(world, position, block state)`. `forward.rs:1206–1211` marks the tick and drains the Java scheduler before plugin tasks. This queue has no payload/result transaction and cannot implement boolean tile update truthfully.
- `natives.rs:8906–8948`: `set_block` writes immediately on tick, otherwise queues; boolean `break_naturally` already returns false off tick. Recommendation: capture/apply tile operations require the tick thread and return an explicit failure/exception off tick rather than enqueueing a partial snapshot and falsely returning success. Plugins already have synchronous scheduler execution. No blocking waits or new executor are needed.
- `foton-core/src/world/block_updates/mod.rs:151–220`: world changes require the serialized mutation phase. `set_block_if_unchanged` protects a palette claim, but callbacks still require that phase. Use the normal set-block path with validated material/force and the correct physics flags; the full original-state equality is not the Bukkit validation rule.
- `foton-core/src/block_entity/mod.rs:520–560`: common load/save entry points support complete existing payloads, components and metadata. Add PDC there; do not call one stand's `load_additional` directly and accidentally skip common fields. Use a typed capture/apply interface for JNI, preserving the source-established NBT narrowing of persisted brew time (`short`) and fuel (`byte`) when reproducing snapshot/update round trips. Do not silently clamp setters that Paper leaves unrestricted.
- `foton-core/src/inventory/lock.rs:416–478`: `ContainerLockGuard::set_item`/`set_changed` notify the owner after releasing locks; `run_unlocked` is the existing pattern for re-entry. The current hopper JNI setter calls `get_mut(...).set_item` directly, so reusing its name is not proof of dirty/owner notification. Brewing live writes and commit must use the notifying guard path or explicitly release and mark the BE changed.
- `natives.rs:7915,8050`: `describe_slot`/`parse_slot` are bounded item-wire codecs used by the existing inventory bridge, carrying PDC and supported typed/opaque metadata. Do not assume opaque NBT alone gives access to the typed component PATCH required by setLockItem. Derive lock expectations from the decoded native typed patch and verify complex/removed-component cases before exposing that API.
- Actual event dispatcher is `plugin-api/src/foton/EventBridge.java`; register new natives alongside the existing inventory methods in `natives.rs` and declarations in `Native.java`. Keep implementation in the proposed focused block-state module; expose only the existing tick check/helper needed by it.

## Minimum recommended implementation slice

First deliver the reusable live/template predicate matcher, native brewing lock persistence/open checks, and common tile PDC lifecycle with targeted tests. Next deliver one coherent brewing capture/apply + real Java holder including detached/live modes, duration scaling and source-derived copy/update semantics. Only then expose BrewEvent. This avoids introducing an incomplete Container interface that existing downstream code could mistake for a finished foundation.

Resolved: recipe initialization/countdown/progress and lack of persisted duration; item-key predicate composition; tile PDC tag/lifecycle; copy/update/isSnapshot source semantics; available Foton mutation and ownership mechanisms. The two source ambiguities were subsequently tested on the pinned Paper runtime, as recorded below. Other holder, key-item, player-interaction and event scenarios still require their implementation acceptance tests.

## Executed Paper oracle and empty-lock safety boundary

The real Paper 1.21.11 build 132 oracle ran four isolated server sessions (two fresh worlds and their restarts), each exiting 0. Its bootstrap hash is `5ffef465eeeb5f2a3c23a24419d97c51afd7dbb4923ff42df9a3f58bba1ccfba`; API hash is `c577b181c11a8674310e56c92a91e31c010b7f04c9bd10b91c3be18374401070`. Full fixture, input hashes, console/observation logs and report are retained at `%TEMP%/Foton-BrewingHolder-Paper-oracle-6e274469b497467795fc052b622061d1/REPORT.md`.

- Live recipe duration 123 remains 123 after updating a snapshot whose duration was changed to 222. Ordinary captures and copies use 400. Zero and negative setters throw `IllegalArgumentException`. Restart resets the transient duration to 400 while remaining time 87 and fuel 9 survive. This confirms the source-derived snapshot/update contract; no menu-packet scaling or actual potion cycle was tested.
- Null clears a named lock and survives restart unlocked. A formatted nonempty lock (`§aOracle`) survives with its typed custom-name predicate. `setLockItem` and actual player key matching were not part of this oracle.
- Empty string is an upstream broken state, not equivalent to null: the setter succeeds and `isLocked` is true, but getter/copy/NBT serialization throw `NullPointerException`. Snapshot update throws without changing the tested world's lock. Live update returns true while retaining the invalid predicate.
- Saving a live empty lock logs `Failed to save chunk data` for its entire chunk without making `World.save()` throw. A second controlled world separated this case into chunk (2,0): valid controls in chunk (0,0) persisted; the modified block in (2,0) was AIR after restart. A clean process exit therefore does not prove successful persistence.

Implementation ruling: reject the empty legacy string explicitly with `IllegalArgumentException` before changing captured or native state. Do not substitute NO_LOCK and do not reproduce an unsavable live predicate. This is an intentional, disclosed safety difference from this pinned Paper bug, not a passing Paper-equivalence case. `setLock(null)` remains the supported unlock operation. Keep the edge visible in the compatibility ledger; no unqualified full-API equivalence claim may hide it. Default vanilla locks and nonempty valid Paper locks must retain exact typed matching and persistence.

No extractor output was identified as missing for this slice. No generated files should be edited. Keep ownership sequential at shared `natives.rs`, `Native.java`, common block-state files and event forwarding touchpoints; unrelated active edits must be preserved.
