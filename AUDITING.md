# Auditing Foton

How to sweep the whole server for defects, and what each layer is actually
good for. `dev/ci.sh` answers "does it build and do the unit tests pass". That
is one layer of five, and it is the one least likely to find a real bug.

Run the layers in order. Each one costs more than the last and finds different
things, so stopping early is fine as long as you say where you stopped.

## Where to run it

**WSL, on the Windows checkout.** `dev/doctor.sh` reports 11/13 there against
8/13 on Windows: `typos`, `ast-grep` and `prek` live only in the WSL toolchain.
Smart App Control also blocks freshly linked test binaries on Windows with
`os error 4551`, which makes `cargo test` fail for reasons that have nothing to
do with the code.

```bash
wsl -d Ubuntu -u root -- bash -c 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton \
  && export CARGO_TARGET_DIR=/root/foton-target \
  && export PATH=/root/.cargo/bin:/usr/local/bin:/usr/bin:/bin \
  && bash dev/ci.sh'
```

`CARGO_TARGET_DIR` is not optional: without it the Linux and MSVC artifacts
fight over the same `target/debug`.

**The VM does not give memory back.** `~/.wslconfig` grants WSL a slice of
the host -- 20 GB of 31 on this machine -- and a workspace build takes most
of it as page cache. It is never returned while the VM lives: `echo 3 >
/proc/sys/vm/drop_caches` empties the cache *inside* WSL and leaves
`vmmemWSL` exactly as large on the Windows side. Only `wsl.exe --shutdown`
hands it back, and nothing is lost as long as no build is running.

This matters because a Minecraft client is 9 GB. With one open, 20 + 9 leaves
Windows under 2 GB and `cargo test` -- the stage that links nineteen test
binaries -- is killed by the host, over and over, with an error that says
nothing about the real cause. Six runs died that way before the arithmetic
was the obvious suspect. Check `Get-Process | Sort-Object WorkingSet64
-Descending` before believing the build is at fault, shut the VM down between
heavy runs, and lower the `memory=` line while a game is running.

Two invocation traps, each of which has produced a false diagnosis:

- `wsl.exe` expands `$PATH` in the *outer* shell first. The Windows PATH
  contains `Program Files (x86)`, whose parentheses are a bash syntax error.
  Never mention `$PATH` in the command string — write the value out.
- `pgrep -f "dev/ci.sh"` matches the wrapper shell running the `pgrep`. To wait
  for a run, poll its log for the terminal marker instead:
  `until grep -qE "ALL GREEN|FAILURES" ci.log; do sleep 20; done`.

## Layer 1 — static

```bash
bash dev/ci.sh
```

Format, spelling, generated docs, release clippy with `-D warnings`, the unit
tests, the plugin API jar, the native registration check, the test-count ledger
and the `dev/` tooling tests.

Cheap, and the only layer that must be green before anything is merged. It
finds compile errors and lint regressions. It does not find gameplay bugs.

## Layer 2 — in-world

```bash
bash dev/all-tests.sh          # ~70 tests, one server boot each
bash dev/join-test.sh          # just the login → play pipeline
```

This is the layer that tests *the game*. Each script boots a real server on its
own port and speaks the Minecraft protocol to it: joining, containers, mobs,
redstone, loot, death and respawn, the Nether, the End, raids, Bedrock through
Geyser. They run one at a time on purpose — several share a run-directory
naming pattern and tread on each other in parallel.

Read `dev/all-tests.sh` for the list; it is the closest thing to a definition
of "does Foton work".

**Run these from a native filesystem, not through `/mnt/c`.** The run directory
is created next to the checkout, so a Windows checkout driven from WSL puts the
whole world through the 9p bridge. Config generation that takes about four
seconds on `/root` takes minutes there, and the scripts' own timeouts start
firing. Use the WSL checkout for this layer.

**Kill leftover servers first.** An interrupted run keeps its port, and the next
server exits immediately with

    Server startup failed: failed to bind to server port 25565: Address already in use

while the test's client waits for a server that is already gone. The symptom is
a test that hangs with an empty log, which looks like a server bug and is not
one. Use `pkill -f "debug/fo[t]on"` — note the bracket, or the pattern matches
the shell running it and kills that instead, which is the same self-matching
trap as `pgrep` above.

