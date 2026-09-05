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

## Known limits of this method

- **`panic = "abort"` in release** (`Cargo.toml`) means every `expect()` on a
  live path is a server kill with no unwinding, so `shutdown_worlds()` never
  runs and dirty chunks are lost. Treat any reachable `expect` as a data-loss
  bug, not a style problem. Note that clippy is configured with
  `clippy::unwrap_used` but not `expect_used`, so `expect` is not linted.
- **A stack overflow is not a panic.** It is a SIGSEGV, so no `catch_unwind`
  and no abort handler sees it. Recursive parsers need an explicit depth
  ceiling; vanilla uses 512 (`NbtAccounter`).
- Coverage percentages say nothing about correctness. See `PARITY.md`.
