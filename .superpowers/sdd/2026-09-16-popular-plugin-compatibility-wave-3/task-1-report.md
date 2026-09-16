# Task 1 entity wrapper identity report

## Status

Task 1 is **in review**. The original implementation is `ba7cd62e0`
(`fix(plugin): preserve entity wrapper identity`) and the review-fix source
commit is `2febe9169` (`fix(plugin): validate entity class metadata`). The
second-round review fix is `4656276e6` (`fix(plugin): complete Paper class
spawn contracts`). The third-round review fix is `c409748ca`
(`fix(plugin): complete third Task 1 review repair`). The fourth-round review
fix is `ad27d7bc0` (`fix(plugin): complete fourth Task 1 review repair`).
Independent specification and quality re-review remain pending, so the task is
not recorded as complete.

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

The fourth repair makes the Paper hierarchy and public declarations exact:
`Ageable -> Creature`, `Breedable -> Ageable`, `Animals -> Breedable`, and
`AbstractCow -> Animals`, with `Cow` transitively a `Creature` and `Mob`.
Paper's nine abstract `Ageable` methods now have live inherited wrapper
implementations. `canBreed` reads `AgeableMob.age == 0`; `setBreed(false)`
sets an adult age to 6000 and leaves babies unchanged; `setBreed(true)` sets
age to zero. None of those operations modifies love ticks or breed cause, and
negative love-mode ticks are rejected in Java before JNI.

All new Animals/Breedable registry reads now use optional publication access.
The isolated lifecycle fixture registers a live Rust `Entity + Animal` by UUID
without publishing the vanilla registry, then exercises both nonempty
`Material.WHEAT` and `ItemStack(WHEAT)` calls through a real JVM/JNI boundary.

Class-based `TippedArrow` spawn now carries a WATER initialization payload into
native construction. The Arrow's ammo component, potion state, and synchronized
color are complete before `try_add_entity`; invalid potion/type combinations
return before publication. A separate ordinary Arrow path remains unchanged.

Vanilla's exposed tipped-arrow decay now runs in the existing in-ground Arrow
tick path. Nonempty `PotionContents` includes custom effects/color/name, so it
decays to ordinary-arrow pickup contents and synchronized color `-1` exactly on
tick 600. The separate persisted lifetime still despawns at 1200, while the
unsaved exposure counter restarts after reload, matching local Vanilla.

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
12. The fourth-round core RED failed compilation on absent
    `SpawnEntityInitialization`, `create_entity_at`,
    `spawn_entity_at_initialized`, and `ArrowState.in_ground_time`. The Java
    RED failed compilation because the test called the missing six-argument
    native spawn descriptor.
13. Direct hierarchy assertions exposed `Ageable extends LivingEntity` instead
    of Paper's `Creature`; the final `javap` comparison also exposed the extra
    local `setBaby(boolean)` declaration and default-vs-abstract method flags.
14. The live breeding assertions exposed `canBreed` reading love mode and
    `setBreed` mutating love ticks rather than age. Boundary checks cover adult
    cooldown, baby no-op, instant maturation, and love/cause independence.
15. Mutating the repaired breed-item path back to direct `REGISTRY.items`
    access made the isolated live-animal JVM test panic at `Registry not init`
    and abort with SIGABRT. Restoring `REGISTRY.get()` made the same command
    pass.

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

## Fourth review repair evidence

The implementation source commit is
`ad27d7bc0bf4050e661e1d59936f49ca69c492fd`. Paper behavior was verified from
the local source checkout at exact Paper commit
`a2a42c5b12249aaba42a347327fd930a1f94af06` (API build
`26.2.build.121-stable`): `CraftAgeable.canBreed()` is `getAge() == 0`,
`setBreed(true)` sets age zero, and `setBreed(false)` sets an adult age to
6000. Local target-version Vanilla `Arrow.java` uses
`EXPOSED_POTION_DECAY_TIME = 600`, tests nonempty complete `PotionContents`,
and replaces pickup ammo with `Items.ARROW`; `AbstractArrow` persists `life`
and `inGround` but not `inGroundTime`, with despawn at 1200.

- Paper public ABI comparison with `javap -public` — exact diff match for
  `Ageable`, `Breedable`, `Animals`, `AbstractCow`, and `Cow` (5/5), including
  direct inheritance and abstract method flags.
- `python3 -m unittest dev.tests.test_plugin_api_build_scripts` — 8 passed in
  41.876s.
- `bash dev/build-plugin-api.sh --check` — 836 sources compiled, 1015 classes
  written, and the Java/API harness passed; only the optional external fixture
  boot was skipped because `FOTON_PLUGIN_FIXTURE` was unset.
- Initialized-spawn core tests — 2 passed, 4451 filtered out; the tests inspect
  WATER ammo/color before publication and prove type mismatch publishes
  nothing.
- Arrow-focused core tests — 21 passed, 4432 filtered out, including exact
  599/600, custom-effect-only, ordinary-arrow, save/reload, and 1200-despawn
  behavior.
- Potion-content registry tests — 2 passed, 473 filtered out.
- `cargo test -p foton-plugin --lib -- --nocapture` — 31 passed and 5 ignored;
  the ignored real-JVM tests were run separately below.
- Native descriptor registration — 1 passed, 35 filtered out.
- `python3 dev/check-natives.py` — 520 declared and 520 registered; no gaps.
- Isolated pre-publication live-animal JVM/JNI regression — 1 passed, 35
  filtered out; its direct-registry mutation aborts with SIGABRT as recorded
  above.
- Isolated pre-publication stale-Arrow JVM/JNI regression — 1 passed, 35
  filtered out.
- Full real-JVM/JNI spawn bridge — 1 passed, 35 filtered out; it covers exact
  adult/baby/cooldown/love semantics, the `-1` love-tick boundary, nonempty
  breed items, preinitialized WATER TippedArrow, and malformed no-orphan spawn.
- `cargo check --workspace --all-targets` — passed in 31.68s.
- `cargo clippy -p foton-core -p foton-plugin --lib -- -D warnings` — passed in
  9.52s.
- `cargo fmt --all --check` and `git diff --check` — passed before the source
  commit.

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
approve `ba7cd62e0..ad27d7bc0`. No later Wave 3 task is marked started here.
