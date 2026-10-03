# Complete Paper Plugin Compatibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking. Each numbered task is a review gate; split its capability rows into independent, testable worker slices before coding.

**Goal:** Make unchanged Paper plugin JARs function on Foton with Paper-equivalent behavior, reaching 100% of a pinned market certification corpus that includes Observer and Zelda Civ.

**Zelda Civ version-bridge decision (2026-10-03):** The unchanged Zelda Civ JAR built from `origin/main` at `3c324d63c502eb18242af7c771baa11ea1c2915c` is compiled against Paper API 1.21.11. It enables with Simple Voice Chat 2.6.20 on Paper 1.21.11 build 132, but fails on Paper 26.2 build 129 at the inherited `BookMeta.pages(List<Component>)` method. Foton targets Minecraft 26.2, so its Zelda milestone explicitly includes a real backward Paper 1.21.11 API/behavior bridge; matching only Paper 26.2 does not satisfy this user requirement. Keep the 1.21.11 and 26.2 Paper oracles separate. See `dev/compat/zelda-civ-baseline.md` for pinned artifacts and observations.

**Architecture:** Foton's Rust state remains authoritative. A versioned Java Bukkit/Paper/CraftBukkit bridge exposes live state and a real packet pipeline; plugin loading reproduces Paper's descriptor, dependency, library, bootstrap and lifecycle phases. Every claimed capability is compared with a pinned Paper build using the same plugin JAR and scenario.

**Tech Stack:** nightly Rust workspace, JNI, Java 25 runtime for the Paper 26.2 target (Java 21 oracle for Paper 1.21.11), Netty 4.2.15.Final, Paper API/official Paper server as oracle, Python 3 standard library for corpus tooling, Bash/PowerShell CI, FotonExtractor and generated vanilla source.

**Spec:** `PAPER_PLUGIN_COMPATIBILITY_SPEC.md`

## Global Constraints

- `Cargo.toml` currently targets `0.15.2+mc26.2`; check this value again before every implementation wave and keep `minecraft-src/` current with it.
- Paper 26.2 certification runs on Java 25; Paper 1.21.11 reference runs on Java 21. Verify Foton can load Java 25 bytecode before claiming parity with Paper 26.2.
- Do not edit generated Rust, extracted FotonExtractor JSON, `CONFIGURATION.md`, or the Observer/Zelda repositories to make Foton appear compatible.
- Do not introduce placeholder state, silent no-op API methods, fake events, speculative registry values, production `.unwrap()`/`.expect()` or unapproved `unsafe`.
- Keep third-party/plugin JARs out of git unless redistribution is allowed; record URL, version, checksum and license in a manifest. PacketEvents and proprietary plugin JARs remain external inputs.
- Avoid Desktop-root scratch files. Use `%TEMP%`/`/tmp`; keep durable code and documentation inside Foton.
- Run Rust builds under WSL or CI with an isolated Linux `CARGO_TARGET_DIR`; Windows OS error 4551 is a known execution restriction, not a waived test.
- Every task has a failing regression first, a narrow green test, a Paper comparison where observable, an independent review, then a commit containing only that task's files. Preserve the already dirty Foton worktree and all unrelated user changes.
- Do not advance a plugin's status to compatible on ABI coverage or `onEnable` alone. The full certification contract in the spec controls the label.

---

## File and ownership map

| Responsibility | Existing owner / planned additions |
| --- | --- |
| Corpus inventory and status | `dev/plugin_api_usage.py`, `dev/plugin-api-usage.json`; add `dev/plugin_compat.py`, `dev/test_plugin_compat.py`, `dev/compat/corpus.json`, `dev/compat/scenarios/` |
| Paper comparison runner | add `dev/paper-oracle-test.sh`, `dev/compat/compare.py`, `dev/test_compat_compare.py` |
| Descriptors, graph, bootstrap, classpaths | `plugin-api/src/foton/PaperPluginDescriptor.java`, `PluginHost.java`, `org/bukkit/plugin/java/PluginClassLoader.java`; add focused `foton/PluginDependencyGraph.java`, `foton/PluginLibraryResolver.java`, `foton/PaperBootstrapRunner.java` and `plugin-api/check/PaperLoading.java` |
| Public Java API and functional checks | `plugin-api/src/org/bukkit/`, `plugin-api/src/io/papermc/paper/`, `plugin-api/src/foton/`, `plugin-api/check/`; one small module per capability rather than more growth in `FotonPlayer.java` or `natives.rs` |
| Native capability bridge | `foton-plugin/src/natives.rs`, `foton-plugin/src/forward.rs`, `foton-core/src/`; split coherent new JNI services into modules under `foton-plugin/src/compat/` as they are added |
| Packet pipeline | `foton/src/lib.rs`, `foton-plugin/src/via.rs`, `plugin-api/src/foton/network/`; add `foton-plugin/src/packet_pipeline.rs` and protocol tests under `dev/compat/scenarios/network/` |
| Versioned internals | existing `plugin-api/src/net/minecraft/`; add focused `plugin-api/src/org/bukkit/craftbukkit/`, `plugin-api/src/foton/internal/`, `foton-plugin/src/compat/internals/`, generated ABI inventory from the matching Paper build |
| Release proof | `dev/ci.sh`, `.github/workflows/test.yml`, `.github/workflows/release.yml`, `dev/via-test.sh`, `README.md`, `RELEASING.md`; add `dev/compat-test.sh` |

