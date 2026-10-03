# Zelda Civ compatibility baseline — 2026-10-03

This is a pinned diagnostic, not a compatibility certificate. Zelda Civ's working
checkout has uncommitted work; the test artifact was built from a separate,
detached checkout of GitHub `origin/main` at
`3c324d63c502eb18242af7c771baa11ea1c2915c`. Maven's 141 tests passed.
The original Zelda sources and the tested JAR were not edited for Foton.

| Artifact | SHA-256 | Size |
| --- | --- | ---: |
| `zelda-civ-0.1.0-3c324d63.jar` | `7fca812a4e1705fc8814c948e82b31df110d68d8544052f01fd7779b6b803bd1` | 10,275,396 |
| `voicechat-bukkit-2.6.20.jar` | `b63c4389ccfb37dff1d631f91e0bb1b983bcdb55609e7a26c22bb9442976a9fa` | 938,745 |

Local copies and raw logs are under the corresponding `Foton-Zelda-*3c324d63-20261003`
directories in `%TEMP%`. The official Simple Voice Chat artifact was selected
from its Modrinth 2.6.20 release. The Paper builds are pinned in
`dev/compat/paper-builds.json`; their hashes were checked before use.

## Controlled startup comparison

| Server | Java | Observation |
| --- | --- | --- |
| Paper 1.21.11 build 132 | 21 | With both JARs, both plugins enabled. Voice Chat registered Zelda's extension. Zelda initialized storage, recipes and merchants; delayed tasks ran for at least 10 seconds. Console `plugins`, `zczone info`, and `voyageadmin list` responded. No client or gameplay workflow was exercised. |
| Paper 1.21.11 build 132 | 21 | Without Voice Chat, Zelda fails before `onEnable` with missing `de.maxhenkel.voicechat.api.VoicechatPlugin`. Its `softdepend` is not a working standalone mode in this revision. |
| Paper 26.2 build 129 | 25 | With both JARs, Voice Chat enabled, but Zelda failed during `onEnable` with `NoSuchMethodError: BookMeta.pages(List)` in `LivreCuisine.creer`. This is a Paper version/API mismatch, not a Foton-specific failure. |
| Foton `4982ede6d72c4711d36b713d3ae3c04d969f2aa2`, Minecraft 26.2 | 21 | With both JARs, Voice Chat rejected Foton's version and could not find its expected server adapter `getServer`; Zelda then failed to discover its shaded SQLite driver through `DriverManager`; cleanup exposed missing `Entity.getScoreboardTags()`. Foton itself stopped cleanly. |
| Foton `c964da1ac98c9bfdeb4b0d2936e70bbdef3be779`, Minecraft 26.2 | 21 | With the unchanged pair, SQLite initialized Zelda's databases (`zones.db` and `players.db`), then Zelda failed with `NoSuchFieldError: CreatureSpawnEvent$SpawnReason.BEEHIVE` in `VanillaSuppressor`. Voice Chat still rejected the Foton version and reflective `getServer`. Neither plugin enabled; no commands, delayed tasks or client behavior were tested. Foton stopped cleanly. |

The Paper 1.21.11 control log has SHA-256
`4df833cb79abce77f162b394df594c5b11410bac98bdaa6351311cec960cb721`.
The Foton Java log has SHA-256
`61513f1ea19a51a976184b3c8ecc56f53264c7692de97dd911b237980a28b0c8`.
The Paper 26.2 control log has SHA-256
`751440e8ff29ffc85524f558fa326d60e3a3d026d1bbf28b5a5d3545634645c2`.
All runs used an isolated world, offline mode, loopback networking, the same
two plugin JARs and no player connection. Expected warnings for missing
administrator token and optional LuckPerms are not counted as startup failures.
The later Foton run is recorded separately under
`%TEMP%/Foton-Zelda-Foton-postfix-3c324d63-20261003-c36eeffc8ade4836852b92252b70a69c/baseline-report.md`;
its binary SHA-256 is
`f547a1f711ba8dfb61dc28f10863a4412b9df87656bd35bef56dade5f8d550b7`.

## Acceptance path

1. Supply SQLite through Foton's host JVM classpath and prove a real plugin can
   open `jdbc:sqlite::memory:`, write and read data, including from the release
   runtime after restart.
2. Separate Foton's product version from its Bukkit/Paper API version, then
   implement Voice Chat's required CraftBukkit/NMS-facing server and player
   adapters with real behavior. Its 2.6.20 versioned path expects
   `Bukkit.getServer().getServer()`, `CraftPlayer` channel operations and a
   player handle, plus command/chat bridges; implementing only `getServer`
   would move the failure. Enable the unmodified companion, then Zelda, in
   the same dependency order as Paper. Verify the versioned path and later
   voice-message and UDP behavior with connected clients; the reduced Bukkit
   fallback is insufficient for full compatibility.
3. Retain the older Paper 1.21.11 binary API needed by Zelda, starting with
   component book pages; prove component state is preserved, not flattened.
4. Close the Zelda linkage and behavior backlog, including events, persistent
   items, entities, inventories, recipes, merchants, UI, network, and save/load.
   Repeat each scenario on both Paper 1.21.11 and Foton. The static linkage
   scan reported 238 references after MiniMessage was packaged, but this is
   diagnostic only and omits reflective/runtime interactions.

The compatibility target is the *unchanged pinned Zelda JAR*, not a promise
that every version of every plugin works. A newer Zelda commit requires a new
build, hash and certification run.
