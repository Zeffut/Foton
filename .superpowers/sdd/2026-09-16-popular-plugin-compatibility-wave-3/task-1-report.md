# Task 1 entity wrapper identity report

## Status

Task 1 is **in review**. The original implementation is `ba7cd62e0`
(`fix(plugin): preserve entity wrapper identity`) and the review-fix source
commit is `2febe9169` (`fix(plugin): validate entity class metadata`). The
second-round review fix is `4656276e6` (`fix(plugin): complete Paper class
spawn contracts`). The third-round review fix is `c409748ca`
(`fix(plugin): complete third Task 1 review repair`). Independent specification
and quality re-review remain pending, so the task is not recorded as complete.

## Result

Entity wrapper creation receives a UUID and an already-resolved normalized
registry type. Every wrapper caller resolves the type once, while the living
fallback still queries `Native.entityIsLiving(uuid.toString())`. A real-JNI
fixture now proves that an unhandled living `Bat` receives a living wrapper and
that a stale UUID remains a generic `FotonEntity`.

`dev/gen-entity-type.py` no longer emits PascalCase guesses for every registry
entry. It carries the Paper 26.2 irregular class aliases, including
`TNTPrimed`, `EnderCrystal`, `Firework`, `MushroomCow`, `ThrownExpBottle`, and
`FishHook`, plus Paper's supported abstract-class spawn defaults. Regular
mappings are emitted only when the corresponding Bukkit interface source
exists. `Allay`, `ItemDisplay`, and `TextDisplay` are the narrowly documented
Task 4 allowlist. Duplicate class mappings fail generation; generic `Boat` and
`Fish` remain unmapped because no unique registry variant exists.

`dev/build-plugin-api.sh` verifies the exact entity constants it consumes
against `foton-registry/build_assets/entities.json`. Missing or stale generated
registry input forces the registry build script to rerun before any Java
generator executes. Current verified input does not invoke Cargo. The generator
has no Paper jar, download, or runtime-network dependency.

The second review fix adds Paper 26.2's `AbstractCow`, `AbstractCubeMob`,
`SizedFireball`, and deprecated `TippedArrow` class-spawn defaults. The live
wrappers are distinct where identity matters: `SizedFireball` uses native
large/small-fireball item state, while `TippedArrow` is returned only for that
class-spawn path and receives native `minecraft:water` potion contents before
control returns. An ordinary `Arrow` remains a `FotonArrow` with no base
potion.

`AbstractCubeMob` size delegates to existing clamped cube state. Its persistent
`Paper.canWander` value disables MOVE/JUMP/LOOK once in the existing goal
selector and immediately stops running intersecting goals. The four cube goals
contain no wander branch, scan, JNI call, allocation, or lock in their tick
paths.

The third repair adds Paper 26.2's exact plural `Animals`/`Breedable` contract
and direct `AbstractCow -> Animals` and `MushroomCow -> AbstractCow`
inheritance. Cow breeding/love, food, variant, and sound-variant behavior is
live JNI/core state. Slime resizing now passes `Entity::is_alive` as Vanilla's
`updateHealth`, so dead or dying slimes are not healed.

Arrow potion contents now remain on the ammunition stack. Base and custom
effects use the extracted ammunition duration scale on hit, pickup preserves
the full tipped-arrow components (including WATER), synchronized ARGB color is
updated with Vanilla's exact fallback/mixing rules, and the existing projectile
save/load path persists the `item` stack. The JNI setter checks the entity and
Arrow downcast before touching the optional registry publication.

Ravager rider controls now share the selector's transient mob mask, including
TARGET, while persistent external disables remain independent. Class spawning
preflights generated exact wrapper support before native insertion; null,
unsupported, and mismatched classes throw `IllegalArgumentException`, and the
defensive postcondition removes any unexpected native entity.

## TDD RED evidence

The original wrapper and underscored-key REDs remain recorded in `ba7cd62e0`.
The review findings added these fresh RED cycles:

1. `bash dev/build-plugin-api.sh --check` reached `EntityCheck` and exited 1
   with `TNTPrimed class-to-entity type: expected TNT, got null`.
2. The two focused build-script tests both failed before production changes:
   the class validator listed the guessed, source-less interfaces and the
   planted `STALE_CHECKOUT_ENTITY` bypassed Cargo before failing Java
   compilation.
3. After adding the real-JNI regression, temporarily changing
   `Native.entityIsLiving(uuid.toString())` to `Native.entityIsLiving(type)`
   made `cargo test -p foton-plugin --lib spawn_bridge_dispatches -- --ignored
   --nocapture` fail at `an unhandled living type must use the living fallback`.
   Restoring the UUID made the same fixture pass; the stale UUID assertion also
   passed.
4. The second-round Java/API RED failed with 21 diagnostics for the four absent
   Paper contracts and wrappers. Focused Rust REDs failed on absent cube wander
   state and the native arrow potion setter.
5. The first cube implementation put `canWander` branches in four goal paths.
   The corrected selector test proves the goals remain eligible while selector
   controls stop and restore the registered four-goal set.
6. The live-JNI size regression failed because `setSize(0)` was ignored; after
   removing the Java/JNI guards it reaches core's native 1..127 clamp.
