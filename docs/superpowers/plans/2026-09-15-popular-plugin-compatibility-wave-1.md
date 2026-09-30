# Popular Plugin Compatibility Wave 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make more widely used Bukkit/Paper plugins load and enable correctly by fixing compatibility measurement, lifecycle rollback, dependency semantics, runtime libraries and Paper bootstrap support without adding game-tick overhead.

**Architecture:** Keep all new work in the optional Java compatibility host. Descriptor parsing produces immutable typed metadata; a deterministic graph decides load order; plugin activation is transactional; libraries are resolved before class loading; and Paper bootstrap registrations flow into the plugin lifecycle manager. Compatibility reporting distinguishes class/member presence, actual event linkage and fixture execution.

**Tech Stack:** Java 21 Bukkit/Paper compatibility API, Rust nightly, JNI 0.21, Python 3 standard library, shell fixture builder.

**Spec:** `docs/superpowers/specs/2026-09-15-popular-plugin-compatibility-wave-1-design.md`

## Global Constraints

- Target Minecraft and Paper API version is exactly 26.2.
- No NMS or CraftBukkit emulation and no fake `MavenLibraryResolver` implementation.
- No new work, JNI call, allocation, lock or serialization in the game-tick path.
- No JVM is loaded when `FOTON_PLUGIN_DIRECTORY` is absent.
- Every plugin failure is isolated and reports one causal diagnostic; unrelated plugins continue.
- No network access in tests; library fixtures use a pre-populated local cache.
- No production `unwrap`, `expect`, `panic!`, `todo!`, new `unsafe`, lint suppression or generated-file edit.
- All behavior changes follow test-first red-green-refactor and receive task-scoped review.

---

### Task 1: Honest compatibility analysis

**Files:**
- Modify: `dev/plugin_api_usage.py`
- Modify: `dev/test_plugin_api_usage.py`
- Modify: `dev/plugin-api-usage.json` only through `python3 dev/plugin_api_usage.py <corpus> --write` when the original corpus is available; otherwise leave the committed ledger unchanged.

**Interfaces:**
- Consumes: Java class files and the built `plugin-api/build/foton-plugin-api.jar`.
- Produces: `scan()` results that include member references, class references and event handler parameter types; a machine-readable compatibility summary emitted by `--summary-json`; event coverage based on Java bridge methods that have a real Rust caller.

- [ ] **Step 1: Add failing analyzer tests**

Add synthetic class-file fixtures to `dev/test_plugin_api_usage.py` proving four distinctions:

```python
def test_scan_keeps_class_only_references():
    found, unreadable = usage.scan(build_jar(CLASS_ONLY_SOURCE))
    assert unreadable == 0
    assert "org/bukkit/World" in found["classes"]

def test_handler_descriptor_records_fully_qualified_event():
    found, _ = usage.scan(build_jar(HANDLER_SOURCE))
    assert "org/bukkit/event/world/WorldUnloadEvent" in found["handler_events"]

def test_unlinked_java_fire_method_is_not_emitted():
    emitted = usage.emitted_events(JAVA_BRIDGE_WITH_ORPHAN, RUST_WITHOUT_CALLER)
    assert "org/bukkit/event/world/WorldUnloadEvent" not in emitted

def test_summary_keeps_shared_and_long_tail_counts_separate():
    summary = usage.compatibility_summary(LEDGER, PROVIDED, WANTED, EMITTED)
    assert summary["api"]["shared"]["resolved"] == 1
    assert summary["api"]["all"]["missing"] == 1
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run: `python3 -m unittest dev.test_plugin_api_usage -v`

Expected: the new assertions fail because `scan()` has no `classes` or `handler_events`, event linkage is inferred from Java declarations, and `compatibility_summary` does not exist.

- [ ] **Step 3: Implement complete reference extraction**

Extend the existing constant-pool reader to retain class references and parse method descriptors used by methods annotated with Bukkit `EventHandler`. Keep fully qualified slash-separated names internally and convert to dotted names only in output. Do not classify `org/bukkit/plugin/PluginManager` as an event.

Implement:

```python
def emitted_events(java_source_root: pathlib.Path, rust_source_root: pathlib.Path) -> set[str]:
    """Events whose Java fire path is called by the Rust bridge."""

def compatibility_summary(ledger, provided_members, wanted_events, emitted):
    """Return JSON-serializable binary, ceiling and event evidence."""