## Layer 3 — vanilla parity

```bash
python3 dev/coverage.py
python3 dev/coverage.py --list entities
```

Cross-checks `foton-core/build/classes.json` against the annotated behaviors.
The build-time ledger `dev/parity-gaps.txt` must agree; when the two disagree
**the ledger is right** and the script has a detection gap.

Coverage counts classes that exist, not classes that are correct. `PARITY.md`
is the inventory that decides whether a system actually works.

## Layer 4 — adversarial

The layer that finds the bugs no linear reading reveals. Two waves of
subagents:

**Wave A — generate scenarios.** Several read-only agents in parallel, each
with a different angle, each asked for the most improbable and hostile
scenarios it can invent, anchored in real files. Angles that have paid off:
the chaotic player (packet spam, actions out of order, disconnecting mid
operation), poisoned data (giant, empty, NaN, recursion bombs, malformed NBT),
concurrency and shared state (lock ordering, entities referenced across
worlds), the hostile environment (disk full, clock going backwards, truncated
region file), security and cheating (what a modified client gets that a vanilla
one does not), and creative cross-system interactions (a leashed mob whose
holder changes dimension; a piston pushing a container whose menu is open).

**Wave B — simulate.** One agent per scenario, or per small batch. Each traces
the scenario through the real code, or writes a throwaway reproducer, and
returns `BUG CONFIRMÉ` with proof or `pas de bug` with the reason the code
holds. No proof, no bug.

**Verify every claim yourself before fixing it.** In the run that produced this
document, one wave-A agent reported that `SChatCommand`'s 32767-byte bound was
128× too large. It is not: vanilla's `readUtf()` *is* `readUtf(32767)`. The
real defect was the opposite one, in the packet next to it. Another blamed a
fall-damage bug on an enchantment calling `applyPostImpulseGraceTime(10)` —
which is exactly what vanilla's `ApplyEntityImpulse` does. Agents are good at
generating suspicion and bad at settling it.

## Layer 5 — dependencies

```bash
cargo audit
```

Install with `cargo install cargo-audit --locked` if missing.

Note that `cargo audit` scans `Cargo.lock`, which can carry crates nothing
builds any more. Check with `cargo tree -e all --invert <crate>` before
concluding a vulnerability is reachable.

### The one advisory that will not go away

`RUSTSEC-2023-0071` (the Marvin attack) against `rsa` reports
`patched: []` and `unaffected: []` -- no released version of the crate fixes
it, and `0.10.0-rc.18` is the latest published. `cargo audit` will keep failing
on it. Do not "fix" it by pinning an older version or by adding an ignore that
hides the reasoning.

What Foton does about it instead: every private-key operation goes through the
crate's blinded API rather than the plain one. `decrypt` and `sign` pass
`DummyRng::None` to the padding scheme, so the private-key operation runs
unblinded and its duration correlates with the key -- which is what the attack
recovers over the network. `decrypt_blinded` and `sign_with_rng` pass a real
RNG, and `algorithms/rsa.rs` branches on it to blind the ciphertext before the
private-key operation.

Blinding is a mitigation, not a proof of constant time. If the crate ever ships
a fix, take it. If a constant-time backend is ever considered, the two call
sites are `foton-login/src/handlers/login.rs` (the exposed one: attacker-chosen
ciphertext, retryable) and `foton-crypto/src/signature.rs`.

## The recursion the chunk reader could not refuse, and the number that lied

`PersistentPoolElement::List` holds a `Vec<PersistentPoolElement>`, and
`PersistentEntity` holds a `Vec<PersistentEntity>`. Both are
`#[derive(SchemaRead)]`, so the reader recursed once per nesting level, and
`wincode` has no depth limit -- `grep -rn depth` over the crate returns
nothing. A crafted or corrupt region file nests tens of thousands of levels in
a few hundred compressed bytes and overflowed the stack.

That is worse than it sounds. A stack overflow is a SIGSEGV, so
`clear_corrupt_chunk_if_unchanged` never ran, the slot was never quarantined,
and the server died again on the next start as soon as something asked for
that chunk. `MAX_DECOMPRESSED_CHUNK_BYTES` did not help: it bounds bytes, and
each level costs thirteen of them.

