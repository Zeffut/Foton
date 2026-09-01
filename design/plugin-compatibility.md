# Bukkit-family plugins — design

Where Foton stands on running Bukkit, Spigot, Paper and Purpur plugins, what
blocks it, and how the surface gets bounded. Written 2026-08-31. Nothing here is
built yet.

## What is actually being asked for

A Bukkit-family plugin is a JVM jar compiled against `org.bukkit.*`, discovered
from a `plugins/` directory, described by a `plugin.yml`, and driven by an event
bus. Paper and Purpur add their own API on top and keep Bukkit's underneath.

Foton is Rust. So this needs a JVM in the process, and it needs `org.bukkit`
implemented in terms of Foton's game state.

Two things have to be said before anything else, because they bound the whole
problem.

**Thirty percent of them reach past the API.** Measured, not estimated: of the
fifty-nine most-downloaded server plugins, eighteen reference `net.minecraft.*`
or a server implementation's internals. Those cannot ever work here — the
classes they reach for do not exist, and making them exist would mean shipping
Mojang's server inside Foton.

So the honest ceiling on this whole effort is around seventy percent of the
popular ecosystem, before a single line is written. That number belongs beside
every coverage claim this project ever makes about plugins, the way `PARITY.md`
keeps its caveat beside its percentages.

**"Compatibility" is a spectrum, not a state.** Nobody will implement all of
`org.bukkit` — it is on the order of fifteen hundred public types before Paper's
additions. The question is not whether the surface is complete but *which parts*
are done and how that is known. This document's main proposal is an answer to
that question.

## Three routes

### A. Embed a JVM and implement the Bukkit API

Run a JVM in-process through JNI. Ship `org.bukkit` interfaces whose
implementations call into Foton. Load real plugin jars.

This is the only route that runs an existing plugin unmodified. It is also the
one with a JVM in the address space of a Rust server whose selling point is that
it is not one.

### B. A native Rust plugin API, and no Bukkit at all

What `AGENTS.md` already anticipates — "future plugins should register their own
typed blocks/features with their own refs", and the keyed downcasting foundation
exists so that a plugin can own a namespace. The architecture has been pointed
this way from the start.

It does not run a single existing plugin, which is the entire value being asked
for. As a route on its own, it answers a different question.

### C. A native API first, with Bukkit as its first consumer

Build the Rust plugin foundation — events, object model, scheduler, lifecycle —
and then implement `org.bukkit` as a *client* of it rather than as the thing
that shapes Foton's internals.

This is the recommendation, for a reason that is not compromise: it forces the
native API to be good. An API with no demanding consumer drifts into whatever
was convenient to expose. Bukkit is a specification written by twelve years of
other people's requirements, and holding the native layer to it is the cheapest
available design review.

It also means route B falls out for free, and that a plugin written natively is
not a second-class citizen sitting on a Bukkit shim.

## What actually blocks this

Not JNI. The `jni` crate is mature and the mechanics of calling between Rust and
Java are well understood. The blockers are in Foton.

### 1. There is no event system

Verified: `foton-core/src/world/events.rs` broadcasts vanilla level events —
block destruction progress and the like. `foton-core/src/world/game_event/` is
the vanilla game-event system that sculk sensors listen to. Neither is an
extension point. Nothing in `foton-core` lets outside code observe, mutate or
veto a state change.

Bukkit is an event bus with a game attached. Without this, nothing else matters.
This is the foundation, and it is missing.

### 2. Object identity across the boundary

A plugin holds a `Player` and calls methods on it, possibly minutes later,
possibly from another thread. Foton's entities are `Arc<dyn Entity>` with `Weak`
to break cycles, and a player who logged out is gone.

So the bridge needs stable handles with defined lifetimes and a defined answer
for "the thing this refers to no longer exists" — which in Bukkit is usually a
stale object that quietly does nothing, a semantic worth reproducing rather than
improving on, because plugins depend on it.

### 3. The threading model

Bukkit promises a main thread and a scheduler that gets you back onto it.
Foton's gameplay tick is synchronous by design, and `AGENTS.md` is explicit that
ticks must never wait on async work. A plugin calling `world.getBlockAt()` from
its own thread is ordinary Bukkit usage, and it must not be able to corrupt
chunk state by doing so.