Dependencies: Tasks 1–3 establish proof and stabilize the current branch. Tasks 4–6 make the plugin host faithful. Tasks 7–10 fill public behavior and events. Tasks 11–14 add the packet and internal foundations Observer requires. Tasks 15–17 certify combinations and make results a release gate. Public API slices, events and packet slices may proceed in parallel only after their shared interfaces are reviewed.

| Milestone | Exit gate | Main dependency / relative effort |
| --- | --- | --- |
| M0 — trustworthy baseline | Reproducible corpus, Paper oracle and green current branch | Tasks 1–3; medium |
| M1 — Paper host | Legacy and modern descriptors, libraries, bootstrap and lifecycle match Paper | Tasks 4–6; large |
| M2 — public behavior | Target API inventory and referenced event ledger have no unexplained gap | Tasks 7–10; very large |
| M3 — packet and internal layer | Unchanged PacketEvents runs over real channels and required CraftBukkit/NMS calls work | Tasks 11–13; very large, critical path for Observer |
| M4 — priority plugins | Observer and Zelda each pass their full Paper comparison | Tasks 14–15; depends on M1–M3 |
| M5 — market claim | Every dated corpus row passes and CI gates the exact release commit | Tasks 16–17; grows with the market |

These are relative sizes, not calendar promises. Task 1 establishes the denominator and the first honest effort estimate; M2 and M3 may require many small vertical slices, each independently reviewed and tested. The Paper host and public API slices can advance concurrently after M0, while Observer certification waits for the network and internal layer.

## Phase A — measurable baseline and repeatable oracle

### Task 1: Freeze the corpus and report honest coverage

**Files:** modify `dev/plugin_api_usage.py`; create `dev/plugin_compat.py`, `dev/test_plugin_compat.py`, `dev/compat/corpus.json`; test existing `dev/test_plugin_api_usage.py`.

**Interfaces:** `corpus.json` rows contain `name`, `version`, `variant`, `sha256`, `source`, `license`, `api_version`, `paper_build`, `dependencies`, `scenarios`. `plugin_compat.py` emits separate JSON statuses `linkage`, `load`, `events`, `behavior`, `coinstall`; only all-pass rows count as certified.

- [ ] Record the exact hashes of the Observer 2.69.3 JAR, the selected Zelda Civ 0.1.0 JAR and external dependency JARs. Inventory the original 59 artifacts when available; mark unavailable artifacts `not_tested` rather than carrying forward old results.
- [ ] Add scanner regressions for singleton-member gaps, class references, method handles, `invokedynamic`, `META-INF/services`, descriptor classes and reflective class names found in bytecode strings. Require a manual review entry for dynamically constructed reflection names.
- [ ] Implement manifest validation: unique artifact identity, existing local file matching SHA-256, declared dependency closure, source/license fields, and separation of product from build variant.
- [ ] Run `python -m unittest dev.test_plugin_api_usage dev.test_plugin_compat`; expect all tests green and no corpus JAR written to git. Run both real JAR scans; record Observer 30/393 and Zelda 241/1144 only if the current build reproduces them.

### Task 2: Establish a Paper behavioral oracle

**Files:** create `dev/paper-oracle-test.sh`, `dev/compat/compare.py`, `dev/test_compat_compare.py`, `dev/compat/scenarios/observer.json`, `dev/compat/scenarios/zelda.json`.

**Interfaces:** each scenario declares `plugin`, `server_version`, `setup`, `actions`, `observations`, `normalizers`; the runner writes a timestamped temporary `paper.json` or `foton.json` observation file and `compare.py` reports every field mismatch.

- [ ] Pin the exact Paper build and checksum for the Foton target version. Also pin the Paper 1.21.11 reference for older plugin assumptions. Never use `latest` in a passing certification report.
- [ ] Build disposable Paper and Foton worlds outside the repository/Desktop root, install identical unchanged plugin JARs and dependency sets, and capture startup order, logs, server state and protocol observations. Suppress only inherently nondeterministic fields (timestamps, generated UUIDs, random ports) with explicit normalizers.
- [ ] Compile a fixture with `javac --release 25`, load it on the Java 25 Foton runtime, and compare with Paper 26.2; run the separate 1.21.11 reference on Java 21. A class-version rejection fails the 26.2 baseline.
- [ ] Add a comparison regression where one server omits an event and another changes an item component; both must fail with a precise diff. Add a positive identical-fixture case.
- [ ] Run `python -m unittest dev.test_compat_compare` and a tiny fixture through `bash dev/paper-oracle-test.sh`; expect deterministic pass/fail and preserved failure logs in temp for diagnosis.

