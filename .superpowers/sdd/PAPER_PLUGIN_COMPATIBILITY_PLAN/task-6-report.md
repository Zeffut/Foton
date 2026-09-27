# Task 6 — proven standalone bootstrap slice; task remains incomplete

Base: `4ccba2fc49de066f8e8f4fbd97609ffccd5ea52c`. Target remains `0.15.2+mc26.2`.

## Implemented and observed

Paper plugins without bootstrap dependency edges now execute their resolved loader classpath, bootstrap callback, bootstrap command lifecycle, custom or default `createPlugin`, construction, `onLoad`, and `onEnable`. All bootstrap callbacks and then command callbacks run before the construction pass. Bootstrap order comes from the bootstrap graph, independently of the server graph. Server dependency visibility stays unavailable during bootstrap and becomes available for construction. Declared bootstrap dependencies remain explicitly unsupported.

The bootstrap context implements the actual provider context and lifecycle owner interfaces. Paper's default `createPlugin` instantiates the configured main class; returning null is an error. `PluginMeta.getMainClass()` and the lifecycle owner inheritance were checked against build129 bytecode. Only `PluginDescriptionFile` implements PluginMeta in this tree.

Command trees remain private until construction succeeds, survive the later plugin command lifecycle, and are removed by existing owner teardown. Invocation uses the host's enabled/in-flight barrier. Lifecycle linkage errors now propagate instead of logging a successful enable without commands. Configured handlers retain their event identity; local priority/monitor ordering is tested. Non-default bootstrap priority/monitor configuration fails explicitly until cross-plugin ordering exists.

## Oracle evidence

