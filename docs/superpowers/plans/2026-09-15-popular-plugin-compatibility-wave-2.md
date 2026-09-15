# Popular Plugin Compatibility — Wave 2 Implementation Plan

> Execute every implementation task with subagent-driven development. Each task requires a fresh implementer, specification review, code-quality review, and fixes before advancing.

**Goal:** Remove the highest-value real plugin incompatibilities that can be implemented from existing Foton foundations, while making the plugin API build reproducible from paths containing spaces.

**Architecture:** Build-time compatibility fixes stay in `dev/` and the source updater. Bukkit surface adapters live in the Java API and delegate to current abstractions. Entity behavior crosses the existing JNI boundary into real Foton entity state. Modern names alias existing keyed objects. Nothing adds work to the game tick.

**Tech stack:** Rust nightly, Java/JNI, Bash, Python generator scripts, Cargo tests, plugin API checks, Zelda Civ Maven compile gate.

---

### Task 1: Make plugin-API generation fresh-checkout and path safe

**Files:**
- Modify: `dev/build-plugin-api.sh`
- Modify as needed: `dev/gen-entity-type.py`, `dev/gen-enchantment.py`, `dev/gen-potion-type.py`
- Modify: `update-minecraft-src.sh`
- Add/modify focused tests under `dev/tests/`

**Steps:**
1. Add a failing test that copies the required project inputs beneath a temporary path containing spaces, omits ignored generated registry Rust, and demonstrates the current build failure or malformed javac argfile.
2. Make `build-plugin-api.sh` detect missing generated registry inputs and invoke the existing `foton-registry` build to generate them before Java generation.
3. Emit a javac argfile whose individual source paths are correctly quoted/escaped according to javac argfile syntax.
4. Parse the `+mc...` target from workspace metadata in `update-minecraft-src.sh`, pin GitCraft to `61c79f013547b4782096c7a15c183b3f79548e60`, use a temporary directory outside the repository, and pass an exact-version bound. Test command construction without downloading sources.
5. Run the focused tests and `dev/build-plugin-api.sh` from the normal worktree.
6. Commit.

### Task 2: Expose existing entity truth through the JNI bridge

**Files:**
- Modify: `plugin-api/src/org/bukkit/entity/Entity.java`
- Modify: `plugin-api/src/foton/FotonEntity.java`
- Modify: `plugin-api/src/foton/Native.java`
- Modify: `foton-plugin/src/natives.rs`
- Modify relevant focused Java/Rust tests

**Steps:**
1. Add failing API checks for scoreboard tags, dimensions, destination location, gravity, silence, rotation, and rain state.
2. Add failing Rust tests for each new native binding's observable delegation and invalid/missing-entity handling.
3. Implement request-time JNI methods that call existing entity state: tag access/mutation, no-gravity inversion, silence, validated rotation, and rain query. Derive width/height and destination location in Java from existing native results.
4. Preserve existing return/error conventions and avoid all tick-path changes.
5. Run focused Rust tests, plugin API checks, and clippy for the touched crate.
6. Commit.

### Task 3: Add exact API adapters

**Files:**
- Modify: `plugin-api/src/org/bukkit/inventory/EntityEquipment.java`
- Modify: `plugin-api/src/org/bukkit/plugin/Plugin.java`
- Modify: `plugin-api/src/org/bukkit/World.java` and/or `plugin-api/src/foton/FotonWorld.java`
- Modify: `plugin-api/src/org/bukkit/Registry.java`
- Modify: `plugin-api/src/org/bukkit/inventory/ShapelessRecipe.java`
- Modify: `plugin-api/src/io/papermc/paper/datacomponent/item/ResolvableProfile.java`
- Modify: `plugin-api/src/org/bukkit/NamespacedKey.java`
- Modify focused Java checks

**Steps:**
1. Add failing compile/behavior checks for every listed adapter, including equipment slot mapping and repeated ingredient count.
2. Implement each adapter strictly by forwarding to existing live methods or collections.
3. Ensure `NamespacedKey` satisfies Adventure `Key` without changing equality/key normalization semantics.
4. Run plugin API checks and build.
5. Commit.

### Task 4: Add identity-preserving modern aliases

**Files:**
- Modify: `plugin-api/src/org/bukkit/attribute/Attribute.java`
- Modify: `plugin-api/src/org/bukkit/potion/PotionEffectType.java`
- Modify focused Java checks

