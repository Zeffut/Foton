# Paper plugin compatibility: product specification

Date: 2026-09-27
Owner: Foton
Target server: the Minecraft version in `[workspace.package].version` in `Cargo.toml` (`0.15.2+mc26.2` when this specification was written).

## Goal and meaning of 100%

An unchanged plugin JAR which works on Paper for the declared server/API version must have the same observable behavior on Foton for its exercised features. That includes startup, dependencies, class loading, permissions, commands, events, game state, persistence, packets, failure handling and shutdown. `paper-plugin.yml`, legacy `plugin.yml`, optional integrations and plugins that reach CraftBukkit or Mojang internals are in scope. Client version translation through ViaVersion/ViaBackwards is a separate axis of the test matrix.

There is no finite, static list of every existing and future plugin or private reflection target. Consequently, a claim of “100% of the market” is made only for a dated, versioned, reproducible certification corpus. The engineering target is broader: complete public Paper API behavior for the selected server version, a versioned internal compatibility layer for the internals used by the corpus, and a process that incorporates new plugins and upstream releases continuously. A plugin counts as compatible only after its real JAR passes its behavioral matrix; a matching Java method signature or successful `onEnable` is insufficient.

## Current measured baseline

These are observations, not acceptance results. Re-measure at the start of execution because the current branch contains uncommitted work.

| Measurement | Current observation |
| --- | --- |
| Shared public API ledger | 2,478 / 2,487 members used by at least two of 59 scanned plugins are present in the built API JAR; nine are absent. |
| Entire API corpus | 5,973 distinct public members referenced; the shared ledger excludes one-plugin calls. |
| Internal reach | 18 / 59 plugins reference `net.minecraft` or CraftBukkit. |
| Observer 2.69.3 | 30 of 393 referenced public members missing; its core detection needs PacketEvents 2.13.0. |
| Zelda Civ 0.1.0 | 241 of 1,144 referenced public members missing in each of three build variants; these variants are one product, not three independent plugins. |
| Event delivery | A previous audit counted 199 referenced event types, 113 potentially constructed and 86 silent; the numbers need a fresh scanner run and live confirmation. |
| Existing proven cross-version path | `dev/via-test.sh` exercised an unchanged ViaVersion/ViaBackwards 5.11.0 pair with a 1.21.11 client and a native 26.2 client. This does not establish Paper plugin compatibility. |

The first two per-plugin numbers come from `python dev/plugin_api_usage.py <JAR-directory> --gap plugin-api/build/foton-plugin-api.jar`; the shared number comes from `--covered`. The 59-JAR corpus is not checked in. Its manifest must record provenance, license and SHA-256 without redistributing JARs that may not be redistributed.

## Supported versions and comparison oracle

1. The authoritative Foton Minecraft version is the `+mc...` suffix in `Cargo.toml`; `minecraft-src/` and extracted data must match it before gameplay or protocol implementation.
2. The primary Paper oracle is a pinned Paper build for that same Minecraft version. Record the Paper build, API JAR and Java version in the certification manifest. Paper 26.2 requires Java 25; the historical Paper 1.21.11 environment uses Java 21 and is a second oracle for the older binary assumptions of Observer and Zelda Civ, not a substitute for the 26.2 comparison. Foton's compatibility runtime must accept Java 25 plugin bytecode before claiming Paper 26.2 parity.
3. A plugin's `api-version` is checked against the target. Historical Bukkit/Paper aliases and remapping are supported only when their behavior can be defined and tested. An unknown mapping fails with an actionable diagnostic instead of silently advertising compatibility.
4. Clients 1.21.11/protocol 774 and native 26.2/protocol 776 remain in the integration matrix while Foton targets 26.2. Future target or protocol changes require a new matrix row and proof.
5. Paper APIs that are explicitly version-specific, such as data components, are versioned with the target server. An old plugin is not promised every newer API, and a future plugin is not promised behavior absent from its declared Paper version.

## Architecture contract

- Keep Foton's Rust game state authoritative. Bukkit/Paper Java objects are stable-identity views of that state with explicit JNI services; no constant fake answers for player state, collision, ticks, permissions or inventory.
- Preserve vanilla-observable gameplay and protocol outcomes by checking `minecraft-src/` and FotonExtractor output. Do not edit generated Rust or extracted JSON manually.
- Treat Paper descriptors and legacy descriptors as different formats. Resolve bootstrap and server dependency graphs, classpath visibility, libraries, bootstrapper, lifecycle events, `createPlugin`, `onLoad`, `onEnable` and teardown in their specified order. Failure before publication leaves no registered commands, handlers or loaders behind.
- Keep per-plugin classpaths isolated, with a documented shared-API parent and explicit dependency visibility. Library resolution is bounded, cacheable and integrity-checked; it never accepts path traversal or an incomplete cached JAR.
- For each event, trace the native cause, Paper timing/thread, fields, cancellation and write-back. A Java event class without a native emission path is not counted as delivered.
- Expose packet hooks over a real ordered Netty pipeline that owns decoding, framing, compression, encryption, protocol state and lifecycle. PacketEvents and Via must be exercised together with unchanged official JARs; handler order and buffer ownership are test assertions.
- Implement Mojang/CraftBukkit internals as a versioned compatibility module with actual Rust-backed behavior and object identity. It is not a general promise of ABI stability across Minecraft versions. Each target upgrade regenerates an inventory and reruns internal tests. Do not bundle Mojang's server JAR as a substitute for Foton.
- Respect third-party licenses. PacketEvents is supplied as an operator plugin or separately under its own terms; the Foton runtime must not silently redistribute it or proprietary Observer/Zelda binaries.