**It is fixed, and fixing it cost one `unsafe impl`.** Every road led there:
`wincode`'s `SchemaRead` and `SchemaWrite` are `unsafe trait`s, the derive has
no `with`-style attribute to interpose a schema type, and a hand-written reader
is the same `unsafe impl` with more surface. So `chunk_saver::nesting::Nested`
is a newtype on the two recursive edges whose entire body is *count a level,
forward to the inner schema*. It is transparent on the wire, so no saved world
changed meaning. `AGENTS.md` now names it as the second approved `unsafe` site,
next to the keyed `DowncastType`, with the condition that widening it past
delegation ends the approval.

**The number is the part worth remembering.** The obvious ceiling was vanilla's
512, from `NbtAccounter.MAX_STACK_DEPTH`, and it would have been a stack
overflow dressed as a limit. Vanilla's NBT frames are a hundred bytes or so;
a derived `SchemaRead` frame here is kilobytes. Measured on the 2 MiB stack
tokio gives a worker, a debug build decodes 96 levels and overflows at 128 --
so a 512 ceiling never fires, and the guard would have been decoration.

The ceiling is **32**, and it comes from the data rather than from vanilla's
constant: the deepest `list_pool_element` nesting in the whole of vanilla's
built-in datapacks is **1**, measured across every JSON in
`foton-utils/build_assets/builtin_datapacks` and `minecraft-src`. 32 is
thirty-two times the deepest real value and a third of the measured debug
ceiling, which leaves the frames already on the stack when the recursion starts
their own room. The trade-off is named rather than hidden: a world that really
did stack more than 32 riders loses that chunk's entities on load, which is the
price of not dying to a signal no handler can catch.

Measuring the read path also found the half nobody had looked at. **The save
path recursed too, with no guard at all** -- `entity_to_persistent` walks
`entity.passengers()`, and `wincode::serialize` on the way out overflows the
same 2 MiB stack between 128 and 256 levels. A player who stacked a few hundred
entities with `/ride` killed the server at the next autosave, without any
corrupt file involved. It now stops at the same ceiling and logs what it
dropped, because writing a chain the reader will refuse saves a chunk that can
never be loaded again.

Three regression tests hold it: the bomb is refused rather than decoded, a
chain exactly on the ceiling still decodes *on a thread with production's
stack size*, and a refusal gives its depth budget back so one bad chunk cannot
poison the good ones after it on the same worker thread.


## Two more the audit found and left alone

Both are real, both are bounded, and both would take a worse change to fix
than to live with. They are written down so the next pass does not spend its
budget rediscovering them.

**The chunk storage refcount has no owner.** `acquire_chunk` bumps a region's
`loaded_chunk_count` and `release_chunk` drops it, and the unload path only
releases `if has_chunk` -- so a generation task that acquires and then dies
before installing anything leaks the count, and the region file stays open
with its header unflushed until shutdown. The clean fix is an RAII guard, and
`release_chunk` is `async`, which `Drop` cannot be; spawning a task from a
destructor to work around that is worse than the leak. What keeps it small is
`panic = "abort"`: in release a panicking task takes the process with it, so
the recovery branch that leaks only exists in debug builds.

**The NBT heap quota is charged after the fact.** Vanilla's `NbtAccounter`
bills each tag while parsing and gives up partway; Foton builds the whole tree
and then measures it, so the peak is reached before the value is rejected.
Charging incrementally means accounting inside `simdnbt`, which is not ours.
The exposure is bounded rather than open-ended: the wire side is capped at
`MAX_COMPONENT_BYTES` and, since a decode failure now disconnects the way
vanilla does, a client gets one attempt rather than a loop.

## One the audit decided, and one still for the project

**A chunk that fails to decode is no longer erased -- it is copied aside.**
`load_chunk` still treats any decode error as corruption: it blanks the header
entry, fsyncs, and returns `Ok(None)`, so worldgen fills the hole. Vanilla is
lenient instead -- `NbtUtils` turns an unknown block into air and drops an
unknown property, and the rest of the chunk survives.

The strictness is deliberate: two tests name it,
`unknown_referenced_block_state_is_corruption_instead_of_air_recovery` and its
biome twin. What no test covered was the *erasure* that followed. Together they
meant a registry drift -- a block gaining or losing a property between versions
-- destroyed the world chunk by chunk, on disk, irreversibly.

