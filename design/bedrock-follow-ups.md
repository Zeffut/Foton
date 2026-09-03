# Bedrock support — what was left undone

Merged into `master` on 2026-09-02 as commit `15874dd9a`, CI green on the
merged result. This is the list of everything the reviews found and chose not
to fix before shipping, kept because a follow-up nobody wrote down is a
follow-up nobody does.

It also records what the live verification actually covers, which matters more
than it sounds: several claims here are proven by one manual run rather than by
the automated suite, and the difference is stated rather than blurred.

Ordering below is the final review's own priority. Numbers 1, 2, 3, 8 and 15
were closed before the merge; the rest stand.

## Work done before merging

Four follow-ups closed while waiting, all CI-green:

- `70702e57f` — the Floodgate key file is created with `0o600` from the start
  rather than written and then chmodded, closing a window where the feature's
  entire trust boundary was world-readable. **Known limit:** a key already on
  disk keeps its old mode; only creation is fixed. Academic today (the feature
  has never shipped), worth revisiting if it ever does.
- `c19c4d475` — the `java -version` probe is now time-bounded. It ran unbounded
  after the TCP listener binds but before the accept loop, so a wedged JVM
  would have frozen startup with the port open and nothing answering — the same
  shape as the download hang already fixed.
- `91649e448` — **`dev/bedrock-test.sh` now runs the production-shaped
  configuration** (`online_mode = true`, `encryption = true`). Every prior run,
  Stage 0 included, used offline mode, which cannot catch a Floodgate
  regression by construction. Observed passing: Geyser on the shared port,
  `.StageZero` joining twice with UUID
  `00000000-0000-0000-0009-01f571cf05ac`. **Precisely what that proves:** the
  Floodgate path exercised by a raw Java client sending exactly what Geyser
  would send — not a Bedrock client traversing Geyser, whose sub-test was
  *skipped* because `bedrock-protocol` is not installed in this WSL. A real
  Bedrock client reaching the world end to end remains proven once, manually,
  at Stage 0, in offline mode. Corroborating detail worth keeping: the
  `Bedrock: online_mode is false…` warning that fired on every earlier run is
  **absent** here — negative evidence that the configuration actually took
  effect rather than a test passing for the wrong reason.
  Follow-up this creates: the script *skips* that sub-test rather than
  announcing loudly that its most end-to-end assertion did not run. A skip
  that quiet is how a suite drifts into proving less than it claims.
  The offline run was removed rather
  than kept alongside — the Floodgate path never reads `online_mode` or
  `encryption`, so a second run would double the wall-clock for no coverage.
  The argument is written into the script.
- `a6918929e` — `design/bedrock-implementation.md` no longer claims "nothing
  here is built yet", and records what shipped differently from the proposal
  (`enable` not `enabled`, port `0` not `19132`, no `geyser_overrides`).
  Annotated, not revised: its value is the record of what was decided and why.

Follow-ups 1, 2, 3, 8, 15 from the list below are therefore closed, as are the
two I had flagged as the ones I would fix first.

## State at the hold

- 37 commits on `feat/bedrock`, forked from `master`.
- `dev/ci.sh` **ALL GREEN**: fmt, typos, config docs, site build, plugin API
  build, `clippy -r --workspace --all-targets --all-features -D warnings`,
  `cargo test --workspace`, test counts, dev tooling tests.
- Final whole-branch review (opus): **merge with follow-ups**, after its one
  blocking finding was fixed in `68fc79169` and `7272b3c74`.
- Dedicated security review of the login path: **sound** — no path by which a
  forged or failed Floodgate handshake yields a profile.
- Worktree: `C:\Users\Zeffu\Desktop\Projets\Foton-bedrock`. Created manually
  with `git worktree add`, so it is not Superpowers' to remove.
- The main checkout `C:\Users\Zeffu\Desktop\Projets\Foton` was returned to
  `master` early on and left alone since, so the plugin work was never disturbed.

## What it delivers

A Bedrock Edition player joins a Foton server with no Java account. Foton
supervises GeyserMC as a child process and decodes Floodgate's encrypted
identity handshake natively. Java (TCP) and Bedrock (UDP) share one port by
default — `bedrock.port = 0`.

Proven live, not asserted: `run-bedrock/server.log` shows the shared port
(`server_port = 25610` and `Started Geyser on UDP port 25610`), Geyser reaching
Foton, and `.StageZero joined the game` twice with the same derived UUID
`00000000-0000-0000-0009-01f571cf05ac` — the persistence guarantee.

## Predicted conflicts when merging with the plugin branch

