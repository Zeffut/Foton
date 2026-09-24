# Task 1 report — Integration of `codex/audit-hardening`

## Result

Merged `codex/audit-hardening` at `0664fcc00` into
`codex/reconcile-audit-master`, which started at the current `master` commit
`635b84fd8`. The integration keeps the complete audit hardening series and the
two master-only Vercel fixes (`f06e75531`, `635b84fd8`). No push was performed
and `master` was not modified.

The integrated audit work includes the network and RCON admission limits,
authentication redirect and endpoint validation, durable/cancellation-safe
storage, revision-aware map persistence, transactional piston behavior, world
lifecycle and shutdown synchronization, strict lint cleanup, and portable
developer tooling.

## Conflict resolution

The merge produced three content conflicts:

- `foton-core/src/player/chunk_sender.rs`: both sides contained the same pacing
  assertion with different rustfmt layouts. Kept the assertion and let the
  current toolchain format it.
- `foton-protocol/src/packet_reader.rs`: both sides contained the same trailing
  compressed-frame regression test with different rustfmt layouts. Kept the
  test and let the current toolchain format it.
- `foton-core/src/player/player_data_storage/mod.rs`: kept master's stricter and
  more complete bounded-reader implementation. It limits compressed files to
  8 MiB rather than 16 MiB, also protects `known_players`, detects growth while
  reading, and checks arithmetic conversions/overflow. Retained the audit
  branch's compressed-size and decompression-bomb regression tests, changed
  them to use the effective production constants, and kept the 64 MiB bounded
  decompressor.

No generated Rust file was edited. `CONFIGURATION.md` arrived from the audit
branch together with its source schema changes, and the generated-reference CI
check passes.

## Integration repairs

### Developer tooling regression test

`bash dev/test-dev-tools.sh` initially failed its worktree hook scenario. During
an uncommitted merge, the test created a detached worktree from `HEAD`, which
still referred to pre-merge master, so it executed the old `doctor.sh` rather
than the working-tree version under test. The test now copies the current
`doctor.sh` and `wait-tcp.py` into the temporary linked worktree before running
the behavioral hook check. This makes the test valid both before and after a
commit.

### Current-nightly `large_futures`

The first full CI run found 41 `clippy::large_futures` errors rooted in
`FilePlayerDataStorage::read_bounded_file`. Its 16 KiB stack array was retained
inside the async state machine across `.await`, inflating every transitive
future. Moving that same fixed-size read buffer to a `Vec<u8>` keeps all bounds
and behavior unchanged while reducing the future size. No lint was disabled or
suppressed.

## Verification

Focused verification:

- `cargo test -p foton-core player_data_storage::tests::` — 20 passed, 0 failed.
- `cargo test -p foton-core chunk_sender::tests::` — 10 passed, 0 failed.
- `cargo test -p foton-protocol compression_security_tests::` — 4 passed, 0 failed.
- `bash dev/test-dev-tools.sh` — `developer tooling tests passed`.
- `bash dev/test-join-seed.sh` — `join seed insertion test passed`.
- `cargo clippy -r -p foton-core --all-targets --all-features -- -D warnings` — passed.

The first `bash dev/ci.sh` run passed formatting, spelling, generated config,
site generation, all workspace tests, plugin API compilation, native
registration, test-count validation, and developer tooling; only the 41
`large_futures` lints failed. After the root-cause fix, a fresh complete run
reported:

```text
cargo fmt --all --check                                      PASS
typos                                                         PASS
config reference is current                                  PASS
site builds                                                   PASS
cargo clippy -r --workspace --all-targets --all-features     PASS
cargo test --workspace                                       PASS
plugin api builds                                             PASS
every native is registered                                    PASS
test counts are current                                       PASS
dev tooling tests                                             PASS
########## ALL GREEN ##########
```

Additional review checks:

- `git diff --cached --check` and `git diff --check` — passed.
- Repository-wide conflict-marker scan excluding `target/` — clean.
- No changed path under any `src/generated/` directory.
- `git diff master -- api vercel.json dev/test_vercel_config.py` — empty; the
  Vercel state from master is preserved exactly.
- All four pre-existing stashes remain present and untouched.

## Self-review and concerns

