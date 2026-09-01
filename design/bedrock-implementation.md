# Bedrock clients — the implementation

What Foton is going to build so a Bedrock player can join, and why each piece is
shaped the way it is. Written 2026-09-01. Nothing here is built yet.

`bedrock-clients.md` is the analysis that came first: it lays out three routes
and refuses to pick one until stage 0 has been done. This document picks one,
and says what it costs. Read that one for why the other two were not chosen;
this one assumes that argument rather than repeating it.

## The decision

**Foton supervises Geyser as a child process, and speaks Floodgate natively.**

An operator sets `bedrock.enabled = true`, restarts, and Bedrock players join on
port 19132. Foton fetches the Geyser jar, writes its configuration, starts it,
folds its output into Foton's own log, restarts it if it dies, and stops it on
shutdown. The operator manages one server, not two.

That is route A from the analysis — Geyser in front — with the two-process
seam hidden behind Foton's own lifecycle, plus the one piece route A was
silently missing.

### What "100 % joinable" actually required

Route A as written in the analysis does not get a Bedrock player into the
world. Geyser on its own authenticates a Bedrock player by making them **own
and link a Java account**, and the overwhelming majority of Bedrock players —
Switch, Xbox, PlayStation, phone — have never owned one. A server on route A as
described is reachable and empty.

The missing piece is Floodgate: Geyser encrypts the Bedrock player's identity
into the Java handshake with a key both sides share, and the Java server
decrypts it and trusts it. Normally that decryption is a Bukkit plugin, which
Foton is not. Foton owns `foton-login`, so it does it natively, in Rust, and
needs no plugin at all.

This is the piece that makes the difference between "a Bedrock client can reach
the port" and "a Bedrock player is standing in the world", and the analysis did
not name it.

### Why a child process rather than the JVM already in the building

`foton-plugin` loads `libjvm` and calls `JNI_CreateJavaVM`, and its own
documentation states the constraint: *"a JVM cannot be started twice in one
process"*. So in-process Geyser is not a second JVM; it is Geyser sharing the
plugin host's JVM, which makes the Bedrock feature depend on the plugin crate,
its classloader and its lifecycle. Those are under active construction by
another workstream. Coupling a Bedrock listener to a plugin host mid-build buys
one saved process and pays for it in a shared failure surface.

Three further reasons, none of them scheduling:

- **An operator who wants no Bedrock pays nothing.** No JVM, no threads, no
  heap. The plugin design already names this as a requirement for its own JVM,
  and it applies twice over here.
- **Geyser Standalone is a program, not a library.** It expects to own its
  configuration, its logging and its shutdown. Driving that through JNI means
  reimplementing its launcher against internals it does not promise to keep.
  `java -jar` is the interface it actually supports.
- **A crash stays a crash.** Geyser falling over restarts Geyser. It does not
  take the world with it.

The in-JVM route stays open. If the plugin host stabilises and an operator is
running both, hosting Geyser in that JVM becomes a supervisor implementation
detail, and nothing above the `BedrockSupervisor` interface changes.

### What this is not

Foton does not implement Bedrock gameplay. The analysis rejects that outright
and this document does not reopen it: there is no decompiled Bedrock source in
this tree, so there would be no definition of correct. A Bedrock player here
experiences the Java server's behaviour, translated by Geyser. Where Geyser's
translation is imperfect, Foton's answer is to fix the Java protocol gaps Geyser
exposes, not to second-guess the translation.

So "100 % compatible" means, precisely: **every Bedrock player can join, be
identified, persist, and play the Java game as Geyser presents it.** It does
not mean Bedrock-exclusive behaviour, and no honest version of it could.

## The pieces

Three, and they are separable.

### 1. `foton-bedrock` — a new crate

Two modules with nothing in common but a key, which is why they are named
apart:

- `geyser.rs` — locating a Java runtime, fetching and pinning the jar, writing
  the configuration, spawning and supervising the process, relaying its output.
- `floodgate.rs` — decoding and verifying the identity Geyser puts in the
  handshake. Pure: bytes and a key in, a verified identity or an error out. No
  I/O, no process, no clock. Testable without a JVM, a network or a server.

`key.rs` generates the shared secret on first run and loads it after.