**Steps:**
1. Add failing checks that compile the modern names and assert reference identity with their legacy/current keyed counterpart.
2. Add attributes `ARMOR`, `ARMOR_TOUGHNESS`, `KNOCKBACK_RESISTANCE`, `MOVEMENT_SPEED`, and `SCALE` as aliases of the existing `GENERIC_*` constants.
3. Add potion effects `HASTE`, `JUMP_BOOST`, `RESISTANCE`, and `STRENGTH` as aliases of the existing instances.
4. Verify registry lookup still returns the canonical keyed instance and no duplicate values appear.
5. Run plugin API checks and build.
6. Commit.

### Task 5: Match Paper's JDBC runtime and self-disable lifecycle

**Files:**
- Modify: `plugin-api/lib/manifest.txt` and runtime library preparation as required
- Modify: `plugin-api/src/foton/PluginHost.java`
- Modify: plugin lifecycle fixture sources/resources under `plugin-api/fixtures/`
- Modify focused Java checks

**Steps:**
1. Add a JVM-isolated fixture with a tiny host-classpath JDBC provider and `META-INF/services/java.sql.Driver`; its plugin calls only `DriverManager.getConnection` during enable. Observe the current `No suitable driver` failure when the normal thread context classloader cannot see the provider.
2. Pin the same Xerial SQLite JDBC line supplied by the target Paper server in Foton's verified runtime manifest, including exact filename and SHA-256 through the existing library preparation path.
3. Initialize only host-visible JDBC providers with `PluginHost.class.getClassLoader()` before plugin construction/lifecycle, while leaving the thread context classloader unchanged. Do not scan plugin-private providers or register plugin-owned drivers globally.
4. Add a lifecycle fixture whose `onEnable` disables itself. Guard the post-callback transition so Foton emits no enable event, does not log success, and leaves the plugin disabled.
5. Run focused lifecycle/JDBC checks and the normal plugin API build. Verify the provider remains host-owned and reload does not accumulate plugin-owned driver registrations.
6. Re-run Zelda `ae06d49cb093a977ce2192bb31426a118d342c80` and confirm startup advances past SQLite driver acquisition.
7. Commit.

### Task 6: Add additional API already backed by live foundations

**Files:**
- Modify: `plugin-api/src/org/bukkit/entity/Player.java`
- Modify: `plugin-api/src/org/bukkit/OfflinePlayer.java`
- Modify/add: `plugin-api/src/org/bukkit/entity/AnimalTamer.java`
- Modify: `plugin-api/src/org/bukkit/GameRule.java`
- Modify: `plugin-api/src/org/bukkit/Tag.java`
- Modify: `plugin-api/src/org/bukkit/enchantments/Enchantment.java`
- Modify: `plugin-api/src/foton/Native.java` and `foton-plugin/src/natives.rs` only for the enchantment compatibility binding
- Modify focused Java/Rust checks

**Steps:**
1. Add failing checks for the Bukkit player hierarchy, four legacy game-rule handles, live trimmable-armor tag lookup, and bidirectional enchantment conflicts.
2. Correct the hierarchy without duplicating player state or weakening existing implementations.
3. Map `ANNOUNCE_ADVANCEMENTS`, `DO_INSOMNIA`, `DO_PATROL_SPAWNING`, and `DO_TRADER_SPAWNING` to the exact Minecraft 26.2 rule names already accepted by the live native registry.
4. Add `ITEMS_TRIMMABLE_ARMOR` as a normal live item-tag handle.
5. Delegate `Enchantment.conflictsWith` to the existing registry exclusive-set logic through a request-time binding; do not copy or hardcode the set.
6. Run focused plugin API/Rust tests and clippy for the touched crate.
7. Commit.

### Task 7: Rebaseline real-plugin evidence and finish verification

**Files:**
- Modify compatibility design/report files that contain the current evidence and metrics
- Modify `dev/test-counts.txt` only if verified Rust test totals changed

**Steps:**
1. Build the exact Foton branch and compile Zelda Civ at the latest gate commit, starting from the clean 392-error Foton-only baseline at `de3d12ac00a04f077b68c7537b36c689ac3679a2`. Fail the gate if any `paper-api-*.jar` remains on the classpath.
2. Confirm every planned diagnostic shape is gone; classify any residuals rather than adding declarations blindly.
3. Start Zelda at or after `ae06d49cb093a977ce2192bb31426a118d342c80`, where Simple Voice Chat is truly optional, and confirm it advances beyond shaded SQLite driver acquisition.
4. Regenerate compatibility evidence and update the public report with exact, non-inflated metrics.
5. Run `bash dev/ci.sh` on the exact final commit. Fix all failures and rerun until green.
6. Request final whole-branch code review, resolve all Critical/Important findings, and rerun affected verification.
7. Commit the final evidence.
