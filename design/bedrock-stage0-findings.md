# Stage 0 — what happened when Geyser first met Foton

Written 2026-09-01, by hand, against Geyser 2.11.2 build 1233
(`Geyser-Standalone.jar`, SHA-256 `f1a4c6a5cad7ee4820b03c27cd3805680e8c06bd66ce7244f96335d83b652e0e`,
matching the pin in the plan) and a locally built, **unmodified** Foton from this
worktree (`feat/bedrock`, no `foton-bedrock` crate exists yet — this is all
manual wiring).

**Bottom line: yes, Geyser reaches Foton, and the chain is real.** A Bedrock
client behind Geyser can complete a full Java handshake against Foton today,
with zero Bedrock-specific code in Foton. What it gets when it arrives depends
entirely on `online_mode`, and that dependency is the single most important
correction to `design/bedrock-implementation.md` (see "Contradictions" below):
the plan's implicit test case is the wrong one.

## Step 1 — offline-mode baseline

`dev/join-test.sh` passed, but not on the first try, and not for a Bedrock
reason. The controller amendment (R1) requires
`CARGO_TARGET_DIR=/root/foton-target` for every WSL build in this worktree;
`dev/join-test.sh` unconditionally looks for the binary at `$ROOT/target/debug/foton`
(the un-redirected default). With `CARGO_TARGET_DIR` set, the binary lands in
`/root/foton-target/debug/foton` and the script's own `nohup` line silently
fails to start anything, so the server never writes a config and the script
reports `SERVER NEVER WROTE A CONFIG`.

This is a tooling mismatch between the environment's build convention and the
script's hardcoded path — **not a Foton or Bedrock defect**. Worked around for
this session with `ln -s /root/foton-target /mnt/c/.../Foton-bedrock/target`
(gitignored, not committed). Worth fixing in `dev/join-test.sh` itself (make it
respect `$CARGO_TARGET_DIR` if set) so the next person doesn't hit the same
wall.

With the symlink in place, `dev/join-test.sh` passed cleanly: login →
configuration → play, chunks sent, `JOIN TEST PASSED`.

## Step 2 — pinned Geyser build

Downloaded and verified. SHA-256 matched the pin exactly:
`f1a4c6a5cad7ee4820b03c27cd3805680e8c06bd66ce7244f96335d83b652e0e`. Geyser
reports itself as `2.11.2-b1233 (git-master-fac30e9)` at startup, consistent
with the pin.

## Step 3 — Geyser config: the real key names

Running the jar once (no config present) makes Geyser write `config.yml` and
**also fully start** — it does not exit after writing defaults, it starts
listening on UDP 19132 with those defaults. `--help` alone does **not** write
a config; it just prints usage and exits.

**The brief's assumed key names are wrong.** `config.yml` has no `remote:`
section. The actual keys, confirmed from the generated file:

| What the brief assumed | What Geyser 2.11.2 actually generates |
|---|---|
| `remote.address` | `java.address` (default `127.0.0.1`, already correct) |
| `remote.port` | `java.port` (default `25565`, edited to `25566`) |
| `remote.auth-type` | `java.auth-type` (default `online`, edited to `floodgate`) |
| `bedrock.port` | `bedrock.port` — this one was right (default `19132`, already correct) |

Two more keys mattered that the brief didn't mention at all, both under
`advanced:`:

- **`advanced.floodgate-key-file`** (default `key.pem`) — the path to the
  shared AES key, resolved relative to Geyser's own working directory in every
  test run here (the two coincided in this setup, so relative-to-cwd vs.
  relative-to-config-file-location was not distinguished; Task 6 should use an
  **absolute** path to `<run_directory>/bedrock/key.pem` per R2 rather than
  rely on either).
- **`advanced.bedrock.validate-bedrock-login`** (default `true`) — Geyser's
  own Xbox Live authentication check for the *Bedrock* side of the connection,
  independent of `java.auth-type`. See Step 4: this had to be set to `false`
  to drive the chain with a synthetic (non-Xbox-authenticated) test client.
  Geyser's own config comment is explicit that disabling it outside a
  proxy-chain setup is a security hole and breaks "all Floodgate
  functionality" — true in the sense that a real deployment must leave it
  `true` and use a real Xbox Live account; this project's own automated
  testing (Task 2 onward, and any future CI-style Bedrock join test) will need
  to solve this the same way this session did, or accept it cannot fully
  automate an end-to-end Bedrock join test without a real Microsoft account.