```

Add `--summary-json PATH` and write with sorted keys plus a trailing newline. Preserve the existing human commands and output.

- [ ] **Step 4: Run focused tests and compatibility commands**

Run:

```bash
python3 -m unittest dev.test_plugin_api_usage -v
python3 dev/plugin_api_usage.py --covered plugin-api/build/foton-plugin-api.jar
python3 dev/plugin_api_usage.py --covered plugin-api/build/foton-plugin-api.jar --events plugin-api/src
```

Expected: tests pass; shared API coverage remains 2,478 of 2,487; event output uses fully qualified event names and no longer counts orphaned Java `fireX` methods.

- [ ] **Step 5: Commit**

```bash
git add dev/plugin_api_usage.py dev/test_plugin_api_usage.py
git commit -m "feat(plugin): make compatibility evidence honest"
```

---

### Task 2: Transactional plugin lifecycle and cleanup

**Files:**
- Modify: `plugin-api/src/foton/PluginHost.java`
- Modify: `plugin-api/src/foton/FotonScheduler.java`
- Modify: `plugin-api/src/foton/CommandMap.java`
- Modify: `plugin-api/src/foton/EventBridge.java`
- Modify: `plugin-api/check/Checks.java`
- Create: `plugin-api/fixture/src/example/FailingLoadPlugin.java`
- Create: `plugin-api/fixture/src/example/FailingEnablePlugin.java`

**Interfaces:**
- Consumes: existing plugin-owned commands, listeners, services, channels and scheduler tasks.
- Produces: `PluginHost.cleanup(Plugin)` as the idempotent resource-release path; explicit loaded/failed/enabled state used by dependency checks; `FotonScheduler.cancelTasks(Plugin)` called for every rollback and disable.

- [ ] **Step 1: Write failing lifecycle fixtures**

Create fixtures whose `onLoad` and `onEnable` register a command, listener and repeating task before throwing. Extend `Checks.java` with assertions equivalent to:

```java
Checks.equal(0, EventBridge.handlerCount("example.FailingEvent"), "failed plugin listener leaked");
Checks.equal(null, Bukkit.getPluginCommand("failing"), "failed plugin command leaked");
Checks.equal(0, FotonScheduler.pendingTasks("FailingLoad"), "failed plugin task leaked");
Checks.that(FailingEnablePlugin.disabledSawFalse, "onDisable observed enabled=true");
```

Add a healthy dependent fixture and assert neither `onLoad` nor `onEnable` runs when its required dependency failed at the corresponding phase.

- [ ] **Step 2: Build fixtures and verify RED**

Run: `bash dev/build-plugin-api.sh --check`

Expected: one or more leak/dependency/state assertions fail on the existing host.

- [ ] **Step 3: Centralize idempotent cleanup and state transitions**

Refactor `PluginHost` so plugin state is registered before `onLoad`, required dependency state is checked before both phases, and every exceptional path invokes one cleanup routine. The routine must set `enabled=false` before `onDisable`, invoke `onDisable` only after an attempted enable, then cancel tasks and remove commands, services, channels, listeners and classloader entries in `finally`-style cleanup.

Expose package-private diagnostics only where the Java fixture needs them; production callers continue through Bukkit interfaces.

- [ ] **Step 4: Verify lifecycle behavior**

Run:

```bash
bash dev/build-plugin-api.sh --check
cargo test -p foton-plugin
```

Expected: all lifecycle fixtures and Rust host tests pass; no task, command, listener or loader survives a failed plugin.

- [ ] **Step 5: Commit**

```bash
git add plugin-api/src/foton/PluginHost.java plugin-api/src/foton/FotonScheduler.java plugin-api/src/foton/CommandMap.java plugin-api/src/foton/EventBridge.java plugin-api/check/Checks.java plugin-api/fixture/src/example
git commit -m "fix(plugin): make lifecycle rollback transactional"
```

---

### Task 3: Typed descriptors and deterministic dependency graph

**Files:**
- Modify: `plugin-api/src/org/bukkit/plugin/PluginDescriptionFile.java`
- Modify: `plugin-api/src/foton/PluginHost.java`
- Modify: `plugin-api/check/Config.java`
- Modify: `plugin-api/check/Checks.java`
- Create: `plugin-api/fixture/dependencies/` fixture descriptors and Java sources used by the build script.
- Modify: `dev/build-plugin-api.sh`

**Interfaces:**
- Consumes: legacy `depend`, `softdepend`, `loadbefore`, `provides`, `libraries`; Paper `dependencies.bootstrap`, `dependencies.server`, `bootstrapper`, `loader`.
- Produces: immutable `PluginDescriptionFile.Dependency` values with `Load { BEFORE, AFTER, OMIT }`, `required` and `joinClasspath`; case-insensitive real-name and alias lookup; deterministic topological order.

- [ ] **Step 1: Add descriptor and graph tests**

Add parsing assertions:

```java
PluginDescriptionFile d = descriptor("""
    name: Provider
    version: 1
    main: example.Provider
    api-version: '26.2'
    provides: [Vault]
    libraries: [com.example:fixture-lib:1.0]
    loadbefore: [Consumer]
    dependencies:
      server:
        Economy:
          load: BEFORE
          required: false
          join-classpath: true
    """);
