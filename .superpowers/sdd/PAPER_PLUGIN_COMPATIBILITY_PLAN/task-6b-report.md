# Task 6b — Paper bootstrap dependency graph

## Result

Implemented a bounded BOOTSTRAP classpath/ordering slice in `PluginHost`. Discovery gives BOOTSTRAP all accepted JARs, independently of the later SERVER graph; the host builds eligible plugin classloaders before invoking any bootstrapper, then invokes BOOTSTRAP callbacks in Paper dependency order, followed by command callbacks and the SERVER construction/onLoad/enable passes. The classpath view switches from declared BOOTSTRAP providers to SERVER providers at `createPlugin`. An unsuccessful provider remains class-visible to later BOOTSTRAP callbacks, matching the pinned Paper failure fixture; Foton then closes its unpublished loader after the bootstrap command phase. A successful bootstrapper excluded by SERVER dependencies is also closed after preparation. Surviving loaders clear their temporary sibling map, and the tests assert these cleanup paths.

`has-open-classloader` and unsupported native registry lifecycle mutations remain fail-closed. No registry queue, JNI, Rust, generated data or target-version code was changed. This does **not** complete Task 6 or certify arbitrary Paper plugins.

## Oracle cases

The fixture generator `plugin-api/check/PaperBootstrapGraph.java --emit <temp-dir>` compiles its isolated provider and consumer sources against an explicitly selected API classpath. For the reference run, the classpath was the API and required Adventure jars extracted by the pinned Paper 26.2 build 129 server (SHA-256 `b1d8f6bfa1b6101fa8e947b53041cb3bdf5540e7b83b6547ca19ba7edefeb083`), under Java 25. No fixture classes were on the host classpath. The nine cases below were started on that pinned Paper binary and then exercised by the same test sources on Foton; the Paper-only failure-case processes were stopped by a timeout after startup, so their later shutdown is **not** claimed as evidence.

| Case | Observed Paper behavior | Foton focused assertion |
| --- | --- | --- |
| Required BOOTSTRAP provider, `join-classpath: true`; SERVER edge `AFTER` | Provider bootstrap, consumer sees provider class, consumer bootstrap; then consumer and provider construct/load/enable in that order | Same full phase order and visibility |
| Same but BOOTSTRAP `join-classpath: false` | Consumer **still sees** provider class during bootstrap | Same observed visibility; SERVER `join-classpath` behavior remains independent |
| No declared edge | Alphabetically earlier consumer bootstraps first and cannot see later provider class | Same |
| Optional missing BOOTSTRAP provider | Consumer bootstraps/loads/enables; class absent | Same |
| Required missing BOOTSTRAP provider | Paper reports `UnknownDependencyException`; consumer never bootstraps | Same absence, no published plugin |
| Required provider throws during bootstrap | Provider is not loaded, but dependent consumer still sees its class during bootstrap and then loads/enables | Same bootstrap visibility and consumer lifecycle; Foton also asserts failed loader closes after that phase |
| Required missing BOOTSTRAP edge on plugin with **no** bootstrapper | Paper loads/enables the plugin; BOOTSTRAP edge is not evaluated | Same |
| Bootstrapper with missing required **SERVER** dependency | Bootstrap callback runs; Paper then reports `UnknownDependencyException` and does not construct/enable the plugin | Same, and Foton verifies the unpublished bootstrap loader is closed |
| Required BOOTSTRAP edge to a Paper provider **without** bootstrapper | Paper reports that provider as a missing BOOTSTRAP dependency and rejects consumer; plain provider constructs/loads/enables | Same |

The `join-classpath: false` BOOTSTRAP result is a direct observation, not an inference from the documentation. The consumer JAR contains only `Consumer` classes, not `Provider$Marker`; the marker is absent from the API parent. Paper's [plugin guide](https://docs.papermc.io/paper/dev/getting-started/paper-plugins/) describes `join-classpath` as access control, and this pinned-build behavior deserves a broader matrix before generalizing it to other versions or graph shapes.

## Real server comparison

The generator emits a checksum-pinned `oracle-required-joined.json` with both exact JAR paths and SHA-256 values. `CARGO_TARGET_DIR=/root/foton-target-task3 bash dev/paper-oracle-test.sh /var/tmp/foton-bootstrap-graph-cases/oracle-required-joined.json` passed against Paper 26.2 build 129 and a freshly built Foton API JAR, using the existing native Foton binary. Final evidence after the review fix: `/tmp/foton-paper-oracle-20260927T195211Z-0ktx10bk/` (WSL). Provider JAR SHA-256 `0a7dc3bf42d73ef6aae0628528f6c5e00f9c77f68e7bde3388913294fad787e1`; consumer JAR SHA-256 `09991feb883ae7c043ef96ba0b4e7babdaee69d41a1bd424850b2bba00c08c42`. Both copied inputs were rehashed by the runner. The parsed `ORACLE:bootstrap_graph.phase` and `ORACLE:lifecycle.enabled` event sequences matched exactly: provider bootstrap, consumer visibility/bootstrap, consumer/provider construction and onLoad, consumer/provider enable, reverse disable. Both servers answered protocol 776 and stopped cleanly. This is a phase-contract proof for this fixture, not gameplay or market-plugin certification.

## Checks

- Red before implementation: the new two-plugin check failed because Foton rejected BOOTSTRAP dependencies, leaving only the provider loaded.
- Focused check after implementation: passed with Foton-compiled fixtures and unchanged JARs compiled against pinned Paper API.
- `bash dev/build-plugin-api.sh --check`: passed with `PaperBootstrapGraph.check()` in the full Java harness, after the final code and fixture-marker changes.
- Focused `typos` and `git diff --check`: passed.
- Full `bash dev/ci.sh`: not run in this subtask; parent will run after independent review.

## Known limits / next evidence

- The Paper failure fixture proves a failed provider's class is available *during dependent bootstrap*. Foton releases that provider loader after the command phase; Paper's subsequent failed-provider class/resource lifetime was not measured. A server-phase reference to such a provider must be tested before claiming full parity.
- Resource lookup through joined dependency classloaders and transitive/alias BOOTSTRAP visibility are not covered by this fixture. `PluginClassLoader.getResource` currently searches own and library URLs only; that is a separate gap requiring a Paper differential before changing it.
- A provider whose classpath construction fails (distinct from its bootstrap callback throwing) has no oracle fixture yet.
- Native registry transaction/freeze and complete client-visible enchantment effects remain separate foundational work. The current Java queue is disconnected from Rust, so this slice deliberately does not claim registry mutation support.