This is the part most likely to produce subtle, rare, terrible bugs, and it
should be designed before any API method is written, not after.

### 4. Registration happens once, at startup

`BLOCK_BEHAVIORS` and `ITEM_BEHAVIORS` are `OnceLock`s filled by
`init_behaviors()` and frozen. That is workable — a plugin load phase before the
freeze is a normal thing to add — but it means plugin loading has a fixed place
in startup order, and that a plugin cannot register a block at runtime. Both are
fine, and both are decisions to make on purpose rather than discover.

## Bounding the surface, with measurement instead of guesswork

This is the part of this document worth keeping even if everything else is
rewritten.

`org.bukkit` cannot be implemented by working through it alphabetically, and it
should not be implemented by guessing what plugins use. It should be implemented
in the order that a corpus of real plugins actually calls it.

The method:

1. Take the most-downloaded plugins from the public repositories.
2. Walk each jar's class constant pool for method and field references into
   `org/bukkit/`, `io/papermc/` and `net/minecraft/`.
3. Rank by how many distinct plugins reference each member — not by call count,
   because one plugin calling something a thousand times is still one plugin.
4. Implement in rank order.

What this produces, beyond a work queue:

- **A coverage ledger**, in the same shape as `PARITY.md`: what proportion of
  the API that real plugins actually touch is implemented. That number is
  meaningful in a way that "we have implemented 400 of 1500 types" is not.
- **A per-plugin verdict.** For any given jar, the references it makes are known
  and so is which of them exist. "This plugin needs eleven things Foton does not
  have, and here they are" is answerable before the plugin is ever run.
- **An early, quantified answer to the NMS question.** The proportion of the
  corpus that reaches past the public API is measurable rather than asserted,
  and it is the honest ceiling on how much of the ecosystem can ever work.

This is the repository's existing discipline — nothing is guessed, facts are
generated, coverage is published with its caveats — applied to a second
specification. It belongs beside `dev/coverage.py` and `dev/parity-gaps.txt`,
and the ledger belongs in the same generated-and-committed shape.

The corpus analysis should be built *first*, before the API, because it decides
what the API is.

### It has been built, and here is what it says

`dev/plugin_api_usage.py` reads the constant pool of every class in every jar of
a corpus and ranks what it finds; `dev/plugin-api-usage.json` is the committed
ledger from a first run over the fifty-nine most-downloaded server plugins.
6,345 distinct API members are referenced; 2,509 of them by more than one
plugin.

Three findings change the plan that was written above them.

**The first event tranche is no longer a guess.** By how many of the fifty-nine
need them: `PlayerJoinEvent` (40), `PlayerQuitEvent` (36), `PlayerInteractEvent`
(15), `PlayerMoveEvent` (14), `InventoryClickEvent` (13), `BlockPlaceEvent` (11),
`AsyncPlayerChatEvent` (10), `PlayerTeleportEvent` (10). Two events reach two
thirds of the corpus. That is a much smaller stage 1 than "an event system"
sounded like.

**Paper is not optional.** The open question at the end of this document asked
whether Paper and Purpur were in scope. They are: eighteen plugins use Paper's
regionised scheduler and fifteen use its plugin lifecycle events. A
Bukkit-only implementation would fail a third of the corpus for reasons that
have nothing to do with gameplay.

**A large slice of the first tranche never touches Foton at all.**
`YamlConfiguration` is referenced by thirty-four of the fifty-nine, and it is
pure library code — file parsing, defaults, saving. So is much of
`org.bukkit.plugin` and `PluginDescriptionFile`. That part of the surface can be
built and tested with no game running and no event system in place, which makes
it the cheapest real progress available.

The ranking by package says the same thing in one line, and it is not the order
anyone would have guessed: plugin lifecycle, then `org.bukkit` itself, then
entities, commands, player events, the scheduler, configuration, inventories,
and only then blocks.

## Staging

**Stage 1 — the event system.** Native, in Rust, with Bukkit's semantics as the
design constraint so that stage 4 does not have to reshape it: priorities,
cancellation, mutation of the event's own fields, and a defined dispatch point
in the tick. Complete in itself and useful to Foton without any plugin: it is
also what a scripting layer, an audit log or an anti-cheat hook would need.

