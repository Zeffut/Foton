<p align="center">
  <img src=".github/assets/readme/foton-logo.png" alt="Foton" width="160">
</p>

<h1 align="center">Foton</h1>

<p align="center"><em>A Minecraft Java Edition server, written in Rust, that refuses to guess.</em></p>

<p align="center">
  <img alt="Minecraft 26.2" src="https://img.shields.io/badge/Minecraft-26.2-5b8a3c?style=flat-square">
  <img alt="Protocol 776" src="https://img.shields.io/badge/protocol-776-4a6fa5?style=flat-square">
  <img alt="Rust nightly" src="https://img.shields.io/badge/Rust-nightly--2026--07--23-b7410e?style=flat-square">
  <img alt="AGPL-3.0-or-later" src="https://img.shields.io/badge/license-AGPL--3.0--or--later-6c4a8a?style=flat-square">
</p>

![A sunset over a Foton-generated world, with forests, rivers, mountains, and a lit village](.github/assets/readme/sunset.webp)

---

Foton is an independent implementation of the Minecraft Java Edition server,
currently targeting **Minecraft 26.2**. Two ideas hold it together.

**Vanilla parity is not a goal, it is the specification.** Anything a player can
observe — gameplay, protocol, registries, world generation — is written against
the decompiled vanilla source and the data pulled out of the game itself. Nothing
is implemented from memory, and no registry value is transcribed by hand.

**Modern hardware should be used.** Gameplay ticks stay synchronous, because
vanilla semantics depend on it. Chunk generation, lighting, packet processing and
chunk sending do not, and they run off the tick.

> [!IMPORTANT]
> Foton is pre-alpha. You can connect, explore, build and come back to a saved
> world, but survival gameplay is incomplete. Do not put it in front of a
> community you care about yet.

## Where it stands

Coverage of the vanilla behavior classes, cross-checked between
`dev/coverage.py` and the build-time ledger in `dev/parity-gaps.txt`:

| | Covered | Missing | What is left |
|---|---|---|---|
| **Blocks** | 255 / 265 · 96 % | 10 | mostly vanilla base classes and glass variants |
| **Items** | 69 / 70 · 99 % | 1 | vanilla's plain `Item`, which needs no behavior |
| **Entities** | 141 / 142 · 99 % | 1 | `Player`, handled outside the behavior mechanism |

That table counts whether a behavior *exists*, not whether it is *right*.
**`PARITY.md` is the document that answers the second question**, and it is
worth reading before trusting any number here — it opens by explaining how this
very measurement used to lie.

Working today: authentication, encryption and compression; persistent chunk
generation, loading, saving and lighting; movement, collision and block
interaction; inventories, menus and containers; commands, permissions and chat;
loot tables; redstone including pistons, comparators, rails and observers;
projectiles; mob AI, breeding and status effects; brewing, enchanting and
villager trading.

Not there yet: full protocol parity, plugins, and any compatibility with Paper,
Bukkit, Fabric, Forge or NeoForge extensions.

## Run it

```bash
cargo run -p foton                 # boots, writes config/, generates a world
cargo build --release -p foton     # binary at target/release/foton
```

On first boot Foton writes `config/config.toml`, `config/worlds.toml` and
`config/groups.toml` next to the binary, plus `.logs/`. Every key, default and
range is documented in **[CONFIGURATION.md](CONFIGURATION.md)**,
generated from the JSON schemas the server actually validates against.

The `Dockerfile` builds a scratch image from source:

```bash
docker build -t foton .
docker run -p 25565:25565 -v ./config:/config -v ./saves:/saves foton
```

`docker-compose.yml` pulls a published image instead, so it only works once a
release has pushed one to the registry.

## Work on it

The toolchain is pinned by `rust-toolchain.toml`; `rustup` picks it up on its
own. The one prerequisite that is not automatic is the vanilla source:

```bash
./update-minecraft-src.sh          # decompiles the target version into minecraft-src/
bash dev/doctor.sh                 # says what is missing and why
```

Implementing a block, item or entity means writing a struct named exactly after
its vanilla class, annotated with `#[block_behavior]`, `#[item_behavior]` or
`#[entity_behavior]`. The build script matches the struct name against
`foton-core/build/classes.json` and generates the registration. There is no
registry list to edit.

```bash
bash dev/ci.sh                     # the whole verification suite
bash dev/smoke-test.sh             # boot, speak the protocol, shut down
bash dev/join-test.sh              # take a real client from login to play
python3 dev/coverage.py --list entities
```

Raw checks:

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo fmt --all --check
cargo clippy -r --workspace --all-targets --all-features -- -D warnings
typos
```

`AGENTS.md` holds the engineering rules — vanilla-first, no invented data, no
stubs in foundations, no `.unwrap()` in production. `CONTRIBUTING.md` covers the
rest.

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

Never edit anything under `src/generated/` — it is produced at build time from
`build/` scripts and extracted data.

## License

Foton is free software under the
[GNU Affero General Public License v3.0 or later](LICENSE).

It derives from an existing AGPL-3.0 codebase and has been modified
substantially since August 2026; the original copyright notice is preserved in
`LICENSE`. The **network clause** is the part that surprises people: run this
code where other people can reach it, and they are entitled to its source.

## Acknowledgements

World generation, lighting and performance work draws on ideas from
[C2ME](https://github.com/RelativityMC/C2ME-fabric),
[ScalableLux](https://github.com/RelativityMC/ScalableLux),
[FastNoise](https://codeberg.org/ZenXArch/FastNoise),
[Lithium](https://github.com/CaffeineMC/lithium) and
[Structure Layout Optimizer](https://github.com/TelepathicGrunt/StructureLayoutOptimizer).

The logo was designed by **colonthreeing**.