This file asked the smaller question first: separate "refuse to load" from
"delete what is on disk". That is now answered.
`clear_corrupt_chunk_if_unchanged` copies the chunk's raw bytes to
`<region dir>/corrupt/<region>.<index>.chunk` before it clears the slot, and
says so at `warn!`. The copy is best-effort by design -- a chunk that can
neither be decoded nor copied still gets cleared, because otherwise it fails
identically on every load forever -- but the silent destruction is gone. The
cost of a false positive is now a column to restore by hand instead of a column
destroyed, which is what made the nesting ceiling safe enough to adopt: a
refusal there is a decode failure, and it would otherwise have taken the
player's build with the riders.

Reversing the decode *strictness* is still a design call and still open. It is
now a smaller one, because being strict no longer costs anything irreversible.

**A `FORMAT_VERSION` bump discards every region file.** Opening a region whose
version is not the current one renames it to `.srg.v<n>.bak` and creates an
empty replacement. No reader for an earlier version exists, and nothing can
reintroduce the `.bak`. The twenty-two increments so far have each made every
existing world unreadable, announced only by a `warn!` per region. That may be
the right trade while the format still moves, but the code nowhere says so.

## Why the FFI boundary carries no `catch_unwind`

An audit of `foton-plugin` will notice that none of the 496 `extern "system"`
natives wraps its body in `catch_unwind`, and that this is the textbook way to
stop a panic crossing an FFI boundary. Adding them would be wasted work here.

The release profile sets `panic = "abort"`. Under it a panic aborts the process
before any unwinding happens, so `catch_unwind` has nothing to catch -- the
wrapper would be dead code in the only profile that ships, while every new
native would have to carry it.

What protects this boundary is not panicking, and that discipline already
holds: there is no `unwrap`, `expect`, `panic!` or `todo!` in the production
paths of `natives.rs`, and every `slot`/`index` native converts through
`usize::try_from` and tests against the container's size before indexing. The
one exception the audit found -- a shaped recipe whose width was counted in
bytes while its pattern was filled in characters -- was fixed at the cause, and
`ShapedRecipe::matches_at` now indexes through `get` as well, because a recipe
arriving from a plugin is not the registry's to trust.

The pending-exception finding is closed. `jni`'s `check_exception!` turns a
Java exception into `Err(Error::JavaException)` and leaves the exception itself
set on the thread -- it does not call `ExceptionClear` -- and every call in
`forward.rs` swallows its `Err`, because an event bridge has nobody to report
to. Nothing else cleared it either, so the next JNI call on that thread ran with
the previous one's exception pending, which JNI documents as undefined for most
functions: a plugin whose handler threw once looked broken everywhere
afterwards.

The fix is worth describing because the shape of the problem suggested a worse
one. Seventy sites swallow an error, but all seventy-one callbacks pass through
one attach, so `BridgeEnv` wraps the `AttachGuard` and clears any pending
exception when the scope ends -- describing it first, since the stack trace is
the only thing that names the handler that threw, and it goes to the JVM's own
error channel. One place instead of seventy.

The other boundary finding is closed too, differently. `Native.requestChunk`
stored a `ChunkRequestHandle` -- which pins a chunk ticket and releases it on
drop -- under a fresh UUID that only `chunkRequestReady` could retire, so a
plugin that asked and never polled, or that was unloaded mid-poll, pinned a
chunk for the life of the server. Entries now carry the time they were made and
both natives sweep anything older than a minute.

The minute is a judgment, not a transcription, and the code says so. There is
no vanilla counterpart to check it against: the polling shape is Foton's own,
where Paper hands back a future that releases on completion or cancellation
without the plugin having to come back. Being wrong costs a plugin a second
request; doing nothing costs a chunk forever.

## The light engine is not vanilla's, and two findings depend on knowing that

Foton's sky and block light are a port of `ScalableLux`, not of vanilla's
`SkyLightEngine`. Both produce the same light -- `chunk_stage_hashes` compares
the actual light bytes of generated chunks against vanilla references -- but
they get there by different means, and an auditor who assumes vanilla's design
will file two bugs that are not bugs.