The conflict resolutions preserve both parents' intended behavior, with the
stricter master storage limits plus the audit regression coverage. The two
integration-only repairs address demonstrated failures at their source and do
not add `allow` attributes, production `unwrap`/`expect`, new `unsafe`, stubs,
or generated-file edits.

The focused debug-profile `foton-core` test link emits the existing macOS
warning that `__eh_frame` exceeds 16 MiB; tests still pass and the full CI is
green. No functional concern remains.

## Fix round 1/5 — reviewer findings

### Changes

- `foton-core/src/chunk_saver/region_manager.rs`
  - Header mutations made by a save now set `header_dirty` before the first
    header-write await. The flag is cleared and a save-only handle is removed
    only after `write_header` succeeds, leaving failures or cancellation
    recoverable through `flush_all`/shutdown.
  - Quarantine publication now fsyncs the quarantine file, then its directory,
    and also the directory's parent when `corrupt/` was newly created. Any sync
    failure is returned before the original region slot is cleared.
  - Region header validation now validates, sorts, and compares at most 1,024
    `(start, end, index)` intervals instead of allocating a `Vec<bool>` sized
    from the region file's logical sector count.
  - The zstd bomb test writes 1 MiB zero blocks progressively until just over
    the 64 MiB ceiling instead of allocating 1 GiB at once.
  - Added covering tests for pending-header retry, maximum sparse-region
    interval validation, and failed parent-directory sync preserving the slot.
- `foton-core/src/command/rcon.rs`
  - `RconOutput` now writes through a UTF-8-safe bounded formatter and never
    retains more than `MAX_RCON_OUTPUT_BYTES` cumulatively.
  - Added a regression test whose multiple fragments exceed the total limit.
- `foton/src/rcon/packet.rs`
  - Response splitting now consumes the same exported RCON output limit,
    removing the duplicate constant and preventing drift.
- `dev/test-counts.json`
  - Regenerated with `python3 dev/count-tests.py`: 5,538 tests across 19
    targets.

### TDD evidence

- `cargo test -p foton-core command::rcon::tests::output_is_bounded_across_multiple_fragments -- --exact`
  - RED after correcting the owned-string fixture: expected 1,048,576 bytes,
    observed 1,048,590 bytes.
  - GREEN after bounded accumulation: 1 passed, 0 failed.
- The initial region regression compile failed because
  `set_pending_header_entry` and the directory-sync failure instrumentation did
  not exist. After implementing the invariants, each new behavior test passed.

### Commands and results

- `cargo test -p foton-core chunk_saver::region_manager::tests::a_pending_header_commit_survives_cancellation_before_write -- --exact`
  — 1 passed, 0 failed.
- `cargo test -p foton-core chunk_saver::region_manager::tests::maximum_sparse_region_entries_are_checked_by_interval -- --exact`
  — 1 passed, 0 failed.
- `cargo test -p foton-core chunk_saver::region_manager::tests::failed_new_quarantine_parent_sync_keeps_the_original_slot -- --exact`
  — 1 passed, 0 failed.
- `cargo test -p foton-core chunk_saver::region_manager::tests::a_decompression_bomb_is_refused_rather_than_expanded -- --exact`
  — 1 passed, 0 failed.
- `cargo test -p foton-core chunk_saver::region_manager::tests:: --lib`
  — 16 passed, 0 failed.
- `cargo test -p foton-core command::rcon::tests:: --lib`
  — 3 passed, 0 failed.
- `cargo test -p foton rcon::packet::tests::`
  — 9 passed, 0 failed.
- `python3 dev/count-tests.py`
  — wrote `dev/test-counts.json`: 5,538 tests across 19 targets.
- `cargo fmt --all --check` — passed.
- `cargo clippy -r -p foton-core -p foton --all-targets --all-features -- -D warnings`
  — passed.
- `bash dev/ci.sh` — all stages passed: formatting, typos, config reference,
  site build, workspace clippy, workspace tests, plugin API, native
  registration, test counts, and developer tooling; final output
  `########## ALL GREEN ##########`.

The only observed warning remains the pre-existing macOS debug-linker
`__eh_frame` size warning during focused `foton-core` tests; it is non-blocking
and the complete CI run is green.

## Final fix wave

### Changes

