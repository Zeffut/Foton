# Popular Plugin Compatibility — Wave 3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task gets a fresh implementer, specification review, quality review, and fix loop before the next task begins.

**Goal:** Remove all 41 Zelda Civ compile residuals measured at Foton `5fa4936d6bc58c114d8bcee0e6c5d6520a8cf55a` / Zelda `3d7dc062b4353bced69f8383f52c961a5d42e17f`, then advance Zelda through runtime with only live-backed behavior.

**Architecture:** Correct entity identity first, then add event, ray, entity, component, and block-data adapters at their existing native state boundaries. Java objects are façades over Rust state or immutable event snapshots; core game outcomes remain authoritative. Every path is request- or operation-driven, with no new idle/tick work.

**Tech Stack:** Rust nightly, Java 21/JNI, Bash, Python generators, Cargo tests, generated Java checks, isolated real-JVM fixtures, Zelda Maven/runtime gates.

**Spec:** `docs/superpowers/specs/2026-09-16-popular-plugin-compatibility-wave-3-design.md`

## Global constraints

- Exact bases: Foton `5fa4936d6bc58c114d8bcee0e6c5d6520a8cf55a`; Zelda `3d7dc062b4353bced69f8383f52c961a5d42e17f`.
- The valid compile baseline is 41 errors on an 86-entry classpath containing zero `paper-api` jars; the final target is zero errors.
- No stubs, fake Vanilla data, Java-only shadow state, direct extracted-JSON edits, or direct generated-source edits.
- No new per-tick scan, allocation, JNI call, synchronization, polling, sleep, or async wait.
- Release Rust locks before JNI/event callbacks; listeners may reenter server APIs.
- Use `foton_utils::{Downcast, DowncastType, DowncastTypeKey}` for exact plugin-extensible downcasting; no `Any`/`TypeId` substitute.
- Real JVM/JNI tests run in isolated processes after `bash dev/build-plugin-api.sh --check`.
- Update `dev/test-counts.json` only from a verified Unix/Linux count when test totals change.
- `ChestBoat` refill/player-history API and `MusicInstrument.create(...)` dynamic registration stay omitted and documented as ABI gaps.

---

### Task 1: Repair entity wrapper identity and class-to-key spawning

**Files:**
- Modify: `dev/gen-entity-type.py`
- Modify: `plugin-api/src/foton/FotonWorld.java`
- Modify: `plugin-api/src/foton/FotonEntity.java`
- Modify: `plugin-api/src/foton/FotonChunk.java`
- Modify: `plugin-api/src/foton/FotonProjectile.java`
- Modify: `plugin-api/src/foton/FotonEntityFactory.java`
- Modify: `plugin-api/check/EntityCheck.java`
- Modify: `plugin-api/check/Checks.java`
- Modify: `dev/build-plugin-api.sh` only to wire a new generated class-to-type source if the generator cannot emit into an existing generated source

**Interfaces:**
- Produce: `static org.bukkit.entity.Entity FotonWorld.wrapEntity(UUID uuid, String type)`; `type` is a normalized registry path such as `item_display`, not a UUID.
- Produce: generated `Class<? extends Entity> -> EntityType` lookup used by `World.spawn(Location, Class<T>, ...)`.
- Preserve: stale UUID handling and all existing wrapper classes.

- [x] **Step 1: Add the failing wrapper test**

  Add checks that call the current wrapper with a real UUID/type pair and assert an underscored type is not looked up as a UUID:

  ```java
  Entity wrapped = FotonWorld.wrapEntity(id, "item_display");
  require(wrapped.getUniqueId().equals(id));
  require(!(wrapped.getClass().equals(FotonEntity.class)));
  require(EntityType.ITEM_DISPLAY == FotonEntityFactory.typeFor(ItemDisplay.class));
  ```

- [x] **Step 2: Run RED**

  Run `bash dev/build-plugin-api.sh --check` and the Java check harness. Expected: `item_display` falls back to `FotonEntity`, and class-to-type lookup is absent or derives `itemdisplay`.

- [x] **Step 3: Generate exact class-to-registry metadata**

  Extend `dev/gen-entity-type.py` from the existing entity registry input so the generated Java mapping contains the canonical class and `EntityType` identity. Do not edit generated Java by hand and do not infer snake case at runtime.

