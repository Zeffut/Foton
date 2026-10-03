use foton_macros::{ClientPacket, WriteTo};
use foton_registry::packets::config::C_CUSTOM_PAYLOAD;
use foton_registry::packets::play::C_CUSTOM_PAYLOAD as PLAY_C_CUSTOM_PAYLOAD;
use foton_utils::Identifier;

#[derive(WriteTo, ClientPacket, Clone, Debug)]
#[packet_id(Config = C_CUSTOM_PAYLOAD, Play = PLAY_C_CUSTOM_PAYLOAD)]
pub struct CCustomPayload {
    pub identifier: Identifier,
    // Vanilla's custom payload codec writes the bytes remaining in the packet.
    #[write(as = NoPrefixVec)]
    pub payload: Box<[u8]>,
}

impl CCustomPayload {
    #[must_use]
    pub const fn new(identifier: Identifier, payload: Box<[u8]>) -> Self {
        Self {
            identifier,
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use foton_utils::{Identifier, serial::WriteTo};

    use super::CCustomPayload;

    #[test]
    fn custom_payload_bytes_follow_identifier_without_a_length_prefix() {
        let identifier: Identifier = "fixture:voice".parse().expect("valid fixture identifier");
        for body in [Vec::new(), vec![0x55, 0x66]] {
            let packet = CCustomPayload::new(identifier.clone(), body.clone().into_boxed_slice());
            let mut encoded = Vec::new();
            packet.write(&mut encoded).expect("encode custom payload");

            let mut expected = vec![13];
            expected.extend_from_slice(b"fixture:voice");
            expected.extend_from_slice(&body);
            assert_eq!(encoded, expected);
        }
    }
}