**`ChunkSkyLightSources` is maintained and never read in production.** Every
block change whose light properties differ calls `update_sky_light_sources`,
which takes a write lock and may rescan the column; the only readers of
`get_lowest_source_y` are tests, `chunk_stage_hashes` among them (that module is
`#[cfg(test)]`). In vanilla the cache is load-bearing: `SkyLightEngine.checkNode`
reads `getLowestSourceY` to decide whether a node is a source at all. In a
`ScalableLux` port nothing needs it.

Deleting it would be the wrong conclusion. The worldgen parity test compares
Foton's cached source Y against vanilla's own, column by column, for every
generated chunk -- that is real parity coverage of a vanilla structure, and it
only exists because the cache is kept current. The cost is bounded: `update`
returns immediately when the changed block sits below the column's current
source edge, which is the common case underground. Keep it, and know why it is
there.

**A light workset write-locks 25 chunks where 9 are editable.**
`with_light_edit` takes `light_mut()` on every slot of the 5x5 cache window,
though propagation can only write within the inner 3x3. Narrowing it to nine
write locks and sixteen read locks is a genuine optimization and was left
undone: it is a contention change, not a correctness one, and it would be made
blind -- nothing here has been profiled.

There is no deadlock in the current shape, and that is not an accident.
`LightWorkWindowGate` excludes overlapping cache windows outright, so two
worksets never hold locks on the same chunk. Even without the gate the slot
order is a global lexicographic order on (z, x), so two overlapping windows
would acquire their shared chunks in the same relative order. Any narrowing has
to preserve both properties.

## One the audit proved and did not close

**An entity cannot be teleported into a chunk the entity manager does not
track.** `WorldEntityManager::can_move_manager_owned_to_chunk` requires the
destination in `chunk_visibility`, so `/tp @e[type=pig,limit=1] 5000 64 5000`
into unloaded terrain moves nothing. Vanilla's
`PersistentEntitySectionManager.Callback.onMove` never refuses a move.

Only half of that was fixed. `/tp` used to report every target as teleported
whatever happened, so the operator was told the opposite of the truth; it now
counts and announces the ones that actually moved.

The move itself is still refused, and a later pass established *why* it cannot
be a patch -- the first answer this file gave was wrong. It said to copy
vanilla's `TicketType.POST_TELEPORT`; **that ticket does not exist in 26.2**.
`TicketType.java` registers `PLAYER_SPAWN`, `SPAWN_SEARCH`, `DRAGON`,
`PLAYER_LOADING`, `PLAYER_SIMULATION`, `FORCED`, `PORTAL`, `ENDER_PEARL` and
`UNKNOWN`, and nothing for teleports. Vanilla tickets nothing here: `onMove`
always moves, `chunkVisibility` returns `HIDDEN` by default for a chunk with no
entry, and `updateStatus` merely calls `stopTracking`. The entity stays in
`sectionStorage`, and `autoSave` writes it out -- for a `HIDDEN` chunk by
calling `processChunkUnload`, which is the case it exists to handle.

The reason Foton cannot copy that is one line of its own design, stated in
`chunk_saver/format.rs`: *"Unlike vanilla which stores entities in separate
region files, Foton stores entities inline with chunk data for simplicity."*
Vanilla can write an entity in a hidden chunk because its `EntityPersistentStorage`
is a separate file that needs nothing from the chunk. Writing one here means
writing the whole chunk record, which needs the chunk's blocks, which is exactly
what is not loaded. `saveable_entities_outside_chunks` already measures the
consequence and logs it, so the hazard is instrumented rather than hypothetical.

So the choice is real and it is a project's, not a patch's: either the teleport
becomes asynchronous and holds a load ticket on the destination until the chunk
arrives, or entity storage is separated from chunk storage the way vanilla has
it. The portal path is not a precedent for the first --
`TeleportPostTransition::place_portal_ticket` runs *after* the transition, and
portals only work because the portal search has already loaded the destination.

## The one that looked like an architecture change and was not

Chunk serialization does not run on the world's tick thread, and one window in
the tick made that visible. `ChunkMap::tick_game` calls
`collect_scheduled_block_ticks` and `execute_scheduled_block_ticks` as two
statements; the collect half already removes the due ticks from their
`ChunkTickContainer`, so between the two they exist only in the batch.
`World::request_save` meanwhile spawns `save_all_chunks` on the chunk runtime,
and `ChunkTickContainer::snapshot` read only the containers. A save landing in
that window wrote a chunk with neither the pending ticks nor the state changes
they were about to make.