The crate depends on `foton-core` for its config types and nothing heavier.
Critically, `floodgate.rs` is what `foton-login` needs, and it is the half with
no dependencies — so `foton-login` gaining a Bedrock capability does not drag a
process supervisor into the login path.

### 2. The supervisor

Owned by the `foton` binary, started beside the TCP listener and the RCON
listener in `foton/src/lib.rs`, cancelled by the same `CancellationToken`.

Startup, in order, each step failing with a message that says what to do:

1. **Find a Java runtime.** `bedrock.java_home`, else `JAVA_HOME`, else `java`
   on the path. Version checked at startup — Geyser needs 21 or newer, and the
   failure mode of discovering that from a stack trace in a relayed log is
   worse than a sentence at boot.
2. **Get the jar.** `bedrock.jar_path` if the operator supplies one. Otherwise
   downloaded once into the run directory from the pinned build, verified
   against a checksum committed here, and reused thereafter. A server with no
   outbound network and no supplied jar fails at boot with the URL it wanted,
   rather than at the moment a player tries to join.
3. **Write the configuration.** Generated whole, every start, from Foton's
   config — never merged with what is on disk, because a half-owned config file
   is a source of bugs nobody can reproduce. An operator who needs a knob Foton
   does not expose sets it under `bedrock.geyser_overrides`, which is merged
   into the generated YAML.
4. **Spawn, and adopt the output.** Geyser's stdout and stderr are parsed for
   level and re-emitted through `tracing` with a `geyser` target, so
   `RUST_LOG` and the log config work on it the way they work on everything
   else. The operator sees one log.
5. **Supervise.** Exit before shutdown means restart, with backoff; a process
   that cannot stay up past the backoff ceiling logs loudly and stays down
   rather than spinning. Shutdown sends the platform's polite signal, waits,
   and kills.

**The pin is deliberate.** Geyser 2.11.2 build 1233 is the build whose bytecode
was read and confirmed to carry protocol 776 / MC 26.2. Following `latest`
would mean Foton's Bedrock support silently breaks the day Geyser moves to the
next protocol. The version, build number, download URL and SHA-256 are
constants in `foton-bedrock/src/geyser.rs` — data in this repository, bumped by
a person who checked, the same way the Minecraft target is. `dev/doctor.sh`
checks the pinned build still claims the protocol `Cargo.toml` targets, so the
pin ageing is a failed check rather than a player's error message.

### 3. Floodgate, natively

Geyser is configured with `auth-type: floodgate` and the key Foton generated.
It then connects to Foton as a Java client whose handshake `hostname` carries
the Bedrock player's identity, encrypted with that key.

`SClientIntention.hostname` already exists in `foton-protocol` and is already
read. `foton-login` gains a branch: if the hostname carries the Floodgate
marker, decrypt it, verify it, and build the profile from what it says instead
of from Mojang.

**The wire format is read from Floodgate's source, not recalled.** The
repository's rule against transcribed data applies here exactly as it does to
registries: the marker, the framing, the cipher and the field order come from
reading the implementation, and the integration test against a real Geyser is
the oracle that says the reading was right. Anything written from memory would
be a guess that compiles.

Expected shape, to be confirmed against that source rather than assumed:
identity fields covering XUID, gamertag, device, language and input mode; a
UUID derived from the XUID; and a signature that is the whole security of the
scheme.

#### The security of it, stated plainly

A Floodgate handshake is an assertion of identity that skips Mojang. If it can
be forged, anyone becomes anyone, including an operator. Two independent
defences, both on by default:

- **The key.** The assertion is verified against a secret generated on first
  run, readable only by the server, shared only with the Geyser that Foton
  itself launched. Without it, a forged handshake fails verification.
- **The source address.** Foton launches Geyser locally, so Floodgate
  handshakes are accepted only from loopback by default
  (`bedrock.trusted_proxies`). An operator running Geyser on another host
  widens it on purpose, and the config documents what widening it means.

A handshake that claims Floodgate and fails either check is refused with a
neutral message and logged at warn. It is never quietly downgraded to an
ordinary offline login — that would turn a failed forgery into a successful
one.