### Task 3: Stabilize and preserve the current compatibility batch

**Files:** the already modified Foton files shown by `git status --short`; no unrelated files.

**Interfaces:** the PDC, inventory attachment, permission and team packet semantics described by their current tests become baseline contracts for later work.

- [ ] Review the dirty diff by domain; preserve user work and split coherent commits after review. Recheck generated/extracted input provenance and the new dependency licenses.
- [ ] Under WSL with an isolated target, run `bash dev/ci.sh`, then the targeted PDC, scoreboard/permission and Java API checks. Fix any actual failing behavior before creating a baseline commit.
- [ ] Run `bash dev/install-test.sh`, `bash dev/release-test.sh`, `cargo fmt --all -- --check` and `git diff --check`. Record the exact commit and test results in the corpus manifest.

## Phase B — faithful Paper plugin host

### Task 4: Complete descriptor and dependency graph semantics

**Files:** modify `plugin-api/src/foton/PaperPluginDescriptor.java`, `plugin-api/src/foton/PluginHost.java`, `plugin-api/src/org/bukkit/plugin/PluginDescriptionFile.java`; create `plugin-api/src/foton/PluginDependencyGraph.java`, `plugin-api/check/PaperLoading.java`.

**Interfaces:** the graph resolves `bootstrap` and `server` phases separately; each edge carries `required`, `load` (`BEFORE`, `AFTER`, `OMIT`) and `joinClasspath`. Legacy `depend`, `softdepend`, `loadbefore`, `provides`, startup phase and `libraries` remain distinct from `paper-plugin.yml` semantics.

- [ ] Add fixture JARs with both descriptors, required/optional edges, a provider alias colliding with a real plugin name, a mixed legacy/Paper graph, `join-classpath: false`, and hard/optional cycles. Assert construction/enable order and reproduce the pinned Paper build's cycle-edge recovery or rejection; only unrecoverable graphs may fail before publication.
- [ ] Extract the graph from `PluginHost` into `PluginDependencyGraph`; parse and validate every descriptor field against the pinned Paper API and official descriptor documentation.
- [ ] Execute `bash dev/build-plugin-api.sh --check`. Inspect fixture logs for exact ordering and confirm a rejected graph leaves no plugin, class loader, command or permission registered.

### Task 5: Resolve isolated plugin libraries with real transitive dependencies

**Files:** modify `plugin-api/src/foton/PluginHost.java`, `plugin-api/src/org/bukkit/plugin/java/PluginClassLoader.java`, `plugin-api/src/io/papermc/paper/plugin/loader/PluginClasspathBuilder.java`, `plugin-api/src/io/papermc/paper/plugin/loader/library/LibraryStore.java`, `dev/fetch-plugin-api-libs.sh`, `dev/plugin-runtime-test.sh`, `plugin-api/lib/licenses/THIRD-PARTY-NOTICES.txt`; create `plugin-api/src/foton/PluginLibraryResolver.java`, `plugin-api/src/io/papermc/paper/plugin/loader/library/impl/MavenLibraryResolver.java` and focused loader fixtures.

**Interfaces:** `PluginLibraryResolver` accepts validated coordinates/repositories/local paths and returns ordered, verified `URL`s for one plugin loader. `MavenLibraryResolver` implements the exact Paper API signatures used by the corpus, backed by the real resolver dependency graph and repository policy.

- [ ] Make fixtures fail for a transitive dependency, a Maven version conflict, a corrupt cached JAR, path traversal, an unreachable repository and library visibility leakage between two plugins. Include `paper-libraries.json` consumed by a plugin's own `PluginLoader` (not by the Paper host), legacy `libraries`, `JarLibrary`, and a loader-added Maven library.
- [ ] Pin the Maven Resolver runtime closure, checksums and license notices. Resolve to a temp file, verify content and atomically publish it; apply time, size and repository bounds. Respect Paper's configured central mirror.
- [ ] Remove the current host-side regex-only `paper-libraries.json` handling: Paper does not process this generator-specific file automatically. Verify a plugin loader that parses it and adds a Maven resolver, then run Java API, runtime, installer and release packaging tests; compare startup with Paper.

### Task 6: Execute bootstrap, lifecycle, commands and registry phases