Vanilla cannot reach this state: `ChunkMap.save` calls
`SerializableChunkData.copyOf` synchronously on the server thread, and only the
disk write is asynchronous, so serialization is atomic with respect to
`ServerLevel.tick`. Notably vanilla does *not* solve it by packing its in-flight
set -- `LevelChunkTicks.pack` reads `tickQueue` and the unpacked `pendingTicks`
and never touches `LevelTicks.toRunThisTick`. Its correctness comes from the
barrier, not from the contents.

**The fix was neither the barrier nor a rewrite.** The batch already existed on
the world -- `begin_scheduled_block_tick_batch` registers it so
`willTickThisTick` queries can see it -- and it already carried the executor's
cursor. `scheduled_tick_snapshot` now appends the batch entries **at or after
that cursor** for the chunk being written. The cursor is what makes it exact
rather than a trade: everything before it has already run and its effects are in
the chunk, so adding those would re-run them on load; everything at or after it
has not run, so omitting those loses them outright. A test pins both halves,
because getting only the first half right looks identical until a crash.

Two things are still true and worth keeping. The drain and the batch
registration are separate operations, so a save landing exactly between them
still misses that tick -- a window of two adjacent statements rather than an
entire execution phase. Closing it means holding the batch lock across the
drain, which is a change to the tick loop's structure and was not worth the
risk for the remaining nanoseconds. And a tick that is *executing* while the
snapshot reads sections can still be seen half-applied; that is the generic
non-atomicity of reading state off the tick thread, not this bug.

The smaller sibling named here is fixed too. `FullChunkRef::scheduled_tick_snapshot`
read `world.game_time()` *before* taking the container lock, so a tick landing
between the two shifted every persisted `delay` by one. `snapshot` now takes the
clock as a closure and calls it under the same lock the lists are read under.
That is sound rather than merely narrower: the collect half takes that lock, and
the clock advances before it does, so any ordering where the lists have already
lost their due ticks is one where the clock has already moved past them.


## The layer this audit kept skipping

Twelve changes went in against `cargo check`, `cargo clippy` and 5438 unit
tests, and none of that starts the server. Four of them could not be proven that
way at all: connections are ticked per connection now rather than per
world-resident player, so every online player's keep-alive depends on a call
that moved; a duplicate login displaces the incumbent from inside the task that
`queue_player_join` already spawned, which moved the reservation off the
synchronous path; a packet that will not encode disconnects its client instead
of aborting; and the compression threshold is clamped on the way to
`CLoginCompression`. All four sit on the path a client walks to enter the world,
and all four are invisible to layer 1.

`dev/smoke-test.sh` and `dev/join-test.sh` are ten minutes and they answer it:
a real client reaching `JOIN STATUS: OK`, with compression negotiated, an entity
id assigned and its chunks delivered. Run them before believing a green CI on
anything that touches login, connections or packet sending. `PARITY.md` opens by
saying a system that exists is not a system that works; the same is true of a
system that compiles.

## Known limits of this method

