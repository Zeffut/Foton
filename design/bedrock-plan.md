# Bedrock Compatibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Bedrock Edition player on a Switch, phone or console joins a Foton server on UDP 19132, with no Java account, and plays with a stable identity that every UUID-keyed system in the server already understands.

**Architecture:** Foton supervises GeyserMC as a child process (fetched, configured, spawned, log-relayed and shut down by Foton) and decodes Floodgate's encrypted identity handshake natively in Rust. Geyser reaches Foton as an ordinary Java client, so no part of `foton-core`'s networking changes.

**Tech Stack:** Rust (nightly-2026-07-23, edition 2024), `aes-gcm` 0.11.1, `base64` 0.23, `tokio` process supervision, Geyser Standalone 2.11.2 build 1233, Node `bedrock-protocol` for the integration test.

**Spec:** `design/bedrock-implementation.md` (and `design/bedrock-clients.md` for the route analysis it argues from)

## Global Constraints

Every task's requirements implicitly include this section.

- **Build only through WSL.** Windows blocks build scripts (Smart App Control). The validated command, 2 min 14 s cold:
  `wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && /root/.cargo/bin/cargo check --workspace'`
  Never inline `$HOME` or an unquoted `$PATH` into the `bash -lc` string — the Windows PATH interpolates and the command dies with a syntax error.
- **No `.unwrap()` or `.expect()` in production code.** Tests may use them. `Result` for anything recoverable.
- **`missing_docs` is warn-level workspace-wide.** Every public item gets a doc comment. Clippy runs at `pedantic`.
- **No invented data.** Every Floodgate constant in this plan was read from GeyserMC source, cited per task. If an implementation detail is not in this plan, read the cited file — do not guess.
- **Branch:** `feat/bedrock`. Never commit to `master`. Conventional commits.
- **Do not touch `foton-core/src/player/connection/mod.rs`.** It belongs to the concurrent plugin-API work. Nothing here needs it.
- **Guard clauses over deep nesting.** Focused files. Concise comments.
- **Geyser pin:** version `2.11.2`, build `1233`, artifact `Geyser-Standalone.jar`, SHA-256 `f1a4c6a5cad7ee4820b03c27cd3805680e8c06bd66ce7244f96335d83b652e0e`, URL `https://download.geysermc.org/v2/projects/geyser/versions/2.11.2/builds/1233/downloads/standalone`. Verified against the GeyserMC download API on 2026-09-01.

## The Floodgate wire format, as read from source

Cited once here so no task has to re-derive it. Sources:
- `GeyserMC/Geyser@master:common/src/main/java/org/geysermc/floodgate/crypto/FloodgateCipher.java`
- `GeyserMC/Geyser@master:common/src/main/java/org/geysermc/floodgate/crypto/AesCipher.java`
- `GeyserMC/Geyser@master:common/src/main/java/org/geysermc/floodgate/crypto/AesKeyProducer.java`
- `GeyserMC/Geyser@master:common/src/main/java/org/geysermc/floodgate/util/BedrockData.java`
- `GeyserMC/Floodgate@HEAD:core/src/main/java/org/geysermc/floodgate/player/FloodgateHandshakeHandler.java`
- `GeyserMC/Floodgate@HEAD:core/src/main/java/org/geysermc/floodgate/addon/data/HandshakeDataImpl.java`
- `GeyserMC/Floodgate@HEAD:core/src/main/java/org/geysermc/floodgate/util/Utils.java`

**Where it lives.** The Java handshake's `hostname` is split on `\0`. The segment whose first 11 bytes are `^Floodgate^` is the Floodgate payload; the remaining segments rejoined with `\0` are the real hostname. (`FloodgateHandshakeHandler.separateHostname`)

**The envelope.**

```
"^Floodgate^"          11 bytes, IDENTIFIER
(0x3E + VERSION)        1 byte; VERSION = 0, so '>' (0x3E). HEADER is these 12 bytes.
base64(iv)              standard base64 with padding, of a 12-byte IV
0x21                    '!', the splitter. Base64's alphabet never contains it.
base64(ciphertext)      standard base64 with padding
```

`version(data)` is `data[11] - 0x3E`. `checkHeader` verifies only the 11-byte identifier and that the payload is longer than the 12-byte header. (`FloodgateCipher`, `AesCipher.encrypt`/`decrypt`)

**The cipher.** `AES/GCM/NoPadding`, 12-byte IV, 128-bit tag, key size 128 bits. The key file holds the raw 16 key bytes (`produceFrom` is `new SecretKeySpec(keyFileData, "AES")`). (`AesCipher`, `AesKeyProducer`)

**The plaintext.** Exactly 12 fields joined by `\0`; any other count is rejected (`EXPECTED_LENGTH = 12`). In order:

| # | Field | Type |
|---|---|---|
| 0 | version | string |
| 1 | username | string (the Bedrock gamertag) |
| 2 | xuid | string holding a signed 64-bit integer |
| 3 | deviceOs | int |
| 4 | languageCode | string |
| 5 | uiProfile | int |
| 6 | inputMode | int |
| 7 | ip | string |
| 8 | linkedPlayer | string, `"null"` when absent |
| 9 | fromProxy | `"1"` is true |
| 10 | subscribeId | int |
| 11 | verifyCode | string |

(`BedrockData.fromString` / `toString`)

**The identity.** `Utils.getJavaUuid(xuid)` is `new UUID(0, Long.parseLong(xuid))` — high 64 bits zero, low 64 bits the XUID. The Java username is `prefix + username.substring(0, min(username.length(), 16 - prefix.length()))`. (`Utils`, `HandshakeDataImpl`)

## File Structure

| File | Responsibility |
|---|---|
| `foton-bedrock/Cargo.toml` | new crate manifest |
| `foton-bedrock/src/lib.rs` | crate docs, module wiring, re-exports |
| `foton-bedrock/src/floodgate.rs` | envelope parsing, AES-GCM decryption, `BedrockData` parsing, identity derivation. Pure: no I/O, no clock, no process |
| `foton-bedrock/src/key.rs` | generate the shared secret on first run, load it after |
| `foton-bedrock/src/geyser.rs` | pinned build constants, runtime discovery, jar fetch, config generation, spawn, log relay, restart, shutdown |
| `foton/src/config/server.rs` | `BedrockConfig` and its validation |
| `foton/src/lib.rs` | start and cancel the supervisor beside the Rcon listener |
| `foton-login/src/handlers/login.rs` | the one branch: a verified Floodgate identity produces the profile |
| `package-content/config.toml`, `config.schema.json` | the `[server.bedrock]` section |
| `dev/bedrock-test.sh` | simulated Bedrock client → Geyser → Foton, end to end |
| `design/bedrock-stage0-findings.md` | what Task 1 learned |

---

### Task 1: Stage 0 — prove the chain by hand

The spec gates everything on this. Geyser has never actually driven Foton. If it cannot, Tasks 2-8 are built on a broken assumption, and the finding is worth more than the code. Output is a written record, not production code.

**Files:**
- Create: `design/bedrock-stage0-findings.md`

**Interfaces:**
- Consumes: nothing.
- Produces: `design/bedrock-stage0-findings.md`, and — if the chain works — a captured real Floodgate hostname string plus the key that decrypts it, saved to the session scratchpad for Task 2's test vector. Never commit the captured key.

- [ ] **Step 1: Start an offline-mode Foton**

