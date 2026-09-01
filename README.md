<p align="center">
  <img src=".github/assets/readme/foton-logo.png" alt="Foton" width="200">
</p>

<h1 align="center">Foton</h1>

<p align="center"><em>A Minecraft Java Edition server, written in Rust, that refuses to guess.</em></p>

<p align="center">
  <img alt="Minecraft 26.2" src="https://img.shields.io/badge/Minecraft-26.2-5b8a3c?style=flat-square">
  <img alt="Protocol 776" src="https://img.shields.io/badge/protocol-776-4a6fa5?style=flat-square">
  <img alt="Rust nightly" src="https://img.shields.io/badge/Rust-nightly--2026--07--23-b7410e?style=flat-square">
  <img alt="AGPL-3.0-or-later" src="https://img.shields.io/badge/license-AGPL--3.0--or--later-6c4a8a?style=flat-square">
</p>

---

Foton is an independent implementation of the Minecraft Java Edition server. It
speaks protocol 776 and targets **Minecraft 26.2**. Two commitments shape every
decision in it.

**Vanilla parity is the specification, not the goal.** Anything a player can
observe — gameplay, protocol, registries, world generation — is written against
the decompiled vanilla source, and every registry, block, item and worldgen
value comes out of an extractor. Nothing is implemented from memory, and no
value is transcribed by hand. When a faster or more idiomatic Rust design would
change observable behavior, the observable behavior wins.

**Concurrency, but only where vanilla allows it.** Gameplay ticks stay
synchronous, because vanilla semantics depend on the order things happen in.
Chunk generation, lighting, packet processing and chunk sending do not, and they
run off the tick.

> [!IMPORTANT]
> Foton is pre-alpha. You can connect, build, and come back to a saved world,
> but survival gameplay is incomplete. Do not put it in front of a community you
> care about yet.

## Where it stands

Coverage of the vanilla behavior classes, cross-checked between `dev/coverage.py`
and the ledger the build generates in `dev/parity-gaps.txt`:

| | Covered | Missing | What is left |
|---|---|---|---|
| **Blocks** | 255 / 265 · 96 % | 10 | four vanilla base classes, glass, crying obsidian, the structure void, two gametest blocks |
| **Items** | 69 / 70 · 99 % | 1 | vanilla's plain `Item`, which needs no behavior |
| **Entities** | 141 / 142 · 99 % | 1 | `Player`, handled outside the behavior mechanism |

Those numbers say a behavior is *registered*. They say nothing about whether it
is *correct* — and the two have come apart here before, in ways that took real
work to find. **[`PARITY.md`](PARITY.md) is the honest document**, and it opens
by explaining how this very measurement used to lie.

## How it is checked

Two layers, because a unit test cannot see that a furnace has no behavior.

**5,287 tests** across 18 targets, run by `cargo test --workspace` — unit,
integration and doc tests together. They cover the places where being wrong is
silent: component hashing, seeded RNG determinism, protocol encoding, loot
table evaluation, permission resolution.

**77 in-world scripts** in `dev/`, each of which boots a real server on its own
port and talks to it over the Minecraft protocol. They are the layer that
catches what compiles and still does not work:

```
advancement  beacon  beehive  boat  bonemeal  bossbar  campfire  conduit
container  creaking-ai  death  dispenser  dragon  dripstone  enderchest
fire  fishing  frame  function  furnace-minecart  grass  happy-ghast  hopper
interact  jigsaw  join  jukebox  leash  lightning  locate-biome  loot-pickup
map  melee  minecart  mob-persist  mount  nether  raid  rcon  reload  respawn
ride  sapling  scaffolding  sculk-vibration  spawner  statistics  structure-block
summon  tnt  villager-day  warden  workstation  …
```

`bash dev/all-tests.sh` runs every one of them in sequence. `bash dev/ci.sh`
runs the full verification suite: formatting, spelling, the generated-docs
check, clippy with `-D warnings`, and the tests.

## Run it

```bash
cargo run -p foton                 # boots, writes config/, generates a world
cargo build --release -p foton     # binary at target/release/foton
```