7. Removing only the production build-script `touch` made the strengthened
   stale-registry test fail with exit 23; restoring it returned the test to
   green.
8. The third-round API RED failed compilation because Paper's `Animals` and
   `Breedable` contracts did not exist and `AbstractCow` still extended the
   singular local `Animal` interface.
9. The new dead-slime live-JNI assertion exposed the unconditional
   `set_cube_size(size, true)` health update; the transient-control regression
   likewise exposed Ravager's use of the persistent external mask.
10. `cargo test -p foton-plugin
    forward::spawn_bridge_tests::stale_arrow_potion_before_registry_publication_is_safe
    -- --ignored --exact` aborted with SIGABRT before the guard-order fix,
    because `REGISTRY` was dereferenced before stale-handle rejection.
11. The Arrow regressions exposed full-duration hit effects, ordinary-arrow
    pickup identity, absent synchronized fallback color, and missing potion
    ammunition persistence. The class-spawn regression exposed Egg insertion
    followed by a null return, plus non-Paper null/unsupported handling.

## Fresh GREEN evidence

- `python3 -m unittest dev.tests.test_plugin_api_build_scripts` — 8 passed.
- `bash dev/build-plugin-api.sh --check` — 832 Java sources compiled, 1008
  classes written, and the Java/API harness passed. The optional external
  plugin boot was skipped because `FOTON_PLUGIN_FIXTURE` was unset.
- Focused `foton-core` tests — 7 passed across cube wander state/control,
  rider-control preservation, arrow potion contents, and fireball item state.
- `cargo test -p foton-plugin
  spawn_bridge_dispatches_cancellation_and_queries_released_bee -- --ignored
  --nocapture` — 1 passed in an isolated JVM, including all four class-spawn
  identity/state paths and the prior living/stale assertions.
- `python3 dev/check-natives.py --quiet` — passed.
- `cargo check -p foton-plugin --all-targets` — passed.
- `cargo check --workspace --all-targets` — passed.
- `cargo fmt --all --check` — passed.
- `git diff --check` — passed before the source commit.
- `python3 -m py_compile dev/gen-entity-type.py
  dev/tests/test_plugin_api_build_scripts.py` — passed.

The Cargo commands emitted the repository's existing asset extraction messages;
no new online lookup was introduced by Task 1.

## Third review repair evidence

The implementation source commit is `c409748ca9dceced9db63ac5676377de583ee09f`.
Behavior and constants were checked against the local Paper
`26.2.build.121-stable` API jar and local target-version Vanilla
`Arrow.java`, `PotionContents.java`, and `MobEffectInstance.java`; ammunition
duration scaling comes from the existing extracted `POTION_DURATION_SCALE`
component rather than a copied value.

- Paper ABI comparison with `javap -public` — exact diff match for
  `Breedable`, `Animals`, `AbstractCow`, `Cow`, `Cow$Variant`, and
  `Cow$SoundVariant` (6/6).
- `python3 -m unittest dev.tests.test_plugin_api_build_scripts` — 8 passed in
  42.494s, including generated wrapper support and stale-source coverage.
- `bash dev/build-plugin-api.sh --check` — 836 sources compiled, 1015 classes
  written, and the Java/API harness passed; only the optional external fixture
  boot was skipped because `FOTON_PLUGIN_FIXTURE` was unset.
- Arrow-focused core tests — 13 passed, 4434 filtered out.
- Goal-selector tests — 6 passed, 4441 filtered out.
- Ravager tests — 5 passed, 4442 filtered out.
- Base mob-control tests — 3 passed, 4444 filtered out.
- Potion-content registry tests — 2 passed, 473 filtered out.
- Native descriptor registration — 1 passed, 34 filtered out.
- `python3 dev/check-natives.py --quiet` — passed.
- Isolated pre-publication real-JVM/JNI Arrow setter regression — 1 passed,
  34 filtered out; bridge integration target had 0 selected and 1 filtered.
- Full real-JVM/JNI spawn bridge — 1 passed, 34 filtered out; it covers live
  Animals/Cow state, dead-slime resize, class-spawn exceptions/no-orphan, and
  TippedArrow WATER initialization.
- `cargo check -p foton-registry -p foton-core -p foton-plugin --all-targets`
  — passed in 16.87s.
- `cargo check --workspace --all-targets` — passed in 16.49s.
- `cargo fmt --all --check` and `git diff --check` — passed.

## Scope and performance audit

The review fixes change only Task 1-related generator, API, core/native, test,
report, and ledger files. They do not modify generated Rust, extracted JSON,
registry publication, or the test-count ledger. Runtime wrapper creation still
performs one native type lookup. Wander and rider-control changes are
operation-driven and reuse the selector's cached disabled-control masks; no
per-goal branch or other new idle/tick compatibility work was added.

`build/plugin-api-evidence.json` and task-created temporary logs were removed.
The ignored reusable `plugin-api/build/` output remains untracked.

## Re-review gate

The findings have implementation and fresh verification evidence, but Task 1
must remain in review until both the specification and quality re-reviews
approve `ba7cd62e0..c409748ca`. No later Wave 3 task is marked started here.