**Files:** modify `plugin-api/src/foton/PluginHost.java`, `plugin-api/src/foton/PaperPluginDescriptor.java`, `plugin-api/src/io/papermc/paper/plugin/bootstrap/`, `plugin-api/src/io/papermc/paper/plugin/lifecycle/`, `plugin-api/src/foton/CommandMap.java`; create `plugin-api/src/foton/PaperBootstrapRunner.java`, `plugin-api/check/PaperBootstrap.java`; modify `foton-core/src/` registry initialization only at its real bootstrap boundary.

**Interfaces:** for a Paper plugin, execute loader classpath, `PluginBootstrap.bootstrap`, relevant registry/command lifecycle callbacks, `createPlugin` or the Paper-defined default, `onLoad`, then `onEnable`. Any failure rolls back ownership in reverse order. Registry additions are typed, namespaced plugin registrations, not runtime parsing of vanilla JSON.

- [ ] Create a fixture whose bootstrapper adds a library and registers a Brigadier command, then constructs its `JavaPlugin` with `createPlugin`. A second fixture mutates a supported registry during bootstrap. Record expected order from a pinned Paper run.
- [ ] Add failure fixtures for bootstrap throw, `createPlugin` throw, missing library and dependency failure; assert atomic cleanup, no early `onEnable`, and no leaked loader or registration.
- [ ] Implement the phase runner and wire the native registry freeze point. Run `bash dev/build-plugin-api.sh --check`, a real Foton startup fixture, then the Paper comparison. Do not claim arbitrary plugin registry mutation until its Rust type and packet synchronization exist.
- [ ] Before enabling registry lifecycle callbacks, replace the disconnected Java enchantment queue and ownerless Rust pending queue with an owner-scoped, reversible typed transaction transferred across JNI before freeze. Snapshot builder values deeply; stage and validate complete bounded plugin batches without consuming or leaking them on failure, propagate rejection before `onLoad`/`onEnable`, and publish the registry only after all surviving batches are consistent. Preserve description and direct-entry-versus-tag sets in the same definition used for gameplay and the client registry packet. The current Paper builder has no effects field, so an effect-free custom enchantment is legitimate; any future accepted effect must have a typed gameplay/network codec rather than mapping Foton's existing effect booleans to vanilla's conditional-effect lists. Compare registration, save/reload, item lookup and registry packets with Paper/vanilla; reject unsupported entry types instead of accepting lossy registrations. Use the pinned Paper oracle to establish `registerWith`, `copyFrom`, freeze tags and inter-plugin conflict semantics before implementing those paths.
- [ ] Complete bootstrap dependency classpaths and cross-plugin lifecycle ordering against the pinned Paper graph. A standalone bootstrap fixture is evidence for that slice only, not for the full phase contract.

## Phase C — complete public behavior, state and events

### Task 7: Generate the full public API backlog and close it by capability

**Files:** modify `dev/plugin_api_usage.py`, `dev/plugin_compat.py`; add a generated, reviewed ledger under `dev/compat/` containing all public members, not just `SHARED_BY = 2`; modify focused files in `plugin-api/src/` and `foton-plugin/src/compat/` for each capability.

**Interfaces:** each member row is classified `implemented_and_tested`, `missing`, `wrong_behavior`, `not_reachable_on_target_version` or `requires_internal_adapter`, with its plugin audience, native owner, Paper reference test and proof. A field/method is `implemented_and_tested` only after runtime invocation and meaningful state assertion.

