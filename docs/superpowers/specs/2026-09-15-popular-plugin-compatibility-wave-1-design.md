# Popular Plugin Compatibility — Wave 1 Design

## Classification and objective

This is an architectural change. It extends the Java plugin loading contract,
crosses descriptor parsing, class loading, lifecycle, dependency ordering and
compatibility reporting, and therefore cannot be treated as a collection of
isolated API stubs.

The objective of this first wave is to make more widely used Bukkit and Paper
plugins reach a correct `onEnable` state without adding work to Foton's game
tick. It deliberately targets startup-only compatibility before adding more
gameplay events, because `minecraft-src/` is not available in this checkout and
Foton must not guess Vanilla-observable behavior.

## Baseline

The committed 59-plugin corpus gives three different measurements:

- 2,478 of 2,487 API members referenced by at least two plugins resolve in the
  built API jar.
- 18 of 59 plugins reach into NMS or CraftBukkit internals and cannot run on a
  Rust reimplementation without shipping or emulating another server.
- The current analyzer reports 113 of 199 listened-for event types as
  constructible by the Java bridge. This is an optimistic bound: a Java
  `fireX` method can exist without a Rust caller, so symbol coverage and the
  current event number both overstate runtime compatibility.

The theoretical public-API ceiling in this corpus is 41 of 59 plugins. This
wave does not claim that all 41 run; it removes high-impact loader failures and
adds evidence that distinguishes loading from actual behavior.

## Approaches considered

### Selected: corpus-driven loader compatibility

Implement the modern descriptor and lifecycle behavior required before a
plugin can start: legacy aliases and load ordering, declared libraries, and the
Paper bootstrap phase. Exercise each behavior with real fixture jars and make
the compatibility tool report separate binary, event and boot evidence.

This approach has no steady-state tick cost. All added work happens during
descriptor discovery, dependency sorting or class loading. It also unlocks
plugins whose runtime API calls Foton already supports.

### Rejected for this wave: add silent gameplay events first

The event gap is real and will be the next wave, but redstone, physics,
structure growth and inventory transfer require exact dispatch points and
mutation semantics. Implementing them without the target Vanilla source would
violate the project's parity rules.

### Rejected: emulate NMS/CraftBukkit

This would chase the unreachable 18-plugin slice by recreating implementation
internals, couple Foton to another server's private ABI and threaten both
performance and maintainability. Compatibility remains limited to public
Bukkit/Paper APIs.

## Scope

### 1. Descriptor model

`PluginDescriptionFile` will parse and expose the fields needed by the loader:

- legacy `loadbefore`, `provides` and `libraries`;
- Paper `bootstrapper` and `loader` class names;
- Paper `dependencies.bootstrap` and `dependencies.server` entries with their
  `load`, `required` and `join-classpath` values.

Legacy getters retain immutable collections. Paper dependency data receives a
small immutable value type owned by the descriptor rather than untyped maps.
Malformed dependency entries fail descriptor parsing with a precise message;
optional absent fields remain empty.

The target Minecraft/API version remains 26.2. A descriptor requiring a newer
API version is rejected before plugin construction; an omitted API version is
accepted as a legacy plugin and reported as such.

### 2. Transactional lifecycle

Discovery, construction, `onLoad` and `onEnable` form explicit states rather
than a partially updated set of global collections. A plugin is visible through
`PluginManager#getPlugin` and `JavaPlugin.getPlugin` during its own `onLoad`, as
Bukkit consumers expect, but a failure rolls the state back completely.

Rollback removes commands, listeners, services, messaging channels, scheduled
tasks and classloader registrations created by the failed plugin. A dependent
plugin is not loaded when a required dependency failed during `onLoad`, and is
not enabled when that dependency failed during `onEnable`.

Disable ordering is `enabled=false`, then `onDisable`, then unconditional
resource cleanup and `PluginDisableEvent`. Synchronous and asynchronous tasks
owned by the plugin are cancelled even when `onDisable` throws. An exception in
one plugin never skips cleanup or prevents the remaining plugins from shutting
down.

Disabling the host also unregisters Foton's plugin-owned subscriptions from the
Rust event bus. No tick closure may keep the server alive after plugin shutdown.
Repeated bind/disable operations are idempotent and cannot duplicate event
forwarding.

### 3. Deterministic dependency graph

Plugin discovery will resolve names case-insensitively and recognize aliases
declared through `provides`. Exactly one provider may own an alias; duplicate
real names or aliases fail deterministically.

The load graph will honor:

- legacy `depend`, `softdepend` and `loadbefore`;
- Paper bootstrap and server dependency directions;
- `required` when deciding whether a missing dependency rejects a plugin;
- `join-classpath` when deciding which dependency classloaders are visible.

Required dependency cycles reject every member of the cycle with a diagnostic.
Optional ordering edges may be dropped only when necessary to break an
otherwise optional cycle, and the dropped edge is logged. Alphabetical jar
ordering remains the deterministic tie-breaker.

`PluginManager#getPlugin` and dependency lookup will resolve a provided alias
to its provider. The provider's real plugin name remains the lifecycle and log
identity.

### 4. Library resolution and runtime classpath

The existing `.foton-libraries` cache will also resolve `libraries` declared in
legacy `plugin.yml`. Maven coordinates are limited to `group:artifact:version`
with the same conservative character set already accepted for
`paper-libraries.json`. Paths are derived from validated coordinate segments;
no coordinate may escape the cache directory.