`online_mode = true` stays true and stays enforced for Java clients. It is not
weakened, and the config validator gains a rule that says so rather than
leaving an operator to infer it.

## Identity, and what it does to the rest of the server

A Bedrock player gets a **deterministic UUID derived from their XUID** and a
name **prefixed** (`.Steve` by default, `bedrock.username_prefix`).

Deterministic matters more than it sounds: it is what makes player data,
permissions, bans, statistics, advancements and homes work for a Bedrock player
without a single one of those systems learning that Bedrock exists. They are
keyed by UUID; the UUID is stable; nothing else changes.

The prefix exists because a Bedrock gamertag and a Java username occupy the same
namespace and can collide, and because an operator looking at a ban list should
be able to see which edition someone came from. It is configurable and can be
empty, with the collision consequence documented rather than hidden.

Gamertags also contain characters Java usernames cannot. Foton already validates
names with `is_valid_player_name`; the Bedrock path sanitises to what the Java
protocol permits before that check, and the mapping is deterministic so it does
not shift under a returning player.

**Account linking is explicitly out of scope for this work.** A Bedrock player
who also owns a Java account gets two identities. That is a real limitation,
stated here rather than discovered later, and it is a self-contained feature
that can be added on top without changing anything above.

## Configuration

Under `[bedrock]`, schema in `package-content/` like everything else, so
`CONFIGURATION.md` regenerates and `dev/ci.sh` keeps it honest.

```toml
[bedrock]
enabled = false          # off unless asked for; nothing is paid when off
port = 19132             # UDP, the Bedrock default
motd = ""                # empty means reuse Foton's own MOTD
username_prefix = "."
trusted_proxies = ["127.0.0.1", "::1"]
java_home = ""           # empty means JAVA_HOME, then the path
jar_path = ""            # empty means fetch the pinned build
geyser_overrides = {}    # merged into the generated Geyser config
```

Off by default. A feature that starts a JVM has to be asked for.

## Where it attaches, measured

The whole point of this route is how little it touches.

| File | Change |
|---|---|
| `Cargo.toml` | one workspace member |
| `foton/src/lib.rs` | start the supervisor beside the RCON listener, cancel it with the rest |
| `foton/src/config/server.rs` | the `[bedrock]` section and its validation |
| `foton-login/src/handlers/login.rs` | one branch: a verified Floodgate identity produces the profile |
| `package-content/` | the config schema |
| `foton-bedrock/` | new, everything else |

**`foton-core/src/player/connection/mod.rs` is not touched.** The analysis
identified the `NetworkConnection` seam as the expensive prerequisite, and it is
— for route B. Geyser is an ordinary Java client, so route A needs none of it.
That is not only a saving; it is what keeps this work clear of the plugin API
work, which wants that same seam for observing outbound packets.

## Staying out of the plugin work's way

Both efforts want a JVM and both touch login-adjacent code, so the boundary is
worth stating rather than assuming.

- **Separate branch**, `feat/bedrock`, per the repository's one-branch-per-topic
  rule.
- **No shared crate.** `foton-bedrock` does not depend on `foton-plugin`, and
  the reverse. Two independent Java runtimes, one in-process for plugins, one
  out-of-process for Geyser, is a real duplication and the right one for now.
- **Six shared lines.** The overlap is `Cargo.toml`'s member list, the
  listener startup in `foton/src/lib.rs`, and the config struct — three
  append-only edits that conflict trivially if at all.
- **The contested file is left alone.** `NetworkConnection` is the plugin
  work's to reshape. If it lands first and changes shape, nothing here
  notices.

If the plugin host ships first and an operator runs both, the in-JVM supervisor
becomes attractive as an optimisation, and the interface above is already the
place to put it.

## Testing

`dev/bedrock-test.sh`, in the shape of `dev/join-test.sh`, which already boots a
server and walks a real client through login into play.

The chain under test is a **simulated Bedrock client → Geyser → Foton**. The
client is `bedrock-protocol` from PrismarineJS, driven from Node, which WSL
already has. So the whole path is exercised in CI without a phone, a console or
a human — which is what stage 0 was blocked on, and the reason it stayed
undone.

The test asserts what matters and nothing decorative:

1. A Bedrock client completes the Bedrock handshake against Geyser.
2. Foton receives a Floodgate handshake and accepts it.
3. The resulting profile has the derived UUID and the prefixed name.
4. The player reaches the play state and receives chunks.
5. Reconnecting yields the same UUID — the persistence claim, tested rather
   than asserted.

Unit tests in `foton-bedrock` cover the decoder without any of that: a
round-trip against a key generated in the test, a forged signature rejected, a
truncated payload rejected, a handshake from an untrusted address rejected, and
a well-formed one from an untrusted address rejected *for that reason* — the
case where a mistake silently becomes an offline login.

Per the repository's testing rule, none of these restate a constant. Each one
is a regression that would otherwise reach an operator.

## Staging

**Stage 0 — prove the chain, throwaway.** Geyser Standalone by hand in front of
a running Foton, a simulated Bedrock client, and a written record of what
happens. This is the stage `bedrock-clients.md` says to do before anything
else, and it has never been done. If Geyser exposes Java protocol gaps in
Foton, they surface here as a list — and they are worth fixing whatever happens
to Bedrock, because a demanding client found them. Hours.

**Stage 1 — Floodgate, decoded and verified.** `foton-bedrock`'s pure half,
plus the `foton-login` branch, plus the config. Testable against a Geyser
started by hand. Days.

**Stage 2 — the supervisor.** Runtime discovery, jar pinning, config
generation, spawn, log relay, restart, shutdown. Days.

**Stage 3 — the automated test and the documentation.**
`dev/bedrock-test.sh` in `dev/all-tests.sh`, `CONFIGURATION.md` regenerated,
`README.md` stating that Bedrock players can join and exactly what that means.

**Stage 4 — the gaps stage 0 found.** Java protocol work, sized once there is a
list.

Stage 0 gates everything. If Geyser cannot drive Foton at all, stages 1 to 3 are
built on a broken assumption and the finding is worth more than the code.

## Risks

- **Geyser's translation is not perfect and never will be.** Some Bedrock
  behaviour will be wrong in ways Foton cannot fix, because they are Geyser's
  to fix. The README has to say this next to the claim, the way `PARITY.md`
  keeps its caveat beside its percentages. Overstating this is the failure mode
  that costs trust.
- **A forged Floodgate handshake is total impersonation.** Two defences, both
  default-on, and a decoder whose failure path is refusal rather than
  downgrade. This is the part of the work to write carefully and review
  hardest.
- **The pin ages.** Geyser moves with Minecraft; a pinned build stops matching
  after a version bump. `dev/doctor.sh` already checks the Minecraft sources
  against `Cargo.toml`'s target, and it gains a check that the pinned Geyser
  build claims the same protocol.
- **A JVM appears in an operator's memory budget.** Only when enabled, and
  disclosed in the config documentation with a real number, not a shrug.
- **Licensing.** Geyser and Floodgate are MIT; Foton is AGPL-3.0-or-later.
  Shipping no Geyser code, invoking a jar the operator's server fetches, and
  reimplementing a wire format from reading it, are three separate acts and all
  three appear fine. The one that deserves a second look before release is the
  jar fetch, and it is cheap to look now.

## Open questions

The analysis left four. Three are answered here.

- ~~Does Geyser support protocol 776?~~ Yes, verified in its bytecode.
- ~~What identity does a Bedrock player get?~~ A deterministic UUID derived
  from the XUID, a prefixed name, and every UUID-keyed system working
  unchanged.
- ~~Does `online_mode = true` mean anything for a Bedrock client?~~ It stays
  true and stays enforced for Java clients. A Bedrock player is authenticated
  by Xbox Live at Geyser and vouched for by a key only Foton and its own Geyser
  hold. The config validator states the relationship rather than leaving it to
  be inferred.
- **Where does Bedrock block, item and entity data come from?** Still
  unanswered — and now irrelevant, because it was route B's blocker and route B
  is not being built. It is recorded so that anyone reopening route B finds it
  still standing there.

One new one, and it is not blocking: an operator running Geyser on a separate
host has to widen `trusted_proxies`, at which point the shared key is the only
defence and it crosses a network. That deployment should probably be refused
rather than documented, and the decision can wait until someone asks for it.
