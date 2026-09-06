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

## The recursion the chunk reader cannot refuse

`PersistentPoolElement::List` holds a `Vec<PersistentPoolElement>`, and
`PersistentEntity` holds a `Vec<PersistentEntity>`. Both are
`#[derive(SchemaRead)]`, so the reader recurses once per nesting level, and
`wincode` has no depth limit -- `grep -rn depth` over the crate returns
nothing. A crafted or corrupt region file nests a few tens of thousands of
levels in a few hundred compressed bytes and overflows the stack.

That is worse than it sounds. A stack overflow is a SIGSEGV, so
`clear_corrupt_chunk_if_unchanged` never runs, the slot is never quarantined,
and the server dies again on the next start as soon as something asks for
that chunk. `MAX_DECOMPRESSED_CHUNK_BYTES` does not help: it bounds bytes,
and each level costs about two of them.

It is unfixed on purpose. The natural fix -- a newtype on the recursive edge
that counts depth -- means implementing `wincode`'s `SchemaRead`, which is an
`unsafe trait`, and `AGENTS.md` allows `unsafe` only in the keyed
`DowncastType`. So the real options are an upstream depth limit in `wincode`
or a hand-written reader for those two types, and both are calls for the
project to make rather than something to improvise mid-audit.

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

## Two the audit found and left to the project to decide

**A chunk that fails to decode is erased and regenerated.** `load_chunk` treats
any decode error as corruption: it blanks the header entry, fsyncs, and returns
`Ok(None)`, so worldgen fills the hole. Vanilla is lenient instead -- `NbtUtils`
turns an unknown block into air and drops an unknown property, and the rest of
the chunk survives.

The strictness is deliberate: two tests name it,
`unknown_referenced_block_state_is_corruption_instead_of_air_recovery` and its
biome twin. What no test covers is the *erasure* that follows. Together they
mean a registry drift -- a block gaining or losing a property between versions
-- destroys the world chunk by chunk, on disk, irreversibly. Reversing the
decode strictness is a design call, not an audit fix; separating "refuse to
load" from "delete what is on disk" is the smaller question worth asking first.

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

Two boundary findings are left open and are worth knowing about. A failed JNI
call leaves the Java exception pending: `jni`'s `check_exception!` returns
`Err(JavaException)` without calling `ExceptionClear`, and `forward.rs` swallows
that `Err` at some sixty sites, so a later JNI call runs with an exception still
set. It is made reachable by `EventBridge.handlers` being a plain `HashMap` of
plain `ArrayList`s, mutated from async plugin tasks and iterated from the tick.
And `Native.requestChunk` stores a `ChunkRequestHandle` -- which pins a chunk
ticket -- under a fresh UUID that only `chunkRequestReady` can retire, while the
only poller for it is unreferenced code; a plugin calling it directly pins a
chunk for the life of the server.

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

## Two more the audit proved and did not close

**An entity cannot be teleported into a chunk the entity manager does not
track.** `WorldEntityManager::can_move_manager_owned_to_chunk` requires the
destination in `chunk_visibility`, so `/tp @e[type=pig,limit=1] 5000 64 5000`
into unloaded terrain moves nothing. Vanilla's
`PersistentEntitySectionManager.Callback.onMove` never refuses a move.

