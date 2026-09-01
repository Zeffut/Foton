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

**Everything about the project is on the website:** what it does, how to
install it, every configuration key, and how to work on it.

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