- [x] **Step 4: Make wrapping single-pass**

  Change `FotonWorld.wrapEntity` to switch on the supplied type and update every caller to resolve `Native.entityType(uuid.toString())` exactly once. `FotonEntity.handle()` becomes:

  ```java
  String type = Native.entityType(id.toString());
  Entity wrapped = FotonWorld.wrapEntity(id, type);
  return wrapped == null ? this : wrapped;
  ```

- [x] **Step 5: Run GREEN and mutation check**

  Run the Java check harness, `bash dev/build-plugin-api.sh --check`, `python3 dev/check-natives.py --quiet`, `git diff --check`, and a mutation that restores the second lookup to prove the wrapper test fails.

- [x] **Step 6: Commit checkpoint**

  ```bash
  git add dev/gen-entity-type.py dev/build-plugin-api.sh plugin-api/src/foton plugin-api/check
  git commit -m "fix(plugin): preserve entity wrapper identity"
  ```

**Performance invariant:** one native type lookup per wrapper creation, never two; no cache or tick hook.

**Scope boundary:** this task fixes identity and spawn key selection only. It does not add the four Wave 3 entity contracts.

---

### Task 2: Add live BrewEvent and PrepareItemEnchantEvent

**Files:**
- Modify: `foton-core/src/block_entity/entities/brewing_stand.rs`
- Modify: `foton-core/src/inventory/menu/kinds/enchantment_menu.rs`
- Modify: `foton-core/src/event/inventory.rs`
- Modify: `foton-core/src/event/mod.rs`
- Modify: `foton-plugin/src/forward.rs`
- Modify: `plugin-api/src/foton/EventBridge.java`
- Modify: `plugin-api/src/foton/FotonBlock.java`
- Create: `plugin-api/src/foton/FotonBrewerInventory.java`
- Create: `plugin-api/src/foton/FotonBrewingStand.java`
- Create: `plugin-api/src/org/bukkit/block/BrewingStand.java`
- Create: `plugin-api/src/org/bukkit/inventory/BrewerInventory.java`
- Create: `plugin-api/src/org/bukkit/event/inventory/BrewEvent.java`
- Create: `plugin-api/src/org/bukkit/enchantments/EnchantmentOffer.java`
- Create: `plugin-api/src/org/bukkit/event/enchantment/PrepareItemEnchantEvent.java`
- Create: `plugin-api/src/org/bukkit/inventory/view/EnchantmentView.java`
- Create: `plugin-api/check/BrewingAndEnchantingCheck.java`
- Modify: `plugin-api/check/Checks.java`
- Test: focused tests beside `foton-core/src/block_entity/entities/brewing_stand.rs` and `foton-core/src/inventory/menu/kinds/enchantment_menu.rs`

**Interfaces:**
- Produce: `BrewEvent` with snapshot-backed `BrewerInventory`, mutable `List<ItemStack> getResults()`, `int getFuelLevel()`, and cancellation.
- Produce: core brew-completion event carrying five pre-brew slots, three mutable result slots, fuel, and cancellation.
- Produce: `PrepareItemEnchantEvent` with mutable `EnchantmentOffer[3]`; cancellation clears live offers.
- Consume: Task 1's correct entity/wrapper foundation only indirectly through event objects.

- [ ] **Step 1: Write Brew RED tests**

  Cover successful mutation and cancellation:

  ```rust
  // Cancellation: bottle, ingredient, remainder, sound and world state unchanged.
  // Mutation: listener-replaced results occupy slots 0..3; a shortened list clears trailing slots.
  // Reentrancy: listener can query the same stand because no container lock is held.
  ```

  The Java check must load Zelda's listener signature and construct `BrewEvent` with the exact Paper-facing getters.

- [ ] **Step 2: Observe Brew RED**

  Run the focused brewing tests and Java checks. Expected: class resolution fails, and core completion commits before any event exists.

- [ ] **Step 3: Split brew preview from commit**

  Replace monolithic `brew()` with pure preview plus commit operations. Copy the fixed five-slot snapshot, calculate the three Vanilla results, drop the lock, dispatch, then either return unchanged on cancellation or reacquire and atomically commit listener state before consuming the ingredient. Emit sound only after successful commit.