Only half of that was fixed. `/tp` used to report every target as teleported
whatever happened, so the operator was told the opposite of the truth; it now
counts and announces the ones that actually moved. The move itself was left
alone on purpose: presence of the key in `chunk_visibility` is how the manager
knows which chunks it owns entities in, and letting an entity into a chunk with
no entry risks a double-add when that chunk loads, or an entity never saved when
it unloads. Closing it properly means either a load ticket on the destination
(vanilla's `TicketType.POST_TELEPORT`) or a real unloaded-section state, and
neither is a change to make without understanding what the manager promises.

**A duplicate login refuses the newcomer where vanilla kicks the incumbent.**
`reserve_player_join` fails if the UUID is already in `player_admissions` or
`online_players`, and the player is told "You are already connected to this
server"; `login.rs` still carries the `//TODO: Check for duplicate player UUID
or name` where the check belongs. Vanilla calls
`PlayerList.disconnectAllPlayersWithProfile`, parks the login in
`WAITING_FOR_DUPE_DISCONNECT`, and lets the new session in once the old one is
gone.

The severe half of this is already gone: connections are ticked per connection
now rather than per world-resident player, so a client that dies without a FIN
is timed out within thirty seconds whatever world it belongs to, and no ghost
holds a UUID forever. What is left is a window -- quit and reconnect before the
asynchronous disconnect save finishes, and the relog is refused. Fixing it means
making the login path wait on another session's teardown, which is a new
asynchronous state on the most concurrency-sensitive path in the server, and it
deserves its own change rather than a tail-end addition to an audit pass.

## The one that needs an architecture change, not a fix

Chunk serialization does not run on the world's tick thread, and one window in
the tick makes that visible. `ChunkMap::tick_game` calls
`collect_scheduled_block_ticks` and `execute_scheduled_block_ticks` as two
statements; the collect half already removes the due ticks from their
`ChunkTickContainer`, so between the two they exist only in a local `Vec`.
`World::request_save` meanwhile spawns `save_all_chunks` on the chunk runtime,
and `ChunkTickContainer::snapshot` reads only the containers. A save that lands
in that window writes a chunk with neither the pending ticks nor the state
changes they were about to make.

Vanilla cannot reach this state: `ChunkMap.save` calls
`SerializableChunkData.copyOf` synchronously on the server thread, and only the
disk write is asynchronous, so serialization is atomic with respect to
`ServerLevel.tick`.

The damage is bounded, which is why this is documented rather than fixed. The
executing tick marks the chunk dirty again immediately afterwards, so the next
save corrects the file; the bad write only survives if the process dies before
that -- a crash, a `kill -9`, a power cut. Closing it properly means moving
chunk serialization onto the tick thread or gating it behind a tick barrier,
which is an architecture change and not an audit fix. A smaller sibling sits in
the same place and could be fixed on its own: `FullChunkRef::scheduled_tick_snapshot`
reads `world.game_time()` before it takes the container lock, so a tick landing
between the two shifts every persisted `delay` by one.

## Known limits of this method

- **`panic = "abort"` in release** (`Cargo.toml`) means every `expect()` on a
  live path is a server kill with no unwinding, so `shutdown_worlds()` never
  runs and dirty chunks are lost. Treat any reachable `expect` as a data-loss
  bug, not a style problem.

  `Cargo.toml` sets `unwrap_used = "warn"` and CI runs clippy with
  `-D warnings`, so `unwrap` is effectively denied. `expect_used` is not set at
  all, and the workspace holds roughly 1400 `.expect()` calls -- 423 of them in
  `foton-core` files that contain no test module, so those are production sites.
  Most are surely provable invariants, but the class is unlit and unaudited, and
  the audit that measured this also found two live ones: `chunk_sender` aborted
  the server on a chunk packet too large to encode, on a rayon worker, while the
  identical error two files away was already a warning and a skipped packet.

  This is the clearest thing the tooling cannot see. Every one of `dev/ci.sh`'s
  ten stages passed green over that `expect` for the whole audit. Turning
  `expect_used` on would fail CI on all 423 at once; the project already uses
  `#[expect(..., reason = "...")]` elsewhere, which is the lever for adopting it
  a crate at a time.
- **A stack overflow is not a panic.** It is a SIGSEGV, so no `catch_unwind`
  and no abort handler sees it. Recursive parsers need an explicit depth
  ceiling; vanilla uses 512 (`NbtAccounter`).
- Coverage percentages say nothing about correctness. See `PARITY.md`.
