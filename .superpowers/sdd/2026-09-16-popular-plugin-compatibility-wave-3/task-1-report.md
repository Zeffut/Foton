# Task 1 entity wrapper identity report

## Result

Implementation commit: `ba7cd62e0` (`fix(plugin): preserve entity wrapper identity`).

Entity wrapper creation now receives a UUID and an already-resolved normalized
registry type. `FotonEntity`, world/chunk enumeration, and projectile owner
resolution each call `Native.entityType` once and pass the result through;
`FotonWorld.wrapEntity` does not repeat that lookup. Its living-entity fallback
continues to query the supplied UUID, preserving generic/stale handle behavior.

`dev/gen-entity-type.py` now emits exact class-name-to-`EntityType` cases from
the authoritative generated entity registry constants into the existing
generated `EntityType.java`. `FotonEntityFactory.typeFor` exposes that mapping,
and class-based world spawning uses it instead of lowercasing Java simple names.
The generated output includes `BlockDisplay -> BLOCK_DISPLAY` and the future
Task 4 `ItemDisplay -> ITEM_DISPLAY` mapping. There is no handwritten Java map,
generated-source edit, runtime type cache, or tick work.

## TDD RED evidence

Both failures were observed before their corresponding production changes.

1. With only the wrapper regression check added,
   `bash dev/build-plugin-api.sh --check` exited 1 after compiling 826 sources
   and entering the Java harness. `EntityCheck` called
   `wrapEntity(uuid, "block_display")`; the existing implementation called
   `Native.entityType("block_display")` and failed with
   `UnsatisfiedLinkError` at `FotonWorld.wrapEntity`. This proves the test
   catches the second lookup rather than merely checking a wrapper class.
2. After adding the class-to-type assertion, the same command exited 1 during
   check compilation because `FotonEntityFactory.typeFor(Class<BlockDisplay>)`
   did not exist. The old spawn implementation would derive `blockdisplay`,
   which cannot resolve the canonical `block_display` registry path.

`BlockDisplay` is used for the executable RED because `ItemDisplay.java` is a
Task 4-owned API file and does not exist at this checkpoint. It exercises the
same underscored-key defect without expanding Task 1's file scope.

## GREEN and mutation evidence

- `bash dev/build-plugin-api.sh --check` — passed after restoration; 826 Java
  sources compiled, 1002 classes written, and the full Java/API harness passed.
  The optional external plugin boot was explicitly skipped because
  `FOTON_PLUGIN_FIXTURE` was unset.
- Focused Java/API harness (`java ... Checks` with the fixture arguments
  produced by `build-plugin-api.sh`) — passed and printed the normal plugin API
  success summary.
- Wrapper mutation — temporarily restoring
  `type = Native.entityType(type)` with `apply_patch` made
  `bash dev/build-plugin-api.sh --check` exit 1 at
  `EntityCheck.suppliedRegistryTypeSelectsTheWrapperWithoutAnotherLookup` with
  the expected `UnsatisfiedLinkError`. The mutation was removed with
  `apply_patch`, and the full GREEN command passed again.
- `python3 dev/check-natives.py --quiet` — passed.
- `cargo check -p foton-plugin --all-targets` — passed. The build emitted its
  existing generated-asset fetch/extraction warnings and finished successfully.
- `cargo fmt --all --check` — passed.
- `git diff --check` — passed before the implementation commit.
- `python3 -m py_compile dev/gen-entity-type.py` — passed; its disposable bytecode
  was removed before commit.

## Scope and performance audit

Only Task 1 implementation/check files changed. `dev/build-plugin-api.sh` did
not need modification because it already compiles the generator's existing
`EntityType.java` output. No Rust source, extracted JSON, generated source,
test-count ledger, entity contract, cache, polling path, or tick path changed.

All `wrapEntity` callers were audited with `rg`: each resolves the type exactly
once before wrapping. The remaining `Native.entityType` call in
`FotonEntity.getType()` is the accessor's own request and is not wrapper
construction.

The root `build/plugin-api-evidence.json` and Python bytecode created by checks
were removed. The ignored `plugin-api/build/` remains the normal reusable API
build output; no generated artifact is tracked.

## Self-review

The final diff preserves every existing wrapper-selection branch, changes only
the meaning of `wrapEntity`'s second argument from UUID text to normalized type,
and keeps `Native.entityIsLiving` keyed by the actual UUID. Null classes and
unknown generated class names resolve to null as before, so `spawn` declines
unsupported classes rather than guessing. No open Task 1 concern remains.
