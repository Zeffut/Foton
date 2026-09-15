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