First boot writes `config/config.toml`, `config/worlds.toml`,
`config/groups.toml` and `.logs/` beside the binary. Every key, default and
range is documented in **[CONFIGURATION.md](CONFIGURATION.md)**, generated from
the JSON schemas the server validates against.

The `Dockerfile` builds a scratch image from source:

```bash
docker build -t foton .
docker run -p 25565:25565 -v ./config:/config -v ./saves:/saves foton
```

## Work on it

The toolchain is pinned by `rust-toolchain.toml` and `rustup` picks it up. The
one prerequisite that is not automatic is the vanilla source:

```bash
./update-minecraft-src.sh          # decompiles the target version into minecraft-src/
bash dev/doctor.sh                 # says what is missing and why
```

Implementing a block, item or entity means writing a struct named exactly after
its vanilla class, annotated with `#[block_behavior]`, `#[item_behavior]` or
`#[entity_behavior]`, in the right directory. The build matches the struct name
against `foton-core/build/classes.json` and generates the registration. There is
no registry list to edit and no wiring to remember.

Everyday checks:

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo fmt --all --check
cargo clippy -r --workspace --all-targets --all-features -- -D warnings
typos
python3 dev/coverage.py --list entities
```

[`AGENTS.md`](AGENTS.md) holds the engineering rules — no invented data, no stubs
in foundations, no `.unwrap()` in production paths.
[`CONTRIBUTING.md`](CONTRIBUTING.md) covers the bar a change has to clear.

## What is generated

Four things here are outputs, not sources. Editing one by hand works until the
next build overwrites it, so edit what produces it instead.

| Output | Produced from | By |
|---|---|---|
| `*/src/generated/` | extracted vanilla data | the `build/` scripts, at build time |
| `CONFIGURATION.md` | `package-content/*.schema.json` | `dev/gen-config-docs.py` |
| `dev/parity-gaps.txt` | `foton-core/build/classes.json` | the build, checked by a test |
| the logo and the server icon | a list of voxel coordinates | `dev/gen-logo.py` |

`dev/ci.sh` fails when the configuration reference drifts from the schemas, and a
test fails when the parity ledger drifts from the code. Nothing in that table is
drawn, transcribed or maintained by hand — including the mark at the top of this
file, an F built from isometric blocks with the same 2:1 projection and
three-shades-per-material trick the game uses on its own.

## Layout

```
foton            binary, CLI, console, RCON
└─ foton-login   handshake, status, authentication, configuration
   └─ foton-core game logic: worlds, chunks, entities, menus, commands
      ├─ foton-worldgen   terrain, noise, biomes, structures
      ├─ foton-protocol   packet encoding and framing
      ├─ foton-registry   generated vanilla data, recipes, loot tables
      ├─ foton-macros     ReadFrom / WriteTo, the behavior attributes
      ├─ foton-utils      spatial types, NBT, keyed downcasting
      ├─ foton-math       math primitives and trig tables
      └─ foton-crypto     RSA and session authentication
```

## License

Foton is free software under the
[GNU Affero General Public License v3.0 or later](LICENSE).

It began as a fork of [SteelMC](https://github.com/Steel-Foundation/SteelMC)
and has been modified substantially since August 2026; the original copyright
notice is preserved in [`LICENSE`](LICENSE). The **network clause** is the part
that surprises people: run this code where other people can reach it, and they
are entitled to its source.

## Prior art

Four projects are named in the source because a system here is built on their
approach. Each entry points at where it landed, not at a courtesy:

- **[ScalableLux](https://github.com/RelativityMC/ScalableLux)** and the
  Starlight engine behind it — the light propagation in
  `foton-core/src/chunk/light/`, down to the cache radii and the queue layout.
- **[C2ME](https://github.com/RelativityMC/C2ME-fabric)** — the density-function
  transpiler in `foton-worldgen/build/density/`, including its static-bounds
  analysis and spline-interval rewriting.
- **[Structure Layout Optimizer](https://github.com/TelepathicGrunt/StructureLayoutOptimizer)**
  — the jigsaw bounds octree in `foton-worldgen/src/structure/box_octree.rs` and
  the out-of-bounds skip in the template processors.
- **[FastNoise](https://codeberg.org/ZenXArch/FastNoise)** — the write-only fill
  mode on paletted chunk sections.