Checks.equal(List.of("Vault"), d.getProvides(), "provides");
Checks.equal(List.of("com.example:fixture-lib:1.0"), d.getLibraries(), "libraries");
Checks.equal(PluginDescriptionFile.Load.BEFORE,
    d.getServerDependencies().get("Economy").load(), "Paper dependency direction");
```

Build a fixture set whose alphabetical order conflicts with `loadbefore`, whose consumer depends on an alias, and whose optional cycle must not reject an unrelated plugin.

- [ ] **Step 2: Run fixture build and verify RED**

Run: `bash dev/build-plugin-api.sh --check`

Expected: descriptor getters are absent and load order/alias assertions fail.

- [ ] **Step 3: Implement immutable descriptor values and validation**

Parse the fields listed in Interfaces. Reject unknown Paper load directions, non-map dependency bodies, blank names, duplicate aliases and an `api-version` numerically newer than 26.2 with `InvalidDescriptionException`. Keep returned maps/lists immutable.

- [ ] **Step 4: Implement the load graph**

Replace recursive jar ordering with an explicit graph over discovered descriptors. Required edges propagate failed/missing state. Alias resolution happens before edge construction. Use lexical plugin name as the stable tie-breaker. Log an optional edge removed to break an optional-only cycle; reject required cycles with every member named.

- [ ] **Step 5: Verify graph and existing host behavior**

Run:

```bash
bash dev/build-plugin-api.sh --check
python3 dev/check-natives.py
```

Expected: all dependency fixtures load in the asserted order, aliases resolve through `PluginManager#getPlugin`, malformed descriptors fail with their field name, and native declarations remain aligned.

- [ ] **Step 6: Commit**

```bash
git add plugin-api/src/org/bukkit/plugin/PluginDescriptionFile.java plugin-api/src/foton/PluginHost.java plugin-api/check dev/build-plugin-api.sh plugin-api/fixture/dependencies
git commit -m "feat(plugin): honor Bukkit and Paper dependency metadata"
```

---

### Task 4: Runtime classpath defaults and atomic library cache

**Files:**
- Modify: `foton/src/lib.rs`
- Modify: `foton-plugin/src/lib.rs`
- Modify: `foton-plugin/src/tests.rs`
- Modify: `plugin-api/src/foton/PluginHost.java`
- Modify: `plugin-api/check/Checks.java`
- Modify: `README.md`

**Interfaces:**
- Consumes: `PluginHostConfig.api_jar`, optional `library_directory`, descriptor `libraries`, existing `paper-libraries.json`.
- Produces: `PluginHostConfig::resolved_library_directory()`; validated direct Maven coordinates; `.foton-libraries/<group path>/<artifact>/<version>/<artifact>-<version>.jar` cache publication through a unique temporary file.

- [ ] **Step 1: Write failing Rust and Java tests**

Add Rust tests asserting an absent override resolves to the sibling `lib` directory of `plugin-api/build/foton-plugin-api.jar` and an explicit override wins. Add Java fixture checks using a pre-populated cache jar and a descriptor `libraries` entry; the plugin must load a class available only from that jar.