Read the official [Paper plugin guide](https://docs.papermc.io/paper/dev/getting-started/paper-plugins/), [lifecycle guide](https://docs.papermc.io/paper/dev/lifecycle/), and [registry guide](https://docs.papermc.io/paper/dev/registries/). Inspected the actual pinned API using `javap`, including the default createPlugin body, BootstrapContext, LifecycleEventManager, configuration interfaces, RegistrarEvent and ReloadableRegistrarEvent.

The durable fixture sources are `plugin-api/check/fixtures/bootstrap/`. The plugin was compiled with Java 25 against the real build129 API/library closure, not Foton's API. Its separate PluginLoader adds a real external JarLibrary; the bootstrapper consumes that library. Paper assigns library addition to PluginLoader, not to PluginBootstrap.

Pinned Paper 26.2 build129 SHA-256: `b1d8f6bfa1b6101fa8e947b53041cb3bdf5540e7b83b6547ca19ba7edefeb083`.

Unchanged fixture JAR SHA-256: `eaa4b81cd6269d4354fb6f671efd0c8c714f2f10ab03d4168d471814a40447f0`.

External fixture library SHA-256: `4fdb7c1533c6c1023b36b1f6baca1083c27c97efac17623e5f9e46587ba22473`.

Both real servers produced this exact normalized trace:

```text
loader
bootstrap:BootstrapProbe
library:isolated
commands
create
construct:custom
load
enable
execute
disable
```

Paper evidence: `/var/tmp/foton-task6-oracle-5ce7b9ne/server.log`.

Final Foton evidence: `/var/tmp/foton-task6-native-y0ysrgni/foton/server.log` (earlier matching run: `/var/tmp/foton-task6-native-xjyfhcfg/foton/server.log`). The native scenario answered status protocol 776, executed `bootstrap_probe` through the console, and stopped cleanly. It used a freshly built `/root/foton-target-task3/debug/foton`, SHA-256 `aace92860aa7454d22d7243a780805735e35e68d052f98ef00574f3850d5d963`, and the final API JAR. No old Task2 binary was used. Evidence and fixture JARs stay outside the repository; no third-party JAR was added to git.

## Tests

- Red first: the new isolated bootstrap fixture failed `bootstrap loads: 0 != 1`, with `Paper bootstrap execution is not implemented` from the old host.
- `bash dev/build-plugin-api.sh --check`: passed, including Task4 descriptor/graph fixtures, Task5 transitive library fixtures, and the new bootstrap checks.
- Focused PaperBootstrap checks also passed with the real Paper-compiled fixture supplied through `-Dfoton.bootstrap.oracle-jar=...` and `-Dfoton.bootstrap.library=...`.
- Failure fixtures cover bootstrap throw, createPlugin throw after constructing an instance, command callback throw, onLoad throw, missing library, unsupported registry lifecycle, and a failed required server dependency. They assert no early enable, no published plugin/command/permission/event listener/loader ownership, and that captured class loaders cannot reopen their plugin descriptor after cleanup.
- Additional checks cover the default constructor path, all-bootstrap-before-construction ordering, no command invocation before enable, and reverse plugin shutdown order.
- Configured-handler regression verifies event identity (a tag callback must never run for COMMANDS), priority/monitor exclusivity, and explicit rejection of unsupported bootstrap registration.
- `CARGO_TARGET_DIR=/root/foton-target-task3 cargo build -p foton` under WSL root: passed (1m51s). No Rust source changed.
- Actual Paper/Foton startup differential: passed, including command execution and shutdown above.
- `git diff --check`: passed.
- Focused `typos` on bootstrap/lifecycle implementation and fixture: passed.

## Exact blockers and remaining scope

Task 6 is **not complete** and this is not general Paper bootstrap/registry certification.

The native pre-freeze seam exists: `foton/src/lib.rs` loads plugins before `Server::new_with_commands`, and `foton-core/src/bootstrap.rs` invokes `register_queued_plugin_enchantments` inside `init_vanilla_registry_with`. However, searching the entire Java/native bridge finds no consumer or JNI transfer from Java `PluginEnchantmentQueue` into Rust `PENDING_ENCHANTMENTS`. The queues are disconnected. The Rust queue has no plugin owner or rollback transaction, and registration drains it irreversibly. Its dynamic enchantment construction sets `effects_nbt` to `empty_effects_nbt`, so a bridge that only queues values would not prove packet-visible effect parity. Java's supposed Snapshot also retains a mutable builder delegate. Connecting those pieces would be a lossy, non-transactional shared foundation, not a safe phase hook.

Consequently no registry mutation/freeze callback is dispatched. Bootstrap registration for unsupported event types throws explicitly through both registration overloads. No Rust, generated data, extracted JSON, registry values or vanilla gameplay code was modified. A supported typed registry mutation fixture remains blocked until owner-scoped native transactions and the complete typed entry/packet encoding are implemented and verified against target source/extractor output.

Bootstrap dependency classpaths, cross-plugin bootstrap priorities/monitors, reload lifecycle, general registrar generics/source compatibility, command alias/flag parity, and arbitrary registry mutation are not claimed. The descriptor guard explicitly rejects bootstrap dependency declarations; the lifecycle manager explicitly rejects non-default bootstrap priority/monitor configuration. Later work must establish these behaviors with their own Paper differentials before widening support.

Disposable Java fixture directories clean themselves. Temporary orchestration scripts are removed before handoff; Paper/native evidence directories are retained outside the repository for independent review. No Desktop-root files were created.

## Independent-review follow-up: registration boundary

The review of `495c06dc4` found one P2: a retained BootstrapContext accepted handlers after bootstrap returned, although snapshot dispatch would never run them. The new regression first failed with `late registration accepted` during COMMANDS and `retained bootstrap context rejects late registration: 0 != 1`.

The runner now closes registration in a `finally` block immediately after the bootstrap callback exits. Both registration overloads check that state before accepting anything and throw `IllegalStateException("Cannot register lifecycle event handlers")`. Closing and registration share the same monitor, so a racing registration cannot pass its state check after closure. Dispatch snapshots under that monitor and runs callbacks outside it. Failed bootstrap callbacks also close their retained manager before owner cleanup.

`LateRegistrationProbe.java` was compiled with Java 25 against the actual build129 API, then its exact unchanged JAR ran on real Paper and Foton's Java host. In COMMANDS, createPlugin and onLoad, both the shorthand and configured overloads rejected registration with the exact exception class/message above. The seven normalized observations (six rejections followed by enabled) matched with `diff`. Fixture SHA-256: `2049ba67a3bdaca38dc27ca7f461cf1340d7d31fac1b6b136d2ca61b5fcc59a7`. Paper log: `/var/tmp/foton-task6-late-364z43v_/server.log`; Foton host log: `/var/tmp/foton-task6-late-foton.log`.

The full Java harness passed again, including Task4 and Task5. Focused tests passed with both this new Paper-compiled JAR and the unchanged original bootstrap/library/command fixture. Retained contexts after bootstrap and other pre-enable failures reject both overloads as well. Focused typos and `git diff --check` passed. No Rust or native-boundary code changed; the earlier native startup differential remains the native evidence, not a claimed new native run. Ownership-absence assertions do not constitute exhaustive resource-acquisition/rollback certification.

Task 6 remains incomplete for the registry/classpath foundations already listed. The temporary orchestration script was removed; external diagnostic logs and fixture JARs remain for review.