Reuse the harness `dev/join-test.sh` already builds. Run:

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && bash dev/join-test.sh'
```

Expected: it passes today. This confirms the baseline before Geyser is added. If it fails, stop and report — that is a pre-existing break, not Bedrock's.

- [ ] **Step 2: Fetch the pinned Geyser build and verify it**

```bash
wsl -d Ubuntu -- bash -lc 'mkdir -p /root/geyser && cd /root/geyser && \
  curl -sSL -o Geyser-Standalone.jar "https://download.geysermc.org/v2/projects/geyser/versions/2.11.2/builds/1233/downloads/standalone" && \
  echo "f1a4c6a5cad7ee4820b03c27cd3805680e8c06bd66ce7244f96335d83b652e0e  Geyser-Standalone.jar" | sha256sum -c -'
```

Expected: `Geyser-Standalone.jar: OK`. A mismatch means the pin is wrong — stop and report rather than proceeding with an unverified jar.

- [ ] **Step 3: Run Geyser once to have it write its config, then point it at Foton**

```bash
wsl -d Ubuntu -- bash -lc 'cd /root/geyser && export JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64 && timeout 40 $JAVA_HOME/bin/java -jar Geyser-Standalone.jar --help 2>&1 | tail -20; ls -la'
```

Then edit `config.yml` so `remote.address: 127.0.0.1`, `remote.port: 25566` (the port `join-test.sh` uses), `remote.auth-type: floodgate`, and `bedrock.port: 19132`. Record the exact keys you had to set — Task 6 generates this file and needs the real key names, not remembered ones.

- [ ] **Step 4: Start Foton, then Geyser, and drive a simulated Bedrock client at it**

```bash
wsl -d Ubuntu -- bash -lc 'cd /root && npm install bedrock-protocol 2>&1 | tail -3'
```

Write a throwaway client in the **session scratchpad, never in the repo and never on the Desktop**:

```javascript
const bedrock = require('bedrock-protocol')
const client = bedrock.createClient({
  host: '127.0.0.1', port: 19132, username: 'StageZero', offline: true
})
client.on('start_game', (p) => { console.log('START_GAME', p.runtime_entity_id); })
client.on('play_status', (p) => { console.log('PLAY_STATUS', p.status) })
client.on('disconnect', (p) => { console.log('DISCONNECT', JSON.stringify(p)) })
setTimeout(() => { console.log('TIMEOUT'); process.exit(1) }, 60000)
```

Expected, in order: Geyser accepts the Bedrock connection, opens a Java connection to Foton, Foton logs a handshake it does not understand (Floodgate is not implemented yet — this is the expected failure), and the client is disconnected.

- [ ] **Step 5: Capture the Floodgate hostname**

Add a temporary `tracing::warn!` in `foton-login` where `SClientIntention.hostname` is first available, logging the raw hostname with `{:?}` so the `\0` separators are visible. Rebuild, rerun, and record the exact string. Also copy Geyser's generated `key.pem`.

Save both to the session scratchpad. **The captured string and key are the only real test vector this project can have**, and Task 2 depends on them. Revert the temporary log before committing.

- [ ] **Step 6: Write the findings**

Create `design/bedrock-stage0-findings.md` recording, factually: whether Geyser connected; the exact Geyser config keys that mattered; the raw hostname shape (with the ciphertext redacted); every Java-protocol gap Foton showed (each one is worth fixing regardless of Bedrock); and whether anything contradicts `design/bedrock-implementation.md`. If something does, say so plainly — the spec is wrong and gets corrected, rather than the finding being trimmed to fit.

- [ ] **Step 7: Commit**

```bash
git add design/bedrock-stage0-findings.md
git commit -m "design: what happened when Geyser first met Foton"
```

---

### Task 2: The Floodgate envelope, decrypted

**Files:**
- Create: `foton-bedrock/Cargo.toml`, `foton-bedrock/src/lib.rs`, `foton-bedrock/src/floodgate.rs`
- Modify: `Cargo.toml` (workspace `members`, add `aes-gcm`)

**Interfaces:**
- Consumes: Task 1's captured hostname and key, if the chain worked.
- Produces:
  - `foton_bedrock::floodgate::HEADER: [u8; 12]`, `IDENTIFIER: &[u8]`, `IV_LENGTH: usize`
  - `foton_bedrock::floodgate::extract_payload(hostname: &str) -> Option<(&str, String)>` — the Floodgate segment and the hostname with it removed
  - `foton_bedrock::floodgate::decrypt(payload: &[u8], key: &[u8; 16]) -> Result<String, FloodgateError>`
  - `foton_bedrock::floodgate::FloodgateError` (variants `NotFloodgateData`, `Malformed`, `Decrypt`)

- [ ] **Step 1: Create the crate manifest**

`foton-bedrock/Cargo.toml`:

```toml
[package]
name = "foton-bedrock"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
# Floodgate's identity payload is AES-128-GCM with a base64 topping, so the
# cipher and the encoding are the whole dependency list of the pure half.
aes-gcm = "0.11.1"
base64.workspace = true
thiserror.workspace = true
uuid.workspace = true

[lints]
workspace = true
```

Add `"foton-bedrock"` to `members` in the root `Cargo.toml`, and `aes-gcm = "0.11.1"` under `# Cryptography` in `[workspace.dependencies]`.

- [ ] **Step 2: Write the failing tests**

`foton-bedrock/src/floodgate.rs`, at the bottom:

```rust
#[cfg(test)]
mod tests {
    use aes_gcm::aead::{Aead, KeyInit, Payload};
    use aes_gcm::{Aes128Gcm, Nonce};
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;

    use super::{FloodgateError, HEADER, extract_payload};

    /// Builds a payload the way `AesCipher.encrypt` does, so the decoder is
    /// tested against the format rather than against itself.
    fn encrypt(plaintext: &str, key: &[u8; 16], iv: &[u8; 12]) -> String {
        let cipher = Aes128Gcm::new(key.into());
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(iv), Payload { msg: plaintext.as_bytes(), aad: &[] })
            .expect("test vector encrypts");
        let mut out = String::from_utf8(HEADER.to_vec()).expect("header is ascii");
        out.push_str(&STANDARD.encode(iv));
        out.push('!');
        out.push_str(&STANDARD.encode(ciphertext));
        out
    }

    #[test]
    fn round_trips_a_payload() {
        let key = [7u8; 16];
        let iv = [3u8; 12];
        let payload = encrypt("hello\0world", &key, &iv);
        let decrypted = super::decrypt(payload.as_bytes(), &key).expect("decrypts");
        assert_eq!(decrypted, "hello\0world");
    }

    #[test]
    fn rejects_a_tampered_ciphertext() {
        let key = [7u8; 16];
        let iv = [3u8; 12];
        let payload = encrypt("hello", &key, &iv);
        // Flip the last base64 character. GCM's tag must catch it.
        let mut bytes = payload.into_bytes();
        let last = bytes.len() - 2;
        bytes[last] = if bytes[last] == b'A' { b'B' } else { b'A' };
        assert!(matches!(
            super::decrypt(&bytes, &key),
            Err(FloodgateError::Decrypt)
        ));
    }

    #[test]
    fn rejects_the_wrong_key() {
        let payload = encrypt("hello", &[7u8; 16], &[3u8; 12]);
        assert!(matches!(
            super::decrypt(payload.as_bytes(), &[8u8; 16]),
            Err(FloodgateError::Decrypt)
        ));
    }

    #[test]
    fn rejects_data_that_is_not_floodgate() {
        assert!(matches!(
            super::decrypt(b"just a hostname", &[7u8; 16]),
            Err(FloodgateError::NotFloodgateData)
        ));
    }

    #[test]
    fn rejects_a_header_with_nothing_after_it() {
        assert!(matches!(
            super::decrypt(&HEADER, &[7u8; 16]),
            Err(FloodgateError::NotFloodgateData)
        ));
    }

    #[test]
    fn finds_the_payload_among_hostname_segments() {
        let payload = encrypt("x", &[7u8; 16], &[3u8; 12]);
        let hostname = format!("mc.example.com\0{payload}\0127.0.0.1");
        let (found, rest) = extract_payload(&hostname).expect("payload is found");
        assert_eq!(found, payload);
        assert_eq!(rest, "mc.example.com\0127.0.0.1");
    }

    #[test]
    fn finds_nothing_in_an_ordinary_hostname() {
        assert!(extract_payload("mc.example.com\0127.0.0.1\0uuid").is_none());
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && /root/.cargo/bin/cargo test -p foton-bedrock 2>&1 | tail -20'
```