Add a cache publication test where the destination has a truncated temporary sibling and assert the valid final jar remains unchanged.

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test -p foton-plugin resolved_library_directory -- --nocapture
bash dev/build-plugin-api.sh --check
```

Expected: the Rust resolver test fails because no default exists and the Java library fixture fails with `ClassNotFoundException`.

- [ ] **Step 3: Implement default classpath resolution**

Derive `api_jar.parent().parent()/lib` when no override is supplied, include only sorted `.jar` entries, and return a startup error naming the missing directory when public API dependencies cannot be found. Update the README example to state that the bundled pinned directory is automatic and the environment variable is an override.

- [ ] **Step 4: Implement safe direct-library loading**

Use one coordinate parser for descriptor libraries and `paper-libraries.json`. Validate each segment before path construction. Download to a uniquely created temporary sibling, validate ZIP/JAR structure with `JarFile`, then move into place atomically when supported and with replace semantics only after validation. Bound connect/read timeouts to the existing 10/30 seconds. Tests must use an already populated cache and never contact Maven Central.

- [ ] **Step 5: Verify classpath and cache behavior**

Run:

```bash
cargo test -p foton-plugin
bash dev/build-plugin-api.sh --check
```

Expected: default and explicit classpaths pass; cached fixture class loads; interrupted artifacts are ignored; existing cache entries are preserved.

- [ ] **Step 6: Commit**

```bash
git add foton/src/lib.rs foton-plugin/src/lib.rs foton-plugin/src/tests.rs plugin-api/src/foton/PluginHost.java plugin-api/check/Checks.java README.md
git commit -m "feat(plugin): provide reliable runtime libraries"
```

---

### Task 5: Paper bootstrap lifecycle

**Files:**
- Modify: `plugin-api/src/foton/PluginHost.java`
- Create: `plugin-api/src/foton/FotonBootstrapContext.java`
- Modify: `plugin-api/src/foton/FotonLifecycle.java`
- Modify: `plugin-api/src/io/papermc/paper/plugin/lifecycle/event/FotonLifecycleEventManager.java`
- Modify: `plugin-api/src/org/bukkit/plugin/java/JavaPlugin.java`
- Create: `plugin-api/fixture/paper/src/example/PaperBootstrap.java`
- Create: `plugin-api/fixture/paper/src/example/PaperPlugin.java`
- Create: `plugin-api/fixture/paper/src/paper-plugin.yml`
- Modify: `dev/build-plugin-api.sh`
- Modify: `plugin-api/check/Checks.java`

**Interfaces:**
- Consumes: descriptor `bootstrapper`, plugin source path, data directory and plugin classloader.
- Produces: concrete `FotonBootstrapContext`; bootstrap lifecycle manager merged into the eventual plugin manager; `PluginBootstrap.createPlugin(PluginProviderContext)` honored when non-null.

- [ ] **Step 1: Add the failing Paper fixture**

Create a fixture recording ordered markers from bootstrap, main constructor, `onLoad`, lifecycle command callback and `onEnable`. Its bootstrapper returns a `PaperPlugin` constructed with a sentinel string and registers a command through `LifecycleEvents.COMMANDS`.

Assert:

```java
Checks.equal(
    List.of("bootstrap", "createPlugin", "constructor:sentinel", "onLoad", "commands", "onEnable"),
    PaperPlugin.steps,
    "Paper bootstrap lifecycle order");
Checks.that(Bukkit.getPluginCommand("paperfixture") != null,
    "bootstrap lifecycle command was not registered");
