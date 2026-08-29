<p align="center">
  <img src=".github/assets/readme/foton-logo.png" alt="Foton logo" width="192">
</p>

<h1 align="center">Foton</h1>

<p align="center">
  A Minecraft Java Edition server written in Rust.
</p>

<p align="center">
  <a href="LICENSE"><img alt="AGPL-3.0-or-later license" src="https://img.shields.io/badge/license-AGPL--3.0--or--later-blue?style=flat-square"></a>
  <img alt="Minecraft 26.2" src="https://img.shields.io/badge/minecraft-26.2-brightgreen?style=flat-square">
  <img alt="Rust nightly" src="https://img.shields.io/badge/rust-nightly--2026--07--23-orange?style=flat-square">
</p>

![A sunset over a Foton-generated world, with forests, rivers, mountains, and a lit village](.github/assets/readme/sunset.webp)

> [!IMPORTANT]
> Foton is still pre-alpha. You can connect and explore generated worlds, but
> survival gameplay is incomplete and many vanilla systems are still missing. Do not
> replace your production server with it yet.

## What is Foton?

Foton is an independent implementation of the Minecraft Java Edition server. It
tracks the latest Java Edition release and currently targets **Minecraft 26.2**.

The goal is to match vanilla behavior 1:1 while making better use of modern
multicore hardware. Gameplay updates remain synchronous, while chunk generation,
lighting, packet processing, and chunk sending can run outside the main tick.

Foton started as a fork of [SteelMC](https://github.com/Steel-Foundation/SteelMC)
and is being progressively rewritten into a sovereign, self-contained codebase. It
remains free software under the AGPL-3.0-or-later, as required by its origin.

## World generation

World generation is currently the most complete part of Foton. Its parity suite
compares 7,500 randomly selected chunks with a reproducible vanilla reference: 2,500
in each dimension. All tested chunks match block for block. Entity spawning is not
included because most entity behavior has not been implemented yet.

In a focused benchmark on a Ryzen 9 9950X, the generator produced a fresh
10,201-chunk Overworld area in a median of 3.98 seconds. Results vary with hardware,
and this benchmark does not represent every server workload.

## Current status

Today, clients can join a persistent multiplayer world, move and interact, use
inventories and commands, and return later to saved chunks. Foton currently
provides:

- Java Edition networking, authentication, encryption, and compression
- Persistent chunk generation, loading, saving, and lighting
- Player movement, collision, block interaction, and inventories
- Commands, permissions, chat, and server configuration
- Early entity, block entity, and gameplay behavior implementations

Foton is not ready to replace an established server:

- Survival gameplay is incomplete.
- Only a small number of entities have meaningful behavior.
- Full vanilla and protocol parity have not been reached.
- Plugins are not available yet.
- Paper, Bukkit, Fabric, Forge, and NeoForge extensions are not compatible.

`python3 dev/coverage.py` reports the current block, item, and entity coverage
against vanilla; `PARITY.md` records the detailed state of the work.

## Build and run

The repository uses a pinned nightly Rust toolchain (`rust-toolchain.toml`).

```bash
cargo run -p foton              # start the server
cargo build --release -p foton  # release binary at target/release/foton
```

Development helpers:

```bash
bash dev/doctor.sh          # check the environment is complete and coherent
bash dev/ci.sh              # replay the whole verification suite
bash dev/smoke-test.sh      # boot the server and speak Minecraft protocol to it
bash dev/join-test.sh       # take a real client from login to play
python3 dev/coverage.py     # measure real coverage against vanilla
```

## Working on Foton

Most changes begin by reading the vanilla source, understanding the behavior it
implements, and expressing that behavior cleanly in Rust.

1. Generate the targeted vanilla source with `./update-minecraft-src.sh` and verify
   behavior against it — never implement from memory.
2. Take registry, block, item, and worldgen data from the extractor, never from a
   manual transcription.
3. Run the relevant tests and checks before merging.

The common validation commands are:

```bash
cargo check --workspace --all-targets
cargo test
cargo fmt --all --check
cargo clippy -r --all-targets --all-features
typos
```

Engineering rules live in `AGENTS.md`; project-specific instructions live in
`CLAUDE.md`.

## License

Foton is free software licensed under the
[GNU Affero General Public License v3.0 or later](LICENSE).

It derives from [SteelMC](https://github.com/Steel-Foundation/SteelMC), also
AGPL-3.0-or-later. Copyright notices are preserved and modifications are marked as
required by the license. The **network clause** applies: anyone interacting with a
server running this code over a network is entitled to its source.

The logo was designed by **colonthreeing** for SteelMC.

## Acknowledgements

The world generation, lighting, and other performance work has drawn ideas from
[C2ME](https://github.com/RelativityMC/C2ME-fabric),
[ScalableLux](https://github.com/RelativityMC/ScalableLux),
[FastNoise](https://codeberg.org/ZenXArch/FastNoise),
[Lithium](https://github.com/CaffeineMC/lithium), and
[Structure Layout Optimizer](https://github.com/TelepathicGrunt/StructureLayoutOptimizer).