- [ ] **Step 4: Write enchanting RED tests**

  Assert three exact offer slots, mutable costs/enchantments/levels, pre-cancellation for non-enchantable nonempty items, cancellation clearing offers, and listener mutations becoming the synchronized menu result.

- [ ] **Step 5: Observe enchanting RED**

  Run `cargo test -p foton-core enchantment_menu -- --nocapture` and the Java checks. Expected: no prepare event path and missing package/type.

- [ ] **Step 6: Implement snapshot-backed enchanting dispatch**

  In `recompute_offers`, compute Vanilla offers first, create the event snapshot, dispatch with no menu lock crossing JNI, then install either zeroed offers or the listener-mutated three-element array. Reject invalid/null entries using Paper's observable empty-offer semantics rather than panicking.

- [ ] **Step 7: Run GREEN**

  Run focused brewing/enchanting tests, `cargo check -p foton-core -p foton-plugin --all-targets`, `bash dev/build-plugin-api.sh --check`, Java checks, `python3 dev/check-natives.py --quiet`, `cargo fmt --all --check`, and `git diff --check`.

- [ ] **Step 8: Runtime checkpoint**

  Rebuild the Foton candidate and start Zelda. Verify reflective registration advances beyond both event parameter types; record the next first failure rather than broadening this task.

- [ ] **Step 9: Commit checkpoint**

  ```bash
  git add foton-core/src/block_entity/entities/brewing_stand.rs foton-core/src/inventory/menu/kinds/enchantment_menu.rs foton-core/src/event foton-plugin/src/forward.rs plugin-api/src plugin-api/check
  git commit -m "feat(plugin): expose brewing and enchant preparation events"
  ```

**Performance invariant:** one synchronous dispatch only at brew completion or existing offer recomputation; no extra tick scan. The brewing lock is never held across JNI.

**Scope boundary:** `EnchantItemEvent` and unrelated `org.bukkit.event.enchantment` types are not added unless the exact Zelda gate newly proves they are required.

---

### Task 3: Add exact on-demand block and entity ray tracing

**Files:**
- Create: `plugin-api/src/org/bukkit/FluidCollisionMode.java`
- Create: `plugin-api/src/org/bukkit/util/RayTraceResult.java`
- Create: `plugin-api/src/org/bukkit/RayTracingSupport.java`
- Create: `plugin-api/check/RayTraceCheck.java`
- Modify: `plugin-api/check/Checks.java`
- Modify: `plugin-api/src/org/bukkit/World.java`
- Modify: `plugin-api/src/foton/Native.java`
- Modify: `foton-plugin/src/natives.rs`
- Modify: `foton-core/src/world/tests.rs`

**Interfaces:**
- Produce: `World.rayTraceBlocks(Location, Vector, double, FluidCollisionMode, boolean)` plus the standard `Location` block overloads.
- Produce: `World.rayTraceEntities(Location, Vector, double, Predicate<? super Entity>)` plus the standard `Location` entity overloads.
- Produce: immutable `RayTraceResult` with defensive hit-position copies and nullable block/face/entity fields.
- Consume: Task 1's typed wrappers for entity predicate/result identity.

- [ ] **Step 1: Add Java ABI/value RED**

  Check enum order `NEVER`, `SOURCE_ONLY`, `ALWAYS`; all five result constructors; four getters; vector defensive copying; equality/hash/text; null, non-finite, zero-vector, and negative-distance validation.

- [ ] **Step 2: Add core/native RED**

  Add tests for water source versus flowing fluid, `NEVER`, `SOURCE_ONLY`, `ALWAYS`, collider versus outline, exact hit face/position, miss encoding, and nearest entity after predicate filtering.

- [ ] **Step 3: Observe RED**

  Run Java checks, `cargo test -p foton-core world::tests -- --nocapture`, and the focused `foton-plugin` native tests. Expected: missing Java surface/native registration.

- [ ] **Step 4: Implement block delegation**

  Add one primitive-array JNI call that resolves the world, maps the enum/shape flags, calls `World::clip`, and encodes hit position, block coordinates, and Vanilla direction ID. Do not add chunk loading.

- [ ] **Step 5: Implement entity nearest-AABB tracing**

  Build the directional search AABB, use the existing loaded-entity query, wrap candidates through Task 1, apply the Java predicate, clip each live AABB, and select the strict nearest squared distance. Entity-only tracing is not shortened by a block hit.

