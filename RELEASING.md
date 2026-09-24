# Releasing Foton

How a Foton release is made, how someone installs it, and how they update.
Accepted 2026-08-31. The problem statement below records the state at that
date; the procedures after it describe the current release path.

## What this exists to fix

At acceptance time Foton had never shipped a binary. The Docker workflow had
failed on its two-core ARM runner, and the release workflow had never built
anything because it short-circuited unless the version changed.

Meanwhile the website's front page tells people to run
`docker run ghcr.io/zeffut/foton:nightly`, which does not exist.

So the goal is not a nicer pipeline. It is that someone can get a running
server, that the instructions are true, and that neither depends on CI being
healthy.

## Three decisions

### 1. One script verifies locally and dispatches the complete release

`dev/release.sh` is the manual release entry point. It verifies and packages
everything the current machine can build, then dispatches the GitHub Actions
platform matrix from the exact, already-pushed `master` commit. The workflow
runs the same full `dev/ci.sh` gate before building or publishing any artifact.

The shared verification gate prevents a release workflow and the documented
manual path from disagreeing about whether a commit is publishable.

### 2. The installer does not write configuration. The server does

`install.sh` has the freshly installed binary write
`config/config.toml`, `config/worlds.toml` and `config/groups.toml`, and only
then edits the handful of values the person answered.

Writing those files itself would mean transcribing the schemas' defaults into a
shell script, where they would drift from what the server validates against the
first time a default changes. That is the practice `AGENTS.md` forbids for
extracted data, and it applies here too.

**This needs two flags the binary does not have.** `foton` parses no arguments
at all today: it starts a server and runs until killed. So it gains exactly
two, and no more:

- `--generate-config` writes the configuration files and exits, which is what
  the installer calls instead of starting a server it would then have to find
  and kill.
- `--version` prints the version and exits, which is what `--update` compares
  against the latest release. Without it the installer would have to keep its
  own record of what it installed, and a record kept beside the thing it
  describes is a record that goes wrong.

Anything else — a config path override, a port override, a subcommand tree —
is a server feature, not an installer requirement, and stays out.

### 3. Prompts come from `/dev/tty`, never from standard input

The installer is run as `curl -fsSL https://foton.zeffut.fr/install.sh | sh`, so
standard input is the script's own source. A `read` there consumes the script
and answers questions with its own text.

Every prompt reads `/dev/tty` directly. When there is no terminal — a CI job, a
Dockerfile — the installer takes every default, says so, and continues rather
than hanging on a prompt nobody can see.

## Making a release

```bash
bash dev/release.sh              # build/check locally, dispatch CI publication
bash dev/release.sh --dry-run    # local verification only; no CI dispatch
```

What it does, in order, stopping at the first failure:

1. **Refuses a dirty tree or a branch other than `master`.** A release must be
   reproducible from a commit that exists.
2. **Runs `dev/ci.sh`.** Formatting, spelling, the generated-docs check, clippy
   with `-D warnings`, the tests, and the test-count guard.
3. **Reads the version from `Cargo.toml`** and refuses to continue if a tag for
   it already exists. Version bumps are a human decision, made before running
   this.
4. **Builds the host binary** with `cargo build --release --locked --features
   stand-alone`.
5. **Builds the Linux musl binary in a container**, so the result is static and
   runs on any distribution without a runtime. This is why a laptop can produce
   a Linux artifact at all.
6. **Packages the plugin API and its pinned runtime libraries** in `.tar.gz`
   and `.zip` forms, with the complete third-party license/notices and an
   exhaustive internal checksum manifest. The runtime is exactly the API jar
   plus its twenty-four pinned dependency jars, including Guava's real
   transitive runtime closure and the five Netty modules used by the direct
   Via transport bridge.
7. **Writes `SHA256SUMS`** over every artifact.
8. **Prints platform coverage**: which of the seven release assets (below) it
   is about to publish and which are missing, so a partial release is never
   mistaken for a complete one. CI publishes only its complete matrix;
   `--dry-run` still permits inspecting a partial local set.
9. **Dispatches the Build Release workflow** for the exact verified
   `origin/master` commit. CI builds all five platforms, repeats the checks,
   creates the immutable tag and attaches the complete asset set.

A full release has five platform binaries and two plugin-runtime archives:

| Asset | Platform | Built by |
|-------|----------|----------|
| `foton-linux-x86_64-musl` | Linux, Intel/AMD | a laptop (Docker) or CI |
| `foton-linux-aarch64-musl` | Linux, ARM | CI only (`ubuntu-24.04-arm` runner) |
| `foton-macos-aarch64` | macOS, Apple Silicon | a laptop (native) or CI |
| `foton-macos-x86_64` | macOS, Intel | CI only (cross-compiled from `macos-latest`) |
| `foton-windows-x86_64.exe` | Windows, Intel/AMD | CI only |
| `foton-plugin-runtime.tar.gz` | Plugin API/runtime, POSIX installer | laptop or CI |
| `foton-plugin-runtime.zip` | Plugin API/runtime, PowerShell installer | laptop or CI |

Each plugin-runtime archive contains the API JAR, exactly 24 pinned dependency
JARs (including the five Netty 4.2.15.Final modules used by the direct Via
transport bridge), six license/notice texts and an internal SHA-256 manifest.
ViaVersion and ViaBackwards are opt-in operator plugins and are not bundled.
`dev/via-test.sh` is the release gate that verifies a real Minecraft 1.21.11
(protocol 774) client through the unmodified official 5.11.0 JARs to Foton 26.2
(protocol 776), followed by a native 26.2 join while both plugins remain active.