- [ ] Inventory all 5,973 current public references and refresh the upstream Paper API ABI for the target version. Add reflection/service discoveries and Observer/Zelda singleton references. Diff the published Foton JAR against Paper by owner/name/JVM descriptor, access and inheritance, including constants and exceptions.
- [ ] Prioritize independently reviewable vertical slices by dependent plugins and native foundation. Observer's historical count was 30; the pinned GitHub Zelda JAR yields 228 static linkage diagnostics on the frozen post-enchantment snapshot (14 kind, 52 class and 162 member occurrences), down from 238 after MiniMessage packaging. These are diagnostic occurrences, not a compatibility percentage or the older local JAR's 241-member observation. For each slice: write the failing plugin/behavior test, implement the actual Rust-backed or pure Java behavior, compare with Paper, then mark the row proven.
- [ ] Preserve Paper 1.21.11 binary entry points used by the pinned Zelda JAR, even where Paper 26.2 removed them (first observed: `BookMeta.pages(List<Component>)`; Zelda's same book construction also uses component title and author). Exercise each through the unchanged JAR and compare its state with the 1.21.11 oracle; a load-only shim or lossy conversion is not a pass.
  - Implemented slice (`f753316e0`, `3f2a95b0e`): legacy book descriptors, styled pages/cover, integer PDC, Java/Rust slot and NBT round trips; unchanged filtered/unresolved native books and empty styled names are preserved. Focused Java checks, seven Rust/component/JVM checks and strict plugin Clippy passed. This does not certify recipe output, `serializeAsBytes`, arbitrary `openBook(book)`, or the Zelda gameplay workflow.
- [ ] Keep the public-API backlog at zero `missing` and zero `wrong_behavior` for the target version before the public-surface milestone. Re-run `python dev/plugin_api_usage.py <corpus-dir> --gap plugin-api/build/foton-plugin-api.jar` after every slice.

### Task 8: Finish item, inventory, component and merchant semantics

**Files:** `plugin-api/src/org/bukkit/inventory/`, `plugin-api/src/org/bukkit/inventory/meta/`, `plugin-api/src/io/papermc/paper/datacomponent/`, `plugin-api/src/foton/FotonInventory.java`, `foton-plugin/src/compat/`, `foton-core/src/inventory/`, plus focused Java/Rust tests.

**Interfaces:** one item representation round-trips vanilla components, plugin PDC, metadata and unknown components across Java API, native slot bridge, save/load and packets. Inventory clicks, drag, close/reopen, cursor, crafting, smithing, brewing and merchant trades expose and apply the same transaction result as Paper.

- [ ] Add Zelda scenarios for a custom item with PDC and unknown components through storage/restart; a custom GUI close/reopen; smithing/trim; brewing; cursor mutation; and a merchant trade with villager experience. Make each fail independently when its component or event is dropped.
- [ ] Explicit user addition (2026-10-03): include Zelda armor end to end. Trace the pinned plugin's actual armor code first, then compare equip/unequip by every supported route, protection and attribute/effect changes, durability/enchantments, and its used trim/dye/custom appearance metadata against Paper. Verify equipment packets for the wearer and other players, death/drop/reconnect/restart persistence, and no duplicate or stale bonuses. Keep the unchanged plugin authoritative; no Zelda-side compatibility workaround.
- [ ] Implement missing `ItemMeta`, `DataComponents`, trim, `MerchantRecipe`, cursor and menu APIs from matching Paper source and `minecraft-src/`/FotonExtractor data. Extend the native slot format without truncating unknown data and without hardcoded registry IDs.
- [ ] Carry recipe result components/PDC through the typed `ItemStackTemplate`, generated vanilla recipe data, plugin add/get/list/remove operations, crafting output and inventory persistence. Zelda's `LivreCuisine` registers a component-rich written book and uses `removeRecipe` then `addRecipe` to replace it; a material/count-only result or permanently disabled key loses the book on craft/reload. Existing extracted vanilla results with components must also survive generation.
  - Native generated result components are implemented and registry-tested (`44c0fe1cd`, `08d024ddf`). The Java/native recipe bridge, key replacement and runtime recipe ownership remain open; native data generation alone does not certify the crafted Zelda book.
- [ ] Run focused Java checks, native round-trip tests, `bash dev/all-tests.sh` inventory cases, then the Zelda Paper differential scenarios. Require a restart assertion for persisted changes.

### Task 9: Finish entity, world, display, geometry and scheduler semantics

**Files:** `plugin-api/src/org/bukkit/entity/`, `plugin-api/src/org/bukkit/block/`, `plugin-api/src/org/bukkit/World.java`, `plugin-api/src/foton/FotonEntity.java`, `FotonPlayer.java`, `FotonWorld.java`, `foton-core/src/entity/`, `foton-core/src/world/`, `foton-plugin/src/compat/`, plus focused tests.

**Interfaces:** Java handles preserve entity identity; live pose, fluids, vehicle, movement, attributes, active item, collision, bounding boxes, ray tracing, display transforms, particles and TPS read real state. Mutations update both server state and the correct outgoing packets. Async scheduler tasks cannot starve unrelated plugins and teardown cancels owned work.

- [ ] Write real-world fixtures for Observer legitimate movement near water, climbable blocks and collisions, and Zelda displays, mounts, particles and region interactions. Compare getter values and emitted packets with Paper at equivalent ticks.
- [ ] Implement missing player/entity state and block geometry through Foton core, using native world collision shapes and vanilla logic; do not infer them from Java constants. Make display entities persist, track and render through protocol packets.
- [ ] Test world unload cancellation/result, TPS measurement and scheduler non-starvation with two plugins. Run entity/world unit tests, `dev/join-test.sh`, relevant `dev/all-tests.sh` cases and Paper comparisons.

### Task 10: Close the event delivery ledger end to end

**Files:** `foton-core/src/event/`, the actual game action sites under `foton-core/src/`, `foton-plugin/src/forward.rs`, `plugin-api/src/foton/EventBridge.java`, `plugin-api/src/org/bukkit/event/`, `plugin-api/check/Events.java`; add `dev/compat/event-ledger.json` and an event-ledger checker.

**Interfaces:** every referenced event row identifies its native cause, pre/post-action timing, thread, Java type, fields, cancellation, mutations and write-back. The checker rejects a row marked delivered without both a native emission and a Java bridge path.

- [ ] Recount the previous 199/113/86 observation from the current corpus and code. For Observer, start with fail move, armor change, velocity, flight toggle, resurrect hand and placement. For Zelda, include block drops, brewing, crafting, entity load/unload/combust/dismount/place, item consume, hand swap, smithing, enchanting, server ping and vehicle move.
  - The unchanged Zelda `VanillaSuppressor` listener also links `BrewEvent` and `EntitiesLoadEvent`. Core brewing completion is now cancellable with mutable results (`a60cd6692`, 12 focused tests), but its Java event must wait for the real `Container`/`BrewingStand` hierarchy, snapshot/update semantics and lock predicate enforcement. Do not expose a stub holder merely to make listener reflection succeed.
    - The bounded dependency sequence and concrete reference checks are in [`dev/compat/brewing-holder-plan.md`](dev/compat/brewing-holder-plan.md): reusable predicates/locks, authoritative native snapshots and persisted tile PDC, complete holder ABI, event bridge, then unchanged-plugin acceptance. Mutable recipe duration and detached-state behavior are required, not constant/no-op placeholders.
  - Entity load/unload emission must run at the correct synchronous world boundary after all loaded entities are registered, including an empty batch. The current chunk-loading path performs asynchronous work; emitting Bukkit callbacks directly there is not an acceptable shortcut.
- [ ] For each event, write a Paper trace and an in-world Foton test asserting exact firing count/order, fields, cancellation and mutation propagation. Connect missing core emitters and `forward.rs` subscribers; `PrepareGrindstoneEvent` is an initial case because its core and Java endpoints already exist.
- [ ] Include Zelda's beehive workflow: `CreatureSpawnEvent(BEEHIVE)` fires before bee insertion, cancellation preserves the occupant and nectar, and honey/sounds change only after successful insertion. The Paper 1.21.11 oracle also emits `EntityChangeBlockEvent` with the new honey-level `BlockData`; Foton's current block-data bridge loses properties, so treating the spawn event alone as complete beehive parity is forbidden.
  - Implemented slice (`fccdcb8be`, `1b68a5572`): BEEHIVE constant and native emission, pending typed `Bee` lookup, corrected JNI descriptor, and an actual Paper-compiled listener in `dev/beehive-plugin-test.sh`. The listener can cancel release while the occupant and honey level remain unchanged, then allow it and observe increased honey. Still open: entity spawn provenance, cancellable honey block-change event, and `Bee.hasNectar()` returning false for the test's synthetic occupant NBT.
  - Audit every JNI callback descriptor against the compiled Java class. The beehive integration exposed a four-double descriptor for a three-double method; swallowed lookup failures can otherwise masquerade as delivered events.
    - Implemented signature gate (`73edfe418`): CI compares Rust callback descriptors, argument types and compiled Java targets; seven regression tests include malformed and unrecognized calls. PreCreatureSpawn, EntityPortal and BlockExp were repaired. Signature validity does not prove event timing or mutation semantics.
- [ ] Fix reflection-based listener registration to match Paper visibility/inheritance rules with private, protected, overridden and bridge-method fixtures. Require zero silent referenced event rows before market certification, with truly unavailable version-specific events classified by an exact version rule.

## Phase D — packets, PacketEvents and internals

### Task 11: Make the Foton network channel a real Paper-compatible Netty pipeline

**Files:** `foton/src/lib.rs`, `foton-plugin/src/via.rs`, `plugin-api/src/foton/network/FotonViaChannel.java`; create `foton-plugin/src/packet_pipeline.rs` and network scenario fixtures.

**Interfaces:** a channel exposes ordered inbound/outbound handlers around framing, decompression, decryption, packet codec, translation, compression and encryption; every buffer has one owner and a defined release rule. Login/configuration/play state changes and disconnect remove handlers exactly once.

- [ ] Capture Paper pipeline snapshots at initial channel, compression/encryption enable, configuration, play and close. Build Foton tests that fail on wrong handler order, a duplicate handler, a packet mutated after release and lost packets during a state transition.
- [ ] Move the existing Via-specific channel adaptation behind a reusable pipeline interface. Keep `dev/via-test.sh` green for 1.21.11 and native 26.2 clients.
- [ ] Add PacketEvents-shaped inbound/outbound handler fixtures with cancellation, replacement, bundle packets and concurrent disconnect. Run network unit tests and real client joins before exposing the pipeline to plugins.

### Task 12: Build the versioned CraftBukkit/Mojang internal adapter

**Files:** `plugin-api/src/net/minecraft/`, new `plugin-api/src/org/bukkit/craftbukkit/`, `plugin-api/src/foton/internal/`, `foton-plugin/src/compat/internals/`, `dev/compat/internal-abi.json`, internal adapter tests. The Paper build and `minecraft-src/` are references, not copied server runtime payloads.

**Interfaces:** an internal Java wrapper and its public Bukkit handle refer to one Foton object; reads/writes cross a typed JNI boundary. ABI is keyed to the exact target Minecraft/Paper version and mapping namespace. Reflection names, constructors, method descriptors and casts used by the pinned corpus are tested against the Paper oracle.

- [ ] From all 18 corpus plugins that reach internals, extract classes, members, reflection strings, `instanceof`/casts and mapping namespace. Start with PacketEvents 2.13.0's `SpigotReflectionUtil.init()` dependency chain, then other high-audience internals. Record every exact ABI demand and its Foton state owner.
- [ ] Add failing `Class.forName`, method-handle, reflective method/field lookup and cast fixtures for the PacketEvents chain. Implement the matching CraftBukkit/NMS wrappers with real native behavior; preserve object identity and thread affinity.
- [ ] Add versioned startup remapping where a JAR is in the supported older mapping namespace; compare the transformed linkage with Paper. Unknown versions or ABI demands produce a precise unsupported-internal diagnostic.
- [ ] Run the unchanged internal-using JARs. No internal row is complete until its feature scenario works, including packet read/write where applicable. Repeat the inventory on every Foton Minecraft target bump.

### Task 13: Activate unchanged PacketEvents 2.13.0 on Foton

**Files:** adapter and pipeline files from Tasks 11–12, `plugin-api/src/foton/PluginHost.java`, `dev/compat/scenarios/packetevents.json`, `dev/compat-test.sh`.

**Interfaces:** the unchanged `packetevents-spigot-2.13.0.jar` is discovered under its declared plugin name, enabled before Observer, initializes its reflection layer, injects every relevant channel and exposes its normal public PacketEvents API to Observer's loader.

- [ ] Write a real server test that installs the official checksum-pinned JAR, boots it without an Observer JAR and asserts initialized API, channel injection, inbound/outbound callbacks and clean shutdown. Repeat with ViaVersion/ViaBackwards and verify handler ordering both ways.
- [ ] Close each actual linkage/reflection/packet failure through Tasks 11–12 with a focused regression. Do not patch or repackage PacketEvents to turn the test green.
- [ ] Add stress cases: compression, encryption, login refusal, rapid reconnect, decoder exception, packet cancellation/replacement and disconnect during a callback. Require zero Netty refcount or double-injection errors.

### Task 14: Certify Observer on the complete detection path

**Files:** Observer scenario fixtures under `dev/compat/scenarios/`, plus the actual Foton APIs/core/packet modules implicated by failing cases. Do not change the Observer source/JAR.

**Interfaces:** `Observer-2.69.3.jar` and PacketEvents 2.13.0 are exact unchanged inputs. Detection evidence and enforcement are compared with the matching Paper oracle for the same traffic. Explicitly record Observer's shutdown-on-missing-PacketEvents behavior.

- [ ] Reach zero of Observer's 30 remaining public ABI gaps and pass its startup with PacketEvents. Test missing dependency in a disposable server with download/network effects isolated and recorded.
- [ ] Replay clean movement, sprint, climb, fluids, vehicles, elytra, combat, inventory and protocol transitions; compare violation output and ensure no false positive. Replay controlled cheat-shaped packets and assert PacketEvents callbacks and Observer actions match Paper.
- [ ] Restart and co-install with ViaVersion/ViaBackwards. Assert listener count, classloader cleanup, no duplicate Netty handlers and stable client connection. Mark Observer certified only after every declared scenario passes.

## Phase E — Zelda, ecosystem and perpetual release gate

### Task 15: Certify Zelda Civ gameplay and optional integrations

**Files:** `dev/compat/scenarios/zelda.json`, focused API/core changes identified by Tasks 7–10, `dev/compat-test.sh`.

**Interfaces:** the selected unmodified Zelda JAR has zero missing referenced public members; its behavior is checked with optional integrations absent and with each pinned companion plugin present.

- [ ] Drive the pinned Zelda JAR's current linkage diagnostics to zero with reviewed capability slices; also cover reflective and runtime calls that static scanning misses. Run the same Zelda workflow on Paper and Foton: start game, team/scoreboard updates, map/region interactions, custom items and inventories, recipes/merchants, displays/mounts and restart persistence.
- [ ] Certify the pinned Zelda armor workflow explicitly, including actual worn effects/protection and visible equipment, removal/replacement/death and reconnect behavior, and durable item metadata. Run native 26.2 and Via-translated 1.21.11 client observations; a stored armor ItemStack or matching equipment getter alone does not pass this scenario.
- [ ] Verify an unsupported optional integration degrades exactly as on Paper; then add its actual companion JAR and exercise the integration. Preserve current dirty Zelda checkout files.
- [ ] Include the exact Simple Voice Chat 2.6.20 companion in the startup/co-installation gate: the pinned Zelda JAR eagerly links `VoicechatPlugin` despite declaring `softdepend`, and Paper 1.21.11 fails to load Zelda without the companion. Foton must support the real companion and its Bukkit/CraftBukkit-facing adapter, not a fake voice-chat class or version string.
- [ ] For Voice Chat's versioned compatibility path, expose Foton's product version separately from the truthful Bukkit/Minecraft API version; implement the server handle, player messaging-channel bridge, player handle and downstream command/chat hooks through Foton's real networking/scheduler state. Boot with the unmodified duo, then connect two 1.21.11 clients and verify voice handshake, UDP authentication, Zelda microphone/distance callbacks, audio and reconnect against Paper. Startup or Voice Chat's reduced Bukkit fallback alone does not pass this gate.
  - Truthful Bukkit version reporting is implemented (`1b024391d`). Live plugin-channel registration/removal, send filtering and raw custom-payload encoding are covered by an isolated real-client/Paper-compiled-plugin test (`4f4b18e3c`, `34c591157`). VoiceChat itself still fails at the server-handle adapter; these foundations are not a voice handshake or audio pass.
- [ ] Certify each distributed JAR variant separately for linkage and one full primary variant for behavior; do not count three build outputs as three different products.

### Task 16: Expand to the market corpus and co-installation matrix

**Files:** `dev/compat/corpus.json`, `dev/compat/scenarios/`, `dev/plugin_compat.py`, `dev/compat-test.sh`, independent scenario tests.

**Interfaces:** a corpus snapshot is immutable by hash; each entry has an individual result and required pairwise/group combinations. A new plugin version is a new row and starts uncertified.

- [ ] Restore or reacquire the original 59 JARs lawfully, record hashes, and add representative current plugins in the categories listed by the spec. Classify all public, internal, service-loader and reflective demands.
- [ ] Run every individual plugin and its declared dependency closure on Paper and Foton. Add co-installation pairs sharing permissions, economy, packet hooks, classpaths, commands or events, plus the Observer/PacketEvents/Via and Zelda/optional-dependency groups.
- [ ] Fix failures by vertical capability slice. The market milestone passes when all supported artifact rows have `linkage=pass`, `load=pass`, `events=pass`, `behavior=pass`, `coinstall=pass`; publish the exact denominator and never silently exclude a failing row.

### Task 17: Enforce certification for every target upgrade and release

**Files:** `dev/ci.sh`, `.github/workflows/test.yml`, `.github/workflows/release.yml`, `dev/compat-test.sh`, `README.md`, `RELEASING.md`, `AUDITING.md`, `dev/compat/corpus.json`.

**Interfaces:** the release job consumes an exact Foton commit, target Minecraft/Paper build, API JAR, corpus snapshot and plugin/dependency hashes. It emits a machine-readable report and refuses a release compatibility claim if a required row regresses.

- [ ] Add fast fixture/linkage gates on every PR, plus full real-client, real-plugin and Paper differential jobs for release and scheduled runs. `dev/via-test.sh` stays a separate mandatory network regression. Fail closed if an expected corpus input or oracle build is unavailable.
- [ ] Pin Java 25 for the Paper 26.2 CI jobs and keep Java 21 only in the historical Paper 1.21.11 oracle job; verify the distributed Foton runtime accepts Java 25 compiled plugins.
- [ ] Add a report with five status axes per plugin and a list of unsupported target versions/internal demands. Update README with only claims backed by green reports; include runtime library/license changes in release packaging tests.
- [ ] On a `+mc` bump, refresh `minecraft-src/`, FotonExtractor data, the pinned Paper oracle, public ABI, internal ABI, event ledger and plugin corpus before restoring a green release gate.

## End-to-end completion criteria

- [ ] `bash dev/ci.sh`, `bash dev/all-tests.sh`, `bash dev/join-test.sh`, `bash dev/via-test.sh` and `bash dev/compat-test.sh` pass for the exact release commit in CI/WSL.
- [ ] The target-version Paper public API has no unexplained missing or behaviorally incorrect members in the certification inventory; referenced event rows are emitted and their mutations applied.
- [ ] Observer, PacketEvents, Zelda Civ and all supported market corpus rows pass the complete per-plugin and co-installation contract, with the denominator and versions stated explicitly.
- [ ] No official unchanged JAR needed by a claimed passing row was edited, shaded into Foton without license review, or substituted with a test double for the final run.
- [ ] A future Paper/Minecraft/plugin release causes a new certification run rather than silently inheriting “100% compatible.”

## Plan self-review

The specification's descriptor/bootstrap, API/data, event, packet, internals, two priority plugins, market corpus, versioning, legal packaging and release requirements map respectively to Tasks 4–6, 7–9, 10, 11–13, 12, 14–15, 16, 1/17, 5/13 and 17. The two currently uncommitted implementation batches are explicitly preserved in Task 3. Unknown exact ABI details are produced by the pinned artifacts and inventories in Tasks 1, 7 and 12; they are not guessed in advance.