```

Add a second descriptor naming a class that does not implement `PluginBootstrap`; assert only that plugin is rejected and a healthy neighbor enables.

- [ ] **Step 2: Build and verify RED**

Run: `bash dev/build-plugin-api.sh --check`

Expected: the bootstrap markers and command are missing because the host constructs the main class directly.

- [ ] **Step 3: Implement bootstrap context and lifecycle transfer**

Implement `FotonBootstrapContext` with immutable metadata, data directory, SLF4J logger, plugin source and a dedicated `FotonLifecycleEventManager`. Load and type-check the bootstrapper through the plugin classloader, invoke it before main construction, then call `createPlugin(context)`. If it returns null, construct the descriptor main class. Transfer registered lifecycle handlers exactly once before the command lifecycle dispatch.

Reject a descriptor with `loader` using an error that names the unsupported class, before bootstrap begins. Do not add no-op loader classes.

- [ ] **Step 4: Verify Paper lifecycle and regressions**

Run:

```bash
bash dev/build-plugin-api.sh --check
python3 dev/check-natives.py
cargo test -p foton-plugin
```

Expected: fixture order is exact, the lifecycle command exists, invalid bootstrappers are isolated, and legacy fixtures still pass.

- [ ] **Step 5: Commit**

```bash
git add plugin-api/src/foton/PluginHost.java plugin-api/src/foton/FotonBootstrapContext.java plugin-api/src/foton/FotonLifecycle.java plugin-api/src/io/papermc/paper/plugin/lifecycle/event/FotonLifecycleEventManager.java plugin-api/src/org/bukkit/plugin/java/JavaPlugin.java plugin-api/fixture/paper dev/build-plugin-api.sh plugin-api/check/Checks.java
git commit -m "feat(plugin): run the Paper bootstrap lifecycle"
```

---

### Task 6: Host shutdown, compatibility summary and documentation

**Files:**
- Modify: `foton-plugin/src/lib.rs`
- Modify: `foton-plugin/src/tests.rs`
- Modify: `foton/src/lib.rs`
- Modify: `dev/build-plugin-api.sh`
- Create: `dev/plugin_compatibility.py`
- Create: `dev/test_plugin_compatibility.py`
- Modify: `README.md`
- Modify: `design/plugin-compatibility.md`
- Modify: `AUDITING.md`

**Interfaces:**
- Consumes: transactional Java disable, Rust `forward::unsubscribe`, Task 1 summary command and all fixture suites.
- Produces: idempotent host shutdown that removes plugin-owned Rust event subscriptions; `dev/plugin_compatibility.py` combining binary, ceiling, event and fixture evidence into one JSON document; documented compatibility command and current caveated metrics.

- [ ] **Step 1: Add failing shutdown tests**

Add a Rust test around a testable subscription owner proving repeated bind does not duplicate subscriptions and shutdown removes the `foton:plugins` owner. Add a Java host check that `disableAll()` leaves zero plugins, handlers, commands and pending tasks.

Add `dev/test_plugin_compatibility.py` with temporary analyzer and fixture reports:

```python
def test_combined_report_keeps_evidence_kinds_separate(self):
    report = compatibility.combine(API_REPORT, FIXTURE_REPORT)
    self.assertEqual(2478, report["binary"]["shared_members"]["resolved"])
    self.assertEqual(3, report["fixtures"]["enabled"])
    self.assertEqual(1, report["fixtures"]["rejected"])
    self.assertNotIn("plugin_success_percent", report)
```

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test -p foton-plugin unsubscribe -- --nocapture
bash dev/build-plugin-api.sh --check
```

Expected: Rust subscriptions survive because production shutdown never invokes `PluginHost::unsubscribe`.

- [ ] **Step 3: Wire idempotent host shutdown**

Store the weak server association needed for shutdown without creating an `Arc` cycle. Ensure the owning server calls Java disable and Rust unsubscribe exactly once on graceful shutdown and on startup failure after subscription. Keep `Drop` non-blocking; explicit shutdown remains the error-reporting path.

- [ ] **Step 4: Publish compatibility evidence**

Implement `dev/plugin_compatibility.py` with `combine(api_report, fixture_report)` and a CLI accepting `--api-report`, `--fixture-report` and `--output`. Make the Java fixture runner write deterministic JSON containing discovered, loaded, enabled and rejected fixture counts plus rejection reasons. The combined output has top-level `binary`, `ceiling`, `events` and `fixtures` objects and deliberately has no aggregate plugin-success percentage.

Update README and auditing documentation with the exact commands:

```bash
bash dev/build-plugin-api.sh --check
python3 dev/plugin_api_usage.py --covered plugin-api/build/foton-plugin-api.jar
python3 dev/plugin_api_usage.py --covered plugin-api/build/foton-plugin-api.jar --events plugin-api/src
python3 dev/plugin_compatibility.py --api-report build/plugin-api-evidence.json --fixture-report plugin-api/build/fixture-evidence.json --output build/plugin-compatibility.json
```

State beside the output that shared symbol coverage is not a plugin success rate, the NMS/CraftBukkit slice is excluded, and event coverage requires a Rust call site. Update `design/plugin-compatibility.md` only with measurements produced by the final branch.

- [ ] **Step 5: Run narrow verification**

Run:

```bash
cargo test -p foton-plugin
bash dev/build-plugin-api.sh --check
python3 -m unittest dev.test_plugin_api_usage -v
python3 -m unittest dev.test_plugin_compatibility -v
python3 dev/check-natives.py
```

Expected: all commands exit zero.

- [ ] **Step 6: Run full verification**

Run:

```bash
cargo fmt --all --check
cargo clippy -r --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
bash dev/ci.sh
```

Expected: every command exits zero and `dev/ci.sh` ends with `ALL GREEN`.

- [ ] **Step 7: Commit**

```bash
git add foton-plugin/src/lib.rs foton-plugin/src/tests.rs foton/src/lib.rs dev/build-plugin-api.sh dev/plugin_compatibility.py dev/test_plugin_compatibility.py README.md design/plugin-compatibility.md AUDITING.md
git commit -m "docs(plugin): publish verified compatibility evidence"
```
