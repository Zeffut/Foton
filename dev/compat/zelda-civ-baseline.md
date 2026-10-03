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
| Foton `4f4b18e3c44b8ac8f42b3cebe42c7fb689a747ac`, Minecraft 26.2 | 21 | SQLite and the BEEHIVE class initialization pass. Zelda next fails with `NoClassDefFoundError: org/bukkit/event/enchantment/PrepareItemEnchantEvent` while registering `VanillaSuppressor`. Voice Chat recognizes `26.2.build.0-foton`, then fails on reflective `FotonServer.getServer`. Neither plugin enabled. Foton saved the fresh world and stopped with exit 0. |
| Foton post-enchantment snapshot associated with `03263af45097977140bb9a32f0247763e1fc73eb`, Minecraft 26.2 | 21 | The unchanged Zelda JAR passes the previous enchantment-class blocker, then fails on `NoClassDefFoundError: org/bukkit/event/inventory/BrewEvent` during the same listener registration. Voice Chat still fails on `FotonServer.getServer`. Neither plugin enabled. Foton starts, saves 2,025 chunks and 0 players, and stops cleanly with exit 0. |

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
The book/beehive baseline is retained under
`%TEMP%/Foton-Zelda-Foton-books-4f4b18e3c-20261003/baseline-report.md`.
Its frozen binary SHA-256 is
`6591e930c04d1267e69998de08e88003902f75bbd26b1d13422fe580f1448eb8`,
and Java stderr SHA-256 is
`65c377ec308ba3879e9436de130bb26a5015bc71173d34ab1cc18889c7ba29da`.
This no-client run does not validate the channel registration path; a separate
review found an invalid NUL-splitting regex in that snapshot.

The post-enchantment run is retained under
`%TEMP%/Foton-Zelda-Foton-enchant-03263af45-20261003/baseline-report.md`.
Its frozen binary SHA-256 is
`1def34826d8e40ffaf56dffa6bf1e17daf120523006756bf3a8f638ce08fb5dd`,
API SHA-256 is
`21f45a774ef8b91421eb663eb93e6a1cd8169b649dd4374a67077974730d481b`,
and Java stderr SHA-256 is
`c534d698776d80d7fe2d414c2dd874cafa35167227c3fe5fa7c58975c7e984f6`.
These artifacts were built before their corresponding commits, then frozen
before later review fixes. Source association uses the recorded build report,
source state and timestamps; the hashes identify the executed artifacts.
This run establishes the new startup blocker, not live enchanting behavior.
The same frozen API and its 47-library runtime produce 228 static linkage
diagnostics for the exact Zelda JAR: 14 interface/class-kind mismatches, 52
missing classes and 162 missing members. The scanner exits 1; its full output
is retained beside the report as `linkage-after-enchant.txt`. The earlier
MiniMessage-only snapshot produced 238 diagnostics. These counts are neither
a percentage of compatibility nor a gameplay result.

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
   The latest boot blocker is `BrewEvent`. The native completion event exists,
   but the Java event needs a real `BrewingStand`/`Container` holder with
   snapshot/update, lock and inventory semantics. Adding an empty event class
   would only move the reflection failure. The preceding enchantment slice
   has real native and Paper-compiled fixture tests; its review fixes and
   the subsequent integration results are recorded below.

The compatibility target is the *unchanged pinned Zelda JAR*, not a promise
that every version of every plugin works. A newer Zelda commit requires a new
build, hash and certification run.

## Frozen Via regression after the enchantment slice

The same frozen post-enchantment runtime was tested with unchanged official
ViaVersion and ViaBackwards 5.11.0 in a loopback-only network namespace. Both
clients passed: Minecraft 1.21.11 (protocol 774) reached PLAY, received a
translated chunk, acknowledged keep-alive, executed `/list` and stayed
connected; the native 26.2 client completed login/configuration and received
nine chunks. ViaVersion enabled before ViaBackwards. Server shutdown was
clean, exit 0; all 59 manifest inputs remained byte-identical. This does not
test the later retained-enchantment-view fixes.