**Stage 2 — the analysis. Done.** `dev/plugin_api_usage.py` and its committed
ledger. It ran before stage 1 was designed, which is the only reason stage 1 is
now eight events rather than a category.

**Stage 3 — the plugin host.** Discovery, lifecycle, a scheduler with the
threading contract from blocker 3, configuration, logging, and the native
registration window before `init_behaviors()`.

**Stage 4 — the JVM bridge and `org.bukkit`, in measured rank order.** Handles,
thread affinity, and then the API surface, publishing coverage as it goes.

## What this costs, measured

The question "how much of the ecosystem runs" now has a curve rather than an
opinion. `dev/plugin_api_usage.py --write` computes it and
`dev/plugin-api-usage.json` carries it.

| API members implemented | Plugins that run whole | Median plugin covered |
|---|---|---|
| 100 | 1 / 59 | 37 % |
| 500 | 1 / 59 | 76 % |
| 1 000 | 4 / 59 | 86 % |
| 2 000 | 13 / 59 | 95 % |
| 3 000 | 17 / 59 | 97 % |
| **3 500** | **33 / 59 (55 %)** | 100 % |
| 4 000 | 40 / 59 (67 %) | 100 % |
| 4 547 (all) | 41 / 59 (69 %) | 100 % |

Read the two columns together, because either alone lies. The left counts
plugins whose *every* referenced member exists, which is pessimistic: the JVM
resolves lazily, so a missing method breaks the line that calls it rather than
the plugin that ships it. The right is how much of the median plugin is
covered, which is optimistic for the mirror reason — covering most of a plugin
is not the same as it working. The truth is between them, and nobody can place
it without running the plugins.

Three things follow.

**Getting past half the ecosystem costs about 3 500 API members.** Not a
category and not a milestone: a countable list, already ranked, sitting in
`dev/plugin-api-usage.json`. Sixty-nine percent is the ceiling, and 3 500 buys
most of the way there because the curve is nearly vertical at the end.

**The curve is flat for a long time and that is not a reason to stop.** One
plugin runs whole at five hundred members while the median plugin is already
three quarters covered. That gap is the shape of the problem: plugins need
roughly ninety members each and they are not the same ninety. Almost nothing
looks like progress until quite late, and almost everything is.

**The ranking has to be computed over plugins that could run.** A member only
the internals-reaching plugins want is work that serves nobody; counting them
raises dead work up the queue and flattens the curve. Which is exactly what the
first run of this did, before it was corrected.

Stage 1 is weeks. Stage 2 took an afternoon. Stage 3 is months. Stage 4 is
3 500 members, tracked against a number that says where it stands at any point.

## Risks

- **A JVM in the process.** Memory footprint, GC pauses that a Rust server was
  chosen to avoid, and a crash surface that is no longer Rust's. An operator who
  runs no plugins should pay none of it, which means the bridge is an optional
  component and not a dependency of `foton-core`.
- **Bukkit's semantics leaking inward.** The event system exists to serve
  Foton first. If stage 1 is designed backwards from `org.bukkit.event`, Foton
  inherits twelve years of another project's compromises in its core. The
  constraint is that Bukkit's semantics must be *expressible*, not that they are
  the model.
- **Publishing a compatibility number that overstates reality.** The same risk
  `PARITY.md` already names for vanilla coverage, and it needs the same
  treatment: the caveat travels with the number, and the NMS share is stated
  beside it.
- **Licensing.** Bukkit's API is GPL-3.0, Paper's is a mix, Foton is
  AGPL-3.0-or-later. Combining them is plausibly fine, and reimplementing an
  interface is not the same act as copying it — but "plausibly fine" is not a
  standard to ship on. This needs a real opinion before any of stage 4 is
  published, and it is cheap to get early.

## Open questions

- ~~Which Paper and Purpur APIs are in scope?~~ Answered by the corpus: Paper is
  required. Purpur did not appear in the corpus at all and can wait for evidence
  that anyone needs it.
- Does a plugin get to register blocks, items and entities, or only to react?
  The registry freeze makes this a startup-order decision, and it changes what
  the native API has to be.
- What happens to a plugin that calls something unimplemented — an exception it
  can catch, or a refusal to load with a list? The second is kinder to an
  operator and harsher to a plugin author, and it is the one this project's
  temperament points at.