Expected: compilation failure — `decrypt`, `extract_payload`, `HEADER` and `FloodgateError` do not exist.

- [ ] **Step 4: Implement the envelope**

Top of `foton-bedrock/src/floodgate.rs`:

```rust
//! Floodgate's identity payload, decoded.
//!
//! Geyser puts the Bedrock player's identity in the Java handshake's hostname,
//! encrypted with a key both sides hold. This module is the reading half, and
//! it is deliberately pure: bytes and a key in, a verified identity or an error
//! out. No I/O, no clock, no process — so it is testable without a JVM.
//!
//! The format is read from GeyserMC's own source, not recalled:
//! `common/src/main/java/org/geysermc/floodgate/crypto/{FloodgateCipher,AesCipher}.java`.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes128Gcm, Nonce};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use thiserror::Error;

/// The 11 bytes that mark a hostname segment as Floodgate's.
pub const IDENTIFIER: &[u8] = b"^Floodgate^";

/// The format version this decoder understands.
pub const VERSION: u8 = 0;

/// `IDENTIFIER` followed by the encoded version byte: the first 12 bytes of
/// every payload.
pub const HEADER: [u8; 12] = [
    b'^', b'F', b'l', b'o', b'o', b'd', b'g', b'a', b't', b'e', b'^',
    0x3E + VERSION,
];

/// The IV length AES-GCM is used with here.
pub const IV_LENGTH: usize = 12;

/// Separates the base64 IV from the base64 ciphertext. Base64's alphabet never
/// contains it, which is what makes a byte scan for it safe.
const SPLITTER: u8 = b'!';

/// Why a Floodgate payload was not accepted.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FloodgateError {
    /// The data does not carry Floodgate's header at all.
    #[error("not Floodgate data")]
    NotFloodgateData,
    /// The header is there but the envelope is not shaped correctly.
    #[error("malformed Floodgate payload: {0}")]
    Malformed(&'static str),
    /// The ciphertext did not authenticate: wrong key, or tampered with.
    #[error("Floodgate payload failed to decrypt")]
    Decrypt,
}

/// Finds the Floodgate segment in a handshake hostname.
///
/// Returns the payload and the hostname with that segment removed, matching
/// `FloodgateHandshakeHandler.separateHostname`. Returns `None` for an ordinary
/// Java client's hostname.
#[must_use]
pub fn extract_payload(hostname: &str) -> Option<(&str, String)> {
    let mut payload = None;
    let mut rest: Vec<&str> = Vec::new();

    for segment in hostname.split('\0') {
        if payload.is_none() && segment.as_bytes().starts_with(IDENTIFIER) {
            payload = Some(segment);
            continue;
        }
        rest.push(segment);
    }

    payload.map(|found| (found, rest.join("\0")))
}

/// Decrypts a Floodgate payload with the shared key.
///
/// The returned string is `BedrockData`'s null-separated form; parsing it is
/// [`BedrockData::parse`](crate::floodgate::BedrockData::parse)'s job.
pub fn decrypt(payload: &[u8], key: &[u8; 16]) -> Result<String, FloodgateError> {
    if payload.len() <= HEADER.len() || !payload.starts_with(IDENTIFIER) {
        return Err(FloodgateError::NotFloodgateData);
    }

    let body = &payload[HEADER.len()..];
    let split = body
        .iter()
        .position(|byte| *byte == SPLITTER)
        .ok_or(FloodgateError::Malformed("no splitter"))?;

    let iv = STANDARD
        .decode(&body[..split])
        .map_err(|_| FloodgateError::Malformed("iv is not base64"))?;
    if iv.len() != IV_LENGTH {
        return Err(FloodgateError::Malformed("iv is the wrong length"));
    }

    let ciphertext = STANDARD
        .decode(&body[split + 1..])
        .map_err(|_| FloodgateError::Malformed("ciphertext is not base64"))?;

    let cipher = Aes128Gcm::new(key.into());
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&iv), Payload { msg: &ciphertext, aad: &[] })
        .map_err(|_| FloodgateError::Decrypt)?;

    String::from_utf8(plaintext).map_err(|_| FloodgateError::Malformed("plaintext is not UTF-8"))
}
```

`foton-bedrock/src/lib.rs`:

```rust
//! Bedrock Edition players, joining a Java server.
//!
//! Two halves that share only a key. [`floodgate`] decodes the identity Geyser
//! puts in the handshake and is pure. `geyser` supervises the process that put
//! it there. `foton-login` depends on the first and knows nothing of the second.

pub mod floodgate;
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && /root/.cargo/bin/cargo test -p foton-bedrock 2>&1 | tail -20'
```

Expected: 7 passed.

- [ ] **Step 6: If Task 1 captured a real payload, add it as a test**

Only if Stage 0 produced one. Add a test decrypting the captured hostname with the captured key and asserting the plaintext has 12 `\0`-separated fields. Redact the gamertag and XUID to fixed values in the committed test if they are the operator's own — but keep the ciphertext, because that is the part being tested.

