# SDD ledger — plan: docs/superpowers/plans/2026-09-15-popular-plugin-compatibility-wave-2.md

- Plan base: `8a0f97d5793b127d3981e6c14902bf267ed674cc`
- Foton functional baseline: `75f78b09351b3aa622dffd311831c7d61b566d38`
- Zelda baseline: 392 compiler errors at `de3d12ac00a04f077b68c7537b36c689ac3679a2`, measured with a gate that rejects every residual `paper-api-*.jar` from the classpath.
- Post-Task-5 runtime evidence: Zelda `ae06d49cb093a977ce2192bb31426a118d342c80` now passes SQLite acquisition on Foton `abb708f06`; the next Foton-owned gate is missing `CreatureSpawnEvent.SpawnReason.BEEHIVE`. The host artifact is `sqlite-jdbc-3.49.1.0.jar`, SHA-256 `5c8609d2ca341deb8c6f71778974b5ba4995c7d32d7c7c89d9392a3e72c39291`.

## Rulings

- No fake Simple Voice Chat classes; Zelda's provided dependency must be supplied by the runtime environment or isolated by Zelda.
- Do not chase the nine shared-member residuals in this wave: none overlap Zelda and each requires missing architecture/data.
- `Effect.getId()` remains deferred until both authoritative mapping and functional effect routing exist.
- New entity APIs must delegate existing core state and add no tick-path work.
- Modern names must alias existing keyed instances by identity.
- Minecraft source generation is not run in this wave; only make its command target-safe and reproducible.
- The former 293-error measurement is invalid because `-Dmdep.excludeArtifactIds=paper-api` was ignored by maven-dependency-plugin 3.7.0. Use `-DexcludeArtifactIds=paper-api` and never compare progress against the contaminated baseline.
- JDBC ruling: match Paper by pinning SQLite JDBC on Foton's verified host runtime and explicitly initialize host-visible `java.sql.Driver` providers before plugin construction. Do not switch the global/thread context classloader to a plugin loader; that is one-shot, order-dependent, and leaks plugin classloaders through DriverManager.
- Lifecycle ruling: if a plugin disables itself during `onEnable`, Foton must not emit PluginEnableEvent or transition it back to ENABLED.
- Task 6 metric ruling: its original hierarchy/game-rule/tag/enchantment scope remains an expected 11-error Zelda compile reduction (about 330 to about 319). `BEEHIVE` is a later runtime linkage/behavior gate and is tracked separately.
- Spawn provenance ruling: preserve Vanilla `EntitySpawnReason` and store a separate typed 47-value `PluginSpawnReason` beside it in one provenance snapshot under the existing lock. Unambiguous mappings include `ChunkGeneration -> CHUNK_GEN` and `TrialSpawner -> TRIAL_SPAWNER`; ambiguous server causes become `DEFAULT`, never `CUSTOM`. `CUSTOM` is plugin-originated only.
- Beehive ruling: match Paper's pre-insertion `CreatureSpawnEvent(BEEHIVE)`. Cancellation keeps the occupant and suppresses insertion, exit sound/game event, honey mutation, and released-bee side effects. Successful event/query provenance remains `BEEHIVE`. This adds no steady tick work.

## Tasks

- [x] Task 1 — fresh-checkout/path-safe build (`4d081c59f`, `0e1cdfd74`, `1e77aff1d`; spec and quality approved)
- [x] Task 2 — entity truth bridge (`7bf2c0835`, `b0165146a`, `6fe94f1b1`; spec and quality approved)
- [x] Task 3 — exact adapters (`5f7467ac9` through `49c66a52b`; spec and quality approved)
- [x] Task 4 — modern aliases (`8eff093d9` through `634280adf`; spec and quality approved)
- [x] Task 5 — host JDBC runtime/self-disable lifecycle (`d7fdc5609`, `abb708f06`; spec and quality approved; Zelda SQLite runtime gate complete)
- [ ] Task 6 — additional live-backed API (original 11-error compile scope plus separate 47-value spawn-reason/BEEHIVE runtime gate)
- [ ] Task 7 — Zelda evidence and final verification

## Task 3 preflight