- [ ] **Step 6: Run GREEN and Zelda gate**

  Run all new checks, `python3 dev/check-natives.py --quiet`, `bash dev/build-plugin-api.sh --check`, focused Rust tests, formatting/diff checks, then Zelda's compile gate. Assert all seven ray-tracing diagnostics disappear.

- [ ] **Step 7: Commit checkpoint**

  ```bash
  git add plugin-api/src/org/bukkit plugin-api/src/foton plugin-api/check foton-plugin/src/natives.rs foton-core/src/world/tests.rs
  git commit -m "feat(plugin): add live world ray tracing"
  ```

**Performance invariant:** one DDA/native call per block trace; one existing spatial query plus candidate AABB clips per entity trace; zero idle work.

**Scope boundary:** unloaded chunks remain air, and Position/block-predicate/combined-builder overloads are not claimed.

---

### Task 4: Add live display, Allay, and chest-boat identities

**Files:**
- Modify: `dev/gen-entity-type.py`
- Modify: `dev/fetch-plugin-api-libs.sh`
- Modify: `plugin-api/lib/manifest.txt`
- Modify: `plugin-api/src/org/bukkit/entity/Display.java`
- Create: `plugin-api/src/org/bukkit/entity/ItemDisplay.java`
- Create: `plugin-api/src/org/bukkit/entity/TextDisplay.java`
- Create: `plugin-api/src/org/bukkit/entity/Allay.java`
- Create: `plugin-api/src/org/bukkit/entity/ChestBoat.java`
- Create: `plugin-api/src/foton/FotonDisplay.java`
- Create: `plugin-api/src/foton/FotonItemDisplay.java`
- Create: `plugin-api/src/foton/FotonTextDisplay.java`
- Create: `plugin-api/src/foton/FotonAllay.java`
- Create: `plugin-api/src/foton/FotonChestBoat.java`
- Modify: `plugin-api/src/foton/FotonBoat.java`
- Modify: `plugin-api/src/foton/FotonMenuInventory.java`
- Modify: `plugin-api/src/foton/FotonInventoryView.java`
- Modify: `plugin-api/src/foton/EventBridge.java`
- Modify: `plugin-api/src/foton/Native.java`
- Modify: `foton-plugin/src/natives.rs`
- Modify: `foton-plugin/src/forward.rs`
- Modify: `foton-plugin/src/natives_live_api_tests.rs`
- Modify: `foton-core/src/event/inventory.rs`
- Modify: `foton-core/src/entity/entities/objects/display_ui/mod.rs`
- Modify: `foton-core/src/entity/entities/objects/display_ui/item_display.rs`
- Modify: `foton-core/src/entity/entities/objects/display_ui/text_display.rs`
- Modify: `foton-core/src/entity/entities/mobs/passive/allay/mod.rs`
- Modify: `foton-core/src/entity/entities/objects/vehicles/chest_boat.rs`
- Modify: `plugin-api/check/EntityCheck.java`

**Interfaces:**
- Produce: full shared `Display` state used by Zelda, including transformation/matrix, interpolation and teleport durations, view/shadow/display dimensions, delay, billboard, glow override, and brightness.
- Produce: live `ItemDisplay` item/transform and `TextDisplay` Adventure/string/style accessors.
- Produce: typed `Allay` wrapper and live inherited pickup behavior.
- Produce: chest-boat inventory whose holder UUID is the chest boat, carried through inventory-open forwarding.
- Consume: Task 1's generated type mapping and single-pass wrappers.

- [ ] **Step 1: Add wrapper/spawn RED**

  In an isolated server/JNI fixture, spawn `item_display`, `text_display`, `allay`, and each chest-boat type, then assert the Java class and UUID are exact. Assert class-based display spawning uses underscored registry keys.

- [ ] **Step 2: Add display-state RED**

  For every Zelda-used setter/getter, mutate through Java, read native synchronized state, save/load the entity, and assert the value persists when the API contract claims persistent entity state. Include styled Adventure JSON round-trip and brightness/glow null behavior.

- [ ] **Step 3: Implement generated wrappers and live display bridge**

  Generate mappings, add request-time natives over existing entity-data fields, and use pinned Adventure Gson serialization for text. Persist shared Display fields in their owning core serialization path before exposing them as durable getters.

