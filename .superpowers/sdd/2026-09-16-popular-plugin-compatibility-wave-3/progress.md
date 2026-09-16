# SDD ledger — plan: docs/superpowers/plans/2026-09-16-popular-plugin-compatibility-wave-3.md

- Documentation base: `5fa4936d6bc58c114d8bcee0e6c5d6520a8cf55a`.
- Foton implementation base: `5fa4936d6bc58c114d8bcee0e6c5d6520a8cf55a`.
- Zelda gate base: `3d7dc062b4353bced69f8383f52c961a5d42e17f`.
- Compile baseline: 41 errors on an 86-entry classpath containing zero `paper-api` jars; delta from valid Wave 2 baseline is `-351` / `-89.54%`.
- Runtime baseline: SQLite opens; `zones.db` and `players.db` pass `PRAGMA integrity_check`; `SpawnReason.BEEHIVE` passes; first Foton-owned blocker is missing `org.bukkit.event.inventory.BrewEvent` during listener reflection.
- Design: `docs/superpowers/specs/2026-09-16-popular-plugin-compatibility-wave-3-design.md`.

## Rulings

- The 41 measured Zelda diagnostics are the compile-scope oracle. Javac cascades are remeasured after every checkpoint; newly exposed members require evidence before implementation.
- Fix entity wrapper identity and generated class-to-key spawning before ray/entity/event work.
- `BrewEvent` and `PrepareItemEnchantEvent` ship together because one Zelda listener class declares both parameter types.
- All events are connected at their exact native timing boundary; declarations without dispatch/mutation/cancellation are rejected.
- Java state is either an immutable event snapshot or a façade over native state. No persistent Java-only entity, inventory, component, registry, or block-data cache.
- Locks are released before JNI/event callbacks.
- Block rays delegate `World::clip`; entity rays use the loaded-entity spatial query and nearest live AABB. No chunk-loading claim.
- Display state exposed as durable API must use normal entity persistence; no fake defaults/no-op setters.
- Chest-boat holder identity is carried as an entity UUID. Refill/player-history API is omitted because backing state does not exist; plugins calling omitted methods remain ABI-incompatible.
- Banner, instrument, and age data come from existing registries/components/extracted assets via generators. Extracted JSON and generated source are never edited directly.
- `MusicInstrument.create(...)` is omitted because dynamic plugin registry registration does not exist; plugins calling it remain ABI-incompatible.
- No new idle/tick work. Every dispatch/query is charged to an existing operation or plugin request.
- Final success means zero Zelda compile residuals, measured runtime evidence, green `bash dev/ci.sh`, whole-branch approval, and a clean repository. It does not mean 100% Paper/plugin-market compatibility.

## Dependency review

| Producer | Consumer | Contract |
|---|---|---|
| Task 1 | Tasks 3, 4, 6 | Single-pass `wrapEntity(UUID, type)` and generated class-to-`EntityType` mapping preserve real wrapper identity. |
| Task 2 | Task 8 runtime | Both parameter types in `VanillaSuppressor` resolve, and live brew/enchant behavior is connected before runtime requalification. |
| Task 3 | Zelda rune compile/runtime | Ray results expose exact hit data; entity predicates receive typed wrappers from Task 1. |
| Task 4 | Tasks 6 and 8 | Display/Allay/chest-boat wrappers and inventory-holder UUIDs are live and persistent where claimed. |
| Task 5 | Zelda item/crop paths | Components and age data round-trip through existing native registries/codecs; no copied data. |
| Task 6 | Task 8 lifecycle runtime | Load/unload/dismount/place events fire while affected entities remain resolvable and before irreversible effects. |
| Task 7 | Task 8 final compile/runtime | Drop, armor, and smithing mutations/cancellation are committed at existing operation boundaries. |
| Tasks 1–7 | Task 8 | Every residual family is committed, reviewed, and independently green before full CI/evidence. |

## Tasks