- **`panic = "abort"` in release** (`Cargo.toml`) means every `expect()` on a
  live path is a server kill with no unwinding, so `shutdown_worlds()` never
  runs and dirty chunks are lost. Treat any reachable `expect` as a data-loss
  bug, not a style problem.

  `Cargo.toml` sets `unwrap_used = "warn"` and CI runs clippy with
  `-D warnings`, so `unwrap` is effectively denied -- which is exactly why the
  workspace holds roughly 1400 `.expect()` calls. The tests were written with
  `expect` because `unwrap` was closed to them, and that is what makes the lint
  impossible to switch on workspace-wide in one move.

  **Every crate now carries** `#![cfg_attr(not(test), warn(clippy::expect_used))]`,
  and CI runs clippy with `-D warnings`, so a reachable `expect` in production
  code fails the build. The one exception is `foton-macros`, deliberately: it is
  a proc-macro crate, its `expect`s run inside the compiler, and
  `panic = "abort"` has no bearing there.

  The `not(test)` scope is what made this possible at all. The workspace holds
  some 1400 `.expect()` calls because `unwrap_used` was already denied and the
  tests had to write *something*; demanding the lint of test code would mean
  rewriting hundreds of call sites for no safety gained, since a panicking test
  is how a test reports and a panicking server is how a world is lost. It costs
  one wrinkle worth knowing: a bare `#[expect(clippy::expect_used)]` is
  unfulfilled in the test build, so a justified site needs
  `#[cfg_attr(not(test), expect(clippy::expect_used, reason = "..."))]`.

  Ninety-six production sites, **fourteen of them live defects**. The recurring
  one is worth naming because it appeared nine times in three different crates:
  `EncodedPacket::from_bare(...).expect(...)` on a send path. `from_bare`
  refuses anything past `MAX_PACKET_SIZE`, and a container of written books or a
  command tree grown by plugins gets there, so every one of those was a player
  able to kill the server by opening a chest. Vanilla does not die on this --
  `PacketEncoder.encode` throws, `Connection.exceptionCaught` drops that one
  connection -- and that is what they all do now.

  The rest: a compression threshold above `i32::MAX` killing the server at the
  first login; four separate `by_state_id(id).expect("Invalid state ID")` panics
  on ids the caller supplies, one of them inside `try_get_property`, whose
  `try_` prefix already promised an answer instead of a panic; a console
  `read()` that aborted when stdin was closed; `char_pos` panicking on an empty
  input line; and chunk loading panicking on a truncated or corrupt region file,
  where the function already returned `io::Result` and the loader already knew
  what to do with a chunk it could not read.

  Two lessons are worth keeping. **Do not count `.expect()` with grep** -- the
  crate-by-crate count that guided three earlier passes was wrong three times,
  twice too low and once by a factor of six, because "files with no
  `#[cfg(test)]` module" is not the same question as "code compiled outside
  tests". And **a repeated `expect` is an invariant asking for a name**:
  eighteen inventory slot methods each repeated the same one, and the answer was
  not eighteen annotations but four accessors on `ContainerLockGuard`, after
  which the eighteen call sites had no `expect` at all.

- **`panic!` was the other half of the `expect` problem, and it is now denied in
  every crate that runs in the server -- eleven of the twelve.** The twelfth,
  `foton-macros`, is deliberately exempt: a proc-macro crate runs at build time,
  where a `panic!` is a compile error carrying a message, which is how a macro
  is *supposed* to reject bad input. `expect_used` says nothing about `panic!`,
  `unreachable!` or `todo!`, which under `panic = "abort"` kill the server just
  as dead -- one of them sits on the chunk *save* path, where dying costs
  exactly the chunks being written. Measured before adopting: six crates had
  none at all, `foton-utils` and `foton-protocol` four each, `foton-worldgen`
  ten, `foton-registry` thirty-nine. All annotated or fixed.

  `foton-core` had **303** -- the 249 first counted missed the `unreachable!`
  half, the same grep mistake this file warns about twice. They were
  concentrated: 69 in `worldgen/feature/configured.rs`, 20 in
  `worldgen/region.rs`, 11 each in `worldgen/stages/light.rs` and
  `leaf_distance.rs`. It took eight passes to reach zero, and what came out is
  **240 annotations across 89 files carrying 31 distinct reasons** -- one
  annotation often covers several panics in the same function, and one reason
  often covers a whole family of functions that assert the same thing.

  The 57 sites outside worldgen were read first, on the reasoning that a panic
  over extracted feature data shows up on the first generated chunk while one on
  a save, load or network path waits for a player. Exactly one was reachable
  from input the server does not control, and it is fixed: `Chunk::from_disk`
  refreshed the light emptiness maps and panicked when the section count the
  light storage was sized for -- from the world's height -- disagreed with the
  sections the file supplied. A chunk saved under a different world height, or a
  truncated one, aborted the server mid-load and took every other dirty chunk
  with it. `try_persistent_to_chunk` now refreshes first and returns the load
  error it already had. **One live defect in 303 sites** is the honest yield of
  this pass, and it was not in the bulk -- it was in the fifty-seven read first.

  The worldgen bulk split in two, as the sample predicted. Sixty-nine of the 69
  in `worldgen/feature/configured.rs` are one sentence --
  `panic!("<feature> placer received wrong configured feature kind")` -- a
  single genuine invariant, since the registry pairs each placer with its own
  config type; one reason covers all of them honestly. The rest were varied and
  were read one at a time: bulk-section cache indices in `region.rs`, step
  declarations and published status, coordinate conversions and missing chunks
  in the stages, registry lookups in `runner.rs`. That distinction is the whole
  method -- an annotation is only worth having if its reason is true, so the
  families were annotated wholesale and everything else individually.

  The 56 remaining non-worldgen sites are internal invariants: command-chain
  state, packet-lane bookkeeping, heightmap types. One is still a lead rather
  than a chore, and is deliberately left as one --
  `inventory/menu/mod.rs` repeats "the explicitly locked player inventory must
  be present" seven times, which is the same shape as the eighteen slot methods
  that turned into four accessors on `ContainerLockGuard`. A repeated assertion
  is an invariant asking for a name, and naming it is worth more than seven
  annotations.

  Three process notes, all earned the hard way in the same sitting. Attaching
  `clippy::panic` and `clippy::unreachable` together "to be safe" left thirteen
  expectations unfulfilled, then thirty-seven, and the compiler said so both
  times -- an annotation wider than the thing it justifies will one day cover a
  site that deserved to be caught. The fix that finally held was to read the
  source line and derive the lint from it *before* writing the annotation,
  rather than annotating and narrowing afterwards.

  Eleven crates were checked with `cargo clippy -p <crate> --lib` in debug
  while the gate is `cargo clippy -r --workspace --all-targets --all-features`;
  two `unreachable!` in `foton-login` walked straight through the gap. Verify
  with the command that decides, not a narrower one.

  And a script that finds functions by regex must be checked against what it
  did not match. The pattern `pub(\([a-z()]*\))?` silently skipped four
  functions declared `pub(in crate::worldgen::feature)` -- lowercase letters and
  parentheses do not cover a path. The count of sites the script *reports*
  finishing is not the count of sites there are; only the compiler knows that,
  which is why every pass ended on a clippy run and not on the script's output.