- [ ] **Step 4: Add chest-boat holder RED/GREEN**

  Open a chest boat, assert one `InventoryOpenEvent`, assert cancellation prevents opening, and assert `event.getInventory().getHolder()` and the live view holder wrap the same chest-boat UUID. Extend the core open event payload with `Option<Uuid>` holder identity rather than inferring from slot count.

- [ ] **Step 5: Run GREEN and Zelda gate**

  Run focused core entity tests, isolated live JNI fixture, plugin API checks/build, native check, formatting/diff checks, then Zelda compile. Assert all 11 entity/display diagnostics disappear.

- [ ] **Step 6: Commit checkpoint**

  ```bash
  git add dev/gen-entity-type.py dev/fetch-plugin-api-libs.sh plugin-api/lib/manifest.txt plugin-api/src plugin-api/check foton-core/src/event/inventory.rs foton-core/src/entity foton-plugin/src
  git commit -m "feat(plugin): expose live display and chest-boat entities"
  ```

**Performance invariant:** JNI only on plugin calls; dirty display metadata continues to coalesce in the existing pass; holder UUID is transported with the existing open event.

**Scope boundary:** omit unsupported chest-boat refill/player-history methods. Do not return zero/false timestamps or histories. Their absence remains an explicit method-level ABI gap.

---

### Task 5: Add native item components, instruments, and generated Ageable data

**Files:**
- Create: `plugin-api/src/io/papermc/paper/datacomponent/item/BannerPatternLayers.java`
- Modify: `plugin-api/src/io/papermc/paper/datacomponent/DataComponentTypes.java`
- Modify: `plugin-api/src/org/bukkit/Registry.java`
- Modify: `plugin-api/src/org/bukkit/block/banner/Pattern.java`
- Modify: `plugin-api/src/org/bukkit/block/banner/PatternType.java`
- Create: `plugin-api/src/org/bukkit/MusicInstrument.java`
- Create: `plugin-api/src/org/bukkit/inventory/meta/MusicInstrumentMeta.java`
- Create: `plugin-api/src/org/bukkit/inventory/meta/SimpleMusicInstrumentMeta.java`
- Modify: `plugin-api/src/org/bukkit/inventory/ItemStack.java`
- Modify: `plugin-api/src/foton/FotonInventory.java`
- Modify: `foton-plugin/src/natives.rs`
- Create: `plugin-api/src/org/bukkit/block/data/Ageable.java`
- Create: `plugin-api/src/org/bukkit/block/data/SimpleAgeableData.java`
- Modify: `plugin-api/src/foton/FotonBlock.java`
- Create: `dev/gen-block-data.py` or extend the existing block-data generator selected by `dev/build-plugin-api.sh`
- Modify: `dev/build-plugin-api.sh`
- Create: `plugin-api/check/ComponentsAndBlockDataCheck.java`
- Modify: `plugin-api/check/Checks.java`
- Test: `foton-registry/src/data_components/components/banner_patterns.rs`
- Test: `foton-registry/src/instrument.rs`
- Test: slot codec tests in `foton-plugin/src/natives.rs`

**Interfaces:**
- Produce: ordered, value-equal `BannerPatternLayers` and builder backed by `BANNER_PATTERNS`/`BASE_COLOR` native components.
- Produce: existing Vanilla `MusicInstrument` keyed registry values and `MusicInstrumentMeta` backed by the native `INSTRUMENT` component.
- Produce: `SimpleAgeableData` with material-specific maximum generated from existing `blocks.json` property definitions.

- [ ] **Step 1: Add component/instrument RED**

  Check ordered banner layers, builder copies, equality, clone, slot encode/decode, registry keys, `DREAM_GOAT_HORN`, instrument meta clone, and native inventory round-trip. The test must prove client-relevant instrument data survives leaving Java.

- [ ] **Step 2: Implement native codecs and Java registry views**

  Extend the existing slot description/parser format only when these components are present. Resolve banner patterns and instruments through the published registry by key; return false/empty before registry publication using the existing non-initializing accessor.

- [ ] **Step 3: Add Ageable RED**

  Construct block data for wheat age 7 and beetroot age 3, mutate within range, reject `-1` and `max+1`, clone, and round-trip `getAsString()`. Assert non-age block data does not implement `Ageable`.

