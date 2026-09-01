//! Floodgate's identity payload, decoded.
//!
//! Geyser puts the Bedrock player's identity in the Java handshake's hostname,
//! encrypted with a key both sides hold. This module is the reading half, and
//! it is deliberately pure: bytes and a key in, a verified identity or an error
//! out. No I/O, no clock, no process — so it is testable without a JVM.
//!
//! The format is read from `GeyserMC`'s own source, not recalled:
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
    b'^',
    b'F',
    b'l',
    b'o',
    b'o',
    b'd',
    b'g',
    b'a',
    b't',
    b'e',
    b'^',
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
/// The returned string is `BedrockData`'s null-separated form; parsing it into
/// individual fields is a later task's job, not this module's.
pub fn decrypt(payload: &[u8], key: &[u8; 16]) -> Result<String, FloodgateError> {
    if payload.len() <= HEADER.len() || !payload.starts_with(IDENTIFIER) {
        return Err(FloodgateError::NotFloodgateData);
    }

    let body = &payload[HEADER.len()..];
    let split = body
        .iter()
        .position(|byte| *byte == SPLITTER)
        .ok_or(FloodgateError::Malformed("no splitter"))?;

    let iv_bytes = STANDARD
        .decode(&body[..split])
        .map_err(|_| FloodgateError::Malformed("iv is not base64"))?;
    let iv = Nonce::try_from(iv_bytes.as_slice())
        .map_err(|_| FloodgateError::Malformed("iv is the wrong length"))?;

    let ciphertext = STANDARD
        .decode(&body[split + 1..])
        .map_err(|_| FloodgateError::Malformed("ciphertext is not base64"))?;

    let cipher = Aes128Gcm::new(key.into());
    let plaintext = cipher
        .decrypt(
            &iv,
            Payload {
                msg: &ciphertext,
                aad: &[],
            },
        )
        .map_err(|_| FloodgateError::Decrypt)?;

    String::from_utf8(plaintext).map_err(|_| FloodgateError::Malformed("plaintext is not UTF-8"))
}

/// Encrypts a plaintext into a Floodgate wire envelope, the mirror of
/// [`decrypt`].
///
/// This is ordinary public API, not a test helper: a module that implements a
/// wire format owes both directions, and outbound Floodgate-shaped payloads
/// are needed again once Foton drives Geyser itself (see the crate's `geyser`
/// half). `GeyserMC`'s own encoder (`AesCipher.encrypt`) draws its IV from
/// `SecureRandom` internally; taking `iv` as a parameter here instead keeps
/// this function pure and its output reproducible.
///
/// # Errors
///
/// Returns [`FloodgateError::Malformed`] if AES-GCM rejects the plaintext —
/// only possible near its multi-gigabyte single-nonce limit, never true for a
/// handshake hostname, but the cipher's own API is fallible and this crate
/// does not unwrap production code around it.
pub fn encrypt(
    plaintext: &str,
    key: &[u8; 16],
    iv: &[u8; IV_LENGTH],
) -> Result<String, FloodgateError> {
    let cipher = Aes128Gcm::new(key.into());
    let ciphertext = cipher
        .encrypt(
            &Nonce::from(*iv),
            Payload {
                msg: plaintext.as_bytes(),
                aad: &[],
            },
        )
        .map_err(|_| FloodgateError::Malformed("plaintext too large to encrypt"))?;

    // Built from the "^Floodgate^" literal directly (rather than reusing
    // `HEADER`/`IDENTIFIER`'s bytes) so this stays a total function: every
    // piece appended is already a `&str` or comes from `Engine::encode`,
    // which returns `String`, so there is no fallible UTF-8 conversion to
    // handle without unwrapping it.
    let mut out = String::from("^Floodgate^");
    out.push(char::from(0x3E + VERSION));
    out.push_str(&STANDARD.encode(iv));
    out.push(char::from(SPLITTER));
    out.push_str(&STANDARD.encode(ciphertext));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{FloodgateError, HEADER, extract_payload};

    #[test]
    fn round_trips_a_payload() {
        let key = [7u8; 16];
        let iv = [3u8; 12];
        let payload = super::encrypt("hello\0world", &key, &iv).expect("test vector encrypts");
        let decrypted = super::decrypt(payload.as_bytes(), &key).expect("decrypts");
        assert_eq!(decrypted, "hello\0world");
    }

    #[test]
    fn rejects_a_tampered_ciphertext() {
        let key = [7u8; 16];
        let iv = [3u8; 12];
        let payload = super::encrypt("hello", &key, &iv).expect("test vector encrypts");
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
        let payload =
            super::encrypt("hello", &[7u8; 16], &[3u8; 12]).expect("test vector encrypts");
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
        let payload = super::encrypt("x", &[7u8; 16], &[3u8; 12]).expect("test vector encrypts");
        let hostname = format!("mc.example.com\0{payload}\x00127.0.0.1");
        let (found, rest) = extract_payload(&hostname).expect("payload is found");
        assert_eq!(found, payload);
        assert_eq!(rest, "mc.example.com\x00127.0.0.1");
    }

    #[test]
    fn finds_nothing_in_an_ordinary_hostname() {
        assert!(extract_payload("mc.example.com\x00127.0.0.1\0uuid").is_none());
    }

    /// A real Floodgate payload, captured during Task 1's Stage 0 hand-driven
    /// chain: a genuine Geyser 2.11.2 build 1233 instance encrypting a real
    /// `bedrock-protocol` client's handshake with a locally generated
    /// key.pem, exactly as it arrived at `foton-login`'s handshake handler.
    ///
    /// The identity fields inside are synthetic — `bedrock-protocol`'s
    /// `offline: true` mode fabricates a gamertag and XUID locally rather
    /// than using a real Xbox Live account (`username: "StageZero"`,
    /// `xuid: "0"`) — so nothing here is the operator's real identity and
    /// nothing needed redacting. The ciphertext and key are genuine `GeyserMC`
    /// output, which is the part this test exercises: decoding the real wire
    /// format, not a payload this crate encrypted itself.
    #[test]
    fn decrypts_a_real_captured_payload() {
        let key: [u8; 16] = [
            0x93, 0x40, 0xD4, 0x4D, 0x09, 0x83, 0xFF, 0x7E, 0x61, 0x75, 0xBF, 0xFD, 0x3F, 0x73,
            0xFE, 0x69,
        ];
        let hostname = "127.0.0.1\0^Floodgate^>U5gAzTaERI/KEpDd!i5wOGIMqjq+XjHXC7LsvjQSLCTFnlD8bH6lJ3rRzPLJgKIAaPN9e8DHImHB5qxG7uSnPNy5T7XtvADrGR+uVXzurrcOKAGXMmw==";

        let (payload, rest) = extract_payload(hostname).expect("payload is found");
        assert_eq!(rest, "127.0.0.1");

        let plaintext = super::decrypt(payload.as_bytes(), &key).expect("real capture decrypts");
        assert_eq!(
            plaintext.split('\0').count(),
            12,
            "BedrockData.EXPECTED_LENGTH is 12 fields"
        );
    }
}