If Stage 0 produced no payload, skip this step and say so in the commit body. Do not fabricate one.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml foton-bedrock/
git commit -m "feat(bedrock): decode the identity Geyser hides in the handshake"
```

---

### Task 3: `BedrockData` and the identity it yields

**Files:**
- Modify: `foton-bedrock/src/floodgate.rs`

**Interfaces:**
- Consumes: `decrypt` from Task 2.
- Produces:
  - `foton_bedrock::floodgate::BedrockData` with public fields `version`, `username`, `xuid`, `device_os`, `language_code`, `ui_profile`, `input_mode`, `ip`, `linked_player`, `from_proxy`, `subscribe_id`, `verify_code` (all `String` except `device_os: i32`, `ui_profile: i32`, `input_mode: i32`, `from_proxy: bool`, `subscribe_id: i32`)
  - `BedrockData::parse(plaintext: &str) -> Result<Self, FloodgateError>`
  - `BedrockData::java_uuid(&self) -> Result<uuid::Uuid, FloodgateError>`
  - `BedrockData::java_username(&self, prefix: &str) -> String`
  - `FloodgateError::WrongFieldCount(usize)` and `FloodgateError::BadField(&'static str)`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `foton-bedrock/src/floodgate.rs`:

```rust
    use super::BedrockData;

    /// The 12 fields in `BedrockData.toString()` order.
    fn sample(username: &str, xuid: &str) -> String {
        format!("2\0{username}\0{xuid}\07\0fr_FR\01\02\0127.0.0.1\0null\00\0123\0abcdef")
    }

    #[test]
    fn parses_every_field_in_order() {
        let data = BedrockData::parse(&sample("Steve", "2535428478404012")).expect("parses");
        assert_eq!(data.version, "2");
        assert_eq!(data.username, "Steve");
        assert_eq!(data.xuid, "2535428478404012");
        assert_eq!(data.device_os, 7);
        assert_eq!(data.language_code, "fr_FR");
        assert_eq!(data.ui_profile, 1);
        assert_eq!(data.input_mode, 2);
        assert_eq!(data.ip, "127.0.0.1");
        assert_eq!(data.linked_player, "null");
        assert!(!data.from_proxy);
        assert_eq!(data.subscribe_id, 123);
        assert_eq!(data.verify_code, "abcdef");
    }

    #[test]
    fn rejects_a_field_count_that_is_not_twelve() {
        assert!(matches!(
            BedrockData::parse("2\0Steve\0123"),
            Err(FloodgateError::WrongFieldCount(3))
        ));
    }

    #[test]
    fn derives_the_uuid_from_the_xuid() {
        let data = BedrockData::parse(&sample("Steve", "2535428478404012")).expect("parses");
        let uuid = data.java_uuid().expect("xuid is numeric");
        // High 64 bits zero, low 64 bits the XUID: `new UUID(0, xuid)`.
        assert_eq!(uuid.as_u64_pair(), (0, 2_535_428_478_404_012));
    }

    #[test]
    fn rejects_a_xuid_that_is_not_a_number() {
        let data = BedrockData::parse(&sample("Steve", "not-a-number")).expect("parses");
        assert!(matches!(data.java_uuid(), Err(FloodgateError::BadField("xuid"))));
    }

    #[test]
    fn prefixes_the_username() {
        let data = BedrockData::parse(&sample("Steve", "1")).expect("parses");
        assert_eq!(data.java_username("."), ".Steve");
    }

    #[test]
    fn truncates_so_prefix_and_name_fit_sixteen_characters() {
        let data = BedrockData::parse(&sample("AVeryLongGamertag", "1")).expect("parses");
        let name = data.java_username(".");
        assert_eq!(name, ".AVeryLongGamer");
        assert_eq!(name.chars().count(), 16);
    }

    #[test]
    fn truncates_on_a_character_boundary() {
        // A gamertag with multi-byte characters must not panic or split a char.
        let data = BedrockData::parse(&sample("Ünïcödeplayername", "1")).expect("parses");
        let name = data.java_username(".");
        assert!(name.starts_with(".Ünïcöde"));
        assert!(name.chars().count() <= 16);
    }

    #[test]
    fn reads_the_proxy_flag() {
        let proxied = sample("Steve", "1").replace("\0null\00\0", "\0null\01\0");
        assert!(BedrockData::parse(&proxied).expect("parses").from_proxy);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && /root/.cargo/bin/cargo test -p foton-bedrock 2>&1 | tail -20'
```

Expected: compilation failure — `BedrockData` does not exist.

- [ ] **Step 3: Add the error variants**

In `FloodgateError`:

```rust
    /// The plaintext did not hold the twelve fields Floodgate sends.
    #[error("expected 12 Floodgate fields, got {0}")]
    WrongFieldCount(usize),
    /// A field was present but could not be read as its type.
    #[error("Floodgate field {0} could not be read")]
    BadField(&'static str),
```

- [ ] **Step 4: Implement `BedrockData`**

Append to `foton-bedrock/src/floodgate.rs`:

```rust
/// The number of `\0`-separated fields Floodgate sends. `BedrockData.EXPECTED_LENGTH`.
const EXPECTED_FIELDS: usize = 12;

/// The longest name the Java protocol accepts.
const MAX_JAVA_USERNAME: usize = 16;

/// What Geyser says about the Bedrock player.
///
/// Field order is `BedrockData.toString()`'s, which is the order the fields are
/// declared in Geyser's own class. Types stay as close to the wire as is useful:
/// `xuid` is a string there and stays one here, because it is an identifier
/// before it is a number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BedrockData {
    /// The Bedrock client's version string.
    pub version: String,
    /// The Xbox Live gamertag.
    pub username: String,
    /// The Xbox user id, as a signed 64-bit integer in a string.
    pub xuid: String,
    /// Which platform the player is on.
    pub device_os: i32,
    /// The player's language, as `fr_FR`.
    pub language_code: String,
    /// Which UI the client presents.
    pub ui_profile: i32,
    /// Touch, controller, keyboard.
    pub input_mode: i32,
    /// The address Geyser saw the player come from.
    pub ip: String,
    /// A linked Java account, or `"null"`.
    pub linked_player: String,
    /// Whether the data passed through a proxy.
    pub from_proxy: bool,
    /// Skin upload correlation id.
    pub subscribe_id: i32,
    /// Skin upload verification code.
    pub verify_code: String,
}

impl BedrockData {
    /// Parses the decrypted payload.
    ///
    /// A field count other than twelve is refused rather than padded: Geyser
    /// sends exactly twelve, so anything else is a version mismatch or a forgery,
    /// and guessing which would be worse than saying no.
    pub fn parse(plaintext: &str) -> Result<Self, FloodgateError> {
        let fields: Vec<&str> = plaintext.split('\0').collect();
        if fields.len() != EXPECTED_FIELDS {
            return Err(FloodgateError::WrongFieldCount(fields.len()));
        }

        let number = |index: usize, name: &'static str| -> Result<i32, FloodgateError> {
            fields[index]
                .parse::<i32>()
                .map_err(|_| FloodgateError::BadField(name))
        };

        Ok(Self {
            version: fields[0].to_owned(),
            username: fields[1].to_owned(),
            xuid: fields[2].to_owned(),
            device_os: number(3, "deviceOs")?,
            language_code: fields[4].to_owned(),
            ui_profile: number(5, "uiProfile")?,
            input_mode: number(6, "inputMode")?,
            ip: fields[7].to_owned(),
            linked_player: fields[8].to_owned(),
            from_proxy: fields[9] == "1",
            subscribe_id: number(10, "subscribeId")?,
            verify_code: fields[11].to_owned(),
        })
    }

    /// The UUID this player gets on a Java server.
    ///
    /// `new UUID(0, xuid)`: zero in the high bits, the XUID in the low ones. It
    /// is derived rather than stored, so it is the same on every join, which is
    /// what makes player data, permissions and bans work unchanged.
    pub fn java_uuid(&self) -> Result<uuid::Uuid, FloodgateError> {
        let xuid: i64 = self
            .xuid
            .parse()
            .map_err(|_| FloodgateError::BadField("xuid"))?;
        Ok(uuid::Uuid::from_u64_pair(0, xuid as u64))
    }

    /// The name this player is known by, prefixed so it cannot collide with a
    /// Java player's and truncated to what the Java protocol accepts.
    ///
    /// Truncation is on a character boundary, not a byte one: a gamertag can
    /// hold characters a Java name cannot, and slicing one in half would panic.
    #[must_use]
    pub fn java_username(&self, prefix: &str) -> String {
        let room = MAX_JAVA_USERNAME.saturating_sub(prefix.chars().count());
        let truncated: String = self.username.chars().take(room).collect();
        format!("{prefix}{truncated}")
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && /root/.cargo/bin/cargo test -p foton-bedrock 2>&1 | tail -20'
```

Expected: 15 passed.

- [ ] **Step 6: Commit**

```bash
git add foton-bedrock/src/floodgate.rs
git commit -m "feat(bedrock): the identity a Bedrock player gets, derived not stored"
```

---

### Task 4: The shared key, and the `[server.bedrock]` config

**Files:**
- Create: `foton-bedrock/src/key.rs`
- Modify: `foton-bedrock/src/lib.rs`, `foton-bedrock/Cargo.toml`, `foton/src/config/server.rs`, `package-content/config.toml`, `package-content/config.schema.json`, `CONFIGURATION.md` (generated)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `foton_bedrock::key::load_or_create(path: &std::path::Path) -> std::io::Result<[u8; 16]>`
  - `foton::config::server::BedrockConfig` with fields `enable: bool`, `port: u16`, `motd: String`, `username_prefix: String`, `trusted_proxies: Vec<String>`, `java_home: String`, `jar_path: String`

- [ ] **Step 1: Write the failing test**

`foton-bedrock/src/key.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::load_or_create;

    #[test]
    fn creates_a_key_once_and_reuses_it() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("key.pem");

        let first = load_or_create(&path).expect("creates");
        assert!(path.is_file());
        let second = load_or_create(&path).expect("loads");

        assert_eq!(first, second, "the key must survive a restart");
        assert_ne!(first, [0u8; 16], "a key of zeroes is not a key");
    }

    #[test]
    fn refuses_a_key_file_of_the_wrong_size() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("key.pem");
        std::fs::write(&path, b"too short").expect("writes");

        assert!(load_or_create(&path).is_err());
    }
}
```

Add to `foton-bedrock/Cargo.toml`:

```toml
[dependencies]
rand.workspace = true

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && /root/.cargo/bin/cargo test -p foton-bedrock key 2>&1 | tail -20'
```

Expected: `key` module not found.

- [ ] **Step 3: Implement the key**

Top of `foton-bedrock/src/key.rs`:

```rust
//! The secret Foton and its Geyser share.
//!
//! Sixteen raw bytes, which is what Geyser's `AesKeyProducer.produceFrom` reads
//! a key file as. Not PEM despite the conventional name — Geyser reads the file
//! whole and hands it to `SecretKeySpec`.
//!
//! Everything about a Bedrock player's identity rests on this file staying
//! secret: it is the only thing standing between a forged handshake and total
//! impersonation.

use std::fs;
use std::io::{Error, ErrorKind, Result};
use std::path::Path;

use rand::TryRngCore as _;
use rand::rngs::OsRng;

/// The key size Geyser generates and expects: `AesKeyProducer.KEY_SIZE` is 128 bits.
pub const KEY_LENGTH: usize = 16;

/// Loads the shared key, generating it on first run.
///
/// The generated key comes from the operating system's randomness, never from a
/// seeded generator.
pub fn load_or_create(path: &Path) -> Result<[u8; KEY_LENGTH]> {
    if path.is_file() {
        let bytes = fs::read(path)?;
        return bytes.try_into().map_err(|_| {
            Error::new(
                ErrorKind::InvalidData,
                format!("{} is not a {KEY_LENGTH}-byte Floodgate key", path.display()),
            )
        });
    }

    let mut key = [0u8; KEY_LENGTH];
    OsRng
        .try_fill_bytes(&mut key)
        .map_err(|error| Error::other(format!("no system randomness: {error}")))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, key)?;
    restrict_permissions(path)?;
    Ok(key)
}

/// Makes the key readable only by the account running the server, where the
/// platform has a way to say that.
#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

/// Windows inherits the directory's ACL, which is the operator's to set.
#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<()> {
    Ok(())
}
```

Add `pub mod key;` to `foton-bedrock/src/lib.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && /root/.cargo/bin/cargo test -p foton-bedrock 2>&1 | tail -20'
```

Expected: 17 passed. If `rand::TryRngCore` does not resolve, check the workspace's `rand` version (`0.10.2`) and use its current OS-randomness entry point rather than changing the version.

- [ ] **Step 5: Add the config section**

In `foton/src/config/server.rs`, next to `RconConfig`:

```rust
/// Letting Bedrock Edition players in, through a Geyser this server runs.
///
/// Off by default: enabling it starts a Java process, and nothing should pay
/// for a feature it did not ask for.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct BedrockConfig {
    /// Whether Geyser is started and the Bedrock port opened at all.
    pub enable: bool,
    /// The UDP port Bedrock clients connect to.
    pub port: u16,
    /// What Bedrock clients see in the server list; empty reuses the server MOTD.
    pub motd: String,
    /// Prepended to a Bedrock player's gamertag so it cannot collide with a
    /// Java player's name. Empty means no prefix, and collisions become possible.
    pub username_prefix: String,
    /// Addresses a Floodgate handshake is accepted from.
    ///
    /// Foton starts Geyser locally, so loopback is the whole list by default. A
    /// handshake claiming Floodgate from anywhere else is refused, because the
    /// alternative is that anyone who can reach the Java port becomes anyone.
    pub trusted_proxies: Vec<String>,
    /// A JDK or JRE for Geyser; empty means `JAVA_HOME`, then `java` on the path.
    pub java_home: String,
    /// An operator-supplied Geyser jar; empty means fetch the pinned build.
    pub jar_path: String,
}

impl Default for BedrockConfig {
    fn default() -> Self {
        Self {
            enable: false,
            // Bedrock's default port.
            port: 19132,
            motd: String::new(),
            username_prefix: ".".to_owned(),
            trusted_proxies: vec!["127.0.0.1".to_owned(), "::1".to_owned()],
            java_home: String::new(),
            jar_path: String::new(),
        }
    }
}
```

Add to `ServerConfig`, beside `rcon`:

```rust
    /// Letting Bedrock Edition players in.
    #[serde(default)]
    pub bedrock: BedrockConfig,
```

In `validate`, after the Rcon rules:

```rust
    if config.bedrock.enable {
        if config.bedrock.port == config.server_port {
            return Err("bedrock.port must differ from server_port");
        }
        if config.bedrock.trusted_proxies.is_empty() {
            return Err("bedrock.trusted_proxies must list at least one address");
        }
        if config.bedrock.username_prefix.chars().count() >= 16 {
            return Err("bedrock.username_prefix leaves no room for a username");
        }
    }
```

- [ ] **Step 6: Write the failing config tests**

In `foton/src/config/tests.rs`, following the file's existing style for building a config from the packaged default and asserting on `validate`:

```rust
#[test]
fn validate_rejects_bedrock_sharing_the_java_port() {
    let config = with_lines(&[
        "[server.bedrock]",
        "enable = true",
        "port = 25565",
    ]);
    assert_eq!(
        validate(&config),
        Err("bedrock.port must differ from server_port")
    );
}

#[test]
fn validate_rejects_bedrock_with_no_trusted_proxies() {
    let config = with_lines(&[
        "[server.bedrock]",
        "enable = true",
        "trusted_proxies = []",
    ]);
    assert_eq!(
        validate(&config),
        Err("bedrock.trusted_proxies must list at least one address")
    );
}

#[test]
fn bedrock_is_off_when_the_section_is_absent() {
    let config = packaged_default();
    assert!(!config.bedrock.enable);
}
```

Match the helper names the file already uses — read `foton/src/config/tests.rs` first and reuse its construction helper rather than inventing `with_lines` if it is called something else.

- [ ] **Step 7: Add the packaged config and schema**

In `package-content/config.toml`, after the `[server.rcon]` block:

```toml
# Letting Bedrock Edition players in, through a Geyser this server runs.
[server.bedrock]
# Whether Geyser is started and the Bedrock port opened at all.
enable = false
# UDP port Bedrock clients connect to.
port = 19132
# What Bedrock clients see in the server list; empty reuses the server MOTD.
motd = ""
# Prepended to a Bedrock player's gamertag so it cannot collide with a Java
# player's name. Empty means no prefix, and collisions become possible.
username_prefix = "."
# Addresses a Floodgate identity handshake is accepted from. Foton starts
# Geyser locally, so loopback is the whole list. Widening this means trusting
# whoever can reach the Java port to say who they are.
trusted_proxies = ["127.0.0.1", "::1"]
# A JDK or JRE for Geyser; empty means JAVA_HOME, then java on the path.
java_home = ""
# An operator-supplied Geyser jar; empty means fetch the pinned build.
jar_path = ""
```

Mirror it in `package-content/config.schema.json`, following the shape the `rcon` object already uses there.

- [ ] **Step 8: Regenerate the documentation and run the checks**

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && python3 dev/gen-config-docs.py && export CARGO_TARGET_DIR=/root/foton-target && /root/.cargo/bin/cargo test -p foton -p foton-bedrock 2>&1 | tail -20'
```

Expected: `CONFIGURATION.md` gains a `[server.bedrock]` section, tests pass. `dev/ci.sh` fails on a stale `CONFIGURATION.md`, so this step is not optional.

- [ ] **Step 9: Commit**

```bash
git add foton-bedrock/ foton/src/config/ package-content/ CONFIGURATION.md
git commit -m "feat(bedrock): the shared key, and the config that turns Bedrock on"
```

---

### Task 5: Accepting a Floodgate login

**Files:**
- Modify: `foton-login/src/handlers/login.rs`, `foton-login/Cargo.toml`, and whichever module in `foton-login` owns the handshake — read `foton-login/src/connection.rs` and `foton-login/src/lib.rs` first to find where `SClientIntention.hostname` and the peer address are both available.

**Interfaces:**
- Consumes: `foton_bedrock::floodgate::{extract_payload, decrypt, BedrockData}`, `foton_bedrock::key::load_or_create`, `foton::config::server::BedrockConfig`.
- Produces: a `GameProfile` built from Bedrock identity, reaching the same `complete_login` path an authenticated Java player reaches.

- [ ] **Step 1: Read before writing**

Read `foton-login/src/handlers/login.rs` in full and `foton-login/src/pre_play_state.rs`'s tests. The existing offline path builds `GameProfile { id: offline_uuid(&requested_username), .. }`; the Floodgate path is a sibling of it, not a replacement. Note exactly how the handler reaches the peer's `SocketAddr` — the trusted-proxy check needs it, and if it is not reachable there, thread it in rather than skipping the check.

- [ ] **Step 2: Write the failing tests**

In `foton-login/src/pre_play_state.rs`'s test module, or a new `#[cfg(test)] mod floodgate_tests` beside the login handler:

```rust
    #[test]
    fn a_floodgate_handshake_from_loopback_produces_the_bedrock_profile() {
        let key = [7u8; 16];
        let hostname = floodgate_hostname(&key, "Steve", "2535428478404012");
        let config = bedrock_config(&["127.0.0.1"]);

        let profile = resolve_floodgate(&hostname, "127.0.0.1:52000".parse().unwrap(), &key, &config)
            .expect("a valid handshake from a trusted address is accepted");

        assert_eq!(profile.id.as_u64_pair(), (0, 2_535_428_478_404_012));
        assert_eq!(profile.name, ".Steve");
    }

    #[test]
    fn a_floodgate_handshake_from_an_untrusted_address_is_refused() {
        let key = [7u8; 16];
        let hostname = floodgate_hostname(&key, "Steve", "2535428478404012");
        let config = bedrock_config(&["127.0.0.1"]);

        let result = resolve_floodgate(&hostname, "203.0.113.9:52000".parse().unwrap(), &key, &config);

        // Refused for being untrusted, and never quietly downgraded to an
        // ordinary offline login: that would turn a failed forgery into a
        // successful one.
        assert!(matches!(result, Err(FloodgateLoginError::UntrustedAddress(_))));
    }

    #[test]
    fn a_forged_handshake_is_refused() {
        let hostname = floodgate_hostname(&[9u8; 16], "Mallory", "1");
        let config = bedrock_config(&["127.0.0.1"]);

        let result = resolve_floodgate(&hostname, "127.0.0.1:52000".parse().unwrap(), &[7u8; 16], &config);

        assert!(matches!(result, Err(FloodgateLoginError::Rejected(_))));
    }

    #[test]
    fn an_ordinary_hostname_is_not_a_floodgate_login() {
        let config = bedrock_config(&["127.0.0.1"]);
        let result = resolve_floodgate("mc.example.com", "127.0.0.1:52000".parse().unwrap(), &[7u8; 16], &config);
        assert!(matches!(result, Ok(None) | Err(FloodgateLoginError::NotFloodgate)));
    }
```

Write `floodgate_hostname` and `bedrock_config` as test helpers in the same module, reusing the encryption helper from Task 2 (move it into `foton-bedrock` behind `#[cfg(any(test, feature = "test-support"))]` and export it, rather than copying it — a second copy would drift).

- [ ] **Step 3: Run the tests to verify they fail**

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && /root/.cargo/bin/cargo test -p foton-login floodgate 2>&1 | tail -20'
```

Expected: `resolve_floodgate` does not exist.

- [ ] **Step 4: Implement the branch**

Add `foton-bedrock = { path = "../foton-bedrock" }` to `foton-login/Cargo.toml`. Then:

```rust
/// Why a handshake claiming to carry Bedrock identity was not honored.
#[derive(Debug, thiserror::Error)]
pub enum FloodgateLoginError {
    /// The hostname carries no Floodgate payload; this is an ordinary login.
    #[error("not a Floodgate handshake")]
    NotFloodgate,
    /// A Floodgate payload arrived from an address not in `trusted_proxies`.
    #[error("Floodgate handshake from untrusted address {0}")]
    UntrustedAddress(std::net::IpAddr),
    /// The payload did not decrypt or did not parse.
    #[error("Floodgate handshake rejected: {0}")]
    Rejected(foton_bedrock::floodgate::FloodgateError),
}

/// Turns a handshake hostname into a Bedrock player's profile, if it carries one.
///
/// Returns `Ok(None)` when the hostname is an ordinary Java client's, so the
/// caller falls through to the normal path. Every other non-success is an
/// error: a Floodgate payload that fails any check is refused outright and never
/// downgraded to an offline login, because a downgrade turns a failed forgery
/// into a successful one.
fn resolve_floodgate(
    hostname: &str,
    peer: std::net::SocketAddr,
    key: &[u8; 16],
    config: &BedrockConfig,
) -> Result<Option<GameProfile>, FloodgateLoginError> {
    if !config.enable {
        return Ok(None);
    }

    let Some((payload, _rest)) = foton_bedrock::floodgate::extract_payload(hostname) else {
        return Ok(None);
    };

    if !is_trusted(peer.ip(), &config.trusted_proxies) {
        return Err(FloodgateLoginError::UntrustedAddress(peer.ip()));
    }

    let plaintext = foton_bedrock::floodgate::decrypt(payload.as_bytes(), key)
        .map_err(FloodgateLoginError::Rejected)?;
    let data = foton_bedrock::floodgate::BedrockData::parse(&plaintext)
        .map_err(FloodgateLoginError::Rejected)?;
    let id = data.java_uuid().map_err(FloodgateLoginError::Rejected)?;

    Ok(Some(GameProfile {
        id,
        name: data.java_username(&config.username_prefix),
        ..GameProfile::default()
    }))
}

/// Whether an address is one Foton accepts Bedrock identity from.
fn is_trusted(address: std::net::IpAddr, trusted: &[String]) -> bool {
    trusted
        .iter()
        .filter_map(|entry| entry.parse::<std::net::IpAddr>().ok())
        .any(|entry| entry == address)
}
```

Call it in the login handler *before* the online-mode branch, and refuse the connection with a neutral message plus a `tracing::warn!` on any error other than `NotFloodgate`. Build `GameProfile` with whatever fields the struct actually requires — read it rather than assuming `..Default::default()` compiles.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && /root/.cargo/bin/cargo test -p foton-login 2>&1 | tail -25'
```

Expected: the four new tests pass and every pre-existing `foton-login` test still passes. A broken existing test means the branch was inserted in the wrong place.

- [ ] **Step 6: Commit**

```bash
git add foton-login/ foton-bedrock/
git commit -m "feat(bedrock): let a verified Bedrock identity through login"
```

---

### Task 6: The Geyser supervisor

**Files:**
- Create: `foton-bedrock/src/geyser.rs`
- Modify: `foton-bedrock/src/lib.rs`, `foton-bedrock/Cargo.toml`

**Interfaces:**
- Consumes: `foton_bedrock::key::load_or_create`.
- Produces:
  - `foton_bedrock::geyser::{GEYSER_VERSION, GEYSER_BUILD, GEYSER_SHA256, GEYSER_URL}`
  - `foton_bedrock::geyser::GeyserOptions { run_directory, bedrock_port, java_port, motd, username_prefix, java_home, jar_path }`
  - `foton_bedrock::geyser::render_config(options: &GeyserOptions) -> String`
  - `foton_bedrock::geyser::Supervisor::start(options, cancel: CancellationToken) -> Result<Supervisor, GeyserError>`

- [ ] **Step 1: Write the failing tests**

The supervisor's I/O is not unit-testable without a JVM; its decisions are. Test those:

```rust
#[cfg(test)]
mod tests {
    use super::{GeyserOptions, render_config, verify_checksum};

    fn options() -> GeyserOptions {
        GeyserOptions {
            run_directory: std::path::PathBuf::from("/tmp/run"),
            bedrock_port: 19132,
            java_port: 25565,
            motd: "A Foton server".to_owned(),
            username_prefix: ".".to_owned(),
            java_home: None,
            jar_path: None,
        }
    }

    #[test]
    fn the_generated_config_points_geyser_at_this_server() {
        let yaml = render_config(&options());
        assert!(yaml.contains("port: 19132"));
        assert!(yaml.contains("address: 127.0.0.1"));
        assert!(yaml.contains("port: 25565"));
        assert!(yaml.contains("auth-type: floodgate"));
    }

    #[test]
    fn the_generated_config_quotes_a_motd_that_would_break_yaml() {
        let mut options = options();
        options.motd = "Foton: now with #tags & \"quotes\"".to_owned();
        let yaml = render_config(&options);
        // A MOTD is operator input reaching a config parser. It must not be
        // able to add keys.
        assert!(!yaml.contains("\nnow with"));
        assert!(yaml.contains(r#""Foton: now with #tags & \"quotes\"""#));
    }

    #[test]
    fn a_checksum_mismatch_is_refused() {
        assert!(verify_checksum(b"not the jar", super::GEYSER_SHA256).is_err());
    }

    #[test]
    fn a_matching_checksum_is_accepted() {
        // sha256("") — proves the comparison works, without a 40 MB fixture.
        let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(verify_checksum(b"", empty).is_ok());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && /root/.cargo/bin/cargo test -p foton-bedrock geyser 2>&1 | tail -20'
```

Expected: `geyser` module not found.

- [ ] **Step 3: Implement the pinned constants and config generation**

Add to `foton-bedrock/Cargo.toml`: `tokio.workspace = true`, `tracing.workspace = true`, `reqwest.workspace = true`, `sha2.workspace = true`, `tokio-util.workspace = true`.

```rust
//! The Geyser that turns Bedrock into Java, run as a child of this server.
//!
//! Not in the JVM `foton-plugin` starts: a process gets one `JNI_CreateJavaVM`,
//! and sharing it would make Bedrock support depend on the plugin host's
//! lifecycle. A child process costs one process and buys isolation — Geyser
//! crashing restarts Geyser, and an operator who runs no Bedrock runs no JVM.

/// The pinned Geyser release. Bumped by a person who checked that the build
/// still speaks the protocol `Cargo.toml` targets — never followed as `latest`,
/// because that turns a protocol bump into a silent outage.
pub const GEYSER_VERSION: &str = "2.11.2";
/// The pinned build number within [`GEYSER_VERSION`].
pub const GEYSER_BUILD: u32 = 1233;
/// SHA-256 of `Geyser-Standalone.jar` for the pinned build.
pub const GEYSER_SHA256: &str =
    "f1a4c6a5cad7ee4820b03c27cd3805680e8c06bd66ce7244f96335d83b652e0e";
/// Where the pinned jar is fetched from.
pub const GEYSER_URL: &str = "https://download.geysermc.org/v2/projects/geyser/versions/2.11.2/builds/1233/downloads/standalone";
```

`render_config` writes the YAML whole, every start. Use the exact key names Task 1 recorded from a real Geyser config — not the ones in this plan, if they differ. Quote every operator-supplied string with a real YAML double-quote escape (`"` → `\"`, `\` → `\\`), because a MOTD is untrusted input reaching a parser.

`verify_checksum(bytes: &[u8], expected: &str) -> Result<(), GeyserError>` hashes with `sha2::Sha256` and compares case-insensitively.

- [ ] **Step 4: Implement the supervisor**

`Supervisor::start` does, in order, each failure carrying what to do about it:

1. Resolve the Java runtime: `options.java_home`, else `JAVA_HOME`, else `java` on the path. Run `java -version`, parse the major version, and fail with a sentence if it is below 21.
2. Resolve the jar: `options.jar_path` if set, else `run_directory/bedrock/Geyser-Standalone.jar`, downloading from `GEYSER_URL` when absent. Verify the checksum in both cases. A download failure names the URL.
3. Write `run_directory/bedrock/config.yml` from `render_config`, and ensure `key.pem` exists via `key::load_or_create`.
4. Spawn `java -jar Geyser-Standalone.jar` with `current_dir` set to the bedrock directory, `stdout` and `stderr` piped.
5. Spawn a task per stream that reads lines and re-emits them through `tracing` with `target: "geyser"`, mapping Geyser's own level markers onto `tracing` levels and defaulting to `info`.
6. Spawn the supervision task: on unexpected exit, restart with backoff (1s, doubling, capped at 60s); after five consecutive failures, log an error and stay down rather than spinning. On `cancel`, send the platform's polite termination, wait up to 10 seconds, then kill.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && /root/.cargo/bin/cargo test -p foton-bedrock 2>&1 | tail -20'
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add foton-bedrock/
git commit -m "feat(bedrock): run and supervise Geyser as a child of the server"
```

---

### Task 7: Wire it into the server, and guard the pin

**Files:**
- Modify: `foton/src/lib.rs`, `foton/Cargo.toml`, `dev/doctor.sh`

**Interfaces:**
- Consumes: `foton_bedrock::geyser::Supervisor`, `BedrockConfig`.
- Produces: a running Bedrock listener when `[server.bedrock] enable = true`.

- [ ] **Step 1: Read the listener startup**

Read `foton/src/lib.rs` around lines 130-150, where `TcpListener::bind` and `RconListener::bind` happen. The supervisor starts in the same place, with the same `CancellationToken` and `TaskTracker`, and the same "log the failure and carry on" or "refuse to start" policy Rcon uses — match whichever it is rather than choosing a new one.

- [ ] **Step 2: Start the supervisor**

Add `foton-bedrock = { path = "../foton-bedrock" }` to `foton/Cargo.toml`, and beside the Rcon block:

```rust
        if config.bedrock.enable {
            let options = foton_bedrock::geyser::GeyserOptions {
                run_directory: run_directory.clone(),
                bedrock_port: config.bedrock.port,
                java_port: server_port,
                motd: if config.bedrock.motd.is_empty() {
                    config.motd.clone()
                } else {
                    config.bedrock.motd.clone()
                },
                username_prefix: config.bedrock.username_prefix.clone(),
                java_home: (!config.bedrock.java_home.is_empty())
                    .then(|| PathBuf::from(&config.bedrock.java_home)),
                jar_path: (!config.bedrock.jar_path.is_empty())
                    .then(|| PathBuf::from(&config.bedrock.jar_path)),
            };
            match foton_bedrock::geyser::Supervisor::start(options, cancel_token.clone()) {
                Ok(supervisor) => task_tracker.spawn(supervisor.run()),
                Err(error) => {
                    error!("Bedrock support failed to start: {error}");
                }
            };
        }
```

Adjust names to what the surrounding code actually calls things.

- [ ] **Step 3: Guard the pin in doctor.sh**

`dev/doctor.sh` already checks the Minecraft sources match `Cargo.toml`'s target. Add a check in the same style: read `GEYSER_VERSION` and `GEYSER_BUILD` out of `foton-bedrock/src/geyser.rs`, query
`https://download.geysermc.org/v2/projects/geyser/versions/<version>/builds/<build>`, and report if the build no longer exists. Network-dependent, so it warns rather than fails when offline, matching how the script handles other network checks — read it and follow.

- [ ] **Step 4: Verify the whole workspace still builds**

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && /root/.cargo/bin/cargo check --workspace --all-targets 2>&1 | tail -20 && /root/.cargo/bin/cargo clippy -p foton-bedrock -p foton-login --all-targets 2>&1 | tail -20'
```

Expected: no errors, no clippy warnings. `missing_docs` and `pedantic` are on — fix rather than allow, and if an allow is genuinely right use `#[expect(..., reason = "...")]`.

- [ ] **Step 5: Verify the server still starts with Bedrock off**

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && bash dev/smoke-test.sh 2>&1 | tail -10'
```

Expected: passes exactly as before. The default config disables Bedrock, so nothing changed for anyone who did not ask for it.

- [ ] **Step 6: Commit**

```bash
git add foton/ dev/doctor.sh
git commit -m "feat(bedrock): open the Bedrock port when the operator asks"
```

---

### Task 8: The end-to-end test, and saying what this is

**Files:**
- Create: `dev/bedrock-test.sh`, `dev/bedrock-client.js`
- Modify: `dev/all-tests.sh`, `README.md`

**Interfaces:**
- Consumes: everything above.
- Produces: a test that fails when a Bedrock player can no longer join.

- [ ] **Step 1: Write the simulated client**

`dev/bedrock-client.js`, following the shape Task 1 proved:

```javascript
// A Bedrock client, simulated, so the join path is tested without a console.
// Exits 0 only when the player actually reaches the world.
const bedrock = require('bedrock-protocol')

const port = Number(process.argv[2] || 19132)
const username = process.argv[3] || 'StageZero'

const client = bedrock.createClient({ host: '127.0.0.1', port, username, offline: true })
let started = false

client.on('start_game', () => { started = true })
client.on('play_status', (packet) => {
  if (packet.status === 'player_spawn' && started) {
    console.log('JOINED')
    process.exit(0)
  }
})
client.on('disconnect', (packet) => {
  console.error('DISCONNECT', JSON.stringify(packet))
  process.exit(1)
})
client.on('error', (error) => { console.error('ERROR', error.message); process.exit(1) })
setTimeout(() => { console.error('TIMEOUT'); process.exit(1) }, 120000)
```

- [ ] **Step 2: Write the test script**

`dev/bedrock-test.sh`, following `dev/join-test.sh`'s structure exactly — same offline run directory pattern, same config rewriting with `sed`, same `nohup … < /dev/null` discipline for backgrounded servers. It must:

1. Skip with exit 0 and a clear message if `node` or a Java 21+ runtime is missing, so it does not fail a machine that legitimately cannot run it.
2. Build, generate a config, then `sed` in `online_mode = false`, a dedicated `server_port`, and a `[server.bedrock]` block with `enable = true` and a dedicated `port`.
3. Start Foton and wait for the Geyser lines in Foton's own log — proving the log relay works, not just the process.
4. Run `dev/bedrock-client.js` and require `JOINED`.
5. Assert on `server.log` that the joining player's name carries the prefix, and capture the UUID.
6. Run the client a second time with the same username and assert the same UUID appears. This is the persistence claim, and it is the one most likely to silently break.
7. Kill both processes on exit via `trap`, including on failure.

- [ ] **Step 3: Run it**

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && bash dev/bedrock-test.sh 2>&1 | tail -30'
```

Expected: `JOINED`, matching UUIDs across both runs, exit 0. A failure here is the real answer to whether this works — investigate it rather than relaxing the assertion.

- [ ] **Step 4: Add it to the suite**

Add `bedrock-test.sh` to `dev/all-tests.sh` in the order that file uses.

- [ ] **Step 5: Say what this is, in the README**

Add a short section stating: Bedrock players can join; the port; that Foton runs Geyser for them; that they need no Java account; and — beside the claim, not below it — that the translation is Geyser's, so Bedrock-side behaviour is as good as Geyser makes it and some things do not translate. `PARITY.md` keeps its caveat beside its numbers; this claim gets the same treatment.

- [ ] **Step 6: Run the full CI**

```bash
wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Users/Zeffu/Desktop/Projets/Foton && export CARGO_TARGET_DIR=/root/foton-target && bash dev/ci.sh 2>&1 | tail -30'
```

Expected: green. `typos`, `fmt`, `clippy`, the config-docs freshness check and the test suite all have to pass before this branch merges.

- [ ] **Step 7: Commit**

```bash
git add dev/ README.md
git commit -m "test(bedrock): a Bedrock client joins, and keeps its identity"
```

---

## Self-Review

**Spec coverage.** Every section of `design/bedrock-implementation.md` maps to a task: the supervisor (6, 7), Floodgate natively (2, 3, 5), the security of it (5's untrusted-address and forgery tests), identity (3), configuration (4), where it attaches (5, 7 — and `connection/mod.rs` is untouched throughout), testing (8), staging (tasks are in stage order), the pin and its ageing (6, 7). Stage 4 of the spec — fixing the Java-protocol gaps Geyser exposes — is deliberately not a task here: it cannot be sized before Task 1 produces the list, and it gets its own plan.

**Type consistency.** `BedrockData` field names are identical in Tasks 3 and 5. `java_uuid()` and `java_username(prefix)` are called with the same signatures in both. `FloodgateError` variants introduced in Task 2 (`NotFloodgateData`, `Malformed`, `Decrypt`) and extended in Task 3 (`WrongFieldCount`, `BadField`) are the same enum. `GeyserOptions`' fields in Task 6 match the struct literal in Task 7.

**Known soft spots, flagged rather than hidden.** Task 5's insertion point and Task 7's variable names depend on code the implementer must read first — each says so explicitly instead of guessing. Task 6's Geyser config keys come from Task 1's observation, not from this plan. `rand`'s OS-randomness API (Task 4) is version-sensitive and the step says to check rather than assume.