ViaVersion nevertheless logs `ClassNotFoundException: foton.entity.CraftPlayer`
and disables its login race-condition fixer. Its warning explicitly says
plugins using ViaAPI on join may work incorrectly. This is a real compatibility
gap, separate from the expected public-key and update-check network warnings
in the disconnected namespace. The `dev/via-test.sh` exception regex used for
this frozen run missed this named exception, although its codec/refcount
assertions passed.
Consequently the result is **client connection passed, full Via compatibility
not certified**; a successful existing test gate must not hide this warning.

Evidence is retained at
`%TEMP%/Foton-Via-frozen-03263af45-684db737/via-report.md`, with original and
normalized logs, client output, isolation evidence and input hashes. The report
SHA-256 is `0ba515bc3f40d77aafb6580ddd594ed6f793ece590b708bdacc914942f440bf4`.
Temporary runtime copies also remain because their cleanup was denied by the
execution policy; no alternate deletion was attempted.

## Stabilization after the frozen runtime observations

The book canonicalization, retained enchantment views, instance-qualified
deferred menu closes, channel advertisement and test-isolation fixes passed
their independent scoped reviews. These later fixes do not retroactively
change the hashes or results of the frozen server runs above.

The Via harness now uses a shared strict diagnostic classifier and verifies
network isolation before starting the server or clients. It also owns and
stops its client/server processes on interruption, retains failure evidence,
and checks the complete shutdown log. Commits `413764045`, `0cad2618b` and
`684511382` passed focused regressions and independent review, including a
root namespace slow-client interruption test. Classification of the retained
real log now exits 1 for the missing CraftPlayer class. The underlying runtime
adapter is still missing; this tooling repair is not a Via compatibility pass.

The complete `dev/ci.sh` run on `3eb818153` passed workspace tests, Java API,
official-Paper-compiled shapeless/enchantment/SQLite fixtures, runtime closure,
packaging/installers, native/JNI checks, the 5,675-test/20-target inventory and
developer-tooling tests. It exited 1 because strict Clippy found two equivalent
Option expressions in the plugin-loading example. The single-file correction
`f5ff305a8` then passed the FULL strict release workspace/all-target/all-feature
Clippy command, formatting, spelling and independent delta review. This is
combined stabilization evidence, not a single post-correction all-green CI
run or a gameplay certification. The next broad foundation/release gate still
requires the complete suite on its exact revision.

The original Linux `/tmp` integration logs are no longer present; the execution
record retains the stage outcomes. Subsequent detailed logs are retained in
Windows TEMP or the ignored plan workspace, outside the volatile Linux `/tmp`.
At that point, the next slice was native brewing lock enforcement and
persistence. Neither Zelda nor Voice Chat was certified.

## Reviewed native foundations and complete integration

Native typed brewing locks (`4b3899c87`) and tile PDC lifecycle/client filtering
(`d1dd7ce1c`) passed their independent spec and quality reviews. Lock tests cover
real opening/denial and the corrected five-slot brewing menu layout. PDC tests
cover normal/raw/falling persistence and real chunk-update preparation, with
all seven current client-egress routes using the same sanitizer. Live editor
packet capture and world-backed dirty/restart scenarios remain later holder
integration gates; these native tests do not certify the full Java holder.

A new complete `bash dev/ci.sh` run on frozen `f1ca5e71e`, with the generated
test inventory refreshed to **5,687 tests across 20 targets**, exited 0 and
reported **ALL GREEN**. Every stage passed: formatting/spelling, config/site,
strict release workspace Clippy with all targets/features, workspace tests,
Java API, official Paper recipe/enchantment/SQLite fixtures, runtime closure,
packaging/installers, native/JNI checks, test inventory and developer tooling.
Durable stage output is in the ignored compatibility-plan workspace at
`native-wave-ci-20261003.log`; this is one completed full integration run, not
the earlier combined stabilization. It is not a new unchanged-Zelda boot or
an armor/Via gameplay result.

The user's armor requirement is mapped in [the armor plan](zelda-armor-plan.md).
An isolated official Paper 132 oracle completed 111 observations and 97 verifier
checks for full item/meta transitions and PDC semantics; the
[live item design](item-state-bridge-plan.md) records the results and evidence location. Native
canonical item ownership is the next implementation prerequisite. Complete
PDC API, trim/attributes, persistent chestplate bytes, real armor events,
container transfer/holder/BrewEvent and the CraftPlayer/Voice Chat bridges
remain open. **No plugin is certified yet.**