- [ ] **Step 4: Generate age metadata and implement dispatch**

  Read the existing extracted block asset during build and emit only source/build output through the generator. Route blocks that own an `age` integer property to `SimpleAgeableData`; use the property's extracted maximum, not a material-name list.

- [ ] **Step 5: Run GREEN and Zelda gate**

  Run registry component/instrument tests, plugin slot-codec tests, Java checks/build, native check, isolated publication-state checks if touched, formatting/diff checks, then Zelda compile. Assert the three metadata/component errors and one block-data error disappear.

- [ ] **Step 6: Commit checkpoint**

  ```bash
  git add dev plugin-api/src plugin-api/check foton-plugin/src/natives.rs foton-registry/src
  git commit -m "feat(plugin): bridge banner instrument and age data"
  ```

**Performance invariant:** component work is linear in the small present payload; age dispatch is generated/local; absent components retain the compact ordinary-slot encoding.

**Scope boundary:** omit `MusicInstrument.create(...)`. Dynamic plugin registry registration remains an explicit ABI gap; do not fake registration or return an unregistered object.

---

### Task 6: Add exact entity load, unload, dismount, and place events

**Files:**
- Modify: `foton-core/src/event/world.rs`
- Modify: `foton-core/src/event/entity.rs`
- Modify: `foton-core/src/event/mod.rs`
- Modify: `foton-core/src/world/entity_management.rs`
- Modify: `foton-core/src/entity/manager/mod.rs`
- Modify: `foton-core/src/entity/base/mod.rs`
- Modify: `foton-core/src/player/mod.rs`
- Modify: `foton-core/src/behavior/items/boat_item.rs`
- Modify: `foton-core/src/behavior/items/minecart_item.rs`
- Modify: `foton-core/src/behavior/items/armor_stand_item.rs`
- Modify: `foton-core/src/behavior/items/end_crystal_item.rs`
- Modify: `foton-plugin/src/forward.rs`
- Modify: `plugin-api/src/foton/EventBridge.java`
- Create: `plugin-api/src/org/bukkit/event/world/EntitiesLoadEvent.java`
- Create: `plugin-api/src/org/bukkit/event/world/EntitiesUnloadEvent.java`
- Create: `plugin-api/src/org/bukkit/event/entity/EntityDismountEvent.java`
- Create: `plugin-api/src/org/bukkit/event/entity/EntityPlaceEvent.java`
- Create: `plugin-api/check/EntityLifecycleEventCheck.java`
- Modify: `plugin-api/check/Checks.java`

**Interfaces:**
- Produce: immutable entity lists for load/unload, with UUIDs resolvable during dispatch.
- Produce: `stop_riding_relationship(force: bool) -> bool` or an equivalent explicit result; `false` means a cancellable dismount was vetoed.
- Produce: pending-spawn-resolvable placed entity and cancellable pre-insertion event.
- Consume: Tasks 1 and 4 wrappers, including chest boats.

- [ ] **Step 1: Add load/unload RED**

  Test one event per chunk transition, exact de-duplicated entity set including passenger trees, immutable Java list, resolvability during callback, and unload ordering before inactive callbacks/final save.

- [ ] **Step 2: Implement transition-boundary dispatch**

  Build the final UUID vectors where the manager already decides inserted/retained entities. Keep a temporary event-visible lookup until unload forwarding returns, then continue callbacks/save/removal.

- [ ] **Step 3: Add dismount RED**

  Test ordinary cancellation retaining both links and suppressing passenger packets; successful unlink; forced teleport/removal with `isCancellable=false` ignoring cancellation.

- [ ] **Step 4: Refactor dismount result and call sites**

  Centralize pre-unlink dispatch in `EntityBase::stop_riding_relationship`, propagate success to the player override and all callers, and mark only removal/teleport cleanup as forced.

- [ ] **Step 5: Add placement RED**

  For boat/chest boat/minecart/armor stand/end crystal, assert event timing after collision validation and before insertion, game event, or item consumption. Cancellation leaves world/inventory unchanged; success inserts and consumes once.

- [ ] **Step 6: Implement pending placement dispatch**

  Reuse the pending-spawn scope, pass hand/block-face context exactly when available, and commit side effects only after event success.