- `foton-core/src/chunk_saver/region_manager.rs`
  - `close_all` now retains every dirty region handle whose header commit
    fails, while continuing to close successful regions, so a later call can
    retry the exact pending header.
  - Chunk writes validate the sector count conversion and
    `sector_offset + sectors_used` with checked arithmetic before seeking or
    extending a sparse file.
  - Corrupt-slot clearing marks the empty header entry dirty before its first
    header-write await. I/O failure or task cancellation therefore leaves a
    retryable pending clear instead of a clean, reverted in-memory header.
  - New region files are fsynced before their directory entry is committed.
    Missing directories are created level by level with parent-directory
    syncs. Incompatible-version backup links are synced before removing the
    source, and the source removal is synced before replacement creation.
    Directory sync remains a no-op on platforms that do not support opening
    directories, while preserving identical error propagation elsewhere.
  - Added targeted failure, cancellation, sparse-boundary, and namespace
    durability tests.
- `foton-core/src/server/mod.rs` and `foton-core/src/server/tests.rs`
  - World construction now enters a dedicated tracker under the same locked
    admission gate that shutdown closes. Shutdown closes creation admission,
    drains pending attachments, and waits for both admitted constructions and
    detached cleanups before persistence can continue.
  - Added a barrier-driven test proving shutdown waits for an already admitted
    construction and rejects a late one.
- `foton-core/src/command/rcon.rs`
  - RCON output saturation is sticky after the first truncated fragment, so a
    multibyte character that does not fit cannot leave a gap filled by later
    fragments. The retained response is always one continuous input prefix.
- `foton-login/src/authentication.rs`
  - Renamed `RedirectDowngrade` to `RedirectRejected`; redirect failures now
    report the endpoint-policy rejection accurately for both downgrades and
    disallowed remote HTTP targets.
- `dev/test-counts.json`
  - Regenerated with `python3 dev/count-tests.py`: 5,548 tests across 19
    targets.

### TDD evidence

- The UTF-8 RCON regression first failed with 1,048,576 retained bytes instead
  of the expected 1,048,575-byte continuous prefix; it passed after saturation
  became sticky.
- The auth redirect regression first failed to compile because
  `AuthError::RedirectRejected` did not exist; it then passed against a local
  302 response targeting disallowed remote HTTP.
- The sparse write regression first panicked on unchecked `u32` addition; it
  passed after pre-I/O checked arithmetic and confirms the file remains zero
  bytes long.
- The `close_all` regression first failed to compile without header-write
  injection; after the retry invariant was implemented it demonstrated one
  failed close followed by a successful commit and reopen.
- The corrupt-slot failure regression first observed the old slot restored in
  memory. The cancellation regression paused at the header commit await and
  cancelled the load. Both now preserve an empty dirty entry that a later
  `flush_all` commits.
- The initial-region namespace regression first observed creation succeeding
  despite an injected directory-sync failure. Backup and directory-creation
  tests likewise covered the required ordering before the durability changes.
- The shutdown regression first failed to compile because no tracked creation
  admission path existed. It now proves shutdown remains pending until the
  admitted task exits and that post-close admission is rejected.

### Commands and results

- `cargo test -p foton-core chunk_saver::region_manager::tests:: --lib` — 23
  passed, 0 failed.
- `cargo test -p foton-core command::rcon::tests:: --lib` — 4 passed, 0
  failed.
- `cargo test -p foton-core server::tests::world_creation --lib` plus the exact
  in-flight shutdown test — 3 passed, 0 failed.
- `cargo test -p foton-login authentication::tests:: --lib` — 23 passed, 0
  failed.
- `cargo fmt --all --check` — passed.
- `cargo clippy -r -p foton-core -p foton-login --all-targets --all-features -- -D warnings`
  exposed only mechanical strict-lint issues, which were fixed without lint
  suppression.
- The first `bash dev/ci.sh` run passed workspace tests and every non-clippy
  downstream stage, then reported one unused test import and the expected
  stale test count (5,538 committed versus 5,548 measured).
- After removing the import and regenerating the count, a fresh complete
  `bash dev/ci.sh` run passed formatting, typos, generated configuration,
  site build, workspace clippy, workspace tests, plugin API, native
  registration, test counts, and developer tooling, ending with
  `########## ALL GREEN ##########`.