**Addendum (Task 6 fix round 1, 2026-09-02):** two more keys, read the same
way — directly from `/root/geyser/config.yml`, the pinned build's own
generated file — that this section didn't originally record, because Task 1
was focused on the port/auth-type/key-file keys `geyser.rs` needed checked
against the brief's wrong guesses. A `geyser.rs` doc comment cited this
document for these two as if a *re-run* against a partial config had
confirmed them; no such re-run is recorded here. It happened (informally,
during Task 6, against the same pinned jar), but the citation belongs on the
observation, so it's recorded here instead of only in a source comment:

- **`bedrock.address`** (default `0.0.0.0`) — the sibling of `bedrock.port`
  under the same `bedrock:` section in the generated file. The table above
  only lists `bedrock.port` because that was the one key under `bedrock:`
  whose name the brief had guessed at; `bedrock.address` was never in
  question, but it is read from the same file, not invented.
- **`motd:`** — an entire section the port/auth-type table above doesn't
  cover, generated in full as:

  ```yaml
  # MOTD settings
  motd:
    # The MOTD that will be broadcasted to Minecraft: Bedrock Edition clients. This is irrelevant if "passthrough-motd" is set to true.
    # If either of these are empty, the respective string will default to "Geyser"
    primary-motd: Geyser
    secondary-motd: Another Geyser server.

    # Whether Geyser should relay the MOTD from the Java server to Bedrock players.
    passthrough-motd: true

    # Maximum amount of players that can connect.
    # This is only visual, and is only applied if passthrough-motd is disabled.
    max-players: 100
  ```

  `primary-motd` is the operator-facing MOTD string Geyser shows Bedrock
  clients. `passthrough-motd: true` — Geyser's own default — means Geyser
  ignores `primary-motd` and instead relays the *Java* server's own MOTD to
  Bedrock players. This is the exact mechanism `BedrockConfig.motd`'s doc
  comment ("empty reuses the server MOTD") depends on: `geyser.rs` must emit
  `passthrough-motd: true` (and can omit `primary-motd`) when the operator's
  MOTD is empty, and `passthrough-motd: false` with a quoted `primary-motd`
  otherwise.

**The re-run itself.** A `geyser.rs` doc comment claims the module's minimal,
partial `config.yml` (only `bedrock.address`/`port`, `java.address`/`port`/
`auth-type`/`forward-hostname`, `motd.primary-motd`/`passthrough-motd`,
`advanced.floodgate-key-file`, `advanced.bedrock.validate-bedrock-login` —
omitting everything else Geyser's own default file has) was confirmed to
work against the pinned build. What was actually done: the pinned jar was
copied to a scratch directory outside the repo
(`/root/geyser-test-partial/`, WSL, deleted afterward), alongside a `key.pem`
and a hand-written `config.yml` containing exactly those keys — including a
`primary-motd` of `Foton test motd with #tags & "quotes"`, escaped the same
way `geyser.rs`'s `yaml_quote` escapes it, `bedrock.port: 19133`, and
`java.port: 25599` (both intentionally off the defaults, to prove the
override — not the default — was what took effect). Running
`java -jar Geyser-Standalone.jar` there (Java 25, WSL) produced:

```
[12:00:56 INFO] Started Geyser on UDP port 19133
[12:00:56 INFO] Done (1.806s)! Run /geyser help for help!
```

— no "unknown key" or missing-field error, and the bound port matched the
override, not the `19132` default. Geyser then rewrote `config.yml` in
place, filling in every key the partial file omitted with its own defaults
(`config-version: 7`, a fresh `metrics-uuid`, the full `gameplay:` section,
etc.) while preserving every value the partial file did set:
`java.port: 25599`, `java.auth-type: floodgate`,
`java.forward-hostname: false`, `motd.passthrough-motd: false`,
`advanced.floodgate-key-file: /root/geyser-test-partial/key.pem`,
`advanced.bedrock.validate-bedrock-login: true`, and —
the specific claim this addendum exists to back up —
`motd.primary-motd: 'Foton test motd with #tags & "quotes"'`, byte-correct,
re-serialized by Geyser itself into single-quoted YAML rather than the
double-quoted form `geyser.rs` writes, with no extra keys and no parse
error. This confirms both that Geyser tolerates a partial config (filling
gaps from its own defaults rather than rejecting the file) and that
`yaml_quote`'s escaping survives a real parse by the pinned build, not just
the unit tests below.


Geyser does **not** generate `key.pem` itself. With `auth-type: floodgate` and
no key file present, it logs `Error while reading Floodgate key file`
(`java.nio.file.NoSuchFileException: key.pem`) as an **ERROR**, but this is
non-fatal — Geyser finishes starting anyway ("Done (1.727s)!") and sits up,
degraded, with Floodgate silently broken until an operator supplies a key and
restarts. This matches the design doc's plan (Foton's `key.rs` generates the
key, Geyser only ever reads one) but is a real ordering constraint for
`geyser.rs`: the key must exist **before Geyser's first start**, not just
before the first player tries to join, or the failure is silent from an
operator's point of view.