- [ ] **Step 7: Run GREEN and Zelda gate**

  Run focused manager/riding/item tests, Java checks/build, native check, touched-crate checks, formatting/diff checks, then Zelda compile. Assert the seven lifecycle/place diagnostics disappear.

- [ ] **Step 8: Commit checkpoint**

  ```bash
  git add foton-core/src/event foton-core/src/world/entity_management.rs foton-core/src/entity foton-core/src/player foton-core/src/behavior/items foton-plugin/src/forward.rs plugin-api/src plugin-api/check
  git commit -m "feat(plugin): emit entity lifecycle and placement events"
  ```

**Performance invariant:** work occurs only on existing load/unload/dismount/place transitions and is O(the exact affected entity set), with no scan outside that set.

**Scope boundary:** no generalized entity transaction layer and no event for spawn paths not represented by `EntityPlaceEvent`.

---

### Task 7: Add block-drop, armor-change, and smithing behavior

**Files:**
- Modify: `foton-core/src/event/block.rs`
- Modify: `foton-core/src/event/player.rs`
- Modify: `foton-core/src/event/inventory.rs`
- Modify: `foton-core/src/event/mod.rs`
- Modify: `foton-core/src/player/game_mode/block_breaking.rs`
- Modify: `foton-core/src/world/entity_management.rs`
- Modify: `foton-core/src/entity/living_entity.rs`
- Modify: `foton-core/src/entity/living_base/mod.rs`
- Modify: `foton-core/src/inventory/menu/kinds/smithing_menu.rs`
- Modify: `foton-core/src/inventory/menu/kinds/smithing_menu/tests.rs`
- Modify: `foton-core/src/player/player_inventory/player_handlers.rs`
- Modify: `foton-plugin/src/forward.rs`
- Modify: `plugin-api/src/foton/EventBridge.java`
- Modify: `plugin-api/src/foton/FotonInventoryView.java`
- Create: `plugin-api/src/foton/FotonSmithingInventory.java`
- Create: `plugin-api/src/org/bukkit/event/block/BlockDropItemEvent.java`
- Create: `plugin-api/src/com/destroystokyo/paper/event/player/PlayerArmorChangeEvent.java`
- Create: `plugin-api/src/org/bukkit/event/inventory/PrepareResultEvent.java`
- Create: `plugin-api/src/org/bukkit/event/inventory/PrepareSmithingEvent.java`
- Create: `plugin-api/src/org/bukkit/event/inventory/SmithItemEvent.java`
- Create: `plugin-api/src/org/bukkit/inventory/SmithingInventory.java`
- Create: `plugin-api/check/DropArmorSmithingCheck.java`
- Modify: `plugin-api/check/Checks.java`

**Interfaces:**
- Produce: prepare/dispatch/commit block-drop pipeline with mutable pending item entities and original block snapshot.
- Produce: armor events from the existing old/new equipment snapshots for `HEAD`, `CHEST`, `LEGS`, `FEET` only.
- Produce: snapshot-backed smithing result preparation and cancellable result click before consumption.

- [ ] **Step 1: Add block-drop RED**

  Assert Fortune/Silk result creation precedes the event; original `BlockState` survives removal; mutable list removal/replacement affects inserted drops; cancellation inserts none; `BlockBreakEvent#setDropItems(false)` propagates; empty lists follow Paper timing.

- [ ] **Step 2: Implement prepare/dispatch/commit drops**

  Refactor `drop_block_loot` and the player-break use of `pop_resource` to construct item entities without insertion, dispatch with no world/entity lock held, then insert the surviving list. Keep non-player/general `pop_resource` paths unchanged.

- [ ] **Step 3: Add armor RED/GREEN**

  Extend existing equipment-diff tests to assert exact slot enum and old/new stacks, no event for hand/body/saddle changes, and no event when snapshots are equal. Dispatch inside the current diff loop; do not add a second pass.

- [ ] **Step 4: Add smithing RED**

  Assert every input recomputation fires after Vanilla result creation, listener null/replacement controls slot 3, `SmithItemEvent` is an `InventoryClickEvent` for a nonempty result, and cancellation preserves both inputs/result.