- [ ] Task 1 — entity wrapper identity and class-to-key spawning (in review; implementation `ba7cd62e0`, first review fix `2febe9169`, second review fix `4656276e6`, third review fix `c409748ca`, fourth review fix `ad27d7bc0`; independent re-reviews pending)
- [ ] Task 2 — live `BrewEvent` and `PrepareItemEnchantEvent`
- [ ] Task 3 — exact on-demand ray tracing
- [ ] Task 4 — live `ItemDisplay` / `TextDisplay` / `Allay` / `ChestBoat` identity and holder plumbing
- [ ] Task 5 — `BannerPatternLayers` / `MusicInstrument` + meta / generated `Ageable`
- [ ] Task 6 — `EntitiesLoadEvent` / `EntitiesUnloadEvent` / `EntityDismountEvent` / `EntityPlaceEvent`
- [ ] Task 7 — `BlockDropItemEvent` / `PlayerArmorChangeEvent` / `PrepareSmithingEvent` / `SmithItemEvent`
- [ ] Task 8 — exact Zelda rebaseline, runtime, `dev/ci.sh`, and whole-branch review

## Residual accounting

| Family | Baseline | Owning task | Required closing evidence |
|---|---:|---:|---|
| Ray tracing | 7 | 3 | All `FluidCollisionMode` / `RayTraceResult` diagnostics absent. |
| Entities/displays | 11 | 4 | All four entity symbols compile and live wrapper fixture passes. |
| Events/lifecycle | 19 | 2, 6, 7 | All ten event classes compile and operation-timing tests pass. |
| Inventory/meta/data | 3 | 5 | Banner/instrument/meta native round-trips pass. |
| Block data | 1 | 5 | Generated material-specific `Ageable` behavior passes. |
| **Total** | **41** | **2–7** | **Final Zelda compile count: 0.** |

Task 1 has no direct residual count; it is a correctness prerequisite. Task-level javac deltas may differ because missing types currently mask later member diagnostics. Only Task 8 publishes the final exact count.

## Task 1 implementation and review fix

- Base: `79d1250a32ec651706c7934b663dfb404eb21329`; worktree and branch preflight were clean and exact.
- Implementation: `ba7cd62e0` (`fix(plugin): preserve entity wrapper identity`).
- Review fix: `2febe9169` (`fix(plugin): validate entity class metadata`).
- RED: the wrapper check reached the Java harness and failed on the existing second `Native.entityType("block_display")` lookup; the class mapping check separately failed compilation because `FotonEntityFactory.typeFor(Class)` was absent.
- Review RED: canonical Paper classes failed at `TNTPrimed -> null`; generated output contained source-less PascalCase guesses; a planted stale `vanilla_entities.rs` bypassed Cargo; and the real-JNI living fallback failed when `uuid.toString()` was mutated to `type`.
- GREEN: all 8 plugin build-script tests, the Java/API harness and `bash dev/build-plugin-api.sh --check`, the isolated live JNI fixture, native registration, focused and workspace Cargo checks, Rust formatting, Python generator compilation, and diff whitespace checks passed.
- Mutation: restoring the second wrapper lookup reproduced the exact Java harness failure; restoration returned the full API check to green.
- Review mutation: replacing the living fallback's UUID with the resolved type made the live `Bat` lose its living wrapper; restoration passed for both the live `Bat` and a stale UUID.
- Performance: every wrapper creation resolves one native type exactly once; no cache, polling, synchronization, JNI tick call, or other tick work was added.
- Scope: generated `EntityType` output carries only validated source-backed classes, Paper 26.2 aliases/defaults, and the explicit Task 4 allowlist. Stale consumed registry constants are refreshed before Java generation. No generated source, extracted JSON, production Rust, or test-count ledger was edited.
- Report: `.superpowers/sdd/2026-09-16-popular-plugin-compatibility-wave-3/task-1-report.md`.
- Review status: in review; specification and quality re-review approvals are both still required before checking Task 1 complete.
- Second review fix: `4656276e6` adds the four Paper 26.2 class-spawn
  contracts/defaults and live-backed state. `AbstractCubeMob` persists
  `Paper.canWander` and toggles existing MOVE/JUMP/LOOK selector controls,
  immediately stopping running goals without per-goal tick checks.
