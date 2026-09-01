# Releasing Foton

How a Foton release is made, how someone installs it, and how they update.
Accepted 2026-08-31.

## What this exists to fix

Foton has never shipped a binary. The Docker workflow has failed every time it
has ever run — the release profile's `lto = true` with `codegen-units = 1` runs
the two-core ARM runner out of memory — and the release workflow has never built
anything, because it short-circuits unless the version changes. GitHub Actions
is currently refusing to start jobs at all for a billing reason.

Meanwhile the website's front page tells people to run
`docker run ghcr.io/zeffut/foton:nightly`, which does not exist.

So the goal is not a nicer pipeline. It is that someone can get a running
server, that the instructions are true, and that neither depends on CI being
healthy.

## Three decisions

### 1. One script makes a release, whoever runs it

`dev/release.sh` is the procedure. It runs on a laptop today and CI calls the
same script when Actions works again.

The alternative — a workflow that knows how to build and a script that also
knows how to build — is two procedures that drift, and the one nobody runs is
the one that breaks. There is one, and it is the one a human can run and read.

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
bash dev/release.sh              # build, check, publish
bash dev/release.sh --dry-run    # everything except the tag and the upload
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
6. **Writes `SHA256SUMS`** over every artifact.
7. **Prints platform coverage**: which of the five release assets (below) it
   is about to publish and which are missing, so a partial release is never
   mistaken for a complete one.
8. **Creates the tag and the GitHub release** and attaches the binaries and the
   checksum file.

A full release has exactly five assets:

| Asset | Platform | Built by |
|-------|----------|----------|
| `foton-linux-x86_64-musl` | Linux, Intel/AMD | a laptop (Docker) or CI |
| `foton-linux-aarch64-musl` | Linux, ARM | CI only (`ubuntu-24.04-arm` runner) |
| `foton-macos-aarch64` | macOS, Apple Silicon | a laptop (native) or CI |
| `foton-macos-x86_64` | macOS, Intel | CI only (cross-compiled from `macos-latest`) |
| `foton-windows-x86_64.exe` | Windows, Intel/AMD | CI only |

A laptop can produce two of the five: `foton-macos-aarch64` natively, and
`foton-linux-x86_64-musl` through the container -- and only if it happens to be
an Apple Silicon Mac, since the host binary is built for whatever the laptop
is. The other three need GitHub Actions. `dev/release.sh` prints which of the
five it is about to publish and which are missing before it publishes anything,
so a laptop release is never mistaken for a complete one. Run the "Build
Release" workflow (`workflow_dispatch` or a push to `master`) for all five.

## Installing

```
curl -fsSL https://foton.zeffut.fr/install.sh | sh
```

The script:

1. Detects the operating system and CPU, and picks the matching asset. An
   unsupported pair stops with the list of what exists, not a failed download.
   The script is POSIX `sh`, so on Windows it needs Git Bash, MSYS2, Cygwin or
   WSL -- there is no native `sh` to run it under otherwise.
2. Fetches the release metadata from the GitHub API — the repository is public,
   so no token is involved.
3. Downloads the binary **and `SHA256SUMS`**, and verifies the binary against it
   before making it executable. A mismatch deletes the download and stops.
4. Asks six questions on `/dev/tty`: where to install, the server name, the
   port, the maximum number of players, whether to use Mojang authentication,
   and the difficulty.
5. Runs the binary once so it writes its own configuration, then applies the
   answers to `config/config.toml` and `config/worlds.toml`.
6. Offers to start the server.

It never needs root, never writes outside the directory it is given, and
refuses to overwrite an existing installation without being told to.

## Updating

```
curl -fsSL https://foton.zeffut.fr/install.sh | sh -s -- --update
```

Run inside an existing installation, `--update` replaces the binary and leaves
`config/` and `saves/` untouched. It checks the installed version against the
latest release first and does nothing when they match.

The old binary is kept as `foton.previous` until the new one has started once,
so a bad release is one `mv` away from being undone.

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

`docker-build.yml` still fails on the ARM runner and the front page still
advertises the image it never produced. Fixing it means either dropping the
ARM64 target or relaxing `lto` for that build, and both change what ships to
users rather than only how it is built. Until that is decided, the front page's
command should point at what exists.
