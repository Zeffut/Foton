use foton_macros::{ClientPacket, WriteTo};
use foton_registry::packets::config::C_RESOURCE_PACK_PUSH;
use foton_registry::packets::play::C_RESOURCE_PACK_PUSH as PLAY_C_RESOURCE_PACK_PUSH;
use text_components::TextComponent;
use uuid::Uuid;

/// Asks the client to download and apply a resource pack.
///
/// Vanilla parity: `ClientboundResourcePackPushPacket`. The id names the pack
/// for the rest of the exchange: the client's responses and a later pop both
/// refer to it, and a second push with the same id replaces the first.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Config = C_RESOURCE_PACK_PUSH, Play = PLAY_C_RESOURCE_PACK_PUSH)]
pub struct CResourcePackPush {
    /// The pack's identity, as the client reports it back.
    pub id: Uuid,
    /// Where the client downloads the pack from.
    #[write(as = Prefixed(VarInt), bound = 32767)]
    pub url: String,
    /// The pack's SHA-1 in hex, or empty when the server does not vouch for one.
    #[write(as = Prefixed(VarInt), bound = 40)]
    pub hash: String,
    /// Whether the client must accept the pack to stay connected.
    pub required: bool,
    /// Text shown on the client's confirmation screen.
    pub prompt: Option<TextComponent>,
}

impl CResourcePackPush {
    /// The longest hash the packet carries.
    ///
    /// Vanilla parity: `ClientboundResourcePackPushPacket.MAX_HASH_LENGTH`.
    pub const MAX_HASH_LENGTH: usize = 40;
}

#[cfg(test)]
mod tests {
    use foton_utils::serial::WriteTo;

    use super::*;

    fn packet(hash: &str) -> CResourcePackPush {
        CResourcePackPush {
            id: Uuid::from_u128(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10),
            url: "http://a/p.zip".to_owned(),
            hash: hash.to_owned(),
            required: true,
            prompt: None,
        }
    }

    /// The fields go out in vanilla's order, and an absent prompt is one false byte.
    #[test]
    fn fields_are_written_in_codec_order() {
        let mut bytes = Vec::new();
        packet("ab")
            .write(&mut bytes)
            .expect("writing to a vec cannot fail");

        let mut expected = (1..=16).collect::<Vec<u8>>();
        expected.push(14);
        expected.extend_from_slice(b"http://a/p.zip");
        expected.extend_from_slice(&[2, b'a', b'b', 1, 0]);
        assert_eq!(bytes, expected);
    }

    /// Vanilla's constructor refuses a longer hash; the client would drop the packet.
    #[test]
    fn a_hash_over_forty_characters_is_not_written() {
        let mut bytes = Vec::new();
        let result = packet(&"a".repeat(CResourcePackPush::MAX_HASH_LENGTH + 1)).write(&mut bytes);
        assert!(result.is_err());
    }
}