Downloads use Maven Central over HTTPS with finite connect/read timeouts and an
exclusive temporary file followed by atomic publication. A failed or truncated
download does not replace an existing cached jar. Concurrent plugins requesting
the same coordinate converge on one cache artifact.

Library resolution occurs before the plugin class is loaded. Failure rejects
that plugin with the coordinate and cause instead of allowing a later
`NoClassDefFoundError`. Foton does not implement arbitrary repository URLs or
an unverified Maven resolver in this wave.

When `FOTON_PLUGIN_LIBRARY_DIRECTORY` is absent, the server derives the pinned
runtime library directory beside the configured API jar. Startup validates the
same digest-pinned set used to build the API. The minimal configuration shown in
the README therefore provides Adventure, Brigadier, Gson, Guava and the other
types present in public signatures without requiring a hidden environment
variable.

### 5. Paper bootstrap lifecycle

For a descriptor with `bootstrapper`, the host will:

1. create the plugin classloader with descriptor and resolved dependencies;
2. instantiate the declared `PluginBootstrap` implementation;
3. create a concrete `BootstrapContext` carrying immutable metadata, data
   directory, logger, source jar and a lifecycle event manager;
4. invoke `bootstrap(context)` before constructing the main plugin;
5. invoke `createPlugin(context)` and use its non-null result, otherwise fall
   back to the main class no-argument constructor;
6. transfer bootstrap lifecycle registrations needed for command registration
   to the plugin's lifecycle manager before `onEnable`.

A class that does not implement `PluginBootstrap`, a wrong `createPlugin`
result or an exception in bootstrap rejects only that plugin. No exception may
cross JNI or prevent unrelated plugins from loading.

The custom Paper `loader` hook is parsed and diagnosed but remains unsupported
until its classpath-builder API can be implemented completely. A plugin that
requires it is rejected early with an actionable error; Foton does not pretend
the hook ran.

### 6. Compatibility evidence

The Java fixture suite will contain independent jars for:

- a legacy provider/consumer pair exercising `provides`, `depend` and
  `loadbefore`;
- a required dependency that fails in `onLoad` and one that fails in
  `onEnable`, proving dependants never start;
- a plugin that registers commands, listeners and tasks before failing,
  proving rollback and disable cleanup leave no orphaned resources;
- a plugin using a declared library from a pre-populated local cache, so tests
  never depend on the network;
- a Paper plugin whose bootstrapper registers a lifecycle command and returns
  a custom plugin instance.

The compatibility analyzer will expose a machine-readable summary containing:

- corpus size and NMS/CraftBukkit ceiling;
- referenced and resolved public API members;
- listened-for and emitted event types;
- fixture jars discovered, loaded, enabled and rejected, with reasons.

The analyzer will retain the complete long tail rather than only members shared
by two plugins, track member-to-plugin incidence, include class-only references
and distinguish optional adapters from load-bearing references. Event coverage
uses fully qualified class names and counts an event as emitted only when a
reachable Rust bridge call invokes the Java constructor path. Merely declaring
an event class or a Java `fireX` method is not sufficient.

The human output must keep the caveat beside every percentage: API symbol
coverage is not a plugin success rate. CI will verify the built-in fixtures and
the structure of the summary, but it will not download third-party plugins.

## Performance contract

- With no plugin directory configured, no JVM or plugin compatibility code is
  loaded, preserving the existing zero-cost path.
- This wave adds no subscription, serialization, JNI call, lock or allocation
  to the game tick.
- Descriptor parsing, graph construction, bootstrap and library resolution are
  startup-only.
- Network access is never performed from the game tick and always has bounded
  timeouts.
- A later event wave must add listener-interest gating before introducing any
  high-frequency event bridge.

## Error handling

Every plugin is an isolation boundary. A malformed descriptor, missing
required dependency, unavailable library, invalid bootstrapper or unsupported
custom loader rejects that plugin with one causal diagnostic. Independent
plugins continue loading. Temporary downloads are recoverable and never become
visible as complete cache entries.

## Testing and acceptance

Each behavior is developed test-first. The narrow Java fixture check must fail
for the missing behavior before implementation and pass afterward.

Acceptance requires:

- all new loader fixtures pass with no network access;
- `python3 dev/plugin_api_usage.py --covered ...` still reports at least 2,478
  resolved members and identifies all event gaps honestly;
- `bash dev/build-plugin-api.sh --check` passes;
- native binding parity passes;
- `cargo test --workspace` passes;
- `cargo clippy -r --workspace --all-targets --all-features -- -D warnings`
  passes;
- `bash dev/ci.sh` reports `ALL GREEN` on the final branch.

## Follow-up waves

Wave 2 will fix the highest-audience runtime foundations: permission state keyed
by player identity, command dispatch through Foton, and an immutable published
Java handler index. It will mirror listener interests into dynamically installed
Rust subscriptions and make the event-bus read path snapshot-based. Only then
will it implement the highest-audience missing events whose exact dispatch
semantics can be verified against Minecraft 26.2 source. Wave 3 will run a
refreshed, legally acquired popular-plugin corpus end to end and turn concrete
failures into ranked, test-first compatibility tasks. Neither wave will emulate
NMS/CraftBukkit.
