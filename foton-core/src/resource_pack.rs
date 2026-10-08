//! The resource pack the server asks every client to load.
//!
//! Vanilla parity: `MinecraftServer.ServerResourcePackInfo`, built from the
//! `resource-pack*` and `require-resource-pack` properties by
//! `DedicatedServerProperties.getServerPackInfo`, and sent during configuration
//! by `ServerResourcePackConfigurationTask`.

use foton_protocol::packets::common::CResourcePackPush;
use text_components::TextComponent;
use uuid::Uuid;

use crate::text_json::parse_component;

/// The longest URL the push packet carries.
///
/// Vanilla parity: `ByteBufCodecs.STRING_UTF8`, which is `stringUtf8(32767)`.
const MAX_URL_LENGTH: usize = 32767;

/// A pack to push, as the server is configured to.
#[derive(Debug, Clone)]
pub struct ServerResourcePack {
    /// The pack's identity, which the client's answers name.
    pub id: Uuid,
    /// Where clients download it.
    pub url: String,
    /// Its SHA-1 in hex; empty when the operator gave none.
    pub hash: String,
    /// Whether a client that declines the pack is disconnected.
    pub required: bool,
    /// Text for the client's confirmation screen.
    pub prompt: Option<TextComponent>,
}

impl ServerResourcePack {
    /// Builds the pack from its settings, or `None` when no URL is set.
    ///
    /// Vanilla logs and carries on with a malformed hash, a missing id or a
    /// prompt that is not a component. Here an id or prompt that cannot be read
    /// stops startup: the operator asked for a pack, and a server that quietly
    /// runs without a required one lets players in who should not be.
    ///
    /// # Errors
    ///
    /// Returns a message naming the setting that cannot be used.
    pub fn from_settings(
        url: &str,
        id: &str,
        sha1: &str,
        prompt: &str,
        required: bool,
    ) -> Result<Option<Self>, String> {
        if url.is_empty() {
            if required {
                log::warn!("resource_pack.required is set but resource_pack.url is empty");
            }
            return Ok(None);
        }
        if url.len() > MAX_URL_LENGTH {
            return Err(format!(
                "resource_pack.url is too long (max {MAX_URL_LENGTH}, was {})",
                url.len()
            ));
        }
        if sha1.len() > CResourcePackPush::MAX_HASH_LENGTH {
            return Err(format!(
                "resource_pack.sha1 is too long (max {}, was {})",
                CResourcePackPush::MAX_HASH_LENGTH,
                sha1.len()
            ));
        }
        if sha1.is_empty() {
            log::warn!(
                "You specified a resource pack without providing a sha1 hash. Pack will be updated on the client only if you change the name of the pack."
            );
        } else if !is_sha1(sha1) {
            log::warn!("Invalid sha1 for resource_pack.sha1");
        }

        let id = if id.is_empty() {
            let id = name_uuid_from_bytes(url.as_bytes());
            log::warn!("resource_pack.id missing, using default of {id}");
            id
        } else {
            Uuid::parse_str(id).map_err(|_| format!("resource_pack.id '{id}' is not a UUID"))?
        };

        let prompt = if prompt.is_empty() {
            None
        } else {
            Some(parse_component(prompt).ok_or_else(|| {
                format!("resource_pack.prompt '{prompt}' is not a JSON text component")
            })?)
        };

        Ok(Some(Self {
            id,
            url: url.to_owned(),
            hash: sha1.to_owned(),
            required,
            prompt,
        }))
    }

    /// The packet that asks a client to load this pack.
    #[must_use]
    pub fn push_packet(&self) -> CResourcePackPush {
        CResourcePackPush {
            id: self.id,
            url: self.url.clone(),
            hash: self.hash.clone(),
            required: self.required,
            prompt: self.prompt.clone(),
        }
    }
}

/// Vanilla's `^[a-fA-F0-9]{40}$`.
fn is_sha1(hash: &str) -> bool {
    hash.len() == CResourcePackPush::MAX_HASH_LENGTH && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Java's `UUID.nameUUIDFromBytes`: the version 3 UUID of the bytes' MD5, with
/// no namespace in front.
///
/// Vanilla derives a pack's default id from its URL this way, and so does
/// Paper's `Player.setResourcePack`, so the same URL names the same pack
/// whichever side chose the id.
#[must_use]
pub fn name_uuid_from_bytes(bytes: &[u8]) -> Uuid {
    let mut digest = md5::compute(bytes).0;
    digest[6] = (digest[6] & 0x0f) | 0x30;
    digest[8] = (digest[8] & 0x3f) | 0x80;
    Uuid::from_bytes(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `UUID.nameUUIDFromBytes("hello".getBytes())` in Java.
    #[test]
    fn the_default_id_matches_javas_name_uuid() {
        assert_eq!(
            name_uuid_from_bytes(b"hello").to_string(),
            "5d41402a-bc4b-3a76-b971-9d911017c592"
        );
    }

    #[test]
    fn no_url_means_no_pack_whatever_else_is_set() {
        let pack = ServerResourcePack::from_settings("", "", "", "", true);
        assert!(matches!(pack, Ok(None)));
    }

    #[test]
    fn a_pack_with_every_setting_keeps_each_of_them() {
        let pack = ServerResourcePack::from_settings(
            "https://example.test/pack.zip",
            "0b0e7f7e-7d3c-4a64-8f0c-6a4f1d0f9d11",
            "9273409d4693245d8f1108a6ee86904565c12189",
            r##"{"text":"Hello","color":"#12ab34"}"##,
            true,
        )
        .expect("valid settings")
        .expect("a url was given");
        assert_eq!(pack.id.to_string(), "0b0e7f7e-7d3c-4a64-8f0c-6a4f1d0f9d11");
        assert!(pack.required);
        assert!(pack.prompt.is_some());
        assert_eq!(pack.push_packet().hash, pack.hash);
    }

    #[test]
    fn settings_that_cannot_be_read_stop_startup() {
        for (id, sha1, prompt) in [
            ("not-a-uuid", "", ""),
            ("", &"a".repeat(41)[..], ""),
            ("", "", "{not json"),
        ] {
            assert!(
                ServerResourcePack::from_settings(
                    "https://example.test/p.zip",
                    id,
                    sha1,
                    prompt,
                    false
                )
                .is_err(),
                "({id:?}, {sha1:?}, {prompt:?}) should be refused"
            );
        }
    }
}