- Second review evidence: 832 Java sources/1008 classes and the API harness,
  all 8 build-script tests, 7 focused core tests, the isolated live-JNI fixture,
  native registration, workspace all-targets, formatting, and diff checks pass.
- Second review status: in review; both re-review approvals remain pending, so
  Task 1 stays unchecked.
- Third review fix: `c409748ca9dceced9db63ac5676377de583ee09f`
  adds the exact Paper 26.2 Animals/Cow hierarchy and live state, dead-slime
  resize semantics, pre-publication JNI safety, transient Ravager target
  controls, complete Arrow ammunition potion behavior/persistence, and atomic
  class-spawn validation with generated wrapper metadata.
- Third review evidence: exact 6/6 Paper ABI diff; all 8 generator/stale-source
  tests; 29 focused core/registry tests; both native registration checks; both
  isolated real-JVM/JNI fixtures; 836-source/1015-class API build and harness;
  narrow and workspace all-target checks; formatting and diff checks pass.
- Third review performance/scope: no generated Rust or extracted data was
  edited, no per-goal branches or idle compatibility work were added, and
  external selector disables remain separate from the transient mob mask.
- Third review status: in review; independent specification and quality
  approvals are still required, so Task 1 remains unchecked.
- Fourth review fix: `ad27d7bc0bf4050e661e1d59936f49ca69c492fd`
  makes the Paper Ageable/Animals/Cow hierarchy and declarations exact, moves
  Breedable to age-based semantics independent of love state, validates
  negative love ticks before JNI, makes live-animal pre-publication registry
  access safe, constructs WATER TippedArrow state before publication, and adds
  exact Vanilla 600-tick exposed-potion decay alongside 1200-tick despawn.
- Fourth review evidence: exact 5/5 Paper public ABI diff; all 8
  generator/stale-source tests; 23 focused initialized-spawn/Arrow core tests;
  2 registry tests; 31 plugin unit tests; 520/520 native registration; three
  isolated real-JVM/JNI fixtures; 836-source/1015-class API build and harness;
  workspace all-targets, touched-crate Clippy, formatting, and diff checks pass.
- Fourth review performance/scope: the only new tick work is the exact decay
  check inside Arrow's existing in-ground branch; spawn initialization is
  request-driven, registry lifecycle handling is a guarded JNI request, no
  generated Rust or extracted data was edited, and temporary evidence was
  removed.
- Fourth review status: in review; independent specification and quality
  approvals are still required, so Task 1 remains unchecked.

## Review protocol

For each task:

1. Fresh implementer reads the Wave 3 spec, plan task, this ledger, and `AGENTS.md`.
2. Implementer records the exact RED command/output before production changes.
3. Implementer records focused GREEN commands, performance audit, and commit SHA.
4. Fresh specification reviewer approves or returns findings.
5. Fresh quality reviewer approves or returns findings.
6. A fresh fixer handles findings; affected verification and both reviews repeat until approved.
7. Only then mark the task complete and advance.

## Final evidence template

- Exact Foton SHA:
- Exact Zelda SHA: `3d7dc062b4353bced69f8383f52c961a5d42e17f`
- Maven classpath entries:
- Residual Paper jars: expected 0
- Zelda compile errors: expected 0
- Delta from 392 baseline: expected `-392` / `-100%`
- Runtime SQLite integrity:
- Zelda enable result / first remaining runtime blocker:
- `bash dev/ci.sh` result:
- Whole-branch specification review:
- Whole-branch quality review:
- Deferred ABI gaps: chest-boat refill/player-history; `MusicInstrument.create(...)`; unloaded-chunk ray side effects; historical NMS ceiling
- Repository cleanliness:
