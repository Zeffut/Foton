# Bedrock clients — design

Where Foton stands on letting Bedrock Edition players connect, and what each
route actually costs. Written 2026-08-31. Nothing here is built yet.

Design notes live in `design/` because they describe work that has not
happened. Everything in the repository root — `README.md`, `PARITY.md`,
`CONFIGURATION.md` — describes what is true today, and a proposal filed beside
them would read as a promise.

## The question this has to answer first

Bedrock is not a wire format for the same game. It is a different game.
Combat resolves differently, redstone ticks differently, inventories are a
different model, and the two editions generate different worlds from the same
seed. There is no `minecraft-src/` for Bedrock, and there never will be: it is
closed, native, and has no decompiled source in this tree.

Foton's specification is the decompiled Java source. That sentence is the whole
project. So "Bedrock compatibility" can mean exactly one thing here:

> A Bedrock client connects to Foton and experiences **the Java server's
> behavior**, translated at the boundary.

Not "Foton implements Bedrock gameplay". That second reading would mean a
second game with a second specification and no source to write it against, and
it is rejected outright — not because it is large, but because it has no
definition of correct. Everything below is about the first reading.

That is also, precisely, what GeyserMC already does.

## Three routes

### A. Geyser in front, out of process

A Bedrock client connects to Geyser; Geyser connects to Foton as an ordinary
Java client. Foton needs to know nothing about it.

Cost to Foton: **zero**, if Foton's Java protocol is already good enough to
host a normal client — which it is, because real clients play on it.

This route is not a compromise to be embarrassed about. It is the route that
gets Bedrock players onto a Foton server soonest, and it is the honest baseline
every other route has to beat.

Two things must be verified before claiming it works, and neither is known
today:

1. **Does Geyser support this Java protocol version?** Foton targets protocol
   776 (Minecraft 26.2). Geyser tracks Java versions on its own schedule. If it
   does not yet support 776, route A is unavailable until it does, no matter how
   complete Foton is.
2. **Does Foton implement what Geyser leans on?** Geyser is a well-behaved
   client, but it is a demanding one: it wants the full login sequence, entity
   metadata, chunk data, inventory transactions and tab-list handling. A gap
   that a vanilla client tolerates may stop Geyser.

Both are answered by an afternoon's experiment, not by reasoning. That
experiment is the first task in this document.

### B. Translation inside Foton

Foton speaks RakNet and the Bedrock protocol itself, translating between the
Bedrock wire and its own game state.

This is Geyser's job, done again, in Rust, inside the process. What it buys:
one process instead of two, no extra network hop, no version skew between two
projects, and translation that can see server state directly rather than
inferring it from the Java packets it received.

What it costs is the subject of most of this document.

### C. Native Bedrock behavior

Rejected above. Recorded here so the rejection is on paper rather than assumed.

## What route B actually contains

Geyser is roughly 150,000 lines of Java, built by a team over six years, and it
still carries a list of things that do not translate. That is the honest scale
marker. The parts, in rough order of how much they hurt:

**RakNet.** Bedrock runs on UDP with its own reliability layer: ordering
channels, fragmentation and reassembly, acknowledgement, congestion control,
and an offline ping/connect handshake. This is a protocol implementation in its
own right, before a single game packet is read. Rust has crates for it; none
that this project would want to depend on without reading closely.

**The login chain.** Bedrock authenticates through Xbox Live and presents a
chain of signed JWTs. Its relationship with Foton's `online_mode` is not
obvious and needs deciding rather than assuming: a Bedrock player has an XUID,
not a Mojang UUID, and something has to decide what identity they get on a
server whose permissions, player data and bans are all keyed by UUID.

**The packet sets do not correspond.** Foton today has 69 clientbound and 39
serverbound game packets, and it is not finished; the vanilla set it is
converging on is larger. Bedrock's is around two hundred. The mapping is not
one-to-one in either direction: one Java packet can require several Bedrock
ones and the reverse is also true.

**Inventory.** By common account the hardest part of Geyser. Bedrock describes
inventory changes as transactions the server validates, Java as slot updates the
server dictates. Reconciling them while keeping Java's exact click semantics —
which Foton already implements against the vanilla source — is where a
translation layer earns its keep or loses it.

**Chunks and biomes.** Different section format, different palette encoding,
different biome representation. Mechanical, large, and testable.

**Block, item and entity identity.** Every Java block state needs a Bedrock
runtime id, every item an id and metadata pair, every entity a Bedrock type and
metadata layout. Geyser maintains this as generated data. Foton would have to
generate its own — and *would*, because the alternative is transcription, which
this repository forbids. Where that data comes from is an open question this
document cannot answer: the extractor reads the Java client, and there is no
equivalent for Bedrock.

That last point deserves emphasis. Route B does not just need engineering
effort. It needs a *source of truth* for Bedrock data that satisfies the
project's own rule against hand-transcribed values, and no such source exists in
this tree today. Finding or building one is a prerequisite, not a detail.

## Where it would attach

Measured, not guessed.

**The send path is one function.** `PlayerConnection::send_packet<P: ClientPacket>`
in `foton-core/src/player/connection/mod.rs` encodes the typed packet and hands
bytes to the connection. Its 134 call sites across `foton-core` pass typed
packets and would not change. That is a far better position than it could have
been.

**The connection trait is at the wrong level.** `NetworkConnection::send_encoded`
takes an `EncodedPacket`, which is already Java-framed bytes. A Bedrock
connection handed one of those could only decode it and re-encode — which is to
say, be a proxy, inside the process, with the costs of route B and the
translation fidelity of route A. The seam has to move up to the typed packet
for route B to be worth doing at all.

The trait's own documentation already says the `Other` variant is for "tests and
alternative backends", and `foton-login` already names its types `JavaConnection`
and `JavaTcpClient`. The place for a second transport was left open on purpose.

**The listener is TCP.** `foton/src/lib.rs` binds a `TcpListener`. RakNet needs
a `UdpSocket` alongside it, on its own port, with its own accept path.

**Login is Java-shaped.** `foton-login` implements the Java handshake,
encryption and Mojang session check. A Bedrock login is a different sequence
with different cryptography, and belongs beside it rather than inside it.

## Staging

Each stage has to be worth having on its own, because there is a real chance
that stages 2 and later never get built — and that would be a defensible
outcome, not a failure.

**Stage 0 — find out whether route A already works.** Run Geyser in front of a
Foton server, connect a Bedrock client, and write down what happens. If it
works, Bedrock support exists today and the rest of this document becomes
optional. If it fails, the failures are a precise list of Java-protocol gaps
worth fixing regardless of Bedrock, because they are gaps a demanding client
found. Days, not weeks. **Do this before anything else.**

**Stage 1 — move the send seam to the typed packet.** Give `NetworkConnection`
a semantic path: the connection receives the typed packet and decides how to put
it on the wire. The Java implementation encodes exactly as it does now.

Worth having alone, independent of Bedrock: it is also what a plugin API needs
to observe or veto outbound packets, what a session recorder needs, and what any
future transport needs. Weeks. Self-contained. No new dependency.

**Stage 2 — RakNet and the Bedrock login.** A Bedrock client reaches the server,
authenticates, and is disconnected with a clean message. Nothing plays yet.
Months. This is the first stage with no value of its own, and the point at which
route B should be re-justified against whatever route A turned out to be worth.

**Stage 3 — the translation, driven by generated data.** Blocks, items,
entities, chunks, inventories. Only startable once the data-source question
above has an answer.

**Stage 4 — the long tail.** Forms, skins, the things Bedrock has that Java does
not and the reverse.

## Recommendation

Do stage 0 now. Do stage 1 on its own merits, because it pays for itself in
three other places whatever happens to Bedrock. Treat stages 2 through 4 as a
separate project with its own decision to make later, once stage 0 has said what
route A is actually worth — and be willing to conclude that the answer is
"ship a Geyser container and spend the years elsewhere".

The failure mode this staging exists to prevent is a half-built Bedrock
translation sitting in `foton-core`, too incomplete to use and too entangled to
remove. `AGENTS.md` calls that a missing foundation, and it is the one outcome
here that would be worse than not starting.

## Open questions

- Does Geyser support protocol 776? Stage 0 answers it.
- Where does Bedrock block, item and entity data come from, such that it is
  generated rather than transcribed? **Unanswered, and route B is blocked on it.**
- What identity does a Bedrock player get on a server keyed by Mojang UUID, and
  what happens to their player data, permissions and bans?
- Does `online_mode = true` mean anything for a Bedrock client, and what should
  a server operator expect it to mean?