| Scope pair | Producer / consumer | Finding |
|---|---|---|
| Task 2 → Task 3 | Existing entity/inventory truth bridge → equipment adapters | Compatible; adapters must preserve live backing and exact slot mapping. |
| Task 3 → Task 4 | Exact adapters → modern aliases | No shared implementation required by Task 3; keep keyed identity semantics unchanged for Task 4. |
| Task 3 internal | Compile checks → every listed adapter | Consistent; each API addition needs a red compile check before implementation. |
| Task 3 internal | Behavior checks → equipment, ingredient, registry/iterator, world/profile/key contracts | Consistent; assertions must exercise existing live collections/state, destination identity, Iterator semantics, and Adventure Key compatibility. |

- Task 3 base: `6fe94f1b1b95b7f5dd4727ce815fe8f761ea561b`
- Task 3 status: implementing

## Task 6 preflight

| Scope pair | Producer / consumer | Finding |
|---|---|---|
| Task 5 → Task 6 runtime | Host SQLite availability → Zelda `onEnable` | SQLite now passes; missing `SpawnReason.BEEHIVE` is the next independent runtime gate. |
| Task 6 compile scope | Hierarchy, game rules, item tag, enchantment conflicts → Zelda compile gate | Preserve the original expected 11-error reduction; do not add the runtime enum gate to this metric. |
| Core provenance → plugin bridge | Vanilla `EntitySpawnReason` plus typed `PluginSpawnReason` → events/entity query | Keep both values in the existing lock; serialize only at JNI, with `DEFAULT` rather than server-originated `CUSTOM`. |
| Beehive release → event/insertion | Existing release attempt → `CreatureSpawnEvent(BEEHIVE)` | Fire before insertion; cancellation retains the occupant and all pre-success side effects remain suppressed. |

- Paper 26.2 enum oracle: <https://github.com/PaperMC/Paper/blob/ver/26.2/paper-api/src/main/java/org/bukkit/event/entity/CreatureSpawnEvent.java>
- Paper 26.2 beehive ordering oracle: <https://github.com/PaperMC/Paper/blob/ver/26.2/paper-server/patches/sources/net/minecraft/world/level/block/entity/BeehiveBlockEntity.java.patch>

## Task 6 SpawnReason/BEEHIVE implementation

- Scope base: `936a4fa18469641c42c411adf61924d642c43474`; only spawn provenance, event forwarding, beehive release, and their checks.
- RED: Java exact ABI returned 9 instead of 47 constants; absent/unknown parser returned CUSTOM; Paper-compiled consumer failed with NoSuchFieldError BEEHIVE.
- RED: both real-server beehive tests observed zero CreatureSpawnEvent callbacks instead of one; new core/native provenance APIs failed compilation before implementation.
- Ruling: correct both existing spawn JNI descriptors (three doubles, not four) and resolve pre-spawn entity names through the existing keyed registry — the real JNI cancellation test proved both paths were silently bypassed; without this, typed spawn events would still not reach plugins.
- Status: implementation and focused verification complete; fix round 1 closes the live inserted bee's `getEntitySpawnReason()` Java/JNI gate. Unrelated Task 6 API remains pending.
- [x] Task 6 SpawnReason fix round 1 — extend the isolated JVM fixture with a real server and the existing isolated world fixture; release one bee, obtain it from the world, and assert Java `Entity#getEntitySpawnReason()` returns `BEEHIVE` for that exact UUID.
- [x] Live Java/JNI provenance gate — temporary native-query mutation produced `AssertionError: Inserted bee: expected BEEHIVE, got DEFAULT`; mutation restored via `apply_patch`, then the live test passed. Production behavior is unchanged.
- Fix-round verification: live JNI gate 1 passed; plugin unit suite 30 passed (live gate separately invoked); core beehive suite 2 passed; spawn suite 3 passed; formatting and diff whitespace checks passed. Full evidence: `task-6-spawnreason-fix-report.md`.

## Task 6 SpawnReason quality fix round 2

- Base: `6a110a89a6bbdce0524fa4636ac2a10f6f2ddaff`.
- Finding: plugin-visible provenance is not serialized, so a persisted BEEHIVE entity reloads as DEFAULT.
- Ruling: use Paper's exact `Paper.SpawnReason` persisted representation; recognized values restore into the existing provenance snapshot/lock, while absent, malformed, or unknown values restore DEFAULT and never CUSTOM. Old saves remain readable without a migration framework.
- Status: complete. `Paper.SpawnReason` now round-trips through the existing entity NBT payload and provenance lock; absent, unknown, and malformed values restore `DEFAULT` without changing Vanilla provenance.
- Verification: focused round-trip 3 passed; entity persistence 16 passed; core provenance 3 passed; beehive 2 passed; plugin unit suite 30 passed plus the isolated live Java/JNI spawn bridge; JNI integration 1 passed; touched-crate check, formatting, and diff checks passed. Full evidence: `task-6-spawnreason-fix-round-2-report.md`.

