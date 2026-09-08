# Task 1 report — Portable developer tooling and diagnostics

## Scope

Implemented only Task 1 on `codex/audit-hardening`:

- added `dev/wait-tcp.py` with real TCP connection checks, IPv4/IPv6 address resolution, timeout diagnostics, and distinct dead-process diagnostics;
- replaced GNU `find -printf` in the plugin API classpath construction with a flat BSD-compatible shell glob loop;
- replaced direct TCP readiness probes using `ss -ltn` with the helper across developer test scripts, including `join-test.sh` and `smoke-test.sh`;
- preserved the Bedrock UDP probe (`ss -lun`) unchanged;
- extended `doctor.sh` with required checks for Python, `javac`, `jar`, `curl`, checksum tooling, and the TCP helper, plus an optional legacy `ss` check;
- resolved the pre-commit hook through `git rev-parse --git-path hooks/pre-commit`;
- added the focused `dev/test-dev-tools.sh` regression suite.

## TDD evidence

### RED

Before production changes, `bash dev/test-dev-tools.sh` failed with:

```text
AssertionError: plugin API classpath still uses GNU find -printf
```

This demonstrated that the test caught the current non-portable implementation.

### GREEN

After implementation:

```text
$ bash dev/test-dev-tools.sh
developer tooling tests passed
```

The suite exercises the BSD-compatible classpath source, real IPv4 TCP readiness,
IPv6 readiness when the host provides it, timeout diagnostics, dead-process
detection, and worktree hook-path resolution.

## Verification

- `bash dev/test-dev-tools.sh` — passed.
- `python3 -m py_compile dev/wait-tcp.py` — passed.
- `bash -n` on all changed developer shell scripts — passed.
- `git diff --check` — passed.
- `bash dev/build-plugin-api.sh` — passed; compiled 822 sources and wrote `plugin-api/build/foton-plugin-api.jar` with 978 classes.
- `bash dev/doctor.sh` — ran; expected environment diagnostics remain for missing `ast-grep`, `prek`, `minecraft-src`, `FotonExtractor`, and the uninstalled hook. The absent `ss` check is reported as optional warning. Geyser pin verification passed online.

## Concerns

The plugin API build emits its existing Java warnings (deprecated APIs, missing
optional Guava annotation classes, serial warnings, and related lint warnings),
but exits successfully. Full repository CI was not run because Task 1 calls for
focused developer-tool verification and the current checkout is missing the
vanilla source/extractor prerequisites reported by `doctor.sh`.