The key itself: confirmed from `org.geysermc.floodgate.crypto.AesKeyProducer`
in the pinned jar (`KEY_SIZE = 128`, `KeyGenerator.getInstance("AES")` with a
128-bit key, `produceFrom(byte[])` builds a `SecretKeySpec` directly from raw
bytes) — **16 raw bytes, AES-128, no PEM/DER wrapping despite the `.pem`
name.** Generated for this test with `openssl rand -out key.pem 16`.

## Step 4 — driving a simulated Bedrock client at it

`bedrock-protocol` (npm) installed cleanly in WSL. The throwaway client script
matched the brief's, with `error`/`close` handlers added for diagnostics.

**With `auth-type: floodgate` and `validate-bedrock-login: true` (the
default):** the client was rejected by *Geyser itself*, before any Java
connection to Foton was attempted: `Bedrock user with ip: /127.0.0.1 has
disconnected for reason Please log into Xbox to join this server.`
`bedrock-protocol`'s `offline: true` mode fabricates a local, non-Xbox
identity, which Geyser's own Bedrock-side login validation rejects outright.
This gate is upstream of Foton entirely.

**With `validate-bedrock-login: false`:** the client reached Foton and — in
`join-test.sh`'s baseline offline-mode config (`online_mode = false`) — joined
the world **completely successfully**: `PLAY_STATUS login_success`,
`START_GAME`, `PLAY_STATUS player_spawn`. Foton's own log shows `StageZero
joined the game` and later `Player 6a213b11-5dff-3abf-972a-22c2a539c279
removed` on disconnect — an ordinary **offline-mode UUID**, derived the usual
vanilla way from the plain username `StageZero` (the literal
`bedrock-protocol` client `username` option), **not** from any XUID or
Floodgate data. Foton has zero Floodgate-awareness right now, and none was
needed: offline mode does not validate the hostname at all, so the encrypted
Floodgate payload rides along unexamined and the connection succeeds anyway.

This is not "Floodgate working." It is offline mode's total absence of
identity checking, extended transparently across the Geyser hop. See
"Contradictions" below — this is the headline finding.

**With `online_mode = true, encryption = true`** (Foton's config validator
rejects `online_mode = true` with `encryption = false`: "encryption must be
true when online_mode is enabled" — an existing, working validation rule),
the same client was disconnected with:

```
Server requested disconnect: Tried to log in as a Java Edition player! Is Floodgate set up correctly?
```

This is **Geyser's own message**, sent to the Bedrock client — Geyser detected
that Foton was running a normal online-mode login (requesting encryption/Mojang
verification) rather than accepting the Floodgate hostname payload, and told
the player exactly why. **Foton's own server log shows nothing at all for this
connection** — no warning, no error, no kick reason, nothing beyond the
`STAGE0` hostname-capture line added for Step 5. Whatever happens between
Foton's encryption request and the disconnect (Mojang `hasJoined` failing for
a nonexistent account, most likely) is invisible in Foton's log at `Info`/`Warn`
level.

## Step 5 — the captured hostname

A temporary line was added at the point `SClientIntention` is first parsed —
`foton-login/src/tcp_client.rs`, in `handle_handshake`, right after
`SClientIntention::read_packet`. **It used `log::warn!`, not `tracing::warn!`
as the brief specified** — `foton-login`'s `Cargo.toml` depends on `log`, not
`tracing` (only `foton-core` depends on `tracing`); `log::warn!` is what the
rest of `tcp_client.rs` already uses. Reverted before committing; `git diff`
against tracked files is clean.

Two real Floodgate payloads were captured this way (redacted below —
unredacted originals are in the session scratchpad, see "Scratchpad" section).
Every other `STAGE0`-tagged log line during both runs showed a plain,
unmodified hostname (`"127.0.0.1"`) — Geyser's periodic MOTD/player-count
passthrough status pings (`motd.ping-passthrough-interval: 3`), not logins.

Shape (Debug-formatted, `\0` visible):

```
"127.0.0.1\0^Floodgate^>«ciphertext, redacted»"
```

### Wire format, confirmed from the pinned jar's bytecode

Not read from GitHub source, not remembered — disassembled directly out of
`Geyser-Standalone.jar` build 1233 with `javap` (`org.geysermc.floodgate.crypto.
{AesCipher,FloodgateCipher,AesKeyProducer}.class`), i.e. the exact pinned
artifact. This satisfies the "no invented data" rule at least as strictly as
reading the equivalent GitHub source would.

- `FloodgateCipher.IDENTIFIER` = the UTF-8 bytes of the literal string
  `^Floodgate^`.
- `FloodgateCipher.HEADER` = `IDENTIFIER + '>'`, i.e. `^Floodgate^>` — this is
  exactly the marker observed in the captured hostname.
- `AesCipher.encrypt`: generates a random 12-byte IV (`SecureRandom`), does
  `Cipher.getInstance("AES/GCM/NoPadding")` with `GCMParameterSpec(128, iv)`
  (128-bit auth tag, appended to the ciphertext by the JCE — not a separate
  field), then assembles the output as:
  `HEADER + topping.encode(iv) + '!' (0x21) + topping.encode(ciphertext‖tag)`.
- `AesCipher.decrypt` is the mirror: split on the first `!` after the header,
  `topping.decode` each half, `GCMParameterSpec(128, iv)`, `Cipher.doFinal`.
- The "topping" is ordinary `java.util.Base64` (standard alphabet, with `=`
  padding) — confirmed by the observed `+`, `/` and `=` characters in the
  captured ciphertext, and by decrypting successfully with it (below).

So: `hostname = <original address> + '\0' + "^Floodgate^>" + base64(iv[12]) +
"!" + base64(AES/GCM/NoPadding(plaintext, key, iv))` where the GCM tag is the
last 16 bytes of that ciphertext, not a separate field.

**This directly refines `design/bedrock-implementation.md`'s "a signature that
is the whole security of the scheme": there is no discrete signature field in
the plaintext. The AES-GCM authentication tag *is* that signature** — it's
what `Cipher.doFinal` verifies during decrypt, and a tampered ciphertext or
wrong key makes `doFinal` throw rather than silently producing garbage
plaintext. The design doc should say this explicitly rather than imply a
separate field.

### Decryption verified (synthetic data, not committed)

A throwaway Java program (`javax.crypto` directly, no Floodgate classes
needed beyond confirming the algorithm above) decrypted one captured payload
with its matching key. Both are real GeyserMC-produced ciphertext/key pairs —
only the *identity inside* is synthetic, since `bedrock-protocol`'s
`offline: true` mode fabricates it locally rather than using a real Xbox Live
account.

Result: **exactly 12 `\0`-separated fields** — confirming the plan's own
assumption (`design/bedrock-plan.md` Task 2: "asserting the plaintext has 12
`\0`-separated fields") is correct. Positionally, for this synthetic capture:
a client version string (`1.26.40`), the gamertag we set (`StageZero`), what
is presumably an XUID (`0` — no real Xbox account behind this test client, so
this could not be confirmed as a large real XUID), a numeric field (`12`,
plausibly a device/platform id), a language tag (`en_GB` — matches the design
doc's "language" field), and seven more fields including two literal `null`
strings (plausibly the linked-Java-account UUID/name slots, empty because
`bedrock-protocol`'s synthetic identity has no linked account). Exact field
*names* were not verified against source — only positions and that there are
12 of them — so treat this ordering as suggestive, not authoritative; Task 4
should still confirm field semantics against Floodgate's actual `BedrockData`
parsing code before relying on it.

## Java-protocol gaps Foton showed

Independent of Bedrock, worth fixing regardless (per the brief):

1. **No diagnostic logging on a failed online-mode login.** When the
   Mojang-style handshake didn't complete (Step 4's online-mode run), Foton's
   log shows nothing between accepting the connection and the client
   disconnecting — no warn, no error, no kick reason recorded server-side.
   An operator with a real failing Bedrock (or Java) player has no log line to
   go on today; only the client-side message (from Geyser, in this case) says
   anything.
2. **The parsed hostname does not survive past the handshake.**
   `SClientIntention.hostname` is read once in `handle_handshake` and never
   stored — `JavaTcpClient` (`foton-login/src/tcp_client.rs`) has no hostname
   field. `design/bedrock-implementation.md` names
   `foton-login/src/handlers/login.rs` as where the Floodgate branch goes, but
   that file has no access to the hostname string at all right now — it
   branches purely on `self.server.config.online_mode`. Wiring Floodgate in
   will need a place to carry the hostname (or the decrypted identity) from
   handshake time through to login time.
3. **Offline mode performs no validation of the hostname whatsoever.** Not a
   bug exactly — that's what offline mode is — but see "Contradictions"
   below: it means the interesting Bedrock/Floodgate failure mode described
   in the plan is invisible unless the test explicitly uses `online_mode =
   true`.

## Contradictions with `design/bedrock-implementation.md`

Both of these were real contradictions against the document as it stood at
`ca3394e2f` (the commit this task started from). Both have since been folded
into the spec, in `ea0b90365`, which landed after this task's own commit
(`dc4c88e99`) — so the analysis below is left as originally written, as a
record of what was actually observed, with a note on where each one was
addressed. A later reader (or a later task) should not try to "fix" the design
doc against these two again.

- **The design doc's premise — "a server on route A as described is reachable
  and empty" — is only true in online mode.** In offline mode, a Bedrock
  player behind Geyser can already join Foton *today*, fully, with no Foton
  changes at all. Their identity is an ordinary offline-mode UUID derived from
  whatever plain username string happens to reach the Login Start packet —
  not deterministic per-XUID, not collision-safe, not what the design doc's
  "Identity" section promises. This isn't a flaw introduced by Geyser; it's
  offline mode's known behavior extended across the Bedrock hop. But it means
  an operator who turns on `bedrock.enabled` on an offline-mode server (even
  before Floodgate ships) gets *something that looks like it's working* —
  Bedrock players joining — while getting none of the identity guarantees the
  design promises. The design doc should say plainly that Bedrock support is
  meaningless (and actively unsafe, identity-wise) without `online_mode =
  true`, and that this isn't a future state to design toward — it's true
  right now, with zero Bedrock code written. **Applied in `ea0b90365`** — the
  "What '100 % joinable' actually required" section now states both halves
  explicitly (the online-mode refusal and the offline-mode impersonation risk)
  rather than the single "reachable and empty" line.
- **The brief's own Step 1 baseline (offline mode) does not exercise the
  interesting case.** Everything the plan cares about — Foton failing to
  understand Floodgate, needing native decoding — only shows up with
  `online_mode = true`. Task 2 onward should keep this in mind: testing
  against `join-test.sh`'s offline baseline will not catch Floodgate
  regressions, because offline mode doesn't look at the hostname at all. This
  one is a testing-methodology note rather than a claim the design doc makes,
  so there's nothing in the spec to correct — it stays open as guidance for
  whoever writes Task 2 onward's tests.
- **"A signature that is the whole security of the scheme"** (Identity
  section) is better stated as "the AES-GCM authentication tag" — see Step 5.
  There's no separate signature field in the 12-field plaintext, and no
  asymmetric key at all: the shared secret is the entire trust boundary.
  **Applied in `ea0b90365`** — the Identity section now says "an AES-GCM
  authentication tag" and adds a paragraph on why "signature" overstates it
  (no asymmetric signing, no public key, the key file alone is the trust
  boundary).
- Everything else checked out: the plan's crate boundary claims
  (`foton-plugin`/JNI reasoning, `foton-core/src/player/connection/mod.rs`
  untouched), the 12-field plaintext count, and the "Geyser generates nothing,
  the operator/Foton must supply the key" behavior all matched what was
  observed. Re-checked against the current text of
  `design/bedrock-implementation.md` (post-`ea0b90365`) while fixing this
  section: no further contradictions found.

## Scratchpad references (never committed)

- `bedrock-client.js` — the throwaway `bedrock-protocol` test client.
- `bedrock-stage0-captured-hostname.txt` — both captured raw hostnames
  (unredacted), with provenance notes.
- `bedrock-stage0-key.pem` — the 16-byte AES key that encrypted them.
- `raw-hostname.txt`, `DecryptTest.java` — the decrypt-verification tool and
  the exact input it was run against.

All under:
`C:\Users\Zeffu\AppData\Local\Temp\claude\C--Users-Zeffu-Desktop-Projets-Foton\51077e6a-7857-44d7-af02-80b5c6220867\scratchpad\`