## Task 6 remaining live-backed APIs

- Base: `097eb4554183e4030dd3fdc8bedcf591ee4a147b`; tracked checkout verified clean.
- Task 5: approved (unchanged); Task 6 SpawnReason/BEEHIVE: approved through this base (unchanged).
- Scope: player hierarchy, four live game-rule aliases, live trimmable-armor tag, generated enchantment conflict API and request-time JNI bridge only.
- Status: implementation `638b70df5` and verification report `a547386a9` complete; all requested RED/GREEN evidence is recorded in `task-6-live-backed-report.md`. The independent review launched afterward was interrupted before a verdict.
- Verification: plugin API build passed with 826 Java sources; real JVM/JNI test 1/1; plugin 32 unit + 1 integration; registry 474 unit + 1 doctest; 508/508 native declarations registered; touched-crate checks, formatting, and diff checks passed.
- Baseline: local Minecraft source commit `e31ff098` targets 26.2, matching workspace `0.15.2+mc26.2`.

| Scope pair | Producer / consumer | Finding |
|---|---|---|
| Task 5 → remaining Task 6 | Existing live players and Java host → corrected interface hierarchy | Preserve implementations/state; compile-only implementation consequences permitted. |
| Approved Task 6 spawn → remaining Task 6 | Shared Native declarations/registration → one additional enchantment binding | Keep spawn paths unchanged and validate all native descriptors. |
| Remaining Task 6 internal | Focused failing API/JNI tests → minimal live adapters | Tests must use extracted tags and registry exclusivity, not copied data. |
| Remaining Task 6 → Task 7 | Verified committed adapters → later Zelda/full-wave evidence | Do not claim the expected 11-error reduction as measured in this scoped task. |

## Task 6 reviewed registry-publication defects

- Base: `6db5628030917c6432d8e29b41f9b312fe86afe9`.
- Scope: add non-initializing registry observation for JNI calls made during
  legal plugin `onLoad`; isolate both real-JNI lifecycle tests and run them
  explicitly after the plugin API build in CI.
- TDD invariant: the pre-publication generated-Java/JNI regression must fail
  against the base implementation before production edits.

| Scope pair | Producer / consumer | Finding |
|---|---|---|
| Plugin `onLoad` → registry JNI | Legal pre-publication API calls → conflict/can-enchant/tag natives | Return false/empty without panic, abort, blocking, or implicit registry initialization. |
| Registry publication → existing live JNI | Published singleton → request-time tag/enchantment queries | Preserve current live-backed behavior after publication. |
| Plugin API build → isolated JNI tests | Built jar/JDK → real generated-Java coverage | Keep lightweight cargo tests earlier; explicitly run both isolated JNI tests after the API build. |

## Task 6 live-backed quality fix round 1

- Base: `6db5628030917c6432d8e29b41f9b312fe86afe9`.
- Scope: make the four registry-backed JNI paths safe before `REGISTRY` publication, and isolate/schedule the post-publication JVM gate after plugin API construction in CI.
- Ruling: preserve the pre-task untracked root `build/` outside the worktree while this fix runs; clean only a new root `build/` created by this task, then restore the prior artifact unchanged.
- Status: implementation complete; the isolated RED aborted before publication and GREEN returned the documented defaults without publishing the registry.

| Scope pair | Producer / consumer | Finding |
|---|---|---|
| `RegistryLock` → plugin native helpers | Fallible publication state → false/empty JNI fallbacks | Compatible; accessor must not initialize or panic and post-publication deref semantics remain unchanged. |
| Pre-publication JVM fixture → global registry | Fresh isolated process → generated Java calls | Must run before any fixture publishes `REGISTRY`; ordinary in-process ordering is not sufficient. |
| Plugin API build → live-registry JVM gate | Built JAR → explicit ignored test | CI must build/check the API first, then invoke the ignored gate explicitly. |
| Quality fix internal | RED regression → minimal production change | Consistent; no production change before an observed failing test. |