## Certification contract

For each corpus entry, record: plugin name/version, SHA-256, descriptor type, declared server/API range, dependencies and their checksums, whether internals/reflection are used, Paper oracle build, tested Foton build, test scenarios, required companion plugins, result and evidence location. Discover runtime reflection and service loading in addition to constant-pool member references.

The per-plugin pass requires all of the following:

1. The unchanged plugin and declared dependencies load and enable in the expected order, and the server remains healthy when an optional dependency is missing.
2. A scenario suite exercises every feature the certification claims. Compare externally visible results with the matching Paper oracle: messages, packets, world and item state, events, persistence and restart results. The suite must detect both missing callbacks and incorrect extra callbacks.
3. Enabled plugins can be disabled and the server stopped without lingering Netty handlers, tasks, class loaders, permissions, commands, listeners or JNI references.
4. Co-installation scenarios cover shared providers, classpath isolation, permissions, event priority, PacketEvents with ViaVersion/ViaBackwards and conflicts in the corpus.
5. No `NoClassDefFoundError`, `NoSuchMethodError`, `NoSuchFieldError`, `ClassCastException`, JNI error, decoder/encoder error, buffer reference-count error or unexplained plugin exception occurs in the exercised scenarios.

A certification report has separate columns for binary linkage, load/enable, event delivery, functional scenario coverage and co-installation. A single percentage may be shown only for the number of corpus plugins passing the entire contract; API coverage is never reported as plugin compatibility.

## Priority acceptance cases

### Observer

Install the unchanged Observer 2.69.3 JAR and unchanged PacketEvents 2.13.0 Spigot JAR. Verify the PacketEvents dependency activates before Observer; its packet listener receives real inbound/outbound traffic in correct order; Observer's login, movement, combat, inventory and violation actions match Paper without false positives in legitimate movement scenarios. Test PacketEvents absent/restart behavior in an isolated disposable server because Observer can request a server stop and an update download. The remaining 30 ABI gaps, reflected NMS/CraftBukkit access and real player state are explicit work items, not exceptions to the pass criterion.

### Zelda Civ

Install one selected, pinned Zelda Civ 0.1.0 release JAR, then separately the shaded variant if users distribute it. Verify domain/team display, item PDC after save/restart, custom inventory lifecycle, player/entity events, display entities, merchants, trims/data components, recipes, mounts, region interactions and optional integration behavior against Paper. The 241 remaining binary gaps must reach zero for the selected JAR before declaring linkage complete. Dirty files in the Zelda checkout are user work and are not modified by this program.

### Market corpus

Freeze the existing 59-plugin corpus with a manifest and add current high-use Paper plugins from categories including permissions, economy, world editing, protection, anticheat, maps, inventories, chat, packets, world generation and cross-version support. Each independently distributed variant is a separate artifact row; variants of one product are grouped for reporting. A dated corpus passes only when every supported entry passes its contract on the target matrix. New releases and newly discovered plugins reopen certification rather than inheriting a prior green label.

## Global release gate

The local static suite (`bash dev/ci.sh`), in-world suite (`bash dev/all-tests.sh`), real client join, plugin fixture suite, real plugin scenarios, Via test and differential Paper comparison must pass in CI for the release's exact commit. Windows Application Control can block locally linked Rust test executables with OS error 4551; execute Rust gates under WSL with a Linux-only `CARGO_TARGET_DIR` or on CI rather than weakening the gate. Keep downloadable plugin JARs and temporary worlds in the platform temp area, never at the Desktop root.

## Source references

- Paper descriptors and loading: https://docs.papermc.io/paper/dev/plugin-yml/ and https://docs.papermc.io/paper/dev/getting-started/paper-plugins/
- Paper runtime Java requirements: https://docs.papermc.io/paper/getting-started/
- Paper lifecycle and registry mutation: https://docs.papermc.io/paper/dev/lifecycle/ and https://docs.papermc.io/paper/dev/registries/
- Version-specific data components: https://docs.papermc.io/paper/dev/data-component-api/
- Paper internal mapping and remapping rules: https://docs.papermc.io/paper/dev/userdev/
- PacketEvents source and license: https://github.com/retrooper/packetevents