- **`clippy::indexing_slicing` is not the answer to unchecked indexing, and it
  was measured rather than guessed.** The corrupt-region-file panic this audit
  fixed was an unchecked `runtime_palette[index]` sitting next to an `expect`,
  and `expect_used` could not see it -- which suggests turning the indexing lint
  on next. Do not: it reports 101 sites in `foton-utils` alone, the smallest
  crate that has any, and unlike `expect`, most indexing here is provably in
  bounds by construction -- fixed-size arrays reached through masked indices,
  chunk-local coordinates already `& 15`. The signal would be buried. The class
  is real but the lint cannot isolate it; the place to look is any `[]` on a
  length that came from a file or a packet.

- **A guard on one side of a format is half a guard, and the missing half is
  usually worse than no guard at all.** The nesting ceiling was added to the
  reader and to the entity writer in one commit, and the pool-element writer was
  missed. That is not a smaller version of the same bug: a reader-only ceiling
  refuses bad input, while a writer that can emit past it *creates* input its
  own reader refuses -- and here a refused chunk is quarantined and regenerated,
  so the gap cost the whole column. Whenever a limit is added to a serialized
  format, the question is not "is the limit right" but "can anything on the
  write side exceed it".

  The test that caught it states the property rather than comparing the
  constants: *whatever the writer produces, the reader takes*. It found an
  off-by-one that reading the code twice had not, because the two sides are
  genuinely asymmetric -- refusing a passenger drops it and spends no wrapper,
  while an over-deep pool element still returns an `Empty` that its parent
  wraps. Two correct answers, `>` and `>=`, and no way to see that by comparing
  ceilings.

- **The chunk-storage refcount analysis above was re-checked, and it holds.**
  Every exit from `apply_empty_step` after a successful `acquire_chunk` was
  enumerated: the two cancellation paths release, and both `None` returns inside
  `apply_existing_empty_step` release before returning. The leak really is
  confined to a task dying mid-flight, which `panic = "abort"` makes a release
  impossibility. Worth recording that it was verified rather than assumed --
  a residual nobody re-checks slowly becomes a residual nobody believes.

- **A stack overflow is not a panic.** It is a SIGSEGV, so no `catch_unwind`
  and no abort handler sees it. Recursive parsers need an explicit depth
  ceiling -- but vanilla's 512 (`NbtAccounter`) is a number about *vanilla's*
  frames. Measure your own before borrowing it; ours overflow at 128.
- Coverage percentages say nothing about correctness. See `PARITY.md`.