A laptop can produce two of the five platform binaries: its native binary and
`foton-linux-x86_64-musl` through the container. The other platforms need
GitHub Actions. `dev/release.sh` prints the locally available subset, then asks
the "Build Release" workflow to build and publish the complete set. A dry run
stops after local inspection and never contacts the publishing API.
Manual workflow dispatches are accepted only from `master`; a version that is
already tagged is immutable. Every external action in the publishing workflow
is pinned to a full commit SHA.

## Installing

```
curl -fsSL https://foton.zeffut.fr/install.sh | sh
```

Covers macOS and Linux natively, and Windows too but only inside a POSIX
shell -- Git Bash, MSYS2, Cygwin or WSL -- because it is a `sh` script. For
Windows PowerShell, which is what most Windows users actually have, there is
a second installer:

```powershell
irm https://foton.zeffut.fr/install.ps1 | iex
```

The two ask the same five questions with the same defaults and generate the
same three config files; `install.ps1` exists only because requiring a POSIX shell
to install a Minecraft server would rule out most Windows users. See
`site/static/install.ps1`'s header comment for how to pass `-Update` through
`irm | iex`, since piping to `iex` leaves no normal place for arguments.

The `sh` script:

1. Detects the operating system and CPU, and picks the matching asset. An
   unsupported pair stops with the list of what exists, not a failed download.
   The script is POSIX `sh`, so on Windows it needs Git Bash, MSYS2, Cygwin or
   WSL -- there is no native `sh` to run it under otherwise.
2. Fetches the release metadata from the GitHub API — the repository is public,
   so no token is involved.
3. Downloads the binary, the matching plugin-runtime archive and
   **`SHA256SUMS`**. It verifies both downloads and the runtime's internal
   manifest before replacing anything.
4. Stages the binary and runtime in the destination, atomically replaces the
   old pair, and checks that the new binary reports the requested version. Any
   failure restores the previous pair and reports explicitly if restoration
   itself could not complete.
5. On a first install, runs the binary once so it writes
   `config/config.toml`, `config/worlds.toml` and `config/groups.toml`.
6. Then asks five questions on `/dev/tty`:
   the server name, the port, the maximum number of players, whether to use
   Mojang authentication, and the difficulty.
7. Applies the answers to `config/config.toml` and `config/worlds.toml`, then
   offers to start the server.

It never needs root, never leaves installed files outside the directory it is
given, and refuses to overwrite an existing installation without being told to.
Each destination has an exclusive installer lock. The replacement journal is
stored beside the server, so interruption rolls back immediately and a later
invocation repairs any transaction left by an abrupt process or machine stop.

## Updating

```
curl -fsSL https://foton.zeffut.fr/install.sh | sh -s -- --update
```

On Windows PowerShell, the equivalent is `-Update`:

```powershell
iex "& { $(irm https://foton.zeffut.fr/install.ps1) } -Update"
```

Run inside an existing installation, `--update` (`-Update` on Windows) replaces
the binary and plugin runtime together and leaves `config/`, `plugins/` and
`saves/` untouched. It does nothing only when the installed version and every
runtime file match the latest release; otherwise it repairs the pair.

The old binary and runtime stay under transaction-specific `.previous-*` names
until the new binary passes its version check. A failed validation restores
both. If an operating-system error prevents restoration, the installer keeps
the backup paths and names them in the error instead of claiming success.

## What is deliberately not here

- **No package manager.** Not Homebrew, not apt, not the AUR. Each is a
  separate release surface with its own review cycle, and there is nothing to
  put in them yet.
- **No signature on the checksums.** HTTPS and a published SHA-256 are the
  honest level of assurance for a project at this stage; a GPG key nobody
  verifies is theater. If Foton ever ships to people who do not know its author,
  this is the first thing to revisit.
- **No auto-update.** A server that replaces its own binary while players are
  connected is a worse problem than an out-of-date one.

## The Docker image

`docker-build.yml` builds AMD64 and ARM64 images on native runners, publishes
each by digest, then creates one multi-platform manifest. A successful
`Build Release` workflow run starts the release image build from that exact
commit and tags the manifest with the release tag. A push to `master` updates
`nightly` only after the `Test` workflow has finished successfully for that
exact commit. A manual run applies the corresponding successful-workflow proof
and is accepted only from `master`. The builder image and Rust nightly are
pinned together by an immutable image digest. Immediately before creating the
manifest, the workflow verifies that a nightly SHA is still `master`'s HEAD; a
newer push cancels the older nightly run.

Cargo build metadata uses `+`, which OCI tags do not accept. Release image tags
therefore use the single reserved mapping `vX.Y.Z+mcA.B` →
`vX.Y.Z-mcA.B`. The gate rejects other release-tag shapes and any Git tag that
would collide with the mapped OCI name. A missed Docker follow-up can be
recovered with the Docker workflow's manual `release_tag` and `release_sha`
inputs: it requires a successful `Build Release` run for that SHA, validates
the existing immutable Git tag and published GitHub release, then creates only
the missing image without recreating the release. If the OCI tag already
exists, recovery accepts it only when it is an OCI index for exactly Linux
AMD64 and ARM64 and carries the approved source revision.