- [ ] **Step 5: Implement smithing snapshots and result-click subclass**

  Mirror the existing grindstone snapshot pattern. Install prepare mutations before synchronization. In `player_handlers.rs`, select `SmithItemEvent` only for smithing result slot 3 and abort all generic click/consumption effects on cancellation.

- [ ] **Step 6: Run GREEN and Zelda gate**

  Run block-breaking, equipment, and smithing tests; Java checks/build; native check; touched-crate checks; formatting/diff checks; then Zelda compile. Assert all remaining 12 event diagnostics disappear and the total reaches zero.

- [ ] **Step 7: Commit checkpoint**

  ```bash
  git add foton-core/src/event foton-core/src/player foton-core/src/world/entity_management.rs foton-core/src/entity foton-core/src/inventory foton-plugin/src/forward.rs plugin-api/src plugin-api/check
  git commit -m "feat(plugin): add drop armor and smithing events"
  ```

**Performance invariant:** drop work replaces existing per-break insertion, armor dispatch piggybacks the existing diff, and smithing dispatch occurs only on menu changes/clicks.

**Scope boundary:** no general loot-event rewrite outside player block breaking and no duplicate armor scan.

---

### Task 8: Rebaseline Zelda, run full CI, and close reviews

**Files:**
- Modify: `docs/superpowers/specs/2026-09-16-popular-plugin-compatibility-wave-3-design.md` only for final exact evidence
- Modify: `.superpowers/sdd/2026-09-16-popular-plugin-compatibility-wave-3/progress.md`
- Modify: `design/plugin-compatibility.md` only if final measured public metrics change
- Modify: `dev/test-counts.json` only if the verified suite count changed

**Interfaces:**
- Consume: all seven task checkpoints.
- Produce: exact zero-error compile evidence, runtime enablement/next-blocker evidence, green full CI, and whole-branch approval.

- [ ] **Step 1: Build the exact candidate**

  Record `git rev-parse HEAD`, build/check the plugin API, and stage the Foton runtime candidate. Reject any dirty tracked file or generated/build artifact before Zelda testing.

- [ ] **Step 2: Run the exact Zelda compile gate**

  At Zelda `3d7dc062b4353bced69f8383f52c961a5d42e17f`, run:

  ```bash
  FOTON_REPO='<detached exact Foton candidate>' tools/check-foton-compat.sh
  ```

  Record classpath size, Paper-jar count, log hash, diagnostic count, and delta against 392. Required result: zero compile errors and zero Paper jars.

- [ ] **Step 3: Run isolated Zelda runtime**

  Start with only `zelda-civ-0.1.0.jar`. Verify SQLite integrity, plugin enablement beyond all Wave 3 listener registrations, and the first runtime exception if one remains. Stop cleanly and distinguish Foton process shutdown from Zelda enable success.

- [ ] **Step 4: Run full repository verification**

  Run `bash dev/ci.sh` on the exact commit. If it changes generated test counts or exposes failures, fix through fresh SDD implement/review loops, commit, then rerun the entire script from the new exact SHA.

- [ ] **Step 5: Whole-branch reviews**

  Request one specification review against the Wave 3 design and one quality review over `5fa4936d6..HEAD`. Resolve every Critical/Important finding through a fresh subagent and rerun affected tests plus `bash dev/ci.sh`.

- [ ] **Step 6: Update evidence and commit**

  Write only measured facts: exact SHAs, zero compile residuals, runtime result, CI command/result, deferred ABI gaps, and absence of tick overhead/artifacts.

  ```bash
  git add docs/superpowers/specs/2026-09-16-popular-plugin-compatibility-wave-3-design.md .superpowers/sdd/2026-09-16-popular-plugin-compatibility-wave-3/progress.md design/plugin-compatibility.md dev/test-counts.json
  git commit -m "docs: record wave 3 compatibility evidence"
  ```

- [ ] **Step 7: Final cleanliness**

  Run `git status --short`, `git diff --check HEAD^`, and inspect repository-root build outputs. Required result: clean tracked/untracked worktree except intentionally ignored reproducible caches outside the repository.

**Performance invariant:** the closing evidence includes a diff audit confirming no unconditional tick/JNI path was introduced.

**Scope boundary:** zero Zelda residuals are not advertised as 100% Paper/plugin-market compatibility. Deferred method-level ABI gaps and the historical unknown NMS ceiling remain explicit.
