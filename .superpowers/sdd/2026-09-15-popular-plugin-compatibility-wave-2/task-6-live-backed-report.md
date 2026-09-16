# Task 6 live-backed API implementation report

## Result

Implementation commit: `638b70df56c96068132565be5a56b1be3752e782`
(`638b70df5 Add live-backed Bukkit compatibility APIs`).

The original Task 6 live-backed APIs now use existing live state:

- `Player` extends `OfflinePlayer`, `OfflinePlayer` extends the independent
  `AnimalTamer`, and `AnimalTamer` declares `getName()` / `getUniqueId()`.
- The four requested legacy game-rule handles use the authoritative current
  registry keys and have one canonical identity in `values()` / `getByName`.
- `ITEMS_TRIMMABLE_ARMOR` uses the normal live item-tag bridge.
- generated `Enchantment.conflictsWith` passes both complete namespaced keys
  through a request-time JNI binding to
  `Enchantment::are_compatible`; it contains no copied exclusive-set data.

The Minecraft target was verified as Cargo `0.15.2+mc26.2`; local
`minecraft-src/minecraft` is commit `e31ff098` with subject `26.2`.

## TDD red evidence

The initial tests were added before production changes.

1. `cargo test -p foton-plugin live_registry_bridge_resolves_namespaced_tags_and_enchantment_conflicts --no-run`
   exited 101. Rust reported eight `E0425` errors: two unresolved calls to
   `is_tagged_state` and six unresolved calls to
   `enchantments_conflict_state`. This isolated the absent live registry
   bridge.
2. `bash dev/build-plugin-api.sh --check` exited 1. `javac` reported exactly
   five missing symbols: `ANNOUNCE_ADVANCEMENTS`, `DO_INSOMNIA`,
   `DO_PATROL_SPAWNING`, `DO_TRADER_SPAWNING`, and
   `ITEMS_TRIMMABLE_ARMOR`.

Those compile failures masked the hierarchy assertions, and the end-to-end
generated Java/JNI fixture was introduced after the first bridge helper. This
gap was corrected honestly with scoped remove-and-restore mutation cycles while
the tests stayed in place:

3. With only the three hierarchy production edits temporarily removed,
   `bash dev/build-plugin-api.sh --check` built the jar and then exited 1 at
   `LiveBackedApiCheck`: `AssertionError: Player should extend OfflinePlayer`.
   Reapplying the hierarchy made the same check pass.
4. With only generated `Enchantment.conflictsWith` temporarily removed,
   `bash dev/build-plugin-api.sh` succeeded, then
   `cargo test -p foton-plugin generated_java_apis_reach_live_registry_natives -- --nocapture`
   exited 101. The real JVM returned `JavaException` when the fixture invoked
   the missing generated method. Reapplying the generator method made the
   actual JVM/JNI test pass 1/1.

The first attempt to run the repository's older integration bridge exposed
that its Java discovery silently skipped on this macOS host. The new focused
fixture does not skip: missing Java or a missing API jar is a test error. It ran
the actual JVM successfully.

## Green verification

- `bash dev/build-plugin-api.sh --check` — passed; 826 Java sources compiled,
  43 enchantment handles generated, all fixture checks passed. The optional
  external plugin boot remained explicitly skipped because
  `FOTON_PLUGIN_FIXTURE` was unset; this is the script's supported behavior.
- `cargo test -p foton-plugin generated_java_apis_reach_live_registry_natives -- --nocapture`
  — passed 1/1 in a real JVM. It invoked the generated Java tag and enchantment
  handles across registered JNI. Assertions covered diamond chestplate true,
  elytra false, identical enchantments conflicting, Infinity/Mending in both
  directions, Sharpness/Unbreaking compatible, and an unknown non-Minecraft
  namespace failing safely.
- `cargo test -p foton-plugin live_registry_bridge_resolves_namespaced_tags_and_enchantment_conflicts -- --nocapture`
  — passed 1/1 focused registry bridge test.
- `cargo test -p foton-plugin` — passed 32 unit tests plus 1 integration test;
  1 pre-existing spawn bridge test remained ignored by its own annotation.
- `cargo test -p foton-registry` — passed 474 unit tests and 1 compile-fail
  doctest; 5 documentation examples remained ignored.
- `cargo check -p foton-plugin -p foton-registry --all-targets` — passed.
- `python3 dev/check-natives.py` — passed: 508 declarations, 508 registrations.
- `cargo fmt --all --check` — passed.
- `git diff --check` — passed before commit.

There were no verified pre-existing gate failures. Existing compiler warnings
were unchanged; the macOS linker also emitted its pre-existing compact-unwind
size warning during test linking.

## Changed files

- `dev/gen-enchantment.py`
- `foton-plugin/src/lib.rs`
- `foton-plugin/src/natives.rs`
- `foton-plugin/src/natives_live_api_tests.rs`
- `plugin-api/check/Checks.java`
- `plugin-api/check/LiveBackedApiCheck.java`
- `plugin-api/src/foton/Native.java`
- `plugin-api/src/org/bukkit/GameRule.java`
- `plugin-api/src/org/bukkit/OfflinePlayer.java`
- `plugin-api/src/org/bukkit/Tag.java`
- `plugin-api/src/org/bukkit/entity/AnimalTamer.java`
- `plugin-api/src/org/bukkit/entity/Player.java`

No generated or extracted asset was edited or committed. The existing ignored
`plugin-api/build/` was regenerated for the Java/JNI gates. The root
`build/plugin-api-evidence.json` created by `--check` was deleted and its newly
created empty directory removed. Cargo reused the repository's existing
`target/`. No Desktop path was created or touched.

The controller-owned tracked progress ledger was deliberately not staged or
committed.
