<p align="center">
  <img src=".github/assets/readme/foton-logo.png" alt="Foton" width="160">
</p>

<h1 align="center">Foton</h1>

<p align="center"><em>A Minecraft Java Edition server, written in Rust, that refuses to guess.</em></p>

<p align="center"><a href="https://foton.zeffut.fr"><strong>foton.zeffut.fr</strong></a></p>

---

Foton is an independent implementation of the Minecraft Java Edition server.
Everything a player can observe is written against the decompiled vanilla
source, and every registry, block, item and worldgen value comes out of an
extractor rather than being transcribed by hand.

**The website covers operating Foton:** what it does, how to install it, and
every configuration key.

## Install

```
curl -fsSL https://foton.zeffut.fr/install.sh | sh
```

On Windows, in PowerShell:

```powershell
irm https://foton.zeffut.fr/install.ps1 | iex
```

Or from source: `cargo run -p foton`.

## Contributing

The engineering rules are in [`AGENTS.md`](AGENTS.md), the bar a change has to
clear is in [`CONTRIBUTING.md`](CONTRIBUTING.md), and
[`PARITY.md`](PARITY.md) is the honest inventory of where vanilla parity
actually stands — it opens by explaining how its own measurement used to lie.

## Bedrock players

Foton runs a [Geyser](https://geysermc.org) of its own, so Bedrock Edition
players — phone, console, Switch — can join a Foton server with no Java
account at all, on the **same port number** as Java players by default
(`bedrock.port = 0`; TCP and UDP don't share a namespace, so this isn't a
collision). Set `bedrock.enable = true` and restart; Foton fetches, configures,
runs and supervises Geyser for you.

**The translation is Geyser's, not Foton's.** A Bedrock player experiences the
Java server exactly as Geyser presents it, and some things do not translate —
this is the same trade-off every Geyser-fronted server makes, not something
Foton's own code can fix. `dev/bedrock-test.sh` covers the identity guarantee
(a stable, prefixed name and a UUID that survives reconnecting) end to end;
`PARITY.md` is still the source of truth for what a Java client sees, and a
Bedrock client sees exactly that, filtered through Geyser.

## Player reports

Reports sent from the game become GitHub issues and their GitHub status is
shown on the public reports page. Deployment and webhook setup are documented
in [`REPORTING.md`](REPORTING.md).


## Java/Paper plugins

The optional Bukkit/Paper compatibility host is enabled only when both the
plugin directory and Java runtime are configured:

```sh
FOTON_PLUGIN_DIRECTORY=./plugins
FOTON_JAVA_HOME=/path/to/jdk
cargo run
```

The JVM must be **Java 21 or newer** -- the API jar is compiled with
`--release 21`, matching what Paper 26.2 itself requires.

The API jar defaults to `plugin-api/build/foton-plugin-api.jar`; its bundled,
digest-pinned `plugin-api/lib` runtime dependency directory is selected
automatically. Override the jar with `FOTON_PLUGIN_API_JAR`, or override only
its runtime dependencies with `FOTON_PLUGIN_LIBRARY_DIRECTORY`. The override
must contain exactly the same filenames and SHA-256 digests as
`plugin-api/lib/manifest.txt`; missing, extra and changed jars are rejected
before the JVM starts. With no `FOTON_PLUGIN_DIRECTORY`, no JVM is started and
the normal server path is unchanged.

Compatibility evidence is regenerated and combined with:

```sh
bash dev/build-plugin-api.sh --check
python3 dev/plugin_api_usage.py --covered plugin-api/build/foton-plugin-api.jar
python3 dev/plugin_api_usage.py --covered plugin-api/build/foton-plugin-api.jar --events plugin-api/src
python3 dev/plugin_compatibility.py --api-report build/plugin-api-evidence.json --fixture-report plugin-api/build/fixture-evidence.json --output build/plugin-compatibility.json
```

The combined JSON deliberately separates binary symbols, the corpus ceiling,
Rust-reachable events and executable fixtures. Shared symbol coverage is not a
plugin success rate. The current legacy corpus ledger has no per-plugin class
incidence or entrypoint reachability, so its present ceiling is `null`; its old
41-of-59 public-API figure is retained only as historical evidence. A v2 corpus
regeneration distinguishes load-bearing internal references from optional
adapters. An event counts only when an exact Rust JNI owner, method and
descriptor resolves in the compiled Java API and reaches its construction path.
The executable fixture report covers 49 jars across all eight suites; it loads
29, enables 27 and rejects 22 with phase-specific causal diagnostics.

## License

Foton is free software under the
[GNU Affero General Public License v3.0 or later](LICENSE).

It began as a fork of [SteelMC](https://github.com/Steel-Foundation/SteelMC)
and has been modified substantially since August 2026; the original copyright
notice is preserved in [`LICENSE`](LICENSE). The **network clause** is the part that surprises people:
run this code where other people can reach it, and they are entitled to its
source.

## Prior art

Four projects are named in the source because a system here is built on their
approach:

- **[ScalableLux](https://github.com/RelativityMC/ScalableLux)** and the
  Starlight engine behind it — the light propagation in
  `foton-core/src/chunk/light/`.
- **[C2ME](https://github.com/RelativityMC/C2ME-fabric)** — the density-function
  transpiler in `foton-worldgen/build/density/`.
- **[Structure Layout Optimizer](https://github.com/TelepathicGrunt/StructureLayoutOptimizer)**
  — the jigsaw bounds octree in `foton-worldgen/src/structure/box_octree.rs`.
- **[FastNoise](https://codeberg.org/ZenXArch/FastNoise)** — the write-only fill
  mode on paletted chunk sections.