Worth reading before attempting the merge, because most of these are
mechanical and one is not.

**Near-certain, mechanical:**
- `dev/test-counts.json` — both branches add tests. Regenerate with
  `dev/count-tests.py` after merging rather than resolving by hand.
- `CONFIGURATION.md` — generated. Regenerate with `python3 dev/gen-config-docs.py`.
- `Cargo.lock` — regenerate.

**Likely, append-only, resolve by keeping both:**
- `Cargo.toml` workspace `members` and `[workspace.dependencies]` — this branch
  adds `foton-bedrock`.
- `foton/src/config/server.rs` — this branch adds a `bedrock` field to
  `ServerConfig` and a `[server.bedrock]` validation block. A plugins section
  would sit beside it, not on it.
- `package-content/config.toml` and `config.schema.json` — separate sections.
- `dev/all-tests.sh` — this branch appends `bedrock-test.sh`.
- `.gitignore` (`/run-*/`), `_typos.toml` (`Projets`).

**The one to look at properly:**
- `foton/src/lib.rs` — this branch adds a `bedrock` field and a
  `bedrock_supervisor` field to `FotonServer`, an 8th argument to
  `JavaTcpClient::new`, key loading at startup, and supervisor start/shutdown
  beside the Rcon listener. If the plugin work also touched startup or the
  accept loop, resolve by reading both intents rather than taking either side.

**Deliberately untouched, and this matters:**
- `foton-core/src/player/connection/mod.rs` — the typed-packet seam. The plugin
  design wants to reshape it to observe outbound packets; the Geyser route
  needed none of it. Choosing route A is what kept these two workstreams off
  each other. If the plugin branch reshapes it, this branch does not care.

## The 18 follow-ups the final review triaged as non-blocking

In its own priority order. Numbers 1 and 2 were folded into `7272b3c74`
already; the rest stand.

3. `design/bedrock-implementation.md` still says "Nothing here is built yet" and
   documents config that shipped differently (`enabled` vs `enable`, `19132` vs
   `0`, a `geyser_overrides` that was never built). The branch set the precedent
   for a reconciliation banner on `bedrock-clients.md`; the same is owed here.
4. `render_config` regenerates `config.yml` wholesale every start with no header
   saying so — an operator's edits vanish silently.
5. `bedrock.jar_path` is checksum-pinned, so it accepts only a byte-identical
   copy of the pinned build, which the docs imply otherwise.
6. Geyser needs network again after Foton's fetch (it downloads a Minecraft jar
   itself) — an air-gapped host will not work even with `jar_path` set.
7. `trusted_proxies` entries that are not parseable IPs are silently dropped —
   partly addressed in `7272b3c74`; verify.
8. `server_port == 0` — addressed in `7272b3c74`; verify.
9. Four tests in `floodgate.rs` use the crate's own `encrypt` as their oracle.
10. `download()` builds a fresh `reqwest::Client` per call.
11. `request_graceful_stop` is a no-op on non-Unix: Windows always burns the
    full 10-second grace, and `terminate` logs nothing on the normal path.
12. `Supervisor::start` ignores cancellation during its synchronous prefix, and
    `resolve_java` runs `java -version` with **no timeout at all** — the same
    shape as the download hang that was fixed.
13. `dev/doctor.sh` does not check for `curl`; its pin check verifies the build
    exists but not that it claims the right protocol, which the design doc
    claims it does.
14. `GeyserError::Io` says "failed to access {path}" even when the real failure
    was resolving the current directory.
15. **Untested production configuration**: every live run used
    `online_mode = false, encryption = false`. Traced as safe, but never run.
    Flipping two `sed` lines in `dev/bedrock-test.sh` would close it.
16. `dev/bedrock-test.sh` fails rather than skips with no network, re-downloads
    ~10 MB per run, and its `pkill` would kill an unrelated Foton's Geyser on
    the same host.
17. `GeyserOptions::username_prefix` is dead — nothing reads it.
18. Batched smaller items: `decrypt` never validates the version byte;
    `key::load_or_create` writes then chmods, leaving a umask-width window where
    the trust boundary is world-readable (`OpenOptions::mode(0o600)` closes it);
    the respawn give-up message says 5 attempts after 4; `relay_stream` uses
    `lines()` so one non-UTF-8 byte ends log relay for that process; nothing
    detects an orphaned Geyser from a hard-killed Foton.

Of these, **18's key-file window** and **12's untimed `java -version`** are the
two I would fix first: one is a security window on the feature's entire trust
boundary, the other is the same class of startup hang that already bit once.
